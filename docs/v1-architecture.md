# Aline V1 技术架构与实现细节

> 将 [v1-features.md](./v1-features.md) 的 9 组功能点映射到具体架构与实现。
> 引擎机制的调研依据与设计取舍见 [engine-design.md](./engine-design.md)（elymark 逆向分析），本文不重复论证，只给"怎么建"。
> Zed 源码（GPUI 生态最完整参考实现）的架构与实践调研见 [zed-notes.md](./zed-notes.md)——各章借鉴清单按"照搬/改造/不采用"分档，§5 有映射到本文各节的全景表；注意其 gpui 主线与 gpui-pre 存在版本代差。
> 实现期必须遵守的边界与纪律（【强制】/【建议】条款）见 [architecture-guide.md](./architecture-guide.md)；第 15 章收录 Ely 实证的 GPUI 实现要点，与该规范配套。
> 本文的里程碑取代 engine-design.md §2.13。

---

# 第 1 章 总体架构与 crate 划分

## 1.1 分层原则

1. **引擎四件套零 GPUI**（source/buffer/markdown/edit）：纯逻辑、无窗口依赖，全部行为可无头测试——这是比 elymark（引擎与 gpui 编排混在 ely-documents）最重要的结构改进。
2. **应用基础（theme/i18n/settings）零 GPUI**：纯数据模型，ui 层消费。
3. **插件层依赖 gpui**：渲染扩展点天然是 UI 层（要产出 GPUI 元素与资产），不强行抽象。
4. **编排核心（DocCore）是普通 struct**：模式、打开块、解析调度、查找状态都是纯状态机，aline-ui 只负责把它包成 gpui Entity 和接输入——行为测试不需要窗口。

## 1.2 crate 划分与依赖方向（严格单向）

```
app(bin) ──► aline-ui ──► aline-plugin ──► aline-edit ──► aline-markdown ──► aline-buffer
                │▲                        │
                │└──── aline-app-core ◄───┘（theme / i18n / settings，零 GPUI，谁都能依赖）
                └─ gpui

aline-source 独立（不进链）：文件 I/O 由 aline-ui / app 直接编排调用（2026-10-26 复核修订）。
```

| crate | 职责 | 依赖 |
|---|---|---|
| `aline-source` | 文件字节层：decode/encode（BOM/CRLF）、原子写 | 无（独立 crate，由 ui/app 编排调用） |
| `aline-buffer` | Buffer trait、StringBuffer、Selection、补丁式 History、锚点平移 | 无 |
| `aline-markdown` | 预处理改写器 → pulldown-cmark → mdast → 展示树 + sources 映射 + 块切分 + markdown 着色规则 | buffer |
| `aline-edit` | Command/Change、apart 协议、reread 校验、拒绝制、查找替换核心 | markdown |
| `aline-app-core` | 主题 token 模型 + 内置主题文件、i18n catalog + t()、设置 | 无 |
| `aline-plugin` | 插件注册接口、渲染器/命令/导出器注册表、五个内置插件 | edit, app-core, gpui |
| `aline-ui` | DocCore、Document 视图、块渲染、FoldedInlineEditor、行编辑器、IME、资产管线、Workspace/标签页/文件夹浏览器 | 全部 |
| `app` | main：窗口创建、菜单、设置读写、自动保存计时、崩溃恢复 | ui |

## 1.3 仓库骨架

```
aline/
  Cargo.toml                    # workspace，gpui 锁定快照版本
  crates/
    aline-source/src/{decode.rs, encode.rs, atomic.rs}
    aline-buffer/src/{buffer.rs, string_buffer.rs, selection.rs, history.rs, anchors.rs}
    aline-markdown/src/
      pre/{flow.rs, tex.rs, front.rs}     # 预处理改写器（front 见 §4.4 说明）
      build.rs mdast.rs                   # pulldown 事件 → mdast
      tree/{mod.rs, sources.rs, html.rs, html_block.rs, stack.rs}
                                          # 展示树 + 双向映射 + 行内 HTML/HTML 块/标签栈
      parse.rs                            # Parsed/Block/alike
      markdown_hl.rs                      # 源码模式 markdown 着色（纯规则）
      tests/{spec.rs, roundtrip.rs}
    aline-edit/src/{command.rs, format.rs, block.rs, lists.rs, tables.rs,
                    apart.rs, check.rs, find.rs}
    aline-app-core/src/
      theme/{mod.rs, palettes/{aline-default.toml}}
      i18n/{mod.rs, locales/{zh-CN.toml, en.toml}}
      settings.rs
    aline-plugin/src/
      {registry.rs, traits.rs, settings.rs, export.rs}
      built_in/{math.rs, mermaid.rs, table.rs, export.rs, highlight.rs}
    aline-ui/src/
      document/{mod.rs, visual.rs, source_mode.rs, open_block.rs, folded.rs}
      render/{mod.rs, blocks.rs, inline.rs, point.rs, assets.rs}
      editors/{line_editor.rs, text_input.rs, ime.rs}
      workspace/{mod.rs, tabs.rs, folder_pane.rs, open_events.rs}
      find_replace.rs
  app/src/main.rs
```

## 1.4 依赖清单（锁定）

见 engine-design.md §2.12。补充 V1 新增：`toml`（主题/文案/设置）、`rust-embed`（主题与文案打包）、`blake3`（资产缓存键哈希）、`directories`（配置/数据目录）、`rfd`（文件对话框——此前漏登记）。**不引入** notify（文件夹监听，V1 用聚焦刷新；engine-design §2.12 中标注为 V2 预留）、不引入 regex（查找替换是纯文本）。

---

# 第 2 章 应用壳层：窗口 / 标签页 / 文件夹浏览器 / 系统打开（功能组 1）

## 2.1 核心结构

```rust
// aline-ui/src/workspace/mod.rs
/// 一个窗口 = 一个工作区
pub struct Workspace {
    tabs: Vec<Tab>,
    active: usize,
    folder: Option<FolderPane>,
}

pub struct Tab {
    view: Entity<DocumentView>,     // 每个打开文件一个常驻实例
    path: Option<PathBuf>,
    title: String,
    dirty: bool,
}

pub struct FolderPane {
    root: PathBuf,
    show_all: bool,                 // 开关：是否显示不支持的格式，默认 false
    expanded: BTreeSet<PathBuf>,    // 展开的目录
    entries: Vec<FileEntry>,        // 懒加载的当前展开集
}
```

实现要点：
- **标签页切换不销毁实例**：`Entity<DocumentView>` 常驻（含 buffer/parsed/open/undo），切换只是换渲染分支。内存保护：非活动标签且已保存的文档可丢弃 `parsed`（下次激活重解析）。
- **dirty 传播**：订阅 `DocumentEvent::Changed` → `tab.dirty = true` → 标题加圆点；保存后清除。关闭脏标签弹确认（i18n 文案）。
- **文件夹浏览器**：gpui `uniform_list` 渲染扁平化的可见节点列表（目录树 → 缩进深度 + 路径），懒加载展开目录；**V1 不引入文件监听**——聚焦窗口时和自身操作（新建/保存/删除）后刷新；`notify` 留给后续。排序：目录优先、字母序。
- **支持格式判定**：扩展名白名单 V1 只有 `.md` / `.markdown`。`show_all == false` 时浏览器里根本不出现其他文件；`show_all == true` 时其他格式可见但置灰，点击 → toast `ui.unsupported_format`（"不支持的格式"）。
- **点击 .md**：已在某标签打开 → 激活该标签；未打开 → 新标签并激活。

## 2.2 系统文件关联与多窗口（Finder 双击 → 新窗口）

