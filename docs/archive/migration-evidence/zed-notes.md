# zed-notes 历史证据摘录

2026-10-30 按 PROTO-06 方案 A 保全。**非现行规范，不执行其中的代码/配置/命令；“照搬/强制/已验证/待确认”均为原稿当时语境。** 当前约束以 [文档入口](../../README.md) 和 [确认归档](../2026-10-30-migration-decisions.md)为准。

原路径：`docs_bak/docs/zed-notes.md`；原文件 26117 bytes，SHA-256 `ed4ba2ffe05b5232a55aceedd57218b1ca173f43e58fa91052c4a37491222713`。保留的是下列逐段原文（含源码、表、图例、示例、路径）；未迁片段的归属见 [证据索引](./README.md)。源码行号只表示该历史文件，未重验外部源码或第三方性能。

## L1–L240 · 完整原始Zed研究/逐领域清单/源码索引；与当前gpui-pre代差须保留

原片段 SHA-256：`ed4ba2ffe05b5232a55aceedd57218b1ca173f43e58fa91052c4a37491222713`。

~~~~~~~~markdown
# Zed 架构与 GPUI 实践笔记

> 源码：`<external>/zed`（完整克隆，2026-10-26）。
> 定位：补充 [engine-design.md](./engine-design.md)（elymark 逆向）与 [v1-architecture.md](./v1-architecture.md)（实施蓝图）——Zed 是 GPUI 生态最完整的参考实现，本文按领域整理其架构与实践，每条标 **照搬 / 改造 / 不采用**，供各里程碑查阅。
> 本文档全部 file:line 与语义断言已于 2026-10-26 用四个独立代理对照源码对抗式复核（44 组断言），修正了 height=None 语义、action 命名规则、spawn 签名及若干路径/行号后定稿。

## 0. 版本警告（先读）

Zed 主线的 gpui 已是**新架构**：`Window`/`App` 双参数拆分（无 `WindowContext`），平台实现拆到 `gpui_macos`/`gpui_apple` 等 crate。Aline 锁定的 gpui-pre 是 Ely 的快照（旧 API 代差）。本文的**机制级结论**（缓存结构、生命周期、IME 契约、效果系统）跨代际成立，但**具体签名以 gpui-pre 源码为准**（engine-design.md 已验证过的 API 以它为准）。

---

# 第 1 章 gpui 本体（crates/gpui）

## 1.1 Entity / 效果系统

- 无 `Arc<Mutex>`：全部单线程 + 句柄。`Entity<T>`（app/entity_map.rs:435）读 `read(&App)`（:485）、写 `update(cx, |&mut T, &mut Context<T>|)`（:497）；所有回调和跨持有用 `WeakEntity`，upgrade 失败静默放弃——杜绝循环引用。
- **效果系统是核心调度**：跨实体副作用编译成 `Effect` 枚举（Notify/Emit/RefreshWindows/NotifyGlobalObservers/Defer/EntityCreated，app.rs:3487-3505），`flush_effects`（app.rs:2121）循环排空后才对 dirty 窗口 `schedule_frame`（:2179）。
- **隐式依赖追踪**：`App::notify` 用 `tracked_entities` 过滤——绘制期间被 `read` 过的实体才登记（window.rs:3571），即"渲染时读了谁，谁变了才重绘"，窗口失效精确到依赖。

## 1.2 Element 生命周期

- `Element` 三阶段各带独立 state 类型（request_layout → prepaint → paint，element.rs:52）；`Render`（Entity 每帧重建子树）→ `RenderOnce`（纯组件）→ `Element`（底层）。
- **跨帧状态挂 id 不挂结构体**：元素结构体每帧重建，但 `window.with_optional_element_state` 按 GlobalElementId 跨帧查表（div.rs:2285）——hover/点击/滚动状态 survives。
- 交互集中在 `Interactivity` 一个结构（div.rs:2183）：hover/active/focus/scroll/监听器全在这，Div 和 UniformList 都只是嵌入它。
- `deferred(child)`：layout 占位在树内，绘制推迟到祖先画完后按 priority 叠放（deferred.rs:7-65）——tooltip/浮层的机制。

