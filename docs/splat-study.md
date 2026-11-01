# Splat 可视化 Markdown 编辑机制调研

研究日期：2026-10-30。**研究材料，非规范；静态源码核实，不是运行验证、依赖准入或产品决定。**

## 1. 范围、版本与证据效力

- Splat 研究对象：提交 `4e5ead09804b901769580bb0d8fa038f8b7caea3`（本文简写 `splat@4e5ead0`）；`VERSION` 为 `1.0.3`。下文源码行号均属于这个提交。源码引用写作 `splat@4e5ead0:<path>:<line>`，不记录本机克隆位置；异机复查须先检出同一提交。没有 `.codegraph/`，按仓库指引使用普通搜索与逐行阅读。
- Aline 起始 HEAD：`61ad72336aac2dfa934c15ce1a70d3a1be9a9b72`。研究开始时两个仓库的 `git status --short` 均为空。本轮只修改 Aline 文档，不修改 Splat 源码、锁文件或 Aline 应用代码。
- 研究时曾检出本地副本；仓库不保存该克隆位置，也不再写本机绝对路径。
- 已阅读 Aline 的 AGENTS、文档入口及 PRD、架构指引、架构规范、引擎指引、UI、验证计划，并核对 Rust 的坐标、错误、unsafe、测试与依赖要求。权威按职责区分：[PRD](./v1-prd.md)规定结果，AGENTS 铁律和已确认专项规范约束实现，[验证计划](./v1-validation-plan.md)记录证据；本文不改变其中任何已确认选择。
- **事实**：已沿实际实现追踪的行为；**推断**：从实现得出的价值、成本或风险判断；**未确认**：本轮没有足够证据的事项。文中“建议”均是 Aline 适配分析，不表示 Splat 或 Aline 已通过相关测试。
- 本轮未构建、启动 Splat，未执行 Cargo 测试、selftest、基准或真实 IME；“存在测试”仅指读到测试源码。未查询远端最新版本；研究对象是用户指定的本地提交。

## 2. 最值得关注的五项实践

1. **把可视化操作分成定位、选择来源和语义编辑计划。** Splat 的 `insert_point`、`source_selection`、`deletion_ranges`、`Bridge` 各有职责；一次点击的锚点并不足以代表删除范围。Aline 可沿用问题拆分，继续按自己的映射层和单块源码编辑契约实现。
2. **紧凑映射先保留原样路径，变换文本才存额外偏移。** `CharMap::Verbatim/Offsets` 是有实现的存储案例；其扫描对齐算法没有完整精确性/失败协议，不能一起移植。Aline 的来源范围、投影版本、UTF-16 与字素规则仍须保留。
3. **局部优化必须有全量对照及全局依赖回退。** Splat 真的存在窗口重解析，但并非所有路径都增量；脚注、引用定义等有回退，测试比较树、着色、坐标和布局。Aline 先完成全量正确性，再以差分实验评估复用，不能据此直接批准局部解析。
4. **派生展示分批更新时，文字和映射先保持一致。** Splat 占位与最终富文本共用文字序列，用户直接编辑过的块强制失效；邻近块合并替换减少原生文本存储操作。Aline 可研究批量刷新及占位一致性，阈值和 TextKit 更新方式不移植。
5. **把不适合迁移的行为变为反例夹具。** 旧预览丢输入、表格导航顺带重排、弹窗关闭才整组记步、Escape 丢弃公式输入、重载清历史，都与 Aline 当前契约有差异。IME 仅见 marked-text 门控，不能当作中文输入或 ED-01 已被第三方证明。

## 3. 技术结构与 Markdown 数据流

**事实：**根 package 为 `splat`，workspace 另有 `xtask`。普通正文是 macOS AppKit 原生编辑；源码用 TextKit 2，富文本预览用 TextKit 1，后者实际创建 `NSLayoutManager` 与 `NSTextTable`。不是浏览器 `contenteditable`。但公式可视编辑及公式/图表渲染另有 WebKit 路径，因此也不能把整个应用称为“无 WebView”。

证据：Cargo.toml:1/33（`splat@4e5ead0:Cargo.toml:1`）；views.rs:96/171–192（`splat@4e5ead0:src/editor/views.rs:171`）；render.rs:45–48（`splat@4e5ead0:src/editor/render.rs:45`）；mathfield.rs:1–44（`splat@4e5ead0:src/editor/mathfield.rs:1`）；js.rs:40（`splat@4e5ead0:src/editor/js.rs:40`）。

