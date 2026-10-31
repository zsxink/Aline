# 9.1 workspace 检查记录

**依赖清理后的最新结果：** 2026-10-31T15:02:22Z–15:03:05Z 重新执行统一入口，19 项均 exit 0，workspace 测试合计 312 passed、0 failed；原始证据见 [workspace-gates-after-cleanup](./workspace-gates-after-cleanup/) 和[入口命令](./formal-run-after-cleanup/commands.log)。逐项 [commands.log](./workspace-gates-after-cleanup/commands.log) 由该次原始 summary 的实际 argv/时间/退出码生成。此前 14:52 的记录继续保留为历史结果，不代表清理后的依赖图。

2026-10-31T14:52:24Z–14:53:10Z，当前 macOS arm64 主机以 Rust 1.98.1、指定 `CARGO_HOME` 和 `CARGO_NET_OFFLINE=true` 执行用户指定的统一入口，[入口命令与退出码](./formal-run/commands.log)为 exit 0；stdout/stderr 分别位于该目录。入口实际运行 19 项，均 exit 0；[原始汇总](./workspace-gates/summary.json)包含 argv、环境、时间及逐项退出码，[commands.log](./workspace-gates/commands.log)按原始汇总转换为秒级 UTC 命令索引。各项原始 stdout/stderr 及夹具输出保留于 [workspace-gates](./workspace-gates/)。

workspace fmt、带 `--locked --offline` 的 Clippy 和测试均通过；测试原始输出合计 312 passed、0 failed，安全自测 53 例全部符合预期。`--all-targets` 仅指当前目标上的 Cargo target，不证明其他 OS/架构已构建。CI、Windows/Linux、Intel Mac 仍未执行。

统一入口的 workspace Clippy/测试命令已补 `--locked --offline`；其工程依赖图子检查也使用这两个参数。lint 负例夹具内部会运行 `cargo check/clippy --offline` 而不带 `--locked`，`cargo read-manifest` 使用 `--offline`；版本与 fmt 查询没有锁文件语义。这些嵌套命令的实际 argv 以对应夹具原始记录为准，不能将本次 19 项描述为所有嵌套 Cargo 命令均带 `--locked`。主 Agent 在依赖清理时另外遗漏 `--locked` 及原始输出的事实见[依赖清理记录](../../group-8/followup-2026-10-31/dependency-cleanup-note.md)。

原 `safety-guard-fix` 内 13:43 的历史 19 项与提交前三门禁仍保留原处，不改写为本次结果。本记录不代替 E1–E7 的逐项审查，不代表 V-01～V-09 或 M0 出口通过。
