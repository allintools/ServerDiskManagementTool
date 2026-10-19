use crate::errors::{AppError, AppResult};
use crate::ssh::{self, ExecOptions, Target};
use crate::state::AppState;
use crate::types::{LargeFile, ReclaimCandidate, UsageNode, UsageScan};
use crate::util::short_text;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

pub const USAGE_SCRIPT: &str = include_str!("usage_v1.sh");
pub const LARGE_FILES_SCRIPT: &str = include_str!("large_files_v1.sh");
pub const RECOMMEND_SCRIPT: &str = include_str!("recommend_v1.sh");

#[derive(Debug, Clone)]
struct DuEntry {
    path: String,
    size: u64,
}

fn parse_du(stdout: &str) -> (Vec<DuEntry>, bool, bool) {
    if let Some(rest) = stdout.strip_prefix("MODE=0\n") {
        let mut entries = Vec::new();
        for record in rest.split('\0') {
            let record = record.trim_matches('\n');
            if record.is_empty() {
                continue;
            }
            if let Some((size, path)) = record.split_once('\t') {
                if let (Ok(size), false) = (size.trim().parse::<u64>(), path.is_empty()) {
                    entries.push(DuEntry { path: path.to_string(), size });
                }
            }
        }
        return (entries, true, false);
    }
    let mut lines = stdout.lines();
    let mode = lines.next().unwrap_or("");
    let gnu = mode.contains("MODE=0");
    let mut entries = Vec::new();
    for line in lines {
        let Some((size, path)) = line.split_once('\t') else { continue };
        let Ok(size) = size.trim().parse::<u64>() else { continue };
        if path.is_empty() {
            continue;
        }
        // BusyBox du -k reports KiB.
        entries.push(DuEntry { path: path.to_string(), size: if gnu { size } else { size.saturating_mul(1024) } });
    }
    (entries, gnu, false)
}

fn trim_root(path: &str) -> &str {
    if path.len() > 1 { path.trim_end_matches('/') } else { path }
}

fn base_name(path: &str) -> String {
    let trimmed = trim_root(path);
    if trimmed == "/" {
        return "/".into();
    }
    trimmed.rsplit('/').next().unwrap_or(trimmed).to_string()
}

fn target_for(state: &Arc<AppState>, server_id: &str) -> AppResult<(Target, String)> {
    let server = state.db.get_server(server_id)?;
    let secret = state.secret_for(server_id);
    let name = server.name.clone();
    Ok((Target::from_server(&server, secret), name))
}

pub fn start_usage_scan(state: &Arc<AppState>, server_id: &str, mount: &str) -> AppResult<UsageScan> {
    let server = state.db.get_server(server_id)?;
    let snapshot = state
        .db
        .latest_snapshot(server_id)?
        .ok_or_else(|| AppError::invalid("该服务器还没有巡检数据，请先执行巡检"))?;
    let volume = snapshot
        .volumes
        .iter()
        .find(|v| v.mount == mount)
        .ok_or_else(|| AppError::invalid("所选挂载点不在最近一次巡检结果中"))?;
    if volume.virtual_fs {
        return Err(AppError::invalid("虚拟文件系统不支持目录分析"));
    }
    let root = volume.mount.clone();
    let scan = state.db.create_usage_scan(&server.id, &root, &root)?;
    let cancel = Arc::new(AtomicBool::new(false));
    state.scans.lock().unwrap().insert(scan.id.clone(), cancel.clone());
    let state_clone = state.clone();
    let scan_id = scan.id.clone();
    let server_id = server.id.clone();
    let settings = state.settings();
    std::thread::spawn(move || {
        let scan_result = run_du(
            &state_clone, &server_id, &root, 1, &cancel,
            Duration::from_secs(settings.probe_timeout_secs.max(120)),
        );
        match scan_result {
            Ok((entries, partial, err)) => {
                let root_norm = trim_root(&root).to_string();
                let root_entry = entries.iter().find(|e| trim_root(&e.path) == root_norm);
                let total = root_entry.map(|e| e.size).unwrap_or_else(|| entries.iter().map(|e| e.size).sum());
                let root_id = state_clone.db.insert_usage_node(&scan_id, None, "/", &root, "dir", total, 0);
                let root_id = match root_id {
                    Ok(id) => id,
                    Err(e) => {
                        let _ = state_clone.db.finish_usage_scan(&scan_id, "failed", 0, 0, 0, false, Some(&e.message));
                        state_clone.scans.lock().unwrap().remove(&scan_id);
                        return;
                    }
                };
                let mut children: Vec<&DuEntry> = entries
                    .iter()
                    .filter(|e| trim_root(&e.path) != root_norm)
                    .collect();
                children.sort_by(|a, b| b.size.cmp(&a.size));
                children.truncate(2000);
                let mut inserted = 0u64;
                for entry in children {
                    let path = trim_root(&entry.path).to_string();
                    if state_clone.db.insert_usage_node(&scan_id, Some(root_id), &base_name(&path), &path, "dir", entry.size, 1).is_ok() {
                        inserted += 1;
                    }
                }
                let _ = state_clone.db.mark_usage_node_expanded(root_id);
                let status = if partial { "partial" } else { "succeeded" };
                let err = err.as_deref();
                let _ = state_clone.db.finish_usage_scan(&scan_id, status, total, inserted + 1, 0, partial, err);
            }
            Err((error, cancelled)) => {
                let status = if cancelled { "cancelled" } else { "failed" };
                let _ = state_clone.db.finish_usage_scan(&scan_id, status, 0, 0, 0, false, Some(&error));
            }
        }
        state_clone.scans.lock().unwrap().remove(&scan_id);
    });
    Ok(scan)
}

