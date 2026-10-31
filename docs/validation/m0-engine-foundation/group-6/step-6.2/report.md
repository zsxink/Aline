# 6.2 记录：RewriteMap 复合、控制字节与原文回退

日期：2026-10-31。状态：`cargo test -p aline-markdown --test rewrite` 通过。本文只记录前置改写与映射层的无头语义，不宣称 UI 编辑、V-07、AC-09 或 M0 出口通过。

## 1. 为什么需要前置改写

Front matter 不由解析器原生识别：本 crate 在解析前把文首成对的 `---`/`+++` 区块替换为等长以外的占位，使后续解析不把其中的 YAML 当作 Markdown 标题/列表，同时保留回映到原始源码的能力。为此需要逐级 `RewriteMap`：每一级记录“改写后文本 → 上一级文本”的编辑，复合后得到“改写后文本 → 原始源码”。

## 2. 覆盖的语义

`tests/rewrite.rs` 共 9 个用例（`--test rewrite` 输出 9 passed）：

| 用例 | 验证 |
|---|---|
| `front_matter_is_hidden_in_parse_copy_and_maps_back_to_source` | Front matter 在解析副本中被隐藏，回映到原源码的字节范围正确 |
| `multi_edit_map_preserves_each_source_segment` | 单级多处编辑后每段来源独立可回映，不只验证可见字符 |
| `maps_compose_through_two_rewrite_levels` | 两级改写复合后仍逐段回映到原始源码 |
| `control_bytes_do_not_collide_with_synthetic_markers` | 正文中的 U+0001～U+0008 不被误认作合成标记 |
| `reversed_overlapping_and_non_boundary_edits_are_rejected` | 逆序、重叠、非 UTF-8 边界的编辑被拒绝，不静默 clamp |
| `parse_budget_failure_falls_back_to_original_text` | 重写预算超限时回退原文，且 `stats().budget_hits() == 1` |
| `pre_cancelled_rewrite_returns_cancelled_without_text` | 预取消的改写返回 `Cancelled`，不产出文本 |
| `cancellable_rewrite_stops_between_edits` | 取消在两处编辑之间生效（计数式检查器，确定性） |
| `pre_cancelled_front_matter_rewrite_returns_cancelled` | Front matter 改写入口同样可取消 |

后三个用例与预算断言由复核遗留项 F2-3 补入。

## 3. 合成围栏与映射失败回退

任务 6.2 要求的“合成围栏”在 `tests/diagnostics.rs` 的 `synthetic_fence_maps_to_empty_source_anchor_not_real_source` 中验证：在 `"x\n"` 前插入 `"```\n"` 后，重写文本为 `"```\nx\n"`、2 个段；`to_source(0..4)` 得到 `0..0` 且精度为 `Synthetic`；原样片段为 `Exact`；跨段与整段查询降级为 `Synthetic` 而非伪称 `Exact`。这说明合成片段有来源锚点，但不假装与源码双射。

映射失败有两个不同出口的反例，两者不能混为一谈（复核 N-F3-4）：

- `tests/rewrite.rs::parse_budget_failure_falls_back_to_original_text`：`max_segments=0` 时解析仍返回 `Parsed`，绘制文本等于整段原文、块种类为 `Raw`、诊断 1 条——这是**片段数超限的整源回退**，断言“原文 + 诊断”。
- `tests/hostile.rs::rewritten_budget_failure_returns_budget_error_not_partial_text`：`max_rewritten_bytes=2` 时返回 `Err(ParseError::BudgetExceeded)`，**不是**回退，也不产出部分改写文本。该测试断言的是错误，不是整段原文。

## 4. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T08:48:11Z | `cargo test -p aline-markdown --locked --offline --test rewrite` | 0 | `step-6.2/targeted-tests/rewrite.stdout.log`（6 passed） |

`08:48` 记录是修复前快照（6 例）。补入取消与预算断言三例后为 9 例，修复后证据见 `step-6.8/targeted-tests-post-fix/rewrite.stdout.log`。

## 5. 未覆盖

- 取消是协作式的：`pulldown-cmark` 内部事件生成与逐字节重写循环内部不可中断。
- 未实现局部解析优化；复用路径见 6.7。
- 未验证真实 UI 编辑下的改写竞态（属 V-07/V-08）。
