use crate::errors::{AppError, AppResult};
use crate::ssh::{self, shell_quote, ExecOptions, Target};
use crate::state::AppState;
use crate::storage::NewCleanupItem;
use crate::types::*;
use crate::usage;
use crate::util::{new_id, now_ms, short_text};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

pub const PREFLIGHT_SCRIPT: &str = include_str!("cleanup_preflight_v1.sh");
pub const EXEC_SCRIPT: &str = include_str!("cleanup_exec_v1.sh");

fn template_supported(template: &str, caps: &Capabilities) -> bool {
    match template {
        "apt_cache" => caps.package_managers.iter().any(|p| p == "apt-get"),
        "dnf_cache" => caps.package_managers.iter().any(|p| p == "dnf"),
        "yum_cache" => caps.package_managers.iter().any(|p| p == "yum"),
        "pacman_cache" => caps.package_managers.iter().any(|p| p == "pacman"),
        "apk_cache" => caps.package_managers.iter().any(|p| p == "apk"),
        "journal_7d" => caps.journalctl || caps.systemd,
        "docker_dangling" => caps.docker,
        "npm_cache" | "pip_cache" | "tmp_old" => true,
        _ => false,
    }
}

fn quarantine_base(mount: &str, batch: &str, idx: usize, name: &str) -> String {
    let mount = if mount == "/" { "" } else { mount.trim_end_matches('/') };
    format!("{mount}/.nova-rack-quarantine/{batch}/{idx:03}-{name}")
}

