# 参与贡献

感谢提交问题和改进。提交代码前请先开 Issue 描述场景，涉及 SSH、清理和凭据的改动请说明安全影响。

## 开发环境

- Node.js 22.12+；
- Rust stable；
- macOS 13+ 或 Windows 10 1809+ / 11 x64；
- Windows 需要 OpenSSH Client。

```sh
npm ci
npm run desktop:dev
```

## 提交前检查

```sh
npm run check:frontend
npm run build
cd src-tauri && cargo test
```

前端流程测试：

```sh
npx playwright install chromium
npm run test:e2e
```

涉及 SSH、清理或解析逻辑的改动，请同时运行 `npm run test:ssh`（需要 Docker）。破坏性测试只能使用一次性环境。

## 代码约定

- 保持现有分层，前端只传 ID，路径和命令由后端解析；
- 不引入任意命令执行、通配符删除或递归删除目录；
- 新的远端脚本必须随包携带并经过转义与超时处理；
- 不在日志、数据库、导出或测试夹具中写入真实凭据；
- 提交信息说明改了什么、为什么改，以及验证方式。

## 许可

提交贡献即表示同意以本仓库的 MIT 许可证发布。