## 1.3 虚拟化列表

- **变高块用 `list` 不用 `uniform_list`**（Aline 块高不等）：`ListState` 持 `overdraw`（可配置）、`ScrollAnchor`、`PendingScroll`（结构在 list.rs:54-92）；高度**渐进缓存**——未测量用 `size_hint` 估算（`ListItem::Unmeasured`，:233-235），测后转 `Measured`（:246）并按锚点修正防跳（:415）。
- `DeferredScrollToItem`：scroll_to_item **排队到下一帧 prepaint** 再执行（uniform_list.rs:150,378）——避免在事件回调里做布局，编辑器 scroll_to_caret 正需要。

## 1.4 文本系统

- **双帧 LineLayout 缓存**：`previous_frame`/`current_frame` 两个 FrameCache，`finish_frame` 帧间 GC 只留两帧内用过的（line_layout.rs:458,586）。
- **hash 键探缓存**：`shape_line_by_hash`（text_system.rs:722）——调用方给内容 hash，命中零分配返回，miss 才物化子串去 shape。对单 String buffer 的 Aline 极其友好：每块按（内容 hash+字体+宽度）探缓存，编辑时光标行重 shape，其余行全部命中。
- **wrap 不重新 shape**：`WrappedLineLayout` = 整行 Arc + wrap_boundaries 表，宽度变了只重算 boundary（line_layout.rs:277）。
- 索引↔位置就是 glyph 线性扫描：`x_for_index`/`index_for_x`/`closest_index_for_x`（line_layout.rs:61-117，第三个供上下键跨行对齐）。

## 1.5 InputHandler 与 IME

- `EntityInputHandler`（input.rs:26）**对外全部 UTF-16**，内部随意；组合核心是 `replace_and_mark_text_in_range` + `bounds_for_range`（候选窗定位）。
- `ElementInputHandler` 在 paint 时经 `Window::handle_input` 登记（window.rs:5409）。
- **`last_text_input_configuration` 只在配置真变时转发**（window.rs:1255，注释明言"重复重配会重启 IME 会话"）——中文输入法闪烁的经典坑。
- macOS 侧：NSView 直接挂 `NSTextInputClient`（gpui_macos/src/window.rs:210-271）。

## 1.6 actions 与 keybinding

- Action-as-type：`actions!` 宏（action.rs:24）经 inventory 注册进全局 registry（:282），名字是 PascalCase 结构体名加 `::` 命名空间前缀（`editor::MoveUp` 式，derive_action.rs:105-120），keymap JSON 按名字反序列化。
- 优先级：**context 栈深的赢**（Editor > Pane > Workspace），同级后添加的赢；`"x": null` 禁用绑定（keymap.rs:154-159）。
- 多键序列（`"cmd-k left"`）走 pending 状态：前缀匹配后收不到下文时把前缀**回放**为普通按键，超时冲刷（key_dispatch.rs:483,523）。

## 1.7 Asset / 异步 / 窗口

- `Asset` trait（asset_cache.rs:99）：后台执行 + 并发去重 + 错误同样缓存。
- **异步桥接三件套**（app/context.rs:230）：`spawn(AsyncFnOnce(WeakEntity<T>, &mut AsyncApp))` 里 `background_spawn(重活).await` 拿结果再 `weak.update` 回主线程——文件监听、大文档解析全走这条，无需把线程安全设计进 buffer。
- `open_window` **强制画一帧再返回**（app.rs:1581，Windows 平台有未渲染窗口竞态）。
- **`on_open_urls(callback: FnMut(Vec<String>))`（platform.rs:447）= Finder 双击/拖 Dock 的真实 API 名**（engine-design 里存疑的那个）；`on_reopen`（Dock 图标点击，platform.rs:464）；`set_menus(Vec<Menu>, &Keymap)`——**菜单加速键直接从 keymap 解析，菜单与快捷键自动一致**（platform.rs:494）。