- gpui 在应用启动后注册打开事件回调（`cx.on_open_urls` 一类，具体 API 名以锁定的 gpui 快照源码为准；macOS 上对应 Finder 双击、拖到 Dock 图标、"打开方式"）。
- 处理逻辑：

```rust
fn on_open_files(paths: Vec<PathBuf>, cx: &mut App) {
    for path in paths {
        // 新窗口，而不是塞进现有窗口的标签
        let folder = path.parent().map(Path::to_path_buf);
        cx.open_window(WindowOptions::default(), |cx| {
            let mut ws = Workspace::new();
            if let Some(f) = folder { ws.set_folder(f); }   // 自动定位所在文件夹
            ws.open_tab(path, cx);                          // 打开该文档并激活
            ws
        });
    }
}
```

- 平台配置：macOS 在 `Info.plist` 声明 `CFBundleDocumentTypes`（`.md` / `.markdown` / `net.daringfireball.markdown`）；Windows（NSIS/MSIX 注册 `ProgId`）与 Linux（`.desktop` 的 `MimeType`）列入打包任务，代码路径同一套 `on_open_files`。
- 命令行参数 `aline [path...]` 走同一函数——单文档打开与双击打开是同一条路。
- **单实例（Win/Linux）**：启动时在用户数据目录创建锁文件 `aline.lock`（加锁失败 = 已有实例）→ 把打开参数经命名管道（Windows）/Unix domain socket 转发给既有实例后退出；转发失败（陈旧锁）则抢占接管。macOS 由系统保证单实例。消除双进程并发写 `session.json` 与同一文档互覆的窗口（2026-10-26 复核新增；M5 三平台实测）。

## 2.3 每窗口状态持久化

设置目录（macOS `~/Library/Application Support/Aline/`）存 `session.json`：每个窗口的 folder、tabs 路径、active tab。启动时恢复（文件丢失则静默跳过）。V1 不存滚动位置/展开状态。

---

# 第 3 章 文件层实现（功能组 1）

`aline-source` 全部纯函数 + 小状态体，照搬 elymark 已验证语义（engine-design.md §2.9）：

```rust
pub struct Source { bytes: Vec<u8>, bom: bool, crlf: bool, pub text: String }

impl Source {
    pub fn decode(bytes: Vec<u8>) -> Result<Source, DecodeError>;  // UTF-16 等明确报错
    pub fn encode(&self, current: &str) -> Vec<u8>;                // 未编辑 → 原 bytes
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()>;
// 同目录临时文件 → fsync → rename；失败留 .tmp 供排查
```

- **自动保存**：app 层全局计时器，每文档 1s 防抖；仅对"有路径且 dirty"的文档写盘（写盘前先 flush 打开块，见 §6.2）。无路径文档（如有）不自动保存。
- **外部修改检测**：窗口聚焦时对每个有路径标签做 `mtime + size` 检查（廉价）；变化且本地也 dirty → 三方对比对话框（磁盘/内存/diff），V1 用 `similar` 生成简单 diff 视图。
- **崩溃恢复**：每 10s 把 dirty 文档文本（同样先 flush 打开块）写入 `recovery/<blake3(path)>.md` 槽位——dirty 判定、自动保存、恢复槽看到的是同一份文本，open 里的未提交编辑不会丢；槽位随对应文件正常关闭而清理（防陈旧堆积）；下次启动发现槽位 → 提示恢复（恢复内容只进内存，**绝不自动覆盖磁盘文件**）。
- 读写时序保证：保存动作串行化到文档所属队列（避免自动保存与手动保存竞争——elymark 有对应测试 `a_save_marks_the_text_it_writes`，照做）。

---

# 第 4 章 编辑模式（功能组 2）

## 4.1 两模式，共享真源

```rust
pub enum Mode { Visual, Source }   // 没有第三个
```

- 同一个 `DocCore`（见 §5.1）持有唯一 buffer 与 History；模式只是**视图分支**。切换 = 记住光标位置（源码坐标）→ 换视图 → 恢复光标。零字节损耗由结构保证（buffer 不动）+ 测试锁定（`switching_modes_writes_no_byte`，照搬 elymark 用例）。
- Source 模式 = 行编辑器（`editors/line_editor.rs`）：`uniform_list` 逐行虚拟渲染、`markdown_hl` 着色、多选区可后置（V1 单选区）、复用同一 IME 与 History。**不实现** elymark CodeEditor 的 minimap/folds/vim 等全家桶。
- Visual 模式下 >4MiB 文档自动以 Source 打开（`LARGEST_READ` 阈值可配置），状态栏提示。

## 4.2 front matter 的最小处理（功能上"不做"，但要防渲染错乱）

`pre/front.rs` 保留 ~30 行检测：文件头 `---`/`+++` 围栏 → 渲染为等宽原文块、所有编辑命令拒绝（`EditRefusal::FrontMatter`）、块内打开即原文编辑。不做属性面板、不做解析。理由：Obsidian 风格笔记遍地都是 front matter，完全不识别会把首屏渲染成乱掉的正文/分割线；而保留识别的成本是一顿饭。

---

# 第 5 章 查找与替换（功能组 3）

## 5.1 状态与算法（aline-edit/src/find.rs，零 GPUI）

```rust
pub struct FindState {
    query: String,
    replacement: String,
    case_sensitive: bool,
    whole_word: bool,
    matches: Vec<Range<usize>>,   // 源码字节区间，升序
    stamp: u64,                   // 生成匹配时的 buffer.stamp()：编辑后整组过期，
                                  // 消费方必须校验（DocCore 发 MatchesInvalidated 并防抖重搜，§6.1）
    current: usize,
}

pub fn search(text: &str, q: &FindState) -> Vec<Range<usize>>;
pub fn replace_all(text: &str, q: &FindState) -> Change;  // 一组补丁 = 一步撤销
pub fn replace_one(text: &str, q: &FindState) -> Change;
```

- 纯文本匹配（**无正则**，V1 明确）：大小写不敏感用简单大小写折叠（先试 `eq_ignore_ascii_case`，CJK 天然无大小写）；整词 = 两端 char 边界检查 + 前后不是 `alphanumeric`。
- `replace_all`：收集所有替换区间后**升序一次** `buffer.edit(edits)` → 一个 undo 步（apart 协议天然支持多补丁）。倒序应用避免偏移漂移由 `buffer.edit` 内部统一处理（edit 接口要求升序不重叠，见 buffer trait）。
- 查询输入防抖 100ms 重搜；≤4MiB 全量扫描毫秒级，无需增量。

## 5.2 两模式的 UI 行为

- **Source 模式**：命中区间直接注入行编辑器高亮；Enter/Shift-Enter → 选区跳到 `matches[current]`，滚动跟随。
- **Visual 模式**：命中区间按块归组（`block_at`），渲染时通过 `DocCore` 把 `highlights: &[Range<usize>]` 传给块渲染器（映射层把源码区间转绘制区间——A/B 档绘制=源码，恒等直传；B+ 用 `drawn_at`）。当前项所在块 reveal 滚动 + 短暂高亮；**Enter 跳转时自动打开该块并把选区落在匹配文本上**（`Landing::At`）。
- 替换确认：单个替换在当前项执行并 advance；全部替换弹确认（含数量，i18n）。

---

# 第 6 章 可视化编辑引擎（功能组 4）

## 6.1 DocCore：编排核心（普通 struct，可无头测试）