fn run_du(
    state: &Arc<AppState>,
    server_id: &str,
    root: &str,
    depth: u32,
    cancel: &Arc<AtomicBool>,
    timeout: Duration,
) -> Result<(Vec<DuEntry>, bool, Option<String>), (String, bool)> {
    let (target, _) = target_for(state, server_id).map_err(|e| (e.message, false))?;
    let options = ExecOptions {
        connect_timeout: Duration::from_secs(state.settings().connect_timeout_secs.max(2)),
        timeout,
        cancel: cancel.clone(),
    };
    let output = ssh::exec_script(
        &target,
        &state.known_hosts_path,
        USAGE_SCRIPT,
        &[root.to_string(), depth.to_string()],
        &options,
    )
    .map_err(|e| (e.message, false))?;
    if output.cancelled {
        return Err(("分析已取消".into(), true));
    }
    if output.timed_out {
        return Err(("目录分析超时，请缩小范围或增加超时时间".into(), false));
    }
    if output.stdout.trim().is_empty() && !output.stderr.trim().is_empty() {
        return Err((short_text(&output.stderr, 800), false));
    }
    if output.stdout.trim() == "MODE=2" {
        return Err(("远端没有可用的 du 命令".into(), false));
    }
    let (entries, _gnu, _truncated) = parse_du(&output.stdout);
    if entries.is_empty() && !output.success() {
        return Err((short_text(&output.stderr, 800), false));
    }
    let partial = output.stderr.to_lowercase().contains("permission denied")
        || output.stderr.to_lowercase().contains("cannot access")
        || !output.success();
    let warning = if partial {
        Some("部分目录无权限读取或分析不完整".to_string())
    } else {
        None
    };
    Ok((entries, partial, warning))
}

pub fn cancel_usage_scan(state: &Arc<AppState>, scan_id: &str) -> AppResult<()> {
    if let Some(flag) = state.scans.lock().unwrap().get(scan_id) {
        flag.store(true, Ordering::Relaxed);
    }
    Ok(())
}

pub fn latest_scan(state: &Arc<AppState>, server_id: &str) -> AppResult<Option<UsageScan>> {
    state.db.latest_usage_scan(server_id)
}

pub fn scan_root(state: &Arc<AppState>, scan_id: &str) -> AppResult<Option<UsageNode>> {
    state.db.root_usage_node(scan_id)
}

pub fn children(state: &Arc<AppState>, scan_id: &str, parent_id: i64) -> AppResult<Vec<UsageNode>> {
    state.db.list_usage_children(scan_id, parent_id)
}

