use crate::secrets;
use crate::ssh;
use crate::storage::Db;
use crate::types::Settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

pub struct AppState {
    pub db: Db,
    pub data_dir: PathBuf,
    pub known_hosts_path: PathBuf,
    pub settings: Mutex<Settings>,
    pub tasks: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub scans: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pending_trusts: Mutex<HashMap<String, ssh::PendingTrust>>,
    pub ephemeral_secrets: Mutex<HashMap<String, String>>,
    pub started_at: i64,
}

impl AppState {
    pub fn new(db: Db, data_dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&data_dir);
        crate::platform::set_private_dir(&data_dir);
        let known_hosts_path = data_dir.join("known_hosts");
        let _ = ssh::ensure_known_hosts(&known_hosts_path);
        ssh::cleanup_stale_askpass();
        let _ = db.mark_interrupted_tasks();
        let _ = db.clear_session_secrets();
        let settings = db.load_settings();
        Self {
            db,
            data_dir,
            known_hosts_path,
            settings: Mutex::new(settings),
            tasks: Mutex::new(HashMap::new()),
            scans: Mutex::new(HashMap::new()),
            pending_trusts: Mutex::new(HashMap::new()),
            ephemeral_secrets: Mutex::new(HashMap::new()),
            started_at: crate::util::now_ms(),
        }
    }

    pub fn settings(&self) -> Settings {
        self.settings.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn secret_for(&self, server_id: &str) -> Option<String> {
        secrets::get_secret(server_id)
            .or_else(|| self.ephemeral_secrets.lock().unwrap().get(server_id).cloned())
    }
}
