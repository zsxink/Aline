# 7.2 记录：成功/无变化/版本与前提改变、预算与重读拒绝

日期：2026-10-31。状态：`cargo test -p aline-edit --test goldens` 6 例、`--test refusals` **16 例**通过（修复前 7 例 → round-1 复核 F3-1 后 15 例 → round-2 复核 N-F2-1 后 16 例，见 §6 与 `step-7.4/report.md` §9）；validate 侧 3 例、consume 1 例通过。本文记录 7.2 的拒绝矩阵、完整状态不变证据与复验协议，不宣称 V-07、AC-04 或 M0 出口通过。

## 1. 拒绝矩阵（金样逐项）

金样 `step-7.1/goldens/edit-goldens.tsv` 的 26 个 plan 用例覆盖 7.2 要求的全部类别：

| 类别 | 金样用例 | 触发点 | 拒绝值 |
|---|---|---|---|
| 成功 | `source_replace_single`、`source_batch_three`、`format_strong_toggle`、`format_strike_toggle`、`allow_source_footnote`、`allow_source_html_raw`、`allow_visual_body_beside_footnote` | 合法替换/格式 | `Changed` |
| 无变化 | `source_identity_replace`、`format_strong_already` | 替换文本与原文相同；两端已有标记 | `Unchanged(ReplacementIdentical)` / `Unchanged(AlreadyFormatted)` |
| 错工作版本 | `refuse_wrong_version`（validate 场景） | 计划后对快照施加 0..0 插入 `"X"` 再 validate | `WrongVersion` |
| 错投影 | `refuse_wrong_projection`（validate 场景） | 同一快照重新 parse 后 validate | `ProjectionMismatch` |
| 错选区 | `refuse_wrong_selection`（validate 场景） | 计划后改选区再 validate | `SelectionMismatch` |
| 错预期片段 | `refuse_expected_fragment`、`refuse_visual_expected_fragment` | `expected` 与源不符 | `ExpectedFragmentMismatch { range }` |
| 越界 | `refuse_out_of_bounds` | 6..99 超过 11 字节 | `OutOfBounds` |
| 反向区间 | `refuse_reversed_range` | 10..6 | `ReversedRange` |
| 半字符端点 | `refuse_not_char_boundary` | `"a😀b\n"` 的 0..2（emoji 内部） | `NotCharBoundary` |
| 预算 | `refuse_budget_patch`、`refuse_budget_text`、`refuse_budget_parse` | 补丁数 0、文本预算 1、解析源预算 0 | `BudgetExceeded { kind }` |
| 重读失败 | `refuse_hidden_link_target_lost`、`refuse_visible_text_changed`、`refuse_format_across_blocks` | 隐藏链接目标丢失；`&amp;` 渲染为 `&`；跨块格式 | `RereadFailed { reason }` |

## 2. 最小反例：可见文字相同、隐藏链接目标丢失

金样 `refuse_hidden_link_target_lost`：

- 源 `"[x](https://e)\n"`（15 字节），选区 15..15，Visual `Replace` 0..14 `"[x](https://e)"`→`"[x]"`。
- 候选绘制文本仍是 `x`，但隐藏源码 `(https://e)` 被删除。
- 结果：`RereadFailed { reason: HiddenSourceChanged }`。

这条正是任务 7.2 要求的最小反例：只比较可见文字会误放行。`verify` 的 `Visible` 契约先比较隐藏语义、再比较可见文本，两个维度都独立。

## 3. 完整状态不变

`crates/aline-edit/tests/refusals.rs` 的 7 个用例在真实 `StringBuffer` 上比较 `State { text, anchor, head, revision, undo_depth, redo_depth, nodes }` 前后相等：

| 用例 | 覆盖 |
|---|---|
| `refusal_keeps_pending_typing_group_open` | 待定组不因拒绝被封口；拒绝后继续输入仍只形成 1 个历史节点 |
| `refusal_keeps_redo_branch_and_history_intact` | 拒绝不清 redo；redo 深度保持 |
| `refusal_keeps_composition_state_untouched` | 组合中拒绝不改变状态 |
| `stale_plan_wrong_version_does_not_touch_buffer` | 旧计划 validate 拒绝后状态不变 |
| `visual_multi_edit_batch_refused_without_state_change` | Visual 多补丁拒绝（`UnsupportedVisualBatch`）后状态不变 |
| `identity_replace_creates_no_history_step` | 相同替换返回 `Unchanged`，不产生历史步、不产生空步 |
| `successful_plan_and_validation_leave_buffer_untouched` | 成功计划与 validate 都不写真实缓冲 |

`goldens.rs` 的 `golden_validate_*` 三例（3 passed）另在 plan 层验证 `WrongVersion`/`ProjectionMismatch`/`SelectionMismatch` 三种旧计划失效路径：validate 重新核对 revision、快照身份、投影身份与选区，不沿用旧范围。

**复核 F3-1 指出的缺口（如实记录）**：上述 7 例没有用真实 `StringBuffer` 覆盖 `BudgetExceeded`、`RereadFailed`、`ProtectedRange`、`HtmlBoundary`，也没有 `AlreadyFormatted` 的真实缓冲空步测试；`protection.rs` 与 `goldens.rs` 是 plan 级。修复与补测见第 6 节（修复后追加）。

## 4. 复验协议（计划变化后重验，不套旧范围）

`validate_change` 的顺序：待定输入 → 组合 → 模式 → revision → 快照身份 → 投影身份（快照与投影双绑）→ 选区合法性 → 选区与 `before_selection` 相等 → 批次在当前快照上重新校验 → 逐条期望片段重读 → Visual 权限 → 候选重建与同一复读契约 → 目标选区在候选上合法。任一失败返回拒绝，成功结果仍只是可消费凭据，不发布。

