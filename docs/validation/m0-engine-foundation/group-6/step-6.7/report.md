# 6.7 记录：全量解析基线与头尾复用

日期：2026-10-31。状态：`cargo test -p aline-markdown --test reuse` 通过；复用命中与安全回退分别记录。本文只记录无头复用语义，不宣称局部解析优化、性能收益或 V-05 通过。

## 1. 复用模型

`aline-markdown` 的 `reuse` 在**全量解析**基线上做头尾块配对，不是局部 Markdown 解析：

- 旧、新两份 `Parsed` 逐块比较：头部连续相同块与尾部连续相同块可配对，中间变化区不配对。
- 配对后旧块的绝对范围通过 `translate` 重定位到新快照；旧绝对范围直接对新快照使用必须失败。
- 差分比较不只比文本：还比较块种类、重定位后的范围、投影片段来源与精确性、保护范围、样式范围；M0 公开 API 不暴露链接 href，导航只比较标题的 kind/text/range。

## 2. 实际命中

| 用例 | 实际结果 |
|---|---|
| `insertion_before_rebases_matching_suffix_blocks` | 前文插入后，后缀相同块 3 对配对成功 |
| `edit_inside_one_paragraph_keeps_only_outer_blocks` | 单段内编辑只保留外层块 2 对 |
| `unrelated_documents_reuse_nothing` | 不相关文档 0 对 |
| `plain_block_quote_text_still_reuses` | 纯文本块引用不被误拒，仍可复用 |

这些是**实际命中**记录，不是“全部回退冒充优化”。也没有把复用当作性能证明：本组不测耗时。

## 3. 安全回退（拒绝条件）

`global_dependency` 按顺序检查，任一命中即整篇拒绝复用：

| 依赖 | 判定 | 拒绝 |
|---|---|---|
| Front matter | `parsed.front_matter().is_some()` | `FrontMatter` |
| 顶层 List / CodeBlock / FootnoteDefinition / HtmlBlock / Raw | 块种类 | `List` / `Fence` / `Footnote` / `Html` / `RawFallback` |
| 保护范围 | `FrontMatter`/`Footnote*`/`Toc`/`Html`/`RawFallback` | 对应变体 |
| 引用定义 | 任一行（剥离容器前缀后）以 `[` 开头且含 `]:`，或标签跨行且闭 `]` 后紧跟 `:` | `ReferenceDefinition` |
| 围栏 | 任一行（剥离容器前缀后）以 ` ``` ` 或 `~~~` 开头 | `Fence` |

关键点是**容器前缀剥离**：复核遗留项 F2-2 指出原实现只看行首，`> [x]: /u` 与块引用/列表内的围栏会被漏掉。修复后 `strip_container_prefixes` 先剥离至多 3 个前导空格，再循环剥离 `>` 与 `-`/`*`/`+`/`1–9` 位数字加 `.`/`)` 的列表标记（标记后须空白或行尾），每次剥离后再允许至多 4 格填充/制表符。实现 O(行数)、零分配、只做字节切片。

**跨行标签（复核 N-F2-1）**：原 `has_reference_definition` 逐行判断，`[foo` 行不含 `]:`、续行 `bar]: /u` 不以 `[` 开头，因此 `[foo\nbar]: /u` 整体漏检；删除该定义后可复用出可见文本 `"foo bar"`，而全量重解析为 `"[foo bar]"`。修复后改为逐行字节状态机：`[` 置开标签态、`]` 关闭；跨行的闭 `]` 后紧跟 `:` 即命中；空行重置状态；`\[`/`\]` 按转义跳过一个字节。复杂度仍是 O(行数)、零分配。

新增反例（F2-2 处置）：

| 用例 | 验证 |
|---|---|
| `block_quote_reference_definition_changes_refuse_reuse` | `> [x]: /u` 目标改变后拒绝复用 |
| `block_quote_fence_changes_refuse_reuse` | 块引用内围栏内容变化后拒绝 |
| `list_global_dependencies_refuse_reuse` | 列表容器内的全局依赖拒绝 |
| `html_and_footnote_refuse_reuse_in_both_directions` | HTML 与脚注两个方向都拒绝 |
| `config_change_and_raw_fallback_refuse_reuse` | 配置代次变化与整源回退拒绝 |
| `old_absolute_ranges_fail_against_new_snapshot` | 旧绝对范围对新快照失败 |
| `reused_blocks_match_full_reparse_mapping_protection_and_navigation` | 复用块与全量重算在映射/保护/导航上一致 |

跨行标签反例（N-F2-1 处置）：

| 用例 | 验证 |
|---|---|
| `multiline_reference_definitions_refuse_reuse` | `[foo\nbar]: /u` 存在时整篇拒绝复用 |
| `deleting_multiline_definition_refuses_reuse` | 删除跨行定义后仍拒绝，不再复用出 `"foo bar"` |
| `changing_multiline_definition_target_refuses_reuse` | 跨行定义目标改变后拒绝 |
| `ordinary_multiline_paragraph_text_still_reuses` | 普通跨行段落文字不被误拒 |
| `multiline_label_without_definition_still_reuses` | 跨行标签但无定义（无 `]:`）不被误拒 |

保守性取舍：顶层 List/CodeBlock 一律拒绝（宁多拒绝不误复用）；普通块引用文本不拒绝；制表符填充按 ≤4 处理，属过度拒绝的安全方向。跨块 Aligned/Synthetic 片段不做逐对比较，而是直接报错——即“无法证明一致就不复用”，同样偏保守。

## 4. 未覆盖

- **未实现局部解析优化**：复用只省去逐块比较的工作，不减少解析本身。
- 未测量复用带来的耗时收益或命中率分布；属 V-05。
- 导航比较只覆盖标题；链接 href、脚注编号跳转、目录锚点未进入 M0 公开 API。
- 跨块 Aligned/Synthetic 片段直接拒绝比较，因此这类文档的复用收益为 0。

## 5. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T08:48:13Z | `cargo test -p aline-markdown --locked --offline --test reuse` | 0 | `step-6.7/targeted-tests/reuse.stdout.log`（7 passed） |
| 2026-10-31T10:19:53Z | `cargo test -p aline-markdown --locked --offline --test reuse` | 0 | `step-6.7/fix-n-f2-1/targeted-reuse.stdout.log`（18 passed） |

`08:48` 记录是修复前快照（7 例）。F2-2 补入 6 个用例后为 13 例，修复后证据见 `step-6.8/targeted-tests-post-fix/reuse.stdout.log`；round-2 复核 N-F2-1 再补 5 个跨行标签用例后为 18 例，证据见 `step-6.7/fix-n-f2-1/`，调度者复跑见 `leader-verify-round-2/full-crate.stdout.log`。
