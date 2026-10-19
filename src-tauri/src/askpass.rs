use std::io::{self, Write};

pub const ASKPASS_FLAG: &str = "NOVA_RACK_ASKPASS";
pub const ASKPASS_SECRET_ENV: &str = "NOVA_RACK_ASKPASS_SECRET";

/// Windows OpenSSH cannot execute the shell-script askpass used on macOS.
/// `make_askpass` points `SSH_ASKPASS` at the app executable and sets this
/// flag; `main` handles the flag before Tauri starts and prints the secret.
pub fn askpass_main() -> Option<i32> {
    if std::env::var_os(ASKPASS_FLAG).as_deref() != Some(std::ffi::OsStr::new("1")) {
        return None;
    }
    let Some(secret_path) = std::env::var_os(ASKPASS_SECRET_ENV) else {
        return Some(1);
    };
    let secret = match std::fs::read(&secret_path) {
        Ok(secret) => secret,
        Err(_) => return Some(1),
    };
    let mut stdout = io::stdout().lock();
    let mut result = stdout.write_all(&secret);
    if result.is_ok() && !secret.ends_with(b"\n") {
        result = stdout.write_all(b"\n");
    }
    if result.is_ok() {
        result = stdout.flush();
    }
    if result.is_ok() {
        Some(0)
    } else {
        Some(1)
    }
}
