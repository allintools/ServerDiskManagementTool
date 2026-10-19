use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_mode: String,
    pub key_path: Option<String>,
    pub has_secret: bool,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
    pub enabled: bool,
    pub last_status: String,
    pub last_seen_at: Option<i64>,
    pub last_error: Option<String>,
    pub summary: ServerSummary,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerSummary {
    pub os: Option<String>,
    pub distro: Option<String>,
    pub kernel: Option<String>,
    pub arch: Option<String>,
    pub uptime_secs: Option<u64>,
    pub cpu_count: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub load1: Option<f64>,
    pub privilege: Option<String>,
    pub max_used_pct: Option<f64>,
    pub volume_count: u64,
    pub read_only_count: u64,
    pub warning_count: u64,
    pub critical_count: u64,
    pub collected_at: Option<i64>,
    pub latency_ms: Option<i64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub name: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub size_bytes: u64,
    pub disk_type: String,
    pub rotational: bool,
    pub read_only: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VolumeInfo {
    pub mount: String,
    pub device: String,
    pub fs_type: String,
    pub label: Option<String>,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_pct: f64,
    pub inode_total: Option<u64>,
    pub inode_used: Option<u64>,
    pub inode_pct: Option<f64>,
    pub read_only: bool,
    pub virtual_fs: bool,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    pub posix_shell: bool,
    pub du_zero: bool,
    pub gnu_find: bool,
    pub lsblk: bool,
    pub sudo_nopass: bool,
    pub systemd: bool,
    pub journalctl: bool,
    pub docker: bool,
    pub timeout: bool,
    pub package_managers: Vec<String>,
    pub uid: Option<u32>,
    pub containerized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub server_id: String,
    pub collected_at: i64,
    pub status: String,
    pub duration_ms: i64,
    pub error: Option<String>,
    pub max_used_pct: f64,
    pub disks: Vec<DiskInfo>,
    pub volumes: Vec<VolumeInfo>,
    pub capabilities: Capabilities,
    pub os: Option<String>,
    pub distro: Option<String>,
    pub kernel: Option<String>,
    pub arch: Option<String>,
    pub uptime_secs: Option<u64>,
    pub cpu_count: Option<u64>,
    pub memory_total_bytes: Option<u64>,
    pub load1: Option<f64>,
    pub privilege: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub status: String,
    pub total_hosts: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub partial: u64,
    pub cancelled: u64,
    pub concurrency: u64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub plan_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskHost {
    pub id: i64,
    pub server_id: String,
    pub server_name: String,
    pub status: String,
    pub started_at: Option<i64>,
    pub finished_at: Option<i64>,
    pub duration_ms: Option<i64>,
    pub bytes: u64,
    pub error_code: Option<String>,
    pub error: Option<String>,
    pub command_preview: Option<String>,
    pub output_excerpt: Option<String>,
    pub result: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    pub task: Task,
    pub hosts: Vec<TaskHost>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageScan {
    pub id: String,
    pub server_id: String,
    pub mount: String,
    pub root_path: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub total_bytes: u64,
    pub entries: u64,
    pub skipped: u64,
    pub partial: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageNode {
    pub id: i64,
    pub scan_id: String,
    pub parent_id: Option<i64>,
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size_bytes: u64,
    pub depth: i64,
    pub child_count: u64,
    pub expanded_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LargeFile {
    pub id: i64,
    pub scan_id: String,
    pub path: String,
    pub size_bytes: u64,
    pub mtime: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub server_id: String,
    pub server_name: String,
    pub kind: String,
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
    pub resolved_at: Option<i64>,
    pub acknowledged_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub theme: String,
    pub concurrency: u64,
    pub connect_timeout_secs: u64,
    pub probe_timeout_secs: u64,
    pub cleanup_concurrency: u64,
    pub cleanup_expiry_mins: u64,
    pub snapshot_retention_days: u64,
    pub task_retention_days: u64,
    pub threshold_warn: f64,
    pub threshold_critical: f64,
    pub inode_warn: f64,
    pub inode_critical: f64,
    pub auto_refresh_secs: u64,
    pub quarantine_days: u64,
    pub allow_permanent_delete: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "dark".into(),
            concurrency: 6,
            connect_timeout_secs: 8,
            probe_timeout_secs: 20,
            cleanup_concurrency: 2,
            cleanup_expiry_mins: 5,
            snapshot_retention_days: 30,
            task_retention_days: 90,
            threshold_warn: 80.0,
            threshold_critical: 90.0,
            inode_warn: 85.0,
            inode_critical: 95.0,
            auto_refresh_secs: 60,
            quarantine_days: 7,
            allow_permanent_delete: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub id: i64,
    pub host: String,
    pub port: u16,
    pub key_type: String,
    pub fingerprint: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostFingerprint {
    pub key_type: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HostKeyChallenge {
    pub pending_id: String,
    pub host: String,
    pub port: u16,
    pub fingerprints: Vec<HostFingerprint>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTest {
    pub status: String,
    pub message: String,
    pub latency_ms: Option<i64>,
    pub host_key: Option<HostKeyChallenge>,
    pub server_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportCandidate {
    pub alias: String,
    pub host: String,
    pub user: String,
    pub port: u16,
    pub identity_file: Option<String>,
    pub already_added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReclaimCandidate {
    pub server_id: String,
    pub server_name: String,
    pub template_id: String,
    pub title: String,
    pub description: String,
    pub size_bytes: u64,
    pub risk: String,
    pub reversible: bool,
    pub command_preview: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCleanupRequest {
    pub scan_id: String,
    pub node_id: Option<i64>,
    pub large_file_id: Option<i64>,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRequest {
    pub server_ids: Vec<String>,
    pub template_ids: Vec<String>,
    pub files: Vec<FileCleanupRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlanItem {
    pub id: i64,
    pub plan_id: String,
    pub server_id: String,
    pub server_name: String,
    pub kind: String,
    pub template_id: Option<String>,
    pub path: Option<String>,
    pub display_path: Option<String>,
    pub quarantine_path: Option<String>,
    pub size_bytes: u64,
    pub risk: String,
    pub reversible: bool,
    pub command_preview: String,
    pub default_selected: bool,
    pub selected: bool,
    pub status: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupPlan {
    pub id: String,
    pub token: String,
    pub status: String,
    pub created_at: i64,
    pub expires_at: i64,
    pub host_count: u64,
    pub item_count: u64,
    pub total_bytes: u64,
    pub irreversible: u64,
    pub items: Vec<CleanupPlanItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineEntry {
    pub id: i64,
    pub server_id: String,
    pub server_name: String,
    pub original_path: String,
    pub quarantine_path: String,
    pub size_bytes: u64,
    pub created_at: i64,
    pub expires_at: Option<i64>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupExecution {
    pub task_id: String,
    pub plan_id: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionResult {
    pub ok: bool,
    pub message: String,
    pub task_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub product: String,
    pub data_dir: String,
    pub known_hosts_path: String,
    pub ssh_path: String,
    pub ssh_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshDiagnostics {
    pub ssh_path: String,
    pub ssh_version: String,
    pub agent_socket: Option<String>,
    pub agent_keys: Vec<String>,
    pub agent_error: Option<String>,
    pub known_hosts_path: String,
    pub data_dir: String,
    pub app_version: String,
    pub os: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskMetric {
    pub name: String,
    pub read_bytes_per_sec: f64,
    pub write_bytes_per_sec: f64,
    pub read_iops: f64,
    pub write_iops: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MetricSample {
    pub collected_at: i64,
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub load1: f64,
    pub disks: Vec<DiskMetric>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInput {
    pub name: String,
    pub alias: Option<String>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_mode: String,
    pub key_path: Option<String>,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
    pub secret: Option<String>,
    pub remember_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerUpdate {
    pub id: String,
    pub name: String,
    pub alias: Option<String>,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub auth_mode: String,
    pub key_path: Option<String>,
    pub group_name: Option<String>,
    pub tags: Vec<String>,
    pub secret: Option<String>,
    pub remember_secret: bool,
    pub clear_secret: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePlanInput {
    pub scan_id: String,
    pub node_id: i64,
    pub action: String,
}