pub fn prepare_plan(state: &Arc<AppState>, request: &CleanupRequest) -> AppResult<CleanupPlan> {
    let settings = state.settings();
    let mut servers: BTreeSet<String> = request.server_ids.iter().cloned().collect();
    let mut items: Vec<NewCleanupItem> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let batch = new_id();
    let templates: BTreeSet<String> = request.template_ids.iter().cloned().collect();

    for server_id in &request.server_ids {
        let server = state.db.get_server(server_id)?;
        let snapshot = state.db.latest_snapshot(server_id)?;
        let caps = snapshot.map(|s| s.capabilities).unwrap_or_default();
        let supported: Vec<String> = templates
            .iter()
            .filter(|t| template_supported(t, &caps))
            .cloned()
            .collect();
        if supported.is_empty() {
            continue;
        }
        let recs = match usage::recommendations(state, server_id) {
            Ok(recs) => recs,
            Err(_) => Vec::new(),
        };
        let sizes: HashMap<String, u64> = recs.iter().map(|r| (r.template_id.clone(), r.size_bytes)).collect();
        for template in supported {
            let Some((title, description, risk, command)) = usage::template_meta(&template, "") else {
                continue;
            };
            let key = format!("{server_id}|template|{template}");
            if !seen.insert(key) {
                continue;
            }
            items.push(NewCleanupItem {
                server_id: server.id.clone(),
                server_name: server.name.clone(),
                kind: "template".into(),
                template_id: Some(template.clone()),
                path: if template == "tmp_old" { Some("/tmp".into()) } else { None },
                display_path: Some(format!("{} · {}", server.name, title)),
                quarantine_path: None,
                size_bytes: *sizes.get(&template).unwrap_or(&0),
                risk: risk.into(),
                reversible: false,
                command_preview: command,
                default_selected: false,
            });
            let _ = description;
        }
    }

    for file in &request.files {
        let scan = state.db.get_usage_scan(&file.scan_id)?;
        let server = state.db.get_server(&scan.server_id)?;
        servers.insert(server.id.clone());
        if file.action != "quarantine" && file.action != "delete" {
            return Err(AppError::invalid("文件清理动作无效"));
        }
        if file.node_id.is_some() && file.large_file_id.is_some() {
            return Err(AppError::invalid("一次只能选择一个清理目标"));
        }
        let (path, size, display, kind, mtime) = if let Some(node_id) = file.node_id {
            let node = state.db.get_usage_node(node_id)?;
            if node.scan_id != scan.id {
                return Err(AppError::invalid("目录节点不属于所选分析任务"));
            }
            if node.kind != "dir" {
                return Err(AppError::invalid("节点不是目录"));
            }
            if file.action == "delete" {
                return Err(AppError::invalid("首版不允许永久删除目录，请选择隔离"));
            }
            let node_path = node.path.clone();
            (node_path, node.size_bytes, node.path.clone(), "dir", None)
        } else if let Some(large_id) = file.large_file_id {
            let large = state.db.get_large_file(large_id)?;
            if large.scan_id != scan.id {
                return Err(AppError::invalid("文件不属于所选分析任务"));
            }
            let large_path = large.path.clone();
            (large_path, large.size_bytes, large.path.clone(), "file", large.mtime)
        } else {
            return Err(AppError::invalid("缺少清理目标"));
        };
        if path.trim().is_empty() || !path.starts_with('/') {
            return Err(AppError::invalid("目标路径无效"));
        }
        if file.action == "delete" {
            if !settings.allow_permanent_delete {
                return Err(AppError::invalid("永久删除已关闭，请在设置中显式启用"));
            }
            if kind != "file" {
                return Err(AppError::invalid("只允许永久删除普通文件"));
            }
        }
        let key = format!("{}|{}|{}|{}", server.id, file.action, scan.id, path);
        if !seen.insert(key) {
            continue;
        }
        let qpath = if file.action == "quarantine" {
            let name = path.rsplit('/').next().unwrap_or("item");
            Some(quarantine_base(&scan.mount, &batch, items.len() + 1, name))
        } else {
            None
        };
        let preview = if let Some(q) = &qpath {
            format!("mv -- {} {}  (需要时 sudo -n)", shell_quote(&path), shell_quote(q))
        } else {
            format!("rm -f -- {}  (需要时 sudo -n)", shell_quote(&path))
        };
        items.push(NewCleanupItem {
            server_id: server.id.clone(),
            server_name: server.name.clone(),
            kind: if file.action == "quarantine" { "quarantine".into() } else { "delete_file".into() },
            template_id: None,
            path: Some(path.clone()),
            display_path: Some(display),
            quarantine_path: qpath,
            size_bytes: size,
            risk: if file.action == "delete" { "high".into() } else { "medium".into() },
            reversible: file.action == "quarantine",
            command_preview: preview,
            default_selected: false,
        });
        let _ = mtime;
    }

    if items.is_empty() {
        return Err(AppError::invalid("没有可执行的清理项；请选择支持的服务器与模板，或先完成巡检"));
    }
    let token = new_id();
    let expires_at = now_ms() + (settings.cleanup_expiry_mins.max(1) as i64) * 60_000;
    let plan = state.db.create_cleanup_plan(&token, expires_at, &items)?;
    let _ = servers;
    state.db.insert_audit(
        "cleanup_plan", None, None, &format!("生成清理计划 {}，{} 项", plan.id, plan.item_count),
        Some(&serde_json::json!({"expiresAt": plan.expires_at, "totalBytes": plan.total_bytes})),
    )?;
    Ok(plan)
}

pub fn execute_plan(state: &Arc<AppState>, plan_id: &str, token: &str) -> AppResult<CleanupExecution> {
    let plan = state.db.get_cleanup_plan_for_execution(plan_id, token)?;
    let settings = state.settings();
    let selected: Vec<CleanupPlanItem> = plan.items.into_iter().filter(|i| i.selected).collect();
    if selected.is_empty() {
        return Err(AppError::invalid("清理计划没有选中项"));
    }
    let mut grouped: BTreeMap<String, Vec<CleanupPlanItem>> = BTreeMap::new();
    let mut hosts = Vec::new();
    for item in selected {
        if !grouped.contains_key(&item.server_id) {
            hosts.push((item.server_id.clone(), item.server_name.clone()));
        }
        grouped.entry(item.server_id.clone()).or_default().push(item);
    }
    state.db.set_cleanup_plan_status(plan_id, "running")?;
    let concurrency = settings.cleanup_concurrency.clamp(1, 8).min(hosts.len() as u64).max(1);
    let task = state.db.create_task(
        "cleanup", &format!("空间回收 · {} 台服务器", hosts.len()), &hosts, concurrency, Some(plan_id),
    )?;
    let cancel = Arc::new(AtomicBool::new(false));
    state.tasks.lock().unwrap().insert(task.id.clone(), cancel.clone());
    let state_clone = state.clone();
    let task_id = task.id.clone();
    let plan_id_string = plan_id.to_string();
    let grouped = Arc::new(Mutex::new(grouped));
    let thread_plan_id = plan_id_string.clone();
    std::thread::spawn(move || {
        run_cleanup_pool(state_clone.clone(), task_id.clone(), thread_plan_id, grouped, concurrency as usize, cancel);
        state_clone.tasks.lock().unwrap().remove(&task_id);
    });
    Ok(CleanupExecution { task_id: task.id, plan_id: plan_id_string, status: "running".into() })
}

