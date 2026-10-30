# engine-design 历史证据摘录

2026-10-30 按 PROTO-06 方案 A 保全。**非现行规范，不执行其中的代码/配置/命令；“照搬/强制/已验证/待确认”均为原稿当时语境。** 当前约束以 [文档入口](../../README.md) 和 [确认归档](../2026-10-30-migration-decisions.md)为准。

原路径：`docs_bak/docs/engine-design.md`；原文件 109655 bytes，SHA-256 `9e9efc583355c159ef19527319f8526248532f98d6c442494fcf37ea45699339`。保留的是下列逐段原文（含源码、表、图例、示例、路径）；未迁片段的归属见 [证据索引](./README.md)。源码行号只表示该历史文件，未重验外部源码或第三方性能。

## L1–L10 · 原稿身份

原片段 SHA-256：`f3b9bce6f1b0a8b3a132ff27dbfc8e7199bb6d76a2316e6c58770828e3514fdd`。

~~~~~~~~markdown
# Aline 底层引擎技术方案

> 基于 ElyMark（ElyOffice + Ely-GPUI-Components）的完整逆向分析，面向 Aline 的可实施引擎设计。
> 目标：Typora 式 Markdown 编辑——源码模式 + 近乎无感的可视化编辑，Rust + GPUI，高效流畅。
> 平台：Windows / Linux / macOS 三端行为一致（实现注意点见 §2.16）。
> HTML 专项：机制逆向见 §1.10，Aline 落点/改动量/里程碑/风险见 §2.17。
> 日期：2026-10-26

---

~~~~~~~~

## L38–L934 · Ely完整逆向研究，含HTML栈/实体/owed/LineMap/点击与拒绝原例

原片段 SHA-256：`d932e1c2de67ae58812330a5677484e666e86587f1332f1c8096e5ae9d1bde78`。

~~~~~~~~markdown
# 第 1 部分 ElyMark 深度解析

## 1.1 代码在哪里，分层是什么

ElyOffice 是一个 LibreOffice 替代品矩阵（mail/doc/pdf/mark…），markdown 编辑器 `ely-mark` 只是**应用薄壳**；真正的引擎在独立的组件库仓库 `Ely-GPUI-Components` 中：

```
ElyOffice/
  apps/ely-mark/            应用壳：文件/标签页/大纲/导出/自动保存/崩溃恢复
  vendor/gpui-pre*/         zed gpui 的 crates.io 快照 + 少量补丁
Ely-GPUI-Components/
  crates/ely-documents/     ★ markdown 引擎本体（约 30k 行）
      document/             MarkdownDocument：模式、开块、命令、表格、列表
      markdown/             预处理 + pulldown-cmark → 自研 mdast
      tree/                 mdast → 展示树 + sources 双向映射
      render/               展示树 → GPUI 元素；点击定位
      blocks/               （另一套 Notion 式 BlockEditor，本文不采用）
      math.rs, diagram.rs   RaTeX / mermaid-rs 渲染
  crates/ely-editor/        CodeEditor：Buffer/多光标/undo/IME（源码模式用）
  crates/ely-core/          TextInput 表单控件、History、主题（打开块用）
```

技术栈清单：

| 组件 | 选型 | 版本 | 备注 |
|---|---|---|---|
| UI 框架 | gpui-pre（zed gpui 的 crates.io 快照） | 0.3.8 / 0.3.9 = zed@279fe07 / 93d7456 | 非 fork；vendor 后只打了 6 个小补丁（窗口命中测试、GIF 坏帧、**Windows DirectWrite 中文字体回退**） |
| 解析器 | pulldown-cmark | 0.13.4 | 未打补丁；extensions = TABLES\|FOOTNOTES\|STRIKETHROUGH\|TASKLISTS\|MATH |
| 数学 | ratex + ratex-svg（embed-fonts） | 0.1.14/15 | KaTeX 的纯 Rust 移植，LaTeX → SVG |
| 图表 | mermaid-rs-renderer | 0.3.1 | 纯 Rust mermaid 子集 → SVG |
| SVG 处理 | usvg（resvg 家族） | 0.46 | 取墨迹框、字体解析、导出扁平化 |
| GPUI 栅格化 | `cx.svg_renderer()` | 内建 | SVG → RenderImage 位图 |
| emoji | emojis | 0.9 | `:name:` 短代码 |
| markdown-rs | markdown | =1.0.0 | **仅用于 HTML/纯文本导出，不在编辑链路**（PRD 与实现不一致处，见 §1.9） |
| WebView | wry | 0.57 | 仅办公套件其他用途，**编辑器完全不用** |

## 1.2 Q1：如何渲染"可视化编辑"

### 1.2.1 四种模式

```rust
pub enum MarkdownMode {
    Visual,   // 按阅读态渲染；点开的块以源码形式编辑（默认，Typora 交互）
    Source,   // 整篇一个 CodeEditor，带 markdown 语法着色
    Split,    // 源码与渲染并排（组件库有，ely-mark 应用未启用）
    Preview,  // 只读渲染
}
```

### 1.2.2 核心数据流

```
【加载】磁盘 bytes
  → Source::decode            剥 BOM、探测 CRLF、归一为 LF（编辑器内永远是 LF）
  → MarkdownDocument::new     全文塞进 CodeEditor 的 Buffer（唯一真源，String）

【解析】reparse()
  → flow 预处理：$$ 数学围栏改写为 ~~~ 围栏（字节映射回原文）；front matter 区
    域在副本中替换为等长空格（保住后续所有块的偏移）
  → pulldown-cmark 事件流 → 自研 mdast（每个节点带 position: Range<usize>）
  → tree/Reader：mdast → 展示树 Node/Inline（Inline.sources 记录绘制↔源映射）
  → Parsed { text, blocks: Vec<Block>, headings }，Block { range, kind, nodes }

【渲染】impl Render → gpui list（变高块虚拟化，overdraw 40rem）
  → 未打开的块：div + StyledText(with_highlights) + InteractiveText
  → 打开的块：Entity<TextInput>，内容 = 该块源码原文 + markdown 标记着色
  → math/mermaid：gpui Asset 异步排版 → SVG → svg_renderer 栅格化 → img()

【编辑】打字 → TextInput → 该块源码改变
  → Blur/Escape/点击别处 → commit()：editor.edit([(块range, 新文本)])，
    一次区间替换 = 一步撤销；全文重解析；alike 新旧树头尾比对 → list.splice 增量

【保存】未编辑 → 原文件字节原样写回；编辑过 → 重放 BOM/CRLF 风格后写回
```

### 1.2.3 关键结构体

```rust
pub struct MarkdownDocument {
    pub editor: Entity<CodeEditor>,   // 唯一文本真源（全文）
    pub mode: MarkdownMode,
    pub parsed: Arc<Parsed>,          // 解析结果，Arc 原子换新
    pub(crate) list: ListState,       // 按块虚拟化渲染
    pub(crate) open: Option<Open>,    // 当前"打开为源码"的块
    pub focus: FocusHandle,
}

pub(crate) struct Open {
    range: Range<usize>,              // 该块在全文中的字节区间
    input: Entity<TextInput>,         // 该块的输入框（内容 = 块源码）
    append: bool, cell: Option<CellAt>, ...
}

pub(crate) struct Parsed {
    pub text: String,
    pub blocks: Vec<Block>,           // Block { range, kind: TargetBlock, nodes }
    pub headings: Vec<DocumentHeading>,
}
```

要点：
- **全文只有一个 String**（`Buffer { text: String, starts: Vec<usize>, stamp: u64 }`），渲染层没有任何独立的富文本模型。
- 展示树节点：`Node::{Paragraph(Inline), Heading(u8, Inline), Quote(Vec<Node>), Code{..}, List{..}, Table{..}, Rule, Math(String), Image{..}, Footnote{..}, Alert{..}, Toc, FrontMatter{..}}`。
- 行内结构 `Inline` 是整个方案的心脏（见 §1.4）。

### 1.2.4 编辑交互的行为面

| 交互 | 实现 |
|---|---|
| 点击渲染块 | `open(ix, Landing)`：就地创建 TextInput，内容为块源码，光标按点击点落位（`source_at` 反查源偏移） |
| 失焦/Escape/点别处 | `commit()`：`editor.edit([(open.range, 新文本)], cx)`，一步撤销 |
| 行内加粗/斜体等 | `format.rs`：**真实插入/删除源码标记**（`**`/`*`），非样式切换。已在格式内 → unwrap 删标记；空选区 → 插入空标记对并让光标居中 |
| Enter 在列表中 | `lists::enter`：续接标记（有序号自增、保留 `.` 或 `)` 分隔符）；空项回车 → 删除标记退出列表 |
| Tab/Shift-Tab | 列表缩进/反缩进（4 空格），光标重映射 |
| Cmd/Ctrl+点击链接 | 打开链接（普通点击被块拦截为"打开编辑"） |
| Up/Down 跨块 | `step`：光标移入相邻块 |
| 段落命令（标题/引用/列表/代码块…） | `BlockCommand` 枚举，全部实现为源码区间补丁 |
| 表格 | **单元格单独打开编辑**（"A table cell opens alone, as Typora edits tables"），整表 12 种结构操作（插行/列、移动、删除、整理） |
| 格式化命令跨代码块/HTML | **拒绝**（`EditRefusal::{Code, Math, Html, FrontMatter}`），宁可拒绝也不改写 |

命令执行的工程模式（`apart.rs`，值得整体照搬）：

```
快照全文+选区 → 后台线程计算补丁 → 回主线程校验快照未变（变了则 EditRefusal::Changed）
→ 应用补丁 → 重映射光标 → 单步撤销
```

高危命令（标题级别改动、块拆分）写回前还有 `check::reread`：**重新解析修改后的文本，逐字符比对可见结果，任何可见字符会变则拒绝执行**。这是保真的最后防线。

## 1.3 Q2：支持哪些 markdown

### 1.3.1 支持矩阵