`ChangePlan` 绑定 `before_revision`、`before_snapshot`、`before_projection`、`before_selection`、期望片段、批次、目标选区、复读契约、解析配置与预算，因此旧计划在新文本上无法通过。

## 5. 定向证据

| UTC | 命令 | 退出 | 用例 | 日志 |
|---|---|---|---|---|
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test goldens` | 0 | 6 passed | `step-7.1/targeted-tests/goldens.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test goldens golden_validate` | 0 | 3 passed、3 filtered | `step-7.2/targeted-tests/validate-reread.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test goldens golden_consume` | 0 | 1 passed、5 filtered | `step-7.2/targeted-tests/consume.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test refusals` | 0 | 7 passed | `step-7.3/targeted-tests/refusals.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline` | 0 | 17 passed | `step-7.4/targeted-tests/full-crate.stdout.log` |

**字面命令（复核 F3-6，已补跑）**：任务 7.2 写的是 `cargo test -p aline-edit refusal`。原实现轮只跑了 `--test refusals`（整目标），字面命令当时未执行。round-2 复核后已补跑字面命令，2026-10-31T10:55:15Z 退出 0：名字过滤同时命中 `goldens::golden_contains_all_required_refusal_classes`（1 passed、5 filtered）与 `refusals` 目标的 7 个含 `refusal` 的用例（7 passed、9 filtered），因此“名字过滤”弱于 `--test refusals` 的整目标 16 例。补跑日志在 `step-7.4/fix-f3-3-f3-6/`，命令与退出码记录在同目录 `commands.log`。

## 6. 修复后（F3-1 真实缓冲拒绝状态）

2026-10-31 修复后，`tests/refusals.rs` 由 7 例扩到 15 例（round-2 复核 N-F2-1 后为 16 例），新增 8 例全部在真实 `StringBuffer` 上比较 `State { text, anchor, head, revision, undo_depth, redo_depth, nodes }` 前后相等，并额外验证拒绝后 redo 分支仍可消费（`buffer.redo()` 后文本为“原文 + `Q`”，`redo_depth` 归 0）：

| 新增用例 | 覆盖的拒绝 | 断言要点 |
|---|---|---|
| `budget_exceeded_refusal_keeps_buffer_state` | `BudgetExceeded { kind: PatchCount }` | 补丁预算 0 时拒绝；状态不变；redo 分支完好 |
| `reread_failure_refusal_keeps_buffer_state` | `RereadFailed { HiddenSourceChanged }` | 隐藏链接目标被删时拒绝；状态不变；redo 完好 |
| `protected_range_refusal_keeps_buffer_state` | `ProtectedRange { kind: FootnoteReference, range: 7..11 }` | 脚注引用被触及时拒绝；状态不变；redo 完好 |
| `html_boundary_refusal_keeps_buffer_state` | `HtmlBoundary { range: 2..5 }` | HTML 块内格式命令拒绝；状态不变；redo 完好 |
| `already_formatted_creates_no_history_step` | `Unchanged(AlreadyFormatted)` | 不产生历史节点；`undo_depth`/`redo_depth` 均不变 |
| `hidden_fragment_internal_whitespace_change_is_refused` | `RereadFailed { HiddenSourceChanged }`（F2-2） | `[x](https://e "a b")` 删除标题内空格 16..17 被拒绝；状态不变 |
| `visual_replace_range_is_command_authority` | 成功计划（F2-1 协议） | 选区 0..0、命令范围 6..10 可成计划；目标 10..10；预览 `alpha BETA`；真实缓冲不变 |
| `visual_replace_validates_bound_selection_only` | `validate_change` 语义（F2-1 协议） | 只校验“当前选区 == `before_selection`”，不校验范围等于选区；真实缓冲不变 |

因此第 3 节记录的 F3-1 缺口（缺 `BudgetExceeded`/`RereadFailed`/`ProtectedRange`/`HtmlBoundary` 真实缓冲用例、缺 `AlreadyFormatted` 空步用例）已补测；修复前 7 例、修复后 15 例与 round-2 后的 16 例都保留在证据中（16 例日志见 `step-7.4/fix-n-f2-1-n-f1/refusals.stdout.log`）。

修复后执行（`group-7/commands-post-fix.log`，10:00:21Z–10:00:22Z，实现者）：refusals 15、goldens 6、protection 4、全 crate 25，clippy/fmt 退出 0。调度方独立复跑（`step-7.4/leader-verify-post-fix/`，10:04:14Z–10:04:23Z）：同样 15/6/4/25，clippy 与 fmt 退出 0；统一工作区门禁 19/19 退出 0、`cargo test --workspace` 合计 195 passed（详见 `step-7.1/report.md` 第 7.2 节）。round-2 复核 N-F2-1 后再补 1 例，全 crate 为 26 passed（0+6+4+16），clippy 与 fmt 退出 0（`step-7.4/fix-n-f2-1-n-f1/`，10:52Z–10:53Z）。

金样未回填，SHA-256 仍为 `cf2504f2aa6c107021118ce0bb8bd0e5d32b1f65842a00fb8d918ce1d3dec743`。

## 7. 未覆盖与不宣称

- 未覆盖：真实 UI/DocCore 消费者、IME、Windows/Linux。
- 未覆盖：`BudgetExceeded`/`RereadFailed` 在真实缓冲上的状态（修复前），见第 3 节与第 6 节。
- 未实现（M0 范围外）：M3 全量命令、全文格式化、查找替换事务。
- 不宣称 V-07、AC-04 或 M0 出口通过。
