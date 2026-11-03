# M1 只读夹具

这些输入由 `tests/readonly_presentation.rs` 实际读取。五组 `.drawn` 文件是独立的展示文本预期；测试另外验证块与 Gap 无缝覆盖整个源码、源码端点都在 UTF-8 边界上。`line-endings.md` 含 BOM、CRLF、孤立 CR、无尾换行，`empty.md` 是零字节文件。

## 语义裁决矩阵

| 最小样例/维度 | 当前观察 | 独立预期与状态 | 证据入口 |
|---|---|---|---|
| `a\nb`（SoftBreak） | 解析投影含带来源的换行 | M1 裁决为保留可见换行并保留精确来源；窗口布局软折行不新增字符 | `tests/mapping.rs` 的 SoftBreak 金样；真实布局见 M1 virtualization 证据 |
| `a  \nb` / `a\\\nb`（HardBreak） | 解析为有来源硬换行 | 硬换行必须区别于布局软折行，后者不增加源码或绘制字符；像素布局实验待 UI | `tests/mapping.rs` |
| `<br>` | HTML 适配器生成有锚点的合成换行 | 合成字符不得伪装成源码字节 | `tests/html.rs` 的 `<br>` 映射样例 |
| `<input>`、`<link>`、`<meta>`、`<base>`（同类 void 含 `img/hr/source/area/col`） | 修复前被隐藏；当前按原始标签显示；有限支持的 `<br>` 仍合成为换行 | 子集之外的 void 标签显示原始标签；受支持标签仍隐藏标签并展示内容 | `tests/html.rs`：`unsupported_void_tags_are_preserved_as_literal_source`、`br_tag_produces_synthetic_newline` |
| `<foo>a   b</foo>`、`<b><i>x</b>y</i>` | 修复前 `<foo>` 只显示开标签；当前整段原文显示。支持标签交叉闭合按扫描栈弹出 | 未支持容器整段按原文显示，标签及内部连续空白均保留；交叉闭合样式按扫描栈来源输出 | `tests/html.rs`：`unsupported_non_void_container_is_preserved_as_literal_source`、`cross_nesting_pops_through_outer_tag` |
| `<script>x</script>` | 过滤内容保持字面可见且受保护 | 过滤集文本不得执行、加载资源或产生网络副作用 | `tests/html.rs`、`tests/protection.rs` |
| 未闭合 `---` Front matter | 不识别为 Front matter，正文保留 | 不能吞掉首段 Markdown | `tests/spec.rs`：`unclosed_front_matter_is_not_swallowed` |
| `[TOC]`、重复标题 | TOC 识别忽略 ASCII 大小写并指向全部顶层标题，重复标题保留独立源码范围 | 目标按源码顺序；标题锚点展示策略未扩展为 slug 规则 | `tests/navigation.rs`：TOC 与重复范围用例 |
| `[^Note]` 与 `[^note]:` | pulldown-cmark 0.13.6 用 `UniCase`（ASCII 大小写等价）配对；旧导航二次比较曾丢失目标 | 索引必须保留已识别目标、重复目标按源码顺序；缺失定义不产生解析器引用事件 | `tests/navigation.rs`：ASCII、含非 ASCII 字符及重复目标回归 |
| 布局软折行 | 解析器与当前引擎没有窗口宽度输入 | 只影响布局，不增加源码/投影逻辑换行；真实像素行为未测 | M1 UI 尚未接线 |

以上 SoftBreak 和未知/void HTML 口径由用户于 2026-11-03 裁决并已有解析回归；这不表示 V-07 的其他项或真实像素命中已通过。仍未裁决的语义继续单列，不以实现现状代替用户决定。