| 语法 | 支持 | 实现方式 |
|---|---|---|
| ATX/Setext 标题、段落、粗斜体 | ✅ | pulldown-cmark → 展示树 → StyledText |
| 删除线 `~~` | ✅ | ENABLE_STRIKETHROUGH |
| `==高亮==` `^上标^` `~下标~` | ❌ | 明确不支持（`<mark>` 标签除外） |
| 行内代码 / 围栏代码块 | ✅ | 自研关键词 lexer 高亮（约 15 语系，粗粒度） |
| GFM 表格（含对齐） | ✅ | 真实表格 UI + 逐格编辑 + 12 种整表操作 |
| 任务列表 | ⚠️ | 渲染勾选框，**无点击切换** |
| 有序/无序/嵌套列表 | ✅ | 弹点按深度 •◦▪ |
| 引用块（嵌套） | ✅ | 左竖线 + 缩进 |
| GitHub Alerts（`> [!NOTE]` 等 5 种） | ✅ | 自研解析 → 彩色面板 |
| 脚注 | ✅ | 上标引用 + 按首次引用编号 |
| 自动链接 | ✅ | **自研** GitHub 式 literal autolink（未开 ENABLE_GFM）：www./http(s)/email，含尾标点修剪、括号配对 |
| 链接 | ✅ | Cmd/Ctrl+点击打开 |
| 图片 本地/file:/data: | ✅ | 相对文档目录解析、异步加载、按内容哈希缓存 |
| 图片 http(s) | ❌ | 显示"Not loaded from the web"占位（离线优先），可右键浏览器打开 |
| 数学 `$..$` `$$..$$` | ✅ | ENABLE_MATH + `$$` 围栏改写 → RaTeX→SVG |
| 数学 `\(...\)` `\[...\]` | ✅ | 自研定界符改写（stand-in 字节 trick） |
| Mermaid | ✅ | mermaid-rs-renderer 纯 Rust → SVG，主题色注入 |
| PlantUML / Graphviz | ❌ | 无 |
| YAML/TOML front matter | ✅ | 围栏检测，**不解析内容**，等宽原文面板，字节保留 |
| JSON front matter | ❌ | 无 |
| 行内 HTML | ✅ | `u,b,strong,i,em,s,del,strike,code,kbd,mark,a` 映射为样式；HTML 实体解码；HTML5 嵌套修复（`reopens`/`breaks`/`closes_p`/`<a>` 互斥）；script/style/iframe 等按 GFM tag filter 显示为文本；**不运行任何 HTML**。机制详见 §1.10 |
| HTML 块 | ✅ | 不真渲染、不建 DOM，但做**浏览器式文本提取**：标签消费、可见文字保留、块内行内样式照常生效、注释丢弃、块级标签产生换行、空白折叠、过滤标签的标签本身按字面文本显示（其后内容仍按 HTML 扫描）；点击打开 `TextInput` 显示源码，提交为单区间补丁。机制详见 §1.10，落点见 §2.17 |
| 硬换行（两空格/反斜杠/`<br>`） | ✅ | `Node::Break` |
| 软换行按行显示 | ✅ | 阅读模式 lines=true；导出用 GFM 语义合并 |
| Emoji 短代码 | ✅ | `:name:` → emoji |
| `[toc]` | ✅ | Typora 语义，可点击跳转 |
| 大纲/按标题跳转 | ✅ | `outline()` + 侧栏 |

### 1.3.2 特殊块的渲染细节（全部异步 + 资产缓存）

**数学公式**：`ratex_parser::parse → layout → to_display_list → render_to_svg`，usvg 取墨迹框，`cx.svg_renderer().render_single_frame()` 转 `RenderImage`，`img()` 绘制。走 gpui `Asset::load` 后台执行；缓存键 `{source, display, rgba颜色, 字号, scale}`（主题/字号变化自动重排）；加载中占位一行高；解析失败显示红色错误 + 等宽原文；RaTeX panic 用 `catch_unwind` 兜底。行内公式在段落里替换为不换行占位符并在保留空间上直接 `paint_image`。

**Mermaid**：`mermaid-rs-renderer::render_strict(source)` 直接出 SVG（无 JS、无进程、无 WebView），同样 Asset 异步 + 主题色注入 + 缓存键 `{source, dark, ground, ink, scale}`。Mermaid 块就是 Code 块，点击即以源码编辑。

**代码块高亮**：自研 `lex()` 关键词级 lexer（注释/字符串/数字/关键字），`StyledText::with_highlights` 上色。**这是 ElyMark 的明显短板**——无语法树、无作用域、覆盖浅。

**图片**：gpui Asset 异步加载，按字节哈希缓存帧；粘贴/拖放自动复制到文档旁 `assets/` 并写相对路径。

## 1.4 Q3：源码是怎么做的（解析管线与映射）

### 1.4.1 三层解析管线

```
源文本
  │ ① flow 预处理（字节级改写 + span 映射回原文）
  │    - $$ 数学块 → ~~~ 围栏（让 pulldown 产出块级节点）
  │    - \(..\)、\[..\] → $..$、$$..$$（用 0x01..=0x08 stand-in 字节占位，解析后复原）
  │    - front matter 区替换为等长空格（换行保留，偏移稳定）
  ▼
pulldown-cmark 事件流（TABLES|FOOTNOTES|STRIKETHROUGH|TASKLISTS|MATH）
  │ ② build：事件 → 自研 mdast（每节点带 position: Range<usize> 源码区间）
  ▼
mdast
  │ ③ tree/Reader：合并链接引用定义、脚注编号、标题收集、emoji、行内 HTML 样式
  ▼
展示树 Node/Inline —— 渲染与映射共用
```

**为什么用改写而不是换解析器**：Ely 评估过 markdown-rs（CommonMark 规范最全），但 128 KiB 文本解析 1.84s vs pulldown-cmark 20ms（ADR-0041）。缺失的语法用"改写 + 字节映射"补，改写只在**内存副本**上进行，磁盘文本永不重写——这本身就是保真设计的一部分。

### 1.4.2 映射机制（整个方案最精妙的部分）

```rust
pub struct Inline {
    pub text: String,                                   // 绘制文本（不含标记符）
    pub styles: Vec<(Range<usize>, Style)>,             // 绘制坐标的字节区间
    pub links: Vec<(Range<usize>, String)>,
    pub sources: Vec<(usize, Range<usize>, Escapes)>,   // (绘制起点, 源码区间, 转义类型)
}

pub enum Escapes { Plain, Markdown, Html, Pipes }       // 反斜杠转义/HTML实体/表格\|
```

- `source_at(drawn) -> source`：点击落点反查（partition_point 找所在 piece + 偏移对齐）。
- `drawn_at(source) -> drawn`：从源码锚点找渲染位置（光标恢复、reveal 滚动）。
- `aligned()`：**从落点向回做字符级匹配**——跳过被解析丢弃的空白与标记符，处理反斜杠转义和 HTML 实体，把绘制偏移对齐到源偏移。`**bold**` 中点击 "bold" 落在源码 `bold` 字节上，而不是 `*` 上。
- 代码块/HTML/front matter 用 `LineMap` 逐行映射（处理 CRLF、tab）。
- `Node::rebase(start)`：块内偏移统一 rebase 到文件绝对偏移。
- 点击定位用 GPUI 文本布局的 `index_for_position` 得到绘制偏移，再过映射层。

### 1.4.3 渲染与增量

- 渲染层全部是 `div + StyledText + InteractiveText` 组合，**没有自定义块级 Element**（自定义 Element 只存在于 TextInput 和 CodeEditor 内部——文本测量/选区/光标/IME 注册）。
- 编辑后增量：`alike(old, new)` 从新旧块列表**两端**配对（绘制树相同且源文本相同），`ListState::splice` 只替换中间变化的块——未变的块连点击映射都不用重建。
- 解析调度：≤128 KiB 同步立即解析；更大则后台线程 + 300ms 防抖；过期解析结果用 `edits` 计数器丢弃；>2 MiB 永不解析（纯源码模式）。

## 1.5 Q4：如何做到源码保真

ElyMark 把保真做成了**结构不变量**，共七条：

1. **文本永不重建**。全文存于一个 String，编辑永远是 `replace_range`。没有任何"从 AST 重新序列化出 markdown"的路径（唯一的有损序列化 `BlockData::to_markdown` 属于另一套 Notion 式组件，不碰文件链路）。
2. **编辑 = 区间补丁**。块命令返回 `Vec<(Range<usize>, String)>`，排序、断言不重叠、一次写入。渲染时每块记住自己的源码区间，且**区间排除尾部换行**——块间空行/换行永远不在编辑区间内，天然保真。
3. **非规范语法就地保留**。取消加粗删除的是**实际写下的标记**（从源码切出真实区间），不是规范形式；setext 标题改级别只改下划线本身（超 2 级才转 ATX，与 Typora 一致）；`+`/`-`/`*` 弹点、`1.`/`1)` 分隔符、缩进空白、引用前缀全部按原字符续写；链接引用定义、front matter 按原文绘制，命令直接拒绝；HTML 块做浏览器式文本提取绘制（标签不显示），但其**源码区间原样保留**，点击即以原文编辑。
4. **命令拒绝制**。格式化命令跨越代码块/数学/HTML/front matter 时返回 `EditRefusal::*`，UI 提示而非硬做。
5. **写前重读校验**。高危命令（标题改动、块拆分）应用前重新解析，逐字符比对绘制结果，可见字符会变就拒绝（`check::reread`）。
6. **存储层字节重放**。`Source { bytes, bom, crlf, text }`：未编辑 → `encode` 直接返回原文件字节；编辑过 → 只重放两件事：BOM 有无、行尾按读取时多数派（CRLF/LF 混合跟随多数，孤立 CR 因"不改就不碰"而保留）；无尾换行保留；UTF-16 读取即拒（明确报错）。
7. **块级增量 + 身份维持**。`alike` 头尾配对保证未编辑块的源码区间对象原样保留，映射不失效。

保真有完整的测试矩阵（逐字节断言）：模式来回切换不改一个字节、BOM+CRLF 文件打一个字符后其余字节逐字节相等、CRLF/BOM/孤立CR/空文件/无尾换行各自成立、块命令前后映射逐字符回读。**这套测试本身就是方案的一部分**（见 §2.11）。

## 1.6 输入法（IME）——中文输入是硬要求

- 两处 `EntityInputHandler` 实现（CodeEditor、TextInput），走 GPUI 原生 `ElementInputHandler` 通道：渲染时 `window.handle_input(&focus, ElementInputHandler::new(bounds, entity), cx)` 注册。
- 组合态（marked text）应用侧管理：`replace_and_mark_text_in_range` 时 `composing = Some(snapshot())` 并设 `marked` 区间；**整段组合 = 一步撤销**；组合期间不记 history。
- `bounds_for_range` 用文本测量的 `x(column)` 定位候选窗位置。
- Windows 侧 gpui-pre 打了 DirectWrite **中文（Han）字体回退**补丁（简体不再落回日文字体）——macOS 无此问题，但 Windows 发布时必须带上等价物。

## 1.7 GPUI 层面的事实