## 1.8 gpui 层借鉴清单

| # | 结论 | 档 |
|---|---|---|
| 1 | 双帧 LineLayout 缓存 + hash 探缓存：块渲染按内容 hash 探，miss 才 shape | **照搬**（对 Aline 性能影响最大） |
| 2 | 效果驱动精确失效（tracked_entities 隐式依赖） | 用 GPUI 则免费；自建引擎抄思想 |
| 3 | EntityInputHandler 接口切分（对外 UTF-16）；edit crate 做纯逻辑同构 trait，ui 层薄适配；configuration 变更才转发 | **照搬** |
| 4 | WeakEntity 回调语义（upgrade 失败静默退出）——插件回调天然不拖死宿主 | **照搬** |
| 5 | 异步三件套（weak spawn + background_spawn + weak.update） | **照搬** |
| 6 | `on_open_urls` + `set_menus(keymap)` + open_window 强制首帧 | **照搬**（M5 用） |
| 7 | 虚拟化选 `list`：size_hint 渐进测量 + ScrollAnchor 防跳 + DeferredScrollToItem 排队语义 | **照搬**（改造 uniform_list 预期） |
| 8 | Interactivity 集中体 + 跨帧 element_state 表（状态挂 id） | **改造**（块元素自绘时抄结构） |
| 9 | Action 机制保留三样：具名注册（PascalCase + `::` 命名空间）、context 栈深优先、pending 序列 + 超时 replay（Ctrl+K 类 chord） | **改造**（简化为单 dispatch 树） |
| 10 | 索引映射的 glyph 扫描：块内点击→`closest_index_for_x`、光标→`x_for_index` | **照搬** |
| 11 | 每帧重建整个元素树的 Render 模型、a11y/inspector/touch 手势 | **不采用**（保留 element_state 与 deferred 两件） |

---

# 第 2 章 Workspace 壳层与 UI 组件（crates/workspace、crates/ui）

## 2.1 Item trait——"能开成 tab 的东西"

- `Item: Focusable + EventEmitter<Event> + Render`（item.rs:170）；`tab_content_text` 必须实现，`tab_icon/tab_tooltip` 可选。
- **事件桥接**：`to_item_events(&Self::Event, &mut FnMut(ItemEvent))`（item.rs:214）——组件自有事件翻译成 ItemEvent（Activate/Close/Save），Pane 统一订阅。DocumentView 自己发编辑事件，Pane 只认 ItemEvent。
- 脏标记：`is_dirty`/`has_conflict`（item.rs:279,291）；`save(SaveOptions) -> Task<Result<()>>`。
- 序列化独立成 `SerializableItem` trait：`serialized_item_kind()` 字符串做类型键，`should_serialize(event)` 按事件判断是否值得写盘（item.rs:409-436）——防抖思想。
- 关闭确认**不在 Item 上**，由 Pane 用 SaveIntent 驱动。

## 2.2 Pane 与标签页

- `Pane { items: Vec<Box<dyn ItemHandle>>, active_item_index, activation_history(MRU), last_focus_handle_by_item(切 tab 记住焦点), save_modals_spawned(防重复弹窗) }`（pane.rs:399-448）。
- `close_items(save_intent, should_close)`（pane.rs:1964）三段式：收集 → 按"active 放最后"排序保焦点（:1983 注释明言）→ 批量 `window.prompt("Save all/Discard all/Cancel")`（:2013）→ 逐项保存，失败再 prompt（"直接关闭/取消"，:2064）。**`SaveIntent` 枚举 8 变体（Save/FormatAndSave/SaveWithoutFormat/SaveAll/SaveAs/Close/Overwrite/Skip，pane.rs:84）统一保存/关闭/放弃语义——直接照搬**。
- 确认框用 GPUI 原生 `window.prompt(level, msg, detail, &answers) -> oneshot::Receiver<usize>`（gpui/window.rs:6711）——不必自写对话框。
- Tab 拖拽重排：`on_drag(DraggedTab)` + `drag_over::<DraggedTab>`（pane.rs:2963-3006），可后置。