pub fn expand_node(state: &Arc<AppState>, node_id: i64) -> AppResult<Vec<UsageNode>> {
    let node = state.db.get_usage_node(node_id)?;
    let scan = state.db.get_usage_scan(&node.scan_id)?;
    if node.expanded_at.is_some() || node.child_count > 0 {
        return state.db.list_usage_children(&scan.id, node.id);
    }
    if scan.status == "running" {
        return Err(AppError::invalid("目录分析仍在进行，请稍后再展开"));
    }
    let server_id = scan.server_id.clone();
    let (target, _) = target_for(state, &server_id)?;
    let cancel = Arc::new(AtomicBool::new(false));
    state.scans.lock().unwrap().insert(scan.id.clone(), cancel.clone());
    let settings = state.settings();
    let result = (|| -> AppResult<Vec<UsageNode>> {
        let options = ExecOptions {
            connect_timeout: Duration::from_secs(settings.connect_timeout_secs.max(2)),
            timeout: Duration::from_secs(settings.probe_timeout_secs.max(120)),
            cancel: cancel.clone(),
        };
        let output = ssh::exec_script(
            &target,
            &state.known_hosts_path,
            USAGE_SCRIPT,
            &[node.path.clone(), "1".to_string()],
            &options,
        )?;
        if output.cancelled {
            return Err(AppError::new("CANCELLED", "分析已取消"));
        }
        if output.timed_out {
            return Err(AppError::new("COMMAND_TIMEOUT", "目录分析超时"));
        }
        if !output.success() && output.stdout.trim().is_empty() {
            return Err(AppError::ssh(short_text(&output.stderr, 800)));
        }
        let (entries, _gnu, _) = parse_du(&output.stdout);
        let node_path = trim_root(&node.path).to_string();
        let root_entry = entries.iter().find(|e| trim_root(&e.path) == node_path);
        if let Some(entry) = root_entry {
            let _ = state.db.update_usage_node_size(node.id, entry.size);
        }
        let mut seen: HashSet<String> = HashSet::new();
        let mut inserted = 0;
        let mut children: Vec<&DuEntry> = entries.iter().filter(|e| trim_root(&e.path) != node_path).collect();
        children.sort_by(|a, b| b.size.cmp(&a.size));
        children.truncate(2000);
        for entry in children {
            let path = trim_root(&entry.path).to_string();
            if !seen.insert(path.clone()) {
                continue;
            }
            let _ = state.db.insert_usage_node(&node.scan_id, Some(node.id), &base_name(&path), &path, "dir", entry.size, node.depth + 1);
            inserted += 1;
        }
        let _ = state.db.mark_usage_node_expanded(node.id);
        let _ = inserted;
        state.db.list_usage_children(&node.scan_id, node.id)
    })();
    state.scans.lock().unwrap().remove(&scan.id);
    result
}