- gpui-pre 是 zed gpui 的 crates.io 快照（非 fork）；Ely vendor 后的补丁全部是平台小修（标题栏命中测试、GIF 坏帧、Windows 字体回退、销毁竞态），**没有文本测量/排版/IME 层的补丁**——说明 GPUI 原生能力够用。
- 需要自己实现的自定义 Element 只有两类：文本输入框（`request_measured_layout` + `shape_text` 测量、prepaint 画选区/光标、注册 `ElementInputHandler`）和代码编辑器的 uniform_list + canvas。**块级渲染完全不需要自定义 Element**。
- 变高虚拟化用 gpui 内建 `list`（`ListState::new(1, ListAlignment::Top, overdraw 40rem)`），块高由布局自动测量。

## 1.8 ElyMark 的局限（Aline 的改进空间）

| # | 局限 | 影响 | Aline 对策 |
|---|---|---|---|
| 1 | 打开的块显示**源码原文**（带半高亮），不是渲染文本 | 距离 Typora "无感" 还差一步 | `FoldedInlineEditor`（§2.5）：打开块显示渲染文本、标记渐隐→隐藏 |
| 2 | 代码高亮 = 关键词 lexer | 效果粗糙 | syntect（fancy-regex 特性，纯 Rust）+ 按块缓存 |
| 3 | 撤销 = 全文快照栈 | 大文档内存开销大 | 补丁式撤销栈 |
| 4 | Buffer 每次 `replace_range` 后全量重建行表 | O(n)/编辑；2 MiB 上限 | v1 可接受；Buffer trait 化，预留 ropey |
| 5 | 任务列表无点击切换 | 体验缺口 | `[ ]`/`[x]` 位置的源码补丁，实现成本极低 |
| 6 | 远程图片完全不加载 | 有外链笔记的用户不便 | 可选项（默认关），带大小限制与磁盘缓存 |
| 7 | ratex 是 KaTeX 子集；mermaid-rs 是官方 mermaid 子集 | 复杂公式/图会失败 | 失败时显示原文面板（可编辑）；预留 WebView 渲染后备模块 |
| 8 | 无 PlantUML | — | v1 不做；可选接 Kroki 服务（后续） |
| 9 | 大量 `pub(crate)`，公开 API 面窄 | **不可直接依赖**，只能借鉴重写 | Aline 全部自写实现，Ely 源码作为对照参考 |

## 1.9 调研中发现的其他事实

- ElyOffice 的 PRD 宣称解析内核是 markdown-rs，**实现实际是 pulldown-cmark**（markdown-rs 只用于导出）；选型依据是 ADR-0041 的性能实测。Aline 直接采用 pulldown-cmark，无需再评估。但注意：ElyOffice 应用对内核的 pin 尚停留在 markdown-rs 时代（`bcaf2cc`），pulldown-cmark 还没在出货应用里端到端跑过——1.84s vs 20ms 是内核基准数字，Aline 需在渲染里程碑用真实文档自测复核（§2.10 预算表）。
- ElyMark 的 `MarkdownDocument` 与 `BlockEditor`（Notion 式，`BlockKind`/slash 菜单/跨块选区）是**两套并存模型**，文件链路只用前者。Aline 不需要后者。
- 组件库还包含 `RenderSurface`（宿主同步喂位图的画布原语，可用于未来 typst 集成）、`WebView`（wry 原生子窗口覆盖）、`ScrollSync`（双栏同步滚动）、`FloatingToolbar`（选中浮动格式条）——这些是 UI 层可选项，引擎不依赖。
- 许可：组件库 MIT；ratex/mermaid-rs/usvg/pulldown-cmark 均为 MIT/Apache 双许可，无传染风险。

## 1.10 HTML 渲染机制（专项逆向）

**核心发现**：ElyMark 的 HTML 支持**不是浏览器渲染**，而是两套「语义降维」机制——行内 HTML 映射为 GPUI `HighlightStyle`；HTML 块做**浏览器式文本提取**（标签消费掉、可见文字保留、块内行内样式照常生效、注释丢弃）。两者都不执行脚本、不建 DOM、不布局盒子，因此完全落在"纯 GPUI、无 WebView"边界内，可直接移植。

| 层面 | ElyMark 实现 | Aline V1 对齐目标 |
|---|---|---|
| **行内 HTML** | `<b>/<strong>`→字重、`<i>/<em>`→斜体、`<s>/<strike>/<del>`→删除线、`<u>`→下划线、`<mark>`→底色、`<code>/<kbd>`→代码色、`<a>`→链接色 + 链接线；浏览器式实体解码、HTML5 嵌套修复、`<a>` 互斥；过滤标签（`<script>`/`<style>`/`<iframe>`…）的标签本身按文本显示 | **完全复刻**：标签全集 + 样式映射 + 实体解码 + 嵌套修复，全部走 `StyledText::with_highlights` |
| **块级 HTML** | `mdast::Node::Html` → 浏览器式文本提取 → 显示树里是一个带 `Style`/`sources` 的 `Paragraph` → 点击时 `TextInput` 显示**源码原文** → commit 单区间写回 | **完全复刻**：`html_block_text` 同语义；打开走通用 `Open` 路径（无 HTML 专用分支） |
| **交互** | 同一时刻最多一个块「打开」；Escape = 提交（取消用撤销）；点击落点经 `Inline::source_at()` 映射到源码字节 | **完全复刻**：HTML 块走通用 `Open` 路径，落点 `caret = at - range.start`（`at` 已是源码字节，打开态映射恒等） |
| **命令拒绝制** | 块命令作用于 HTML 块、行内格式触及/跨越 HTML 节点时返回 `EditRefusal::Html`，UI toast 提示 | **完全复刻**：`aline-edit/src/block.rs` `refusal()` + `aline-edit/src/format.rs` 对 Html 的特判 |

**技术栈清单**（依赖登记见 §2.12）：

| 组件 | 选型 | 来源 |
|---|---|---|
| 解析器 | pulldown-cmark 0.13（`Node::Html` 原生支持） | 继承，无新增依赖 |
| HTML 实体解码 | `entities = "1"` crate | 直接复用 |
| 样式→HighlightStyle | gpui 内置 `UnderlineStyle`/`StrikethroughStyle`/`font_weight`/`font_style` | 直接复用 |
| HTML5 标签栈修复 | 自研 `tree/stack.rs`（`reopens`/`breaks`/`closes_p`/`<a>` 互斥） | 逐行重写 |

代码分布（`Ely-GPUI-Components/crates/ely-documents/src/`）：

```
markdown/mdast.rs      # Html(Literal) 节点（pulldown 直出）
tree/html.rs           # ★ 行内 HTML 解析器：inline_tag/tag/tag_end/inline_style/
                       #   reopens/breaks/closes_p/filtered/reference（483 行）
tree/html_block.rs     # ★ HTML 块：html_block_text() → Inline（318 行）
tree/stack.rs          # ★ Run 状态管理：html/start/end/close（HTML5 嵌套修复，88 行）
tree/sources.rs        # ★ 双向映射：(drawn_offset, source_range, Escapes)
render/style.rs        # ★ Style → HighlightStyle 映射
document/point.rs      # TargetBlock::Html（点击命中类型）
document/block.rs      # EditRefusal::Html（块命令拒绝）
document/edit.rs       # open/commit（TextInput 显示源码）
document/format.rs     # 行内格式命令特判
```

新增依赖只有一个：`entities = "1"`（HTML 实体表）；样式输出全部走 gpui 内置 `UnderlineStyle`/`StrikethroughStyle`/`font_weight`/`font_style`，**没有自定义 Element**。

### 1.10.1 行内 HTML 标签全集

**位置**: `tree/html.rs:23–34`

```rust
/// How Typora shows an inline tag; others show text.
pub(crate) fn inline_style(name: &str) -> Option<Style> {
    match name {
        "u" => Some(Style::Underline),
        "b" | "strong" => Some(Style::Strong),
        "i" | "em" => Some(Style::Emphasis),
        "s" | "del" | "strike" => Some(Style::Strike),
        "code" | "kbd" => Some(Style::Code),
        "mark" => Some(Style::Mark),
        "a" => Some(Style::Link),
        _ => None,                          // 其余标签 → InlineTag::Gone（标签消失、文字保留）
    }
}
```

**四类处理结果**（`html.rs:117` `inline_tag()` → `InlineTag` 枚举，定义在 `:101`）：

| 输入 | 结果 | 说明 |
|---|---|---|
| `inline_style()` 命中 | `Open{name, style, href}` / `Close(name)` | 开/闭一个样式区间 |
| `filtered()` 命中 | `Text(原文)` | 该标签（含属性）显示为文本 |
| `name == "br"` | `Break` | 插入 `\n` |
| 其余（含无 `href` 的 `<a>`、`<span>`、`<div>` 等） | `Gone` | 标签消失，周围文字保留 |

**风格映射表**（Aline 落点 `aline-ui/render/inline.rs`；ElyMark 为 `render/style.rs`）：

| `Style` | GPUI `HighlightStyle` 字段 |
|---|---|
| `Underline` | `underline: Some(UnderlineStyle { thickness: rule, color: None, wavy: false })` |
| `Strong` | `font_weight: FontWeight::BOLD` |
| `Emphasis` | `font_style: FontStyle::Italic` |
| `Strike` | `strikethrough: Some(StrikethroughStyle { thickness: rule, color: None })` |
| `Code` | 等宽字体 + 底色（`colors.code_background`） |
| `Mark` | `background_color: Some(colors.warning_subtle)` |
| `Link` | `color: Some(colors.link)` + 细下划线（`colors.link` 低透明度） |

**关键原则**：所有视觉输出均通过 `StyledText::with_highlights(range, highlight_id)` 完成，**无自定义绘制逻辑**。`br` 不产生样式，只是文本里的 `\n`。

### 1.10.2 HTML5 嵌套修复算法

**问题背景**：Typora 式编辑器允许错误嵌套，如 `<b><i>hello</b> world</i>`，需自动闭合/重开以恢复合法结构。**位置**: `tree/stack.rs`

**① 可重开标签列表**（`html.rs:37`）

```rust
/// HTML5's formatting elements reopen past a crossed close.
pub(crate) fn reopens(name: &str) -> bool {
    matches!(name, "a" | "b" | "code" | "em" | "i" | "s" | "strike" | "strong" | "u")
}
```

