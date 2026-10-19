use crate::errors::{AppError, AppResult};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

#[derive(Debug, Clone)]
pub struct SshTools {
    pub ssh: PathBuf,
    pub keyscan: Option<PathBuf>,
    pub keygen: Option<PathBuf>,
    pub add: Option<PathBuf>,
}

impl SshTools {
    pub fn keyscan(&self) -> AppResult<&Path> {
        self.keyscan.as_deref().ok_or_else(|| {
            AppError::new(
                "SSH_TOOL_MISSING",
                "未找到 ssh-keyscan，请安装 OpenSSH Client 或通过 NOVA_RACK_SSH 指定路径",
            )
        })
    }

    pub fn keygen(&self) -> AppResult<&Path> {
        self.keygen.as_deref().ok_or_else(|| {
            AppError::new(
                "SSH_TOOL_MISSING",
                "未找到 ssh-keygen，请安装 OpenSSH Client 或通过 NOVA_RACK_SSH 指定路径",
            )
        })
    }
}

static TOOLS: OnceLock<Result<SshTools, AppError>> = OnceLock::new();

pub fn toolchain() -> AppResult<&'static SshTools> {
    TOOLS
        .get_or_init(detect_owned)
        .as_ref()
        .map_err(|error| error.clone())
}

fn detect_owned() -> Result<SshTools, AppError> {
    if let Some(raw) = std::env::var_os("NOVA_RACK_SSH") {
        let ssh = PathBuf::from(raw);
        if !ssh.is_file() {
            return Err(AppError::new(
                "SSH_NOT_FOUND",
                format!("NOVA_RACK_SSH 指向的文件不存在: {}", ssh.display()),
            ));
        }
        return Ok(from_ssh_path(ssh));
    }

    for candidate in candidate_paths() {
        if candidate.is_file() {
            return Ok(from_ssh_path(candidate));
        }
    }

    Err(AppError::new(
        "SSH_NOT_FOUND",
        "未找到 OpenSSH。请安装 Windows 可选功能 OpenSSH Client（或 macOS 的 /usr/bin/ssh），\
         并确认 ssh 在 PATH 中；也可用 NOVA_RACK_SSH 指向 ssh.exe",
    ))
}

fn candidate_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    #[cfg(windows)]
    {
        if let Some(windir) = std::env::var_os("WINDIR") {
            candidates.push(
                PathBuf::from(windir)
                    .join("System32")
                    .join("OpenSSH")
                    .join("ssh.exe"),
            );
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles") {
            candidates.push(PathBuf::from(program_files).join("OpenSSH").join("ssh.exe"));
        }
        if let Some(program_files) = std::env::var_os("ProgramFiles(x86)") {
            candidates.push(PathBuf::from(program_files).join("OpenSSH").join("ssh.exe"));
        }
    }

    #[cfg(not(windows))]
    {
        candidates.push(PathBuf::from("/usr/bin/ssh"));
        candidates.push(PathBuf::from("/usr/local/bin/ssh"));
        candidates.push(PathBuf::from("/opt/homebrew/bin/ssh"));
    }

    if let Some(found) = find_in_path(&["ssh.exe", "ssh"]) {
        candidates.push(found);
    }
    candidates
}

fn from_ssh_path(ssh: PathBuf) -> SshTools {
    let directory = ssh.parent().map(Path::to_path_buf);
    let find = |name: &str| -> Option<PathBuf> {
        if let Some(directory) = &directory {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        find_in_path(&[name])
    };

    SshTools {
        ssh,
        keyscan: find(if cfg!(windows) { "ssh-keyscan.exe" } else { "ssh-keyscan" }),
        keygen: find(if cfg!(windows) { "ssh-keygen.exe" } else { "ssh-keygen" }),
        add: find(if cfg!(windows) { "ssh-add.exe" } else { "ssh-add" }),
    }
}

fn find_in_path(names: &[&str]) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&path) {
        for name in names {
            let candidate = directory.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolchain_detection_does_not_panic() {
        let _ = toolchain();
    }

    #[test]
    fn path_search_handles_missing_path_entries() {
        let _ = find_in_path(&["definitely-not-a-real-nova-rack-tool"]);
    }
}
