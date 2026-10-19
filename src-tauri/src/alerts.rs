use crate::errors::AppResult;
use crate::state::AppState;
use crate::types::{Server, Settings, Snapshot};
use std::sync::Arc;

pub fn evaluate_probe(
    state: &Arc<AppState>,
    server: &Server,
    snapshot: &Snapshot,
    settings: &Settings,
) -> AppResult<()> {
    let mut active: Vec<String> = Vec::new();
    for volume in &snapshot.volumes {
        if volume.virtual_fs {
            continue;
        }
        let mount = &volume.mount;
        if volume.used_pct >= settings.threshold_critical {
            let key = format!("volume-critical:{mount}");
            state.db.upsert_alert(
                &server.id, &server.name, "volume_full", "critical", &key,
                &format!("{mount} 容量 {:.1}%", volume.used_pct),
                &format!("已用 {:.1}%，阈值 {:.0}%", volume.used_pct, settings.threshold_critical),
            )?;
            active.push(key);
        } else if volume.used_pct >= settings.threshold_warn {
            let key = format!("volume-warning:{mount}");
            state.db.upsert_alert(
                &server.id, &server.name, "volume_full", "warning", &key,
                &format!("{mount} 容量 {:.1}%", volume.used_pct),
                &format!("已用 {:.1}%，阈值 {:.0}%", volume.used_pct, settings.threshold_warn),
            )?;
            active.push(key);
        }
        if let Some(inode) = volume.inode_pct {
            if inode >= settings.inode_critical {
                let key = format!("inode-critical:{mount}");
                state.db.upsert_alert(
                    &server.id, &server.name, "inode_full", "critical", &key,
                    &format!("{mount} inode {:.1}%", inode),
                    "inode 即将耗尽，即使容量还有剩余也无法创建新文件",
                )?;
                active.push(key);
            } else if inode >= settings.inode_warn {
                let key = format!("inode-warning:{mount}");
                state.db.upsert_alert(
                    &server.id, &server.name, "inode_full", "warning", &key,
                    &format!("{mount} inode {:.1}%", inode),
                    "inode 使用率较高",
                )?;
                active.push(key);
            }
        }
        if volume.read_only {
            let key = format!("readonly:{mount}");
            state.db.upsert_alert(
                &server.id, &server.name, "readonly_mount", "warning", &key,
                &format!("{mount} 为只读挂载"),
                "该文件系统当前以只读方式挂载，写入可能失败",
            )?;
            active.push(key);
        }
    }
    state.db.resolve_stale_alerts(&server.id, &active)?;
    Ok(())
}

pub fn mark_unreachable(state: &Arc<AppState>, server: &Server, error: &str) -> AppResult<()> {
    state.db.upsert_alert(
        &server.id, &server.name, "unreachable", "critical", "server:offline",
        "服务器不可达",
        &format!("最近一次巡检失败: {error}"),
    )?;
    Ok(())
}