- **行为**：`end(name)` 找到最内层同名标签后，把它**之上**的所有未闭合标签一并关闭；其中属于 `reopens` 白名单的，在关闭点之后**重新开启**（`start = end`）。
- **注意**：`mark` **不在**白名单里（HTML5 的 formatting elements 不含 `mark`）；`a` 在列表里，但因为 `start()` 里 `<a>` 互斥，实际表现为"新链接顶掉旧链接"而非重开。
- **目的**：兼容 `<b><i>hello</b> world</i>` 这类交叉嵌套——`world` 仍带 italic。

**② 块级标签列表**（`html.rs:61`）

```rust
pub(crate) fn breaks(name: &str) -> bool {
    matches!(name,
        "address"|"article"|"aside"|"blockquote"|"center"|"details"|"div"|"dl"|"dd"|"dt"
      | "figcaption"|"figure"|"footer"|"h1"|"h2"|"h3"|"h4"|"h5"|"h6"|"header"|"hr"
      | "li"|"main"|"nav"|"ol"|"p"|"pre"|"section"|"summary"|"table"|"tr"|"ul")
}
```

- **32 个**标签，覆盖 HTML5 全部"浏览器按块布局"的元素；每个产生两侧换行。
- 白名单较长但**是闭合集合**（HTML5 块级元素全集），不是随意挑的常用标签。

**③ p 元素自动闭合规则**（`html.rs:146`）

```rust
pub(crate) fn closes_p(name: &str) -> bool {
    matches!(name,
        "address"|"article"|"aside"|"blockquote"|"center"|"details"|"dialog"|"dir"|"div"
      | "dl"|"dd"|"dt"|"fieldset"|"figcaption"|"figure"|"footer"|"form"|"h1"|"h2"|"h3"|"h4"
      | "h5"|"h6"|"header"|"hgroup"|"hr"|"li"|"listing"|"main"|"menu"|"nav"|"ol"|"p"|"pre"
      | "search"|"section"|"summary"|"table"|"ul")
}
```

- **39 个**标签（`html.rs:146–157`），遇到这些开始标签时，若当前有未闭合的 `<p>`，隐式闭合它。

**④ `<a>` 元素互斥**（`stack.rs:33`）

```rust
pub(super) fn start(&mut self, name: &str, style: Option<Style>, url: Option<String>) {
    if name == "a" {
        self.end("a");            // 新链接先关闭旧链接
    }
    self.shortcodes.split();
    self.push(Opened { name: name.to_string(), start: self.text.text.len(), style, url });
}
```

- **补充细节**：`<a>` 只有**带 `href` 时**才产生 `Style::Link`；无 `href` 的 `<a>` 走 `InlineTag::Gone`（标签消失、文字保留）。见 `html.rs:133-137`：

```rust
let href = match style {
    Style::Link => tag.attribute("href").map(str::to_string),
    _ => None,
};
let style = (style != Style::Link || href.is_some()).then_some(style);
```

**⑤ `end()` 的完整修复流程**（`stack.rs:47`）

```rust
pub(super) fn end(&mut self, name: &str) {
    if self.count(name) == 0 {
        return;                                   // 无同名标签 → 什么都不做（不是 panic）
    }
    self.shortcodes.split();
    let at = self.open.iter().rposition(|opened| opened.name == name)
                   .expect("a counted name is open");
    let end = self.text.text.len();
    if let Some((_, from)) = &mut self.owed {
        *from = (*from).min(at);
    }
    let closed = self.open.split_off(at);
    for opened in &closed {
        close(&mut self.text, opened, end);       // 记录 style/link 区间
        *self.named.get_mut(&opened.name).expect("counted") -= 1;
    }
    for opened in closed.into_iter().skip(1) {    // 跳过被点名关闭的那个
        if html::reopens(&opened.name) {
            self.push(Opened { start: end, ..opened });   // 重开
        }
    }
}
```

> `owed` 是**延迟空白记账**字段（`Option<(char, usize)>`：欠写的一个字符 + 其起始栈深），它不只是"跨段落未闭合标签"的修补，更是 §1.10.5 空白折叠的核心机制——空白/换行先记欠、等到真正写出下一个词或标签时才 `pay()` 补上，避免行首行尾多余空白。`end()` 里的 `*from = (*from).min(at)` 保证欠账起始栈深不越过被关闭的标签。**不可省略**：去掉它就无法复刻浏览器的空白折叠语义。

**⑥ 过滤标签**（`html.rs:45`）

```rust
pub(crate) fn filtered(name: &str) -> bool {
    matches!(name, "title"|"textarea"|"style"|"xmp"|"iframe"
                  |"noembed"|"noframes"|"script"|"plaintext")
}
```

- **行为**：`inline_tag()` 对过滤标签返回 `InlineTag::Text(原文)`——**这个标签本身**（名字 + 属性，属性里的实体照常解码）按字面文本写出，永不执行。但注意：**标签之后的内容仍按 HTML 规则继续扫描**，所以 `<script><b>x</b></script>` 显示为 `<script>` + 粗体 `x` + `</script>`，而不是整段死文本（`html_block.rs:244` 的 `filtered_tags_are_text_and_names_end_as_html5_ends_them` 测试即此断言）。
- **安全意义**：这是 GFM tag filter 的实现，`<script>alert(1)</script>` 在编辑器里就是一行字。

### 1.10.3 HTML 实体解码机制

**位置**: `tree/sources.rs:9`（枚举）、`:237`（`read()`）、`tree/html.rs:303`（`reference()`）

**① Escapes 枚举（四态，不是两态）**

```rust
/// How a piece's source turns into its text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Escapes {
    Plain,      // 原样
    Markdown,   // 反斜杠转义 + 实体，按 Markdown 规则
    Html,       // 实体，按 HTML 规则
    Pipes,      // 表格单元格里的 \| → |
}
```

**四种状态各有各的解码规则**，这正是 `(绘制起点, 源区间, Escapes)` 三元组必须带第三项的原因。

**② 反向读取：`read()` 逐字符建映射**（`sources.rs:237`）

```rust
fn read(written: &str, escapes: Escapes) -> Vec<Read> {
    let mut out = Vec::with_capacity(written.len());
    let (mut at, mut line_start) = (0, false);
    while let Some(ch) = written[at..].chars().next() {
        let rest = &written[at..];
        let decoded = match (escapes, ch) {
            (Escapes::Markdown, '\\') => rest[1..].chars().next()
                .filter(char::is_ascii_punctuation)
                .map(|punct| (punct.to_string(), 2, at + 1)),
            (Escapes::Markdown, '&') => reference(rest).map(|(v, n)| (v, n, at)),
            (Escapes::Html, '&') => html::reference(rest, false).map(|(v, n)| (v, n, at)),
            (Escapes::Pipes, '\\') => rest[1..].starts_with('|')
                .then(|| ("|".to_string(), 2, at + 1)),
            _ => None,
        };
        if let Some((value, used, place)) = decoded {
            // 一个实体解码成多个字符时，每个字符都指向同一个源起点
            out.extend(value.chars().map(|ch| Read { ch, at: place, raw: false, lead: false }));
            (at, line_start) = (at + used, false);
            continue;
        }
        /* … 普通字符：at 前进 len_utf8 … */
    }
    out
}
```

**关键不变量**：实体解码出的**每个**字符都记 `at = place`（实体起点）。所以"绘制字符 → 源字节"永远落在实体的 `&` 上，不会错位到实体内部。

**③ HTML 实体表**（`html.rs:353` `named()` / `:303` `reference()`）——不是自己维护的表，直接用 `entities` crate：

```rust
static NAMES: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
let names = NAMES.get_or_init(|| {
    entities::ENTITIES.iter()
        .map(|entity| (&entity.entity[1..], entity.characters))   // 去掉前导 '&'
        .collect()
});
```

**支持的全部形式**（比想象中多）：

| 形式 | 例子 | 说明 |
|---|---|---|
| 命名 + 分号 | `&amp;` → `&` | 标准形式，最长名 32 字节（`LONGEST`） |
| 命名 + 无分号（legacy） | `&amp` → `&` | 文本里允许，属性值里**不允许**（`attribute` 参数区分） |
| 十进制数字 | `&#65;` → `A` | 分号可选 |
| 十六进制数字 | `&#x41;` / `&#X41;` → `A` | 分号可选 |
| Windows-1252 C1 块 | `&#128;` → `€` | 按浏览器规则做 0x80–0x9F 重映射（32 项表） |
| `&#0;` / 非法码点 | → `U+FFFD` | 替换字符 |

属性值里的规则更严：`body[len..].starts_with(['=', ';'])` 时不认 legacy 无分号形式。

**双向映射不变量（精确表述）**：

> 对 HTML 块中任意绘制字符 `c`（来自 `Escapes::Html` 的文字段），其 `Read.at` 落在源文本中产生 `c` 的那个字节上；若 `c` 由实体解码而来，`at` 是实体 `&` 的字节位置。因此"点击绘制字符 → 光标落在源码实体起点"，用户改实体时看到的是完整 `&amp;` 而非半个。

### 1.10.4 块级 HTML 渲染管线（**ElyMark 最反直觉的部分**）

**位置**: `tree/html_block.rs`（318 行）+ `tree/mod.rs:343`

**先纠正一个常见误解**：ElyMark 的 HTML 块**不是**"按原文显示"。它是**浏览器式文本提取**：把 HTML 当作文档渲染，只保留可见文字，标签被消费掉。

实测（`html_block.rs:166` 的 `an_html_block_shows_its_text_as_a_browser_does`）：

| 源码 | 绘制出的文本 |
|---|---|
| `<div align="center">\n  <b>Bold</b>   and\n  :smile: &amp; <code>:smile:</code>\n</div>` | `Bold and 😄 & :smile:` |
| `<p>one <b>two </b></p>\n<p>three &lt;b&gt;</p>` | `one two` ⏎ `three <b>` |
| `<div>\na <br> b</br>c</div>` | `a` ⏎ `b` ⏎ `c` |
| `<div>\n<script>x</script>\n<!-- note -->\nshown<style>p{}</style>\n</div>` | `<script>x</script> shown<style>p{}</style>` |

要点：

- **标签消失**，文字保留（`<b>Bold</b>` → `Bold`，但带 `Style::Strong`）
- **行内样式在块内生效**（`<b>`→Strong、`<code>`→Code、`<mark>`→Mark）
- **实体解码**（`&amp;` → `&`、`&lt;b&gt;` → `<b>`）
- **emoji 短代码仍生效**（`:smile:` → 😄）；`<code>` 内不生效
- **注释丢弃**（`<!-- note -->` 消失，且不留空白）
- **块级标签产生换行**（`<div>/<p>/<li>/<br>` 两侧）
- **过滤标签的标签本身是文字**：`<script>`/`<style>` 等只把**这一个标签**（含属性）当文字写出——属性里的实体与 emoji 短代码照常解码（`<script title=" :smile:">` → `<script title=" 😄">`），但永不解析成元素、永不执行；**标签之后的内容仍按 HTML 规则继续扫描**（`<script><b>:smile:</b></script>` → `<script>` + 粗体 `😄` + `</script>`）