## 2.3 Panel/Dock 与文件树

- `Panel` trait 25+ 方法（dock.rs:36）+ Dock 每侧一个实体 + 异步 pending 恢复——**Aline 砍成**：`FolderPanel` struct + `is_open/size` 存 KVP，不做三侧 dock/zoom/PanelHandle 门面。
- project_panel（8.4k 行）可借鉴的骨架：`uniform_list` 虚拟化（project_panel.rs:7732）+ `expanded_dir_ids: HashMap<WorktreeId, Vec<EntryId>>` 有序展开集（:114，binary_search + insert 维持有序）；**单击=预览打开、双击=激活聚焦**（:6611-6613，`click_count` 判定；单击预览受 `enable_preview_from_project_panel` 设置控制，关闭时单击即普通打开）；**打开走事件解耦**（`open_entry` 只 emit `Event::OpenedEntry`，:2422-2430；订阅注册在 project_panel.rs:937 的 `subscribe_in`，handler 以 workspace 为接收者调 `open_path_preview`）。砍掉 collab/远程/多选/内联重命名。

## 2.4 弹窗 / 通知 / 会话恢复

- **ModalLayer**（modal_layer.rs:111）：单 active modal（:112）+ 可暂存 stashed modal（:116）+ `on_before_dismiss -> DismissDecision`（可否决关闭，hide_modal :232-239，DismissDecision 定义 :8）+ `previous_focus_handle` 焦点恢复（字段在 ActiveModal :107，恢复逻辑在 hide_modal :254-256）——链接/图片编辑弹窗照这个模式。
- **ToastLayer**：同时只有一个 toast + 10s 自动消失（`DEFAULT_TOAST_DURATION`，toast_layer.rs:15）+ `ToastAction`（label+on_click，:54-57）；Notification 按 `NotificationId`（Unique(TypeId)/Composite(TypeId, ElementId)/Named 三变体，notifications.rs:42-43）同 id 先 dismiss 再 push——新替旧不堆叠（:162-194）。
- 持久化是 SQLite（WorkspaceDb + workspaces/panes/items 表，persistence.rs:536）——**Aline 缩成**每窗口一条记录（文件夹 + md 路径列表 + active tab），轻量项（窗口 bounds、侧栏开合）走 KVP；写回走防抖任务（200ms 节流，workspace.rs:1623、:7769-7782）。

## 2.5 窗口与组件风格

- `open_paths` 入口先 `find_existing_workspace` 复用已开窗口（active_window 优先），`OpenMode::NewWindow` 才开新窗（workspace.rs:11266）——**系统双击 md 的"新窗口"语义靠 OpenMode 表达**；`AppState.build_window_options` 闭包工厂（:1332）。
- StyledExt（`crates/ui/src/traits/styled_ext.rs:26-89`）：`h_flex()/v_flex()`（另在 components/stack.rs:7,13 有同名自由函数）、三级 `elevation_1/2/3(cx)`（bg+border+圆角+shadow 一次设置，:6-12）；`.when(cond, |el| ...)` 条件链来自 gpui 的 FluentBuilder（gpui/src/util.rs:11）而非 StyledExt，全库统一风格。
- 主题注入：组件**不存 token**，render 时 `cx.theme()` 现取；ui 组件 60+ 个，Aline 需要的白名单：Button/IconButton、Label、Icon、ContextMenu、Popover、Tooltip、Modal、Divider、Scrollbar、Tab/TabBar、Notification。

## 2.6 壳层借鉴清单