```text
NSDocument 有效 UTF-8 / BOM 格式元数据
    → source NSTextStorage（权威源码；由 NSTextView 原生编辑/撤销）
    → Snapshot(text, version)（同步只读副本）
    → comrak → 自有 Doc（源码范围）
    → Layout / TopBlock / Para / Run（文字、样式、来源）
    → Index（preview UTF-16 段落索引）
    → attributed string → preview NSTextStorage

preview 原生输入 / 删除 / 粘贴
    → 通知的 UTF-16 改动，必要时 diff
    → Bridge 生成源码 Edit
    → apply_edit 写 source → 更新解析及展示
```

证据链：update.rs:210–234 `snapshot/exact_text`（`splat@4e5ead0:src/editor/update.rs:210`） → doc.rs:248–289 `parse/parse_part`（`splat@4e5ead0:src/markdown/doc.rs:248`） → layout.rs:299 `layout/relayout`（`splat@4e5ead0:src/preview/layout.rs:299`） → map.rs:32 `Index::new`（`splat@4e5ead0:src/preview/map.rs:32`） → render.rs:95 `render_block`（`splat@4e5ead0:src/editor/render.rs:95`）。

**推断：**纯 Markdown/preview 逻辑可以无窗口测试，`src/lib.rs:1–7` 的导出与集成测试支持这一点；但 Splat 并无 Aline 八 crate 的依赖隔离。它以原生 source storage 为权威、预览短暂领先后回写，不等于 Aline 的 DocCore/Buffer 加唯一草稿协议。Aline 只借鉴边界和测试，自写逻辑，不能让 GPUI 子控件成为另一份独立可写全文。

对 Aline 六项关键约束的逐项核对：

| Aline 当前约束 | Splat 对应证据与适配结论 |
|---|---|
| 源文本唯一真源，不从 AST 重建全文 | source → Doc → Layout 是投影链；保留这一方向。原生预览先发生改动再回写的时序不同，不能取代 DocCore 唯一草稿 |
| 所有编辑是源码区间补丁 | `Edit/compose/apply_edit` 提供实际单区间例证；仍须增加 Aline 的版本、保护、预演、拒绝零副作用和多补丁原子性 |
| 源码/绘制/UTF-16 经映射层 | 有来源映射及 UTF-16 helper，但多处是裸 usize、向后对齐；不能直接作为 Aline 跨模块类型或非法输入协议 |
| 未编辑字节原样写回 | Format 编码有 BOM/CRLF 测试，但没有原 bytes 快路径；不据此断言破坏保真，也不以有限测试代替门禁；编辑后混合行尾规则确实不同 |
| 引擎 crate 不依赖 GPUI | 纯库与 AppKit 宿主分开有参考价值；并无八 crate 保证，平台对象、线程、全局缓存只能按 Aline 所有权边界重新设计 |
| 不引入第二个 Markdown 解析器 | Splat 用 Comrak；Aline 保持 pulldown-cmark，只迁验证方法和实现思想，不迁解析依赖或 AST |

具体调用链及限制分别见下文 §4～6；这些差异没有构成修改 Aline 铁律的理由。

## 4. 可视化编辑的实际机制

### 4.1 编辑模型与状态转换

**事实：**Splat 是“源码 pane + 持续可编辑富文本预览 pane”。window.rs:440–451 `toggle_preview`（`splat@4e5ead0:src/editor/window.rs:440`）仅折叠/展开预览，不执行 Markdown 内容转换。普通段落没有“打开一个块、关块后提交”的状态机。焦点改变时，另一个 pane 的选区成为高亮镜像：selection.rs:18–84（`splat@4e5ead0:src/editor/selection.rs:18`）；恢复预览选区走 `preview_offset`，见同文件 142–159 行。

实际输入链：

1. mod.rs:277–306（`splat@4e5ead0:src/editor/mod.rs:277`） 的 NSTextView delegate 转发 `shouldChangeTextInRanges` 和 `textDidChange`。
2. editing.rs:396–464（`splat@4e5ead0:src/editor/editing.rs:396`） 记录 `Announced::Edit`；`preview_change`（同文件 796 行起）优先用通知范围及插入文字，长度关系不符才比较旧/新预览。fallback 为 sync.rs:187 `utf16_diff`（`splat@4e5ead0:src/editor/sync.rs:187`），重复文字时偏向当前光标解释；并非每键必做全文 diff。
3. bridge.rs:99–145 `replace/drop_empty_containers`（`splat@4e5ead0:src/preview/bridge.rs:99`） 计算删除区间、空容器清理和转义；同文件:484 `compose`（`splat@4e5ead0:src/preview/bridge.rs:484`）合成单区间 replacement，中间保留内容从原源码切片取得，不从 AST 序列化全文。
4. editing.rs:142–178 `apply_edit`（`splat@4e5ead0:src/editor/editing.rs:142`） 把源字节范围转成 source UTF-16 范围，写 source，恢复选区，调用 `update`。`Edit.selection` 是修改后源码坐标，见 commands.rs:8–19（`splat@4e5ead0:src/markdown/commands.rs:8`）。