**完整算法**（`html_block.rs:6`）：

```rust
pub(super) fn html_block_text(written: &str, source: &str, span: Range<usize>) -> Inline {
    let (mut run, map) = (Run::default(), LineMap::new(source, span.start, written));
    let mut rest = written;
    // 每个文字段记录它写在哪里（双向映射的来源）
    let words = |run: &mut Run, rest: &str, text: &str| {
        let from = written.len() - rest.len();
        let at = |value| map.as_ref()?.at(value);
        if text.chars().all(html::spacing) {
            return run.words(text);            // 纯空白：不记映射
        }
        match at(from).zip(at(from + text.len())) {
            Some((start, end)) => run.text.sources.push(
                (run.text.text.len(), start..end, Escapes::Html)   // ★ 三元组
            ),
            None => log::warn!("markdown: an HTML block's text lost its source"),
        }
        run.words(text);
    };

    while let Some(at) = rest.find('<') {
        words(&mut run, rest, &rest[..at]);       // 先吃掉 '<' 前的文字
        rest = &rest[at..];

        // ① 注释：整段丢弃，不产生文字、不产生映射
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.split_once("-->").map_or("", |(_, after)| after);
            run.shortcodes.split();
            continue;
        }

        // ② 不是标签（'<' 后不是字母或 / ! ?）：按字面 '<' 写出
        let opens = rest[1..].starts_with(|ch: char| ch.is_ascii_alphabetic() || "/!?".contains(ch));
        if !opens {
            words(&mut run, rest, "<");
            rest = &rest[1..];
            continue;
        }

        // ③ 未闭合的标签：丢弃其后全部内容（HTML5 语义）
        let Some(end) = html::tag_end(rest) else {
            rest = "";
            break;
        };
        let (tag_text, after) = rest.split_at(end);

        match html::tag(tag_text) {
            // ④ 过滤标签：这个标签本身按字面文字写出（其后内容继续扫描）
            Some(tag) if html::filtered(&tag.name) => words(&mut run, rest, tag_text),

            // ⑤ 块级标签：先换行，再开/闭元素（HTML5 隐式闭合）
            Some(tag) if html::breaks(&tag.name) => {
                run.line();
                match tag.closing {
                    true => run.end(&tag.name),
                    false => {
                        run.implied(&tag.name);
                        match tag.name == "hr" {
                            true => run.shortcodes.split(),   // <hr> 无内容
                            false => run.start(&tag.name, None, None),
                        }
                    }
                }
            }

            // ⑥ 结束 <p> 但不占块的元素：隐式闭合后按行内标签处理
            Some(tag) if !tag.closing && html::closes_p(&tag.name) => {
                run.implied(&tag.name);
                run.html(tag_text);
            }

            // ⑦ 闭合标签或 <br>：处理换行欠账，然后走行内标签
            Some(tag) if tag.closing || tag.name == "br" => {
                run.owed = run.owed.filter(|_| tag.name != "br");
                run.html(tag_text);
            }

            // ⑧ 其余（<span>、无 href 的 <a>、<b>、<i>...）：行内标签
            Some(_) => {
                run.pay();                    // 先补上欠的空白
                run.html(tag_text);
            }

            None => run.shortcodes.split(),   // 不是合法标签：忽略
        }
        rest = after;
    }
    words(&mut run, rest, rest);
    run.cut()
}
```

### 1.10.5 空白与换行的"欠账"机制（`owed`）

这是 ElyMark 处理 HTML 空白的核心技巧，**不是简单的 trim**：

```rust
/// 空白不立即写出，而是记一笔"欠账"，等下一个词到来再补。
fn spaced(&mut self, shown: &str) {
    for ch in shown.chars() {
        if !html::spacing(ch) {
            self.pay();                                  // 有词 → 补上欠的空白
            self.text.text.push(ch);
        } else if self.owed.is_none() && !self.text.text.ends_with('\n') {
            let open = self.open.len();
            self.owed = (!self.text.text.is_empty()).then_some((' ', open));
        }
    }
}

/// 块级元素开始/结束时欠一个换行（而非空格）。
fn line(&mut self) {
    let from = self.owed.map_or(self.open.len(), |(_, from)| from);
    let text = &self.text.text;
    self.owed = (!(text.is_empty() || text.ends_with('\n'))).then_some(('\n', from));
}

/// 补上欠账；欠账之后开启的样式从补完的位置开始。
fn pay(&mut self) {
    let Some((owed, from)) = self.owed.take() else { return };
    self.text.text.push(owed);
    let start = self.text.text.len();
    for opened in &mut self.open[from..] {
        opened.start = start;      // ★ 样式起点随空白移动
    }
}
```

**为什么需要它**：HTML 源码里的缩进、换行、多个空格在浏览器里折叠成一个空格；块级元素之间换行而不留空格；行首行尾的空白要丢掉。`owed` 用一个延迟写入的槽位同时解决这三件事，并保证"空白被写出时，跨过它的样式起点跟着平移"——这正是点击落点能准确回溯的原因。

**HTML5 隐式闭合**（`implied`，`html_block.rs:125`）：

```rust
fn implied(&mut self, name: &str) {
    let items: &[&str] = match name {
        "li" => &["li"],
        "dd" | "dt" => &["dd", "dt"],
        _ => &[],
    };
    // 搜索在"特殊元素"处停止
    let stops = |name: &str| html::breaks(name) && !matches!(name, "address" | "div" | "p");
    let found = items.iter().any(|item| self.count(item) > 0).then(|| {
        let mut names = self.open.iter().rev().map(|o| o.name.as_str());
        names.find(|n| items.contains(n) || stops(n))
    }).flatten().filter(|n| items.contains(n)).map(str::to_string);
    if let Some(item) = found {
        self.end(&item);            // <li> 遇到新 <li> → 关掉旧的
    }
    if html::closes_p(name) && self.count("p") > 0 {
        self.line();
        self.end("p");              // 遇到块级元素 → 关掉 <p>
    }
}
```

实测（`:220` 测试）：`<ul><li><mark>x<li>y</ul>z` → 文本 `x⏎y⏎z`，只有 `x` 带 Mark。

### 1.10.6 双向映射的关键：`LineMap`

```rust
let map = LineMap::new(source, span.start, written);
let at = |value| map.as_ref()?.at(value);   // 绘制偏移 → 源码绝对字节
```

- `LineMap` 处理 CRLF/tab 等行内字节差异（与代码块同一套机制）。
- 每个非空白文字段压入 `(绘制起点, 源码区间, Escapes::Html)`。
- `Escapes::Html` 让反向读取时按 HTML 规则解码实体（见 §1.10.3）。

**这解释了为什么 ElyMark 的 HTML 块能精确点击**：映射在"文字段"级别建立，标签与注释根本不在映射里，天然不产生落点。

**性能护栏**（`html_block.rs:289` 的 `hostile_html_reads_in_linear_time`）：用 10 种病态输入（`"<".repeat(2n)`、`"<a".repeat(n)`、`"<!--".repeat(n)`、`"<b>".repeat(n)`、交叉闭合 `"<b>"*n/2 + "</i>"*n/2`、`"<script></script>"*n/8`、`"<div>a "*n`、`"<b> "*n`、`"<li><b>x "*n/2`、`"<p><i>x "*n/2`）在 1<<14 与 1<<16 两档测时，断言 4 倍输入耗时不超过 8 倍——即 **O(n)**。这是自研标签扫描器必须自带的护栏，不能靠"输入正常"假设。

### 1.10.7 点击定位与编辑提交链路

**位置**: `document/point.rs`, `document/edit.rs`, `document/block.rs`

**点击命中类型**（`point.rs:21` 枚举，`Html` 变体在 `:41`；`of()` 在 `:54`）：

```rust
pub enum TargetBlock {
    Paragraph, Heading(u8), Quote, Alert, List, Code, Math, Table, Rule,
    Html,                       // ★ 专有分类
    Footnote, Definition, FrontMatter, Toc,
}

impl TargetBlock {
    pub(super) fn of(node: &mdast::Node, source: &str) -> Self {
        match node {
            mdast::Node::Html(_) => Self::Html,
            // ...
        }
    }
}
```

**注意层级**：`TargetBlock` 读的是 **mdast** 节点（`mdast::Node::Html`，`point.rs:66`），而显示树把 HTML 块转成了 `Node::Paragraph(Inline)`（`tree/mod.rs:343`）。所以"HTML 块"这个身份只存在于 mdast 层——显示树里它就是一个普通段落，带 `Style` 区间与 `sources` 映射。点击时用 mdast 的 `TargetBlock::Html` 做分类（供 UI 显示"这是 HTML"），绘制与落点则走段落的 `Inline`。

**打开块流程**（`edit.rs:76`，**所有块同一路径**）：

```rust
pub(super) fn open(&mut self, ix: Option<usize>, landing: Landing,
                   window: &mut Window, cx: &mut Context<Self>) {
    self.waiting = None;
    let pressed = /* 块起点 */;
    let written = self.commit(cx);          // 先提交上一个打开块
    if written.is_some() { self.reparse(true, cx); }
    /* … 长文档重读的分支 … */
    let text = self.editor.read(cx).text();
    let (range, append, cell) = /* 块区间；表格走 cell */;
    let caret = match landing {             // 落点 → 块内偏移
        Landing::Start => Some(0),
        Landing::End | Landing::Cell(_) => None,
        Landing::At(at) => match moved(at, &written) {
            Some(at) if range.contains(&at) || at == range.end => Some(at - range.start),
            _ => None,
        },
    };
    let source = text[range.clone()].to_string();
    let input = cx.new(|cx| {
        let input = TextInput::new(window, cx).highlighter(markdown_highlights);
        match cell {
            Some(_) => input,                       // 单元格单行
            None => input.multi_line(1, usize::MAX), // 块多行
        }
    });
    input.update(cx, |input, cx| {
        input.set_text(source, cx);
        if let Some(caret) = caret { input.select(caret..caret, cx); }
    });
    window.focus(&input.focus_handle(cx), cx);
    /* … 订阅 Blur → commit / Changed → 事件 / Submit(单元格) → commit … */
    self.open = Some(Open { range, input, append, cell, lent: None, _subscriptions });
}
```

