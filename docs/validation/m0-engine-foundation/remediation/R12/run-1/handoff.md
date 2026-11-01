# R12 交接

- 状态：实现和验证完成，未提交，交主 Agent核验。
- 修改：`scripts/check_lints.py`、`scripts/check_engineering.py`、`scripts/engineering_self_test.py`、`scripts/check_public_api.py`；新增 R12 审计/运行脚本、报告及证据。
- 原因：fixture 缺 lock 时 `cargo check --offline` 真实 exit 0 并生成 lock；offline 不等于 locked。
- 修复：六处曾缺 `--locked` 的依赖执行点（check_lints 四处、engineering self-test 正例一处、缺失 manifest 负例一处）现在均锁定；缺 lock 的 fixture 先单独生成并记 `initial-lock-generation`。check_lints/engineering/public-api 命令 JSON 的 `evidence_kind`、`locked`、`offline` 均由 argv 推导。
- AST 审计：25 个实际执行调用点，16 个依赖 lock；16/16 locked，0 个缺锁调用；3 个初始生成点；无语法解析错误。
- 验证：check_lints 正常/self-test、engineering self-test 97项、public API 8项（预热 CARGO_HOME 复跑）、safety self-test 65项，AST、py_compile、diff-check 均通过。public API 16 条 Cargo 记录均含 evidence_kind/locked/offline（8 生成、8 锁定检查）。
- 文档落盘后复核 `git diff --check` 与 47 个文本证据文件的尾随空白，均通过。
- public API 首轮离线失败因自动选择的隔离 CARGO_HOME 缺缓存，发生在初始生成阶段；原失败保留，使用已有 `<cargo-home>` 重跑通过，未联网下载。
- 详见 `report.md`、`commands.txt`、`post-fix.txt`、各轮 `*-command-artifacts.tar.gz` 与 follow-up supplement 及 `nested-cargo-audit.json/.md`。
- 需主 Agent 将 report 提供的 R12 勘误追加到 group-2 engineering guard report；本 Agent 未越权写该文件。
- 未执行 workspace clippy/test；留 R14。未 commit。