| # | 结论 | 档 |
|---|---|---|
| 1 | Item trait 精简版 + `to_item_events` 事件桥（DocumentView→Pane 只认 ItemEvent） | **照搬** |
| 2 | `SaveIntent` 枚举 + close_items 三段式 + 原生 `window.prompt` 三选 | **照搬** |
| 3 | SQLite 持久化骨架 → 缩成"每窗口一条记录 + KVP 轻量项"，防抖写回 | **改造** |
| 4 | ToastLayer 单实例 + Notification TypeId 去重（拒绝提示不堆叠） | **照搬** |
| 5 | ModalLayer 的 DismissDecision 可否决关闭 + 焦点恢复 | **照搬** |
| 6 | StyledExt 风格（h/v_flex、elevation、.when 链）、组件白名单 11 个 | **照搬** |
| 7 | 文件树：uniform_list + 有序展开集 + 单击预览/双击激活 + 事件解耦打开 | **改造**（大幅裁剪） |
| 8 | Panel/Dock 砍成单个 FolderPanel；MultiWorkspace/NavHistory/collab/PaneGroup 分割/文件树内联编辑 | **不采用** |

---

# 第 3 章 编辑器内核与查找替换（crates/text、editor、search）

## 3.1 事务与 undo（对照 Aline 的逆补丁）

- Zed 事务 = edit_ids: Vec<Lamport> 值列表 + CRDT UndoMap 奇偶计数（text.rs:135-141, undo_map.rs:47-70，is_undone 即 undo_count % 2 == 1），支持子集撤销/重放/时间合并——**代价是 fragment 双 SumTree + Lamport 时钟，单机 String buffer 完全不划算，Aline 逆补丁保留**。
- 但**事务 API 形状值得抄**（A 档）：嵌套 begin/end（depth==1 才入栈，空事务丢弃，text.rs:238-306）、300ms `group_interval` 自动合并打字、`group_until` 供 IME 把整个组合折叠成单事务；**undo 栈每条事务存"当时的选区快照"**（SelectionHistory，editor.rs:1450-1454,8786-8830），恢复光标不靠猜。

## 3.2 高亮与 decorations

- **`HighlightKey` 枚举、枚举顺序即优先级**（display_map.rs:161-164，注释原文 "lower means higher priority"，derive Ord）：syntect spans/选区/搜索/输入组合各自一个 key；消费端 custom_highlights.rs:16 用 `BTreeMap<HighlightKey, HighlightStyle>` 按枚举序升序迭代依次叠加，靠后的覆盖靠前的；`TextHighlights = Arc<HashMap<HighlightKey, Arc<(HighlightStyle, Vec<Range<Anchor>>)>>`（display_map.rs:199）；`SemanticToken(u32)` 的载荷是动态 precedence——Aline 对应 `(key, style, Vec<块内range>)`。
- **Block decorations = "上编辑下渲染"的直接参照**（block_map.rs:159-310）：`RenderBlock = Arc<dyn Fn(&mut BlockContext) -> AnyElement>`（:159）；`BlockPlacement { Above/Below/Near/Replace(RangeInclusive) }`（:163-173，Replace 是闭区间）——**Aline 打开的块可建模为该块处于 Replace 状态**；`height: Option<u32>`（:275,286）**None = 完全不占行空间**（如横线，height() 取 0，:429）；自适应测量走 `Some(初始估算)`：渲染时 `layout_as_root` 实测量化到行高，偏差经 `resize_blocks` 回写（element.rs:3503-3552、block_map.rs:1909-1921）；priority + tie_break 排序。

## 3.3 IME 落地细节（editor/src/input.rs:2798-3120）

- **marked text 用高亮 key 表示**（`HighlightKey::InputComposition` 下划线样式），不引入独立 marked buffer。
- 组合期：每步 transact + `group_until_transaction` 折叠成单事务；**临时关闭自动配对/autoclose 防副作用**；selection 变更 `no_scroll`。
- macOS 替换 range 相对 marked 区间、空 range = 插入光标处的坑有成排注释（:2870-2934）——实现时对着抄。

## 3.4 查找替换（crates/search/buffer_search.rs）