**差异与适配：**Aline 在阅读投影命中后打开源码块，输入投影与块源码一致；不能照搬 Splat 对普通输入自动转义 Markdown 的规则。Splat 的 `escape`（bridge.rs:523–570）让富文本输入的 `*` 等保持字面文字、代码内不转义、表格粘贴换行变空格；Aline 必须允许用户逐步写入不完整 Markdown。

### 4.2 各类内容的呈现和编辑

下表均为**源码事实**，不是 Aline 功能采用表。源码位置均在 Splat。

| 内容 | 呈现与编辑机制 | 源码证据 |
|---|---|---|
| 段落、标题 | `ParaKind::Body/Heading`，标题标记不进入正文；空内容靠 `insert_at` 提供插入点 | layout.rs:798–829（`splat@4e5ead0:src/preview/layout.rs:798`）；render.rs:79–92（`splat@4e5ead0:src/editor/render.rs:79`） |
| 引用、嵌套列表、任务项 | 递归布局携带 `quote_depth/list_depth/group`；项目符号/编号/任务框为 marker atom；任务框点击生成源补丁 | layout.rs:830/922–964（`splat@4e5ead0:src/preview/layout.rs:922`）；navigation.rs:25–33（`splat@4e5ead0:src/editor/navigation.rs:25`） |
| 代码块 | 显示去围栏 literal 并映回 content；高亮施加在 attributed string；普通代码换行保留字面语义 | layout.rs:967–1034（`splat@4e5ead0:src/preview/layout.rs:967`）；render.rs:542–553（`splat@4e5ead0:src/editor/render.rs:542`）；bridge.rs:165 |
| 表格 | 每格为带 row/col/alignment 的 Para，原生 `NSTextTable` 排版；格内输入走 Bridge，Tab/Enter 则走结构导航 | layout.rs:1111–1147（`splat@4e5ead0:src/preview/layout.rs:1111`）；bridge.rs:161/273（`splat@4e5ead0:src/preview/bridge.rs:161`） |
| 链接 | 显示链接子文字并赋 link 样式；普通点击放光标，Cmd 点击打开目标；独立链接弹窗命令生成源码 Edit | layout.rs:1284–1287（`splat@4e5ead0:src/preview/layout.rs:1284`）；navigation.rs:63–70（`splat@4e5ead0:src/editor/navigation.rs:63`）；editing.rs:618–627（`splat@4e5ead0:src/editor/editing.rs:618`） |
| 图片 | 单个 attachment 对应整个图片源范围，不把 alt 作为可逐字编辑正文；加载失败仍有源引用 | layout.rs:1288–1300（`splat@4e5ead0:src/preview/layout.rs:1288`）；render.rs:524–529（`splat@4e5ead0:src/editor/render.rs:524`）；images.rs:76–150（`splat@4e5ead0:src/editor/images.rs:76`） |
| 软/硬换行 | SoftBreak 变空格，LineBreak 为 `LINE_SEPARATOR` atom；每 Para 最后附绘制换行，不能把所有绘制换行当源码 LF | layout.rs:1210–1215（`splat@4e5ead0:src/preview/layout.rs:1210`）；render.rs:110–120（`splat@4e5ead0:src/editor/render.rs:110`） |
| HTML | 布局保留 `html` 来源标志，Enter 有专门分支；未在普通 `replace/escape` 路径发现统一的 HTML 写入拒绝 | layout.rs:74/100（`splat@4e5ead0:src/preview/layout.rs:74`）；bridge.rs:99/152/523（`splat@4e5ead0:src/preview/bridge.rs:99`） |
| 公式、图表 | 原生正文中作为 atom；专用 popover 编辑源码，公式可切 MathLive WebView；图表/公式资产有 WKWebView renderer | atom.rs:134（`splat@4e5ead0:src/editor/atom.rs:134`）；mathfield.rs:1–44（`splat@4e5ead0:src/editor/mathfield.rs:1`）；js.rs:40（`splat@4e5ead0:src/editor/js.rs:40`） |

**推断：**“普通文字、合成 marker、整段来源 atom”的区分有助于 Aline 的命中、保护和资源展示；不能因此将图片或公式的 atom 直接绑定为可删除源码范围。Aline 仍须执行当前投影/版本/来源精确性及产品保护检查。Splat 的 `ParaStyle.html` 注释虽称 HTML 仅源码编辑，实际普通写入口未见对应完整门控；本轮不把注释当作只读保证，也未实测 HTML 编辑结果。Aline 允许点击 HTML 块原文编辑、拒绝 HTML 格式命令，不能从来源标志或灰色样式推导写入权限。

### 4.3 坐标映射、光标与选择

