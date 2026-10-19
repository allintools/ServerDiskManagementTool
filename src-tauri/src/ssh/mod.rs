use crate::askpass;
use crate::errors::{AppError, AppResult};
use crate::platform;
use crate::types::{
    ConnectionTest, HostFingerprint, HostKeyChallenge, ImportCandidate, KnownHost, Server,
    SshDiagnostics,
};
use crate::util::{new_id, now_ms, short_text};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod tools;
pub use tools::toolchain;

#[derive(Debug, Clone)]
pub struct Target {
    pub alias: Option<String>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_mode: String,
    pub key_path: Option<String>,
    pub secret: Option<String>,
}

impl Target {
    pub fn from_server(server: &Server, secret: Option<String>) -> Self {
        let key_path = server.key_path.as_deref().map(|raw| {
            platform::normalize_path(raw, false)
                .map(|path| path.display().to_string())
                .unwrap_or_else(|_| raw.to_string())
        });
        Self {
            alias: server.alias.clone(),
            host: server.host.clone(),
            port: server.port,
            user: server.user.clone(),
            auth_mode: server.auth_mode.clone(),
            key_path,
            secret,
        }
    }

    pub fn destination(&self) -> String {
        if let Some(alias) = &self.alias {
            if !alias.trim().is_empty() {
                return alias.clone();
            }
        }
        self.host.clone()
    }
}

#[derive(Debug, Clone)]
pub struct ExecOptions {
    pub connect_timeout: Duration,
    pub timeout: Duration,
    pub cancel: Arc<AtomicBool>,
}

impl ExecOptions {
    pub fn new(connect_timeout_secs: u64, timeout_secs: u64) -> Self {
        Self {
            connect_timeout: Duration::from_secs(connect_timeout_secs.max(2)),
            timeout: Duration::from_secs(timeout_secs.max(5)),
            cancel: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn with_cancel(mut self, cancel: Arc<AtomicBool>) -> Self {
        self.cancel = cancel;
        self
    }
}

#[derive(Debug, Clone)]
pub struct ExecOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    pub timed_out: bool,
    pub cancelled: bool,
    pub truncated: bool,
}

impl ExecOutput {
    pub fn success(&self) -> bool {
        self.code == Some(0) && !self.timed_out && !self.cancelled
    }
}

#[derive(Debug, Clone)]
pub struct ScannedKey {
    pub key_type: String,
    pub fingerprint: String,
    pub line: String,
}

struct AskpassGuard {
    dir: PathBuf,
}

impl Drop for AskpassGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

struct Askpass {
    program: PathBuf,
    env: Vec<(String, String)>,
    _guard: AskpassGuard,
}

fn make_askpass(secret: &str) -> AppResult<Askpass> {
    let dir = std::env::temp_dir().join(format!("nova-rack-askpass-{}", new_id()));
    std::fs::create_dir_all(&dir).map_err(|e| AppError::ssh(format!("无法创建临时目录: {e}")))?;
    platform::set_private_dir(&dir);
    let secret_path = dir.join("secret");
    std::fs::write(&secret_path, secret.as_bytes())
        .map_err(|e| AppError::ssh(format!("无法写入临时凭据: {e}")))?;
    platform::set_private_file(&secret_path);

    #[cfg(unix)]
    {
        let script_path = dir.join("askpass.sh");
        let script = format!(
            "#!/bin/sh\ncat '{}'\n",
            secret_path
                .display()
                .to_string()
                .replace('\'', "'\\''")
        );
        std::fs::write(&script_path, script)
            .map_err(|e| AppError::ssh(format!("无法写入 askpass: {e}")))?;
        platform::set_private_executable(&script_path);
        return Ok(Askpass {
            program: script_path,
            env: vec![
                ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
                ("DISPLAY".into(), ":0".into()),
            ],
            _guard: AskpassGuard { dir },
        });
    }

    #[cfg(windows)]
    {
        let exe = std::env::current_exe()
            .map_err(|e| AppError::ssh(format!("无法定位应用程序路径: {e}")))?;
        let program = platform::strip_verbatim_prefix(exe);
        let secret_text = secret_path.to_string_lossy().to_string();
        return Ok(Askpass {
            program,
            env: vec![
                ("SSH_ASKPASS_REQUIRE".into(), "force".into()),
                ("DISPLAY".into(), ":0".into()),
                (askpass::ASKPASS_FLAG.into(), "1".into()),
                (askpass::ASKPASS_SECRET_ENV.into(), secret_text),
            ],
            _guard: AskpassGuard { dir },
        });
    }

    #[cfg(not(any(unix, windows)))]
    {
        let _ = secret;
        Err(AppError::ssh("当前平台不支持 askpass"))
    }
}

pub fn ensure_known_hosts(path: &Path) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::ssh(format!("无法创建 known_hosts 目录: {e}")))?;
    }
    if !path.exists() {
        std::fs::write(path, b"").map_err(|e| AppError::ssh(format!("无法创建 known_hosts: {e}")))?;
    }
    platform::set_private_file(path);
    Ok(())
}

