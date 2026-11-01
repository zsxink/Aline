# R01 主 Agent 核对记录

日期：2026-11-01。核对人：主 Agent（未参与 R01 实现）。核对对象：提交 `79c3b54`（BASE `2f1dcea`）。

本文件是 tasks.md §5「主 Agent 的 diff/断言/原始输出核对记录」，与 worker 自述的 `report.md` 分开保存。
下面每条都写明「我实际执行/读取了什么」，不接受 worker 转述。

## 1. diff 核对（`git diff 2f1dcea 79c3b54`）

改动范围与 `git show --stat` 一致：`doc_core.rs` +40/-13、`open_block.rs` +3、`document_typing.rs` +146，加证据文件。
未出现越权路径（无 `aline-markdown/**`、无 `scripts/**`）。

逐处确认：

| 位置 | 核对结果 |
|---|---|
| `DocError::PendingInput` | 新增变体带中文文档注释；`Display` 走 Debug 兜底、`source()` 落 `_ => None`，无破坏 |
| `undo()` 待定分支 | 先 `let mut captured = group.clone(); captured.after_selection = block.selection;` 再覆盖 `block.selection = before_selection`。捕获顺序正确——若反过来则捕获到的是 before 值 |
| `redo()` 拒绝点 | 位于 `ensure_ready()` 与组合检查之后、`next_version`（`version.next_draft()`）之前 |
| `redo()` 草稿分支 | 用 `group.after_selection`；先在 `probe`（克隆块 + `baseline_fragment.clear()`）上 `validate_selection`，通过后才写真实块 |
| `append_to_group()` | 追加 `group.after_selection = selection`（第 888 行处） |
| `start_group()` 两处构造 | 965/966 与 979/980 两个分支都设置 `after_selection = selection` |

## 2. 零副作用核对（不是只看测试通过）

`redo()` 的拒绝点在 `let next_version = self.version.next_draft()` **之前**，而 `next_version` 的构造即会推进草稿代号。
拒绝路径上唯一先执行的是 `ensure_ready()`，其实现（`doc_core.rs:784-790`）为：

```rust
fn ensure_ready(&self) -> Result<(), DocError> {
    let block = self.open.as_ref().ok_or(DocError::NoBlock)?;
    if block.baseline_revision != self.buffer.revision() { return Err(DocError::DraftBaselineConflict); }
    Ok(())
}
```

只读 `&self`，不改任何字段。`is_composition_active()` 同样只读。因此拒绝路径确实零副作用——这是读代码得出的，不是从测试断言推断的。

另核对 `set_selection()`（331-345）**不**更新 `after_selection`：这正是 F-05 能成立的原因——撤销时刻的选区取自活的 `block.selection`，所以「输入后移动/反向选区再 Undo」时捕获的是用户实际停留的选区，而不是输入末端。

## 3. 断言核对（读 `document_typing.rs` 新增 5 条，不是只看名字）

- 5 条都走 `common::assert_rejected_without_change` 或具体 `assert_eq!`；没有只断言 `is_err()` 的空断言。
- `assert_rejected_without_change`（`tests/common/mod.rs:278-292`）做**全状态前后比较**：`capture()` 覆盖 buffer_text / snapshot_text / snapshot_id / resources / mode / next_wait_id / parse_config / request / revision / draft_generation / undo / redo / nodes / buffer_anchor / buffer_head / open / composition / wait，然后 `assert_eq!(before, after)`。
- 拒绝原因用 `matches!(error, DocError::PendingInput)` 精确断言，不是 `|_| true`。
- F-05 三条分别断言 4:2（反向非折叠）、2:4（正向非折叠）、3:3（折叠），且 `assert_version` 精确到 revision/draft_generation，`assert_history` 精确到 undo/redo/nodes。
- 分支结算那条断言「提交只加 1 步、nodes 仍为 1」（旧节点被替换而非新增空步），随后 redo 以 `Buffer(NothingToRedo)` 拒绝且零副作用。

## 4. 原始输出核对（读落盘文件，不是读转述）

- `pre-fix-tests.txt`：4 段运行。前 2 段是 **E0599 编译失败**（测试引用了尚不存在的 `PendingInput`），worker 在 `report.md` §5.3 与 `handoff.md`「已知偏差」里如实标注，并把首轮 F-01 回归改成不依赖新变体的行为断言后重跑；第 3 段得到 `assertion failed: error.is_err()`（`tests/common/mod.rs:287`），第 4 段得到 `left: 4:4 / right: 4:2、2:4、3:3` 三条失败。**因此「先失败」是真实行为失败，不是拿编译错误冒充。**
- `post-fix-tests.txt`：我实际 grep 到 **62 行 `test result: ok`**、0 行 `FAILED`、0 行 `^error`，末行 `# exit: 0`。新增 5 条在 document_typing 目标与 workspace 目标中各出现一次并通过。
- `commands.txt`：12 行，含 argv/cwd/起止/exit。第 7 行 `cargo fmt --all --check` exit 1（3 处格式差异）被标为中间态，第 9 行复跑 exit 0——记录完整，未隐去失败。

## 5. 干净检出上的复跑

工作树里 `cargo test -p aline-markdown --test rewrite` 的 2 条 FAILED 属 R02 未提交的 pre-fix 用例，
若直接在工作树复跑 workspace 全量，会把 R02 的中间态混进 R01 结论。因此复跑须在 `79c3b54` 的**干净检出副本**上进行。

**本轮该复跑由独立 reviewer 执行（不是我）。** reviewer 实际在 `/tmp/aline-r01-head` 检出 `79c3b54` 并运行了
`cargo test --workspace --locked --offline`、`cargo test -p aline-ui --locked --offline`、
`cargo clippy -p aline-ui --all-targets -D warnings`，其原始输出归入 R01 审查证据。
我本人核对的是落盘文件（`post-fix-tests.txt` 的 62 条 ok / 0 failed / exit 0），以及对 `git diff` 与测试源码的阅读——
不把 reviewer 的运行记作我的运行。

## 6. 范围隔离说明

派发 R01 审查时工作树存在 ` M crates/aline-markdown/tests/rewrite.rs`（R02 正在写入的 pre-fix 失败用例）。
我向独立 reviewer 下达了范围澄清：该文件与那 2 条 FAILED 不得计入 R01；R01 只以 `crates/aline-ui/**` 与 R01 证据为准；
并明确禁止 reviewer 使用 `git stash/checkout` 等会动到他人文件的命令，改用干净检出副本验证。

## 7. 未由我单独验证、留给独立 reviewer 的点

- T-8-17「受检拒绝」是否优于「新分支协议结算」的规范裁决（我倾向拒绝符合 §9.3 文字，但这是裁决项，不由实现方自定）。
- `DocError::PendingInput` 作为新增公开变体对宿主 `match` 的影响面（仓内无穷尽匹配，宿主侧未接线）。
- 既有 8.2-15/16/18/19 用例在「撤销时刻选区 = 输入末端」前提下与新实现的等价性（worker 声称等价，我读了代码认为成立，但未逐条重算期望）。