- **无定时器防抖**：query 每次编辑立即 clear + `update_matches`（buffer_search.rs:1375-1400）。
- **每次全量搜、后台执行、按 CPU 切块并行**（items.rs:2028-2120；块数 = min(行数+1, num_cpus)，正则或含换行的查询退化为单块，:2400-2405）；**匹配结果存稳定引用**（Zed 用 Range<Anchor>，编辑后自动跟随；Aline 用 `(块id, 块内range)`，编辑块只失效该块）。
- 失效→重搜靠事件：编辑器 emit `MatchesInvalidated`（editor.rs:10141），bar 收到重跑——"全量重搜 + 事件失效"，无增量匹配维护。
- **replace_all 单事务**（items.rs:1937-1968）：先收集全部替换，一次批量 edit，一次 undo 全回。

## 3.5 增量失效与高亮缓存

- DisplayMap 5 层映射（Inlay/Fold/Tab/Wrap/Block），每层 sync 消费上游 edits patch、SumTree slice 复用未变区段（display_map.rs:561-575 的 `sync_through_wrap` 依次传同一份 edits；`crates/editor/src/display_map/fold_map.rs:456` 的 sync 用 `cursor.slice` 复用未变段）——**B 档**。
- tree-sitter 高亮**按 chunk 粒度缓存**（Buffer 在 `crates/language/src/buffer.rs`，字段 highlights_by_chunks :156、消费 :4233-4240），后台 parse + 同步等待 ≤1ms 窗口（`reparse(may_block)` :1943，超时转 background_spawn，:1967-1984）——Aline 现在就可做简化版："块级缓存、编辑只重算所在块"。
- **`Edit<D>` 泛型：一次编辑遍历同时产出多坐标系的新旧区间**（text.rs:527-530, edits_since :2727）——多维形状是 `sum_tree::Dimensions<D1, D2, ()>`（rope.rs:1447-1461 blanket impl，usize/Point/PointUtf16/OffsetUtf16/TextSummary 任两维组合），`Edit<(D1,D2)>::flatten()`（text.rs:558-568）拆回单维——Aline 的 sources 映射等价于 `Edit<(offset, display_offset)>` 的 flatten，**每次编辑一次遍历同时产出两个坐标系补丁**，取代两套各自维护（A 档思想）。

## 3.6 内核借鉴清单

**A 档（String buffer 时代现在就抄）**：① HighlightKey 枚举即优先级；② 搜索 = 全量重搜 + 事件失效 + 匹配存稳定引用（块级）；③ replace_all 单事务 + 批量 edit API；④ IME 四件套（marked-as-highlight、group_until 单事务、组合中关自动配对、bounds_for_range 公式）；⑤ 事务 API 形状（嵌套/空丢弃/300ms 合并/选区快照）；⑥ 打开的块 = BlockPlacement::Replace + `Some(估算高度)` + resize 回写（**height=None 是"不占位"，勿混淆**）；⑦ 编辑广播一次遍历产出双坐标系补丁。

**B 档（ropey/性能迁移再抄）**：① Rope TextSummary（utf16/point 维度 ropey 不自带要包一层）；② Anchor 简化版（单机 Lamport 退化为单调 edit 序号 + offset_in_edit + bias）；③ edits_since::<D>() 泛型广播 + 逐层 DisplayMap；④ chunk 粒度缓存 + 后台 parse + 1ms 同步窗口；⑤ CRDT undo 不抄。

---

# 第 4 章 主题、设置与文件系统（crates/theme、settings、fs）

## 4.1 主题

