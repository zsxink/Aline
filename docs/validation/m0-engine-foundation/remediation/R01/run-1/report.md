# R01 报告：F-01 待定输入保全与 F-05 方向选区恢复

基线 HEAD `2f1dcea46d5998f4e2ba213e73c11de166f943c4`；工具链 rustc/cargo 1.98.1；全部 Cargo 命令
`--locked --offline` + `CARGO_NET_OFFLINE=true`，无联网、无安装、无离线失败。

## 1. 根因

### F-01 待定输入被 Redo 吞掉

`DocCore::redo()` 只在 `open.undone_typing` 为 `Some` 时走草稿分支；否则直接 `buffer.redo()`，
再 `sync_block_after_buffer_change()`。后者第一件事是 `block.draft = DraftSlot::Empty` 并清空
`undone_typing`。因此 undo 之后新输入的待定组 b 只存在于 `block.draft`，redo 落到 Buffer 分支，
重做旧节点 a，Buffer 文本变回 `xa`，草稿 b 被清空，`redo_depth` 归 0。即「Redo 越过未结算的输入」，
违反 §9.3「Redo 恢复同一组及对应选区」「无理由不得清空 redo」。

### F-05 方向与展开长度丢失

`TypingGroup` 原本只有 `before_selection`；undo 把 `block.selection` 覆盖成 `before_selection` 后，
撤销时刻的选区（4:2）无处留存。redo 用 `SelectionSpec::new(end, end)` 重建，必然折叠到输入末端（4:4）。

## 2. 改动（3 个文件，均属我所有的路径）

- `crates/aline-ui/src/document/open_block.rs`
  - `TypingGroup` 新增 `after_selection: SelectionSpec`（撤销时刻的选区，供重做恢复），并补中文文档注释。
- `crates/aline-ui/src/document/doc_core.rs`
  - `DocError` 新增 `PendingInput`（文档注释；`Display` 走 Debug，`source()` 落 `_ => None`，无破坏）。
  - `undo()` 待定组分支：克隆组时写入 `block.selection` 作为 `after_selection`，随后才覆盖为 `before_selection`。
  - `redo()`：在 `ensure_ready` 与组合检查之后、任何状态改动之前，存在待定普通输入组则返回
    `Err(DocError::PendingInput)`；草稿分支改用 `group.after_selection`，并先在克隆「探针块」上
    `validate_selection`，通过后才写真实状态（受检恢复）。
  - `start_group()` 两个构造点与 `append_to_group()` 同步维护 `after_selection`。
- `crates/aline-ui/tests/document_typing.rs`：新增 5 条回归，既有 8.2 用例未改动。

生产代码未新增 `unwrap/expect/panic!/unreachable!/assert!`；测试中未使用 `unwrap()`。

## 3. 断言逐条对照

| 要求 | 用例 | 断言 |
|---|---|---|
| F-01 输入保全 | `redo_must_not_discard_pending_input` | redo 返回 `Err(DocError::PendingInput)` 且 `capture` 前后完全相等（buffer `x`、快照 `xb`、draft `Some("b")`、rev 2 / dg 4、undo/redo/nodes = 0/1/1）；`full_text` 含 `b`；两次 `snapshot_id` 相同（快照不封口）；`assert_history(0,1,1)`；`assert_open(2, Some("b"), 2, 2)`；无组合/等待 |
| F-01 分支结算 | `pending_input_commit_invalidates_redo_branch_without_empty_step` | 待定组提交 = 新增实际编辑 → 原 redo 分支失效（§9.3）；提交只加 1 步（node=1，无空步），随后 redo 以 `Buffer(NothingToRedo)` 拒绝且零副作用 |
| F-05 反向非折叠 | `pending_redo_restores_directional_selection` | undo 后选区 1:1；redo 后 `current_selection() == 4:2`；buffer `x`、快照 `xabc`、rev 0 / dg 3、历史 0/0/0、`assert_open(0, Some("abc"), 4, 2)`、快照 id new |
| 正向非折叠 | `pending_redo_restores_forward_selection` | 同上，选区 2:4 |
| 折叠 | `pending_redo_restores_collapsed_selection` | 同上，选区 3:3 仍折叠 |
| 无空历史步 / 单历史 | 全部五条 | redo 前后 `assert_history` 均 0/0/0（草稿重做不计数）；提交分支 1/0/1 |
| 工作版本 | 全部五条 | `assert_version` 精确断言 revision 与 draft_generation |
| 快照不封口 | F-01 与 F-05 | 连续两次 `snapshot_id` 相等 / `assert_new_snapshot` |

既有约束未破坏：8.2-15（undo 待定 → 1:1）、8.2-16（redo 待定 → 3:3）、8.2-18/19（节点级 → 2:2）全部仍通过
（undo 时 `after_selection` 取自 `block.selection`，在这些用例下与旧实现 `end..end` 等价）。

## 4. 验证

- `cargo test -p aline-ui --test document_typing`：30 passed。
- `cargo test -p aline-ui`：8 个测试目标全 ok。
- `cargo fmt --all --check`：exit 0；`cargo clippy -p aline-ui --all-targets -- -D warnings`：exit 0。
- `cargo test --workspace`：exit 0，62 个 `test result: ok`，0 failed。
- 证据：`pre-fix-tests.txt`、`post-fix-tests.txt`、`commands.txt`。

## 5. 剩余问题 / 待裁决

1. **T-8-17 分叉**：undo 后开启新草稿时，Buffer redo 分支与草稿 redo 的失效时序仍为 `待定`。
   本实现选择「受检拒绝」——待定输入存在时 `redo()` 返回 `PendingInput`，状态零变化、不清空 redo；
   待定组一旦提交（新增实际编辑）才按 §9.3 截断 redo。与复核 F-01 建议中的「拒绝」一致，最贴近 §9.3 文字。
2. `DocError::PendingInput` 是新增公开变体；仓库内无其它 crate 对 `DocError` 穷尽匹配（workspace 测试通过），
   但宿主侧 `match` 需补分支。
3. `pre-fix-tests.txt` 前两次运行因测试引用尚未存在的 `PendingInput` 而编译失败（E0599）；
   已在 `commands.txt` 与 `handoff.md` 如实记录该 TDD 顺序偏差及改正方式。