```rust
// aline-ui/src/document/mod.rs —— 纯状态机，不持有窗口
pub struct DocCore {
    buffer: CodeBuffer,          // String + line starts + selections + History + IME 状态
    parsed: Arc<Parsed>,
    parse: ParseState,           // Idle | Debounced(Instant) | InFlight(u64 edits 计数)
    open: Option<OpenBlock>,
    mode: Mode,
    find: Option<FindState>,
}

pub struct OpenBlock {
    range: Range<usize>,         // 块源码区间（不含尾部换行）
    text: String,                // 打开态的块文本（打字先落这里）
    kind: OpenKind,
}

pub enum OpenKind {
    Plain,                       // 段落/标题/列表/引用：阶段 A/B 输入框，B+ 折叠编辑器
    Panel,                       // 代码/公式/front matter 源码面板：等宽原文
                                 // （HTML 块不在此列：走 Plain + markdown 着色，见 §6.7 ②）
    Split { debounce: bool },    // 块级公式/Mermaid：上编辑 + 下实时渲染
    Cell(CellAt),                // 表格单元格（单行）
}
```

aline-ui 的 `DocumentView: Render` 持有 `Entity<DocCore>` 的等价 gpui 包装（实际实现：DocCore 直接放进 Entity，行为函数与 Render 分文件）。事件：`Changed / Parsed / Pointed(Target) / EditRefused(EditRefusal) / DirtyChanged / MatchesInvalidated`（查找匹配因编辑过期时发出，消费方校验 `FindState.stamp` 后丢弃整组，防抖重搜）。

## 6.2 打开-提交循环（阶段 A/B 的主循环，全部照搬 elymark 语义）

```
点击渲染块（无 secondary 修饰键）
  → point()：gpui index_for_position → Place → source_at() → 源码字节
  → 已有 open 块先 commit
  → open_block(range, Landing)：
      OpenKind 按块类型定（Plain/Panel/Split/Cell）
      text = buffer.slice(range)
      光标 = Landing 映射（End / At(source_at)）
  → 打字：改 open.text（不碰 buffer）→ DocumentEvent::Changed（dirty 置位）
       Split 型：防抖 300ms 重渲染下半区
  → 失焦 / Escape / 点击其他块 / 整体命令 / 自动保存与恢复槽落盘前（先 flush open）
  → commit()：
      if open.text != 原块文本 {
          buffer.edit([(open.range, open.text)])     // 单区间 = 单步撤销
          edits += 1
      }
      reparse()（≤128KiB 同步；否则防抖后台）
      alike(old, new) → list.splice 只换变化块
```

- 打字期间全文不重解析（只动 open 块）——性能宽松的根本原因。
- **Escape = 提交**，与失焦同路径；不设"取消"手势，误操作走撤销——块与表格单元格同语义（2026-10-26 统一决策，§6.6）。
- Up/Down 在块边界把光标移交给相邻块（保持视觉列，`goal` 记忆）；命令产生的编辑用 `moved()/shifted()` 重映射存活光标与锚点。

Ely 实证补充的三条状态机细则（architecture-guide §5.4【强制】）：

- 【打开时机】只有"当前文本"的解析结果就绪才能 open；未就绪挂起 `waiting`，解析到达后补开——否则点击落在过期字节上（Ely `edit.rs:76`）。
- 【打开期间失效】打开期间文本被外部改变（非自己写回）→ `drop_open()` + log，绝不静默写回（Ely `edit.rs:161`）。
- 【焦点借出】菜单/对话框临时借走焦点不关闭打开块（`lent` 语义），归还后继续；子编辑器 Undo/Redo 用 `capture_action` 接管转发到文档历史（Ely `edit.rs:217,271`）。

## 6.3 命令框架（apart 协议 + 拒绝制 + reread）

```rust
// aline-edit —— 纯函数，无 GPUI
pub fn run(buf: &Buffer, parsed: &Parsed, sel: &[Selection], cmd: Command)
    -> Result<Change, EditRefusal>;

pub enum Command {
    Block(BlockCommand),      // Heading(u8) Paragraph Quote BulletList OrderedList
                              // TaskList Rule CodeBlock MathBlock ParagraphBefore/After Delete
    Format(InlineFormat),     // Strong Emphasis Code Strike Link
    Table(TableEdit),         // RowAbove/Below ColumnLeft/Right DeleteRow/Column
                              // MoveRow/Column Tidy Delete
    ListEnter, ListIndent, ListOutdent,
    TaskToggle { box_at: usize },           // 勾选框点击：'['..']' 内 空格↔x
    EditLink { at: usize, url: String, title: Option<String> },   // 弹窗产物
    EditImage { at: usize, url: String, alt: String },            // 弹窗产物
}
pub type Change = (Vec<(Range<usize>, String)>, Vec<Selection>);
```

执行协议（DocCore 内联实现，V1 命令计算都在毫秒级，无需后台线程；保留"快照-校验"防并发重解析错位）：

```rust
fn run_command(&mut self, cmd: Command) -> Result<(), EditRefusal> {
    let text_before = self.buffer.text().to_owned();
    let change = aline_edit::run(&self.buffer, &self.parsed, &self.buffer.selections(), cmd)?;
    if self.buffer.text() != text_before { return Err(EditRefusal::Changed); }
    // 高危命令：标题改动/块拆分 → reread 校验
    if needs_reread(&cmd) {
        let mut probe = self.buffer.text().clone();
        apply_edits(&mut probe, &change.0);
        if !visible_text_unchanged(&text_before, &probe, &self.parsed) {
            return Err(EditRefusal::RereadMismatch);
        }
    }
    self.apply(change);   // buffer.edit + 光标重映射 + history 记一步 + reparse
    Ok(())
}
```

- 格式 toggle 语义照搬 elymark：unwrap 删**实际写下的标记**（切真实源码区间）、wrap 写规范形式、空选区插标记对并居中；代码围栏取"最长连续反引号 + 1"。
- 列表 Enter：空项行尾 → 删标记退出列表；否则续接（有序自增、保留 `.`/`)`、原弹点字符、原缩进空白）。
- 跨代码块/公式/front matter 的命令 → 对应 `EditRefusal::*`，UI 层 toast（i18n key）。
- **Undo（补丁式）**：`History { undo: Vec<UndoStep>, redo: Vec<UndoStep>, saved_at: usize }`；`UndoStep { edits, inverse, selections_before, selections_after }`（撤销恢复 before 选区、重做恢复 after 选区）。打字合并窗口 `UNDO_GROUP = 900ms`（命名常量，单一来源，engine-design §2.2）；合并规则：仅"相邻的单区间插入/删除接续"可并入既有步（区间与 inverse 对应延长），替换、跨块、命令、commit、IME 组合结束一律新开一步（IME 组合过程整体为一步）。字节预算 trim 时清空 redo。**不采用** elymark 的全文快照。保存隐式封口当前合并组；dirty 判定 = `undo 栈深 != saved_at || open 未提交`（§15.2，不用 stamp）。

## 6.4 IME（硬要求，照搬 elymark 对接）

- `editors/ime.rs`：统一实现 `EntityInputHandler`（UTF-16 接口、marked text、bounds_for_range 候选窗定位），在编辑器元素 prepaint 时 `window.handle_input(&focus, ElementInputHandler::new(bounds, …), cx)` 注册。
- 组合态：`composing: Option<快照>`；开始组合打快照 + 设 marked 区间；组合期间不记 undo；`end_composition` 落定为一步撤销。
- 阶段 A/B：InputHandler 的文本 = 块源文本（恒等映射）。B+ 折叠编辑器：InputHandler 以**绘制文本**为对外基准（Zed multibuffer 模式），内部经 `sources` 映射翻译到源码坐标；组合文本插入源码并标记区间，块级重解析后绘制视图下划线高亮。

## 6.5 折叠编辑器阶梯（B+ 为挑战项，可整体后移）