fn run_cleanup_pool(
    state: Arc<AppState>,
    task_id: String,
    plan_id: String,
    grouped: Arc<Mutex<BTreeMap<String, Vec<CleanupPlanItem>>>>,
    concurrency: usize,
    cancel: Arc<AtomicBool>,
) {
    let hosts: Vec<String> = grouped.lock().unwrap().keys().cloned().collect();
    if hosts.is_empty() {
        return;
    }
    let (tx, rx) = mpsc::channel::<String>();
    let rx = Arc::new(Mutex::new(rx));
    let workers = concurrency.max(1).min(hosts.len());
    let mut handles = Vec::new();
    for _ in 0..workers {
        let rx = rx.clone();
        let state = state.clone();
        let task_id = task_id.clone();
        let plan_id = plan_id.clone();
        let grouped = grouped.clone();
        let cancel = cancel.clone();
        handles.push(std::thread::spawn(move || loop {
            let next = rx.lock().unwrap().recv();
            let Ok(server_id) = next else { break };
            let items = grouped.lock().unwrap().get(&server_id).cloned().unwrap_or_default();
            execute_host(&state, &task_id, &plan_id, &server_id, &items, &cancel);
        }));
    }
    for host in hosts {
        let _ = tx.send(host);
    }
    drop(tx);
    for handle in handles {
        let _ = handle.join();
    }
    let any_failed = state.db.task_hosts(&task_id).map(|hosts| hosts.iter().any(|h| h.status == "failed" || h.status == "partial")).unwrap_or(true);
    let any_cancelled = state.db.task_hosts(&task_id).map(|hosts| hosts.iter().any(|h| h.status == "cancelled")).unwrap_or(false);
    if cancel.load(Ordering::Relaxed) || any_cancelled {
        let _ = state.db.set_cleanup_plan_status(&plan_id, "cancelled");
    } else if any_failed {
        let _ = state.db.set_cleanup_plan_status(&plan_id, "partial");
    } else {
        let _ = state.db.set_cleanup_plan_status(&plan_id, "completed");
    }
    let _ = state.db.recount_task(&task_id);
    let _ = state.db.insert_audit(
        "cleanup_execute", None, Some(&task_id), &format!("清理计划 {plan_id} 执行结束"),
        Some(&serde_json::json!({"planId": plan_id, "cancelled": cancel.load(Ordering::Relaxed)})),
    );
}