**关键点（对 HTML 块而言）**：

- **没有 HTML 专用分支**。HTML 块就是一个普通 `Open`：`range` = 块源码区间，`cell = None`（多行）。
- 打开态显示的是**源码原文**（`text[range]`），加 markdown 语法着色。
- 光标落点 `caret = at - range.start`——注意 `at` 已经是 `on_point` 经 `inline.source_at()` 映射出的**源码字节**，所以这里是**直接相减，不再走映射**。这是"打开态映射恒等"的具体体现。
- `Landing::Cell(_)` 与表格单元格走同一分支；HTML 块永远 `cell = None`。

**提交流程**（`edit.rs:271`）：

```rust
pub(super) fn commit(&mut self, cx: &mut Context<Self>) -> Option<(Range<usize>, usize)> {
    let open = self.open.take()?;
    self.orphan = Some(open.input.focus_handle(cx));
    let typed = open.input.read(cx).text().to_string();
    let caret = open.input.read(cx).cursor();
    let text = self.editor.read(cx).text();
    let edges = cells::edges(text, open.range.clone());   // 表格单元格用
    let (lead, written) = match open.append {
        true if typed.is_empty() => return None,           // 新块没打字 → 不写
        true => (joint(text).len(), format!("{}{typed}", joint(text))),
        false if open.cell.is_some() => (0, cells::kept(&typed, edges)),
        false => (0, typed.clone()),                       // ← HTML 块走这里
    };
    let edit = (text[open.range.clone()] != written).then(|| {
        (open.range.clone(), written.len())
    });
    if edit.is_some() {
        self.writes += 1;
        let landed = self.editor.update(cx, |editor, cx| {
            editor.edit([(open.range, written.clone())], cx);   // ★ 单区间补丁
            editor.text().get(start..start + written.len()) == Some(written.as_str())
        });
        if !landed { /* 回滚计数 + error */ }
    }
    self.editor.update(cx, |editor, cx| editor.select(once(at..at), cx));  // 光标跟随
    edit
}
```

**撤销语义**：`editor.edit([(range, written)])` 是**一次** buffer 编辑 → 历史里**一步**。打开→打字→提交整体可一步撤销，回到打开前的源码。

**防丢机制**：写回后立刻校验 `text.get(start..start+len) == written`；不成立就回滚计数并报错（`"the editor refused a write-back"`）——避免"以为写了其实没写"的静默数据损坏。

### 1.10.8 命令拒绝制规则

**位置**: `document/format.rs:226`、`document/block.rs:468`

**① 行内格式命令特判**（`format.rs`）：

```rust
let held = format != InlineFormat::Clear && touches(false);
let why = match node {
    Node::Code(_) if touches(true) => EditRefusal::Code,
    Node::Math(_) if touches(true) => EditRefusal::Math,
    Node::Yaml(_) | Node::Toml(_) if touches(true) => EditRefusal::FrontMatter,
    Node::InlineCode(_) if format != InlineFormat::Code && held => EditRefusal::Code,
    Node::InlineMath(_) if held => EditRefusal::Math,
    Node::Html(_) => match holder(&blocks, &whole).is_some() {
        true if held => EditRefusal::Html,              // 选区在 HTML 内且要加格式 → 拒
        false if touches(true) => EditRefusal::Html,    // 选区跨越 HTML 块 → 拒
        _ => continue,
    },
    _ => continue,
};
```

三个要点：

1. `InlineFormat::Clear`（清除全部格式）**不**被 `held` 判定拦住——Clear 需要能穿过 HTML。
2. `holder(...)` 为 `Some` 表示选区落在某个 HTML 节点内部 → 拒绝加格式。
3. `touches(true)` 表示选区跨到了 HTML 块 → 拒绝。

**② 块命令拒绝**（`block.rs:468`）：

```rust
pub(super) fn refusal(node: &Node) -> EditRefusal {
    match node {
        Node::Code(_) => EditRefusal::Code,
        Node::Math(_) => EditRefusal::Math,
        Node::Html(_) => EditRefusal::Html,
        Node::Table(_) => EditRefusal::Table,
        Node::Yaml(_) | Node::Toml(_) => EditRefusal::FrontMatter,
        _ => EditRefusal::Outside,
    }
}
```

**范围说明**：`block.rs:12` 是 `use crate::markdown::mdast::Node;`——`refusal()` 读的是 **mdast**，不是显示树。所以 `Node::Html(_) => EditRefusal::Html` **确实命中** HTML 块：把光标放进一个 HTML 块再按段落命令（如"设为标题"），得到的就是 `EditRefusal::Html`。两条拒绝路径分工明确：

- **块命令**（标题/引用/列表/代码块…）→ `block.rs::refusal()`，按 mdast 块节点类型判定，HTML 块命中 `Node::Html(_)`。
- **行内格式**（加粗/斜体/链接…）→ `format.rs` 的 `held` / `touches(true)` 特判，按选区与 HTML 节点的交叠判定。

显示树里 HTML 块是 `Node::Paragraph`，这只影响**渲染与落点**，不影响这两处拒绝判定（它们都走 mdast）。

**③ 拒绝后的 UI 行为**：`EditRefusal` 是纯枚举（`format.rs:39`），不携带文案；UI 层按变体映射 i18n key（Aline 走 `t("refusal.html")`）。ElyMark 侧由 `DocumentEvent::EditRefused(EditRefusal)` 发出。

---

~~~~~~~~

## L1328–L1352 · 历史依赖清单与版本理由（非当前准入）

原片段 SHA-256：`dbd621593a3aa1bf961be9e350a1ae4fe1b05f928513261a9ffcbd459b6ac60b`。

~~~~~~~~markdown
## 2.12 依赖清单

```toml
# UI
gpui = { version = "=0.3.9", package = "gpui-pre" }   # 或官方 crates.io gpui，锁死快照 rev
# 引擎（零 gpui）
pulldown-cmark = { version = "0.13", default-features = false }
ropey = "1"                  # 预留，v1 可不接入
# 资产
ratex = { version = "0.1", features = ["embed-fonts"] }  # + ratex-svg
mermaid-rs-renderer = "0.3"
usvg = "0.46"
syntect = { version = "5", default-features = false, features = ["default-fancy"] }
two-face = "0.4"             # syntect 语法集扩充（可选）
emojis = "0.9"
entities = "1"               # HTML 实体解码（行内 HTML / HTML 块的 &amp; &#65; &#x41;）
# 工具
similar = "0.5"              # 可选：版本历史/外部变更 diff
notify = "6"                 # V2 预留：文件监听（V1 不引入，v1-architecture §1.4）
# dev
proptest = "1"
```

GPUI 版本策略：与 Ely 一致选 `gpui-pre` 快照（0.3.9 = zed@93d7456）或 Zed 官方 crates.io 发布版（若可用），**锁死版本**，vendor 目录收平台补丁（Windows 上必带 DirectWrite Han 回退）。

~~~~~~~~

## L1587–L1630 · 完整Ely关键文件索引

原片段 SHA-256：`04ca34bd1a316d3a374559b6fb5a6aac99590513d7b5e6686a7e2eaca70bd0b0`。

~~~~~~~~markdown
# 附录 ElyMark 关键文件索引（实现期对照用）

```
Ely-GPUI-Components/crates/ely-documents/src/
  document/mod.rs        MarkdownDocument 主体、模式、解析调度、增量采纳
  document/parse.rs      Parsed/Block/alike
  document/edit.rs       open/commit/moved（编辑核心）
  document/format.rs     行内格式 toggle（written/toggle_tree）
  document/block.rs      BlockCommand 框架、change() 多补丁应用
  document/block/*.rs    标题/列表/代码等具体命令 + check::reread
  document/lists.rs      列表续行/缩进
  document/cells.rs      表格单元格编辑
  document/tables.rs     TableEdit 12 操作
  document/apart.rs      快照-验证-应用执行协议
  document/visual.rs     Visual 模式 Render、点击拦截、链接 Cmd+click
  document/point.rs      点击落点 → Landing
  markdown/flow.rs       $$ 围栏改写 + 字节映射
  markdown/tex.rs        \(\)/\[\] stand-in 字节改写
  markdown/front.rs      front matter 空格占位
  markdown/build.rs      pulldown 事件 → mdast
  tree/mod.rs            展示树 Node/Inline
  tree/sources.rs        source_at/drawn_at/aligned/LineMap/rebase、Escapes 四态、read()
  tree/html.rs           行内 HTML 解析器：inline_style/reopens/breaks/closes_p/filtered/
                         tag/tag_end/reference/named（§1.10.1–§1.10.3）
  tree/html_block.rs     HTML 块浏览器式文本提取：html_block_text + Run::implied/pay/line（§1.10.4–§1.10.6）
  tree/stack.rs          Run 状态管理：start/end/html/close + HTML5 嵌套修复（§1.10.2）
  render.rs              展示树 → GPUI 元素
  render/point.rs        index_for_position → Place
  render/math_text.rs    行内公式占位与绘制映射
  math.rs diagram.rs     ratex / mermaid-rs Asset 渲染
  picture.rs             图片加载缓存
Ely-GPUI-Components/crates/ely-editor/src/
  state.rs buffer.rs     CodeEditor/Buffer/History
  cursor.rs edit.rs      Selection/多区间编辑/anchors
  ime.rs keys.rs         EntityInputHandler/action 键位
  line.rs view.rs        行测量/press 定位/uniform_list 渲染
Ely-GPUI-Components/crates/ely-core/src/
  forms/text/*           TextInput：element.rs(自定义 Element 范式)/ime.rs
  forms/code.rs syntax.rs 自研 lexer（Aline 不采用，仅参考 markdown_highlights）
ElyOffice/apps/ely-mark/src/
  engine.rs view.rs      壳层 Text(source/current)、模式、自动保存联动
  text.rs                Source decode/encode（保真存储层范本）
  blocks.rs              应用层键位→命令映射
```
~~~~~~~~

## L1427–L1435 · 平台输入风险原表：AltGr/Ctrl+Alt等历史线索，须V-01/V-09复验

2026-10-30 独立删除准备复核补存：原文只作非权威历史证据；不恢复其中的依赖、平台断言、版本、工期、预算或未来产品承诺。现行契约仍由当前规范与原 V 实验负责。

原片段 SHA-256：`0cd46d9426eb319448136637cee546e99fdbcbd145f153d7d65198fe20be65e9`。

~~~~~~~~markdown
### GPUI 层（aline-ui / app）

