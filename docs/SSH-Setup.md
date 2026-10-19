# SSH 与 sudo 配置

## 本机要求

- macOS 13+：系统 OpenSSH `/usr/bin/ssh`；密码或加密私钥口令模式需要 OpenSSH ≥ 8.4。
- Windows 10 1809+ / 11 x64：安装 Windows 可选功能 **OpenSSH Client**，并确认 `ssh -V` 在 PowerShell 中可用。应用优先使用 `%WINDIR%\System32\OpenSSH\ssh.exe`，也支持 `PATH` 中的 OpenSSH，或通过 `NOVA_RACK_SSH` 指定路径。
- 远端 sshd 允许公钥认证；如使用密码，需允许 password 或 keyboard-interactive（应用显式禁用其它认证方式）。

## 推荐：密钥 + agent

### macOS

1. 生成密钥（如尚未生成）：

```sh
ssh-keygen -t ed25519 -C "nova-rack" -f ~/.ssh/id_ed25519
ssh-copy-id -i ~/.ssh/id_ed25519.pub deploy@server
```

2. 在本机启动 agent 并加入密钥：

```sh
eval "$(ssh-agent -s)"
ssh-add --apple-use-keychain ~/.ssh/id_ed25519
```

3. 在“服务器硬盘管理工具”中添加服务器，认证方式选 **SSH agent**；或选 **指定私钥** 并填写 `~/.ssh/id_ed25519`。

macOS 从 Finder 启动的 GUI 可能拿不到终端里的 `SSH_AUTH_SOCK`。应用会尝试 `launchctl getenv SSH_AUTH_SOCK`；仍不可用时，使用“指定私钥 + 口令存钥匙串”模式。

### Windows

1. 以管理员 PowerShell 安装 OpenSSH Client（如尚未安装）：

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Client~~~~0.0.1.0
ssh -V
```

2. 启动并启用 agent 服务（可选，仅 agent 认证需要）：

```powershell
Get-Service ssh-agent
Set-Service -Name ssh-agent -StartupType Automatic
Start-Service ssh-agent
ssh-add $env:USERPROFILE\.ssh\id_ed25519
ssh-add -l
```

3. 应用会使用命名管道 `\\.\pipe\openssh-ssh-agent`；设置页会显示 agent socket 和 `ssh-add -l` 的错误信息。服务未启动时，agent 认证失败，但私钥/密码模式仍可用。

## 加密私钥

- 在添加服务器时选择“指定私钥”，填写路径与口令；口令可保存到系统凭据存储，或仅本次会话。
- 路径支持 `~`、`%USERPROFILE%` 和带空格的绝对路径；应用会在保存前展开并规范化。
- macOS 通过临时 `SSH_ASKPASS` 脚本提供口令；Windows 通过应用自身的 askpass 分支提供口令，不把秘密放进命令行参数。

## 密码认证

- 添加服务器时选择“密码”，填写 SSH 密码。
- 推荐勾选保存到系统凭据存储；不保存则只在本次应用会话内有效，重启后需要重新输入。
- 密码认证不会自动重试，避免触发服务器锁定策略。
- Windows 依赖 OpenSSH 的 `SSH_ASKPASS` 支持（Windows OpenSSH 8.4+；实测 9.5p1）。若企业版 OpenSSH 禁用了 askpass，请改用密钥/agent。

## 从 SSH 配置导入

- 支持 `~/.ssh/config`（Windows 为 `%USERPROFILE%\.ssh\config`）中具体 `Host` 别名；导入时使用 `ssh -G` 解析主机、用户、端口用于展示。
- 连接时直接使用别名，因此保留 `ProxyJump`、`IdentityFile`、`ProxyCommand` 等配置。
- 不修改用户的 SSH 配置，也不复制私钥。
- 应用使用自己的 `known_hosts`；导入不会自动信任主机密钥，首次连接仍需确认指纹。

## sudo 配置

只读巡检不需要 sudo。空间回收的系统级模板需要 **免密 sudo**：

```
# /etc/sudoers.d/nova-rack
# 仅为示例；请根据发行版与实际命令路径收紧。
rack ALL=(ALL) NOPASSWD: /usr/bin/apt-get clean, /usr/bin/dnf clean all, /usr/bin/yum clean all, /usr/bin/journalctl --vacuum-time=7d
```

不建议生产环境直接授予 `ALL=(ALL) NOPASSWD:ALL`。应用不会请求 sudo 密码，也不会保存它。

## 主机密钥

1. 首次连接时应用扫描主机密钥并展示 SHA256 指纹。
2. 通过控制台/服务商面板带外核对。
3. 确认后写入应用私有 `known_hosts`。
4. 如果服务器更换密钥，应用会阻断连接；到“设置 → SSH 与主机密钥”删除旧记录，核对新指纹后重新连接。

Windows 下 `known_hosts` 位于 `%LOCALAPPDATA%\com.novarack.desktop\known_hosts`；路径中的空格和反斜杠已做 OpenSSH 配置转义。

## 常见错误

| 错误 | 处理 |
| --- | --- |
| `SSH_NOT_FOUND` | 安装 OpenSSH Client，确认 `ssh -V` 可用，或设置 `NOVA_RACK_SSH` 指向 ssh.exe |
| `AUTH_FAILED` | 检查 agent 是否加载、私钥路径/权限、密码；macOS GUI 可能拿不到 agent，Windows 检查 ssh-agent 服务 |
| `HOST_KEY_CHANGED` | 不要直接信任；先确认服务器是否重装/轮换密钥，删除旧信任后再确认 |
| `HOST_UNREACHABLE` | 检查网络、端口、防火墙、VPN/跳板机 |
| `KEY_MISSING` | 私钥路径不存在或不可读；私钥权限应为 0600（Windows 检查当前用户 ACL） |
| `KEYCHAIN_DENIED` | 允许系统凭据存储访问，或不保存口令仅本次使用 |
| `KEYCHAIN_UNSUPPORTED` | 当前平台不支持凭据存储；仅在非 macOS/Windows 构建中出现 |
| `COMMAND_TIMEOUT` | 目录过大或网络慢；缩小分析范围、提高超时或重试 |
| 部分结果 | 无权限读取部分目录；如需完整分析，使用有读权限的用户或免密 sudo |

## 环境变量（测试/企业覆盖）

- `NOVA_RACK_SSH`：指定 `ssh`/`ssh.exe` 路径，应用会自动查找同目录的 `ssh-keyscan`、`ssh-keygen`、`ssh-add`。
- `RACK_DATA_DIR`：覆盖应用数据目录（隔离测试数据）。
- `NOVA_RACK_ASKPASS` / `NOVA_RACK_ASKPASS_SECRET`：应用内部 askpass 使用，不建议手工设置。