**事实：**layout.rs:152–189（`splat@4e5ead0:src/preview/layout.rs:152`） 用 `RunMap::Text { chars, end }` 或 `Atom { src, atom }` 区分逐字来源和整体来源；`CharMap::Verbatim(start)` 不分配逐字表，`Offsets(base, Box<[u32]>)` 存相对源字节偏移。它按 Unicode scalar 记录，不是字素索引。`Index` 存预览 UTF-16 段落位置，另记录脚注重排前的源码有序前缀（map.rs:14–58）。

- 插入：`insert_point`（map.rs:143–202（`splat@4e5ead0:src/preview/map.rs:143`））偏向光标左侧字符的样式归属；段首跳过 marker，空段落用 `insert_at`。
- 源到预览：`preview_offset`（map.rs:205–265）把标记内部位置吸附到可见文字或 atom 边缘，处理被移至文末的脚注。**不是双射**。
- 选择：`Bridge::source_selection`（bridge.rs:25–62）与删除范围计算不同；后者要覆盖实体/atom 的完整来源及跨段间隙，不能简单映射两个端点后相减。
- UTF-16：text.rs:139 `byte_at_u16_exact`（`splat@4e5ead0:src/markdown/text.rs:139`）可拒绝代理对中间/超长位置；sync.rs:31–74（`splat@4e5ead0:src/editor/sync.rs:31`）用它更新 source 同步副本，并缓存最近编辑点。另一套 `Utf16Map::to_byte`（text.rs:86–94）会向后对齐，不能把两者统称严格拒绝。
- 屏幕命中及普通上下左右移动主要由 NSTextView/TextKit 实现。显式拖放命中见 views.rs:51–53（`splat@4e5ead0:src/editor/views.rs:51`） 的 `characterIndexForInsertionAtPoint`；几何/滚动定位见 scroll.rs:110 起（`splat@4e5ead0:src/editor/scroll.rs:110`）。滚动按块源码/预览跨度和块内比例关联（同文件 24–106），并不证明逐像素或逐字精确映射。

**不能照搬的对齐算法：**layout.rs:1551–1632（`splat@4e5ead0:src/preview/layout.rs:1551`） 的实体解码只列部分单字符实体；`align` 顺序扫描相等字符、跳转义、处理换行/排版替换，找不到字符时填 `src.end`，没有显式 MappingError/精确性状态，含 `unwrap()` 和未受检 `as u32`。重复字符、未覆盖实体和多字符实体仍须独立验证；本轮没有运行构造样例证明某个具体误编辑。Aline 可研究紧凑表示，必须自行保存完整来源关系、受检宽度和 newtype，失败回源码安全入口。

### 4.4 删除、回车与隐藏标记

**事实：**map.rs:275–332 `deletion_ranges/normalize`（`splat@4e5ead0:src/preview/map.rs:275`） 为覆盖的文字生成删除片段，atom 覆盖整个来源，允许合并的跨段操作加入中间源码范围；bridge.rs:114–145（`splat@4e5ead0:src/preview/bridge.rs:114`） 进一步删除内容已空的格式容器及定界符。

editing.rs:485–525（`splat@4e5ead0:src/editor/editing.rs:485`） 只接管 Enter、软换行、Tab/Backtab、块首 Backspace 等结构操作，其余交原生文本视图。`Bridge::newline`（149–224）分列表续项、空项退出、任务续项、引用、代码、表格和普通段落；`split_inlines`（242 起）拆开并重开格式容器。`delete_at_block_start`（332–397）处理标题/列表/引用前缀，防止代码/表格边界被普通退格破坏。

**差异与适配：**Aline 阅读态隐藏标记、开块显示标记，应把“命中→打开源码→同源锚点恢复”作为验证对象，不引入 Splat 的全时隐藏输入、空容器自动清理和富文本自动转义。Splat 折叠 callout 的点击实际改 `+/-` 源码（navigation.rs:35–42），不是无内容变化的 UI 折叠；此扩展不进入 Aline V1。

### 4.5 IME、历史、粘贴和命令

**事实与限制：**editing.rs:260/419（`splat@4e5ead0:src/editor/editing.rs:419`） 仅在 source 待定格式处理及 preview 回写处检查 `hasMarkedText()`；预览组合未结束直接返回。源码有共享文档 undo manager（mod.rs:296–299（`splat@4e5ead0:src/editor/mod.rs:296`）），预览关闭独立 undo、source 开启 undo（window.rs:366–367（`splat@4e5ead0:src/editor/window.rs:366`）），`apply_edit` 的 typing 分支用 `insertText`，命令分支用 `breakUndoCoalescing`。没有看到 Aline 的 900ms 分组或 ED-01 发布等待协议；不能推断原生撤销会自动满足它们。