pub fn write_known_hosts(path: &Path, lines: &[String]) -> AppResult<()> {
    ensure_known_hosts(path)?;
    let tmp = path.with_extension("tmp");
    let mut body = String::new();
    for line in lines {
        let line = line.trim();
        if !line.is_empty() {
            body.push_str(line);
            body.push('\n');
        }
    }
    std::fs::write(&tmp, body.as_bytes()).map_err(|e| AppError::ssh(format!("无法写入 known_hosts: {e}")))?;
    platform::set_private_file(&tmp);
    platform::replace_file(&tmp, path)
        .map_err(|e| AppError::ssh(format!("无法更新 known_hosts: {e}")))?;
    Ok(())
}

fn known_host_pattern(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

pub fn import_user_known_hosts(host: &str, port: u16) -> Vec<String> {
    let pattern = known_host_pattern(host, port);
    let Ok(tools) = toolchain() else { return Vec::new() };
    let Ok(keygen) = tools.keygen() else { return Vec::new() };
    let output = platform::command(keygen)
        .arg("-F")
        .arg(&pattern)
        .output();
    let Ok(output) = output else { return Vec::new() };
    if !output.status.success() {
        return Vec::new();
    }
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

pub fn fingerprint_known_host_lines(lines: &[String]) -> Vec<ScannedKey> {
    fingerprint_lines(lines)
}

fn fingerprint_lines(lines: &[String]) -> Vec<ScannedKey> {
    let Ok(tools) = toolchain() else { return Vec::new() };
    let Ok(keygen) = tools.keygen() else { return Vec::new() };
    let dir = std::env::temp_dir().join(format!("nova-rack-fp-{}", new_id()));
    let _ = std::fs::create_dir_all(&dir);
    let mut out = Vec::new();
    for (idx, line) in lines.iter().enumerate() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let key_type = line.split_whitespace().nth(1).unwrap_or("unknown").to_string();
        let path = dir.join(format!("line-{idx}"));
        if std::fs::write(&path, format!("{}\n", line.trim())).is_err() {
            continue;
        }
        let output = platform::command(keygen).arg("-lf").arg(&path).output();
        if let Ok(output) = output {
            if output.status.success() {
                let text = String::from_utf8_lossy(&output.stdout);
                let fp = text
                    .split_whitespace()
                    .find(|t| t.starts_with("SHA256:") || t.starts_with("MD5:"))
                    .unwrap_or("")
                    .to_string();
                if !fp.is_empty() {
                    out.push(ScannedKey { key_type, fingerprint: fp, line: line.trim().to_string() });
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
    out
}

pub fn effective_host_port(target: &Target) -> (String, u16) {
    if let Some(alias) = &target.alias {
        if !alias.trim().is_empty() {
            let Ok(tools) = toolchain() else { return (target.host.clone(), target.port) };
            let output = platform::command(&tools.ssh).arg("-G").arg(alias).output();
            if let Ok(output) = output {
                if output.status.success() {
                    let text = String::from_utf8_lossy(&output.stdout);
                    let mut host = target.host.clone();
                    let mut port = target.port;
                    for line in text.lines() {
                        if let Some(v) = line.strip_prefix("hostname ") {
                            if !v.trim().is_empty() {
                                host = v.trim().to_string();
                            }
                        }
                        if let Some(v) = line.strip_prefix("port ") {
                            if let Ok(p) = v.trim().parse::<u16>() {
                                port = p;
                            }
                        }
                    }
                    return (host, port);
                }
            }
        }
    }
    (target.host.clone(), target.port)
}

pub fn scan_host_keys(target: &Target) -> AppResult<Vec<ScannedKey>> {
    let (host, port) = effective_host_port(target);
    let keyscan = toolchain()?.keyscan()?.to_path_buf();
    let output = platform::command(keyscan)
        .arg("-T").arg("5")
        .arg("-p").arg(port.to_string())
        .arg("-t").arg("ed25519,ecdsa,rsa")
        .arg(&host)
        .output()
        .map_err(|e| AppError::ssh(format!("无法执行 ssh-keyscan: {e}")))?;
    if !output.status.success() && output.stdout.is_empty() {
        return Err(AppError::new("HOST_KEY_SCAN_FAILED", "无法扫描主机密钥，请确认主机可达并允许 SSH 连接"));
    }
    let lines: Vec<String> = String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .map(|l| l.to_string())
        .collect();
    if lines.is_empty() {
        return Err(AppError::new("HOST_KEY_SCAN_FAILED", "没有扫描到主机密钥"));
    }
    Ok(fingerprint_lines(&lines))
}

pub fn known_host_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.to_string())
        .collect()
}

fn build_ssh_command(
    target: &Target,
    known_hosts: &Path,
    options: &ExecOptions,
    remote_command: &str,
) -> AppResult<(std::process::Command, Option<Askpass>)> {
    if target.auth_mode == "key" {
        let Some(key) = target.key_path.as_ref().filter(|s| !s.trim().is_empty()) else {
            return Err(AppError::new("KEY_MISSING", "未配置 SSH 私钥路径"));
        };
        if !Path::new(key).exists() {
            return Err(AppError::new("KEY_MISSING", format!("SSH 私钥不存在: {key}")));
        }
    }
    if target.auth_mode == "password" && target.secret.is_none() {
        return Err(AppError::new("SECRET_MISSING", "该服务器需要密码，但系统凭据存储中没有可用凭据"));
    }
    let tools = toolchain()?;
    let mut cmd = platform::command(&tools.ssh);
    cmd.arg("-T");
    cmd.arg("-o").arg(format!("ConnectTimeout={}", options.connect_timeout.as_secs()));
    cmd.arg("-o").arg("ServerAliveInterval=15");
    cmd.arg("-o").arg("ServerAliveCountMax=3");
    cmd.arg("-o").arg("StrictHostKeyChecking=yes");
    let kh_path = platform::quote_ssh_config_path(known_hosts);
    cmd.arg("-o").arg(format!("UserKnownHostsFile={kh_path}"));
    cmd.arg("-o").arg("GlobalKnownHostsFile=/dev/null");
    cmd.arg("-o").arg("UpdateHostKeys=no");
    cmd.arg("-o").arg("LogLevel=ERROR");
    cmd.arg("-o").arg("RequestTTY=no");
    cmd.arg("-o").arg("ClearAllForwardings=yes");
    if target.alias.as_deref().map(|s| !s.trim().is_empty()).unwrap_or(false) {
        // Imported aliases use the user's SSH config, including ProxyJump and IdentityFile.
    } else {
        cmd.arg("-p").arg(target.port.to_string());
        if !target.user.trim().is_empty() {
            cmd.arg("-l").arg(&target.user);
        }
    }
    if let Some(key) = target.key_path.as_ref().filter(|s| !s.trim().is_empty()) {
        cmd.arg("-i").arg(key);
        cmd.arg("-o").arg("IdentitiesOnly=yes");
    }
    if target.auth_mode == "agent" {
        cmd.arg("-o").arg("PreferredAuthentications=publickey");
        cmd.arg("-o").arg("PasswordAuthentication=no");
        cmd.arg("-o").arg("KbdInteractiveAuthentication=no");
    } else if target.auth_mode == "key" {
        cmd.arg("-o").arg("PreferredAuthentications=publickey");
        cmd.arg("-o").arg("PasswordAuthentication=no");
        cmd.arg("-o").arg("KbdInteractiveAuthentication=no");
    } else if target.auth_mode == "password" {
        cmd.arg("-o").arg("PreferredAuthentications=password,keyboard-interactive");
        cmd.arg("-o").arg("PubkeyAuthentication=no");
        cmd.arg("-o").arg("NumberOfPasswordPrompts=1");
    }
    if target.secret.is_none() {
        cmd.arg("-o").arg("BatchMode=yes");
    }
    let askpass = if let Some(secret) = target.secret.as_deref() {
        let askpass = make_askpass(secret)?;
        cmd.env("SSH_ASKPASS", &askpass.program);
        for (key, value) in &askpass.env {
            cmd.env(key, value);
        }
        Some(askpass)
    } else {
        None
    };
    if let Some(sock) = platform::agent_socket() {
        cmd.env("SSH_AUTH_SOCK", sock);
    }
    cmd.arg(target.destination());
    if !remote_command.is_empty() {
        cmd.arg(remote_command);
    }
    cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    Ok((cmd, askpass))
}

pub fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".into();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn read_capped<R: Read + Send + 'static>(mut reader: R) -> std::thread::JoinHandle<(String, bool)> {
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        let mut out = Vec::new();
        let mut truncated = false;
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if out.len() < 8 * 1024 * 1024 {
                        let left = 8 * 1024 * 1024 - out.len();
                        out.extend_from_slice(&buf[..n.min(left)]);
                    } else {
                        truncated = true;
                    }
                }
                Err(_) => break,
            }
        }
        (String::from_utf8_lossy(&out).to_string(), truncated)
    })
}

