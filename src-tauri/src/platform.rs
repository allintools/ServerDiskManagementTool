use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Build a child command with platform-appropriate window handling.
///
/// The app is a GUI process on Windows; without `CREATE_NO_WINDOW` every
/// short-lived ssh helper can flash a console window.
pub fn command(program: impl AsRef<OsStr>) -> Command {
    let mut command = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

#[cfg(unix)]
pub fn set_private_dir(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
pub fn set_private_dir(_path: &Path) {}

#[cfg(unix)]
pub fn set_private_file(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
}

#[cfg(not(unix))]
pub fn set_private_file(_path: &Path) {}

#[cfg(unix)]
pub fn set_private_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700));
}

#[cfg(not(unix))]
pub fn set_private_executable(_path: &Path) {}

/// Expand `~`, and on Windows `%VAR%`, before resolving a local path.
pub fn expand_home_path(input: &str) -> PathBuf {
    let value = input.trim();
    let home = dirs::home_dir();
    let expanded = if value == "~" {
        home.clone().unwrap_or_else(|| PathBuf::from("~"))
    } else if let Some(rest) = value
        .strip_prefix("~/")
        .or_else(|| value.strip_prefix("~\\"))
    {
        home.map(|dir| dir.join(rest)).unwrap_or_else(|| PathBuf::from(value))
    } else {
        PathBuf::from(value)
    };

    #[cfg(windows)]
    {
        PathBuf::from(expand_percent_vars(&expanded.to_string_lossy()))
    }
    #[cfg(not(windows))]
    {
        expanded
    }
}

#[cfg(windows)]
fn expand_percent_vars(input: &str) -> String {
    let mut out = String::new();
    let mut rest = input;
    while let Some(start) = rest.find('%') {
        if let Some(rel_end) = rest[start + 1..].find('%') {
            let end = start + 1 + rel_end;
            let name = &rest[start + 1..end];
            if !name.is_empty() {
                if let Ok(value) = std::env::var(name) {
                    out.push_str(&rest[..start]);
                    out.push_str(&value);
                    rest = &rest[end + 1..];
                    continue;
                }
            }
        }
        out.push_str(&rest[..start + 1]);
        rest = &rest[start + 1..];
    }
    out.push_str(rest);
    out
}

/// Resolve a user-entered local path. Relative paths are resolved against the
/// current directory; existing paths are canonicalized and returned without
/// the Windows verbatim prefix.
pub fn normalize_path(input: &str, must_exist: bool) -> Result<PathBuf, String> {
    let expanded = expand_home_path(input);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir()
            .map_err(|e| format!("无法解析当前目录: {e}"))?
            .join(expanded)
    };
    if absolute.exists() {
        return std::fs::canonicalize(&absolute)
            .map(strip_verbatim_prefix)
            .map_err(|e| format!("无法解析路径 {}: {e}", absolute.display()));
    }
    if must_exist {
        return Err(format!("路径不存在: {}", absolute.display()));
    }
    Ok(strip_verbatim_prefix(absolute))
}

#[cfg(windows)]
pub fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{rest}"));
    }
    if let Some(rest) = text.strip_prefix(r"\\?\") {
        return PathBuf::from(rest);
    }
    path
}

#[cfg(not(windows))]
pub fn strip_verbatim_prefix(path: PathBuf) -> PathBuf {
    path
}

/// Format a path for OpenSSH's `UserKnownHostsFile=` option. OpenSSH parses
/// this as a config value, so backslashes and double quotes need escaping.
pub fn quote_ssh_config_path(path: &Path) -> String {
    let raw = path
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    format!("\"{raw}\"")
}

/// Replace `destination` with `temporary`, falling back to copy+remove when
/// Windows refuses to rename over an existing file.
pub fn replace_file(temporary: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::fs::rename(temporary, destination)
    }
    #[cfg(windows)]
    {
        match std::fs::rename(temporary, destination) {
            Ok(()) => Ok(()),
            Err(_) => {
                let _ = std::fs::remove_file(destination);
                if std::fs::rename(temporary, destination).is_ok() {
                    return Ok(());
                }
                std::fs::copy(temporary, destination)?;
                std::fs::remove_file(temporary)?;
                Ok(())
            }
        }
    }
}

/// Name of the OpenSSH agent endpoint, when one is available.
pub fn agent_socket() -> Option<String> {
    #[cfg(windows)]
    {
        if let Ok(socket) = std::env::var("SSH_AUTH_SOCK") {
            if socket.trim().starts_with(r"\\.\pipe\") {
                return Some(socket);
            }
        }
        return Some(r"\\.\pipe\openssh-ssh-agent".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(socket) = std::env::var("SSH_AUTH_SOCK") {
            if !socket.trim().is_empty() {
                return Some(socket);
            }
        }
        let output = command("launchctl")
            .arg("getenv")
            .arg("SSH_AUTH_SOCK")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var("SSH_AUTH_SOCK")
            .ok()
            .filter(|socket| !socket.trim().is_empty())
    }

    #[cfg(not(any(unix, windows)))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_known_hosts_paths_for_ssh_config() {
        assert_eq!(
            quote_ssh_config_path(Path::new(r"C:\Users\A B\AppData\known_hosts")),
            r#""C:\\Users\\A B\\AppData\\known_hosts""#
        );
        assert_eq!(
            quote_ssh_config_path(Path::new(r#"/tmp/a "b"/known_hosts"#)),
            r#""/tmp/a \"b\"/known_hosts""#
        );
    }

    #[test]
    fn expands_tilde_to_home() {
        let home = dirs::home_dir().expect("home directory");
        assert_eq!(expand_home_path("~"), home);
        assert_eq!(expand_home_path("~/.ssh/id_ed25519"), home.join(".ssh/id_ed25519"));
        assert_eq!(expand_home_path("~\\ssh\\id_ed25519"), home.join("ssh\\id_ed25519"));
    }

    #[test]
    fn absolute_paths_are_unchanged() {
        #[cfg(unix)]
        let path = "/tmp/nova-rack-test";
        #[cfg(windows)]
        let path = r"C:\nova-rack-test";
        assert_eq!(expand_home_path(path), PathBuf::from(path));
    }

    #[test]
    fn replaces_existing_file() {
        let dir = std::env::temp_dir().join(format!("nova-rack-platform-{}", crate::util::new_id()));
        std::fs::create_dir_all(&dir).expect("temp directory");
        let target = dir.join("known_hosts");
        let temporary = dir.join("known_hosts.tmp");
        std::fs::write(&target, b"old").expect("old file");
        std::fs::write(&temporary, b"new").expect("new file");
        replace_file(&temporary, &target).expect("replace");
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "new");
        assert!(!temporary.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
