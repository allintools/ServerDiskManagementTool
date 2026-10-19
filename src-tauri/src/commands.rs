use crate::cleanup;
use crate::errors::{AppError, AppResult};
use crate::metrics;
use crate::platform;
use crate::secrets;
use crate::ssh;
use crate::state::AppState;
use crate::tasks;
use crate::types::*;
use crate::usage;
use crate::util::{new_id, now_ms};
use std::sync::Arc;
use tauri::State;

type AppStateRef<'a> = State<'a, Arc<AppState>>;

#[tauri::command]
pub fn frontend_ready() -> String {
    println!("[nova-rack] frontend ready");
    "ok".into()
}

#[tauri::command]
pub fn smoke_report(text: String) -> String {
    if let Ok(path) = std::env::var("RACK_SMOKE_FILE") {
        let _ = std::fs::write(path, &text);
    }
    println!("[nova-rack] smoke: {}", text.chars().take(200).collect::<String>());
    "ok".into()
}

#[tauri::command]
pub fn get_app_info(state: AppStateRef<'_>) -> AppResult<AppInfo> {
    let diagnostics = ssh::diagnostics(&state.known_hosts_path, &state.data_dir, env!("CARGO_PKG_VERSION"));
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        product: "服务器硬盘管理工具".into(),
        data_dir: state.data_dir.display().to_string(),
        known_hosts_path: state.known_hosts_path.display().to_string(),
        ssh_path: diagnostics.ssh_path,
        ssh_version: diagnostics.ssh_version,
    })
}

#[tauri::command]
pub fn diagnose_ssh(state: AppStateRef<'_>) -> AppResult<SshDiagnostics> {
    Ok(ssh::diagnostics(&state.known_hosts_path, &state.data_dir, env!("CARGO_PKG_VERSION")))
}

#[tauri::command]
pub fn list_servers(state: AppStateRef<'_>) -> AppResult<Vec<Server>> {
    state.db.list_servers()
}

#[tauri::command]
pub fn get_server(state: AppStateRef<'_>, server_id: String) -> AppResult<Server> {
    state.db.get_server(&server_id)
}

fn normalize_key_path(input_auth: &str, key_path: Option<&str>) -> AppResult<Option<String>> {
    let Some(raw) = key_path.filter(|path| !path.trim().is_empty()) else {
        return Ok(None);
    };
    let path = platform::normalize_path(raw, input_auth == "key").map_err(AppError::invalid)?;
    Ok(Some(path.display().to_string()))
}

fn validate_auth(input_auth: &str, key_path: Option<&str>) -> AppResult<()> {
    if !matches!(input_auth, "agent" | "key" | "password") {
        return Err(AppError::invalid("认证方式无效"));
    }
    if input_auth == "key" {
        let Some(path) = key_path.filter(|s| !s.trim().is_empty()) else {
            return Err(AppError::invalid("密钥认证需要填写私钥路径"));
        };
        if !std::path::Path::new(path).exists() {
            return Err(AppError::invalid(format!("私钥不存在: {path}")));
        }
    }
    Ok(())
}

fn validate_common(name: &str, host: &str, port: u16, user: &str) -> AppResult<()> {
    if name.trim().is_empty() {
        return Err(AppError::invalid("服务器名称不能为空"));
    }
    if host.trim().is_empty() {
        return Err(AppError::invalid("主机地址或 SSH 别名不能为空"));
    }
    if host.starts_with('-') {
        return Err(AppError::invalid("主机地址不能以 - 开头"));
    }
    if port == 0 {
        return Err(AppError::invalid("端口无效"));
    }
    if user.trim().is_empty() {
        return Err(AppError::invalid("用户名不能为空"));
    }
    if user.contains(char::is_whitespace) {
        return Err(AppError::invalid("用户名不能包含空白字符"));
    }
    Ok(())
}

fn apply_secret(state: &Arc<AppState>, server_id: &str, secret: Option<&str>, remember: bool) -> AppResult<Option<String>> {
    let Some(secret) = secret.filter(|s| !s.is_empty()) else { return Ok(None) };
    if remember {
        let reference = secrets::store_secret(server_id, secret)?;
        Ok(Some(reference))
    } else {
        state.ephemeral_secrets.lock().unwrap().insert(server_id.to_string(), secret.to_string());
        Ok(Some("session".into()))
    }
}

