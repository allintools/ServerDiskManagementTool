mod alerts;
mod askpass;
mod cleanup;
mod commands;
mod errors;
mod metrics;
mod platform;
mod probe;
mod secrets;
mod ssh;
mod state;
mod storage;
mod tasks;
mod types;
mod usage;
mod util;

pub use askpass::askpass_main;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let data_dir = if let Some(override_dir) = std::env::var_os("RACK_DATA_DIR") {
                std::path::PathBuf::from(override_dir)
            } else if cfg!(windows) {
                app.path()
                    .app_local_data_dir()
                    .map_err(|e| std::io::Error::other(e.to_string()))?
            } else {
                app.path()
                    .app_data_dir()
                    .map_err(|e| std::io::Error::other(e.to_string()))?
            };
            std::fs::create_dir_all(&data_dir)?;
            let db_path = data_dir.join("nova-rack.sqlite3");
            let db = storage::Db::open(db_path).map_err(std::io::Error::other)?;
            app.manage(Arc::new(AppState::new(db, data_dir)));
            if std::env::var("RACK_SMOKE").is_ok() {
                let handle = app.handle().clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(2200));
                    if let Some(win) = handle.get_webview_window("main") {
                        let _ = win.show();
                        let _ = win.set_focus();
                        let cfg = serde_json::json!({
                            "ssh": std::env::var("RACK_SMOKE_SSH").is_ok(),
                            "host": std::env::var("RACK_TEST_HOST").unwrap_or_else(|_| "127.0.0.1".into()),
                            "port": std::env::var("RACK_TEST_PORT").ok().and_then(|v| v.parse::<u16>().ok()).unwrap_or(2222),
                            "user": std::env::var("RACK_TEST_USER").unwrap_or_else(|_| "rack".into()),
                            "key": std::env::var("RACK_TEST_KEY").unwrap_or_default(),
                            "auth": std::env::var("RACK_TEST_AUTH").unwrap_or_else(|_| "key".into()),
                            "password": std::env::var("RACK_TEST_PASSWORD_FILE").ok()
                                .and_then(|p| std::fs::read_to_string(p).ok()).map(|s| s.trim().to_string()),
                            "mount": std::env::var("RACK_TEST_MOUNT").unwrap_or_else(|_| "/data".into()),
                        });
                        let _ = win.eval(&format!("window.__RACK_SMOKE_CONFIG = {};", cfg));
                        let script = r#"
                          (async () => {
                            const sleep = ms => new Promise(r => setTimeout(r, ms));
                            const out = { pages: [], checks: [] };
                            const invoke = window.__TAURI_INTERNALS__.invoke;
                            const nav = label => [...document.querySelectorAll('.nav-item')].find(b => (b.innerText || '').includes(label));
                            const wait = async (fn, timeout = 8000) => { const start = Date.now(); while (Date.now() - start < timeout) { const v = fn(); if (v) return v; await sleep(200); } return null; };
                            await wait(() => document.querySelectorAll('.nav-item').length === 6);
                            out.navCount = document.querySelectorAll('.nav-item').length;
                            for (const label of ['机群总览','服务器','磁盘分析','任务','告警','设置']) {
                              nav(label)?.click();
                              await sleep(450);
                              out.pages.push({ label, heading: document.querySelector('.title-block h1')?.innerText || '', page: !!document.querySelector('main .page'), empty: !!document.querySelector('.empty-state, .empty-inline') });
                            }
                            nav('服务器')?.click(); await sleep(300);
                            const add = [...document.querySelectorAll('button')].find(b => (b.innerText || '').includes('添加服务器'));
                            add?.click();
                            await wait(() => document.querySelector('.add-server-modal'), 3000);
                            out.checks.push({ addDialog: !!document.querySelector('.add-server-modal') });
                            document.querySelector('.add-server-modal .icon-button')?.click();
                            try {
                              const info = await window.__TAURI_INTERNALS__.invoke('get_app_info');
                              out.appInfo = info;
                              const cfg = window.__RACK_SMOKE_CONFIG || {};
                              if (cfg.ssh) {
                                const server = await invoke('create_server', { input: { name: 'native-smoke', alias: null, host: cfg.host, port: cfg.port, user: cfg.user, authMode: cfg.auth, keyPath: cfg.auth === 'key' ? cfg.key : null, groupName: 'smoke', tags: [], secret: cfg.auth === 'password' ? cfg.password : null, rememberSecret: false } });
                                let test = await invoke('test_connection', { serverId: server.id, secret: null });
                                if (test.status === 'host_key_unknown') {
                                  await invoke('trust_host_key', { pendingId: test.hostKey.pendingId });
                                  test = await invoke('test_connection', { serverId: server.id, secret: null });
                                }
                                const task = await invoke('start_batch_probe', { serverIds: [server.id] });
                                let detail = null;
                                for (let i = 0; i < 120; i++) {
                                  detail = await invoke('get_task', { taskId: task.id });
                                  if (!['running', 'queued'].includes(detail.task.status)) break;
                                  await sleep(400);
                                }
                                const snap = await invoke('get_latest_snapshot', { serverId: server.id });
                                const scan = await invoke('start_usage_scan', { serverId: server.id, mount: cfg.mount });
                                let scanState = scan;
                                for (let i = 0; i < 120; i++) {
                                  scanState = await invoke('get_usage_scan', { scanId: scan.id });
                                  if (!['running', 'queued'].includes(scanState.status)) break;
                                  await sleep(400);
                                }
                                const root = await invoke('usage_root', { scanId: scan.id });
                                const children = root ? await invoke('list_usage_children', { scanId: scan.id, parentId: root.id }) : [];
                                const recs = await invoke('get_recommendations', { serverId: server.id });
                                const metrics = await invoke('sample_metrics', { serverId: server.id });
                                out.ssh = { serverId: server.id, test: test.status, task: detail?.task?.status, volumes: snap?.volumes?.map(v => v.mount), scan: scanState.status, nodes: children.length, recommendations: recs.map(r => r.templateId), metrics: { cpu: metrics.cpuPercent, disks: metrics.disks.length } };
                                location.hash = '#/servers/' + server.id + '/usage';
                                await sleep(2200);
                                out.ui = { heading: document.querySelector('.title-block h1')?.innerText || '', cells: document.querySelectorAll('.treemap-cell').length, rows: document.querySelectorAll('.usage-table-wrap tbody tr').length, tabs: document.querySelectorAll('.tab').length };
                                location.hash = '#/fleet';
                                await sleep(800);
                                out.fleetRows = document.querySelectorAll('.server-table tbody tr').length;
                                location.hash = '#/servers';
                                await sleep(800);
                                const deleteButton = document.querySelector('button[aria-label^="删除服务器"]');
                                out.deleteButton = !!deleteButton;
                                window.confirm = () => true;
                                deleteButton?.click();
                                await sleep(1000);
                                out.serversAfterDelete = (await invoke('list_servers')).length;
                              }
                              const servers = await invoke('list_servers');
                              out.serverCount = servers.length;
                            } catch (e) { out.errors = [String(e)]; }
                            await window.__TAURI_INTERNALS__.invoke('smoke_report', { text: JSON.stringify(out, null, 2) });
                          })().catch(e => window.__TAURI_INTERNALS__.invoke('smoke_report', { text: JSON.stringify({ fatal: String(e) }) }));
                        "#;
                        let _ = win.eval(script);
                    }
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::frontend_ready,
            commands::smoke_report,
            commands::get_app_info,
            commands::diagnose_ssh,
            commands::list_servers,
            commands::get_server,
            commands::create_server,
            commands::update_server,
            commands::delete_server,
            commands::import_ssh_config,
            commands::test_connection,
            commands::trust_host_key,
            commands::list_known_hosts,
            commands::remove_known_host,
            commands::get_latest_snapshot,
            commands::list_snapshots,
            commands::start_batch_probe,
            commands::get_task,
            commands::list_tasks,
            commands::cancel_task,
            commands::start_usage_scan,
            commands::get_usage_scan,
            commands::latest_usage_scan,
            commands::cancel_usage_scan,
            commands::usage_root,
            commands::list_usage_children,
            commands::expand_usage_node,
            commands::scan_large_files,
            commands::list_large_files,
            commands::get_recommendations,
            commands::sample_metrics,
            commands::list_alerts,
            commands::acknowledge_alert,
            commands::resolve_alert,
            commands::prepare_cleanup_plan,
            commands::execute_cleanup_plan,
            commands::get_cleanup_plan,
            commands::list_quarantine,
            commands::restore_quarantine,
            commands::purge_quarantine,
            commands::get_settings,
            commands::save_settings,
            commands::export_report,
            commands::list_audit,
        ])
        .run(tauri::generate_context!())
        .expect("error while running ServerDiskManagementTool");
}