- **ThemeFamily 一文件多 variant**（theme.rs:221-233：`{ id, name, author, themes: Vec<Theme>, scales }`；assets/themes/one/one.json 单文件含 One Light/One Dark；编译期 fallback 的 "Zed Default" 家族则单 variant）——与 Aline"一 TOML 三段 day/night/eye"**同构，方向验证**；区别是 Zed 用 JSON + 字段全 Option。
- **refine 默认色模式（最值得抄的一笔）**：主题文件只写少量锚点色，缺省 token 回填程序化默认——链路 `load_user_theme → refine_theme_family → refine_theme`（crates/theme_settings/src/theme_settings.rs:244-372），基底是 `ThemeColors::light()/dark()` 等编译期默认色板（default_colors.rs，doc 原文 "refined off of these defaults"），另有少量跨 token 派生（前景缺省取 `fg.opacity(0.25)` 等，fallback_themes.rs:29,47）；所谓"Radix 12 步色阶"是硬编码 `ColorScaleSet`（scale.rs:240-246），色值与 Radix 色板一致但代码不标注 Radix（Aline 用更简单的 HSL 派生即可）——手写主题成本从 145 键（ThemeColors 字段精确计数）降到 ~15 键。
- **内容与选择分离**：`ThemeSelection::Dynamic { mode: System|Light|Dark, light, dark }` 在独立 crate（crates/theme_settings/src/settings.rs:147-163）+ `SystemAppearance` global（crates/theme/src/theme.rs:161-212）——token 段是内容，用哪段是运行期选择，将来加"跟随系统"不用动 schema。
- ThemeRegistry：`HashMap<SharedString, Arc<Theme>>` 按 name 覆盖去重（registry.rs:63,174-179）；内置主题两路——编译期 fallback（registry.rs:124）+ bundled JSON 经 AssetSource 加载后 refine 再插入（theme_settings.rs:218-241）；切换传播 = ThemeSettings global 观察 + `cx.refresh_windows()`，组件不做单独订阅（每帧 `cx.theme()` 现取）。
- **外部主题目录 + watch 热重载整套只有 ~85 行**（crates/zed/src/main.rs:1894-1978：`load_user_themes_in_background` + `watch_themes`）：`~/.config/zed/themes/*.json` 后台读入 + fs.watch 100ms 去抖——Aline 的外部主题（§8.5）照此实现，与"SVG 缓存键含主题"天然兼容。

## 4.2 设置

- 完整的六层合并（default/extension/global/user/server/project）+ derive 宏 + inventory 自动注册——**对个人项目是负资产，不抄**。
- 抄三件便宜货：① **类型化快照**——JSON 变更时解析一次成强类型 struct，渲染端读字段零开销；② **单一 global + observe + refresh_windows** 的传播模型（Zed 全仓 78 个观察点验证过）；③ `recompute_values` 式"变更时全量重算快照，观察方自己 diff"。
- 将来要分层时：两级合并（代码默认 → 用户 → 工作区）+ `MergeFrom`（标量覆盖、Some 覆 None）是最小切片。

## 4.3 文件系统

- `Fs` trait 的 **watch 签名形状是留后路的关键**：`watch(path, latency) -> (Stream<Vec<PathEvent>>, Watcher)`（fs.rs:104-208，签名 :175-182），**去抖由 watch 层负责，调用方只拿干净的批量事件**（fs_watcher.rs:37 起：pending 缓冲 + latency 定时后整批冲出）——Aline V1 不做监听，但 trait 位按这个形状预留。
- `PathEventKind::Rescan` 变体 = "监视器积压/失序"兜底通道，加成本为零（fs.rs:84-89）。
- FakeFs 用 `Duration::ZERO` 绕过测试时钟去抖（fs.rs:3444,3461，注释：确定性执行器不会自行推进时间，真去抖会卡死测试）——将来做测试 FS 的现成答案。
- paths.rs 全 `OnceLock` 惰性初始化（config_dir/data_dir 各平台约定，paths.rs:122-144）——Aline 的设置目录照抄结构。
- poll watcher/NFS 探测/inotify 配额冷却是 Zed 规模才需要的，跳过。

---

# 第 5 章 对 Aline 的全景影响（映射到 v1-architecture）