- A/B：`editors/text_input.rs`（块源码输入，A 正常高亮 / B 标记符低对比）——**映射恒等，先保证全链路可用**。
- B+（`document/folded.rs`）：显示渲染文本，"光标所在最小格式区间"内标记显形；光标/点击/IME 全部源码坐标不变量；隐藏区间原子化（方向键跨 run）；`position_for_index`/`index_for_position` 用 GPUI 原生接口（已验证存在）。
- C：谓词改"全部隐藏" + 退格 unwrap 规则表（engine-design.md §2.4）。
- **行内公式/图片/链接不进折叠编辑器**（打开块内显示源码，阅读态渲染）——已从该组件范围裁剪。

## 6.6 表格

- 渲染真实表格；单元格点击 → `OpenKind::Cell` 单行输入，Enter/Escape 提交写回该格源码区间（Escape 与块打开态同语义 = 提交；取消走撤销，2026-10-26 统一决策）。
- 整表 12 种 `TableEdit` 全部实现为"表格源码区间整体重写"（生成新表文本 + 一次 buffer.edit），附带 `TableRefusal`（如列数不一致时拒绝整理）。
- 单元格内 `\|` 转义在解析层特判（tree/mod.rs 的 Pipes escapes），保真测试覆盖（照搬 elymark `a_closing_backslash_keeps_the_table` 等用例）。

## 6.7 HTML（V1，D-2026-10-26-13）

**两套语义，同一份源码**（机制逆向见 engine-design §1.10，落点与里程碑见 engine-design §2.17）：

**① 行内 HTML → 样式降维**（`tree/html.rs` + `tree/stack.rs` + `render/inline.rs`）

```rust
// 标签 → Style 映射表（全部走 StyledText::with_highlights，无自定义 Element）
u → Underline | b/strong → Strong | i/em → Emphasis
s/del/strike → Strike | code/kbd → Code | mark → Mark | a → Link
// 其余标签按 GFM tag filter 显示为文本；br → Break
```

- **嵌套修复**（HTML5 式）：`reopens(name)` 允许 `a/b/code/em/i/s/strike/strong/u` 在交叉嵌套时重开（**`mark` 不在内**，`a` 虽在但被 `<a>` 互斥规则盖住）；`breaks(name)` 的 32 个块级标签（`address/article/aside/blockquote/center/details/div/dl/dd/dt/figcaption/figure/footer/h1-h6/header/hr/li/main/nav/ol/p/pre/section/summary/table/tr/ul`）两侧产生换行；`closes_p(name)` 的 39 个标签（含 `fieldset/form/search/hgroup/listing/menu/dir/dialog`）隐式闭合 `<p>`；**`<a>` 互斥**——新 `<a>` 先 `end("a")`，且无 `href` 的 `<a>` 不计 `Style::Link`。
- **实体解码**：`Escapes::Html` 标记的片段在反向读取时经 `entities::ENTITIES` 表（去前导 `&` 建 `HashMap`，最长名 32 字节）解码——命名带/不带分号（legacy 仅正文，属性值不认）、十进制 `&#65;`、十六进制 `&#x41;`/`&#X41;`、Windows-1252 C1 块（0x80–0x9F）重映射、非法码点 → `U+FFFD`；解码出的**每个**字符都指向实体 `&` 的字节。
- **过滤标签**：`title/textarea/style/xmp/iframe/noembed/noframes/script/plaintext` 内容原样显示为文本，永不解析、永不执行。

**② HTML 块 → 浏览器式文本提取 + 点击编辑**（`tree/html_block.rs` + `document/edit.rs`）

- 解析：`mdast::Node::Html` → `html_block_text()` → 显示树里是 `Node::Paragraph(Inline)`（**没有** `Node::Html` 变体）。
- 绘制：标签被消费、可见文字保留、块内行内样式（`<b>`→Strong 等）与实体/emoji 照常生效、`<!-- -->` 注释丢弃、`br/div/h1-h6/hr` 等 32 个块级标签产生换行、空白经 `owed` 延迟机制按浏览器规则折叠、过滤标签的**标签本身**（含属性）按字面文本写出（不是吞到闭合标签的"死区"——`<script><b>x</b></script>` 显示为 `<script>` + 粗体 `x` + `</script>`）。
- 打开：**通用 `Open` 路径，无 HTML 专用分支**（`range` = 块源码区间，`cell = None` 多行）；`TextInput` 显示源码原文 + markdown 着色；落点 `caret = at - range.start`（`at` 已是映射出的源码字节，不再走映射）。
- 提交：单区间 `editor.edit([(range, written)])` = 单步撤销；写回后立即校验 `text.get(start..start+len) == written`，不成立则回滚计数并报错。
- **拒绝制**：两条路径都读 **mdast**（`block.rs:12` 为 `use crate::markdown::mdast::Node;`）——① 块命令（标题/引用/列表/代码块…）走 `block.rs` 的 `refusal()`，HTML 块命中 `Node::Html(_) => EditRefusal::Html`；② 行内格式走 `format.rs` 的交叠特判（`holder(...).is_some() && held` 或 `touches(true)`）。显示树里 HTML 块是 `Node::Paragraph`，那只影响渲染与落点，不影响拒绝判定。
- **V1 不做的**：`style`/`class` 不产生视觉差异（`href` 是唯一被使用的属性，参与 `Style::Link` 与 Cmd/Ctrl+点击）；`<script>` 不执行；不做 DOM 布局。后续"上编辑下渲染"双视图见 engine-design §2.17.9。

---

# 第 7 章 渲染层与资产管线（功能组 5）

## 7.1 块渲染与虚拟化

- `render/blocks.rs`：`BlockKind` → 分支函数，全部 `div + StyledText(with_highlights) + InteractiveText` 组合；标题分级字号、引用竖线、Alerts 五色面板、`[toc]` 可点击目录、脚注上标、任务列表图标（可点 → `TaskToggle`）。
- 虚拟化：gpui `list`（变高块），`ListState::new(1, Top, 40rem overdraw)`；`reveal_at(标题锚点/查找命中)` 滚动定位。
- 增量：`alike(old, new)` 头尾配对 → `splice`；未变块的元素与映射全部复用。
- 点击：`render/point.rs` 用 `index_for_position` → `Place` → 源码字节；Cmd/Ctrl+点击链接 → `cx.open_url`（打开失败 toast）。

## 7.2 资产管线（全部异步，统一模式）

```rust
// render/assets.rs —— 每个"重活"一个 Asset，键里必须含主题
struct MathAsset { source: String, display: bool, theme: ThemeKey, scale: f32 }
struct DiagramAsset { source: String, theme: ThemeKey, scale: f32 }
struct HighlightAsset { lang: String, text_hash: blake3::Hash, theme: ThemeKey }
struct PictureAsset { origin: PictureOrigin, hash: blake3::Hash }
```

| 资产 | 后台任务 | 失败降级 |
|---|---|---|
| 数学 | ratex parse→layout→SVG → usvg 墨迹框 → gpui svg_renderer → RenderImage | 错误面板（红字 + 等宽原文，可点编辑）；`catch_unwind` 兜底 |
| Mermaid | mermaid-rs-renderer → SVG → 同上 | 同上 |
| 高亮 | syntect → `Vec<(Range, HighlightStyle)>`（>64KiB 块降级无高亮） | 纯文本 |
| 图片 | 本地相对路径（相对文档目录）/`file:`/`data:` base64 解码 → gpui ImageAssetLoader | 占位条 + alt |

