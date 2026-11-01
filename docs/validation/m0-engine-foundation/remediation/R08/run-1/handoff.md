# R08 handoff

- F-11 覆盖完成：独立 TSV 14 行由 `crates/aline-edit/tests/crossing.rs` 加载；既有 `refusals.rs` 四条新增脚注/HTML拒绝继续保留。
- 空删除 5 例：正文 Changed、脚注跨入一字节拒绝、Visual 全引用拒绝、Source 全引用删除且 1 个历史步、空范围空替换为 `Unchanged(ReplacementIdentical)`。
- 粘贴 6 例：左边界 7..7 与右边界 11..11 拒绝；左侧不触边插入、Source 同位置、多行粘贴允许；覆盖脚注拒绝。边界裁决及原 brief 矛盾说明见 brief 附录和 report。
- HTML 3 例：格式右端切入闭标签、格式范围跨全元素、编辑开标签内部；均断言精确 HtmlBoundary 范围。
- 所有拒绝在 `plan_change` 前捕获完整 State，前后相等且 redo 可实际消费；脚注/HTML 区间来自 Parsed，范围探针记录 `7..11`、`2..5`、`5..6`、`6..10`。
- `cargo test -p aline-edit --locked --offline` 全通过：crossing 1、goldens 6、protection 4、refusals 16。aline-edit all-target clippy、fmt check、diff check 通过。workspace test/门禁留 R14。
- 无生产逻辑缺陷；未改生产代码/UI/scripts，未提交。
- 逐命令证据及早期失败保存在同目录 `*.command.json`、`*.stdout.log`、`*.stderr.log`；最终摘要见 `report.md`、包级原始输出见 `post-fix.txt`。
