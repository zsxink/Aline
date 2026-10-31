# 组 6 Markdown 夹具

这些文件在实现前手工建立，用于组 6 的解析、重写映射、块/间隙、保护范围、HTML 与预算测试。金样不是由 `aline-markdown` 自身生成的，也不得在实现后用解析结果反向覆盖。

## 1. 文件

| 文件 | 覆盖点 |
|---|---|
| `basic.md` | front matter、标题、段落、强调/删除线/行内代码/链接、引用、列表、围栏代码、表格、脚注定义与引用、公式块、emoji shortcode、无尾换行 |
| `basic.blocks.tsv` | `basic.md` 的顶层块/间隙手工金样 |
| `front_matter_unclosed.md` | 未闭合 `---` 不得吞全文 |
| `front_matter_plus.md` | `+++` 成对 front matter |
| `literal.md` | 围栏代码中的 `---`、`[toc]`、`[^1]`、`<script>` 均按字面处理 |
| `gfm.md` | GFM 表格、删除线、自动链接、任务列表、GitHub Alert |
| `md11.md` | front matter、`[toc]`、脚注引用/定义、内联 HTML 保护 |
| `nested.md` | 嵌套列表与嵌套引用 |
| `spec.blocks.tsv` | 上述夹具的顶层块/间隙手工金样 |

## 2. 区间约定

- 偏移均为 UTF-8 字节半开区间 `start..end`。
- 顶层块尾不含块后的分隔换行；分隔换行计入 `gap`。
- front matter 含开闭标记行，不含标记行后的换行。
- 公式块 `$$ ... $$` 在 M0 的 dump 中记为顶层 `Math` 块。
- 行号与字节偏移由 `python3` 逐行读取原始字节得到，不依赖 Rust 解析器。

## 3. 当前状态

夹具与金样在组 6 实现前提交；对应的 Rust 测试与实现状态以组 6 各步骤证据为准。本文不声称 V-06、V-07、AC-09 或 M0 出口通过。
