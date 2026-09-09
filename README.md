# Herdr Companion

Herdr Companion 是一个面向 Windows 的轻量桌面 HUD，用于查看本机 Herdr 中的 Workspace、Agent 状态和任务摘要。

## 当前功能

- 连接本机 Herdr JSON API
- 按 Workspace 筛选 Agent
- 按 Blocked、Done、Working、Idle、Unknown 排序
- 断线后保留并标记最后一次 Snapshot
- 自动重连和手动重试
- 记忆窗口尺寸、位置和置顶偏好

当前 Agent 条目仅用于展示，不提供 Focus、托盘或系统通知。

## 环境要求

- Windows 11
- Herdr `0.9.0-preview.2026-09-08-62431dbd033b` 或兼容 JSON API
- Node.js、pnpm 和 Rust 工具链
- WebView2 Runtime

## 开发

```powershell
pnpm install
pnpm typecheck
pnpm test
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri dev
```

`pnpm tauri dev` 会启动长运行桌面窗口，需要手动检查。

## Release 构建

```powershell
pnpm tauri build --no-bundle
```

生成的程序位于：

```text
src-tauri/target/release/herdr-companion.exe
```