- 行内公式/图片/链接：**阅读态渲染，打开块内显示源码**（等宽 + 底色样式提示）——决策已定，折叠编辑器不含图元混排。
- 块级公式/Mermaid 打开态 = `OpenKind::Split`：块内上下布局（非浮层），上=源码输入、下=渲染结果，打字防抖 300ms 触发资产重渲染（键含新 source）。

---

# 第 8 章 主题系统（功能组 7）

## 8.1 Token 模型（aline-app-core/src/theme，零 GPUI）

```rust
pub struct Theme {
    pub id: &'static str,               // "aline-default"
    pub mode: Mode,                     // Day | Night | Eye
    pub tokens: Tokens,
}
pub struct Tokens {
    // Color / 字体族是 app-core 自有类型（如 Color { r, g, b, a: u8 }）——app-core 零 GPUI
    // （机器可查判定：cargo tree -p aline-app-core -i gpui 为空），到 gpui::Hsla /
    // SharedString 的转换只发生在 aline-ui（impl From<Color> for Hsla）。
    // 基础
    pub background: Color, pub surface: Color,      // surface = 块面板/输入框底
    pub foreground: Color, pub muted: Color, pub subtle: Color,
    pub accent: Color, pub link: Color,
    pub selection_bg: Color, pub caret: Color, pub border: Color,
    // 代码
    pub code_bg: Color, pub code_border: Color,
    pub syntax: SyntaxColors,           // comment/keyword/string/number/function/type/operator/punctuation
    // 结构
    pub quote_line: Color, pub table_border: Color, pub table_head_bg: Color,
    pub alert: [AlertColor; 5],         // note/tip/important/warning/caution：边线+图标+标题
    pub error_fg: Color, pub error_bg: Color,
    // 特殊块
    pub diagram_bg: Color, pub diagram_fg: Color, pub formula_bg: Color,
    pub marker: Color,                  // 阶段 B 标记渐隐色（低对比）
    // 字体
    pub ui_family: Arc<str>, pub mono_family: Arc<str>,
    pub scale: f32,
}
```

**主题 = TOML 文件**（rust-embed 打包），一个文件内含三段模式：

```toml
# palettes/aline-default.toml
id = "aline-default"

[day]
background = "#ffffff"
foreground = "#1f2328"
accent     = "#0969da"
# ……全部 token

[night]   # 同一套 token 的夜间调色
[eye]     # 护眼 = 独立调色（米黄底、暖前景），不是滤镜
```

Token 四组（外加字体族与缩放）：

| 组 | 内容 | 消费者 |
|---|---|---|
| 基础面 | background / surface / foreground / muted / accent / link / selection_bg / caret / border | 一切组件 |
| 代码 | code_bg / code_border + syntax 8 色 | 代码块、源码模式 |
| 结构 | quote_line / table_border / table_head_bg / alert 5 色 / error_fg / error_bg | 引用、表格、Alerts、错误面板 |
| 特殊块 | diagram_bg/fg / formula_bg / marker（阶段 B 标记渐隐色） | 数学/图表资产、折叠编辑器 |

语义化的意义：渲染器里永远不出现具体色值，只出现 `theme.tokens.accent`——换主题零代码改动。

## 8.2 加载与运行时（gpui Global 机制）

- 启动：rust-embed 读出 TOML → 解析成 `Vec<Theme>`（hex → app-core `Color`，解析失败的主题跳过并 log）；`Color → gpui Hsla` 的转换只发生在 aline-ui 渲染入口（app-core 零 GPUI，§8.1）。
- 活动主题 = 设置项 `(theme_id, mode)` 选中某段 → 构造 `ActiveTheme` → `cx.set_global(...)`。
- 各视图/组件用 `cx.observe_global::<ActiveTheme>()` 订阅；切换 = 一次 `set_global` → 所有订阅者下帧重绘，**包括正在编辑的块**（其渲染同样读 global）。模式切换即时全窗口生效，无重启。

## 8.3 三个接线点（实现的关键）

**① 源码模式 markdown 着色——引擎零颜色依赖。** `aline-markdown::markdown_hl` 只产出语义枚举（Heading1、Strong、Link、CodeSpan…），UI 层查 token 表映射颜色与字重。着色规则的测试不需要主题，主题测试不需要解析器。

**② syntect 代码高亮——程序化生成 syntect Theme。** syntect 只认自己的 `syntect::highlighting::Theme` 类型：主题加载时用 `SyntaxColors` 生成一组 ThemeItem（scope 选择器 → 颜色），不依赖外部主题文件；`SyntaxSet` 用 two-face 扩充集 + `LazyLock` 懒加载。高亮输出 `Vec<(Range, HighlightStyle)>`，后台资产按块缓存。

**③ ratex / mermaid-rs——颜色烘焙进 SVG，缓存键含主题。** 这两个库在渲染时把 foreground/accent/background 写死进 SVG，所以资产键必须含主题：

```
key = math:{source}:{display}:{theme_id}:{mode}:{相关token哈希}
```

切主题 → 键变 → 资产后台重渲染，期间旧图垫底显示、新图就绪后替换。

## 8.4 切换端到端时序

```
用户点"夜间" → 设置持久化(theme_id, mode) → resolve(theme.toml[night])
  → cx.set_global(ActiveTheme) → 全窗口下帧重绘
  → 所有含 ThemeKey 的资产键失效 → 后台重渲染 SVG/高亮 → 逐块换色
```

解析、buffer、映射完全颜色无关——主题是纯 UI 层概念，对保真和引擎零影响。

## 8.5 外部主题（后续，机制已预留）

主题是纯声明式资源：加载 = 读用户/扩展目录的 TOML → 同一个 resolve 管道 → 注册进主题列表，**不进 wasm 沙箱**（Zed `theme_extension` 就是"扩展提供主题 JSON → 宿主直读注入 ThemeRegistry"的同款分层）。"用户自定义主题"功能届时只是把 rust-embed 换成/加上目录扫描，其他全部复用。

---

# 第 9 章 插件系统（功能组 6）

> V1 插件的产品行为、设置项和启停验收以 [plugin-system.md](./plugin-system.md) 为准；本章说明 crate 接口和加载边界。

## 9.1 定位与边界

- **V1 = 编译期内置插件 + 稳定的注册接口**；动态加载明确后置，但外置化路线已定稿（wasmtime + WIT 组件模型，照 Zed 扩展架构，见 §9.3）——先用五个内置插件验证接口。
- 插件接口 v1 提供四类能力：
  1. 注册**块渲染器**（按围栏语言/块类型接管渲染，含"上编辑下渲染"的打开态）；
  2. 注册**命令**（进快捷键/菜单/右键，产出源码补丁）；
  3. 注册**导出器**（格式、设置 schema 与执行入口）；
  4. 贡献**i18n catalog**（自带翻译）和插件设置定义。

