use crate::alerts;
use crate::errors::{AppError, AppResult};
use crate::probe::{parse_probe, PROBE_SCRIPT};
use crate::ssh::{self, ExecOptions, Target};
use crate::state::AppState;
use crate::types::{Task, TaskDetail};
use crate::util::{new_id, now_ms, short_text};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};

pub fn start_probe(state: &Arc<AppState>, server_ids: &[String]) -> AppResult<Task> {
    if server_ids.is_empty() {
        return Err(AppError::invalid("请选择至少一台服务器"));
    }
    let settings = state.settings();
    let mut hosts = Vec::new();
    for id in server_ids {
        let server = state.db.get_server(id)?;
        hosts.push((server.id.clone(), server.name.clone()));
    }
    let concurrency = settings.concurrency.clamp(1, 32).min(hosts.len() as u64).max(1);
    let title = if hosts.len() == 1 {
        format!("巡检 {}", hosts[0].1)
    } else {
        format!("批量巡检 {} 台服务器", hosts.len())
    };
    let task = state.db.create_task("probe", &title, &hosts, concurrency, None)?;
    let cancel = Arc::new(AtomicBool::new(false));
    state.tasks.lock().unwrap().insert(task.id.clone(), cancel.clone());
    let state_clone = state.clone();
    let task_id = task.id.clone();
    let host_ids: Vec<String> = hosts.iter().map(|h| h.0.clone()).collect();
    std::thread::spawn(move || {
        run_probe_pool(state_clone.clone(), task_id.clone(), host_ids, concurrency as usize, cancel);
        state_clone.tasks.lock().unwrap().remove(&task_id);
    });
    Ok(task)
}

fn run_probe_pool(state: Arc<AppState>, task_id: String, hosts: Vec<String>, concurrency: usize, cancel: Arc<AtomicBool>) {
    let (tx, rx) = mpsc::channel::<String>();
    let rx = Arc::new(Mutex::new(rx));
    let workers = concurrency.max(1).min(hosts.len().max(1));
    let mut handles = Vec::new();
    for _ in 0..workers {
        let rx = rx.clone();
        let state = state.clone();
        let task_id = task_id.clone();
        let cancel = cancel.clone();
        handles.push(std::thread::spawn(move || loop {
            let next = rx.lock().unwrap().recv();
            let Ok(server_id) = next else { break };
            if cancel.load(Ordering::Relaxed) {
                let _ = state.db.finish_task_host(
                    &task_id, &server_id, "cancelled", 0, Some("CANCELLED"), Some("任务已取消"), None, None, None,
                );
                continue;
            }
            probe_one(&state, &task_id, &server_id, &cancel);
        }));
    }
    for id in hosts {
        let _ = tx.send(id);
    }
    drop(tx);
    for handle in handles {
        let _ = handle.join();
    }
    if cancel.load(Ordering::Relaxed) {
        let _ = state.db.mark_task_cancelled(&task_id);
    } else {
        let _ = state.db.recount_task(&task_id);
    }
}

pub fn probe_one(state: &Arc<AppState>, task_id: &str, server_id: &str, cancel: &Arc<AtomicBool>) {
    let server = match state.db.get_server(server_id) {
        Ok(server) => server,
        Err(e) => {
            let _ = state.db.finish_task_host(task_id, server_id, "failed", 0, Some(&e.code), Some(&e.message), None, None, None);
            return;
        }
    };
    let _ = state.db.start_task_host(task_id, server_id);
    let settings = state.settings();
    let secret = state.secret_for(server_id);
    let target = Target::from_server(&server, secret);
    let preview = format!("ssh {} probe_v1.sh", target.destination());
    let nonce = new_id();
    let options = ExecOptions::new(settings.connect_timeout_secs, settings.probe_timeout_secs)
        .with_cancel(cancel.clone());
    let started = now_ms();
    let output = match ssh::exec_script(&target, &state.known_hosts_path, PROBE_SCRIPT, &[nonce.clone()], &options) {
        Ok(output) => output,
        Err(e) => {
            let _ = state.db.set_server_status(server_id, "error", Some(&e.message), None);
            let _ = state.db.finish_task_host(task_id, server_id, "failed", 0, Some(&e.code), Some(&e.message), Some(&preview), None, None);
            return;
        }
    };
    if output.cancelled || cancel.load(Ordering::Relaxed) {
        let _ = state.db.finish_task_host(task_id, server_id, "cancelled", 0, Some("CANCELLED"), Some("任务已取消"), Some(&preview), None, None);
        return;
    }
    if !output.success() {
        let err = ssh::classify_error(&output);
        let excerpt = short_text(&format!("{}\n{}", output.stderr, output.stdout), 1200);
        let _ = state.db.set_server_status(server_id, "offline", Some(&err.message), Some(now_ms()));
        let _ = alerts::mark_unreachable(state, &server, &err.message);
        let _ = state.db.finish_task_host(task_id, server_id, "failed", 0, Some(&err.code), Some(&err.message), Some(&preview), Some(&excerpt), None);
        return;
    }
    let latency_ms = now_ms() - started;
    let parsed = match parse_probe(&output.stdout, &output.stderr, server_id, latency_ms, &settings, &nonce) {
        Ok(parsed) => parsed,
        Err(err) => {
            let excerpt = short_text(&format!("{}\n{}", output.stderr, output.stdout), 1200);
            let _ = state.db.set_server_status(server_id, "error", Some(&err.message), Some(now_ms()));
            let _ = state.db.finish_task_host(task_id, server_id, "failed", 0, Some(&err.code), Some(&err.message), Some(&preview), Some(&excerpt), None);
            return;
        }
    };
    let snapshot = parsed.snapshot;
    if let Err(err) = state.db.apply_snapshot(server_id, &snapshot, latency_ms, &parsed.counts) {
        let _ = state.db.finish_task_host(task_id, server_id, "failed", 0, Some(&err.code), Some(&err.message), Some(&preview), None, None);
        return;
    }
    let _ = alerts::evaluate_probe(state, &server, &snapshot, &settings);
    let payload = serde_json::json!({
        "volumes": snapshot.volumes.len(),
        "disks": snapshot.disks.len(),
        "maxUsedPct": snapshot.max_used_pct,
        "status": snapshot.status,
        "collectedAt": snapshot.collected_at,
        "latencyMs": latency_ms,
        "warning": parsed.warning,
    });
    let result = if snapshot.status == "partial" { "partial" } else { "succeeded" };
    let error = if snapshot.status == "partial" { parsed.warning.as_deref() } else { None };
    let _ = state.db.finish_task_host(
        task_id, server_id, result, 0, None, error, Some(&preview), parsed.warning.as_deref(), Some(&payload),
    );
}

pub fn cancel_task(state: &Arc<AppState>, task_id: &str) -> AppResult<()> {
    if let Some(flag) = state.tasks.lock().unwrap().get(task_id) {
        flag.store(true, Ordering::Relaxed);
    }
    state.db.mark_task_cancelled(task_id)?;
    Ok(())
}

pub fn get_task(state: &Arc<AppState>, task_id: &str) -> AppResult<TaskDetail> {
    state.db.get_task(task_id)
}

pub fn list_tasks(state: &Arc<AppState>, limit: u32) -> AppResult<Vec<Task>> {
    state.db.list_tasks(limit)
}
