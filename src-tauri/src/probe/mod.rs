use crate::errors::{AppError, AppResult};
use crate::storage::SnapshotCounts;
use crate::types::*;
use crate::util::{new_id, now_ms};
use std::collections::BTreeMap;

pub const PROBE_SCRIPT: &str = include_str!("probe_v1.sh");

pub struct ProbeOutcome {
    pub snapshot: Snapshot,
    pub counts: SnapshotCounts,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct MountInfo {
    device: String,
    fs_type: String,
    options: Vec<String>,
}

pub fn parse_probe(
    stdout: &str,
    stderr: &str,
    server_id: &str,
    latency_ms: i64,
    settings: &Settings,
    nonce: &str,
) -> AppResult<ProbeOutcome> {
    let sections = split_sections(stdout);
    let meta = parse_key_values(sections.get("meta").map(|s| s.as_str()).unwrap_or(""));
    if meta.is_empty() {
        return Err(AppError::new("PROBE_FAILED", "远端巡检未返回可识别的数据")
            .detail(truncate(stderr, 1200)));
    }
    let complete = sections
        .get("end")
        .map(|s| s.lines().any(|l| l.trim() == nonce))
        .unwrap_or(false);

    let mounts = parse_mounts(sections.get("mounts").map(|s| s.as_str()).unwrap_or(""));
    let df = parse_df(sections.get("df").map(|s| s.as_str()).unwrap_or(""));
    let dfi = parse_dfi(sections.get("dfi").map(|s| s.as_str()).unwrap_or(""));

    let mut volumes: BTreeMap<String, VolumeInfo> = BTreeMap::new();
    for (mount, (device, total, used, available, pct)) in df {
        let info = mounts.get(&mount);
        let (inode_total, inode_used, inode_pct) = dfi
            .get(&mount)
            .copied()
            .map(|(total, used, pct)| (Some(total), Some(used), Some(pct)))
            .unwrap_or((None, None, None));
        let fs_type = info.map(|m| m.fs_type.clone()).unwrap_or_default();
        let options = info.map(|m| m.options.clone()).unwrap_or_default();
        let read_only = options.iter().any(|o| o == "ro");
        volumes.insert(
            mount.clone(),
            VolumeInfo {
                mount,
                device,
                fs_type: fs_type.clone(),
                label: None,
                total_bytes: total,
                used_bytes: used,
                available_bytes: available,
                used_pct: pct,
                inode_total,
                inode_used,
                inode_pct,
                read_only,
                virtual_fs: is_virtual_fs(&fs_type),
                options,
            },
        );
    }

    let caps = parse_caps(
        sections.get("caps").map(|s| s.as_str()).unwrap_or(""),
        &meta,
    );
    let disks = parse_lsblk(sections.get("lsblk").map(|s| s.as_str()).unwrap_or(""));

    let mut volumes: Vec<VolumeInfo> = volumes.into_values().collect();
    // Container runtimes bind-mount individual files (e.g. /etc/hosts) as
    // filesystems; they are not useful disk volumes and would pollute the fleet view.
    volumes.retain(|v| !matches!(v.mount.as_str(), "/etc/hosts" | "/etc/hostname" | "/etc/resolv.conf" | "/etc/machine-id"));
    volumes.sort_by(|a, b| a.mount.cmp(&b.mount));
    let real_max = volumes
        .iter()
        .filter(|v| !v.virtual_fs && v.total_bytes > 0)
        .map(|v| v.used_pct)
        .fold(0.0_f64, f64::max);
    let max_used = if real_max > 0.0 {
        real_max
    } else {
        volumes.iter().map(|v| v.used_pct).fold(0.0_f64, f64::max)
    };

    let mut warning = None;
    let status = if volumes.is_empty() {
        warning = Some("远端未返回文件系统信息，可能是权限不足或缺少 df 命令".into());
        "partial"
    } else if !complete {
        warning = Some("远端输出不完整，结果可能缺少部分文件系统".into());
        "partial"
    } else if !stderr.trim().is_empty() && stderr.to_lowercase().contains("permission denied") {
        warning = Some("部分路径无权限读取，结果可能不完整".into());
        "partial"
    } else {
        "ok"
    };

    let mut read_only = 0u64;
    let mut warn = 0u64;
    let mut critical = 0u64;
    for v in &volumes {
        if v.virtual_fs {
            continue;
        }
        if v.read_only {
            read_only += 1;
        }
        if v.used_pct >= settings.threshold_critical
            || v.inode_pct.map(|p| p >= settings.inode_critical).unwrap_or(false)
        {
            critical += 1;
        } else if v.used_pct >= settings.threshold_warn
            || v.inode_pct.map(|p| p >= settings.inode_warn).unwrap_or(false)
        {
            warn += 1;
        }
    }

    let privilege = if caps.uid == Some(0) {
        "root".to_string()
    } else if caps.sudo_nopass {
        "sudo".to_string()
    } else {
        "user".to_string()
    };

    let snapshot = Snapshot {
        id: new_id(),
        server_id: server_id.into(),
        collected_at: now_ms(),
        status: status.into(),
        duration_ms: latency_ms,
        error: warning.clone(),
        max_used_pct: max_used,
        disks,
        volumes,
        capabilities: caps,
        os: meta.get("os").cloned(),
        distro: meta.get("distro").cloned().filter(|s| !s.is_empty()),
        kernel: meta.get("kernel").cloned(),
        arch: meta.get("arch").cloned(),
        uptime_secs: meta.get("uptime_secs").and_then(|s| s.parse().ok()),
        cpu_count: meta.get("cpu_count").and_then(|s| s.trim().parse().ok()),
        memory_total_bytes: meta.get("memory_total_bytes").and_then(|s| s.trim().parse().ok()),
        load1: meta.get("load1").and_then(|s| s.trim().parse().ok()),
        privilege,
    };
    let counts = SnapshotCounts { read_only, warning: warn, critical };
    Ok(ProbeOutcome { snapshot, counts, warning })
}

#[allow(clippy::type_complexity)]
fn parse_df(raw: &str) -> BTreeMap<String, (String, u64, u64, u64, f64)> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        if line.trim_start().starts_with("Filesystem") {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let total = fields[1].parse::<u64>().ok().map(|v| v.saturating_mul(1024));
        let used = fields[2].parse::<u64>().ok().map(|v| v.saturating_mul(1024));
        let available = fields[3].parse::<u64>().ok().map(|v| v.saturating_mul(1024));
        let pct = fields[4].trim_end_matches('%').parse::<f64>().ok();
        let (Some(total), Some(used), Some(available), Some(pct)) = (total, used, available, pct) else {
            continue;
        };
        let mount = fields[5..].join(" ");
        let device = fields[0].to_string();
        out.insert(mount, (device, total, used, available, pct));
    }
    out
}