#[tauri::command]
pub fn create_server(state: AppStateRef<'_>, mut input: ServerInput) -> AppResult<Server> {
    input.key_path = normalize_key_path(&input.auth_mode, input.key_path.as_deref())?;
    validate_common(&input.name, &input.host, input.port, &input.user)?;
    validate_auth(&input.auth_mode, input.key_path.as_deref())?;
    let state = state.inner().clone();
    let server = state.db.create_server(&input, None)?;
    let secret_ref = match apply_secret(&state, &server.id, input.secret.as_deref(), input.remember_secret) {
        Ok(value) => value,
        Err(err) => {
            let _ = state.db.delete_server(&server.id);
            return Err(err);
        }
    };
    state.db.set_secret_ref(&server.id, secret_ref.as_deref())?;
    state.db.insert_audit("server_create", Some(&server.id), None, &format!("添加服务器 {}", server.name), None)?;
    state.db.get_server(&server.id)
}

#[tauri::command]
pub fn update_server(state: AppStateRef<'_>, mut input: ServerUpdate) -> AppResult<Server> {
    input.key_path = normalize_key_path(&input.auth_mode, input.key_path.as_deref())?;
    validate_common(&input.name, &input.host, input.port, &input.user)?;
    validate_auth(&input.auth_mode, input.key_path.as_deref())?;
    let state = state.inner().clone();
    let _existing = state.db.get_server(&input.id)?;
    let secret_ref = if input.clear_secret {
        let _ = secrets::delete_secret(&input.id);
        state.ephemeral_secrets.lock().unwrap().remove(&input.id);
        None
    } else if input.secret.as_deref().map(|s| !s.is_empty()).unwrap_or(false) {
        apply_secret(&state, &input.id, input.secret.as_deref(), input.remember_secret)?
    } else {
        state.db.get_secret_ref(&input.id)?
    };
    let server = state.db.update_server(&input, secret_ref)?;
    state.db.insert_audit("server_update", Some(&server.id), None, &format!("更新服务器 {}", server.name), None)?;
    Ok(server)
}

#[tauri::command]
pub fn delete_server(state: AppStateRef<'_>, server_id: String) -> AppResult<()> {
    let state = state.inner().clone();
    let server = state.db.get_server(&server_id)?;
    let _ = secrets::delete_secret(&server_id);
    state.ephemeral_secrets.lock().unwrap().remove(&server_id);
    state.db.delete_server(&server_id)?;
    state.db.insert_audit("server_delete", Some(&server_id), None, &format!("删除服务器 {}", server.name), None)?;
    Ok(())
}

#[tauri::command]
pub fn import_ssh_config(state: AppStateRef<'_>) -> AppResult<Vec<ImportCandidate>> {
    let existing: Vec<String> = state.db.list_servers()?.into_iter().filter_map(|s| s.alias).collect();
    Ok(ssh::ssh_config_candidates(&existing))
}

#[tauri::command]
fn test_connection_inner(state: &Arc<AppState>, server_id: &str, secret: Option<String>) -> AppResult<ConnectionTest> {
    let server = state.db.get_server(server_id)?;
    let secret = secret.filter(|s| !s.is_empty()).or_else(|| state.secret_for(server_id));
    let target = ssh::Target::from_server(&server, secret);
    let (host, port) = ssh::effective_host_port(&target);
    let existing = state.db.list_known_hosts()?;
    let has_entry = existing.iter().any(|k| k.host == host && k.port == port);
    if !has_entry {
        let lines = ssh::import_user_known_hosts(&host, port);
        if !lines.is_empty() {
            for key in ssh::fingerprint_known_host_lines(&lines) {
                let _ = state.db.insert_known_host(&host, port, &key.key_type, &key.fingerprint, &key.line);
            }
            let lines = state.db.known_host_lines()?;
            ssh::write_known_hosts(&state.known_hosts_path, &lines)?;
        }
    }
    let settings = state.settings();
    match ssh::test_connection(&target, &state.known_hosts_path, std::time::Duration::from_secs(settings.connect_timeout_secs.max(2))) {
        Ok(mut test) => {
            if test.status == "host_key_unknown" {
                let keys = ssh::scan_host_keys(&target)?;
                let pending_id = new_id();
                let pending = ssh::build_pending(&target, server_id, keys);
                let challenge = ssh::challenge_from_pending(&pending_id, &pending, "请与服务器管理员核对指纹后再信任");
                state.pending_trusts.lock().unwrap().insert(pending_id, pending);
                test.host_key = Some(challenge);
                test.message = "首次连接，请核对主机指纹".into();
                state.db.set_server_status(server_id, "host_key_unknown", Some("等待确认主机指纹"), None)?;
            } else if test.status == "host_key_changed" {
                state.db.set_server_status(server_id, "host_key_changed", Some("主机密钥与已信任记录不一致"), None)?;
            } else if test.status == "online" {
                state.db.set_server_status(server_id, "online", None, Some(now_ms()))?;
            }
            test.server_id = Some(server_id.into());
            Ok(test)
        }
        Err(err) => {
            let _ = state.db.set_server_status(server_id, "error", Some(&err.message), None);
            Err(err)
        }
    }
}