预览剪贴板声明 String/PNG/TIFF，先进入图片处理函数，再调用原生 paste，见 views.rs:116–133（`splat@4e5ead0:src/editor/views.rs:116`）；图片处理函数实际在有文字时优先返回给文字路径（editing.rs:662–701（`splat@4e5ead0:src/editor/editing.rs:662`）），图片成功写盘后才插引用，失败仅蜂鸣，未命名文档要求先保存再粘贴。Aline 不照搬后两项，继续按临时图片/恢复及可理解错误契约实现。普通文字随后走 Bridge 字面转义。格式命令经 `Action` 分派（editing.rs:35 起（`splat@4e5ead0:src/editor/editing.rs:35`）），纯逻辑 `commands` 返回 Edit/Pending，再由宿主应用。菜单及快捷键元数据见 actions.rs（`splat@4e5ead0:src/editor/actions.rs:1`）；不能把其快捷键和额外命令照搬进 Aline UI-04 清单。

专用公式/图表输入与普通输入不同：atom_edit.rs:204–244（`splat@4e5ead0:src/editor/atom_edit.rs:204`） 禁用历史注册、镜像当前源码预览；关闭时（258–316）先恢复原文，再把最终内容作为一个 Edit，Escape 则丢弃。**不建议照搬**：与 Aline Escape 保留输入、按输入组记步、快照不封口不兼容。

### 4.6 两个需要反向借鉴的具体行为

1. **陈旧预览的输入被丢掉。** `preview_changed` 在 State 不属于当前 Snapshot 时 invalidate preview 并返回（editing.rs:422–426）。selftest/scenarios.rs:604–614（`splat@4e5ead0:src/editor/selftest/scenarios.rs:604`） 明确验证源码输入 Q 后立即在未同步预览输入 Zzz，最终两处都没有 Zzz。事实是实现与测试采用此恢复策略；**推断**是 Aline 不能照搬，应按引擎 §8/架构 R-5-06 保留当前输入、禁旧范围写入并重新定位。
2. **表格导航有源码副作用。** table.rs:43–89（`splat@4e5ead0:src/markdown/table.rs:43`） 从源单元格切片 trim 出内容；`navigate/move_down`（232–267）调用 `replace_table`（216–229），由 `Table::render`（100–144）重新对齐整表。即使只导航到已有格，也可能改变空格及分隔行。它使用源片段，但仍规范化整表；Aline 只能把它当结构导航问题样本，不能继承“每次导航顺便整理”。

## 5. 解析、局部更新、性能与异常输入

### 5.1 真正的增量范围

**事实：**incremental.rs:38–159（`splat@4e5ead0:src/markdown/incremental.rs:38`） 从旧/新源码公共前后缀求变化，回退到合法 UTF-8 边界；找到周围顶层块，重解析窗口并比较首尾边界块，未稳定则扩窗，之后移动尾部所有源码范围。脚注/跨空行注释直接全量；Front matter 与引用定义有额外条件；窗口大于半篇且全文超过 4096 字节也回退。不能概括为“空行总能隔离 Markdown”。

调用路径不相同：update.rs:333–395（`splat@4e5ead0:src/editor/update.rs:333`） 预览焦点时前景 `update → reparse_and_color`；源码焦点的后台路径 `parse_and_color` 为全量解析。后台开算前比较 latest generation，主线程采纳再检查 generation/源码/忙碌和预览编辑状态（398–405）；这不等于运行中的解析已经可中断，也没有 Aline 完整文档/路径/配置采纳键。

robustness.rs:430–473（`splat@4e5ead0:tests/robustness.rs:430`） 随机编辑后比较局部/全量的 Doc、着色、复用布局和所有坐标，并要求确实触发局部路径，脚注排除另走全量。**可直接借鉴验证方法**；局部解析实现需另做 Aline 的预处理映射、HTML、保护、脚注/引用/目录全局影响实验，不替换现行全量正确优先方案。

### 5.2 布局复用、失效和渐进展示

**事实：**layout.rs:318–387（`splat@4e5ead0:src/preview/layout.rs:318`） 复用解析窗口外的块，排除旧光标块，检查 HTML wrapper context，并通过 `shift_para` 更新定位。sync.rs:116–178（`splat@4e5ead0:src/editor/sync.rs:116`） 找块 key 的公共前后缀；预览直接改过的块即使 key 相同也纳入更新。key 匹配本身不是 Aline 所要求的源片段相等证明。

update.rs:25–28/558–605（`splat@4e5ead0:src/editor/update.rs:558`） 设置 changed-block 阈值 300、立即渲染附近 120 块；其余先放纯文本占位。render.rs:125–133（`splat@4e5ead0:src/editor/render.rs:125`） 的占位共用 run.text 和段尾换行，保证文字序列/UTF-16 长度与最终富文本相同。progressive.rs:16–18/78–158（`splat@4e5ead0:src/editor/progressive.rs:78`） 在可见块前 150、后 600 的窗口内按约 16ms 分批，连续块聚合替换；可见区另外立即补渲染。