```rust
// aline-plugin/src/traits.rs
pub trait Plugin: 'static {
    fn id(&self) -> &'static str;                                  // "math"
    fn block_renderers(&self) -> Vec<Box<dyn BlockRenderer>> { vec![] }
    fn commands(&self) -> Vec<CommandSpec> { vec![] }              // id/快捷键/菜单文案 key
    fn exporters(&self) -> Vec<Box<dyn Exporter>> { vec![] }
    fn settings(&self) -> PluginSettingsSpec;
    fn catalog(&self) -> Option<Catalog> { None }
}

pub trait BlockRenderer: 'static {
    fn id(&self) -> &'static str;
    fn accepts(&self, fence_lang: Option<&str>, kind: BlockKind) -> bool;
    /// 渲染阅读态；open 为 Some 时渲染打开态（如 Split 的上下布局）
    fn render(&self, ctx: &RenderCtx, block: &Block, open: Option<&OpenState>) -> AnyElement;
    /// 接管点击/编辑交互（表格逐格、任务勾选、公式错误面板点击编辑）
    fn interact(&self, ctx: &RenderCtx, block: &Block, event: BlockEvent) -> Option<BlockAction>;
}

// 插件接口分两层（2026-10-26 裁决，与 engine-design §1.4.2 / architecture-guide §14.2 统一）：
// ① 内置原生层——编译期进程内插件，用上面的元素级 BlockRenderer（experimental，
//    返回 AnyElement，永不跨 wasm 边界）；
// ② 外部资产层——wasm/进程外插件，只允许下面的 AssetRenderer：纯数据进出，无元素、
//    无 gpui 类型（V2 落地，V1 只锁形状——形状即"返回 desc 不返回元素"的架构规范）。
pub trait AssetRenderer: 'static {
    fn id(&self) -> &'static str;
    fn accepts(&self, fence_lang: Option<&str>) -> bool;
    fn render_desc(&self, source: &str, theme: &ThemeKey) -> Result<RenderDesc, RenderError>;
}
pub enum RenderDesc { Svg { bytes: Vec<u8>, size: (f32, f32) }, /* styled-runs / highlight-ranges 后续按需 */ }
```

- `RenderCtx` 提供：主题 token、缩放、文档相对目录（图片/资产解析）、`cx`/`window` 句柄、提交 `Change` 的回调（插件一切编辑必须走补丁通道，禁止自己摸文本）。
- **注册表**：启动时静态收集（`registry.rs` 里显式列出内置插件，V1 不用 inventory 宏），按 `(BlockKind, fence_lang)` 优先匹配，未命中走内建默认渲染。

## 9.2 五个内置插件的职责切分

| 插件 | 提供的能力 | 备注 |
|---|---|---|---|
| `math` | Math 块渲染、错误面板编辑命令、背景色设置 | ratex；块级公式上编辑下渲染；缓存键含主题参数 |
| `mermaid` | Mermaid 围栏渲染、预览/复制/SVG-PNG 导出命令、Mermaid 主题设置 | mermaid-rs-renderer；主题与 Aline 全局主题隔离 |
| `table` | 表格渲染、逐格编辑、快捷增行列、右键增删/复制/对齐命令 | 所有修改走补丁通道；关闭插件只隐藏悬停快捷按钮 |
| `export` | PDF、Word、HTML 导出器及格式专属设置 | 插件关闭时隐藏工具栏与菜单栏的导出入口 |
| `highlight` | 代码块渲染（syntect 高亮）及源码模式 fenced 高亮 | 关闭或失败时回退普通代码块 |

- 预留插件位（V1 只留接口空间，不写代码）：`plantuml`（BlockRenderer + HTTP 资产，接 Kroki）、`picgo` 图床（Command：选文件→上传→`EditImage` 补丁替换 URL）。
- 插件命令的快捷键冲突策略：内建 > 插件按注册顺序，冲突时后者不生效并 log。

## 9.3 加载模型与外置化路线（参考 Zed 源码）

> 依据：Zed 扩展系统源码分析（2026-10-26，稀疏克隆于 `/Users/xian/Project/book/zed`，关键文件索引见本节末）。
> V1 不实现本节；这是 V2 外置插件的定稿路线，写下来是为了让 V1 的接口形状不做返工。

### 9.3.1 最重要的分层借鉴：声明式资源不进沙箱

Zed 把扩展能力拆成两层：**主题 JSON、语言 config.toml、snippet 这类声明式资源由宿主直读，根本不进 wasm**；wasm 只跑动态逻辑（slash command、语言服务器代理、下载安装）。代价高的边界调用只发生在真正需要逻辑的地方。

Aline 的映射：

| 资源 | 层 |
|---|---|
| 主题 TOML、社区语言包（i18n catalog TOML） | 声明式，宿主直读，永远不进沙箱 |
| PlantUML、图表引擎、图床、其他公式后端 | wasm 插件（资产型：传块源码，回 SVG/URL 补丁） |
| 表格/数学/Mermaid/高亮 | 内置插件（元素级 API，进程内） |

### 9.3.2 宿主机制（照抄 Zed，wasmtime 48 + component-model）

- **Engine 全局单例**：`wasm_component_model(true)` + `epoch_interruption(true)` + 后台 100ms 心跳 `increment_epoch`；store 设 `epoch_deadline_async_yield_and_update`——超时**协作式让出**而非硬杀。Zed 没有 fuel、没有内存限制，Aline 同样不引入。
- **编译缓存走内存**：wasmtime `CacheStore` 包装 moka LFU 缓存（~32MB），不落盘；每扩展编译产物约 512KB 条目。
- **每扩展一个 actor**：mpsc channel 把对该实例的调用**串行化**（天然免去重入），跑在 tokio 上；要碰主线程的结果经 channel 回 GPUI。
- **文件系统沙箱**：WASI 只预开放 `work/<id>/` 读写；宿主侧对路径 canonicalize 后做前缀校验防 `..`/symlink 逃逸；guest 侧禁 `chdir`。
- **能力门禁（双层）**：manifest `capabilities` 声明 × 用户设置授权（默认拒绝），用于 `process:exec`、任意 URL 下载。Aline V2 第一版可只信本地 dev 扩展，门禁后加。
- **依赖隔离**：wasmtime 编译很重，宿主放独立 crate `aline-extension-host` + cargo feature 门控，不拖累默认构建。

### 9.3.3 接口、版本与 guest 形态

- WIT world：guest 只强制导出 `init-plugin` 一个函数，其余（`render-block` / `run-command` / …）可选——Zed 同款约定。渲染返回**数据描述**（WIT record 优先，配置类参数才用 JSON 字符串，Zed 两种都用）：

```wit
package aline:plugin@0.1.0;          // 草图，外置化启动时定稿

world plugin {
  export init-plugin: func();        // 唯一强制导出
  export render-block: func(source: string, lang: option<string>,
                            theme: theme-info) -> result<render-desc, string>;
  export run-command: func(id: string, arg: string) -> result<string, string>;
  import log: func(level: log-level, msg: string);
}
// render-desc = svg(bytes) | styled-runs(...) | highlight-ranges(...) —— 数据，不是元素
```

- guest 侧发 `aline-extension-api` crate：`trait Plugin`（方法全带默认实现）+ `register_plugin!` 宏（导出 `init-plugin` 符号），扩展作者 `cargo build --target wasm32-wasip2` 即得组件——形态完全对照 Zed 的 `zed_extension_api`。
- **版本内嵌**：build.rs 把 API 版本写成 wasm 自定义段 `aline:api-version`（Zed 用 6 字节 3×u16），宿主加载时先读段再选 linker。
- **版本策略裁剪**：Zed 为 10 个 WIT 版本各生成一个 bindgen 模块 + 枚举分发 + 旧版降级 stub（每版本约千行样板），以支持市场里所有旧扩展共存。Aline **只支持当前一个版本**，API 变更即断（扩展就几个、重编成本为零）；将来有外部用户再引入多版本分发。

### 9.3.4 清单、目录与 dev 工作流（照抄结构）

- `extension.toml`（id / version / lib / 声明式资源引用 / capabilities），带自动推导：扫目录补默认值（Zed 的 `populate_defaults`）。
- 磁盘布局：`extensions/installed/<id>/{extension.toml, extension.wasm, …}` + `extensions/work/<id>/`（沙箱可写区）+ `index.json`（反查表，mtime 决定重建）。
- **dev 扩展工作流**：`cargo build --target wasm32-wasip2` → 在 `installed/<id>` 建 symlink 指向源目录；`is_dev` 的判定就是这个 symlink。重建 = 重编 + touch。
- **不做**：扩展市场、tar.gz 分发、签名验证——Zed 的信任链在服务端 registry，宿主侧只查 Content-Length；个人项目本地目录即信任。