pub fn scan_large_files(state: &Arc<AppState>, scan_id: &str, min_bytes: u64) -> AppResult<Vec<LargeFile>> {
    let scan = state.db.get_usage_scan(scan_id)?;
    let (target, _) = target_for(state, &scan.server_id)?;
    let settings = state.settings();
    let options = ExecOptions {
        connect_timeout: Duration::from_secs(settings.connect_timeout_secs.max(2)),
        timeout: Duration::from_secs(180),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let output = ssh::exec_script(
        &target,
        &state.known_hosts_path,
        LARGE_FILES_SCRIPT,
        &[scan.root_path.clone(), min_bytes.to_string()],
        &options,
    )?;
    if output.cancelled {
        return Err(AppError::new("CANCELLED", "扫描已取消"));
    }
    if output.timed_out {
        return Err(AppError::new("COMMAND_TIMEOUT", "大文件扫描超时"));
    }
    if !output.success() && output.stdout.trim().is_empty() {
        return Err(AppError::ssh(short_text(&output.stderr, 800)));
    }
    let mut seen = HashSet::new();
    for line in output.stdout.lines() {
        let mut parts = line.splitn(3, '\t');
        let Some(size) = parts.next().and_then(|s| s.parse::<u64>().ok()) else { continue };
        let Some(mtime) = parts.next() else { continue };
        let Some(path) = parts.next() else { continue };
        if path.is_empty() || !seen.insert(path.to_string()) {
            continue;
        }
        let mtime = mtime.parse::<f64>().ok().map(|v| v as i64);
        let _ = state.db.insert_large_file(scan_id, path, size, mtime);
    }
    state.db.list_large_files(scan_id)
}

pub fn list_large_files(state: &Arc<AppState>, scan_id: &str) -> AppResult<Vec<LargeFile>> {
    state.db.list_large_files(scan_id)
}

pub fn recommendations(state: &Arc<AppState>, server_id: &str) -> AppResult<Vec<ReclaimCandidate>> {
    let server = state.db.get_server(server_id)?;
    let (target, _) = target_for(state, server_id)?;
    let settings = state.settings();
    let options = ExecOptions {
        connect_timeout: Duration::from_secs(settings.connect_timeout_secs.max(2)),
        timeout: Duration::from_secs(90),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let output = ssh::exec_script(&target, &state.known_hosts_path, RECOMMEND_SCRIPT, &[], &options)?;
    if output.cancelled {
        return Err(AppError::new("CANCELLED", "检测已取消"));
    }
    if output.timed_out {
        return Err(AppError::new("COMMAND_TIMEOUT", "回收建议检测超时"));
    }
    let mut out = Vec::new();
    for line in output.stdout.lines() {
        let mut parts = line.splitn(3, '|');
        let Some(id) = parts.next() else { continue };
        let Some(size) = parts.next().and_then(|s| s.parse::<u64>().ok()) else { continue };
        let Some(detail) = parts.next() else { continue };
        let Some((title, description, risk, command)) = template_meta(id, detail) else { continue };
        out.push(ReclaimCandidate {
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            template_id: id.to_string(),
            title: title.into(),
            description: description.into(),
            size_bytes: size,
            risk: risk.into(),
            reversible: false,
            command_preview: command.into(),
            available: true,
        });
    }
    out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    Ok(out)
}

pub fn template_meta(id: &str, detail: &str) -> Option<(&'static str, &'static str, &'static str, String)> {
    match id {
        "apt_cache" => Some(("APT 下载缓存", "删除 /var/cache/apt/archives 中已下载的软件包，可重新下载", "low", "sudo -n apt-get clean".into())),
        "dnf_cache" => Some(("DNF 缓存", "清理 /var/cache/dnf 元数据与软件包缓存", "low", "sudo -n dnf clean all".into())),
        "yum_cache" => Some(("YUM 缓存", "清理 /var/cache/yum 元数据与软件包缓存", "low", "sudo -n yum clean all".into())),
        "pacman_cache" => Some(("Pacman 缓存", "清理未安装软件的包缓存", "medium", "sudo -n pacman -Sc --noconfirm".into())),
        "apk_cache" => Some(("APK 缓存", "清理 Alpine 软件包缓存", "low", "sudo -n apk cache clean".into())),
        "journal_7d" => Some(("systemd journal", "保留最近 7 天日志，释放旧 journal", "medium", "sudo -n journalctl --vacuum-time=7d".into())),
        "docker_dangling" => Some(("Docker 悬空镜像", "清理悬空镜像与构建缓存，不处理 volume 和运行中容器", "medium", "sudo -n docker image prune -f && sudo -n docker builder prune -f".into())),
        "npm_cache" => Some(("npm 缓存", "清理当前 SSH 用户的 npm 缓存", "low", "npm cache clean --force".into())),
        "pip_cache" => Some(("pip 缓存", "清理当前 SSH 用户的 pip 缓存", "low", "pip cache purge".into())),
        "tmp_old" => Some(("/tmp 旧文件", "删除 /tmp 中超过 7 天的普通文件；可能影响仍在运行的程序", "high", "sudo -n find /tmp -xdev -type f -mtime +7 -delete".into())),
        _ => {
            let _ = detail;
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nul_du() {
        let raw = "MODE=0\n123\t/data\0\n45\t/data/logs\0";
        let (entries, gnu, _) = parse_du(raw);
        assert!(gnu);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].size, 123);
        assert_eq!(entries[1].path, "/data/logs");
    }

    #[test]
    fn parses_busybox_du_as_kib() {
        let raw = "MODE=1\n123\t/data\n45\t/data/logs\n";
        let (entries, gnu, _) = parse_du(raw);
        assert!(!gnu);
        assert_eq!(entries[0].size, 123 * 1024);
    }

    #[test]
    fn basename_handles_root() {
        assert_eq!(base_name("/data/logs/"), "logs");
        assert_eq!(base_name("/"), "/");
    }
}
