# 6.5 记录：有限 HTML 与保护子区间

日期：2026-10-31。状态：`cargo test -p aline-markdown --test html --test protection` 通过（html 20 例、protection 6 例）。本文只记录 §11.2 有限 HTML 子集与保护范围的无头行为，不宣称有限 HTML 已完整、不宣称 AC-09 或 V-07 通过。

## 1. 支持标签与样式

支持标签 `b/strong/i/em/s/del/strike/u/mark/code/kbd/a`（另含 `span/thead/...` 等结构化子标签）映射为样式并隐藏标记；嵌套标签样式合并；交叉嵌套按栈弹出；未闭合的支持标签保留样式且不 panic。

`a` 只在 href 为 `#`、`/`、`http://`、`https://`、`mailto:` 时获得链接样式（`links_require_allowed_href_schemes`）。

## 2. 过滤集合

`script/style/iframe/title/textarea/xmp/noembed/noframes/plaintext`（并保留 `object/embed/form/button/svg/math`）整体按字面文本显示：内部原始文本不解析为标签、不继承样式；未闭合时把剩余源码全部字面输出，不吞源码。

复核遗留项 F2-1 指出的两个缺陷已修：

- 过滤集原先缺少 `title/textarea/xmp/noembed/noframes/plaintext`，现补齐。
- 过滤判定原先在 void 判定之后，`input/link/meta/base` 会抢占隐藏路径；现过滤判定先行。

**更正（复核 N-F3-1）**：上面第二条对 `input/link/meta/base` 是 **no-op**。过滤集与 void 集（`img/hr/input/meta/link/source/area/base/col`）不相交，这些标签从未进入过滤分支；把过滤判定移到 void 判定之前不改变它们的行为，它们仍走 void 分支整体隐藏。`tree/html.rs:330` 的同句注释已按此更正。实际行为与处置表“非过滤集 void 标签不擅自改、作为 V-07 待定项记录”一致。

## 3. 空白折叠与块级合成换行

- HTML 文本上下文中连续 `空格/tab/\n/\r` 折叠为一个空格；折叠空格为 `Aligned` 片段，源码范围覆盖全部被消费的空白（含软换行与被丢弃的事件间缩进）。
- 块首/块尾/换行处的欠写空白丢弃，不产生纯空白无来源片段。
- 块级标签 `div/p/section/article/header/footer/ul/ol/li/h1–h6/table/tr/td/th/blockquote/pre` 边界产生 Synthetic `\n`，锚点为标签起点；构树侧去重，`<br>` 行为不变。

因此 `<div>a</div><div>b</div>` 绘制为 `a\nb\n`，不再是原先的 `ab\n`。

## 4. 实体与零可见

`&amp;#65;`、`&amp;#x1F600;`、`&amp;NotEqualTilde;` 解码为 `A`、`😀`、`≂̸`，均为 `Aligned` 并回指完整实体源码。仅标签零可见文字（`<b></b>`、`<i> </i>`）不绘制任何文本、无样式，源码由 Hidden/Protected 覆盖。

## 5. 保护子区间

`tests/protection.rs` 的 6 个用例：

| 用例 | 验证 |
|---|---|
| `front_matter_and_toc_are_protected_independently` | Front matter 与 `[toc]` 各自独立保护 |
| `adjacent_footnote_references_stay_separate_subranges` | 相邻脚注引用是独立子区间，不扩到正文 |
| `inline_html_tags_are_protected_per_tag` | 行内 HTML 逐标签保护 |
| `html_block_is_protected_as_one_range` | HTML 块作为单范围保护 |
| `literal_code_does_not_trigger_protection_or_front_matter` | 代码区内的 `---`/`[toc]`/`[^1]` 不触发保护 |
| `raw_fallback_protects_the_whole_source` | 整源回退保护整源 |

HTML 原文语义不因展示成段落而消失：保护范围与映射片段分开记录，样式区间只覆盖可见文字。

## 6. 已调整的既有断言

`entities_inside_html_keep_aligned_mapping` 的 `styled.len()` 由 3 改为 5：空白折叠新增两个继承粗体的折叠空格片段。其 drawn `"a & b"` 与实体片段 `5..10 → 2..3 Aligned` 断言不变。测试内已注释说明原因。

## 7. 定向证据

| UTC | 命令 | 退出 | 日志 |
|---|---|---|---|
| 2026-10-31T08:48:13Z | `cargo test -p aline-markdown --locked --offline --test html` | 0 | `step-6.5/targeted-tests/html.stdout.log`（11 passed） |
| 2026-10-31T08:48:13Z | `cargo test -p aline-markdown --locked --offline --test protection` | 0 | `step-6.5/targeted-tests/protection.stdout.log`（6 passed） |
| 2026-10-31T10:32:02Z | `cargo test -p aline-markdown --locked --offline --test html` | 0 | `leader-verify-round-2/html-after-nf32.stdout.log`（21 passed，含 N-F3-2 现状用例） |

`08:48` 记录是修复前快照（html 11 例）。F2-1/F3-5 补入 9 个用例后为 20 例，修复后证据见 `step-6.8/targeted-tests-post-fix/html.stdout.log`；round-2 复核 N-F3-2 补 1 个现状记录用例后为 21 例。

## 8. 未冻结/未覆盖（不得写成“有限 HTML 已完成”）

- **未支持的非 void 标签（复核 N-F3-2）**：`<foo>a   b</foo>` 当前绘制为 `"<foo>a   b"`——开标签按字面可见（Exact，`0..5`）、闭标签隐藏（Hidden，`10..17`）、内部连续空白不折叠；而支持标签 `<span>e   f</span>` 绘制为 `"e f"`（两侧标签都隐藏、空白折叠）。该不对称由 `unsupported_non_void_tags_record_the_v07_pending_asymmetry`（`tests/html.rs`）固定为**现状记录**，不表示口径已冻结；§11.2 是否统一处理属 V-07 待定。
- **非过滤 void 标签**（`input/link/meta/base` 等）仍按“不支持标签 → 标签隐藏、无可见内容”处理；§11.2 尚未冻结其展示口径，代码中已标 V-07 待定。
- 空白折叠与块级合成换行的口径本身也待 V-07 冻结；本组只记录当前实际行为与金样。
- 未验证真实 UI 渲染、HTML 块在视图中的灰色只读呈现。
- 未覆盖嵌套过滤标签与块级标签交错的全部组合、属性解析的完整 HTML 语法。
- 未测量 HTML 密集文档的性能；属 V-05。