### 9.3.5 工作量预估

有 Zed 作地图，V2 外置化 ≈ 2–3 周：`aline-extension-api`（trait + 宏 + WIT）约 3 天、`aline-extension-host`（engine/actor/沙箱/dev 工作流，单版本）约 1.5 周、首个 wasm 插件（PlantUML）走通端到端约 2–3 天。

### Zed 关键文件索引（扩展宿主）

```
crates/extension_host/src/wasm_host.rs            Engine/Store/实例创建、epoch、缓存、actor
crates/extension_host/src/extension_host.rs       ExtensionStore：安装/目录/index/dev 扩展
crates/extension_host/src/wasm_host/wit.rs        版本枚举 + 宿主→guest 调用的版本分发
crates/extension_host/src/wasm_host/wit/since_v0_8_0.rs  bindgen、linker、host import 实现
crates/extension_host/src/capability_granter.rs   manifest×settings 双层能力门禁
crates/extension/src/extension_manifest.rs        extension.toml schema 与自动推导
crates/extension/src/extension.rs                 宿主侧 Extension trait
crates/extension/src/extension_host_proxy.rs      宿主→应用注入接口（8 个 proxy trait）
crates/extension/src/extension_builder.rs         cargo/wasi-sdk 编译、grammar 编译
crates/extension_api/src/extension_api.rs         guest trait + register_extension! 宏
crates/extension_api/wit/since_v0.8.0/extension.wit  最新 world 定义
```

---

# 第 10 章 多语言（功能组 8）

## 10.1 机制（aline-app-core/src/i18n，零 GPUI）

```rust
// locales/{en.toml, zh-CN.toml}，rust-embed 编译进二进制
[ui]
unsupported_format = "不支持的格式"
[engine]
refusal_changed = "文档已变化，命令未执行"
refusal_code = "无法跨代码块格式化"

pub fn t(key: &str) -> &str;               // 当前 locale 查表，缺失回退 en，再缺失回退 key
pub fn t_with(key: &str, args: &[(&str, &str)]) -> String;   // {name} 插值
```

- locale 决策：设置项 > 系统 locale > en。切换即时生效（gpui Global + 全窗口刷新）。
- **从第一天强制 `t("key")`**：M0 起所有面向用户字符串禁止硬编码（含 toast、对话框、占位符）；key 随功能走，M2 前机制定型。
- 命名空间：`ui.*`（壳层/对话框/toast）、`engine.*`（拒绝制/错误面板文案）、`plugin.<id>.*`（内置插件自带，启动时合并进 catalog）。
- 引擎层零依赖 i18n：所有错误是枚举，翻译只在 UI 层发生（这是"i18n 不做插件"的架构根据）。
- 社区语言包（后续）= **声明式插件**：catalog TOML 由宿主直读、不进 wasm 沙箱——Zed 扩展"声明式资源不进沙箱"的同款分层（§9.3.1）。

---

# 第 11 章 性能实现（功能组 9）

| 机制 | 实现 | 目标 |
|---|---|---|
| Visual 打字 < 1ms | 只动 open 块文本，全文不解析不重排 | 单键无感 |
| commit < 10ms（≤32KiB 典型）/ < 50ms（≤128KiB 上限档） | 预算分档（2026-10-26 修订：pulldown 单解析 128KiB 实测 ~20ms，10ms 单值预算自相矛盾，与 engine-design §2.10 统一）；≤128KiB 同步解析 + alike splice；实际文档远小，M1 用真实文档复核 | 换块无感 |
| 大文档 | 300ms 防抖后台解析 + `edits` 计数丢弃过期结果 | 1MiB 流畅 |
| 滚动 | 变高虚拟化 + 40rem overdraw；不可见块零渲染 | 长文档流畅 |
| 资产 | 全异步 + 内容哈希缓存（数学/图表随主题换键重渲） | UI 线程零阻塞 |
| 撤销内存 | 补丁栈 + 字节预算 trim | O(编辑量) |
| 多标签 | 非活动且已保存的标签可丢 `parsed` | 内存可控 |
| 大文件 | >4MiB 纯源码模式（阈值可配置） | 不卡死 |

基线基准（进 CI 的简单 bench）：解析 ≤128KiB < 50ms；commit ≤32KiB < 10ms、≤128KiB < 50ms；alike 增量 < 5ms；Source 模式 4MiB 档连续打字 < 4ms/键（StringBuffer O(n) 行表重建在 LARGEST=4MiB 是 Ely 2MiB 的两倍负担，超预算再上增量行表或调阈值）。已知限制：虚拟化粒度为块——单个巨型表格/超长段落仍整块布局测量（后续可做表格块内行虚拟化，engine-design §2.14）。

---

# 第 12 章 测试与验收门禁（映射功能组）

| 功能组 | 测试 | 门禁 |
|---|---|---|
| 1 文件 | 逐字节保真套件（BOM/CRLF/无尾换行/未编辑原样）；混合行尾编辑后按多数派重放断言（engine-design §2.9 明示语义）；模式切换零字节；保存-加载-保存幂等；open 态未提交文本被自动保存/恢复槽包含（flush 语义）；undo 回保存点后 dirty=false（栈深判定）；并发保存竞争 | 全绿才发布 |
| 3 查找替换 | 匹配/整词/大小写金样；replace_all 一步撤销；跨块命中 | 同上 |
| 4 可视化编辑 | 命令金样表（每命令 × 输入 × 选区 → 补丁 + 光标）；reread 拒绝用例；IME mark/replace/end 序列模拟；undo 逐字节还原（proptest 随机操作序列） | 同上 |
| 5 元素/渲染 | CommonMark 0.31 + GFM 样例规格测试；映射往返 proptest（`source_at(drawn_at(x))==x`）；块区间拼接=全文不变量；**HTML 专项**：标签→Style 金样、HTML5 嵌套修复（交叉嵌套/`<a>` 互斥/隐式闭合）、实体解码往返、过滤标签按文本显示、HTML 块注释丢弃 + 点击→提交单步撤销、`EditRefusal::Html` 拒绝用例 | 同上 |
| 6 插件 | 插件启停及设置持久化；渲染快照（给定块 + 主题 → 元素树/资产键）；导出入口门控与格式输出；表格行列操作、复制及补丁撤销金样 | 同上 |
| 7 主题 | token 完整性（三模式全 token 有值）；对比度检查 | 同上 |
| 8 i18n | key 覆盖检查（en/zh key 集合相等，CI 脚本） | 同上 |
| 集成 | gpui TestAppContext 模拟键盘/鼠标/IME 跑通：打开→编辑→保存→重开 | 冒烟 |

---

# 第 13 章 里程碑（V1 重排，取代 engine-design §2.13）