fn execute_host(
    state: &Arc<AppState>,
    task_id: &str,
    plan_id: &str,
    server_id: &str,
    items: &[CleanupPlanItem],
    cancel: &Arc<AtomicBool>,
) {
    let server = match state.db.get_server(server_id) {
        Ok(server) => server,
        Err(e) => {
            let _ = state.db.finish_task_host(task_id, server_id, "failed", 0, Some(&e.code), Some(&e.message), None, None, None);
            return;
        }
    };
    let _ = state.db.start_task_host(task_id, server_id);
    let settings = state.settings();
    let target = Target::from_server(&server, state.secret_for(server_id));
    let preview = format!("cleanup plan {plan_id}");
    let mut success = 0u64;
    let mut failed = 0u64;
    let mut released = 0u64;
    let mut item_results = Vec::new();
    let mut stop = false;
    for item in items {
        if stop || cancel.load(Ordering::Relaxed) {
            break;
        }
        let _ = state.db.set_cleanup_item_status(item.id, "running", None);
        let action = if item.kind == "template" {
            item.template_id.clone().unwrap_or_default()
        } else if item.kind == "quarantine" {
            "quarantine".to_string()
        } else {
            "delete_file".to_string()
        };
        let path = item.path.clone().unwrap_or_default();
        let qpath = item.quarantine_path.clone().unwrap_or_default();
        let preflight = (|| -> AppResult<crate::ssh::ExecOutput> {
            let options = ExecOptions::new(settings.connect_timeout_secs, 30).with_cancel(cancel.clone());
            ssh::exec_script(
                &target,
                &state.known_hosts_path,
                PREFLIGHT_SCRIPT,
                &[action.clone(), path.clone(), item.size_bytes.to_string(), "0".to_string()],
                &options,
            )
        })();
        let preflight_ok = match preflight {
            Ok(output) => output.success() && output.stdout.contains("PREFLIGHT=OK"),
            Err(_) => false,
        };
        if !preflight_ok {
            failed += 1;
            stop = true;
            let _ = state.db.set_cleanup_item_status(item.id, "failed", Some("执行前检查未通过：目标已变化、权限不足或工具缺失"));
            item_results.push(serde_json::json!({"id": item.id, "status": "failed", "error": "preflight"}));
            continue;
        }
        let options = ExecOptions::new(settings.connect_timeout_secs, 300).with_cancel(cancel.clone());
        let output = ssh::exec_script(
            &target,
            &state.known_hosts_path,
            EXEC_SCRIPT,
            &[action.clone(), path.clone(), qpath.clone()],
            &options,
        );
        let output = match output {
            Ok(output) => output,
            Err(e) => {
                failed += 1;
                stop = true;
                let _ = state.db.set_cleanup_item_status(item.id, "failed", Some(&e.message));
                item_results.push(serde_json::json!({"id": item.id, "status": "failed", "error": e.message}));
                continue;
            }
        };
        let ok = output.success() && output.stdout.contains("CLEANUP=OK");
        let before = parse_free(&output.stdout, "FREE_BEFORE");
        let after = parse_free(&output.stdout, "FREE_AFTER");
        let free_delta = if after > before { (after - before) * 1024 } else { 0 };
        if ok {
            success += 1;
            released = released.saturating_add(free_delta);
            let _ = state.db.set_cleanup_item_status(item.id, "succeeded", None);
            if item.kind == "quarantine" {
                let entry = QuarantineEntry {
                    id: 0,
                    server_id: server.id.clone(),
                    server_name: server.name.clone(),
                    original_path: path.clone(),
                    quarantine_path: qpath.clone(),
                    size_bytes: item.size_bytes,
                    created_at: now_ms(),
                    expires_at: Some(now_ms() + (settings.quarantine_days.max(1) as i64) * 86_400_000),
                    status: "stored".into(),
                };
                let _ = state.db.insert_quarantine(&entry);
            }
            let _ = state.db.insert_audit(
                "cleanup_item", Some(&server.id), Some(task_id),
                &format!("{} {}", action, item.display_path.clone().unwrap_or_default()),
                Some(&serde_json::json!({"itemId": item.id, "freedBytes": free_delta, "ok": true})),
            );
            item_results.push(serde_json::json!({"id": item.id, "status": "succeeded", "freedBytes": free_delta}));
        } else {
            failed += 1;
            stop = true;
            let err = short_text(&format!("{}{}", output.stderr, output.stdout), 800);
            let _ = state.db.set_cleanup_item_status(item.id, "failed", Some(&err));
            let _ = state.db.insert_audit(
                "cleanup_item", Some(&server.id), Some(task_id),
                &format!("{} {}", action, item.display_path.clone().unwrap_or_default()),
                Some(&serde_json::json!({"itemId": item.id, "ok": false, "error": err})),
            );
            item_results.push(serde_json::json!({"id": item.id, "status": "failed", "error": err}));
        }
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = state.db.finish_task_host(task_id, server_id, "cancelled", released, Some("CANCELLED"), Some("计划已取消"), Some(&preview), None, None);
        return;
    }
    let status = if failed == 0 && success > 0 {
        "succeeded"
    } else if success > 0 {
        "partial"
    } else {
        "failed"
    };
    let result = serde_json::json!({
        "planId": plan_id,
        "success": success,
        "failed": failed,
        "freedBytes": released,
        "items": item_results,
    });
    let _ = state.db.finish_task_host(
        task_id, server_id, status, released,
        if failed > 0 { Some("CLEANUP_FAILED") } else { None },
        if failed > 0 { Some("部分或全部清理项失败") } else { None },
        Some(&preview), None, Some(&result),
    );
}

