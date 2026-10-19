# 服务器硬盘管理工具 实施步骤与完成状态

日期：2026-10-17  
范围：远程 Linux 服务器磁盘巡检、空间分析、告警、任务与受控空间回收。

## 总原则

1. **只读先于写入**：Step 2–5 只读；Step 6 才引入写命令。
2. **逐主机结果优先**：批量任务不使用单一“成功”，部分失败必须可见。
3. **固定命令模板**：远端脚本随包携带，参数转义；没有前端传入的任意命令。
4. **计划与重检**：写操作必须经过逐主机计划、一次性令牌、执行前检查与审计。
5. **秘密不落库**：SSH 密码/口令只进系统凭据存储（macOS 钥匙串 / Windows 凭据管理器），或仅本次会话内存。

## Step 1：独立桌面壳

状态：**完成**

- 建立独立工程与桌面壳；应用标识 `服务器硬盘管理工具` / `com.novarack.desktop`。
- 6 个主页面、详情页 4 个标签、深色/浅色主题、演示模式、原生窗口 1280×820。
- 验收：`vue-tsc`、Vite 构建、Tauri release 构建通过。

## Step 2：SSH 核心与单机接入

状态：**完成**

- 系统 OpenSSH 传输；严格主机密钥；应用私有 `known_hosts`；未知主机指纹确认；密钥变化阻断。
- 认证：agent / 私钥 / 加密私钥口令 / 密码；口令通过 `SSH_ASKPASS` 临时文件提供；密码/口令可存系统凭据存储。
- 从 `~/.ssh/config` 导入具体 Host；保留 ProxyJump/IdentityFile 等配置。
- `probe_v1.sh` 采集 OS、内核、CPU/内存、`df`、`df -i`、mounts、`lsblk`、能力矩阵；解析为磁盘/卷/快照。
- 验收：Docker SSH 夹具、密码、加密密钥、主机密钥未知/变化、真实 Linux 测试机均通过。

## Step 3：批量巡检与机群总览

状态：**完成**

- 每任务多主机、并发、取消、逐主机状态、部分完成/中断；任务历史持久化。
- 机群总览 KPI、容量风险、服务器表、搜索/筛选、批量刷新、告警。
- 快照当前表与历史；阈值 80/90%（容量）与 85/95%（inode）、只读挂载、离线告警。
- 验收：Docker 单在线 + 单离线批量任务得到 `partial`（1 成功、1 失败）；原生 UI 机群显示真实行。

## Step 4：磁盘空间分析

状态：**完成**

- 按需逐层 `du -x -B1 -d 1`；优先 NUL 分隔，BusyBox 使用近似 KiB；后端路径校验，前端只传节点 ID。
- Treemap、面包屑、目录表、大文件扫描（GNU find，≥100MB/1GB/10GB，最多 500 条）。
- 权限不足/部分结果/取消/超时标记；回收建议只读检测。
- 验收：Docker `/data` 分析 4 项、大文件 2 个；真实 Linux 主机 `/boot` 分析 4 个节点；UI 原生 smoke 渲染 Treemap 与表格。

## Step 5：实时指标、历史、设置与首个可运行包

状态：**完成**

- `metrics_v1.sh` 双采样 `/proc/stat`、`/proc/meminfo`、`/proc/diskstats`、`/proc/loadavg`；后端计算 CPU/内存/磁盘速率。
- 任务历史、设置：并发、超时、阈值、保留期、主题、SSH 诊断、known_hosts、写并发、隔离保留、永久删除开关。
- 启动时把运行中任务标记 `interrupted`；清理计划同样标记。
- 验收：原生 smoke 6 页渲染、添加对话框、IPC `get_app_info`、真实 SSH 详情的 Treemap/4 标签；`.app` 构建成功。

## Step 6：空间回收

状态：**完成**

### 6A 模板回收

- 固定模板：apt/dnf/yum/pacman/apk 缓存、journal 7 天、npm/pip 缓存、Docker 悬空镜像/构建缓存、`/tmp` 旧文件。
- 只读检测 → 生成计划 → 5 分钟令牌 → 执行前重检 → `sudo -n` 执行 → 前后 `df` 对比 → 审计。
- 写任务并发 2；无自动重试；SSH 中断标记失败/未知。
- 验收：Docker apt 缓存真实释放；计划状态 `completed`。

### 6B 文件级回收与隔离

- 大文件/目录来自分析结果；前端只传 scan/node/large-file ID。
- 执行前校验存在性、非符号链接、文件大小、挂载非只读；目录不做永久删除。
- 隔离区：同文件系统 `/.nova-rack-quarantine/<batch>/...`；支持恢复、清空；清空才释放空间。
- 永久删除仅限普通文件；设置里显式开启，执行时需输入 `DELETE`。
- 验收：Docker 隔离 120MB 文件并恢复成功；`/data/logs` 目录隔离/恢复成功；永久删除 2MB 测试文件，远端验证 `missing`，计划 `completed`。

## Step 7：加固、全量测试与交付

状态：**完成**

- Rust 单测 10 项：命令引用、`df`/`mounts`/`lsblk`/`du` 解析、BusyBox 回退、任务/指标解析等。
- Docker 集成：密钥、加密密钥、密码、主机密钥未知/变化、单机与批量、分析、大文件、回收、隔离/恢复、永久删除、指标。
- Playwright 5 条：机群/详情、添加服务器、服务器删除、分析→回收计划→执行、设置保存。
- 原生 smoke：真实 SSH 容器与真实 Linux 测试机；页面、IPC、真实数据详情、批量任务、Treemap。
- 构建 `服务器硬盘管理工具.app` 与 zip；文档完成。

## Step 8：Windows 客户端

状态：**完成**

- 平台抽象：`platform.rs` 统一 child process 隐藏控制台、路径展开、known_hosts 转义、文件替换和 Unix/Windows 权限差异。
- SSH 工具链：`ssh/tools.rs` 从 `NOVA_RACK_SSH`、`%WINDIR%\System32\OpenSSH`、PATH 解析 `ssh`/`ssh-keyscan`/`ssh-keygen`/`ssh-add`。
- askpass：Windows 使用应用自身可执行文件作为 `SSH_ASKPASS`，临时 secret 文件；macOS 保留 shell 脚本。
- 凭据：macOS `apple-native`、Windows `windows-native`；Linux 测试构建使用不支持 stub，避免引入 DBus。
- 数据目录：Windows 使用 `%LOCALAPPDATA%\com.novarack.desktop`；新增 `RACK_DATA_DIR` 测试覆盖。
- 打包：`tauri.windows.conf.json`、`icons/icon.ico`、NSIS current-user、WebView2 bootstrapper、VC runtime。
- 验收：Windows 10 22H2/OpenSSH 9.5p1 上完成编译、Rust 单测、Playwright、原生 smoke、askpass、`--selftest-ssh`（一次性 Linux 主机）、NSIS 安装/卸载。macOS 侧改动通过 `cfg` 隔离，仍需在 macOS 环境补跑构建与测试。

## 未包含

- Windows ARM64/x86、Windows 7、MSI、代码签名/公证。
- 远端 agent、SMART、RAID/LVM/分区/挂载修改。
- 定时后台巡检、外部通知、自动更新。
- 未签名本地自用构建；发布前需补齐签名、公证与更新策略。