#[tauri::command]
pub async fn test_connection(state: AppStateRef<'_>, server_id: String, secret: Option<String>) -> AppResult<ConnectionTest> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || test_connection_inner(&state, &server_id, secret))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("连接测试失败: {e}")))?
}

#[tauri::command]
pub fn trust_host_key(state: AppStateRef<'_>, pending_id: String) -> AppResult<Vec<KnownHost>> {
    let pending = state.pending_trusts.lock().unwrap().remove(&pending_id)
        .ok_or_else(|| AppError::invalid("主机密钥确认已过期，请重新测试连接"))?;
    if now_ms() - pending.created_at > 5 * 60_000 {
        return Err(AppError::invalid("主机密钥确认已过期，请重新测试连接"));
    }
    for key in &pending.keys {
        state.db.insert_known_host(&pending.host, pending.port, &key.key_type, &key.fingerprint, &key.line)?;
    }
    let lines = state.db.known_host_lines()?;
    ssh::write_known_hosts(&state.known_hosts_path, &lines)?;
    state.db.insert_audit(
        "host_key_trust", Some(&pending.server_id), None,
        &format!("信任 {}:{} 的主机密钥", pending.host, pending.port),
        Some(&serde_json::json!({"fingerprints": pending.keys.iter().map(|k| k.fingerprint.clone()).collect::<Vec<_>>()})),
    )?;
    state.db.list_known_hosts()
}

#[tauri::command]
pub fn list_known_hosts(state: AppStateRef<'_>) -> AppResult<Vec<KnownHost>> {
    state.db.list_known_hosts()
}

#[tauri::command]
pub fn remove_known_host(state: AppStateRef<'_>, id: i64) -> AppResult<()> {
    state.db.remove_known_host(id)?;
    let lines = state.db.known_host_lines()?;
    ssh::write_known_hosts(&state.known_hosts_path, &lines)?;
    state.db.insert_audit("host_key_remove", None, None, &format!("移除主机密钥记录 {id}"), None)?;
    Ok(())
}

#[tauri::command]
pub fn get_latest_snapshot(state: AppStateRef<'_>, server_id: String) -> AppResult<Option<Snapshot>> {
    state.db.latest_snapshot(&server_id)
}

#[tauri::command]
pub fn list_snapshots(state: AppStateRef<'_>, server_id: String, limit: Option<u32>) -> AppResult<Vec<Snapshot>> {
    state.db.list_snapshots(&server_id, limit.unwrap_or(30).min(200))
}

#[tauri::command]
pub fn start_batch_probe(state: AppStateRef<'_>, server_ids: Vec<String>) -> AppResult<Task> {
    tasks::start_probe(&state.inner().clone(), &server_ids)
}

#[tauri::command]
pub fn get_task(state: AppStateRef<'_>, task_id: String) -> AppResult<TaskDetail> {
    tasks::get_task(&state.inner().clone(), &task_id)
}

#[tauri::command]
pub fn list_tasks(state: AppStateRef<'_>, limit: Option<u32>) -> AppResult<Vec<Task>> {
    tasks::list_tasks(&state.inner().clone(), limit.unwrap_or(100).min(500))
}

#[tauri::command]
pub fn cancel_task(state: AppStateRef<'_>, task_id: String) -> AppResult<()> {
    tasks::cancel_task(&state.inner().clone(), &task_id)
}

#[tauri::command]
pub fn start_usage_scan(state: AppStateRef<'_>, server_id: String, mount: String) -> AppResult<UsageScan> {
    usage::start_usage_scan(&state.inner().clone(), &server_id, &mount)
}

#[tauri::command]
pub fn get_usage_scan(state: AppStateRef<'_>, scan_id: String) -> AppResult<UsageScan> {
    state.db.get_usage_scan(&scan_id)
}

#[tauri::command]
pub fn latest_usage_scan(state: AppStateRef<'_>, server_id: String) -> AppResult<Option<UsageScan>> {
    usage::latest_scan(&state.inner().clone(), &server_id)
}