| v1-architecture 位置 | 补强内容 | 来源 |
|---|---|---|
| §5 查找替换 | 全量重搜 + MatchesInvalidated 事件 + 匹配按块存稳定引用；replace_all 单事务 | §3.4 |
| §6.3 命令/undo | 事务 API 形状（嵌套/空丢弃/300ms 合并/**选区快照**）；批量 edit 接口形状 | §3.1 |
| §6.4 IME | marked-as-highlight、group_until 单事务、组合中关自动配对、**configuration 变更才转发**、macOS 坑注释 | §3.3、§1.5 |
| §6.6/块打开 | "上编辑下渲染" = BlockPlacement::Replace + `Some(估算高度)` + resize 回写的建模参照（height=None 是不占位） | §3.2 |
| §7 渲染 | **list（变高）+ size_hint 渐进测量 + ScrollAnchor**；HighlightKey 分桶叠色；shape_line_by_hash 块级探缓存 | §1.3、§1.4、§3.2 |
| §2 应用壳层 | Item 事件桥、SaveIntent + prompt 三选、ModalLayer/ToastLayer、open_paths 复用窗口、SQLite→缩水版会话恢复 | 第 2 章 |
| §8 主题 | refine 默认色（~15 键起）、内容/选择分离、外部主题目录 + watch ~80 行 | §4.1 |
| 设置 | 类型化快照 + 单 global observe + refresh；两级合并留待将来 | §4.2 |
| 文件层 | watch(path, latency) trait 位按形状预留 + Rescan 兜底 | §4.3 |
| §9.3 插件 | （不变，wasmtime 路线已定稿） | — |
| M5 双击 md | `on_open_urls` 真实 API 名确认 + `OpenMode::NewWindow` 语义 + `set_menus(keymap)` | §1.7 |
| 性能 §11 | 双帧 LineLayout 缓存 + hash 探缓存、块级高亮缓存（现在就做简化版） | §1.4、§3.5 |

---

# 第 6 章 Zed 关键文件索引

```
crates/gpui/src/
  app.rs / app/entity_map.rs        Entity/效果系统/notify 依赖追踪
  app/context.rs                    observe/subscribe/listener/异步 spawn
  element.rs / elements/div.rs      Element 生命周期/Interactivity/跨帧状态
  elements/list.rs / uniform_list.rs  虚拟化（Aline 用 list）
  text_system.rs / text_system/line_layout.rs  双帧缓存/hash 探缓存/wrap/索引映射
  input.rs / window.rs              InputHandler/IME 桥/handle_input/prompt
  keymap.rs / key_dispatch.rs       绑定优先级/上下文谓词/pending 序列
  asset_cache.rs / svg_renderer.rs  异步资产/FFI: svg 栅格
  platform.rs (+ gpui_macos/)       on_open_urls/set_menus/NSTextInputClient
crates/workspace/src/
  item.rs pane.rs dock.rs           Item/Pane(tab/关闭/拖拽)/Panel
  modal_layer.rs notifications.rs toast_layer.rs  弹窗/通知/toast
  persistence.rs (+ persistence/model.rs)  SQLite 会话恢复
crates/project_panel/src/project_panel.rs   文件树参照
crates/ui/src/                     组件库（白名单 11 个）/ styled_ext.rs
crates/text/src/{text.rs, anchor.rs, rope 相关}  事务/undo/Anchor（B 档）
crates/editor/src/{display_map.rs, display_map/block_map.rs, display_map/fold_map.rs, input.rs, editor.rs, items.rs}
crates/language/src/buffer.rs               chunk 高亮缓存/后台 reparse
crates/search/src/buffer_search.rs          查找替换
crates/theme/src/{theme.rs, styles/, registry.rs} + theme_settings/  主题模型/registry/refine
crates/settings/src/settings_store.rs       分层设置
crates/fs/src/{fs.rs, fs_watcher.rs}         Fs trait/watch 去抖
crates/paths/src/paths.rs                    目录约定
```

（各机制的具体 file:line 见上文各节。）
~~~~~~~~
