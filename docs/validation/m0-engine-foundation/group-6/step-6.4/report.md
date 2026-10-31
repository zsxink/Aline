# 6.4 记录：映射片段、规范锚点与 UTF-16 统一换算

日期：2026-10-31。状态：映射金样 6 例已由测试直接消费（来源时点见 §2 的限定，不写“手写”）；`cargo test -p aline-markdown --test mapping` 通过。本文只记录无头映射语义，不宣称真实命中、IME、V-07、AC-09 或 M0 出口通过。

## 1. 片段关系

映射由片段序列构成，片段平铺绘制文本，不要求与源码双射：

| 精度 | 条件 | `source_at` |
|---|---|---|
| `Exact` | 绘制与来源等长 | 源起点加偏移 |
| `Aligned` | 两侧非空、长度不同 | 按 affinity 取源区间起点（Before）或终点（After） |
| `Hidden` | 绘制为空、来源非空 | 同 `Aligned`，取源区间起止 |
| `Synthetic` | 绘制非空、来源为空 | 取源锚点（合成位置） |

`drawn_at` 对称地按 affinity 取绘制区间起止。不要求全源码双射，也不靠 clamp/backscan 修补非法端点——越界、非字符边界、代理对中间、旧投影一律受检失败（`tests/mapping.rs` 的 `local_projection_rejects_sources_outside_its_range`、`stale_projection_and_snapshot_are_rejected`、`utf16_offsets_reject_surrogate_middles`）。

## 2. 金样与消费者

`step-6.4/goldens/mapping-goldens.tsv` 含 6 例：`entities`、`multichar_entities`、`soft_break`、`hard_break`、`br_newline`、`hidden_synthetic`。金样头注明相邻片段是否合并取决于文本事件边界：同一事件内相邻且精度/来源连续则合并，跨事件（例如标签隐藏后接可见文字）不合并。

**来源限定（复核 N-F3-5）**：本报告不再声称该 TSV 是“实现前手写”。可核对的事实只有：TSV mtime 08:52:17Z，早于消费者 `tests/mapping.rs`（09:31:07Z）与 round-1 复核（09:15:27Z），且复核后未被回填或改写；08:48 已存在修复前的解析器，因此不能排除该 TSV 由当时的实现生成。其内容已由独立 Python 脚本按文档模型重算（6 例、0 问题），但重算只证明内容正确，不证明生成方式。

该文件现在由 `tests/mapping.rs::mapping_goldens_tsv_matches_parser_output` 用 `include_str!` 直接读取并逐项断言绘制文本、每段绘制/来源区间与精度、以及每个规范锚点；不再只是证据文件。用例先断言 `cases.len() == 6`，任一值不符即失败。

复核遗留项 F3-1 的处置：该消费者在修复轮补入。

## 3. 补充覆盖

| 用例 | 验证 |
|---|---|
| `entities_and_escapes_map_as_aligned_segments` | `&amp;` 与 `\*` 转义的 Aligned 来源 |
| `multichar_and_numeric_entities_keep_drawn_width` | `&NotEqualTilde;`、`&#x1F600;` 的绘制宽度与来源区间 |
| `soft_and_hard_breaks_are_exact_or_aligned` | SoftBreak 与两空格 HardBreak 的来源 |
| `br_tag_produces_synthetic_newline` | `<br>` 产生 Synthetic 换行 |
| `hidden_and_synthetic_segments_anchor_without_pretending_bijection` | Hidden/Synthetic 混合的规范锚点 |
| `selections_preserve_direction_and_entity_alignment` | 正反向选区不丢方向 |
| `identity_roundtrip_covers_all_char_boundaries` | 恒等路径的逐字符边界往返 |
| `zwj_family_emoji_stays_exact_before_hidden_newline` | 19 字节 ZWJ 家庭 emoji → `Exact 0..18` + `Hidden 18..19`；UTF-16 偏移 8 映射到源 18；偏移 4 拒绝为 `MiddleSurrogate` |
| `repeated_text_keeps_distinct_segments_and_canonical_anchors` | `**a** **a**` 的同文重复保留独立片段与规范锚点 |
| `consecutive_spaces_stay_exact_and_trailing_newline_hidden` | 连续空白保持 Exact，尾换行 Hidden |
| `tab_and_combining_character_keep_utf8_boundaries` | tab 与组合附加符不拆 UTF-8 边界 |
| `local_projection_combines_utf16_offsets_with_source_ranges` | 局部投影把 UTF-16 偏移与源区间组合，越界/代理中间拒绝 |

后 5 个用例由 F3-1 补入。

## 4. 仓外探针交叉核对

`step-6.4/goldens-check/` 保存一个仓外探针（`probe-main.rs`、`gap-probe-main.rs`，独立 Cargo 工程，不改本仓代码），对金样之外的边界做了交叉核对：

| 样本 | 结果 |
|---|---|
| ZWJ 家庭 emoji（19 字节） | `Exact 0..18` + `Hidden 18..19` |
| 重复文字 | 4 个片段，锚点 0→2、1→5、2→8、3→9 |
| 连续两个空格 | `Exact 0..4` + `Hidden 4..5` |
| `\*x\*` 转义 | 绘制 `*x*` |
| tab 缩进 | `Exact 0..3` + `Hidden 3..4` |
| 组合字符 | `Exact 0..3` + `Hidden 3..4` |

这些值与 `tests/mapping.rs` 的断言一致，属于“独立工具复算同一期望”，不是由被测实现反向生成。

## 5. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T08:48:13Z | `cargo test -p aline-markdown --locked --offline --test mapping` | 0 | `step-6.4/targeted-tests/mapping.stdout.log`（10 passed） |
| 2026-10-31T09:10:14Z | 仓外探针 `cargo run --quiet` | 0 | `step-6.4/goldens-check/commands.log` |
| 2026-10-31T09:12:23Z | 仓外 Gap 探针 `cargo run --quiet` | 0 | `step-6.4/goldens-check/commands.log` |

`08:48` 记录是修复前快照（10 例）。F3-1 补入 6 个用例后为 16 例，修复后证据见 `step-6.8/targeted-tests-post-fix/mapping.stdout.log`。

## 6. 未覆盖与待冻结项

- **SoftBreak 口径未冻结**：当前实现把段内换行投影为带来源的换行（`soft_break` 金样为 `Exact` 段 + 尾 `Hidden`）。是否改为空格或其他展示口径须由 V-07 冻结；本组只记录当前实际行为。
- 真实 UI 命中、像素/候选窗、焦点、系统字素边界、IME 均未验证。
- 窗口软折行不进入映射；折行只属布局层。
- 未做跨块 Aligned/Synthetic 片段的逐对比较（复用差分对这类片段直接报错而不是比较，见 6.7）。