#[tauri::command]
pub fn cancel_usage_scan(state: AppStateRef<'_>, scan_id: String) -> AppResult<()> {
    usage::cancel_usage_scan(&state.inner().clone(), &scan_id)
}

#[tauri::command]
pub fn usage_root(state: AppStateRef<'_>, scan_id: String) -> AppResult<Option<UsageNode>> {
    usage::scan_root(&state.inner().clone(), &scan_id)
}

#[tauri::command]
pub fn list_usage_children(state: AppStateRef<'_>, scan_id: String, parent_id: i64) -> AppResult<Vec<UsageNode>> {
    usage::children(&state.inner().clone(), &scan_id, parent_id)
}

#[tauri::command]
pub async fn expand_usage_node(state: AppStateRef<'_>, node_id: i64) -> AppResult<Vec<UsageNode>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || usage::expand_node(&state, node_id))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("展开目录失败: {e}")))?
}

#[tauri::command]
pub async fn scan_large_files(state: AppStateRef<'_>, scan_id: String, min_bytes: u64) -> AppResult<Vec<LargeFile>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || usage::scan_large_files(&state, &scan_id, min_bytes.clamp(1_048_576, 1_099_511_627_776)))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("大文件扫描失败: {e}")))?
}

#[tauri::command]
pub fn list_large_files(state: AppStateRef<'_>, scan_id: String) -> AppResult<Vec<LargeFile>> {
    usage::list_large_files(&state.inner().clone(), &scan_id)
}

#[tauri::command]
pub async fn get_recommendations(state: AppStateRef<'_>, server_id: String) -> AppResult<Vec<ReclaimCandidate>> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || usage::recommendations(&state, &server_id))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("回收建议检测失败: {e}")))?
}

#[tauri::command]
pub async fn sample_metrics(state: AppStateRef<'_>, server_id: String) -> AppResult<MetricSample> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || metrics::sample(&state, &server_id))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("指标采样失败: {e}")))?
}

#[tauri::command]
pub fn list_alerts(state: AppStateRef<'_>, active_only: Option<bool>, limit: Option<u32>) -> AppResult<Vec<Alert>> {
    state.db.list_alerts(active_only.unwrap_or(true), limit.unwrap_or(200).min(1000))
}

#[tauri::command]
pub fn acknowledge_alert(state: AppStateRef<'_>, alert_id: String) -> AppResult<()> {
    state.db.acknowledge_alert(&alert_id)
}

#[tauri::command]
pub fn resolve_alert(state: AppStateRef<'_>, alert_id: String) -> AppResult<()> {
    state.db.resolve_alert(&alert_id)
}

#[tauri::command]
pub async fn prepare_cleanup_plan(state: AppStateRef<'_>, request: CleanupRequest) -> AppResult<CleanupPlan> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cleanup::prepare_plan(&state, &request))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("生成清理计划失败: {e}")))?
}

#[tauri::command]
pub fn execute_cleanup_plan(state: AppStateRef<'_>, plan_id: String, token: String) -> AppResult<CleanupExecution> {
    cleanup::execute_plan(&state.inner().clone(), &plan_id, &token)
}

#[tauri::command]
pub fn get_cleanup_plan(state: AppStateRef<'_>, plan_id: String) -> AppResult<CleanupPlan> {
    cleanup::get_plan(&state.inner().clone(), &plan_id)
}

#[tauri::command]
pub fn list_quarantine(state: AppStateRef<'_>, server_id: Option<String>) -> AppResult<Vec<QuarantineEntry>> {
    cleanup::list_quarantine(&state.inner().clone(), server_id.as_deref())
}

#[tauri::command]
pub async fn restore_quarantine(state: AppStateRef<'_>, entry_id: i64) -> AppResult<ActionResult> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cleanup::restore_quarantine(&state, entry_id))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("恢复隔离项失败: {e}")))?
}

#[tauri::command]
pub async fn purge_quarantine(state: AppStateRef<'_>, entry_id: i64) -> AppResult<ActionResult> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || cleanup::purge_quarantine(&state, entry_id))
        .await
        .map_err(|e| AppError::new("TASK_JOIN", format!("清理隔离项失败: {e}")))?
}

#[tauri::command]
pub fn get_settings(state: AppStateRef<'_>) -> Settings {
    state.settings()
}

