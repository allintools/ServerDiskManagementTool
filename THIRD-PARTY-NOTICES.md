# 第三方组件与许可证

本项目使用以下开源组件。各组件的版权归其作者所有，完整许可文本见组件仓库和 `LICENSES/` 目录。

## 运行时组件

| 组件 | 用途 | 许可证 |
| --- | --- | --- |
| [Vue 3](https://github.com/vuejs/core) | 前端框架 | MIT |
| [Lucide](https://github.com/lucide-icons/lucide) | 图标 | ISC |
| [Tauri](https://github.com/tauri-apps/tauri) | 桌面运行时与打包 | MIT 或 Apache-2.0 |
| [@tauri-apps/api](https://github.com/tauri-apps/tauri) | 前端 IPC | MIT 或 Apache-2.0 |
| [tauri-plugin-dialog](https://github.com/tauri-apps/plugins-workspace) | 文件选择对话框 | MIT 或 Apache-2.0 |
| [tauri-plugin-opener](https://github.com/tauri-apps/plugins-workspace) | 打开外部链接 | MIT 或 Apache-2.0 |
| [rusqlite](https://github.com/rusqlite/rusqlite) | SQLite 访问 | MIT |
| [SQLite](https://www.sqlite.org/) | 本地数据库 | Public Domain |
| [serde](https://github.com/serde-rs/serde) / [serde_json](https://github.com/serde-rs/json) | 序列化 | MIT 或 Apache-2.0 |
| [uuid](https://github.com/uuid-rs/uuid) | 标识符 | Apache-2.0 或 MIT |
| [chrono](https://github.com/chronotope/chrono) | 时间处理 | MIT 或 Apache-2.0 |
| [dirs](https://github.com/dirs-dev/dirs-rs) | 系统目录 | MIT 或 Apache-2.0 |
| [keyring](https://github.com/hwchen/keyring-rs) | 系统凭据存储 | MIT 或 Apache-2.0 |

前端依赖清单与版本见 `package-lock.json`，Rust 依赖清单见 `src-tauri/Cargo.lock`。两者包含完整的传递依赖树。

## 开发与测试工具

| 组件 | 用途 | 许可证 |
| --- | --- | --- |
| [Vite](https://github.com/vitejs/vite) | 前端构建 | MIT |
| [TypeScript](https://github.com/microsoft/TypeScript) | 类型检查 | Apache-2.0 |
| [vue-tsc](https://github.com/vuejs/language-tools) | Vue 类型检查 | MIT |
| [Playwright](https://github.com/microsoft/playwright) | 端到端测试 | Apache-2.0 |

开发工具不随桌面应用分发，但本仓库包含其版本锁定信息。

## MPL-2.0 组件

`src-tauri/Cargo.lock` 中的以下传递依赖使用 Mozilla Public License 2.0：

- `cssparser` 0.37.0
- `cssparser-macros` 0.7.1
- `selectors` 0.38.0
- `dtoa-short` 0.3.5
- `option-ext` 0.2.0

这些组件以未修改的库形式链接。其完整许可文本见 `LICENSES/MPL-2.0.txt`，对应源码可从 crates.io 获取，例如 <https://crates.io/crates/cssparser/0.37.0>。如需核对或替换这些组件，请以 `src-tauri/Cargo.lock` 锁定的版本为准。

## 系统组件

- **OpenSSH** 不随应用分发，由用户自行安装；Windows 安装包通过 WebView2 Bootstrapper 获取运行时，受 Microsoft 的分发条款约束。
- **WebView2**、macOS 系统框架和 Windows 系统库按各自系统条款使用。

## 许可文本

- MIT：根目录 [LICENSE](LICENSE)
- 第三方 MIT 组件：[LICENSES/MIT-Vue.txt](LICENSES/MIT-Vue.txt)、[LICENSES/MIT-Vite.txt](LICENSES/MIT-Vite.txt)、[LICENSES/MIT-Rollup.txt](LICENSES/MIT-Rollup.txt)、[LICENSES/MIT-Esbuild.txt](LICENSES/MIT-Esbuild.txt)、[LICENSES/MIT-Plugin-Vue.txt](LICENSES/MIT-Plugin-Vue.txt)、[LICENSES/MIT-Vue-TSC.txt](LICENSES/MIT-Vue-TSC.txt)
- Apache-2.0：[LICENSES/Apache-2.0.txt](LICENSES/Apache-2.0.txt)
- MPL-2.0：[LICENSES/MPL-2.0.txt](LICENSES/MPL-2.0.txt)
- ISC：Lucide 的许可文本见 [public/licenses/Lucide-ISC.txt](public/licenses/Lucide-ISC.txt)
- BSD-2-Clause：[LICENSES/BSD-2-Clause.txt](LICENSES/BSD-2-Clause.txt)
- BSD-3-Clause：[LICENSES/BSD-3-Clause.txt](LICENSES/BSD-3-Clause.txt)

如发现遗漏或错误的声明，请通过 Issue 联系。

分发桌面安装包时，请把本文件和 `LICENSES/` 目录一并放入安装包，或在应用内提供同等可访问的第三方声明页面。