**推断：**这减少 attributed string 替换成本，但不是 Aline 块虚拟化的替代品；Doc/Layout/纯文本占位仍可覆盖整篇。16ms 是循环时间片，不保证单个巨块操作在预算内完成。只研究“批量派生刷新、稳定投影、可见区优先”，不继承阈值，也不沿用原生文本存储的成本假设。

### 5.3 不完整语法、嵌套和资源边界

**事实：**异常 Markdown 仍交同一 Comrak 解析；自有树转换以 `MAX_DEPTH = 64` 限制深度，超出转原始文本，见 doc.rs:398–449（`splat@4e5ead0:src/markdown/doc.rs:398`），相关用例在 1358 行起；未匹配 HTML 原文测试在 1373 行起。此保护发生在 `parse_document` 之后，不能证明第三方解析本身已有 CPU/内存预算。随机用例包含不完整围栏、嵌套和标记，见 robustness.rs:15–99/105–124（`splat@4e5ead0:tests/robustness.rs:15`）。

图片另有字节/尺寸/分配限制：safe_image.rs:9–39（`splat@4e5ead0:src/editor/safe_image.rs:9`）；不等于已统一覆盖文档、映射、历史、表格预算。后台 parser 用 thread-local 队列、`catch_unwind` 并有 spawn `expect`（update.rs:64–86）；large_alloc.rs:6–98（`splat@4e5ead0:src/large_alloc.rs:6`） 自定义全局大块 mmap allocator。两者与 Aline 引擎零线程/零全局状态、生产无 panic、自有 unsafe forbid 均不能直接兼容。

**性能证据限制：**有忽略的 10 MB 局部更新计时测试（robustness.rs:545 起）和 xtask/src/main.rs:787 起（`splat@4e5ead0:xtask/src/main.rs:787`） 的真实窗口 bench 入口；部分后台结果应用计时见 update.rs:407–415。本轮未运行，不引用 README 跑分，不据此宣称长文达标或得出 Aline 性能数字。

## 6. 文件保存、保真、外改、依赖与测试

| 主题 | 源码确认的事实 | 对 Aline 的关系与限制 |
|---|---|---|
| 解码与编码 | document.rs:16–56/97–112/137–143（`splat@4e5ead0:src/document.rs:16`）：严格 UTF-8；剥 BOM 后保留格式标志；只有所有 LF 都带 CR 时记 `crlf=true`，保存读取 `exact_text` 并编码 | BOM/纯 CRLF 有例证。混合行尾沿用现有文本，与 Aline 编辑后多数派统一不同。无原 bytes 未修改快路径不等于已证实破坏保真；有限测试（295–313）也不等于完整字节门禁 |
| 外部变化 | document.rs:120–134/189–255（`splat@4e5ead0:src/document.rs:189`）：通知+两秒 mtime/size 轮询，实际比较内容；有本地修改直接返回，无修改则重载并清 undo | 可参考通知不完备时再检查的形状；Aline 按聚焦/写前检查、差异冲突及 TA-02 可撤销重载实现，不能清历史；mtime/size 也不能独自充当文件版本 |
| 保存可靠性 | NSDocument `autosavesInPlace=true`，`dataOfType` 返回编码后的 source 数据 | AppKit 的实际替换/权限/磁盘失败/写前竞争保证本轮未实测；注释“AppKit 会询问”不是跨平台事务证明，不迁移为 Aline 已批准机制 |
| 图片粘贴 | views.rs:125（`splat@4e5ead0:src/editor/views.rs:125`）优先图片路径，文字走原生 paste/Bridge | 仅能作为“资产与正文分开处理”线索；未命名临时资源、恢复、另存为与失败原子性仍按 Aline V-08，不从这条入口推断事务完整 |
| 依赖 | Cargo.toml:33–45（`splat@4e5ead0:Cargo.toml:33`）；Cargo.lock:114–115（`splat@4e5ead0:Cargo.lock:114`）锁 Comrak 0.59.0；objc2/AppKit/WebKit 为平台基础 | Comrak 不替代/并入 Aline 的 pulldown-cmark；AppKit/WebKit/unsafe 不因是 Rust 就被准入 |
| 捆绑第三方资源 | vendor.lock:1（`splat@4e5ead0:vendor.lock:1`）记录 URL/版本/SHA-256；xtask/src/main.rs:695–739（`splat@4e5ead0:xtask/src/main.rs:695`） `vendor check` 校验本地哈希，fetch 才下载 | 可参考实际选定资源的版本/摘要账本；不是 Cargo offline、许可证审查或 Aline 引入 JavaScript 运行时的许可 |
| 无头/集成测试 | `lib.rs` 暴露纯模型；robustness.rs 检查范围/字符边界、增量差分；sync.rs:219 起用 Vec<u16> 对照同步副本；selftest/scenarios.rs 驱动真实窗口 | 测试分层与独立对照可借鉴。Splat 测试允许 unwrap/expect，不能照搬到 Aline 引擎；原生 selftest 不替代真实中文输入和三平台验收 |