fn parse_free(stdout: &str, key: &str) -> u64 {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}=")))
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(0)
}

pub fn get_plan(state: &Arc<AppState>, plan_id: &str) -> AppResult<CleanupPlan> {
    state.db.get_cleanup_plan(plan_id)
}

pub fn list_quarantine(state: &Arc<AppState>, server_id: Option<&str>) -> AppResult<Vec<QuarantineEntry>> {
    state.db.list_quarantine(server_id, 200)
}

pub fn restore_quarantine(state: &Arc<AppState>, entry_id: i64) -> AppResult<ActionResult> {
    let entry = state.db.get_quarantine(entry_id)?;
    if entry.status != "stored" {
        return Err(AppError::invalid("该隔离项已恢复或已清理"));
    }
    let server = state.db.get_server(&entry.server_id)?;
    let target = Target::from_server(&server, state.secret_for(&entry.server_id));
    let settings = state.settings();
    let options = ExecOptions::new(settings.connect_timeout_secs, 120);
    let output = ssh::exec_script(
        &target,
        &state.known_hosts_path,
        EXEC_SCRIPT,
        &["restore".into(), entry.original_path.clone(), entry.quarantine_path.clone()],
        &options,
    )?;
    if !output.success() || !output.stdout.contains("CLEANUP=OK") {
        return Err(AppError::ssh(short_text(&format!("{}{}", output.stderr, output.stdout), 800)));
    }
    state.db.update_quarantine_status(entry_id, "restored")?;
    state.db.insert_audit(
        "quarantine_restore", Some(&entry.server_id), None,
        &format!("恢复隔离项 {}", entry.original_path),
        Some(&serde_json::json!({"entryId": entry_id})),
    )?;
    Ok(ActionResult { ok: true, message: "已恢复到原路径".into(), task_id: None })
}

pub fn purge_quarantine(state: &Arc<AppState>, entry_id: i64) -> AppResult<ActionResult> {
    let entry = state.db.get_quarantine(entry_id)?;
    if entry.status != "stored" {
        return Err(AppError::invalid("该隔离项已恢复或已清理"));
    }
    let server = state.db.get_server(&entry.server_id)?;
    let target = Target::from_server(&server, state.secret_for(&entry.server_id));
    let settings = state.settings();
    let options = ExecOptions::new(settings.connect_timeout_secs, 120);
    let output = ssh::exec_script(
        &target,
        &state.known_hosts_path,
        EXEC_SCRIPT,
        &["purge".into(), entry.original_path.clone(), entry.quarantine_path.clone()],
        &options,
    )?;
    if !output.success() || !output.stdout.contains("CLEANUP=OK") {
        return Err(AppError::ssh(short_text(&format!("{}{}", output.stderr, output.stdout), 800)));
    }
    state.db.update_quarantine_status(entry_id, "purged")?;
    state.db.insert_audit(
        "quarantine_purge", Some(&entry.server_id), None,
        &format!("清空隔离项 {}", entry.original_path),
        Some(&serde_json::json!({"entryId": entry_id})),
    )?;
    Ok(ActionResult { ok: true, message: "隔离项已永久释放".into(), task_id: None })
}
