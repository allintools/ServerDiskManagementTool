# 服务器硬盘管理工具 测试记录

日期：2026-10-17  
构建：`服务器硬盘管理工具 0.1.0` · macOS arm64 · Tauri 2.12.4  
环境：macOS 27.0.1（26A434）arm64；Node.js 24.19.0；Rust 1.99.1；OpenSSH 10.3p1；Docker 29.7.2；Playwright 1.64（Chromium）。

## 结果概览

| 项目 | 结果 |
| --- | --- |
| 前端类型检查 + 生产构建 | 通过（vue-tsc + Vite，1608 modules） |
| Rust 单元测试 | **18 passed / 0 failed** |
| Playwright 演示流程 | **5 passed / 0 failed** |
| Docker SSH 集成 selftest | 通过（密钥/密码/加密密钥、主机密钥、批量、分析、回收、隔离、永久删除、指标） |
| 原生 UI/IPC smoke（Docker key） | 通过；6 页、添加对话框、真实 SSH、Treemap、4 标签、机群行 |
| 原生 UI/IPC smoke（真实 Linux 主机，密码认证） | 通过；真实容量/挂载/目录分析/指标 |
| 从 zip 解压启动 | 通过；应用窗口与 Tauri IPC 正常 |
| 最终 `.app` | 约 18 MB；zip 约 4.3 MB |

## 自动化测试命令

```sh
# 在仓库根目录执行
npm run check:frontend
npm run build
cd src-tauri && cargo test
cd .. && npx playwright test
# Docker 集成（需要 Docker 与 debug 二进制）
cd src-tauri && cargo build
cd .. && bash tests/ssh/run.sh
```

## Rust 单元测试（18）

覆盖：

- SSH 参数单引号转义（包含注入尝试）。
- known_hosts 主机/端口格式与 Windows 路径转义。
- `df`/`dfi`/mounts 解析，含挂载路径空格。
- `lsblk` JSON、磁盘类型、只读/旋转位。
- `du -0` NUL 输出与 BusyBox KiB 回退。
- 缺失结束标记 → 部分结果。
- 两段 `/proc` 采样 → CPU/内存/磁盘速率。
- 路径 basename/根目录、`~` 展开、known_hosts 原子替换。
- SSH 工具链检测、Windows 凭据管理器 round-trip、服务器记录删除。

结果：`cargo test` 18 passed。

## Playwright 演示流程（5）

| 用例 | 结果 |
| --- | --- |
| 机群总览 → 服务器详情 → 磁盘与卷 → 空间分析 | 通过 |
| 添加服务器对话框 → 保存 → 列表出现 | 通过 |
| 服务器列表删除添加的服务器 → 列表移除 | 通过 |
| 目录分析 → 回收候选 → 计划 → 确认执行 | 通过 |
| 设置页 SSH 诊断、known_hosts、保存阈值 | 通过 |

演示流程使用 `demoApi`，不需要桌面窗口；生产 Tauri 构建不会回退到演示数据。

## Docker SSH 集成

夹具：本地 Debian 基础镜像 + OpenSSH，用户 `rack` 带免密 sudo；端口 `127.0.0.1:2222`；bind mount `/data` 含 120 MB 文件、2 MB 删除文件、日志和带空格/中文目录。

### 认证与主机密钥

| 检查 | 结果 |
| --- | --- |
| 首次连接未知主机 | `host_key_unknown`，展示指纹，等待确认 |
| 信任后连接 | `online` |
| 密码认证 | `online`（askpass 临时文件） |
| 加密私钥 + 口令 | `online`（askpass 临时文件） |
| 错误 known_hosts | `host_key_changed`（阻断） |

### 批量与只读

| 检查 | 结果 |
| --- | --- |
| 批量巡检：1 在线 + 1 离线 | 任务 `partial`；1 成功、1 失败；在线主机继续完成 |
| Probe | 卷、物理盘、能力矩阵解析通过 |
| 目录分析 `/data` | `succeeded`；4 项、3 个子节点 |
| 大文件扫描 | 2 个（120 MB + 2 MB） |
| 回收建议 | 检测到 `apt_cache` |
| 实时指标 | CPU/内存/3 个磁盘采样成功 |

### 空间回收

| 检查 | 结果 |
| --- | --- |
| APT 缓存模板 + 文件隔离计划 | 计划生成、令牌校验、执行成功 |
| 文件隔离 | 120 MB 文件移动到 `/data/.nova-rack-quarantine/...`；可恢复 |
| 目录隔离 | `/data/logs` 目录隔离并恢复；目录预检不比较 stat entry size |
| 隔离恢复 | `restore` 成功，原路径恢复 |
| 永久删除 | 设置显式开启后删除 2 MB 测试文件；远端验证 `missing`；计划 `completed` |
| 执行前检查 | 文件大小/类型/符号链接/只读挂载检查逻辑覆盖；目录只允许隔离 |
| 审计 | 计划、逐项执行、恢复均写入 audit |

## 原生 smoke

### Docker key 认证

- 6 个主页面均渲染：机群总览、服务器、磁盘分析、任务、告警、设置。
- 添加服务器对话框打开/关闭正常。
- IPC `get_app_info`、`list_servers`、`create_server`、`test_connection`、`trust_host_key`、`start_batch_probe`、`get_task`、`get_latest_snapshot`、`start_usage_scan`、`usage_root`、`list_usage_children`、`get_recommendations`、`sample_metrics` 均通过。
- 服务器详情真实数据显示 Treemap 2 个色块、3 行目录、4 个标签；机群显示 1 行。
- 数据目录为临时 HOME，未污染真实用户数据。