| 主题 | 平台现状 | 对策 |
|---|---|---|
| IME | macOS 最顺；Windows 经 DirectWrite Han 回退补丁已验证（Ely）；Linux Wayland text-input / XIM 支持仍在演进，fcitx5/ibus 需实测 | M2 起三平台人工冒烟清单：中文组合、候选窗定位（`bounds_for_range`）、组合中点击、组合中切块；平台修复一律进 vendor 补丁目录并登记 |
| 字体 | macOS CoreText 自动回退；Windows DirectWrite 需补丁；Linux 依赖 fontconfig，发行版差异大 | 默认字体栈按平台配置——等宽：SF Mono/Menlo、Cascadia Mono/Consolas、JetBrains Mono/DejaVu Sans Mono；CJK：苹方、微软雅黑、Noto Sans CJK。缺字体走逐级回退，不报错 |
| 窗口 / DPI | 标题栏与命中测试各平台不同（Ely 已有 Windows 补丁）；Windows per-monitor DPI、Linux 分数缩放 | 尺寸一律走 gpui px 体系（自动乘缩放因子），不缓存绝对像素；M1 冒烟含双显示器/缩放场景 |
| 键位 | Cmd vs Ctrl；Windows 上 AltGr 被 OS 报告为 Ctrl+Alt 合成 | 键位表按平台定义；Windows 不得把 Ctrl+Alt 当独立修饰吞掉，否则欧洲键盘布局无法输入 |
| 文件监听 | notify（§2.12）：FSEvents / ReadDirectoryChangesW / inotify 事件粒度与语义不一 | 外部修改检测以"聚焦时 mtime+size"为准（§2.9），notify 只做提前提示，不作为正确性依据 |

~~~~~~~~

## L1444–L1586 · 旧HTML实施论证/限制/失败测试/决定与未来研究；旧估算和路线无现行效力

2026-10-30 独立删除准备复核补存：原文只作非权威历史证据；不恢复其中的依赖、平台断言、版本、工期、预算或未来产品承诺。现行契约仍由当前规范与原 V 实验负责。

原片段 SHA-256：`1212e0dc94ec9a58224ce9c4e7040f08965007ed5ba8a81ba36c5124e8196c7e`。

~~~~~~~~markdown
## 2.17 HTML 实施方案（对齐 §1.10 机制）

机制解析见 §1.10；本节只讲 Aline 的落点、改动量、里程碑与风险。裁决依据 D-2026-10-26-13。

### 2.17.1 架构分层与落点

按 Aline 的 crate 划分（v1-architecture §1.2–1.3），HTML 相关代码分布如下——**解析与映射全在零 GPUI 的 aline-markdown，样式 token 在 aline-ui，命令拒绝在 aline-edit**：

```
crates/
  aline-markdown/src/          # 零 GPUI
    tree/html.rs               # 行内 HTML：Tag/inline_style/reopens/breaks/closes_p/
                               # filtered/tag/tag_end/text/reference/attributes（新建）
    tree/html_block.rs         # HTML 块 → Inline：html_block_text + Run::implied/pay/line（新建）
    tree/stack.rs              # Run::start/end/html/close（新建，与行内共用）
    tree/sources.rs            # Escapes 四态 + read() 的 Html 分支（扩展）
    build.rs / mdast.rs        # mdast::Node::Html 由 pulldown 直出（已有）
    tree/mod.rs                # mdast::Node::Html → html_block_text → Node::Paragraph（扩展）
  aline-edit/src/
    format.rs                  # 格式 toggle 的 Html 特判（扩展，读 mdast）
    block.rs                   # refusal() 的 Html 分支（扩展，读 mdast 块节点）
  aline-ui/src/
    render/inline.rs           # Style → HighlightStyle 映射（新建映射表）
    render/blocks.rs           # 段落渲染复用，无需 Html 专用分支
    document/open_block.rs     # 通用 Open 路径，无需 Html 专用分支
```

> **边界铁律**：`Style` 枚举是纯数据（aline-markdown 定义），`HighlightStyle` 转换只发生在 aline-ui——与 D-2026-10-26-02（app-core 零 GPUI）同理，引擎层不出现 gpui 类型。

### 2.17.2 增量改动清单

| 文件 | 改动类型 | 行数预估 |
|---|---|---|
| `aline-markdown/tree/html.rs` | 新建：`Tag`/`inline_style`/`reopens`/`filtered`/`breaks`/`closes_p`/`tag`/`tag_end`/`text`/`reference`/`attributes` | +380 |
| `aline-markdown/tree/html_block.rs` | 新建：`html_block_text` + `Run::implied/words/spaced/line/pay` | +190 |
| `aline-markdown/tree/stack.rs` | 新建：`Run::start/end/html/close` + `InlineTag` 消费 | +100 |
| `aline-markdown/tree/sources.rs` | 扩展：`Escapes` 四态 + `read()` 的 Html 分支 | +30 |
| `aline-markdown/tree/mod.rs` | `mdast::Node::Html` → `html_block_text` → `Node::Paragraph`；行内 `run.html()` | +20 |
| `aline-edit/src/format.rs` | 格式 toggle 的 `Node::Html` 拒绝逻辑 | +15 |
| `aline-edit/src/block.rs` | `refusal()` 的 `EditRefusal::Html`（读 mdast 块节点，HTML 块命中 `Node::Html(_)`） | +5 |
| `aline-ui/render/inline.rs` | `Style` → `HighlightStyle` 映射表 | +80 |
| `Cargo.toml` | `entities = "1"` | +1 |

**总新增代码约 820 行**（不含测试）。其中 `html.rs` 是最大块——实体表、属性解析、引号感知的 `tag_end`、32 个块级标签白名单、legacy 实体规则都在这里。

**两个"不需要改"的地方**（容易被误估）：
- `aline-ui/render/blocks.rs`：HTML 块在显示树里是 `Paragraph`，走既有段落渲染路径，**无需 Html 分支**。
- `aline-ui/document/open_block.rs`：打开走通用 `Open` 路径（`cell = None` 多行输入），**无需 Html 分支**。

### 2.17.3 里程碑划分（对齐 §2.13 / v1-architecture M0–M6）

HTML 不单独占里程碑，按依赖关系嵌入既有阶段：

| 嵌入阶段 | HTML 内容 | 交付物 |
|---|---|---|
| **M0 解析内核** | `tree/html.rs` + `stack.rs` + `html_block.rs` + `Escapes::Html` 解码；规格测试（标签映射金样、嵌套修复、实体往返、`hostile_html_reads_in_linear_time`） | `cargo test -p aline-markdown html` 全绿；映射往返 proptest 绿 |
| **M1 只读渲染** | `render/inline.rs` 样式映射；HTML 块走既有段落渲染（无专用分支） | 渲染快照测试；点击落点逐字符正确 |
| **M2 开合式编辑** | HTML 块走通用 `Open` 路径 → TextInput → commit 单区间补丁 + 写回校验 | 打开→编辑→提交→撤销 一步还原 |
| **M3 命令框架** | `EditRefusal::Html` 两条路径：`format.rs` 的选区交叠特判 + `block.rs` 的 `refusal()`（都读 mdast） | 拒绝金样表全绿（块命令作用于 HTML 块、行内格式触及/跨越 HTML） |

### 2.17.4 性能预算

| 指标 | 目标 | 测量方法 |
|---|---|---|
| 128KiB HTML 密度文档解析耗时 | ≤30ms | `benches/html_density.rs` |
| 点击→打开态延迟 | ≤5ms | 手动计时 + DevTools |
| Commit 补丁大小 | ≤源码区间长度（零膨胀） | `edit()` 调用参数断言 |
| 内存占用峰值 | ≤2×文档大小 | `pprof` heap snapshot |

### 2.17.5 已知限制

1. **块级 HTML 不做 DOM 布局**（V1 现状，与 ElyMark 一致）
   - 原因：只做文本提取（换行 + 行内样式），不做盒模型、不做定位、不做 CSS；这与"无 WebView"铁律一致
   - 后续迭代方向：`OpenKind::Split` 式"上编辑下渲染"双视图（§2.17.9）

2. **属性基本忽略**（V1 设计决策）
   - `style` / `class` 不解析、不产生任何视觉差异（不引入 CSS 解析器，维持「无 WebView」边界）
   - `href` **是唯一被使用的属性**：`<a href>` 参与 `Style::Link` 与 Cmd/Ctrl+点击打开（与 Markdown 链接同路径）；`title` 等其余属性忽略

3. **不解析 HTML 块内的 markdown**（CommonMark 语义）
   - `<div>` 内的 `**bold**` 按字面显示，这与 CommonMark/GFM 的 HTML 块规则一致，不是缺陷

### 2.17.6 技术债务预警

| 债务 | 影响 | 化解计划 |
|---|---|---|
| `entities` crate 版本锁定 | 未来升级可能 API 不兼容 | Pin 主版本，升级需回归实体往返测试 |
| HTML5 修复算法硬编码 | 新增标签需手工维护白名单 | 白名单集中为单一常量表（`html.rs` 顶部），不散落；不引入配置文件（V1 无运行时配置需求） |
| 双视图落地时映射层扩展 | `sources.rs` 需新增"渲染坐标"来源 | 先在 `LineMap` 上扩展（HTML 块与代码块同构）；若复杂再抽 `MappingLayer` trait |

### 2.17.7 测试策略（并入 §2.11 门禁，对齐 v1-architecture §12 功能组 5）

| 测试类型 | 覆盖范围 | 位置 |
|---|---|---|
| 规格测试 | 标签→Style 金样表（每个标签 × 属性组合）；`filtered` 标签的标签本身按文本显示 | `aline-markdown/tests/spec.rs` |
| 嵌套修复 | 交叉嵌套 `<b><i></b></i>`、`<a>` 互斥、`closes_p` 隐式闭合 | `aline-markdown/tests/html_recover.rs` |
| 映射往返 | `source_at(drawn_at(x)) == x` 对含实体/嵌套的 HTML 片段（proptest 随机生成） | `aline-markdown/tests/roundtrip.rs` |
| 实体解码 | `&amp;` / `&#65;` / `&#x41;` / 无效实体按原文 | `aline-markdown/tests/html_entities.rs` |
| HTML 块 | 注释丢弃、块级标签换行、点击→提交单步撤销 | `aline-ui` 集成测试（TestAppContext） |
| 拒绝制 | 格式 toggle 触及/跨越 HTML 块；块命令作用于 HTML 块 | `aline-edit/tests/refusal.rs` |
| 性能基准 | 128KiB 高 HTML 密度文档解析；`hostile_html_reads_in_linear_time` 同款 O(n) 断言 | `aline-markdown/benches/html_density.rs` |