/// Headless integration test used by the Docker SSH harness.
/// It never touches the user's real app data unless RACK_SELFTEST_DATA is set.
pub fn selftest_ssh() -> Result<serde_json::Value, String> {
    use crate::probe::{parse_probe, PROBE_SCRIPT};
    use crate::ssh::{self, ExecOptions, Target};
    use crate::types::*;
    use crate::util::new_id;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    let host = std::env::var("RACK_TEST_HOST").unwrap_or_else(|_| "127.0.0.1".into());
    let port: u16 = std::env::var("RACK_TEST_PORT").ok().and_then(|v| v.parse().ok()).unwrap_or(2222);
    let user = std::env::var("RACK_TEST_USER").unwrap_or_else(|_| "rack".into());
    let key = std::env::var("RACK_TEST_KEY").map_err(|_| "RACK_TEST_KEY 未设置".to_string())?;
    let data_dir = std::env::var("RACK_SELFTEST_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::env::temp_dir().join(format!("nova-rack-selftest-{}", new_id())));
    let _ = std::fs::remove_dir_all(&data_dir);
    std::fs::create_dir_all(&data_dir).map_err(|e| e.to_string())?;
    let db = storage::Db::open(data_dir.join("selftest.sqlite3")).map_err(|e| e.message)?;
    let state = std::sync::Arc::new(AppState::new(db, data_dir.clone()));
    let server = state.db.create_server(&ServerInput {
        name: "selftest".into(), alias: None, host: host.clone(), port, user: user.clone(),
        auth_mode: "key".into(), key_path: Some(key), group_name: Some("selftest".into()),
        tags: vec!["selftest".into()], secret: None, remember_secret: false,
    }, None).map_err(|e| e.message)?;
    let target = Target::from_server(&server, None);
    let settings = state.settings();
    let first_test = ssh::test_connection(&target, &state.known_hosts_path, Duration::from_secs(settings.connect_timeout_secs))?;
    if first_test.status != "host_key_unknown" {
        return Err(format!("首次连接未返回 host_key_unknown，而是 {}", first_test.status));
    }
    let keys = ssh::scan_host_keys(&target)?;
    for key in &keys {
        state.db.insert_known_host(&host, port, &key.key_type, &key.fingerprint, &key.line).map_err(|e| e.message)?;
    }
    let lines = state.db.known_host_lines().map_err(|e| e.message)?;
    ssh::write_known_hosts(&state.known_hosts_path, &lines).map_err(|e| e.message)?;
    let test = ssh::test_connection(&target, &state.known_hosts_path, Duration::from_secs(settings.connect_timeout_secs))?;
    if test.status != "online" {
        return Err(format!("信任主机密钥后连接失败: {}", test.message));
    }
    let fake_dir = data_dir.join("fake-known");
    std::fs::create_dir_all(&fake_dir).map_err(|e| e.to_string())?;
    let fake_key_path = fake_dir.join("fake_key");
    let keygen = ssh::toolchain().map_err(|e| e.message)?.keygen().map_err(|e| e.message)?;
    let _ = platform::command(keygen)
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(&fake_key_path)
        .output();
    let fake_pub = std::fs::read_to_string(fake_dir.join("fake_key.pub")).unwrap_or_default();
    let mut fake_parts = fake_pub.split_whitespace();
    let fake_type = fake_parts.next().unwrap_or("ssh-ed25519");
    let fake_b64 = fake_parts.next().unwrap_or("");
    let fake_known = fake_dir.join("known_hosts");
    let fake_line = if port == 22 { format!("{} {} {}\n", host, fake_type, fake_b64) } else { format!("[{}]:{} {} {}\n", host, port, fake_type, fake_b64) };
    std::fs::write(&fake_known, fake_line).map_err(|e| e.to_string())?;
    let changed_key_status = ssh::test_connection(&target, &fake_known, Duration::from_secs(settings.connect_timeout_secs))
        .map(|t| t.status).unwrap_or_else(|e| format!("error: {}", e.message));
    let password_target = Target {
        alias: None, host: host.clone(), port, user: user.clone(), auth_mode: "password".into(),
        key_path: None, secret: Some(std::env::var("RACK_TEST_PASSWORD").unwrap_or_else(|_| "racktest".into())),
    };
    let password_auth = ssh::test_connection(&password_target, &state.known_hosts_path, Duration::from_secs(settings.connect_timeout_secs))
        .map(|t| t.status).unwrap_or_else(|e| format!("error: {}", e.message));
    let enc_key = std::env::var("RACK_TEST_ENC_KEY").ok();
    let encrypted_key_auth = if let Some(enc_key) = enc_key {
        let target = Target {
            alias: None, host: host.clone(), port, user: user.clone(), auth_mode: "key".into(),
            key_path: Some(enc_key), secret: Some(std::env::var("RACK_TEST_ENC_PASSWORD").unwrap_or_else(|_| "rackpass".into())),
        };
        ssh::test_connection(&target, &state.known_hosts_path, Duration::from_secs(settings.connect_timeout_secs))
            .map(|t| t.status).unwrap_or_else(|e| format!("error: {}", e.message))
    } else { "skipped".into() };
    let offline = state.db.create_server(&ServerInput {
        name: "offline-selftest".into(), alias: None, host: "127.0.0.1".into(), port: 2299,
        user: "rack".into(), auth_mode: "key".into(), key_path: Some(std::env::var("RACK_TEST_KEY").unwrap_or_default()),
        group_name: None, tags: vec![], secret: None, remember_secret: false,
    }, None).map_err(|e| e.message)?;
    let batch_task = tasks::start_probe(&state, &[server.id.clone(), offline.id.clone()]).map_err(|e| e.message)?;
    for _ in 0..200 {
        let detail = state.db.get_task(&batch_task.id).map_err(|e| e.message)?;
        if !["running", "queued"].contains(&detail.task.status.as_str()) { break }
        std::thread::sleep(Duration::from_millis(200));
    }
    let batch = state.db.get_task(&batch_task.id).map_err(|e| e.message)?;
    let nonce = new_id();
    let options = ExecOptions::new(settings.connect_timeout_secs, settings.probe_timeout_secs)
        .with_cancel(std::sync::Arc::new(AtomicBool::new(false)));
    let output = ssh::exec_script(&target, &state.known_hosts_path, PROBE_SCRIPT, &[nonce.clone()], &options).map_err(|e| e.message)?;
    if !output.success() {
        return Err(format!("probe 失败: {}\n{}", output.stderr, output.stdout));
    }
    let parsed = parse_probe(&output.stdout, &output.stderr, &server.id, 0, &settings, &nonce).map_err(|e| e.message)?;
    state.db.apply_snapshot(&server.id, &parsed.snapshot, 0, &parsed.counts).map_err(|e| e.message)?;
    let metrics = metrics::sample(&state, &server.id).map_err(|e| e.message)?;
    let data_volume = parsed.snapshot.volumes.iter().find(|v| v.mount == "/data").map(|v| v.mount.clone())
        .or_else(|| parsed.snapshot.volumes.iter().find(|v| v.mount == "/").map(|v| v.mount.clone()));
    let Some(mount) = data_volume else { return Err("未找到可分析卷 /data 或 /".into()) };
    let scan = usage::start_usage_scan(&state, &server.id, &mount).map_err(|e| e.message)?;
    for _ in 0..300 {
        let current = state.db.get_usage_scan(&scan.id).map_err(|e| e.message)?;
        if !["running", "queued"].contains(&current.status.as_str()) { break }
        std::thread::sleep(Duration::from_millis(200));
    }
    let scan = state.db.get_usage_scan(&scan.id).map_err(|e| e.message)?;
    if scan.status != "succeeded" && scan.status != "partial" {
        return Err(format!("目录分析失败: {:?}", scan.error));
    }
    let root = state.db.root_usage_node(&scan.id).map_err(|e| e.message)?.ok_or_else(|| "缺少根节点".to_string())?;
    let children = state.db.list_usage_children(&scan.id, root.id).map_err(|e| e.message)?;
    let large = usage::scan_large_files(&state, &scan.id, 1_048_576).map_err(|e| e.message)?;
    let Some(large_file) = large.first() else { return Err("大文件扫描没有结果（测试夹具缺少大文件）".into()) };
    let recommendations = usage::recommendations(&state, &server.id).map_err(|e| e.message)?;
    let template = recommendations.iter().find(|r| r.template_id == "apt_cache").map(|r| r.template_id.clone());
    let plan = cleanup::prepare_plan(&state, &CleanupRequest {
        server_ids: vec![server.id.clone()],
        template_ids: template.into_iter().collect(),
        files: vec![FileCleanupRequest { scan_id: scan.id.clone(), node_id: None, large_file_id: Some(large_file.id), action: "quarantine".into() }],
    }).map_err(|e| e.message)?;
    let execution = cleanup::execute_plan(&state, &plan.id, &plan.token).map_err(|e| e.message)?;
    for _ in 0..600 {
        let detail = state.db.get_task(&execution.task_id).map_err(|e| e.message)?;
        if !["running", "queued"].contains(&detail.task.status.as_str()) { break }
        std::thread::sleep(Duration::from_millis(200));
    }
    let final_plan = state.db.get_cleanup_plan(&plan.id).map_err(|e| e.message)?;
    let quarantined = cleanup::list_quarantine(&state, Some(&server.id)).map_err(|e| e.message)?;
    let restore = if let Some(entry) = quarantined.first() {
        cleanup::restore_quarantine(&state, entry.id).ok()
    } else { None };
    let task = state.db.get_task(&execution.task_id).map_err(|e| e.message)?.task;
    let delete_file = large.iter().find(|f| f.path.ends_with("delete-me.bin")).cloned();
    let permanent_delete = if let Some(delete_file) = delete_file {
        let mut allow = state.settings();
        allow.allow_permanent_delete = true;
        state.db.save_settings(&allow).map_err(|e| e.message)?;
        *state.settings.lock().unwrap() = allow;
        let plan = cleanup::prepare_plan(&state, &CleanupRequest {
            server_ids: vec![server.id.clone()],
            template_ids: vec![],
            files: vec![FileCleanupRequest { scan_id: scan.id.clone(), node_id: None, large_file_id: Some(delete_file.id), action: "delete".into() }],
        }).map_err(|e| e.message)?;
        let execution = cleanup::execute_plan(&state, &plan.id, &plan.token).map_err(|e| e.message)?;
        for _ in 0..300 {
            let detail = state.db.get_task(&execution.task_id).map_err(|e| e.message)?;
            if !["running", "queued"].contains(&detail.task.status.as_str()) { break }
            std::thread::sleep(Duration::from_millis(200));
        }
        let verify = ssh::exec_script(
            &target, &state.known_hosts_path,
            "if [ -e \"$1\" ]; then echo exists; else echo missing; fi\n",
            &[delete_file.path.clone()], &ExecOptions::new(settings.connect_timeout_secs, 20),
        ).map(|o| o.stdout.trim().to_string()).unwrap_or_else(|e| format!("error: {}", e.message));
        serde_json::json!({
            "file": delete_file.path,
            "task": state.db.get_task(&execution.task_id).map(|d| d.task.status).unwrap_or_else(|_| "error".into()),
            "plan": state.db.get_cleanup_plan(&plan.id).map(|p| p.status).unwrap_or_else(|_| "error".into()),
            "remote": verify,
        })
    } else {
        serde_json::json!({"skipped": "delete-me.bin not found"})
    };
    let directory_quarantine = if let Some(dir_node) = children.iter().find(|n| n.kind == "dir" && (n.path.ends_with("/logs") || n.path.ends_with("/space dir"))).cloned() {
        let plan = cleanup::prepare_plan(&state, &CleanupRequest {
            server_ids: vec![], template_ids: vec![],
            files: vec![FileCleanupRequest { scan_id: scan.id.clone(), node_id: Some(dir_node.id), large_file_id: None, action: "quarantine".into() }],
        }).map_err(|e| e.message)?;
        let execution = cleanup::execute_plan(&state, &plan.id, &plan.token).map_err(|e| e.message)?;
        for _ in 0..300 {
            let detail = state.db.get_task(&execution.task_id).map_err(|e| e.message)?;
            if !["running", "queued"].contains(&detail.task.status.as_str()) { break }
            std::thread::sleep(Duration::from_millis(200));
        }
        let entries = cleanup::list_quarantine(&state, Some(&server.id)).map_err(|e| e.message)?;
        let entry = entries.iter().find(|e| e.original_path == dir_node.path).cloned();
        let restored = entry.as_ref().map(|e| cleanup::restore_quarantine(&state, e.id).is_ok()).unwrap_or(false);
        serde_json::json!({
            "path": dir_node.path,
            "task": state.db.get_task(&execution.task_id).map(|d| d.task.status).unwrap_or_else(|_| "error".into()),
            "plan": state.db.get_cleanup_plan(&plan.id).map(|p| p.status).unwrap_or_else(|_| "error".into()),
            "restored": restored,
        })
    } else {
        serde_json::json!({"skipped": "directory node not found"})
    };
    let checks = serde_json::json!({
        "serverId": server.id,
        "hostKey": test.status,
        "passwordAuth": password_auth,
        "encryptedKeyAuth": encrypted_key_auth,
        "batch": {"status": batch.task.status, "total": batch.task.total_hosts, "succeeded": batch.task.succeeded, "failed": batch.task.failed},
        "metrics": {"cpu": metrics.cpu_percent, "memory": metrics.memory_used_bytes, "disks": metrics.disks.len()},
        "hostKeyChanged": changed_key_status,
        "permanentDelete": permanent_delete,
        "volumes": parsed.snapshot.volumes.iter().map(|v| serde_json::json!({"mount": v.mount, "fs": v.fs_type, "usedPct": v.used_pct})).collect::<Vec<_>>(),
        "analysis": {"scanId": scan.id, "status": scan.status, "entries": scan.entries, "children": children.len()},
        "largeFiles": large.len(),
        "recommendations": recommendations.iter().map(|r| r.template_id.clone()).collect::<Vec<_>>(),
        "plan": {"id": final_plan.id, "status": final_plan.status, "items": final_plan.items.iter().map(|i| serde_json::json!({"kind": i.kind, "status": i.status, "error": i.error})).collect::<Vec<_>>()},
        "task": task,
        "quarantine": quarantined.len(),
        "restore": restore,
        "directoryQuarantine": directory_quarantine,
        "dataDir": data_dir.display().to_string(),
    });
    Ok(checks)
}
