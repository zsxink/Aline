# R01 交接

## 已做事项

1. 先写 5 条失败回归并证明先失败（F-01 行为断言 + F-05 三条选区用例）。
2. 实施最小修复：`TypingGroup` 增加 `after_selection`；undo 捕获、redo 受检恢复；
   `redo()` 遇待定输入组返回 `DocError::PendingInput`（零副作用）。
3. 复跑回归 + `cargo test -p aline-ui` + fmt + clippy + workspace 测试，全部通过。

## 修改文件（仅我所有的路径）

- `crates/aline-ui/src/document/open_block.rs`：`TypingGroup.after_selection` 字段与注释。
- `crates/aline-ui/src/document/doc_core.rs`：`DocError::PendingInput`；`undo()` 捕获
  `block.selection`；`redo()` 待定输入守卫 + 用 `group.after_selection` 受检恢复；
  `start_group()` / `append_to_group()` 维护新字段。
- `crates/aline-ui/tests/document_typing.rs`：新增 `limits()` 与 5 条 R01 回归。
- 未触碰 `crates/aline-markdown/**`；未使用 `git add -A`。

## 验证结果与证据路径

证据目录 `/Users/xian/Project/editor/Aline/docs/validation/m0-engine-foundation/remediation/R01/run-1/`：

- `pre-fix-tests.txt`：4 次运行原始输出（前 2 次为编译失败，见下方偏差；后 2 次为真实行为失败）。
- `post-fix-tests.txt`：document_typing（30 passed）、`cargo test -p aline-ui`（8 目标全 ok）、
  `cargo fmt --all --check`（格式化前 exit 1 → 格式化后 exit 0）、clippy（exit 0）、
  `cargo test --workspace`（exit 0，62 个 ok）。
- `commands.txt`：逐条 argv / cwd / 起止 UTC 时间 / exit code。
- `report.md`：根因、改动、断言逐条对照、剩余问题。

结论：F-01、F-05 两条阻断失败均已修复并有回归锁定；既有无 8.2 用例与 workspace 测试无回归。

## 审查结论（供复核参考）

- 既有约束保持：8.2-15 → 1:1、8.2-16 → 3:3、8.2-18/19 → 2:2 均仍通过。
- 生产代码未新增 `unwrap/expect/panic!/unreachable!/assert!`；测试未使用 `unwrap()`。
- 修复无新增空历史步：草稿 redo 仍不计数；提交分支 node 数符合 §9.3。

## 待裁决

1. **T-8-17**（已登记，`docs/validation/m0-engine-foundation/group-8/README.md` 与
   `step-8.4/report.md` 均标 `待定`）：「Undo 后开启新草稿时，Buffer redo 分支与草稿 redo 的失效时序」。
   本实现取「受检拒绝」：待定输入存在时 `redo()` → `Err(DocError::PendingInput)`，文本/草稿/版本/
   历史/redo 深度全部不变；待定组提交后才截断 redo 分支（视为 §9.3 的「新增实际编辑」）。
   理由：最贴近 §9.3 文字（不无故清空 redo、不越过未结算输入），且与原复核 F-01 建议的
   「拒绝或按新分支协议结算，保证输入保全」中的「拒绝」一致。
   若复核方更倾向「新分支协议结算」，需要补充规范条文，再据此改写守卫。
2. `DocError::PendingInput` 是新增公开变体：workspace 内无穷尽匹配，但宿主 `match` 需补分支。

## 已知偏差（如实记录）

`pre-fix-tests.txt` 的第 1、2 次运行是**编译失败**（E0599：测试引用了尚不存在的
`DocError::PendingInput`），不是断言失败。为不让编译错误冒充「先失败」证据，把首轮 F-01 回归
改为不依赖新变体的行为断言（`assert_rejected_without_change(..., |_| true)`）后重跑，才得到真实失败
（`assertion failed: error.is_err()`）；修复完成后再把断言收紧为 `DocError::PendingInput`。

## 剩余问题

- 无已知功能缺陷；等待 T-8-17 裁决确认守卫选择。
- 提交未 push。

## 下一步

1. 复核 `report.md` §3 断言对照与 `post-fix-tests.txt` 原始输出。
2. 裁决 T-8-17；若改为「新分支协议结算」，只需替换 `redo()` 中 `PendingInput` 守卫。
3. 合并后同步 `docs/maintenance/m0-remediation/tasks.md` 的 R01 状态（不在本 worker 文件范围内）。