## 7. 迁移建议对照表

类别是本轮研究结论：**直接**指方法与当前规范兼容，仍需 Aline 自写和验证；**改造**指必须更换语义/边界后再实验；**不照搬**指原设计在当前范围不适用。每行证据指向前文实际调用链，不以 README 为依据。

| ID / Splat 机制与证据 | 类别、Aline 落点、阶段 | 收益 | 成本与风险 / 可验证方式 |
|---|---|---|---|
| SP-01 插入/选择/删除分别规划（§4.1/4.3/4.4；map:143/275，Bridge:25/99） | **直接：职责拆分与夹具**；markdown 映射 + edit 计划；引擎 §7～9、R-4/5；M0～M2 | 避免点击锚点冒充可删来源，缩小宿主职责 | 中：阅读/输入投影要切换；V-07 对实体、隐藏标记、段尾、Gap、atom 分别检验；不继承自动清理 |
| SP-02 Verbatim/相对偏移（layout:152–189） | **改造**；引擎 §7.1、RS-02；M0 建正确表示，M1/V-05 再测压缩 | 原样文本省逐字映射分配 | 中：受检宽度/来源区间/精确性/字素不得丢；与未压缩映射逐点差分并测内存；align 算法不采用 |
| SP-03 单 Edit compose、保留中间源切片（Bridge:484） | **改造**；buffer/edit、引擎 §9；M0/M2 | 明确 before/after 选区与一次提交 | 中：加 WorkingVersion、权限、整批预检和 TA-01；正逆补丁/拒绝全状态不变；不能把所有多补丁命令强制成一段 |
| SP-04 窗口重解析及全局回退（incremental:77）；全量对照（robustness:430） | **直接：差分测试；改造：局部算法**；引擎 §12、架构 §5.3；M0 正确性、M1/V-05 评估 | 提前发现非局部语义失效，优化有权威对照 | 高：需 pulldown + 改写/HTML/保护全依赖；局部/全量在树、范围、映射、保护、展示一致才讨论收益；不自动启用 |
| SP-05 编辑触及块强制失效、复用重定位（sync:116；layout:318） | **直接：失效验证方法**；R-7-02/03、UI 调度；M1～M2 | 防止相同 key/外观掩盖过期范围 | 中：保留源片段/配置证明，补丁后逐项比对命中；不能仅 hash 判等 |
| SP-06 同文字占位、邻近优先与批量替换（render:125；progressive:78） | **改造**；aline-ui、引擎 §12；M1/V-05/V-09 | 降低派生样式/资产刷新次数，保定位 | 中高：GPUI 成本不同、巨块仍可能阻塞；占位升级前后逐字/映射相同，图片几何另验锚点，测排队/最长停顿；不复制阈值 |
| SP-07 旧预览输入丢弃（editing:422；selftest:604） | **不照搬；直接取反例**；引擎 §8.3、R-5-06；M2 | 用竞争样例验证不丢输入 | 中：保留输入并重新定位/冲突提示；V-07 比对文本/草稿/组合/选区/历史，无旧范围写入 |
| SP-08 表格导航整表 render（table:216–267） | **不照搬；改造导航与结构规划**；引擎 §9.2、UI §8；M3 | 分清纯移动与显式结构变更，保护原写法 | 中高：需源格/分隔/前缀片段模型；已有格 Tab 只移选区、bytes/history 不变；新增行/显式整理另有补丁金样 |
| SP-09 原生 marked-text/undo、atom 关窗一步和 Escape 丢弃（§4.5） | **不照搬**；引擎 §8～10、TA-01/ED-01；M2/M4 | 明确不能靠控件默认历史/IME 保证产品契约 | 高：GPUI 适配与真实输入矩阵；900ms/IME 分组、请求等待取消、模式/焦点交错分别验 |
| SP-10 深度降级为原文（doc:398–449），图片预解码限额（safe_image:9） | **改造**；V-06、引擎 §12.3；M0～M4 | 在错误边界保留源入口，预算前置 | 中：上游解析尚可能先耗尽资源；检查前/增长中预算，超限不截断，数值重新测 |
| SP-11 BOM/CRLF 和通知+轮询（document:16/189） | **改造：只借问题样本**；source/UI、SAFE/TA-02；M0/M2/M5 | 覆盖存储格式与漏通知 | 中高：Aline 保留原 bytes、多数派、写前校验、可撤销重载；V-08 注入同尺寸外改、失读及保存竞争；不移植 2 秒策略/清历史 |
| SP-12 资源版本/摘要账本与分层测试（§6） | **直接：核验方法**；Rust §12/15、M0/M4/M6 | 可复查输入和最小失败样例 | 低～中：仅对实际准入资源适用；篡改摘要失败、许可另核；不把 selftest 当 IME/性能通过 |
| SP-13 全时富文本、Comrak、WebKit、全局分配器（§3/4/5） | **不照搬**；PRD §2/8、AGENTS、RS-04 | 保持单真源/单解析器/无 WebView 与平台边界 | 改换这些需新的范围/架构裁决；当前无需为保留它们发起裁决，研究即止于机制参考 |

