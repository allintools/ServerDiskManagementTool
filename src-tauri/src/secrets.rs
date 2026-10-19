use crate::errors::{AppError, AppResult};

const SERVICE: &str = "com.novarack.desktop";

fn account(server_id: &str) -> String {
    format!("server:{server_id}")
}

#[cfg(any(target_os = "macos", windows))]
mod platform_impl {
    use super::{account, AppError, AppResult, SERVICE};

    fn entry(server_id: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, &account(server_id))
            .map_err(|e| AppError::new("KEYCHAIN_DENIED", format!("无法访问系统凭据存储: {e}")))
    }

    pub fn store_secret(server_id: &str, secret: &str) -> AppResult<String> {
        if secret.is_empty() {
            return Err(AppError::invalid("凭据不能为空"));
        }
        entry(server_id)?
            .set_password(secret)
            .map_err(|e| AppError::new("KEYCHAIN_DENIED", format!("写入系统凭据存储失败: {e}")))?;
        Ok(format!("keychain:{SERVICE}:{}", account(server_id)))
    }

    pub fn get_secret(server_id: &str) -> Option<String> {
        entry(server_id).ok()?.get_password().ok()
    }

    pub fn delete_secret(server_id: &str) -> AppResult<()> {
        if let Ok(entry) = entry(server_id) {
            let _ = entry.delete_credential();
        }
        Ok(())
    }

    pub fn has_secret(server_id: &str) -> bool {
        get_secret(server_id).is_some()
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod platform_impl {
    use super::{AppError, AppResult};

    pub fn store_secret(_server_id: &str, _secret: &str) -> AppResult<String> {
        Err(AppError::new(
            "KEYCHAIN_UNSUPPORTED",
            "当前平台不支持系统凭据存储",
        ))
    }

    pub fn get_secret(_server_id: &str) -> Option<String> {
        None
    }

    pub fn delete_secret(_server_id: &str) -> AppResult<()> {
        Ok(())
    }

    pub fn has_secret(_server_id: &str) -> bool {
        false
    }
}

pub use platform_impl::*;

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn windows_credential_round_trip() {
        let server_id = format!("selftest-{}", crate::util::new_id());
        let secret = "nova-rack-selftest-secret";
        let _ = delete_secret(&server_id);
        let reference = store_secret(&server_id, secret).expect("store credential");
        assert!(reference.starts_with("keychain:"));
        assert_eq!(get_secret(&server_id).as_deref(), Some(secret));
        delete_secret(&server_id).expect("delete credential");
        assert!(get_secret(&server_id).is_none());
    }
}
