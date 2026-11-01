# R01 派发契约：F-01/F-05 待定组 Redo 输入保全与方向选区

基线 HEAD：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`（2026-11-01 起）。仓库根 `<repo>`。

## 文件所有权（只有你可写这些文件）

- `crates/aline-ui/src/document/doc_core.rs`
- `crates/aline-ui/src/document/open_block.rs`
- `crates/aline-ui/tests/document_typing.rs`
- `docs/validation/m0-engine-foundation/remediation/R01/run-1/` 下你自己新建的证据文件

不得写其它文件。不得 `git reset`、不得回退他人改动。另一个 worker 正在并行改 `crates/aline-markdown/**`，那不是你的范围。

## 两个独立失败（必须先各自写出失败回归，再修）

**F-01（阻断）**：`x → 输入 "a" 并提交 → undo → 输入 "b"（未封口）→ redo`。原实测结果 `xb / draft b / redo_depth 1` 变成 `xa / draft None / Ok`——待定输入 b 被 redo 吞掉。

**F-05（阻断）**：`x → 在末端输入 "abc" → set_selection(4,2)（反向非折叠）→ undo → redo`。原选区变成 `(4,4)`，方向与展开长度都丢了。

原复核用的独立测试源（可原样用作回归起点）：

```rust
use aline_buffer::{HistoryBudget,LineIndexBudget,SelectionSpec,TextBudget};
use aline_ui::{DocCore,DocLimits};
fn limits()->DocLimits { DocLimits::new(TextBudget::new(4096),LineIndexBudget::new(512),HistoryBudget::new(512,4096)) }
#[test] fn redo_must_not_discard_pending_input()->Result<(),Box<dyn std::error::Error>> {
let mut d=DocCore::new("x",SelectionSpec::new(1,1),limits())?;d.type_text(1,"a",0)?;d.commit_open_block(1)?;d.undo()?;d.type_text(1,"b",2)?;
let _=d.redo();let current=d.current_snapshot(TextBudget::new(4096))?.full_text(TextBudget::new(4096))?;
assert!(current.contains('b'),"pending b disappeared, actual={current:?}");Ok(()) }
#[test] fn pending_redo_restores_directional_selection()->Result<(),Box<dyn std::error::Error>> {
let mut d=DocCore::new("x",SelectionSpec::new(1,1),limits())?;d.type_text(1,"abc",0)?;d.set_selection(SelectionSpec::new(4,2))?;d.undo()?;d.redo()?;
assert_eq!(d.current_selection(),SelectionSpec::new(4,2));Ok(()) }
```

## 现行契约（不得自创行为）

`openspec/changes/m0-engine-foundation/specs/engine/document-core/spec.md`：
- Scenario「Undo 与组合取消」：当前存在待定组时 Undo，Undo 优先撤销待定组并恢复选区；取消恢复组合前状态且无空步。
- Requirement「唯一草稿与当前快照」：不封口、不改变历史。

`docs/engine-design.md` §9.3：Undo 优先处理待定组；Redo 恢复同一组及对应选区；新的实际编辑使 redo 分支失效；不产生空历史步；无理由不得清空 redo。

Redo 的既有语义边界（已登记事实，供你据此判断，不是要你照抄）：
- `DocCore::redo()` 当前在无 `undone_typing` 时落到 `buffer.redo()`，再 `sync_block_after_buffer_change()` 清空 draft，这既是 F-01 的路径。
- 有 `undone_typing` 时用 `SelectionSpec::new(end,end)` 重建选区，这是 F-05 的路径；`TypingGroup` 目前没有 `after_selection` 字段，而 undo 时 `block.selection` 保存着当时的 (4,2)。
- `type_text`/`start_group`/`commit_typing_group` 都会 `block.undone_typing = None`。

## 要求

1. 先写失败回归（两个独立测试，命名清楚对应 F-01 / F-05），在证据目录 `pre-fix-tests.txt` 记录实际命令、cwd、起止时间、exit code、原始 stdout/stderr，确认它们**先失败**。
2. 再实施最小修复。不得为通过测试临时选一个无规范依据的行为；若你认为契约在某处分叉含糊，把该分叉写进 handoff 的「待裁决」并选择最贴近 §9.3 文字的解释。
3. 覆盖正/反向及非折叠选区；检查文本、草稿、Buffer 文本、选区、工作版本、undo/redo 深度、单历史、快照不封口、无空历史步。
4. 不得新增 `unwrap()/expect()/panic!/unreachable!/assert!` 到 engine 生产代码（clippy 已 deny）。测试里也不得 `unwrap()`。
5. 全部 Cargo 命令用 `--locked --offline`，环境 `CARGO_NET_OFFLINE=true`，工具链 1.98.1。工具缺失/离线失败记「未核实」，不要联网安装。
6. 提交（Conventional Commits，scope 用 `aline-ui`）。

## 证据（放 `docs/validation/m0-engine-foundation/remediation/R01/run-1/`）

- `pre-fix-tests.txt`（失败回归原始输出）
- `post-fix-tests.txt`（修复后回归 + `cargo test -p aline-ui` 相关集）
- `commands.txt`（每条的 argv、cwd、起止时间、exit code）
- `report.md`（根因、改动、断言逐条对照、剩余问题）
- `handoff.md`（≤80 行：已做事项、修改文件、验证结果/路径、审查结论、剩余问题、下一步）

## 返回给我（≤800 中文字）

status（DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED）、提交哈希、一行测试小结、concerns、证据路径。不要把长日志贴回来。
