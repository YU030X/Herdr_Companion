# Herdr Companion Agent Notes

## Scope & routing

- 以 `docs/IMPLEMENTATION_PLAN.md` 的当前里程碑为范围边界；不要提前实现后续里程碑。
- 修改 Herdr transport、schema 或兼容行为前先读 `docs/HERDR_PROTOCOL.md`，并在同一改动中同步协议文档。
- 修改领域术语或状态语义时同步 `docs/CONTEXT.md`；实现细节不要写入词汇表。

## Landmines

- 不得从终端输出、Pane 布局或创建顺序推测任务文案和 Agent 关系；fixture、日志和提交中不得包含真实终端内容或 Session Snapshot。
- 当前 Agent 条目刻意保持非交互；除非任务明确恢复 Focus UI，否则不要因为后端仍有 `focus_agent` 就添加点击入口。
- `pnpm tauri dev` 是长运行 GUI，只能交给用户执行人工检查。
- Release 构建前检查 `herdr-companion.exe` 是否正在运行；文件被占用时请用户关闭窗口，不得擅自终止用户进程。

## Validation

- 使用 `docs/VALIDATION.md` 中的完整检查集；行为变更必须增加最小回归测试。
- Release 验收使用 `pnpm tauri build --no-bundle`，不生成安装器。