#[allow(clippy::type_complexity)]
fn parse_dfi(raw: &str) -> BTreeMap<String, (u64, u64, f64)> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        if line.trim_start().starts_with("Filesystem") {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 6 {
            continue;
        }
        let total = fields[1].parse::<u64>().ok();
        let used = fields[2].parse::<u64>().ok();
        let pct = fields[4].trim_end_matches('%').parse::<f64>().ok();
        let (Some(total), Some(used), Some(pct)) = (total, used, pct) else {
            continue;
        };
        let mount = fields[5..].join(" ");
        out.insert(mount, (total, used, pct));
    }
    out
}

fn parse_mounts(raw: &str) -> BTreeMap<String, MountInfo> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 {
            continue;
        }
        let mount = unescape_mount(fields[1]);
        let fs_type = fields[2].to_string();
        let options = fields[3]
            .split(',')
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        out.insert(
            mount.clone(),
            MountInfo {
                device: unescape_mount(fields[0]),
                fs_type,
                options,
            },
        );
    }
    out
}

fn parse_key_values(raw: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in raw.lines() {
        if let Some((key, value)) = line.split_once('=') {
            out.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    out
}

fn parse_caps(raw: &str, meta: &BTreeMap<String, String>) -> Capabilities {
    let kv = parse_key_values(raw);
    let enabled = |key: &str| kv.get(key).map(|v| v == "1").unwrap_or(false);
    let mut package_managers = Vec::new();
    for id in ["apt-get", "dnf", "yum", "pacman", "apk"] {
        if kv.get(id).map(|v| v == "1").unwrap_or(false) {
            package_managers.push(id.to_string());
        }
    }
    Capabilities {
        posix_shell: true,
        du_zero: enabled("du0"),
        gnu_find: enabled("gnufind"),
        lsblk: enabled("lsblk"),
        sudo_nopass: enabled("sudo_nopass"),
        systemd: enabled("systemctl"),
        journalctl: enabled("journalctl"),
        docker: enabled("docker"),
        timeout: enabled("timeout"),
        package_managers,
        uid: kv.get("uid").and_then(|s| s.trim().parse().ok()),
        containerized: meta.get("containerized").map(|s| s == "1").unwrap_or(false),
    }
}

fn parse_lsblk(raw: &str) -> Vec<DiskInfo> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(raw.trim()) else {
        return Vec::new();
    };
    let Some(devices) = value.get("blockdevices").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for device in devices {
        let kind = device.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if kind != "disk" {
            continue;
        }
        let name = device.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string();
        if name.is_empty() {
            continue;
        }
        let size = json_u64(device.get("size")).unwrap_or(0);
        if size == 0 {
            continue;
        }
        let rotational = json_bool(device.get("rota")).unwrap_or(false);
        let read_only = json_bool(device.get("ro")).unwrap_or(false);
        let model = device
            .get("model")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let serial = device
            .get("serial")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        let disk_type = if name.starts_with("nvme") {
            "nvme".to_string()
        } else if rotational {
            "hdd".to_string()
        } else {
            "ssd".to_string()
        };
        out.push(DiskInfo { name, model, serial, size_bytes: size, disk_type, rotational, read_only });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn json_u64(value: Option<&serde_json::Value>) -> Option<u64> {
    match value? {
        serde_json::Value::Number(n) => n.as_u64(),
        serde_json::Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn json_bool(value: Option<&serde_json::Value>) -> Option<bool> {
    match value? {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::Number(n) => n.as_u64().map(|v| v != 0),
        serde_json::Value::String(s) => Some(s == "1" || s.eq_ignore_ascii_case("true")),
        _ => None,
    }
}

fn is_virtual_fs(fs: &str) -> bool {
    matches!(
        fs,
        "proc" | "sysfs" | "devtmpfs" | "devpts" | "tmpfs" | "cgroup" | "cgroup2"
            | "pstore" | "autofs" | "mqueue" | "hugetlbfs" | "debugfs" | "tracefs"
            | "securityfs" | "configfs" | "fusectl" | "binfmt_misc" | "nsfs" | "squashfs"
            | "overlay" | "ramfs"
    ) || fs.starts_with("fuse.")
}

fn split_sections(stdout: &str) -> BTreeMap<String, String> {
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    let mut current = String::new();
    let mut buffer = String::new();
    for line in stdout.lines() {
        if let Some(name) = line.strip_prefix("@@") {
            if !current.is_empty() {
                out.insert(current.clone(), std::mem::take(&mut buffer));
            }
            current = name.trim().to_string();
        } else {
            buffer.push_str(line);
            buffer.push('\n');
        }
    }
    if !current.is_empty() {
        out.insert(current, buffer);
    }
    out
}

fn unescape_mount(raw: &str) -> String {
    raw.replace("\\040", " ")
        .replace("\\011", "\t")
        .replace("\\012", "\n")
        .replace("\\134", "\\")
}

fn truncate(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        input.to_string()
    } else {
        let mut out: String = input.chars().take(max).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fixture_volumes_and_disks() {
        let nonce = "nonce-1";
        let stdout = include_str!("fixtures/probe_debian.txt").replace("__NONCE__", nonce);
        let settings = Settings::default();
        let out = parse_probe(&stdout, "", "srv-1", 10, &settings, nonce).unwrap();
        assert_eq!(out.snapshot.status, "ok");
        assert!(out.snapshot.volumes.iter().any(|v| v.mount == "/"));
        assert!(out.snapshot.volumes.iter().any(|v| v.mount == "/data"));
        let data = out.snapshot.volumes.iter().find(|v| v.mount == "/data").unwrap();
        assert_eq!(data.total_bytes, 100 * 1024 * 1024 * 1024);
        assert_eq!(data.used_pct, 82.0);
        assert!(data.inode_pct.unwrap() > 0.0);
        assert_eq!(out.snapshot.disks.len(), 1);
        assert_eq!(out.snapshot.disks[0].name, "vda");
        assert!(out.snapshot.capabilities.sudo_nopass);
        assert!(out.snapshot.capabilities.package_managers.iter().any(|p| p == "apt-get"));
        assert_eq!(out.counts.warning, 1);
        assert_eq!(out.counts.read_only, 1);
    }

    #[test]
    fn missing_end_marks_partial() {
        let stdout = "@@meta\nos=Linux\n@@df\n/dev/sda1 1000 500 500 50% /\n";
        let out = parse_probe(stdout, "", "srv-1", 10, &Settings::default(), "nonce").unwrap();
        assert_eq!(out.snapshot.status, "partial");
        assert_eq!(out.snapshot.volumes.len(), 1);
    }

    #[test]
    fn handles_mount_path_with_space() {
        let stdout = "@@meta\nos=Linux\n@@df\n/dev/sda1 1000 500 500 50% /mnt/my data\n@@mounts\n/dev/sda1 /mnt/my\\040data ext4 rw,relatime 0 0\n@@end\nnonce\n";
        let out = parse_probe(stdout, "", "srv-1", 10, &Settings::default(), "nonce").unwrap();
        let volume = out.snapshot.volumes.iter().find(|v| v.mount == "/mnt/my data").unwrap();
        assert_eq!(volume.fs_type, "ext4");
        assert!(!volume.read_only);
    }
}