#[tauri::command]
pub fn save_settings(state: AppStateRef<'_>, settings: Settings) -> AppResult<Settings> {
    let mut next = settings;
    next.concurrency = next.concurrency.clamp(1, 32);
    next.cleanup_concurrency = next.cleanup_concurrency.clamp(1, 8);
    next.connect_timeout_secs = next.connect_timeout_secs.clamp(2, 60);
    next.probe_timeout_secs = next.probe_timeout_secs.clamp(5, 300);
    next.cleanup_expiry_mins = next.cleanup_expiry_mins.clamp(1, 60);
    next.snapshot_retention_days = next.snapshot_retention_days.clamp(1, 365);
    next.task_retention_days = next.task_retention_days.clamp(1, 3650);
    next.threshold_warn = next.threshold_warn.clamp(50.0, 99.0);
    next.threshold_critical = next.threshold_critical.clamp(next.threshold_warn, 100.0);
    next.inode_warn = next.inode_warn.clamp(50.0, 99.0);
    next.inode_critical = next.inode_critical.clamp(next.inode_warn, 100.0);
    next.auto_refresh_secs = next.auto_refresh_secs.clamp(0, 3600);
    next.quarantine_days = next.quarantine_days.clamp(1, 90);
    if !matches!(next.theme.as_str(), "dark" | "light" | "system") {
        return Err(AppError::invalid("主题值无效"));
    }
    state.db.save_settings(&next)?;
    *state.settings.lock().unwrap() = next.clone();
    let _ = state.db.prune();
    Ok(next)
}

fn csv_escape(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

#[tauri::command]
pub fn export_report(state: AppStateRef<'_>, path: String, format: String) -> AppResult<String> {
    let target = std::path::PathBuf::from(&path);
    if !target.is_absolute() {
        return Err(AppError::invalid("导出路径必须是绝对路径"));
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::db(format!("无法创建导出目录: {e}")))?;
    }
    let servers = state.db.list_servers()?;
    if format == "json" {
        let mut list = Vec::new();
        for server in &servers {
            let volumes = state.db.list_volumes(&server.id).unwrap_or_default();
            list.push(serde_json::json!({"server": server, "volumes": volumes}));
        }
        let payload = serde_json::json!({
            "product": "服务器硬盘管理工具",
            "version": env!("CARGO_PKG_VERSION"),
            "exportedAt": now_ms(),
            "servers": list,
        });
        let text = serde_json::to_string_pretty(&payload).map_err(|e| AppError::db(e.to_string()))?;
        std::fs::write(&target, text).map_err(|e| AppError::db(format!("写入失败: {e}")))?;
    } else {
        let mut out = String::from("server,group,host,status,os,mount,fsType,device,totalBytes,usedBytes,availableBytes,usedPct,inodePct,readOnly,collectedAt\n");
        for server in &servers {
            let volumes = state.db.list_volumes(&server.id).unwrap_or_default();
            if volumes.is_empty() {
                out.push_str(&format!(
                    "{},{},{},{},{},,,,,,,,,,{}\n",
                    csv_escape(&server.name),
                    csv_escape(server.group_name.as_deref().unwrap_or("")),
                    csv_escape(&server.host),
                    csv_escape(&server.last_status),
                    csv_escape(server.summary.os.as_deref().unwrap_or("")),
                    server.summary.collected_at.unwrap_or(0),
                ));
            }
            for v in volumes {
                out.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{:.1},{},{},{}\n",
                    csv_escape(&server.name),
                    csv_escape(server.group_name.as_deref().unwrap_or("")),
                    csv_escape(&server.host),
                    csv_escape(&server.last_status),
                    csv_escape(server.summary.os.as_deref().unwrap_or("")),
                    csv_escape(&v.mount),
                    csv_escape(&v.fs_type),
                    csv_escape(&v.device),
                    v.total_bytes,
                    v.used_bytes,
                    v.available_bytes,
                    v.used_pct,
                    v.inode_pct.map(|p| format!("{p:.1}")).unwrap_or_default(),
                    if v.read_only { "yes" } else { "no" },
                    server.summary.collected_at.unwrap_or(0),
                ));
            }
        }
        std::fs::write(&target, out).map_err(|e| AppError::db(format!("写入失败: {e}")))?;
    }
    state.db.insert_audit("export", None, None, &format!("导出报告 {}", target.display()), None)?;
    Ok(target.display().to_string())
}

#[tauri::command]
pub fn list_audit(state: AppStateRef<'_>, limit: Option<u32>) -> AppResult<Vec<serde_json::Value>> {
    state.db.list_audit(limit.unwrap_or(100).min(1000))
}
