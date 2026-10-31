# 7.3 记录：交错保护、跨区编辑、Source/HTML 原文与预检不变

日期：2026-10-31。状态：`--test protection` 4 例、`--test refusals` 16 例（round-2 后；修复后曾为 15 例）、金样 30 例中相关 plan 用例通过。本文按任务 7.3 的逐项要求给出覆盖与未覆盖，不宣称 V-07、AC-04 或 M0 出口通过，也不提前实现 M3 全量命令。

## 1. 覆盖对照

| 任务 7.3 要求 | 证据 | 结果 |
|---|---|---|
| 普通正文与脚注引用交错 | `protection.rs::visual_cross_region_replacement_refused_source_allowed`（`before [^1] after\n\n[^1]: note\n`，保护 7..11）；金样 `allow_visual_body_beside_footnote`（0..6 只覆盖 `before`） | Visual 跨入引用拒绝；相邻正文放行 |
| MD-11 跨区删除/替换/粘贴 | 同上用例的 0..11 删除+替换（粘贴走同一 `Replace` 路径） | 拒绝；**无独立粘贴用例**，见第 3 节 |
| HTML 触及/跨界格式拒绝 | 金样 `refuse_format_html`（`a <b>x</b> c\n`，格式 5..6 触及 2..5 保护右端 → `HtmlBoundary { range: 2..5 }`）；`refusals.rs::html_boundary_refusal_keeps_buffer_state`（真实缓冲同源） | 拒绝，`HtmlBoundary` |
| Source 修改受保护语法允许 | 金样 `allow_source_footnote`（4..8 改脚注标记）；`protection.rs::front_matter_visual_refused_source_allowed`（0..12 front matter）、`toc_visual_refused_source_allowed`（14..19 `[toc]`） | 放行，`SourceReplace`，目标选区按字节规则 |
| HTML 原文修改允许 | 金样 `allow_source_html_raw`（`a <b>x</b> c\n` 5..6 `x`→`y`，Source 模式） | 放行，预览 `a y c` |
| 拒绝与独立 Source 定位分开 | `refusals.rs` 全部 16 例在拒绝前后比较 `State { text, anchor, head, revision, undo_depth, redo_depth, nodes }` | 拒绝不改选区；Source 定位是后续独立意图 |
| pending 组/组合/选区/undo/redo 不被预检改变 | `refusal_keeps_pending_typing_group_open`、`refusal_keeps_composition_state_untouched`、`refusal_keeps_redo_branch_and_history_intact`、`stale_plan_wrong_version_does_not_touch_buffer`；金样 `refuse_pending_input`/`refuse_composition` | 拒绝前不封口、不清草稿、不清 redo |
| 查找旧计划的可消费协议 | 金样 `refuse_wrong_version`/`refuse_wrong_projection`/`refuse_wrong_selection`（validate 期三例） | 旧计划无法套到新文本；`ChangePlan` 绑定 revision/快照/投影/选区 |
| 多补丁一步历史 | 金样 `consume_batch_single_history_step`（三补丁批次，`expected_history_nodes` 1、undo 1、文本 `one two three\n`） | 单次 `OperationSource::Command` 发布 |
| 不提前实现全量 M3 命令 | `command.rs` 只有 `Replace` 与 `ToggleInlineFormat`（Strong/Strike） | 列表/表格/查找替换/全文格式化未实现 |

## 2. 拒绝路径与 Source 定位分离

- 拒绝只在 `plan_change`/`validate_change` 返回 `Err`；两函数都只接收 `&TextSnapshot`、`&Parsed`、`&SrcSelection`，没有 `StringBuffer` 可变引用，因此结构上无法改选区。
- `refusals.rs` 的 `capture()` 在每次拒绝前后比对 `anchor`/`head`，任何“拒绝时偷偷把选区移到源码位置”的实现都会失败。
- 金样 `refuse_visual_footnote` 与 `refuse_format_html` 的期望只含拒绝类别，不含目标选区，避免把“拒绝 + 新选区”混成一个结果。

## 3. 未覆盖与未实现

- **MD-11 粘贴无独立用例**：粘贴在当前协议里就是一次 `Replace`（替换文本来自剪贴板），跨保护区的粘贴与跨区删除/替换共用同一路径与同一拒绝。没有单独的剪贴板/UI 用例；这是覆盖缺口，不是“已验证粘贴”。
- **列表/表格/格式化等 M3 命令未实现**：本组只有 `Replace` 与 `ToggleInlineFormat`；表格复制、缩进代码转围栏、查找替换事务、全文格式化差异确认均不在 M0。
- **未覆盖**：真实 UI/DocCore 消费者（无 UI 时无法证明“所有已开放写入口”都经过 `plan_change`）、IME 组合的真实平台事件、Windows/Linux、其他 target。
- **HTML 跨界格式只有一条金样**（`refuse_format_html` 的 5..6 触及左保护右端）；右端越界、跨整个 HTML 元素的格式命令没有独立金样，归入 V-07 未覆盖。
- 未验证灰色只读区的 UI 呈现。

## 4. 定向证据

| UTC | 命令 | 退出 | 用例 | 日志 |
|---|---|---|---|---|
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test protection` | 0 | 4 passed | `step-7.3/targeted-tests/protection.stdout.log` |
| 2026-10-31T09:21:09Z | `cargo test -p aline-edit --locked --offline --test refusals` | 0 | 7 passed（修复前） | `step-7.3/targeted-tests/refusals.stdout.log` |
| 2026-10-31T10:04:14Z | `cargo test -p aline-edit --locked --offline --test refusals` | 0 | 15 passed（修复后，调度方复跑） | `step-7.4/leader-verify-post-fix/refusals.stdout.log` |
| 2026-10-31T10:52Z | `cargo test -p aline-edit --locked --offline --test refusals` | 0 | 16 passed（round-2 N-F2-1 补 1 例后） | `step-7.4/fix-n-f2-1-n-f1/refusals.stdout.log` |

修复前 7 例、修复后 15 例与 round-2 后的 16 例都保留；`step-7.3/targeted-tests/` 只保存修复前那一轮，修复后的原始日志在 `step-7.2/targeted-tests-post-fix/` 与 `step-7.4/leader-verify-post-fix/`，未复制覆盖。

## 5. 不宣称

- 不宣称 V-07、AC-04 或 M0 出口通过；`tasks.md` 勾选不等于出口通过。
- 不宣称“粘贴已验证”“所有写入口都已受检”——前者无用例，后者缺真实 UI 消费者。