### 2.17.8 决策记录

已落为 **D-2026-10-26-13**（见 [decisions.md](./decisions.md)）。

**背景**：原 v1-features 把块级 HTML 真渲染列为"后续迭代"；逆向 ElyMark 源码后发现其 HTML 能力本就是「标签→样式」降维映射，并非浏览器式渲染。用户 2026-10-26 明确要求 HTML 进一期。

**选项**：
- A. 完整 HTML5 真渲染（WebView 或 webkit-rs）—— 违背「纯 GPUI、无 WebView」铁律，否决
- B. ElyMark 风格「标签→样式」降维映射（行内）+ 浏览器式文本提取（块级） —— 采纳
- C. 暂缓 HTML 支持至 V2 —— 与用户要求冲突，否决

**后果**：v1-features §5（三条明细）；engine-design §1.3.1 支持矩阵 / §1.10 机制 / §2.7 特殊块 / §2.12 依赖 / §2.13 M4 / §2.17 本方案；v1-architecture §1.3 / §6.7 / §12 / §13。

### 2.17.9 后续迭代路线图（不承诺时点，V1 之后逐项决策）

**① 块级 HTML 双视图（优先级最高）**

**方案**：HTML 块打开态升级为 `OpenKind::Split`（与块级公式/Mermaid 同构）：
- **上区**：源码输入（V1 已有行为，不动）
- **下区**：只读渲染视图——把同一套 `Style` 映射应用于块内标签，块级标签（`div/h1-h6/hr/details`）产生垂直布局与间距

**为什么不引入 `html5ever`**：V1 的 `stack.rs` 已经实现了子集所需的标签栈与修复算法；为了"下区渲染"再引一棵完整 DOM 树，等于同时维护两套 HTML 语义（栈式 vs 树式），必然分叉。**正确路径是把现有 `Run`/`Opened` 栈扩成轻量节点树（只保留渲染所需字段），不新增解析器依赖。**

**技术挑战**：
- 双向映射：下区是渲染视图，只需"点击 → 源码字节"，复用现有 `sources` + `aligned()`，不需要新的映射类型
- 主题适配：样式走 `Palette` token，天然跟随主题

**前置条件**：V1 的 HTML 规格测试全绿（语义已钉死），否则双视图会放大错误。

**② 样式属性解析（可选，风险最高）**

**范围**：`<span style="color:…">` → `foreground_color`；`background-color`；`font-weight`；`font-style`。

**风险**：CSS 是图灵完备的前缀；一旦开始解析 `style`，用户会期待 `class`、`<style>` 块、选择器、继承。**边界必须写死**：只认这四个属性、只认内联 `style`、不认 `class`、不认 `<style>` 块，其余一律降级为文本。若无法守住这条线，则不做。

**③ `<svg>` 内联渲染（与图表插件合流）**

`<svg>` 在 V1 按文本显示。若后续要做，**不应该是 HTML 特性，而应该是资产管线特性**：识别 `<svg>…</svg>` 块 → 走与 Mermaid 相同的 `Asset`（usvg 解析 → `RenderImage`）→ 主题色注入 + 缓存。这样复用 `render/assets.rs` 的整套机制，而不是给 HTML 开新路。

---


~~~~~~~~

## L1129–L1203 · 旧四档映射与折叠编辑候选研究：成本理由/原子区间/IME草图，不恢复产品承诺

2026-10-30 独立删除准备复核补存：仅保全独有历史论证，原平台/API/性能/算法断言未经本轮外部复验，当前产品选择、里程碑与V验证状态不变。

原片段 SHA-256：`d19f1de790b97b5ebc7555bf5ceb5c1fca432ad2c71f4b80882540b4fce52f1a`。

~~~~~~~~markdown
## 2.4 可视化编辑：四档阶梯（A → B → B+ → C）

**总原则：光标/选区/IME 状态一律以源文本字节坐标为不变量；"渲染视图"只是源文本的投影。** 这是 Ely 验证过的正确基座。

**关键洞察（决定阶梯形状）**：标记符"看得见"的档位（A/B），绘制文本恒等于块源文本，**映射是恒等的，成本≈零**；复杂度全部集中在 B+（绘制文本开始 ≠ 源文本）。而 B+ 与 C 只是"隐藏谓词"的范围差别——**编辑器机器是同一台，隐藏哪些区间是它的一个参数**。B 是天然的安全垫：最坏情况回退 B，产品依然成立。

| 档 | 打开的块显示什么 | 映射 | 边际成本 | 体验对标 |
|---|---|---|---|---|
| A | 块源码 + markdown 正常高亮 | 恒等 | ≈0（Ely 现状） | Obsidian 编辑行 |
| B | 块源码 + 标记符低对比渐隐 | 恒等 | ≈0（纯样式） | A 的 80% 手感 |
| B+ | 渲染文本；**仅光标所在格式区间的标记显形**，其余隐藏 | 真映射 | 全部核心工作 | Obsidian Live-Preview |
| C | 渲染文本；**全部标记隐藏** | 真映射 | 小增量（改谓词 + 退格规则） | Typora |

### 阶段 A：开合式编辑（= ElyMark 现状，先跑通全链路）

- 点击渲染块 → `Open { range, editor: 块源码输入框 }`，输入框显示**块源码原文** + markdown 标记半高亮，光标按 `source_at(点击点)` 落位。
- 打字直接改块源码；Blur/Escape/点别处/命令 → `commit()`：`buffer.edit([(range, 新文本)])`，单步撤销；全文重解析 + alike 增量。**Escape 与失焦同路径 = 提交**（2026-10-26 统一决策：不设"取消"手势——取消式 Escape 会不可恢复地丢字，提交式 + 补丁撤销零丢失；表格单元格同语义，见 §2.7）。
- 打字期间全文不重解析（只动 open 块）——这正是 Ely 性能宽松的原因。
- 交互面照 §1.2.4 全部实现（列表续行、格式 toggle、表格逐格、命令拒绝制）。

### 阶段 B：标记渐隐（纯样式，零映射成本）

A 的输入框把标记符（`**`、`#`、`` ` `` 等）渲染为低对比度颜色即可。绘制文本 = 源文本，点击/IME/选区全部是恒等映射，先拿到大部分"无感"体验。

### 阶段 B+（M5 目标）：折叠编辑器 + 光标附近标记显形

把打开块的输入框换成 `FoldedInlineEditor`：**显示该块的渲染文本**，"光标所在的最小格式区间"内的标记符显形（Obsidian Live-Preview 行为）：

- 显示：解析块源码 → `Inline`（绘制文本 + styles + sources）→ 按隐藏谓词 shape 绘制文本。
- 点击：`index_for_position` → 绘制偏移 → `source_at` → 源码偏移 → 光标画在 `drawn_at` 对应屏幕位置。
- 打字：`replace_text_in_range`（以绘制文本为显示基准，见 §2.8）→ 翻译为源码区间编辑 → **只重解析当前块**（段落级 µs）→ 新映射 → 重绘；光标因是源码坐标，重解析不移动它（只需 +delta 平移）。
- 隐藏区间是"原子"的：方向键一次跨过整个标记 run（CM6 atomic ranges 概念），源码偏移仍是落点。

### 阶段 C：全标记隐藏（Typora 手感）

与 B+ 共用同一台机器，隐藏谓词改为"全部隐藏"，外加点击/退格规则：

1. 点击落在被隐藏的标记上 → 落到最近的内容字符（映射层的 `aligned` 已保证内容字符映射正确）。
2. Backspace 在标记边界：删除整个标记 run（unwrap）而非半个星号。规则表：

| 光标位置（源码坐标） | Backspace 行为 |
|---|---|
| `**bold│**`（格式结束标记前） | 删除 `**`（unwrap），光标停在 `bold` 后 |
| `**bol│d**`（内容中间） | 删除 `o` |
| `**│bold**`（格式开始标记后） | 删除 `**`（unwrap） |
| 行首 │前无字符 | 正常（交给列表续行/缩进逻辑） |

**范围裁剪（重要）**：行内公式/图片**不参与**折叠编辑器的图元混排——打开的块内它们直接显示源码（§2.7），阅读态照常渲染。折叠编辑器因此只需处理"文本 + 样式 + 隐藏区间"，砍掉了 NBSP 占位 + 画图 + 原子跳过这整块最难的自定义绘制。

### `FoldedInlineEditor` 设计（核心自研组件，阶段 B+/C 共用）

坐标互逆用 GPUI 原生接口：`TextLayout::index_for_position` / `position_for_index`（已在 gpui-pre 0.3.8 源码验证存在，`elements/text.rs:863/897`、`text_system/line_layout.rs:348/428`）。

```rust
/// 自定义 Element（对照 ely-core TextElement 的结构，但显示的是绘制文本）
struct FoldedInlineEditorElement { open: Entity<Open> }
impl Element for FoldedInlineEditorElement {
    fn request_layout(&mut self, …) -> LayoutResponse {
        // shape_text(绘制文本, 段落宽) → 行数 → 高度；窗口宽变化时重排
    }
    fn prepaint(&mut self, …) {
        // 画选区背景、IME 下划线、光标；末尾注册:
        // window.handle_input(&focus, ElementInputHandler::new(bounds, editor.clone()), cx)
    }
    fn paint(&mut self, …) { /* 文本 runs + 标记渐隐/隐藏样式 */ }
}
```

- **InputHandler 以绘制文本为显示基准**（Zed multibuffer 的同款模式）：`selected_text_range`/`marked_text_range` 等接口返回绘制坐标（UTF-16），内部经 `sources` 双向映射翻译成源码字节区间再操作 buffer。GPUI 平台层只见"显示文本"，不需要知道源码的存在。
- IME 组合：`replace_and_mark_text_in_range` → 组合文本插入**源码**（替换选区）→ 标记该源码区间 → 块级重解析 → 绘制视图里以下划线高亮；`end_composition` 落定，整段组合 = 一步撤销。
- `bounds_for_range`：源码坐标 → `drawn_at` → 绘制坐标 → 屏幕 bounds（候选窗定位）。
- Up/Down 跨块：块边界处把光标移交给相邻块（`drawn_at` 保持视觉列，`goal` 列记忆）。

**风险与对策**：这是全项目最复杂的单个组件。对策：阶段 A/B/C 是渐进增强，A 完全可用后每一阶段都可独立发布；映射函数（`aligned` 反向匹配）先在纯逻辑层用属性测试钉死，再做 UI。


~~~~~~~~