## 8. 按里程碑推进的后续工作

| 阶段 | 建议优先顺序与产物 | 对应验证 |
|---|---|---|
| M0 | 先全量解析及来源金样；补丁/选区/UTF-16 非法端点、原 bytes、深嵌套原文回退；记录紧凑映射实验接口，不预装 Comrak/新依赖 | V-06/V-07；E-T01～05/08～11/16/17 |
| M1 | 实际 GPUI 点击到源锚点；原样/变换映射内存对照；布局复用重定位和占位一致性；按阶段测快照/解析/布局/应用/首屏 | V-01/V-05/V-06/V-09，先 macOS |
| M2 | 最先覆盖陈旧投影与当前输入竞争，再覆盖双模式/900ms/IME/拒绝/保存等待；保留源选区方向；文件最小闭环不可后置 | V-07/V-08；AC-02～06/08 |
| M3 | 表格纯导航不重排、列表/引用/代码 Enter/退格与保护边界；显式结构/整理另测源补丁和一步撤销 | V-07/V-09；AC-04/09/17 |
| M4 | 资产源码/版本/失败占位和再启用；原生公式图表候选后端、导出及临时图片全链路；实际第三方资源账本 | V-02～04/V-06/V-08；不带入 MathLive/WKWebView |
| M5 | 把竞争样本扩到路径、多标签、多窗、关闭/外改；仍按稳定 DocumentId/路径代次采纳 | V-01/V-08/V-09 |
| M6 | 固定样本/seed 全量回归；Release 冷热、P50/P95、最长停顿和内存；三平台实测，不借第三方成绩 | V-01～V-09 发布门禁 |

**需验证、无需重新产品裁决的机制：**紧凑映射是否省内存；局部窗口何时安全；占位是否保持同一投影；输入事件能否提供可靠受检范围；中文 IME、双向选区和真实几何。这些纳入现有 V，不新建“已通过”编号。

**需另行裁决才可能采用的变化：**若未来拟全时隐藏标记编辑、引入 Split/独立 Preview、改变输入/撤销/表格保真行为、换解析器或引入 WebView，应带具体提案按当前决定流程处理。本文没有提出采用这些变化，没有新增必须由用户裁决的产品问题；现行决定不重开。

## 9. 未证实事项与研究盲区

- 未实测中文/日文等 IME 的候选窗、确认/取消、跨块、保存中组合与 emoji ZWJ/组合附加符的原生删除；scalar/代理对测试不足以证明字素行为。
- 未实测全文/局部解析速度、窗口首屏、滚动、长文峰值内存、巨型单块时限及 Splat README 性能数字；也未证明全部解析阶段均受预算限制。
- 未验证 AppKit NSDocument 的磁盘故障、原子替换、权限复制、并发外改和恢复保证；未完整审计图片粘贴/未命名文档/另存为资源事务、全部导出产物、远程请求和沙箱安全。
- 未穷举 Comrak 每项扩展的来源精确性、实体/HTML/异常嵌套和所有格式命令。已有不匹配对齐的风险判断基于失败出口，不冒充已复现错误。
- 未实测反向选择的 anchor/head 保留；Splat 多处使用无方向 Range/NSRange，Aline 必须自行验证方向，不能据此直接断言 Splat 用户行为错误。
- 没有审计依赖的全部许可证/安全公告，也不把资源哈希校验等同供应链安全完成。本轮只静态分析用户指定本地提交。

## 10. 文档回写与验证边界

现行文档仅接收与既有契约相容的实施建议/测试细化：引擎的紧凑映射候选、操作来源区别和优化差分；UI 的纯导航/开块样本；V-05/V-07/V-08 的问题、方法、样本及通过条件。架构和文档入口链接本文并明确非规范。未修改 PRD 范围、AGENTS 铁律、已确认决定或 AC 条件；V-01～V-09 仍未执行。

文档交付检查包含新增/修改链接、章节标题、术语与 diff 空白检查、改动范围及 Splat 工作区只读核对。Aline 尚无 Cargo workspace，本轮仅改文档，不运行 Cargo 检查；不能把这些文档检查记为 V 实验完成。