| 阶段 | 内容（对应功能组） | 验收标准 | 估时 |
|---|---|---|---|
| **M0 引擎地基** | source/buffer/markdown 四件套；改写器、mdast、展示树、sources、块切分、markdown_hl；i18n 机制定型（§10） | 规格测试 + 映射往返 proptest 全绿；CLI dump 树 | 1–2 周 |
| **M1 单文档只读** | aline-ui：DocCore 骨架、块渲染、虚拟化、点击映射、文件层（打开/保存/保真）、单窗口单标签 | 1MiB 滚动流畅；保真套件绿；点击落点逐字符正确 | 2 周 |
| **M2 可编辑核心** | 阶段 A 开合编辑、commit、IME、补丁式 undo、双模式 + 模式切换、查找替换（功能组 2/3/4 主体） | IME 全程正确；命令金样表绿；打开块外观零位移（1px 断言）；"看+改+搜"日常可用 | 3 周 |
| **M3 命令与表格** | apart/reread/拒绝制、格式 toggle、列表、Block 命令、表格逐格 + 12 操作、任务勾选、标记渐隐（B 档）、**HTML 行内样式 + HTML 块编辑 + `EditRefusal::Html`** | 每日写作无阻；拒绝/校验测试绿；HTML 实体/嵌套修复金样绿 | 2–3 周 |
| **M4 平台层** | 插件注册表 + 五个内置插件、插件管理设置页、PDF/Word/HTML 导出、主题系统（1 套 ×3 模式）、i18n 全量文案、链接/图片弹窗 | 插件开关与设置即时生效；公式按主题参数重渲染，Mermaid 按自身主题配置重渲染；导出格式可用 | 2–3 周 |
| **M5 应用壳完整** | 标签页、文件夹浏览器（开关/不支持提示）、多窗口、系统文件关联、自动保存/外部修改/崩溃恢复收尾（功能组 1 全部） | Finder 双击 → 新窗口 + 定位文件夹；多标签切换状态无损 | 2–3 周 |
| **M6 收尾与调优** | 性能基准进 CI、大文件降级、打包（macOS 优先 + Info.plist 关联）、（挑战项）折叠编辑器 B+ | 全门禁绿；发布候选 | 2 周 + stretch |

总盘 ≈ 3–4 个月个人节奏。M2 完成即有可日用产品；每个 M 独立可发布。

---

# 第 14 章 风险与对策（V1 口径更新）

| 风险 | 等级 | 对策 |
|---|---|---|
| 折叠编辑器 B+（IME/映射长尾） | 中 | 四档阶梯，A/B≈免费且是安全垫；B+ 是 stretch 不阻塞发布；映射先在纯逻辑层钉死 |
| gpui open-file 事件 API 因版本而异 | 低 | M5 首个任务：锁定版本源码确认回调名；封装 `workspace/open_events.rs` 隔离 |
| 插件接口 v1 形状不稳 | 中 | 五个内置插件即“接口测试用例”；接口标注 experimental，动态加载后置 |
| ratex/mermaid-rs 覆盖不足 | 中 | 失败原文面板可编辑可复制；预留 WebView 后备渲染位 |
| Windows 中文字体回退 | 中 | 移植 Ely DirectWrite 补丁（方向已验证）；macOS 优先发布缓解 |
| 多标签内存 | 低 | 非活动标签丢 parsed；上限告警 |
| gpui 版本漂移 | 中 | 锁死快照 + vendor 补丁目录；升级为显式任务 |
| 双实例进程互覆（Win/Linux 双击拉起第二进程） | 中 | 单实例锁文件 + 命名管道/Unix socket 转发（§2.2）；macOS 系统天然单实例；M5 三平台实测 |
| Source 模式大文档打字（O(n) 行表重建 × 4MiB） | 低 | bench 盯防（§11 基线）；超预算再上增量行表或调 LARGEST |

---

# 第 15 章 Ely 实证的 GPUI 实现要点

本章收录 ElyOffice 源码验证过、但前 14 章未展开的壳层与 GPUI 落地要点；逐条纪律的【强制】依据见 architecture-guide §8–§10、§13。

## 15.1 窗口骨架与启动

- 一个窗口一个 `Root` 实体：焦点根（`FocusScope.root()`）+ 菜单 + 命令面板 + Toast 视口 + 内容视图，全部从 Root 挂出；焦点链、浮层、toast 都不散落在各视图里。
- 初始化收口一个 `init(cx)`（含 `init_for_tests`）；主题/语言/设置 `Global` 在此注册，首帧主题用"立即应用"路径（Ely `set_mode_now`），避免首帧闪默认色。
- 窗口关闭确认走统一 guard（Ely `shell::guard_close`），不直接 `on_window_should_close`——后者会被自绘标题栏绕过。关闭询问用 SaveIntent 三选（保存/放弃/取消，未保存的排最后）。

## 15.2 焦点、undo 与 dirty

- 打开块子编辑器的 Undo/Redo 用 `capture_action` 接管，转发到文档级历史；写回恒为一个撤销步（§6.2）。
- dirty 是合成判定：`history.undo 栈深 != saved_at || open 有未提交修改`（2026-10-26 修订：不用 stamp——undo/redo 是精确逆补丁，栈深回到保存值 ⟺ 内容与已保存字节一致，stamp 会把"打字后撤销回原文"误判为 dirty；保存隐式封口打字合并组，详见 §6.3）；状态栏、标签页、关闭 guard 消费同一个函数，不各自维护 bool。

## 15.3 Command 表落地

- Command 定义集中在 `workspace/commands.rs` 一张表（`id / keys / blocked / action`），快捷键绑定、菜单、命令面板、禁用态全部由表生成；未知 id 记 warn（architecture-guide §5.2）。
- 菜单文本走 i18n key；`blocked` 由命令自己声明（如无打开块时表格命令禁用），不由菜单层判断。

## 15.4 测试夹具与冒烟

- gpui 集成测试夹具独立成模块（`open` / `press` / `frames` 三件套，Ely 形态），不用私有函数直调，统一走 `debug_selector` 命中。
- `debug_selector` 约定值（随实现一起写）：块 `markdown-block-{ix}`、打开块 `markdown-open`、表格单元格 `markdown-open-cell`。
- M0 起提供 `ALINE_SMOKE` 冒烟钩子（建窗口 → 打开样例文档 → 渲染 N 帧 → 退出），供 CI 与三平台验收复用。

## 15.5 失焦与布局

- 失焦窗口设 `inactive_frame_interval`（Ely 250ms）降频；动画只在运行时请求帧，空闲零重绘。
- 会话恢复的窗口布局保存/运行同形（一个 `Serialize`/`Deserialize` 类型，Ely `PaneLayout` 形态），版本化 JSON、全有或全无恢复（§2.3、architecture-guide §11.5）。

---

# 附录 A 解析内核要点索引

解析内核（改写器、mdast、展示树、sources 映射、alike 增量、解析调度阈值、保真七不变量）的完整设计与 elymark 代码对照，见 engine-design.md §2.2–2.3、§1.4–1.5 与附录文件索引。本文档不重复。

# 附录 B 关键类型速查

```
Source                    文件字节层（BOM/CRLF 重放）
Buffer trait / StringBuffer/CodeBuffer  Buffer 的 v1 实现（纯文本）／编辑器级聚合（Buffer+选区+History+IME，DocCore 持有）
Parsed / Block            解析结果（块源码区间不含尾部换行）
Node / Inline / sources   展示树 + 双向映射（Escapes 四类：Plain/Markdown/Html/Pipes）
Style                     行内样式 token（Strong/Emphasis/Strike/Underline/Code/Mark/Link）
DocCore                   编排状态机（模式/打开块/解析调度/查找）
OpenBlock / OpenKind      打开块（Plain/Panel/Split/Cell）
Command / Change          命令 → (Vec<(Range,String)>, 选区)
EditRefusal               拒绝制（Changed/Code/Math/Html/FrontMatter/RereadMismatch/…）
FindState                 查找替换（纯文本，无正则）
Theme / Tokens / Mode     主题（aline-default × Day/Night/Eye）
Plugin / BlockRenderer    插件接口 v1（渲染器/命令/catalog）
Workspace / Tab / FolderPane  窗口=工作区（标签页 + 文件夹浏览器）
```
