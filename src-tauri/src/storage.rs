use crate::errors::{AppError, AppResult};
use crate::types::*;
use crate::util::{new_id, now_ms};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::PathBuf;
use std::sync::Mutex;

pub struct Db {
    pub path: PathBuf,
    conn: Mutex<Connection>,
}

#[derive(Debug, Clone)]
pub struct SnapshotCounts {
    pub read_only: u64,
    pub warning: u64,
    pub critical: u64,
}

#[derive(Debug, Clone)]
pub struct NewCleanupItem {
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct CleanupTokenCheck {
    pub plan_id: String,
    pub status: String,
    pub expires_at: i64,
    pub token: String,
}

impl Db {
    pub fn open(path: PathBuf) -> AppResult<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| AppError::db(format!("无法创建数据目录: {e}")))?;
        }
        let conn = Connection::open(&path)
            .map_err(|e| AppError::db(format!("无法打开数据库: {e}")))?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             PRAGMA synchronous=NORMAL;
             PRAGMA busy_timeout=5000;
             PRAGMA foreign_keys=ON;",
        )
        .map_err(|e| AppError::db(format!("数据库初始化失败: {e}")))?;
        let db = Self { path, conn: Mutex::new(conn) };
        db.migrate()?;
        db.prune()?;
        Ok(db)
    }

    pub fn lock(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn migrate(&self) -> AppResult<()> {
        let conn = self.lock();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings(
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS servers(
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                alias TEXT,
                host TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                user TEXT NOT NULL,
                auth_mode TEXT NOT NULL,
                key_path TEXT,
                secret_ref TEXT,
                group_name TEXT,
                tags_json TEXT NOT NULL DEFAULT '[]',
                enabled INTEGER NOT NULL DEFAULT 1,
                last_status TEXT NOT NULL DEFAULT 'unknown',
                last_seen_at INTEGER,
                last_error TEXT,
                os TEXT,
                distro TEXT,
                kernel TEXT,
                arch TEXT,
                uptime_secs INTEGER,
                cpu_count INTEGER,
                memory_total_bytes INTEGER,
                load1 REAL,
                privilege TEXT,
                max_used_pct REAL,
                volume_count INTEGER NOT NULL DEFAULT 0,
                read_only_count INTEGER NOT NULL DEFAULT 0,
                warning_count INTEGER NOT NULL DEFAULT 0,
                critical_count INTEGER NOT NULL DEFAULT 0,
                collected_at INTEGER,
                latency_ms INTEGER,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_servers_status ON servers(last_status);
            CREATE TABLE IF NOT EXISTS snapshots(
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                collected_at INTEGER NOT NULL,
                status TEXT NOT NULL,
                duration_ms INTEGER NOT NULL DEFAULT 0,
                error TEXT,
                max_used_pct REAL,
                payload_json TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_snapshots_server ON snapshots(server_id, collected_at DESC);
            CREATE TABLE IF NOT EXISTS disks(
                server_id TEXT NOT NULL,
                name TEXT NOT NULL,
                model TEXT,
                serial TEXT,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                disk_type TEXT NOT NULL DEFAULT 'unknown',
                rotational INTEGER NOT NULL DEFAULT 0,
                read_only INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY(server_id, name)
            );
            CREATE TABLE IF NOT EXISTS volumes(
                server_id TEXT NOT NULL,
                mount TEXT NOT NULL,
                device TEXT,
                fs_type TEXT,
                label TEXT,
                total_bytes INTEGER NOT NULL DEFAULT 0,
                used_bytes INTEGER NOT NULL DEFAULT 0,
                available_bytes INTEGER NOT NULL DEFAULT 0,
                used_pct REAL NOT NULL DEFAULT 0,
                inode_total INTEGER,
                inode_used INTEGER,
                inode_pct REAL,
                read_only INTEGER NOT NULL DEFAULT 0,
                virtual_fs INTEGER NOT NULL DEFAULT 0,
                options TEXT,
                updated_at INTEGER NOT NULL,
                PRIMARY KEY(server_id, mount)
            );
            CREATE INDEX IF NOT EXISTS idx_volumes_server ON volumes(server_id);
            CREATE TABLE IF NOT EXISTS tasks(
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL,
                params_json TEXT,
                total_hosts INTEGER NOT NULL DEFAULT 0,
                succeeded INTEGER NOT NULL DEFAULT 0,
                failed INTEGER NOT NULL DEFAULT 0,
                partial INTEGER NOT NULL DEFAULT 0,
                cancelled INTEGER NOT NULL DEFAULT 0,
                concurrency INTEGER NOT NULL DEFAULT 4,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                plan_id TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_started ON tasks(started_at DESC);
            CREATE TABLE IF NOT EXISTS task_hosts(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                server_id TEXT NOT NULL,
                server_name TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at INTEGER,
                finished_at INTEGER,
                duration_ms INTEGER,
                bytes INTEGER NOT NULL DEFAULT 0,
                error_code TEXT,
                error TEXT,
                command_preview TEXT,
                output_excerpt TEXT,
                result_json TEXT,
                UNIQUE(task_id, server_id)
            );
            CREATE INDEX IF NOT EXISTS idx_task_hosts_task ON task_hosts(task_id);
            CREATE TABLE IF NOT EXISTS usage_scans(
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                mount TEXT NOT NULL,
                root_path TEXT NOT NULL,
                status TEXT NOT NULL,
                started_at INTEGER NOT NULL,
                finished_at INTEGER,
                total_bytes INTEGER NOT NULL DEFAULT 0,
                entries INTEGER NOT NULL DEFAULT 0,
                skipped INTEGER NOT NULL DEFAULT 0,
                partial INTEGER NOT NULL DEFAULT 0,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_usage_server ON usage_scans(server_id, started_at DESC);
            CREATE TABLE IF NOT EXISTS usage_nodes(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_id TEXT NOT NULL,
                parent_id INTEGER,
                name TEXT NOT NULL,
                path TEXT NOT NULL,
                kind TEXT NOT NULL,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                depth INTEGER NOT NULL DEFAULT 0,
                child_count INTEGER NOT NULL DEFAULT 0,
                expanded_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_nodes_scan ON usage_nodes(scan_id, parent_id);
            CREATE TABLE IF NOT EXISTS large_files(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                scan_id TEXT NOT NULL,
                path TEXT NOT NULL,
                size_bytes INTEGER NOT NULL,
                mtime INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_large_scan ON large_files(scan_id, size_bytes DESC);
            CREATE TABLE IF NOT EXISTS alerts(
                id TEXT PRIMARY KEY,
                server_id TEXT NOT NULL,
                server_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                severity TEXT NOT NULL,
                dedupe_key TEXT NOT NULL,
                title TEXT NOT NULL,
                detail TEXT NOT NULL,
                first_seen_at INTEGER NOT NULL,
                last_seen_at INTEGER NOT NULL,
                resolved_at INTEGER,
                acknowledged_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_alerts_active ON alerts(resolved_at, severity);
            CREATE TABLE IF NOT EXISTS cleanup_plans(
                id TEXT PRIMARY KEY,
                token TEXT NOT NULL,
                status TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                host_count INTEGER NOT NULL,
                item_count INTEGER NOT NULL,
                total_bytes INTEGER NOT NULL,
                irreversible INTEGER NOT NULL DEFAULT 0,
                params_json TEXT
            );
            CREATE TABLE IF NOT EXISTS cleanup_plan_items(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                plan_id TEXT NOT NULL,
                server_id TEXT NOT NULL,
                server_name TEXT NOT NULL,
                kind TEXT NOT NULL,
                template_id TEXT,
                path TEXT,
                display_path TEXT,
                quarantine_path TEXT,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                risk TEXT NOT NULL,
                reversible INTEGER NOT NULL,
                command_preview TEXT NOT NULL,
                default_selected INTEGER NOT NULL DEFAULT 0,
                selected INTEGER NOT NULL DEFAULT 1,
                status TEXT NOT NULL DEFAULT 'planned',
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_cleanup_items_plan ON cleanup_plan_items(plan_id);
            CREATE TABLE IF NOT EXISTS quarantine_entries(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                server_id TEXT NOT NULL,
                server_name TEXT NOT NULL,
                plan_item_id INTEGER,
                original_path TEXT NOT NULL,
                quarantine_path TEXT NOT NULL,
                size_bytes INTEGER NOT NULL DEFAULT 0,
                fingerprint TEXT,
                created_at INTEGER NOT NULL,
                expires_at INTEGER,
                status TEXT NOT NULL DEFAULT 'stored',
                restored_at INTEGER,
                purged_at INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_quarantine_server ON quarantine_entries(server_id, status);
            CREATE TABLE IF NOT EXISTS known_hosts(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                host TEXT NOT NULL,
                port INTEGER NOT NULL,
                key_type TEXT NOT NULL,
                fingerprint TEXT NOT NULL,
                line TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                UNIQUE(host, port, fingerprint)
            );
            CREATE TABLE IF NOT EXISTS audit(
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                at INTEGER NOT NULL,
                kind TEXT NOT NULL,
                server_id TEXT,
                task_id TEXT,
                summary TEXT NOT NULL,
                detail_json TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_audit_at ON audit(at DESC);
            PRAGMA user_version=1;",
        )
        .map_err(|e| AppError::db(format!("数据库迁移失败: {e}")))?;
        Ok(())
    }

    pub fn prune(&self) -> AppResult<()> {
        let conn = self.lock();
        let now = now_ms();
        let snapshot_cutoff = now - 30 * 86_400_000;
        let task_cutoff = now - 90 * 86_400_000;
        let scan_cutoff = now - 7 * 86_400_000;
        let alert_cutoff = now - 30 * 86_400_000;
        conn.execute("DELETE FROM snapshots WHERE collected_at < ?1", params![snapshot_cutoff])?;
        conn.execute(
            "DELETE FROM snapshots WHERE id NOT IN (
                SELECT id FROM snapshots s2 WHERE s2.server_id = snapshots.server_id
                ORDER BY collected_at DESC LIMIT 500
             )",
            [],
        )?;
        conn.execute("DELETE FROM tasks WHERE started_at < ?1", params![task_cutoff])?;
        conn.execute("DELETE FROM task_hosts WHERE task_id NOT IN (SELECT id FROM tasks)", [])?;
        conn.execute("DELETE FROM usage_nodes WHERE scan_id NOT IN (SELECT id FROM usage_scans WHERE started_at >= ?1)", params![scan_cutoff])?;
        conn.execute("DELETE FROM large_files WHERE scan_id NOT IN (SELECT id FROM usage_scans WHERE started_at >= ?1)", params![scan_cutoff])?;
        conn.execute("DELETE FROM usage_scans WHERE started_at < ?1", params![scan_cutoff])?;
        conn.execute("DELETE FROM alerts WHERE resolved_at IS NOT NULL AND resolved_at < ?1", params![alert_cutoff])?;
        conn.execute("DELETE FROM usage_nodes WHERE scan_id NOT IN (SELECT id FROM usage_scans)", [])?;
        conn.execute("DELETE FROM cleanup_plan_items WHERE plan_id NOT IN (SELECT id FROM cleanup_plans WHERE created_at >= ?1)", params![task_cutoff])?;
        conn.execute("DELETE FROM cleanup_plans WHERE created_at < ?1", params![task_cutoff])?;
        Ok(())
    }

    // ---------- settings ----------
    pub fn load_settings(&self) -> Settings {
        let conn = self.lock();
        let raw: Option<String> = conn
            .query_row("SELECT value FROM settings WHERE key='app'", [], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        raw.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
    }

    pub fn save_settings(&self, settings: &Settings) -> AppResult<()> {
        let raw = serde_json::to_string(settings).map_err(|e| AppError::db(e.to_string()))?;
        self.lock().execute(
            "INSERT INTO settings(key,value) VALUES('app',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![raw],
        )?;
        Ok(())
    }

    // ---------- servers ----------
    fn server_columns() -> &'static str {
        "id,name,alias,host,port,user,auth_mode,key_path,secret_ref,group_name,tags_json,enabled,last_status,last_seen_at,last_error,os,distro,kernel,arch,uptime_secs,cpu_count,memory_total_bytes,load1,privilege,max_used_pct,volume_count,read_only_count,warning_count,critical_count,collected_at,latency_ms,created_at,updated_at"
    }

    fn row_to_server(row: &rusqlite::Row<'_>) -> rusqlite::Result<Server> {
        let tags_json: String = row.get(10)?;
        let secret_ref: Option<String> = row.get(8)?;
        let summary = ServerSummary {
            os: row.get(15)?,
            distro: row.get(16)?,
            kernel: row.get(17)?,
            arch: row.get(18)?,
            uptime_secs: row.get::<_, Option<i64>>(19)?.map(|v| v.max(0) as u64),
            cpu_count: row.get::<_, Option<i64>>(20)?.map(|v| v.max(0) as u64),
            memory_total_bytes: row.get::<_, Option<i64>>(21)?.map(|v| v.max(0) as u64),
            load1: row.get(22)?,
            privilege: row.get(23)?,
            max_used_pct: row.get(24)?,
            volume_count: row.get::<_, i64>(25)?.max(0) as u64,
            read_only_count: row.get::<_, i64>(26)?.max(0) as u64,
            warning_count: row.get::<_, i64>(27)?.max(0) as u64,
            critical_count: row.get::<_, i64>(28)?.max(0) as u64,
            collected_at: row.get(29)?,
            latency_ms: row.get(30)?,
        };
        Ok(Server {
            id: row.get(0)?,
            name: row.get(1)?,
            alias: row.get(2)?,
            host: row.get(3)?,
            port: row.get::<_, i64>(4)?.max(1) as u16,
            user: row.get(5)?,
            auth_mode: row.get(6)?,
            key_path: row.get(7)?,
            has_secret: secret_ref.is_some(),
            group_name: row.get(9)?,
            tags: serde_json::from_str(&tags_json).unwrap_or_default(),
            enabled: row.get::<_, i64>(11)? != 0,
            last_status: row.get(12)?,
            last_seen_at: row.get(13)?,
            last_error: row.get(14)?,
            summary,
            created_at: row.get(31)?,
            updated_at: row.get(32)?,
        })
    }

    pub fn list_servers(&self) -> AppResult<Vec<Server>> {
        let conn = self.lock();
        let sql = format!("SELECT {} FROM servers ORDER BY name COLLATE NOCASE", Self::server_columns());
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], Self::row_to_server)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_server(&self, id: &str) -> AppResult<Server> {
        let conn = self.lock();
        let sql = format!("SELECT {} FROM servers WHERE id=?1", Self::server_columns());
        conn.query_row(&sql, params![id], Self::row_to_server)
            .optional()?
            .ok_or_else(|| AppError::new("NOT_FOUND", "服务器不存在"))
    }

    pub fn find_server_by_alias(&self, alias: &str) -> AppResult<Option<Server>> {
        let conn = self.lock();
        let sql = format!("SELECT {} FROM servers WHERE alias=?1", Self::server_columns());
        Ok(conn.query_row(&sql, params![alias], Self::row_to_server).optional()?)
    }

    pub fn create_server(&self, input: &ServerInput, secret_ref: Option<String>) -> AppResult<Server> {
        let id = new_id();
        let now = now_ms();
        let tags = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into());
        self.lock().execute(
            "INSERT INTO servers(id,name,alias,host,port,user,auth_mode,key_path,secret_ref,group_name,tags_json,enabled,last_status,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,1,'unknown',?12,?12)",
            params![
                id, input.name, input.alias, input.host, input.port as i64, input.user,
                input.auth_mode, input.key_path, secret_ref, input.group_name, tags, now
            ],
        )?;
        self.get_server(&id)
    }

    pub fn get_secret_ref(&self, id: &str) -> AppResult<Option<String>> {
        let conn = self.lock();
        Ok(conn.query_row(
            "SELECT secret_ref FROM servers WHERE id=?1",
            params![id],
            |r| r.get(0),
        ).optional()?.flatten())
    }

    pub fn set_secret_ref(&self, id: &str, secret_ref: Option<&str>) -> AppResult<()> {
        self.lock().execute(
            "UPDATE servers SET secret_ref=?2,updated_at=?3 WHERE id=?1",
            params![id, secret_ref, now_ms()],
        )?;
        Ok(())
    }

    pub fn update_server(&self, input: &ServerUpdate, secret_ref: Option<String>) -> AppResult<Server> {
        let tags = serde_json::to_string(&input.tags).unwrap_or_else(|_| "[]".into());
        self.lock().execute(
            "UPDATE servers SET name=?2,alias=?3,host=?4,port=?5,user=?6,auth_mode=?7,key_path=?8,
             secret_ref=?9,group_name=?10,tags_json=?11,updated_at=?12 WHERE id=?1",
            params![
                input.id, input.name, input.alias, input.host, input.port as i64, input.user,
                input.auth_mode, input.key_path, secret_ref, input.group_name, tags, now_ms()
            ],
        )?;
        self.get_server(&input.id)
    }

    pub fn clear_session_secrets(&self) -> AppResult<()> {
        self.lock().execute("UPDATE servers SET secret_ref=NULL WHERE secret_ref='session'", [])?;
        Ok(())
    }

    pub fn delete_server(&self, id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute("DELETE FROM alerts WHERE server_id=?1", params![id])?;
        conn.execute("DELETE FROM quarantine_entries WHERE server_id=?1", params![id])?;
        conn.execute("DELETE FROM volumes WHERE server_id=?1", params![id])?;
        conn.execute("DELETE FROM disks WHERE server_id=?1", params![id])?;
        conn.execute("DELETE FROM snapshots WHERE server_id=?1", params![id])?;
        conn.execute("DELETE FROM usage_nodes WHERE scan_id IN (SELECT id FROM usage_scans WHERE server_id=?1)", params![id])?;
        conn.execute("DELETE FROM large_files WHERE scan_id IN (SELECT id FROM usage_scans WHERE server_id=?1)", params![id])?;
        conn.execute("DELETE FROM usage_scans WHERE server_id=?1", params![id])?;
        conn.execute("DELETE FROM servers WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn set_server_status(&self, id: &str, status: &str, error: Option<&str>, seen_at: Option<i64>) -> AppResult<()> {
        self.lock().execute(
            "UPDATE servers SET last_status=?2,last_error=?3,last_seen_at=COALESCE(?4,last_seen_at),updated_at=?5 WHERE id=?1",
            params![id, status, error, seen_at, now_ms()],
        )?;
        Ok(())
    }

    pub fn apply_snapshot(
        &self,
        server_id: &str,
        snapshot: &Snapshot,
        latency_ms: i64,
        counts: &SnapshotCounts,
    ) -> AppResult<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let payload = serde_json::to_string(snapshot).map_err(|e| AppError::db(e.to_string()))?;
        tx.execute(
            "INSERT OR REPLACE INTO snapshots(id,server_id,collected_at,status,duration_ms,error,max_used_pct,payload_json)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                snapshot.id, server_id, snapshot.collected_at, snapshot.status,
                snapshot.duration_ms, snapshot.error, snapshot.max_used_pct, payload
            ],
        )?;
        tx.execute("DELETE FROM disks WHERE server_id=?1", params![server_id])?;
        for d in &snapshot.disks {
            tx.execute(
                "INSERT INTO disks(server_id,name,model,serial,size_bytes,disk_type,rotational,read_only,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![server_id, d.name, d.model, d.serial, d.size_bytes as i64, d.disk_type,
                    d.rotational as i64, d.read_only as i64, snapshot.collected_at],
            )?;
        }
        tx.execute("DELETE FROM volumes WHERE server_id=?1", params![server_id])?;
        for v in &snapshot.volumes {
            tx.execute(
                "INSERT INTO volumes(server_id,mount,device,fs_type,label,total_bytes,used_bytes,available_bytes,
                    used_pct,inode_total,inode_used,inode_pct,read_only,virtual_fs,options,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                params![
                    server_id, v.mount, v.device, v.fs_type, v.label,
                    v.total_bytes as i64, v.used_bytes as i64, v.available_bytes as i64, v.used_pct,
                    v.inode_total.map(|x| x as i64), v.inode_used.map(|x| x as i64), v.inode_pct,
                    v.read_only as i64, v.virtual_fs as i64,
                    serde_json::to_string(&v.options).unwrap_or_else(|_| "[]".into()),
                    snapshot.collected_at
                ],
            )?;
        }
        let max_used = snapshot.volumes.iter().map(|v| v.used_pct).fold(0.0_f64, f64::max);
        tx.execute(
            "UPDATE servers SET last_status='online',last_seen_at=?2,last_error=NULL,
                os=?3,distro=?4,kernel=?5,arch=?6,uptime_secs=?7,cpu_count=?8,memory_total_bytes=?9,
                load1=?10,privilege=?11,max_used_pct=?12,volume_count=?13,read_only_count=?14,
                warning_count=?15,critical_count=?16,collected_at=?17,latency_ms=?18,updated_at=?19
             WHERE id=?1",
            params![
                server_id, snapshot.collected_at, snapshot.os, snapshot.distro, snapshot.kernel,
                snapshot.arch, snapshot.uptime_secs.map(|x| x as i64), snapshot.cpu_count.map(|x| x as i64),
                snapshot.memory_total_bytes.map(|x| x as i64), snapshot.load1, snapshot.privilege,
                max_used, snapshot.volumes.len() as i64, counts.read_only as i64,
                counts.warning as i64, counts.critical as i64, snapshot.collected_at, latency_ms, now_ms()
            ],
        )?;
        let live_status = if snapshot.status == "partial" { "degraded" } else { "online" };
        tx.execute("UPDATE servers SET last_status=?2 WHERE id=?1", params![server_id, live_status])?;
        tx.commit()?;
        Ok(())
    }

    pub fn latest_snapshot(&self, server_id: &str) -> AppResult<Option<Snapshot>> {
        let conn = self.lock();
        let raw: Option<String> = conn
            .query_row(
                "SELECT payload_json FROM snapshots WHERE server_id=?1 ORDER BY collected_at DESC LIMIT 1",
                params![server_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(raw.and_then(|s| serde_json::from_str(&s).ok()))
    }

    pub fn list_snapshots(&self, server_id: &str, limit: u32) -> AppResult<Vec<Snapshot>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT payload_json FROM snapshots WHERE server_id=?1 ORDER BY collected_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![server_id, limit], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            if let Ok(v) = serde_json::from_str::<Snapshot>(&row?) {
                out.push(v);
            }
        }
        Ok(out)
    }

    pub fn list_volumes(&self, server_id: &str) -> AppResult<Vec<VolumeInfo>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT mount,device,fs_type,label,total_bytes,used_bytes,available_bytes,used_pct,
             inode_total,inode_used,inode_pct,read_only,virtual_fs,options FROM volumes WHERE server_id=?1 ORDER BY mount",
        )?;
        let rows = stmt.query_map(params![server_id], |r| {
            let options: Option<String> = r.get(13)?;
            Ok(VolumeInfo {
                mount: r.get(0)?,
                device: r.get::<_, Option<String>>(1)?.unwrap_or_default(),
                fs_type: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                label: r.get(3)?,
                total_bytes: r.get::<_, i64>(4)?.max(0) as u64,
                used_bytes: r.get::<_, i64>(5)?.max(0) as u64,
                available_bytes: r.get::<_, i64>(6)?.max(0) as u64,
                used_pct: r.get(7)?,
                inode_total: r.get::<_, Option<i64>>(8)?.map(|x| x.max(0) as u64),
                inode_used: r.get::<_, Option<i64>>(9)?.map(|x| x.max(0) as u64),
                inode_pct: r.get(10)?,
                read_only: r.get::<_, i64>(11)? != 0,
                virtual_fs: r.get::<_, i64>(12)? != 0,
                options: options.and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default(),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn list_disks(&self, server_id: &str) -> AppResult<Vec<DiskInfo>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT name,model,serial,size_bytes,disk_type,rotational,read_only FROM disks WHERE server_id=?1 ORDER BY name",
        )?;
        let rows = stmt.query_map(params![server_id], |r| {
            Ok(DiskInfo {
                name: r.get(0)?,
                model: r.get(1)?,
                serial: r.get(2)?,
                size_bytes: r.get::<_, i64>(3)?.max(0) as u64,
                disk_type: r.get(4)?,
                rotational: r.get::<_, i64>(5)? != 0,
                read_only: r.get::<_, i64>(6)? != 0,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ---------- tasks ----------
    fn row_to_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
        Ok(Task {
            id: row.get(0)?,
            kind: row.get(1)?,
            title: row.get(2)?,
            status: row.get(3)?,
            total_hosts: row.get::<_, i64>(4)?.max(0) as u64,
            succeeded: row.get::<_, i64>(5)?.max(0) as u64,
            failed: row.get::<_, i64>(6)?.max(0) as u64,
            partial: row.get::<_, i64>(7)?.max(0) as u64,
            cancelled: row.get::<_, i64>(8)?.max(0) as u64,
            concurrency: row.get::<_, i64>(9)?.max(1) as u64,
            started_at: row.get(10)?,
            finished_at: row.get(11)?,
            plan_id: row.get(12)?,
            error: row.get(13)?,
        })
    }

    pub fn create_task(&self, kind: &str, title: &str, hosts: &[(String, String)], concurrency: u64, plan_id: Option<&str>) -> AppResult<Task> {
        let id = new_id();
        let now = now_ms();
        let total = hosts.len() as i64;
        let conn = self.lock();
        conn.execute(
            "INSERT INTO tasks(id,kind,title,status,total_hosts,succeeded,failed,partial,cancelled,concurrency,started_at,plan_id)
             VALUES(?1,?2,?3,'running',?4,0,0,0,0,?5,?6,?7)",
            params![id, kind, title, total, concurrency as i64, now, plan_id],
        )?;
        for (sid, sname) in hosts {
            conn.execute(
                "INSERT INTO task_hosts(task_id,server_id,server_name,status) VALUES(?1,?2,?3,'queued')",
                params![id, sid, sname],
            )?;
        }
        drop(conn);
        self.get_task(&id).map(|d| d.task)
    }

    pub fn get_task(&self, id: &str) -> AppResult<TaskDetail> {
        let task = {
            let conn = self.lock();
            conn.query_row(
                "SELECT id,kind,title,status,total_hosts,succeeded,failed,partial,cancelled,concurrency,started_at,finished_at,plan_id,error
                 FROM tasks WHERE id=?1",
                params![id],
                Self::row_to_task,
            )
            .optional()?
            .ok_or_else(|| AppError::new("NOT_FOUND", "任务不存在"))?
        };
        let hosts = self.task_hosts(id)?;
        Ok(TaskDetail { task, hosts })
    }

    pub fn list_tasks(&self, limit: u32) -> AppResult<Vec<Task>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,kind,title,status,total_hosts,succeeded,failed,partial,cancelled,concurrency,started_at,finished_at,plan_id,error
             FROM tasks ORDER BY started_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], Self::row_to_task)?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn task_hosts(&self, task_id: &str) -> AppResult<Vec<TaskHost>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,server_id,server_name,status,started_at,finished_at,duration_ms,bytes,error_code,error,command_preview,output_excerpt,result_json
             FROM task_hosts WHERE task_id=?1 ORDER BY id",
        )?;
        let rows = stmt.query_map(params![task_id], |r| {
            let result_json: Option<String> = r.get(12)?;
            Ok(TaskHost {
                id: r.get(0)?,
                server_id: r.get(1)?,
                server_name: r.get(2)?,
                status: r.get(3)?,
                started_at: r.get(4)?,
                finished_at: r.get(5)?,
                duration_ms: r.get(6)?,
                bytes: r.get::<_, i64>(7)?.max(0) as u64,
                error_code: r.get(8)?,
                error: r.get(9)?,
                command_preview: r.get(10)?,
                output_excerpt: r.get(11)?,
                result: result_json.and_then(|s| serde_json::from_str(&s).ok()),
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn task_host(&self, task_id: &str, server_id: &str) -> AppResult<Option<TaskHost>> {
        Ok(self.task_hosts(task_id)?.into_iter().find(|h| h.server_id == server_id))
    }

    pub fn start_task_host(&self, task_id: &str, server_id: &str) -> AppResult<()> {
        self.lock().execute(
            "UPDATE task_hosts SET status='running', started_at=?3 WHERE task_id=?1 AND server_id=?2",
            params![task_id, server_id, now_ms()],
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn finish_task_host(
        &self,
        task_id: &str,
        server_id: &str,
        status: &str,
        bytes: u64,
        error_code: Option<&str>,
        error: Option<&str>,
        command_preview: Option<&str>,
        output_excerpt: Option<&str>,
        result: Option<&serde_json::Value>,
    ) -> AppResult<()> {
        let now = now_ms();
        let start: Option<i64> = self.lock().query_row(
            "SELECT started_at FROM task_hosts WHERE task_id=?1 AND server_id=?2",
            params![task_id, server_id],
            |r| r.get(0),
        ).optional()?.flatten();
        let duration = start.map(|s| (now - s).max(0));
        self.lock().execute(
            "UPDATE task_hosts SET status=?3,finished_at=?4,duration_ms=?5,bytes=?6,error_code=?7,error=?8,
             command_preview=?9,output_excerpt=?10,result_json=?11 WHERE task_id=?1 AND server_id=?2",
            params![
                task_id, server_id, status, now, duration, bytes as i64, error_code, error,
                command_preview, output_excerpt,
                result.map(|v| v.to_string())
            ],
        )?;
        self.recount_task(task_id)?;
        Ok(())
    }

    pub fn recount_task(&self, task_id: &str) -> AppResult<Task> {
        let conn = self.lock();
        let total: i64 = conn.query_row("SELECT COUNT(*) FROM task_hosts WHERE task_id=?1", params![task_id], |r| r.get(0))?;
        let count = |statuses: &str| -> AppResult<i64> {
            let sql = format!("SELECT COUNT(*) FROM task_hosts WHERE task_id=?1 AND status IN ({statuses})");
            Ok(conn.query_row(&sql, params![task_id], |r| r.get(0))?)
        };
        let running = count("'queued','running'")?;
        let succeeded = count("'succeeded'")?;
        let failed = count("'failed'")?;
        let partial = count("'partial'")?;
        let cancelled = count("'cancelled'")?;
        let interrupted = count("'interrupted'")?;
        let status = if running > 0 {
            "running"
        } else if cancelled > 0 && succeeded == 0 && failed == 0 && partial == 0 {
            "cancelled"
        } else if interrupted > 0 && succeeded == 0 && failed == 0 && partial == 0 {
            "interrupted"
        } else if succeeded == 0 && failed > 0 && partial == 0 {
            "failed"
        } else if failed > 0 || partial > 0 || interrupted > 0 || cancelled > 0 {
            "partial"
        } else if succeeded > 0 {
            "succeeded"
        } else {
            "failed"
        };
        let finished = if running == 0 { Some(now_ms()) } else { None };
        conn.execute(
            "UPDATE tasks SET status=?2,total_hosts=?3,succeeded=?4,failed=?5,partial=?6,cancelled=?7,
             finished_at=COALESCE(?8,finished_at) WHERE id=?1",
            params![task_id, status, total, succeeded, failed, partial, cancelled, finished],
        )?;
        drop(conn);
        Ok(self.get_task(task_id)?.task)
    }

    pub fn mark_task_cancelled(&self, task_id: &str) -> AppResult<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE task_hosts SET status='cancelled',finished_at=?2,
             duration_ms=CASE WHEN started_at IS NULL THEN 0 ELSE ?2-started_at END
             WHERE task_id=?1 AND status IN ('queued','running')",
            params![task_id, now_ms()],
        )?;
        conn.execute("UPDATE tasks SET status='cancelled',finished_at=?2 WHERE id=?1", params![task_id, now_ms()])?;
        drop(conn);
        let _ = self.recount_task(task_id);
        Ok(())
    }

    pub fn mark_interrupted_tasks(&self) -> AppResult<()> {
        let conn = self.lock();
        conn.execute(
            "UPDATE task_hosts SET status='interrupted' WHERE status IN ('queued','running')",
            [],
        )?;
        conn.execute(
            "UPDATE tasks SET status='interrupted',finished_at=?1 WHERE status='running'",
            params![now_ms()],
        )?;
        conn.execute(
            "UPDATE cleanup_plans SET status='interrupted' WHERE status='running'",
            [],
        )?;
        Ok(())
    }

    // ---------- usage ----------
    pub fn create_usage_scan(&self, server_id: &str, mount: &str, root_path: &str) -> AppResult<UsageScan> {
        let id = new_id();
        let now = now_ms();
        self.lock().execute(
            "INSERT INTO usage_scans(id,server_id,mount,root_path,status,started_at) VALUES(?1,?2,?3,?4,'running',?5)",
            params![id, server_id, mount, root_path, now],
        )?;
        self.get_usage_scan(&id)
    }

    pub fn get_usage_scan(&self, id: &str) -> AppResult<UsageScan> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id,server_id,mount,root_path,status,started_at,finished_at,total_bytes,entries,skipped,partial,error
             FROM usage_scans WHERE id=?1",
            params![id],
            |r| Ok(UsageScan {
                id: r.get(0)?,
                server_id: r.get(1)?,
                mount: r.get(2)?,
                root_path: r.get(3)?,
                status: r.get(4)?,
                started_at: r.get(5)?,
                finished_at: r.get(6)?,
                total_bytes: r.get::<_, i64>(7)?.max(0) as u64,
                entries: r.get::<_, i64>(8)?.max(0) as u64,
                skipped: r.get::<_, i64>(9)?.max(0) as u64,
                partial: r.get::<_, i64>(10)? != 0,
                error: r.get(11)?,
            }),
        )
        .optional()?
        .ok_or_else(|| AppError::new("NOT_FOUND", "分析任务不存在"))
    }

    pub fn latest_usage_scan(&self, server_id: &str) -> AppResult<Option<UsageScan>> {
        let conn = self.lock();
        let id: Option<String> = conn.query_row(
            "SELECT id FROM usage_scans WHERE server_id=?1 ORDER BY started_at DESC LIMIT 1",
            params![server_id],
            |r| r.get(0),
        ).optional()?;
        drop(conn);
        match id {
            Some(id) => Ok(Some(self.get_usage_scan(&id)?)),
            None => Ok(None),
        }
    }

    pub fn finish_usage_scan(&self, id: &str, status: &str, total_bytes: u64, entries: u64, skipped: u64, partial: bool, error: Option<&str>) -> AppResult<()> {
        self.lock().execute(
            "UPDATE usage_scans SET status=?2,finished_at=?3,total_bytes=?4,entries=?5,skipped=?6,partial=?7,error=?8 WHERE id=?1",
            params![id, status, now_ms(), total_bytes as i64, entries as i64, skipped as i64, partial as i64, error],
        )?;
        Ok(())
    }

    pub fn insert_usage_node(&self, scan_id: &str, parent_id: Option<i64>, name: &str, path: &str, kind: &str, size_bytes: u64, depth: i64) -> AppResult<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO usage_nodes(scan_id,parent_id,name,path,kind,size_bytes,depth) VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![scan_id, parent_id, name, path, kind, size_bytes as i64, depth],
        )?;
        let id = conn.last_insert_rowid();
        if let Some(pid) = parent_id {
            conn.execute("UPDATE usage_nodes SET child_count=child_count+1 WHERE id=?1", params![pid])?;
        }
        Ok(id)
    }

    pub fn get_usage_node(&self, id: i64) -> AppResult<UsageNode> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id,scan_id,parent_id,name,path,kind,size_bytes,depth,child_count,expanded_at FROM usage_nodes WHERE id=?1",
            params![id],
            |r| Ok(UsageNode {
                id: r.get(0)?,
                scan_id: r.get(1)?,
                parent_id: r.get(2)?,
                name: r.get(3)?,
                path: r.get(4)?,
                kind: r.get(5)?,
                size_bytes: r.get::<_, i64>(6)?.max(0) as u64,
                depth: r.get(7)?,
                child_count: r.get::<_, i64>(8)?.max(0) as u64,
                expanded_at: r.get(9)?,
            }),
        )
        .optional()?
        .ok_or_else(|| AppError::new("NOT_FOUND", "目录节点不存在"))
    }

    pub fn list_usage_children(&self, scan_id: &str, parent_id: i64) -> AppResult<Vec<UsageNode>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,scan_id,parent_id,name,path,kind,size_bytes,depth,child_count,expanded_at
             FROM usage_nodes WHERE scan_id=?1 AND parent_id=?2 ORDER BY size_bytes DESC, name",
        )?;
        let rows = stmt.query_map(params![scan_id, parent_id], |r| {
            Ok(UsageNode {
                id: r.get(0)?,
                scan_id: r.get(1)?,
                parent_id: r.get(2)?,
                name: r.get(3)?,
                path: r.get(4)?,
                kind: r.get(5)?,
                size_bytes: r.get::<_, i64>(6)?.max(0) as u64,
                depth: r.get(7)?,
                child_count: r.get::<_, i64>(8)?.max(0) as u64,
                expanded_at: r.get(9)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn root_usage_node(&self, scan_id: &str) -> AppResult<Option<UsageNode>> {
        let conn = self.lock();
        let id: Option<i64> = conn.query_row(
            "SELECT id FROM usage_nodes WHERE scan_id=?1 AND parent_id IS NULL ORDER BY id LIMIT 1",
            params![scan_id],
            |r| r.get(0),
        ).optional()?;
        drop(conn);
        match id {
            Some(id) => Ok(Some(self.get_usage_node(id)?)),
            None => Ok(None),
        }
    }

    pub fn mark_usage_node_expanded(&self, id: i64) -> AppResult<()> {
        self.lock().execute("UPDATE usage_nodes SET expanded_at=?2 WHERE id=?1", params![id, now_ms()])?;
        Ok(())
    }

    pub fn update_usage_node_size(&self, id: i64, size_bytes: u64) -> AppResult<()> {
        self.lock().execute("UPDATE usage_nodes SET size_bytes=?2 WHERE id=?1", params![id, size_bytes as i64])?;
        Ok(())
    }

    pub fn insert_large_file(&self, scan_id: &str, path: &str, size_bytes: u64, mtime: Option<i64>) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO large_files(scan_id,path,size_bytes,mtime) VALUES(?1,?2,?3,?4)",
            params![scan_id, path, size_bytes as i64, mtime],
        )?;
        Ok(())
    }

    pub fn get_large_file(&self, id: i64) -> AppResult<LargeFile> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id,scan_id,path,size_bytes,mtime FROM large_files WHERE id=?1",
            params![id],
            |r| Ok(LargeFile {
                id: r.get(0)?,
                scan_id: r.get(1)?,
                path: r.get(2)?,
                size_bytes: r.get::<_, i64>(3)?.max(0) as u64,
                mtime: r.get(4)?,
            }),
        )
        .optional()?
        .ok_or_else(|| AppError::new("NOT_FOUND", "大文件记录不存在"))
    }

    pub fn list_large_files(&self, scan_id: &str) -> AppResult<Vec<LargeFile>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,scan_id,path,size_bytes,mtime FROM large_files WHERE scan_id=?1 ORDER BY size_bytes DESC LIMIT 500",
        )?;
        let rows = stmt.query_map(params![scan_id], |r| {
            Ok(LargeFile {
                id: r.get(0)?,
                scan_id: r.get(1)?,
                path: r.get(2)?,
                size_bytes: r.get::<_, i64>(3)?.max(0) as u64,
                mtime: r.get(4)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ---------- alerts ----------
    #[allow(clippy::too_many_arguments)]
    pub fn upsert_alert(&self, server_id: &str, server_name: &str, kind: &str, severity: &str, dedupe_key: &str, title: &str, detail: &str) -> AppResult<()> {
        let conn = self.lock();
        let existing: Option<String> = conn.query_row(
            "SELECT id FROM alerts WHERE server_id=?1 AND dedupe_key=?2 AND resolved_at IS NULL LIMIT 1",
            params![server_id, dedupe_key],
            |r| r.get(0),
        ).optional()?;
        let now = now_ms();
        match existing {
            Some(id) => {
                conn.execute(
                    "UPDATE alerts SET severity=?2,title=?3,detail=?4,last_seen_at=?5 WHERE id=?1",
                    params![id, severity, title, detail, now],
                )?;
            }
            None => {
                conn.execute(
                    "INSERT INTO alerts(id,server_id,server_name,kind,severity,dedupe_key,title,detail,first_seen_at,last_seen_at)
                     VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",
                    params![new_id(), server_id, server_name, kind, severity, dedupe_key, title, detail, now],
                )?;
            }
        }
        Ok(())
    }

    pub fn resolve_stale_alerts(&self, server_id: &str, active_keys: &[String]) -> AppResult<()> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,dedupe_key FROM alerts WHERE server_id=?1 AND resolved_at IS NULL",
        )?;
        let rows = stmt.query_map(params![server_id], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut to_resolve = Vec::new();
        for row in rows {
            let (id, key) = row?;
            if !active_keys.iter().any(|k| k == &key) {
                to_resolve.push(id);
            }
        }
        drop(stmt);
        for id in to_resolve {
            conn.execute("UPDATE alerts SET resolved_at=?2 WHERE id=?1", params![id, now_ms()])?;
        }
        Ok(())
    }

    pub fn list_alerts(&self, active_only: bool, limit: u32) -> AppResult<Vec<Alert>> {
        let conn = self.lock();
        let sql = if active_only {
            "SELECT id,server_id,server_name,kind,severity,title,detail,first_seen_at,last_seen_at,resolved_at,acknowledged_at
             FROM alerts WHERE resolved_at IS NULL ORDER BY CASE severity WHEN 'critical' THEN 0 WHEN 'warning' THEN 1 ELSE 2 END, last_seen_at DESC LIMIT ?1"
        } else {
            "SELECT id,server_id,server_name,kind,severity,title,detail,first_seen_at,last_seen_at,resolved_at,acknowledged_at
             FROM alerts ORDER BY last_seen_at DESC LIMIT ?1"
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(Alert {
                id: r.get(0)?,
                server_id: r.get(1)?,
                server_name: r.get(2)?,
                kind: r.get(3)?,
                severity: r.get(4)?,
                title: r.get(5)?,
                detail: r.get(6)?,
                first_seen_at: r.get(7)?,
                last_seen_at: r.get(8)?,
                resolved_at: r.get(9)?,
                acknowledged_at: r.get(10)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn acknowledge_alert(&self, id: &str) -> AppResult<()> {
        self.lock().execute("UPDATE alerts SET acknowledged_at=?2 WHERE id=?1", params![id, now_ms()])?;
        Ok(())
    }

    pub fn resolve_alert(&self, id: &str) -> AppResult<()> {
        self.lock().execute("UPDATE alerts SET resolved_at=?2 WHERE id=?1", params![id, now_ms()])?;
        Ok(())
    }

    // ---------- known hosts ----------
    pub fn insert_known_host(&self, host: &str, port: u16, key_type: &str, fingerprint: &str, line: &str) -> AppResult<KnownHost> {
        let now = now_ms();
        let conn = self.lock();
        conn.execute(
            "INSERT OR IGNORE INTO known_hosts(host,port,key_type,fingerprint,line,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![host, port as i64, key_type, fingerprint, line, now],
        )?;
        let id = conn.query_row(
            "SELECT id FROM known_hosts WHERE host=?1 AND port=?2 AND fingerprint=?3",
            params![host, port as i64, fingerprint],
            |r| r.get(0),
        )?;
        Ok(KnownHost { id, host: host.into(), port, key_type: key_type.into(), fingerprint: fingerprint.into(), created_at: now })
    }

    pub fn list_known_hosts(&self) -> AppResult<Vec<KnownHost>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,host,port,key_type,fingerprint,created_at FROM known_hosts ORDER BY host,port,key_type",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(KnownHost {
                id: r.get(0)?,
                host: r.get(1)?,
                port: r.get::<_, i64>(2)?.max(1) as u16,
                key_type: r.get(3)?,
                fingerprint: r.get(4)?,
                created_at: r.get(5)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn remove_known_host(&self, id: i64) -> AppResult<()> {
        self.lock().execute("DELETE FROM known_hosts WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn known_host_lines(&self) -> AppResult<Vec<String>> {
        let conn = self.lock();
        let mut stmt = conn.prepare("SELECT line FROM known_hosts ORDER BY id")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    // ---------- cleanup ----------
    pub fn create_cleanup_plan(&self, token: &str, expires_at: i64, items: &[NewCleanupItem]) -> AppResult<CleanupPlan> {
        let id = new_id();
        let now = now_ms();
        let total_bytes: u64 = items.iter().map(|i| i.size_bytes).sum();
        let irreversible = items.iter().filter(|i| !i.reversible).count() as u64;
        let hosts: std::collections::BTreeSet<&str> = items.iter().map(|i| i.server_id.as_str()).collect();
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO cleanup_plans(id,token,status,created_at,expires_at,host_count,item_count,total_bytes,irreversible)
             VALUES(?1,?2,'planned',?3,?4,?5,?6,?7,?8)",
            params![id, token, now, expires_at, hosts.len() as i64, items.len() as i64, total_bytes as i64, irreversible as i64],
        )?;
        for item in items {
            tx.execute(
                "INSERT INTO cleanup_plan_items(plan_id,server_id,server_name,kind,template_id,path,display_path,quarantine_path,size_bytes,
                    risk,reversible,command_preview,default_selected,selected,status)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,1,'planned')",
                params![
                    id, item.server_id, item.server_name, item.kind, item.template_id, item.path,
                    item.display_path, item.quarantine_path, item.size_bytes as i64, item.risk,
                    item.reversible as i64, item.command_preview, item.default_selected as i64
                ],
            )?;
        }
        tx.commit()?;
        drop(conn);
        self.get_cleanup_plan(&id)
    }

    pub fn get_cleanup_plan(&self, id: &str) -> AppResult<CleanupPlan> {
        let conn = self.lock();
        let (token, status, created_at, expires_at, host_count, item_count, total_bytes, irreversible) = conn.query_row(
            "SELECT token,status,created_at,expires_at,host_count,item_count,total_bytes,irreversible FROM cleanup_plans WHERE id=?1",
            params![id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?, r.get::<_, i64>(5)?, r.get::<_, i64>(6)?, r.get::<_, i64>(7)?)),
        ).optional()?.ok_or_else(|| AppError::new("NOT_FOUND", "清理计划不存在"))?;
        let mut stmt = conn.prepare(
            "SELECT id,plan_id,server_id,server_name,kind,template_id,path,display_path,quarantine_path,size_bytes,risk,reversible,
             command_preview,default_selected,selected,status,error FROM cleanup_plan_items WHERE plan_id=?1 ORDER BY server_name,id",
        )?;
        let rows = stmt.query_map(params![id], |r| {
            Ok(CleanupPlanItem {
                id: r.get(0)?,
                plan_id: r.get(1)?,
                server_id: r.get(2)?,
                server_name: r.get(3)?,
                kind: r.get(4)?,
                template_id: r.get(5)?,
                path: r.get(6)?,
                display_path: r.get(7)?,
                quarantine_path: r.get(8)?,
                size_bytes: r.get::<_, i64>(9)?.max(0) as u64,
                risk: r.get(10)?,
                reversible: r.get::<_, i64>(11)? != 0,
                command_preview: r.get(12)?,
                default_selected: r.get::<_, i64>(13)? != 0,
                selected: r.get::<_, i64>(14)? != 0,
                status: r.get(15)?,
                error: r.get(16)?,
            })
        })?;
        let mut items = Vec::new();
        for row in rows {
            items.push(row?);
        }
        Ok(CleanupPlan {
            id: id.into(),
            token,
            status,
            created_at,
            expires_at,
            host_count: host_count.max(0) as u64,
            item_count: item_count.max(0) as u64,
            total_bytes: total_bytes.max(0) as u64,
            irreversible: irreversible.max(0) as u64,
            items,
        })
    }

    pub fn get_cleanup_plan_for_execution(&self, id: &str, token: &str) -> AppResult<CleanupPlan> {
        let plan = self.get_cleanup_plan(id)?;
        if plan.token != token {
            return Err(AppError::new("PLAN_TOKEN_INVALID", "清理令牌无效，请重新生成计划"));
        }
        if plan.expires_at < now_ms() {
            return Err(AppError::new("PLAN_EXPIRED", "清理计划已过期，请重新生成"));
        }
        if plan.status != "planned" {
            return Err(AppError::new("PLAN_USED", "该清理计划已执行或已取消"));
        }
        Ok(plan)
    }

    pub fn set_cleanup_plan_status(&self, id: &str, status: &str) -> AppResult<()> {
        self.lock().execute("UPDATE cleanup_plans SET status=?2 WHERE id=?1", params![id, status])?;
        Ok(())
    }

    pub fn set_cleanup_item_status(&self, id: i64, status: &str, error: Option<&str>) -> AppResult<()> {
        self.lock().execute(
            "UPDATE cleanup_plan_items SET status=?2,error=?3 WHERE id=?1",
            params![id, status, error],
        )?;
        Ok(())
    }

    pub fn get_cleanup_plan_items(&self, plan_id: &str) -> AppResult<Vec<CleanupPlanItem>> {
        Ok(self.get_cleanup_plan(plan_id)?.items)
    }

    pub fn insert_quarantine(&self, entry: &QuarantineEntry) -> AppResult<i64> {
        let conn = self.lock();
        conn.execute(
            "INSERT INTO quarantine_entries(server_id,server_name,plan_item_id,original_path,quarantine_path,size_bytes,
                fingerprint,created_at,expires_at,status)
             VALUES(?1,?2,NULL,?3,?4,?5,NULL,?6,?7,'stored')",
            params![
                entry.server_id, entry.server_name, entry.original_path, entry.quarantine_path,
                entry.size_bytes as i64, entry.created_at, entry.expires_at
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_quarantine(&self, server_id: Option<&str>, limit: u32) -> AppResult<Vec<QuarantineEntry>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT id,server_id,server_name,original_path,quarantine_path,size_bytes,created_at,expires_at,status
             FROM quarantine_entries WHERE (?1 IS NULL OR server_id=?1) ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![server_id, limit], |r| {
            Ok(QuarantineEntry {
                id: r.get(0)?,
                server_id: r.get(1)?,
                server_name: r.get(2)?,
                original_path: r.get(3)?,
                quarantine_path: r.get(4)?,
                size_bytes: r.get::<_, i64>(5)?.max(0) as u64,
                created_at: r.get(6)?,
                expires_at: r.get(7)?,
                status: r.get(8)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    pub fn get_quarantine(&self, id: i64) -> AppResult<QuarantineEntry> {
        let conn = self.lock();
        conn.query_row(
            "SELECT id,server_id,server_name,original_path,quarantine_path,size_bytes,created_at,expires_at,status
             FROM quarantine_entries WHERE id=?1",
            params![id],
            |r| Ok(QuarantineEntry {
                id: r.get(0)?,
                server_id: r.get(1)?,
                server_name: r.get(2)?,
                original_path: r.get(3)?,
                quarantine_path: r.get(4)?,
                size_bytes: r.get::<_, i64>(5)?.max(0) as u64,
                created_at: r.get(6)?,
                expires_at: r.get(7)?,
                status: r.get(8)?,
            }),
        )
        .optional()?
        .ok_or_else(|| AppError::new("NOT_FOUND", "隔离记录不存在"))
    }

    pub fn update_quarantine_status(&self, id: i64, status: &str) -> AppResult<()> {
        let now = now_ms();
        self.lock().execute(
            "UPDATE quarantine_entries SET status=?2,
                restored_at=CASE WHEN ?2='restored' THEN ?3 ELSE restored_at END,
                purged_at=CASE WHEN ?2='purged' THEN ?3 ELSE purged_at END
             WHERE id=?1",
            params![id, status, now],
        )?;
        Ok(())
    }

    // ---------- audit ----------
    pub fn insert_audit(&self, kind: &str, server_id: Option<&str>, task_id: Option<&str>, summary: &str, detail: Option<&serde_json::Value>) -> AppResult<()> {
        self.lock().execute(
            "INSERT INTO audit(at,kind,server_id,task_id,summary,detail_json) VALUES(?1,?2,?3,?4,?5,?6)",
            params![now_ms(), kind, server_id, task_id, summary, detail.map(|v| v.to_string())],
        )?;
        Ok(())
    }

    pub fn list_audit(&self, limit: u32) -> AppResult<Vec<serde_json::Value>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT at,kind,server_id,task_id,summary,detail_json FROM audit ORDER BY at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            let detail: Option<String> = r.get(5)?;
            Ok(serde_json::json!({
                "at": r.get::<_, i64>(0)?,
                "kind": r.get::<_, String>(1)?,
                "serverId": r.get::<_, Option<String>>(2)?,
                "taskId": r.get::<_, Option<String>>(3)?,
                "summary": r.get::<_, String>(4)?,
                "detail": detail.and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok()),
            }))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deletes_server_record() {
        let dir = std::env::temp_dir().join(format!("nova-rack-db-test-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        {
            let db = Db::open(dir.join("test.sqlite3")).expect("open db");
            let input = ServerInput {
                name: "delete-me".into(),
                alias: None,
                host: "192.0.2.1".into(),
                port: 22,
                user: "root".into(),
                auth_mode: "agent".into(),
                key_path: None,
                group_name: None,
                tags: vec![],
                secret: None,
                remember_secret: false,
            };
            let server = db.create_server(&input, None).expect("create server");
            assert_eq!(db.list_servers().expect("list").len(), 1);
            db.delete_server(&server.id).expect("delete server");
            assert!(db.list_servers().expect("list after delete").is_empty());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}
