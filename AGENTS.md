# Herdr Companion 项目约定

## 项目目标

Herdr Companion 是面向单个操作者的轻量桌面状态与导航伴侣。Herdr 是唯一主要运行状态来源；应用优先呈现需要人工介入的 Agent，并通过 `agent.focus` 帮助用户返回工作现场。

## 当前技术方向

- 桌面外壳：Tauri 2
- 后端：Rust
- 前端：Preact + TypeScript
- 首个运行环境：Windows 原生 Herdr
- 首个里程碑：真实连接 → Session Snapshot → HUD Agent 列表 → Focus Agent
- 首版窗口：支持切换置顶；Focus 成功后保持 Companion 可见
- 状态时效：Blocked/Done 目标在 1 秒内显示，优先使用 Herdr 事件；重连后重新获取 Snapshot
- 断线行为：保留最后快照并明确标记 stale，禁用依赖连接的操作；自动退避重连并支持手动重试
- 任务文案：只使用显式元数据或 Herdr 标题，不读取终端输出猜测
- 窗口偏好：首次默认不置顶，记住用户最近一次置顶选择
- 隐私边界：诊断数据只保留本地，不集成分析或自动崩溃上传；日志不得包含终端内容
- 数据持久化：运行状态快照不落盘；应用重启后必须从 Herdr 获取新 Snapshot
- 首次交付：生成可运行的本机 release 程序，暂不包含安装器、签名和自动更新
- 第二里程碑：托盘、关闭到托盘和 Blocked/Done 系统提醒；通知由 Companion 统一去重和发送
- 性能策略：先记录 release 构建内存基线，再确定硬预算

## 架构边界

- Rust 负责 Herdr IPC、协议兼容、状态规范化、重连和桌面系统能力。
- Preact 只消费规范化后的 Companion 模型，不直接解析 Named Pipe 或 Herdr 原始协议。
- Herdr 的五种状态为 `blocked / done / working / idle / unknown`；不得建立竞争状态机。
- 没有显式父子元数据时，不得根据 Pane 布局、创建顺序或输出内容猜测 Agent 关系。
- 时长必须表述为 observed duration，不得冒充真实启动时间或运行时长。

## 实施原则

- 保持轻量，只实现当前里程碑需要的功能。
- 优先迁移现有 HUD 信息结构和 CSS，再做视觉重构。
- Adapter 必须隔离真实 Herdr transport 与开发用 Mock。
- 对未知字段宽容；对不兼容的 Herdr 版本显示可操作错误，不静默失败。
- 修改行为前先定义可验证的验收条件，并补充最小必要测试。

## 文档

- `docs/HERDR_COMPANION_RESEARCH.md`：技术调研与 MVP 基线
- `docs/HERDR_PROTOCOL.md`：当前支持的 Herdr 协议与 transport 基线
- `docs/IMPLEMENTATION_PLAN.md`：已确认的实施顺序与验收门槛
- `docs/CONTEXT.md`：领域统一语言
- `docs/adr/`：满足 ADR 条件的长期架构决策
- 领域术语确定后立即更新 `docs/CONTEXT.md`；实现细节不得写入词汇表。

## 验证

```powershell
pnpm typecheck
pnpm test
pnpm build
cargo fmt --manifest-path .\src-tauri\Cargo.toml -- --check
cargo check --manifest-path .\src-tauri\Cargo.toml
cargo test --manifest-path .\src-tauri\Cargo.toml
pnpm tauri info
```

`pnpm tauri dev` 会启动长运行开发进程，仅在需要人工检查窗口时由用户执行。发布验收使用 `pnpm tauri build --no-bundle`。