### 真实 Linux 测试机（只读）

- 认证：测试账户 + 密码；主机密钥扫描/确认；未写入任何清理命令。
- 系统：CentOS Linux 7 (Core)；`df`/mounts/卷解析成功；`/boot` 目录分析 `succeeded`，4 个节点。
- 回收建议（只读）：`yum_cache`、`pip_cache`、`tmp_old`、`npm_cache`、`docker_dangling`。
- 实时指标：CPU 与 15 个磁盘设备采样成功。
- 原生 UI 详情页正常渲染，未执行清理。

## 安全验证

- 主机密钥变化被阻断；未知主机不静默信任。
- 密码/口令不出现在 SQLite、日志或命令预览中；测试使用临时 askpass 文件，命令后删除。
- known_hosts 路径包含空格（`Application Support`）时，OpenSSH 选项已加引号；真实 smoke 通过。
- 前端仅传 ID/计划/令牌；路径由后端从数据库解析。
- 清理计划过期/令牌不匹配/状态非 planned 会拒绝执行（代码路径）。
- 无任意命令、无自由 `rm`、无远程脚本下载、无 sudo 密码。
- 临时测试数据目录（`tests/ssh/state`、`tests/ssh/keys`、大文件夹具）不进入应用 zip。

## 构建与分发

- `.app`：`src-tauri/target/release/bundle/macos/服务器硬盘管理工具.app`（约 18 MB，arm64，ad-hoc signed）。
- zip：`dist/ServerDiskManagementTool-0.1.0-macOS-arm64.zip`（约 4.3 MB，含 `INSTALL.txt`）。
- 从 zip 解压后直接运行二进制：窗口、Tauri IPC、SQLite 初始化正常。
- 未做 Developer ID 签名与公证；Finder 直接打开已在本机通过，分发到其他机器可能被 Gatekeeper 拦截，需右键打开。

## Windows 客户端

环境：Windows 10 Enterprise 22H2（19045.6456，x64）；OpenSSH_for_Windows_9.5p1；Node.js 24.14.1；Rust 1.98.1 MSVC；Tauri CLI 2.12.3；WebView2 155.0.4283.62；VS Build Tools 2022。Windows 11 未实机，使用同一 WebView2/OpenSSH 机制。

| 检查 | 结果 |
| --- | --- |
| `npm ci`（lockfile 含全部平台 optional deps） | 通过；54 packages |
| `npm run check:frontend` / `npm run build` | 通过；1608 modules |
| `cargo test` / `cargo build` | 通过；18 passed（含 Windows 凭据管理器 round-trip 与服务器删除） |
| `npm run test:e2e` | 通过；5 passed |
| 原生 smoke（6 页、添加对话框、IPC、服务器删除） | 通过；Tauri release 二进制，删除后 `serversAfterDelete=0` |
| Windows askpass（加密私钥解锁） | 通过；应用自身 askpass 解锁 ed25519 私钥 |
| `--selftest-ssh`（一次性 Linux 主机） | 通过；见下节 |
| NSIS 构建/安装/启动/卸载 | 通过；5.20 MiB，current-user 安装，VC runtime DLL 已包含 |
| Windows 凭据管理器存取 | 通过 Rust round-trip（写入/读取/删除）；跨进程重启未做 GUI 自动化 |
| 高 DPI / 分辨率 | 3840×2160 @100% 通过；1366×768/125% 未实机（Windows 窗口默认最大化） |

### Windows SSH selftest 明细

使用测试服务器上的独立 ext4 loopback 挂载 `/data`，不扫描或清理服务器真实业务目录：

- 主机密钥：首次 `online`；变更 `host_key_changed`。
- 认证：密钥 `online`、加密密钥 `online`、密码 `online`。
- 批量巡检：1 在线 + 1 离线 → `partial`，1 成功 / 1 失败。
- 目录分析：`succeeded`，4 entries / 3 children。
- 大文件：2 个。
- 回收建议：`yum_cache`、`pip_cache`、`tmp_old`、`npm_cache`、`docker_dangling`。
- 清理：文件隔离 `completed` 并恢复；目录 `/data/logs` 隔离并恢复；永久删除 `delete-me.bin` 后远端验证 `missing`；任务 `succeeded`。
- 指标：15 个磁盘设备、CPU/内存采样成功。
- 清理后已卸载 loopback、删除测试镜像、恢复服务器 `sshd_config` 和 `authorized_keys`。

## 未验证 / 已知限制

- macOS 钥匙串“保存后重启再读取”的自动化未做；可能与系统授权弹窗/签名状态有关，需要首次手动确认。密码/口令的 askpass 路径已由 Docker 集成测试验证。
- 取消运行中的清理命令未做端到端断言；取消逻辑复用 SSH 子进程终止与任务状态机，已做代码审查。
- 目录大文件/百万文件级性能只做了架构约束与中小规模夹具，未做正式基准。
- 未验证 SMART、RAID/LVM、远端 agent、定时后台巡检、外部通知。
- 真实服务器只做了只读测试；没有在生产服务器执行清理或永久删除。
- 未签名/未公证；正式发布前需补齐 Developer ID、公证和更新方案。