pub fn exec_script(
    target: &Target,
    known_hosts: &Path,
    script: &str,
    args: &[String],
    options: &ExecOptions,
) -> AppResult<ExecOutput> {
    ensure_known_hosts(known_hosts)?;
    let quoted_args = args.iter().map(|a| shell_quote(a)).collect::<Vec<_>>().join(" ");
    let remote = if quoted_args.is_empty() {
        "sh -s --".to_string()
    } else {
        format!("sh -s -- {quoted_args}")
    };
    let (mut cmd, _askpass_guard) = build_ssh_command(target, known_hosts, options, &remote)?;
    let mut child = cmd.spawn().map_err(|e| AppError::ssh(format!("无法启动 ssh: {e}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        let mut body = script.to_string();
        if !body.ends_with('\n') {
            body.push('\n');
        }
        if let Err(e) = stdin.write_all(body.as_bytes()) {
            let _ = child.kill();
            return Err(AppError::ssh(format!("无法向 ssh 写入脚本: {e}")));
        }
    }
    let stdout_handle = child.stdout.take().map(read_capped);
    let stderr_handle = child.stderr.take().map(read_capped);
    let deadline = Instant::now() + options.timeout;
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if options.cancel.load(Ordering::Relaxed) {
                    cancelled = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                if Instant::now() >= deadline {
                    timed_out = true;
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(e) => {
                let _ = child.kill();
                return Err(AppError::ssh(format!("等待 ssh 进程失败: {e}")));
            }
        }
    };
    let (stdout, t1) = stdout_handle.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    let (stderr, t2) = stderr_handle.map(|h| h.join().unwrap_or_default()).unwrap_or_default();
    Ok(ExecOutput {
        stdout,
        stderr,
        code: status.and_then(|s| s.code()),
        timed_out,
        cancelled,
        truncated: t1 || t2,
    })
}

pub fn classify_error(output: &ExecOutput) -> AppError {
    if output.cancelled {
        return AppError::new("CANCELLED", "操作已取消");
    }
    if output.timed_out {
        return AppError::new("COMMAND_TIMEOUT", "SSH 操作超时").retryable();
    }
    if output.truncated {
        return AppError::new("OUTPUT_TOO_LARGE", "远端输出超过限制，已截断");
    }
    let text = output.stderr.to_lowercase();
    if text.contains("host key verification failed") || text.contains("remote host identification has changed") {
        return AppError::new("HOST_KEY_CHANGED", "主机密钥验证失败，可能发生变化");
    }
    if text.contains("permission denied") || text.contains("no supported authentication methods") {
        return AppError::new("AUTH_FAILED", "SSH 认证失败，请检查密钥、agent 或密码");
    }
    if text.contains("could not resolve hostname") || text.contains("name or service not known") {
        return AppError::new("HOST_UNREACHABLE", "无法解析服务器主机名").retryable();
    }
    if text.contains("connection refused") || text.contains("connection timed out") || text.contains("no route to host") {
        return AppError::new("HOST_UNREACHABLE", "无法连接服务器，请检查网络、端口和防火墙").retryable();
    }
    if text.contains("connection closed") || text.contains("broken pipe") {
        return AppError::new("CONNECTION_CLOSED", "SSH 连接中断").retryable();
    }
    let detail = output.stderr.trim().chars().take(1200).collect::<String>();
    AppError::ssh(if detail.is_empty() { "远端命令失败".into() } else { detail })
}

pub fn exec_ok(
    target: &Target,
    known_hosts: &Path,
    script: &str,
    args: &[String],
    options: &ExecOptions,
) -> AppResult<ExecOutput> {
    let output = exec_script(target, known_hosts, script, args, options)?;
    if !output.success() {
        return Err(classify_error(&output));
    }
    Ok(output)
}

pub struct PendingTrust {
    pub host: String,
    pub port: u16,
    pub keys: Vec<ScannedKey>,
    pub created_at: i64,
    pub server_id: String,
}

pub fn build_pending(target: &Target, server_id: &str, keys: Vec<ScannedKey>) -> PendingTrust {
    let (host, port) = effective_host_port(target);
    PendingTrust { host, port, keys, created_at: now_ms(), server_id: server_id.into() }
}

pub fn challenge_from_pending(pending_id: &str, pending: &PendingTrust, message: &str) -> HostKeyChallenge {
    HostKeyChallenge {
        pending_id: pending_id.into(),
        host: pending.host.clone(),
        port: pending.port,
        fingerprints: pending
            .keys
            .iter()
            .map(|k| HostFingerprint { key_type: k.key_type.clone(), fingerprint: k.fingerprint.clone() })
            .collect(),
        message: message.into(),
    }
}

pub fn known_host_exists(known_hosts: &Path, host: &str, port: u16) -> bool {
    if !known_hosts.exists() {
        return false;
    }
    let pattern = known_host_pattern(host, port);
    let Ok(tools) = toolchain() else { return false };
    let Ok(keygen) = tools.keygen() else { return false };
    platform::command(keygen)
        .arg("-F")
        .arg(pattern)
        .arg("-f")
        .arg(known_hosts)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

pub fn test_connection(target: &Target, known_hosts: &Path, connect_timeout: Duration) -> AppResult<ConnectionTest> {
    let start = Instant::now();
    let options = ExecOptions {
        connect_timeout,
        timeout: Duration::from_secs(15),
        cancel: Arc::new(AtomicBool::new(false)),
    };
    let output = exec_script(target, known_hosts, "printf 'NOVA_RACK_OK\\n'\n", &[], &options)?;
    if output.success() && output.stdout.contains("NOVA_RACK_OK") {
        return Ok(ConnectionTest {
            status: "online".into(),
            message: "连接成功".into(),
            latency_ms: Some(start.elapsed().as_millis() as i64),
            host_key: None,
            server_id: None,
        });
    }
    let err = classify_error(&output);
    if err.code == "HOST_KEY_CHANGED" || output.stderr.to_lowercase().contains("host key verification failed") {
        let (host, port) = effective_host_port(target);
        let keys = scan_host_keys(target).unwrap_or_default();
        let changed = known_host_exists(known_hosts, &host, port);
        return Ok(ConnectionTest {
            status: if keys.is_empty() { "host_key_error".into() } else if changed { "host_key_changed".into() } else { "host_key_unknown".into() },
            message: if keys.is_empty() {
                "主机密钥验证失败，且无法自动获取新指纹".into()
            } else if changed {
                "主机密钥与已信任记录不一致，已阻止连接".into()
            } else {
                "首次连接，请核对指纹".into()
            },
            latency_ms: Some(start.elapsed().as_millis() as i64),
            host_key: Some(HostKeyChallenge {
                pending_id: String::new(),
                host,
                port,
                fingerprints: keys.iter().map(|k| HostFingerprint { key_type: k.key_type.clone(), fingerprint: k.fingerprint.clone() }).collect(),
                message: "请与服务器管理员核对指纹后再信任".into(),
            }),
            server_id: None,
        });
    }
    Err(err)
}

fn parse_ssh_g(text: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in text.lines() {
        if let Some((k, v)) = line.split_once(' ') {
            map.entry(k.to_string()).or_insert_with(|| v.trim().to_string());
        }
    }
    map
}

pub fn ssh_config_candidates(existing: &[String]) -> Vec<ImportCandidate> {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("~"));
    let config = home.join(".ssh/config");
    let Ok(text) = std::fs::read_to_string(config) else { return Vec::new() };
    let mut aliases = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let lower = line.to_ascii_lowercase();
        if !lower.starts_with("host ") {
            continue;
        }
        for alias in line[5..].split_whitespace() {
            if alias.contains('*') || alias.contains('?') || alias.contains('[') || alias.contains('!') {
                continue;
            }
            if !aliases.iter().any(|a| a == alias) {
                aliases.push(alias.to_string());
            }
        }
    }
    let Ok(tools) = toolchain() else { return Vec::new() };
    let mut out = Vec::new();
    for alias in aliases.into_iter().take(200) {
        let output = platform::command(&tools.ssh).arg("-G").arg(&alias).output();
        let (host, user, port, identity) = match output {
            Ok(o) if o.status.success() => {
                let map = parse_ssh_g(&String::from_utf8_lossy(&o.stdout));
                (
                    map.get("hostname").cloned().unwrap_or_else(|| alias.clone()),
                    map.get("user").cloned().unwrap_or_else(|| "root".into()),
                    map.get("port").and_then(|p| p.parse().ok()).unwrap_or(22),
                    map.get("identityfile").cloned(),
                )
            }
            _ => (alias.clone(), "root".into(), 22, None),
        };
        out.push(ImportCandidate {
            alias: alias.clone(),
            host,
            user,
            port,
            identity_file: identity,
            already_added: existing.iter().any(|a| a == &alias),
        });
    }
    out
}

pub fn diagnostics(known_hosts: &Path, data_dir: &Path, app_version: &str) -> SshDiagnostics {
    let (ssh_path, ssh_version, tools) = match toolchain() {
        Ok(tools) => {
            let version = platform::command(&tools.ssh)
                .arg("-V")
                .output()
                .ok()
                .map(|output| String::from_utf8_lossy(&output.stderr).trim().to_string())
                .filter(|text| !text.is_empty())
                .unwrap_or_else(|| "ssh 版本未知".into());
            (tools.ssh.display().to_string(), version, Some(tools))
        }
        Err(error) => ("未找到 OpenSSH".into(), error.message, None),
    };
    let sock = platform::agent_socket();
    let mut agent_keys = Vec::new();
    let mut agent_error = None;
    if let Some(tools) = tools {
        if let Some(add) = &tools.add {
            let mut cmd = platform::command(add);
            cmd.arg("-l");
            if let Some(sock) = &sock {
                cmd.env("SSH_AUTH_SOCK", sock);
            }
            match cmd.output() {
                Ok(output) => {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    if output.status.success() {
                        agent_keys = stdout.lines().map(|line| line.to_string()).collect();
                    } else if output.status.code() != Some(1)
                        || !stderr.to_lowercase().contains("no identities")
                    {
                        let detail = if stderr.trim().is_empty() { stdout } else { stderr };
                        agent_error = Some(short_text(detail.trim(), 300));
                    }
                }
                Err(error) => {
                    agent_error = Some(format!("无法执行 ssh-add: {error}"));
                }
            }
        } else {
            agent_error = Some("未找到 ssh-add，SSH agent 认证不可用".into());
        }
    } else {
        agent_error = Some("未找到 OpenSSH，请安装 OpenSSH Client".into());
    }
    SshDiagnostics {
        ssh_path,
        ssh_version,
        agent_socket: sock,
        agent_keys,
        agent_error,
        known_hosts_path: known_hosts.display().to_string(),
        data_dir: data_dir.display().to_string(),
        app_version: app_version.into(),
        os: std::env::consts::OS.into(),
    }
}

pub fn known_hosts_entries_for_lines(host: &str, port: u16, lines: &[String]) -> Vec<KnownHost> {
    fingerprint_lines(lines)
        .into_iter()
        .map(|key| KnownHost {
            id: 0,
            host: host.into(),
            port,
            key_type: key.key_type,
            fingerprint: key.fingerprint,
            created_at: now_ms(),
        })
        .collect()
}

pub fn rewrite_known_hosts_from_lines(path: &Path, lines: &[String]) -> AppResult<()> {
    write_known_hosts(path, lines)
}

pub fn append_known_hosts_line(path: &Path, line: &str) -> AppResult<()> {
    ensure_known_hosts(path)?;
    let mut body = String::new();
    body.push_str(line.trim());
    body.push('\n');
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| AppError::ssh(format!("无法写入 known_hosts: {e}")))?;
    file.write_all(body.as_bytes())
        .map_err(|e| AppError::ssh(format!("无法写入 known_hosts: {e}")))?;
    platform::set_private_file(path);
    Ok(())
}

pub fn cleanup_stale_askpass() {
    let temp = std::env::temp_dir();
    if let Ok(entries) = std::fs::read_dir(temp) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("nova-rack-askpass-") {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quotes_dangerous_values() {
        assert_eq!(shell_quote("/tmp/a b"), "'/tmp/a b'");
        assert_eq!(shell_quote("a'; rm -rf /"), "'a'\\''; rm -rf /'");
    }

    #[test]
    fn known_host_pattern_adds_port_when_needed() {
        assert_eq!(known_host_pattern("example.com", 22), "example.com");
        assert_eq!(known_host_pattern("example.com", 2222), "[example.com]:2222");
    }

    #[test]
    fn agent_socket_does_not_panic() {
        let _ = platform::agent_socket();
    }
}
