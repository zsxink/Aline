# Aline 架构规范（architecture-guide）

> 版本：v0.2（2026-10-26）
> 适用范围：本仓库全部代码的**架构级纪律**——分层与依赖、边界、数据流、坐标系、命令与事务、GPUI 使用规则、组件形态、Markdown 引擎规则、性能、测试、演进。
> 配套文档：[engine-design.md](./engine-design.md)（为什么这么设计）、[v1-architecture.md](./v1-architecture.md)（怎么建）、[rust-style-guide.md](./rust-style-guide.md)（代码怎么写）、[zed-notes.md](./zed-notes.md)（Zed 源码实证）。
> 依据：对 ElyOffice 与 Ely-GPUI-Components 的两轮分析（2026-10-26，含成文规范、PRD 工程条款、ADR 与源码实证）+ Zed 全量克隆调研（zed-notes）。
> 冲突裁决：本文与 v1-architecture.md 的具体实现设计冲突时，以 v1-architecture 为准（蓝图更新更快）；本文与 engine-design 的机制结论冲突时，以更新的一方为准并回改另一方。

**执行方式**：与 [rust-style-guide.md](./rust-style-guide.md) 相同，规则分 **【强制】** 与 **【建议】** 两级。每条【强制】都要回答"怎么自动拦截"——能进 CI / 脚本 / 测试的必须落地。这是 Ely 最值得学的元实践：**规则不落到可失败的检查，几个月必然退化**（见 §18 与附录 B）。

v0.2 说明：本文由两份独立起草的规范合并而成（一份基于 Ely 二轮工程实践分析，一份基于 engine-design / zed-notes 的架构边界梳理），取并集、去重复，条款锚点（§1.2 跨层调用、§11 性能反面实证）与既有交叉引用保持一致。

---

## 目录

0. [规范的定位与文档一致性](#0-规范的定位与文档一致性)
1. [分层与依赖](#1-分层与依赖依赖的方向即架构)
2. [公共 API 与模块边界](#2-公共-api-与模块边界强制)
3. [数据流与真源](#3-数据流与真源强制)
4. [坐标系与映射边界](#4-坐标系与映射边界强制)
5. [编辑命令与事务](#5-编辑命令与事务强制)
6. [并发与调度](#6-并发与调度强制)
7. [缓存与失效](#7-缓存与失效强制)
8. [GPUI 使用规则](#8-gpui-使用规则强制)
9. [组件形态](#9-组件形态强制)
10. [渲染与虚拟化边界](#10-渲染与虚拟化边界强制)
11. [性能与反面实证](#11-性能与反面实证)
12. [Markdown 引擎规则](#12-markdown-引擎规则强制)
13. [主题 / 设置 / i18n 传播](#13-主题--设置--i18n-传播强制)
14. [插件边界](#14-插件边界)
15. [持久化与文件](#15-持久化与文件)
16. [错误、断言与日志](#16-错误断言与日志)
17. [测试架构](#17-测试架构强制)
18. [演进与决策记录](#18-演进与决策记录)
- [附录 A：Ely 证据索引](#附录-aely-证据索引)
- [附录 B：门禁分阶段清单](#附录-b门禁分阶段清单)
- [附录 C：三项目对照速查](#附录-c三项目对照速查)

---

## 0. 规范的定位与文档一致性

**分工**（谁回答什么问题，互不重复）：

| 文档 | 回答的问题 |
|---|---|
| engine-design.md | 为什么（ElyMark 逆向 + 机制取舍） |
| v1-architecture.md | 怎么建（模块、类型、流程、接线） |
| 本文件 | 边界与纪律（什么不许做、什么必须做） |
| rust-style-guide.md | 代码怎么写（命名、格式、错误、测试组织） |
| zed-notes.md | Zed 逐机制实证（照搬 / 改造 / 不采用清单） |

1. 【强制】**改架构先改文档再改代码**。影响分层、依赖方向、数据流、公共 API 形状的改动，必须同步更新对应文档；文档没跟上、review 拒绝。
2. 【强制】**文档引用具体实现名必须挂出处**。凡文档写死"用什么库、什么方案"，必须注明决策来源（决策条目或 ADR）。Ely 的 PRD 至今写着 markdown-rs 而实现早已换成 pulldown-cmark（决策在 ADR-0041）——"文档说 A、代码做 B"的漂移就是从引用不挂出处开始的。
3. 【强制】**学到的规则写进管辖它的最近文档**：跨模块原则写本文件，代码级写 rust-style-guide，机制与取舍写 engine-design / v1-architecture。二选一的决定追加到 `docs/decisions.md`（时间线 + 常设决定清单，ElyOffice 模式）；影响多个模块的重大选型可单立 `docs/adr/NNNN-*.md`（Ely 组件库模式：结论 + 测量 + 约束 + 被谁取代）。
4. 【建议】规则条文**一条一个要点**，可审查、可引用；禁止写成一段几千字符的超长条款（Ely `ely-documents/AGENTS.md:7` 单条约 3000 字符、含 40+ 个语义点，无法逐条执行——反面教材）。

## 1. 分层与依赖（依赖的方向即架构）

### 1.1 crate 分层

crate 划分与模块骨架以 [v1-architecture.md §1.2](./v1-architecture.md) 为准，依赖方向：

```
app (bin) → aline-ui → aline-plugin → aline-edit → aline-markdown → aline-buffer
                └─────────────── aline-app-core（theme / i18n / settings，零 GPUI）←──── 谁都能依赖

aline-source 独立：aline-buffer 不依赖它，文件 I/O 由 aline-ui / app 直接编排（2026-10-26 与 v1-architecture §1.2 统一）。
```

各层职责一句话：`aline-source` 磁盘字节层（BOM / CRLF 重放、原子读写，唯一碰文件系统的 crate）；`aline-buffer` 缓冲 + 选区 + 撤销 + IME 状态（trait 化预留 ropey）；`aline-markdown` 解析 / mdast / 展示树 / sources 双向映射；`aline-edit` 命令框架 / 查找替换 / 表格编辑；`aline-plugin` 插件接口与内置插件；`aline-app-core` 主题 token / i18n / 设置；`aline-ui` GPUI 实体、组件、窗口；`app` 只是入口。

**机器可查判定**：引擎 crate（source / buffer / markdown / edit / app-core）逐个跑 `cargo tree -p <crate> -i gpui`，输出必须为空；CI 执行（M0 起）。

### 1.2 跨层调用规则【强制】

- 引擎 crate 之间的调用只走 `lib.rs` re-export 面；**`pub` 面即跨 crate 契约，宁窄勿宽**。Ely `pub(crate)` 泛滥导致外部项目只能整体借鉴重写，前车之鉴。
- `aline-ui` **禁止绕过 `Buffer` 持有文本副本**，一切文本读写走 `Buffer` trait；UI 层出现文本 `String` 字段即违规（打开块编辑器的 `open.text` 除外——它是受控写回通道，§5）。

### 1.3 依赖准入【强制】

1. 新增依赖先对照 engine-design §2.12 清单；**同类功能不得引入第二个库**。
2. **禁止引入第二个 markdown 解析器**（Ely ADR-0041 实测：markdown-rs 比 pulldown-cmark 慢约 90 倍，已定案）。
3. V1 不引入 notify / regex / tokio 等运行时与额外依赖。
4. **gpui 全工作区单一来源、精确锁版**：`gpui = { package = "gpui-pre", version = "=0.3.9" }`（随锁定快照更新）。Ely ADR-0032 的教训：两个来源的 gpui 类型不兼容，无法在同一工作区共存。
5. **依赖版本集中**：第三方依赖统一进根 `[workspace.dependencies]`，成员 crate 一律 `foo.workspace = true`，禁止成员各自写版本号。

### 1.4 拆分与隔离【强制】

1. **不提前拆 crate**：当前按 v1 骨架 8 个 crate 走，拆分的触发条件是**测量数据**，不是审美——Ely ADR-0001 拆分的依据是单 crate 974 文件 / 231k 行 / `cargo check` 63 秒，拆后增量 check 1.3s vs 3.5–4.0s（ADR-0045）。出现"改一处 UI 要等一分钟编译"再拆。
2. **重资产依赖按 crate 隔离**：syntect / ratex / mermaid-rs-renderer / usvg 只允许出现在 `aline-plugin`（内置插件）的依赖里；引擎 crate 与 `aline-app-core` 不得引入（Ely ADR-0043"重依赖按 crate 分配 + check.sh 禁令"）。

## 2. 公共 API 与模块边界【强制】

1. `pub` 是承诺（rust-style-guide §3.3）。补充两条 Ely 实证规则（M4 做一次公开面审计）：
   - **仅为其他 crate 使用的 `pub` 项加 `#[doc(hidden)]`**——它不是给外部用户的 API，不该污染文档与 API 面（Ely 对 ely-core 审出 135 个，ADR-0046）；
   - **没有任何路径能从 crate 外到达的 `pub`，一律降级 `pub(crate)`**。
2. **一个组件一个 home**：UI 组件与它的变体同目录（如 `button.rs` + `icon_button.rs`）；复制的实现必须指向 home（Ely 组件规则，明文）。
3. **复杂控件按关注点拆文件**（Ely `forms/text/` 模式）：实体状态（`mod.rs`）／自定义 `Element`（`element.rs`）／按键绑定（`actions.rs`）／IME（`ime.rs`）／几何与高亮各一个文件。禁止出现一个 2000 行的 `text_input.rs`。
4. **清单用测试或编译期持有，不靠口头**：凡"必须注册、必须穷尽"的约定——块渲染注册表、Command 表、i18n key 集、枚举全量映射——写成**会失败的测试**或穷尽 `match`，新增项忘注册 = CI 红。Ely 的两个范本：`src/containers.rs` 用测试扫描源码断言容器注册表完整；`src/shipped.rs` 扫描已交付组件源码，出现固定英文即失败。

## 3. 数据流与真源【强制】

### 3.1 单一写路径

```
磁盘字节 ⇄ aline-source ⇄ StringBuffer ⇄(只读 Arc<Parsed>)⇄ aline-markdown → 渲染
```

1. **文本永不重建，编辑永远是区间补丁**（`Vec<(Range<usize>, String)>`，块区间排除尾部换行）。没有任何"从 AST 反序列化出 markdown"的路径。
2. **单一真相**：一篇文档的全文与撤销历史只存在一个 buffer（DocCore 持有）；Visual / Source、打开块、查找高亮全是它的投影。**禁止任何"第二份文档模型 + 双向同步"**（Ely PRD §17.2 明文）。
3. **派生状态是不可变快照**：`Parsed` 用 `Arc` 原子换新；渲染只读快照，禁止持有可变的解析状态。
4. 一次 commit = 一次 `Buffer.edit` = 一步撤销；连续打字按 `UNDO_GROUP`（900ms，ElyMark 实证）+ 字节预算合并，补丁合并规则见 v1-architecture §6.3（2026-10-26 统一：此前本条引 Zed 300ms 与 v1-architecture 900ms 两说并存）。

### 3.2 每帧路径【强制】

1. 每帧不新建大 `String` / `Vec`：预格式化内容存进 `Parsed` / `Block`，渲染期只做布局。
2. 组件**不存主题 token**，渲染期从 global 现取（Zed `cx.theme()` 模式，全仓 60+ 组件验证）。

### 3.3 过期判定与异步出口【强制】

1. 过期判定只有一种形状：**单调计数器**（`buffer.stamp()` / `edits`），后台任务回主线程先比计数再采纳（rust-style-guide §14.1）。
2. 每条异步路径（解析、高亮、图片、导出）都要有自己的计数或快照比对。
3. 回调持有 `WeakEntity` + `update(...).ok()`，**错误必须有出口**（toast / 可见状态 / log），禁止 spawn 后不管（Ely 的 `toast_output` / `detach_and_log_err` 模式）。

### 3.4 阈值常量单一来源【强制】

`AT_ONCE`（128 KiB）、`PAUSE`（300 ms）、`LARGEST`（4 MiB）等阈值定义为 `pub const`，文档引用**常量名**而不是数字。禁止文档与代码各写一遍（Ely 的教训：阈值在 AGENTS.md 与 `mod.rs` 双写，改一处必漏另一处）。

### 3.5 事件单向与回调纪律【强制】

子视图 / 子实体通过 `EventEmitter` 发语义事件（`Event::{Changed, Parsed, Pointed, EditRefused, ...}`），父层订阅转换。**有状态实体不存回调**（不把 `Box<dyn Fn>` 塞进状态字段）；无状态组件的 handler 是构造参数——漏传编译失败（Ely 明文："a handler a component needs is an argument of `new`"）。

## 4. 坐标系与映射边界【强制】

1. **源码字节偏移是唯一坐标系**：绘制偏移、UTF-16 偏移都是派生值。三种坐标（源码 `src_off` / 绘制 `drawn_off` / UTF-16 `utf16_off`）之间的每一次转换必须走映射层（`Inline::source_at` / `drawn_at` / `aligned`），调用点禁止手工加减偏移（细则见 rust-style-guide §5）。
2. **映射层所有权归 aline-markdown**：UI 需要新映射需求时扩展 `Inline` 方法，禁止在 UI 层自建第二套映射表或缓存局部偏移（Zed `Edit<D>` 双坐标系一次遍历为目标形态，zed-notes §3.4）。
3. 映射实现细则（`Escapes` 四态、`source_at` / `drawn_at` 尾部对齐、`LineMap` 逐行）以 engine-design §1.4.2 与 §2.2 为准（2026-10-26 修正指向：原引 §2.6 是渲染层章节，非映射规格所在）；新增绘制面必须接入既有映射，不得复刻。
4. 对外 UTF-16 只出现在 IME 边界（`ElementInputHandler` 协议要求），内部一律字节。

## 5. 编辑命令与事务【强制】

1. **命令框架协议固定**：快照 → 后台计算 → 验证 → 应用 → reread 拒绝（Ely apart 模式，engine-design §2.5）；**每条命令 = 一步撤销**；撤销不跨文件、模式切换不产生内容重写（采纳 Ely PRD §14.3 的验收条款）。
2. **Command 表单一来源**：`struct Command { id, keys, blocked, action }` 一张表同时生成快捷键绑定、菜单项、命令面板条目；`blocked` 统一决定禁用；未知命令 id 记 warn、不静默（Ely `ely-ui/src/commands.rs` 模式）。禁止"菜单一份定义、快捷键另一份"。
3. `EditRefusal` 是业务返回值不是错误（rust-style-guide §7.3）；UI 必须用 i18n key 显示原因——"用不了的地方说明原因"（Ely PRD MARK-15）。
4. **打开块四规则**（打开块同一时刻最多一个、只经 DocCore 状态机；Ely 踩坑记录成规，实现细节 v1-architecture §6.2）：
   - 只有"当前文本"的解析结果就绪才能 open；未就绪先挂起（`waiting`），解析到达后补开——否则点击落在过期字节上；
   - 打开期间文本被外部改变（非自己写回）→ 作废：`drop_open()` + log，**绝不静默写回**；
   - 写回 = 一个撤销步，与全文撤销同一历史；子编辑器的 Undo/Redo 用 `capture_action` 接管、转发到文档历史；
   - 菜单 / 对话框临时借走焦点**不关闭打开块**（焦点借出 `lent` 语义），归还后继续。
5. **查找替换**：全量重搜（文档规模下重搜远快于增量维护）+ `MatchesInvalidated` 事件统一失效；匹配区间携带生成时的 `buffer.stamp()`，过期即整组作废、消费方必须校验（v1-architecture §5.1 的 `FindState.stamp`，2026-10-26 与其统一——绝对偏移无失效判据，"(块 id, 块内 range) 稳定引用"取消）；`replace_all` 升序一次 edit = 一步 undo。
6. **IME**：组合期 = 单事务；组合期禁用自动配对；`text_input_configuration` 真变化才转发（防每帧重建 IME 状态）；marked text 用高亮显示而非替换文本。
7. **dirty 是合成事实**：`dirty = history.undo 栈深 != saved_at || 打开块有未提交修改`（2026-10-26 修订：不用 stamp 比对——逆补丁精确，栈深回到保存值 ⟺ 字节一致，stamp 会把"打字后撤销回原文"误判 dirty；保存隐式封口打字合并组，v1-architecture §6.3/§15.2）；状态栏与关闭确认消费同一个合成判定，禁止散落多个 bool 各自为政（Ely `Tab::dirty` 三事实合成的教训）。

## 6. 并发与调度【强制】

1. 引擎 crate **零线程、零锁、零全局状态**；并发只发生在 UI 层 spawn 点。
2. **异步三件套**（Zed 形状，rust-style-guide §18.2）：`cx.spawn(async move |this, cx| ...)` 后台任务 / `background_spawn` 纯计算 / `WeakEntity` 回调（弱引用升级失败即静默放弃，不许 panic）。
3. **防抖归属拥有状态的层**：解析防抖在 DocCore（拥有 `Parsed`），保存防抖在拥有会话状态的层；视图层出现定时器即架构错误（Zed `fs.watch(path, latency)` 范式）。
4. V1 不做文件外部修改监听；`Fs` trait 按接口预留 watch 槽位（Zed 签名），兜底方案 Rescan（焦点恢复时重读 mtime + 大小）。【建议】

## 7. 缓存与失效【强制】

1. 缓存键 = **内容 hash + 环境**（主题 / 字体 / 宽度 / 缩放）。缺一维就是缓存错版 bug（Zed `shape_line_by_hash` 范式）。
2. 失效靠事件（`Event` / `cx.notify` / `emit`），不靠轮询、不靠时间猜测。
3. 粒度：块级优先（`alike` 后只 splice 变化块），全量兜底；禁止节点级细粒度缓存在 V1 出现。

## 8. GPUI 使用规则【强制】

主要来源：ElyOffice `agents.md:94-104`（gpui 规则 14 条）+ 组件库 `crates/ely-core/AGENTS.md`，逐条对照 Aline 采纳。

**渲染纪律**

1. `Render::render` 是纯函数：只读状态、产出元素树；**render 里禁止改状态、禁止 `cx.notify()`**。
2. 渲染之外改了状态 → `cx.notify()`；绘制中（prepaint/paint）改了 → 再 `window.request_animation_frame()`；**不用 RAF 轮询布局**。
3. 动画只在运行时请求帧，**空闲零重绘**；时长走统一 motion 常量，单段 ≤220ms；遵守系统"减少动效"（reduced motion 下静止不抖）；动画 id 传给子孙元素——换 id 会重建它们的焦点与拖拽状态。

**元素与状态**

4. 三种局部状态形态，**先选形态再写代码**（§9.1）。
5. 元素 id 稳定：可复用组件内的 id 带上属主 id；同块内节点的 id 带字节偏移（Ely：`key(what) → format!("{what}-{at}")`）——id 稳定则跨帧不重建、不丢焦点。
6. `debug_selector` 是测试锚点，随实现一起写：块 `markdown-block-{ix}`、打开块 `markdown-open`、表格单元格 `markdown-open-cell`（Ely 同名约定；测试靠它走真实命中测试，见 §17）。
7. `hover()` 必须带元素 id；虚拟列表行必须 keyed；`uniform_list` 必须 `w_full`。

**浮层与焦点**

8. 浮层（弹窗/菜单/浮动工具条/toast）统一走 `primitives::raise` **单一入口**（Ely 用 check.sh 禁止其他地方直接 `deferred(`）；需要挡住下层交互用 `occlude()`；滚动穿透按需 `block_mouse_except_scroll`。
9. 对话框五件套：抢焦点给首个可交互元素、`FocusScope::trap()` 困焦点、Escape `stop_propagation` 关闭、**关闭时把焦点还给打开前的元素**（take_focus / give_back 链，Ely ADR-0026）、遮罩可点击关闭。
10. 每个窗口一个 `Toaster` + `ToastViewport`。
11. 可按压元素都是 Tab 停靠点：`tab_index(0)` + `focus_ring`（1px focus 色边框）；按 role + aria_label 命名；**禁用即退出 Tab 序列**；按钮在 `on_mouse_down` 里 `prevent_default`——点击不抢焦点。
12. 文本输入统一用 TextInput 实体（三层分离见 §9.3）；需要语义的按键（Up/Down/Enter/Backspace…）用 `capture_action` 接管。

**全局与启动**

13. 需要全局广播的横切状态才进 `Global`（主题 / 语言 / 设置），其余状态一律 Entity 树 + 订阅；消费统一走 `cx.observe_global::<T>()`（传播细则 §13）。
14. **初始化收口一个 `init(cx)`**：资产、主题、i18n、键位、插件注册、测试初始化（`init_for_tests`）全部从这一个入口走，宿主不需要知道内部结构。
15. 窗口关闭确认走统一 guard，不直接用 `on_window_should_close`——Ely 的教训（`shell::guard_close`）：后者会被自绘标题栏的关闭按钮绕过。
16. 颜色、尺寸、圆角、阴影只能来自 `cx.theme()` token；间距用 rem 阶梯、边框用固定像素（`border_1` 发丝线）；**禁止裸 `px()`**（M4 起脚本扫描）。**测量值也 token 化**：侧栏宽、滚动条粗细、弹窗宽、列表 overdraw 等产品级尺寸写成 `Theme` 方法，组件里不出现内联数字（Ely `theme/tokens.rs` 约 60 个测量方法的模式）。
17. 图标集中注册为 `static`、组件按名字引用；`svg()` 必须显式给 `text_color`，否则暗色下不可见。

## 9. 组件形态【强制】

### 9.1 三形态选择表（先选形态再写代码）

| 形态 | 判据 | Aline 对应 |
|---|---|---|
| `#[derive(IntoElement)]` + `RenderOnce` 构建器 | 无持久状态，每次渲染由调用方给全部输入 | 按钮、菜单项、块渲染、状态栏 |
| `Entity<T>` + `EventEmitter` + `Focusable` | 有编辑/选区/历史/IME 等持久状态，需要被外部观察 | TextInput、DocCore、Toaster |
| `window.use_keyed_state` | 只属于某个元素实例的短暂状态（开合、动画代际、拖拽槽位） | 弹层开合、侧栏折叠动画 |

### 9.2 构建器纪律

- 构建器方法只赋值不做计算；**`RenderOnce` 是唯一"算账"的地方**（Ely `button.rs` 的模板）。
- 变体用枚举描述**语义**（Primary / Ghost / Danger…），颜色由 `tone(variant, &Palette)` 从主题调色板算出——变体里不出现色值。
- 同一组件的颜色/尺寸映射函数集中一处（Ely 的 `tone()`、`label_size()`）。
- 回调一律命名类型别名，全库统一签名：`type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>`。

### 9.3 文本输入三层分离（对 Aline 最关键的模式，Ely 已验证）

- **实体层**：状态机 + 语义——文本、选区、IME（marked/composing）、撤销历史、过滤、禁用；对外 emit `InputEvent::{Changed, Submit, Focus, Blur}`。语义细节照抄 Ely：`set_text` 不是撤销步、`insert` 是撤销步；`commit` 仅在文本真变时 record + emit；越界 `select` log 后放弃、不 panic；构造参数非法（如 min > max）构造期 `assert!`。
- **Element 层**：`request_layout`（多行用 `request_measured_layout` 实测换行，不能猜）→ prepaint 画选区/光标/IME 下划线 → 注册 `ElementInputHandler`。实体不碰像素，Element 不存语义。
- **外壳层**：`RenderOnce` 的框——边框按 `invalid > focused > normal` 选色、前后缀、清除按钮；持有 `Entity<TextInput>`。

### 9.4 能力 trait（第二个可设尺寸/可禁用的控件出现时落地）

`Sizable / Disableable / Selectable / Changeable` 统一方法名，让泛型 UI 代码不需要 match 组件类型（Ely 用 `capable!` 宏批量实现：30 个 size、19 个 disabled，且有泛型测试证明只靠 trait 约束可驱动任意组件）。v1 控件少：先统一方法名，trait 化等第二个使用者（与 rust-style-guide §10 一致）。

## 10. 渲染与虚拟化边界【强制】

1. 长文档用**变高 `list`**（Zed）不用 `uniform_list`：`size_hint` 渐进测量 + `ScrollAnchor` 稳滚动 + scroll-to 排队到下一帧 prepaint。
2. 块高度：`Some(估算)` + 实测后 `resize_blocks` 回写；`None` = 完全不占位——禁止拿 `None` 当自适应（zed-notes §3.2 复核结论）。
3. 语法高亮桶：`HighlightKey` 枚举变体顺序即优先级（`derive(Ord)` + `BTreeMap` 升序），不另设优先级表。
4. 上编辑下渲染（块级公式 / mermaid）按 `BlockPlacement::Replace` 建模（块内布局），不走浮层。【建议】

## 11. 性能与反面实证

预算表见 [v1-architecture.md 第 11 章](./v1-architecture.md)。架构级补充：

1. **关键场景的 bench 至少断言"一次输入只重绘一次"**（Ely `benches/budget.rs` 的 `draws_once`：包一层计数视图，输入一个字符断言只画一帧）——防 GPUI 最隐蔽的浪费：一次交互多帧重绘。首批场景：Visual 打字一键、commit 换块、滚动长文档、切主题。
2. **预算是数字，进 CI**：帧 ≤16ms、解析 ≤50ms/128KiB、回归 >10% 判失败（对齐 Ely 的预算协议）。
3. **布局降级有确定顺序**：面板空间不足时按"收缩两侧 → 隐藏右 → 隐藏左"的固定次序降级，每级有最小宽度与内容下限（Ely `SidePanel::room`，内容下限 360px），禁止"挤到 0"或无序跳变。Aline 对应：文件夹侧栏与正文区的分配规则。
4. **失焦窗口降频**：`WindowOptions` 设 `inactive_frame_interval`（Ely 250ms），进打包任务。
5. **布局持久化同形**：当布局需要持久化时，保存的形态与运行的形态是**同一个类型**（`Serialize` / `Deserialize`，Ely `PaneLayout` 模式），版本化 JSON、恢复全有或全无——不维护两套互转结构。
6. **编辑事件路径禁止 O(文档) 工作**：按键、选区移动、打开块等高频路径上不得做全文解析、序列化、指纹、字数统计、全量高亮。这类工作只允许由 commit / 命令应用 / 显式功能触发，或转后台 + 代次校验（§3.3）。反面实证见下。

### 反面实证：MarkFlow 性能诊断（2026-10-25，issue #458/#459/#460）

旧项目 MarkFlow（Tauri + ProseMirror）128KB 文档打开慢、打字卡的三个病根，全部是本节与 §3.3 规则的反面案例（完整证据链与量级见 MarkFlow 仓库对应 issue）：

| 病根 | 违反的纪律 | 量级 |
|---|---|---|
| frontmatter 面板默认参数在函数体前求值 + onSelectionUpdate 同步 emit 绕过防抖 → 每次按键跑全文档 reconcileSave | 编辑事件路径做了 O(文档) 工作 | 按键中位 2.1ms → 36.6ms（17×） |
| KaTeX 装饰器无 memo，每事务重建全部 widget DOM（同仓库 code-block-highlight 有正确范本：memo + map 复用） | 装饰/缓存必须 memo + 位移复用 | 每按键 +45ms，随公式数线性增长 |
| 打开路径 parse→fp→serialize→re-parse 双解析双指纹 | 打开只解析一次；中间的重新序列化正是"序列化器架构"额外交的税 | 打开 ~230ms |

ElyMark 也发作过同一种病：16MiB 全选字数统计在 UI 线程 1.8s，后改后台 + 代次校验（§3.3 的形状即源于此类修复）。两个项目踩的是同一个坑——**在编辑事件路径上做全文档工作**；Aline 的防线就是规则 6 + §3.3 + 本节 1/2 的 bench 断言。

## 12. Markdown 引擎规则【强制】

机制设计见 engine-design.md 第 1、2 部分；这里是实现纪律，全部有 Ely 源码实证：

1. **解析 = pulldown-cmark（默认 features 全关、逐项开启）→ 自建 mdast（每节点 `position: Range<usize>`）→ 展示树**。不直接消费事件流渲染；事件折叠成树用显式 `Frame` 栈，结束时 `assert!(stack.is_empty())`——"每个 start 都有它的 end"是构造事实，用断言表达（Ely `build.rs:335`）。
2. **解析器读法修正独立成模块**（Ely `markdown/shape.rs` 11 个 pass 的模式）：每条修正（`$$` 围栏改写、`\(\)` 定界符、front matter 占位、字面 autolink、任务框、超表头单元格保留原文……）单独函数 + 注明理由 + 独立测试；修正输出必须携带回原文的字节映射。
3. **换实现、加修正，用"参考实现 + 语料比对"验证**：Ely 换 pulldown-cmark 时在 1324 条 CommonMark+GFM 规范样例上与旧实现逐条比对（1222 条一致，ADR-0041），而不是靠人读代码。Aline 建 M0 解析内核时用同样方法锁定行为。
4. **增量 = 全量重解析 + 头尾配对**，不做增量解析器：`alike(old, new)` 的判据是**节点结构相等 && 对应源切片相等**；只 `splice` 中间差异块，未变块的元素与映射全部复用。解析结果的块坐标 `rebase` 成块相对坐标，保证"前面编辑不影响后面块的等价性"。
5. **三阈值是常量**（§3.4）：≤ `AT_ONCE` 同步解析；更大走后台 + `PAUSE` 防抖；> `LARGEST` 不解析、降级源码模式。
6. **source 映射四要素**（rust-style-guide §5 的机制根据）：
   - 每个绘制片段记三元组 `(绘制起点, 源区间, Escapes)`；`Escapes` 四态（Plain / Markdown / Html / Pipes）缺一不可——同一段绘制文字回推源码时解码规则不同；
   - `source_at` / `drawn_at` 用 `partition_point` 二分，**匹配从尾部对齐**（等字符优先 → 空格对行尾 → 跳过被丢弃的标记与前导空白）——行首引号、缩进、emoji 前文本不错位的全部原因；
   - 代码块/围栏/HTML 块用 `LineMap` 逐行映射（LF/CRLF/CR 都算行尾）；
   - 禁止调用点手工加减偏移（§4）。
7. **打开块四规则见 §5.4**；打开块的文本语义（源码坐标光标、单撤销步）同样约束本节。
8. **一个光标服务所有模式**：光标/选区永远是源文本字节坐标（rust-style-guide §5）；模式切换只换视图、不换坐标、不产生内容重写。
9. **行尾铁律**：编辑器内部一律 LF；行逻辑（行首/行尾/End 键）不得假设"CR 不存在"——Ely CodeEditor 只按 LF 分行，CRLF 文本里 End 键可落在 CR 与 LF 之间（其 AGENTS.md 明文记录的已知缺陷）。Aline 在编辑器层统一处理 CRLF，不让同类缺陷过版。
10. **文本消费者同树**：渲染、导出、大纲、字数统计、查找高亮全部消费同一个 `Parsed` 与同一套解码规则；禁止任何地方出现第二套"自己再解析一遍"的路径。
11. **高亮一份实现两处用**：markdown 高亮函数同时服务 Source 模式的 CodeEditor 与打开块的 TextInput（Ely 同款复用）。【建议】

## 13. 主题 / 设置 / i18n 传播【强制】

1. **单一 global + observe + refresh_windows 是唯一传播机制**：theme / locale / settings 各一个 `Global`（§8.13 的准入规则），改动后 `cx.observe_global` + `refresh_windows()`（Zed 78 个观察点验证的形态）；禁止第二套通知机制。
2. **设置一次解析成类型化快照**（`Settings` struct），运行期不再碰 TOML；两级合并（默认值 + 用户文件）起步，Zed 六层 SettingsStore 明确不抄。
3. **主题 token 语义分组**（背景 / 前景 / 强调 / 状态 / 语法）+ refine 派生：内置默认值可只填约 15 个锚点色，其余由锚点推导（Zed `ThemeSettings::refine` 形态）；用户主题先逐字段合并 GitHub 基准，再执行派生。主题包可覆盖规范 allowlist 中的叶子字段；内容色与选择色分离。【建议】
4. **i18n `t("key")` 第一天强制**（rust-style-guide §11）；引擎错误枚举的翻译只发生在 UI 层。
5. 主题切换首帧走"立即应用"路径（Ely `set_mode_now` 专供首帧），之后的切换才可过渡——首帧不闪。
6. token 名与调色板字段的对齐用测试锁死：每个颜色名都能被 `token()` 找到，未知名 panic（Ely `theme/tests.rs`）。

## 14. 插件边界

1. V1 编译期静态注册（五个内置插件即接口测试用例）；动态加载后置到 V2。插件设置归属与启停行为见 [plugin-system.md](./plugin-system.md)。【强制】
2. **插件接口分两层（2026-10-26 裁决，与 v1-architecture §9.1 统一）**：外部（wasm/进程外）插件只允许 `AssetRenderer → RenderDesc`（SVG 字节 / 尺寸，纯数据进出，无元素、无 gpui 类型）——锁定 V2 wasm 化的接口形状；内置（编译期进程内）插件可用元素级 `BlockRenderer`（experimental，永不跨 wasm 边界）。原"一律返回 desc"的表述与 v1-architecture §9.1 的 AnyElement 签名冲突，本条修订为两层制。【强制】
3. **解析扩展契约由 `aline-markdown` 定义，插件实现并由宿主注入**；引擎 crate 禁止反向依赖 `aline-plugin`。预处理必须携带“改写字节 → 原源码字节”映射；解析后处理只消费核心 mdast 并生成带源码范围的纯数据扩展节点。pulldown-cmark 仍是唯一 Markdown 解析器。【强制】
4. 扩展解析为确定性纯逻辑，不访问 GPUI、文件系统或网络；固定执行顺序，扩展 ID 唯一，冲突需显式诊断并回退核心行为。【强制】
5. 声明式资源（外部主题 / 语言包 catalog）宿主直读，永不进沙箱；`extension.toml` 清单约定第一天就用（Zed 同款）。【建议】
6. 接口标注 `experimental`，形状不稳是已知风险（v1-architecture 第 14 章）。【强制】
7. **文档修改型插件只能返回提案，不能直接写 buffer**：提案绑定文档身份和生成时版本，携带源码区间补丁；UI 展示差异并等待用户接受，`aline-edit` 校验版本和范围后以单事务应用。过期提案拒绝并重新生成，不自动重基。【强制】
8. 将文档内容发送给远程 AI 前，必须清楚展示发送范围与目标服务并获得用户授权；不得在后台静默上传完整文档。【强制】

## 15. 持久化与文件

1. **原子写是唯一出口**：`aline-source` 内 temp + rename；任何其他 crate 出现 `std::fs::write` 即违规。【强制】
2. 路径约定 `OnceLock` 收口（`aline_source::paths`），`init(cx)` 里装配。【强制】
3. **会话恢复缩水版**：每窗口一条记录（打开文件、活动标签、布局）+ KVP 轻量项；防抖写回，退出时 flush。Zed SQLite 方案按此缩水。【建议】
4. 布局持久化同形原则见 §11.5。【建议】

## 16. 错误、断言与日志

1. 错误三分类（rust-style-guide §7 已定分层）：**拒绝**（`EditRefusal`，业务正常返回）／**领域错误**（引擎 crate 枚举 + thiserror）／**应用错误**（aline-ui / app 用 anyhow）。Ely 用手写 `impl Display` 替代 thiserror，以精确区分"给用户看的"与"给日志看的"；Aline 保留 thiserror 的既定决策，但吸收其要求：**引擎错误的 `Display`/`Debug` 面向日志与诊断写；面向用户的文案一律由 UI 层的 i18n key 给出**（v1-architecture §10：引擎零依赖 i18n）。
2. **出错就暴露**：外部数据/活数据可触发的失败状态 → `log::error!`（指名归属）+ 可见状态（错误面板 / 占位 / toast）+ 程序继续运行；**禁止静默兜底、禁止用默认值遮挡缺失数据**（Ely 应用层十条之二）。
3. 断言分级（rust-style-guide §7.2 补充）：`assert!` 只用于两类事实——调用方参数、构造上必真的不变量；外部输入永远不触发 assert。每个 `expect` / `panic!` / `unreachable!` 都要能回答"为什么此刻成立"（Ely 用 `panics.tsv` 白名单逐个登记，403 行；Aline M6 后可选引入同款 CI 检查）。
4. **日志纪律**：`log` crate（与 gpui 一致）；关键路径必须有日志（后台解析被丢弃、保存、命令拒绝、超时告警）；消息英文短句、带对象标识（如 `log::debug!("markdown document: read {edits} lapsed")`）；**禁止打印文档正文、用户内容、密钥**。
5. **已知缺陷明文记录**：发现缺陷先写进文档（现象、影响、触发条件），再决定修或标注"已知"——Ely 在 AGENTS.md 明文记录 End 键缺陷的做法，比"看见了装没看见"诚实且可追溯。

## 17. 测试架构【强制】

组织规则见 rust-style-guide §13；分层与基础设施：

1. **四层**：单元（同文件）→ 模块集成（`tests/` 模块目录；共享夹具**独立成模块并在文档写明用法**——Ely `document/tests/` 19 个模块的夹具藏在 `mod.rs`，新人必须读夹具源码才能写测试，反面教材）→ 公共 API 外部测试（`tests/` 下只用公开 API 拼一个"小应用"，检验 API 真实可用性，Ely `tests/form.rs` 模式）→ gpui 集成（`TestAppContext`）。
2. **gpui 测试走真实事件路径**：用 `debug_selector` / `debug_bounds` 拿元素坐标，再 `simulate` 鼠标/键盘/滚轮事件，**不直接调用私有函数**（Ely 的 `press()` / `frames()` 夹具模式）。
3. **复杂度守卫**：核心算法（解析、映射、查找）配线性守卫测试——4 倍输入 ≤ 8 倍耗时，取多次最优（Ely `linear(source)` 防算法渐进退化）。
4. **等价比对锁定行为**：换实现、加修正用参考实现 + CommonMark / GFM 样例逐条比对（§12.3）。
5. **hermetic**：测试不依赖宿主环境（剪贴板、系统主题、时区、网络、字体）；时间用注入值（FakeFs `Duration::ZERO` 确定性缝，rust-style-guide §13）。
6. **清单注册表由测试持有**：块渲染注册表、Command 表、i18n key 集写成会失败的测试（§2.4；Ely `containers.rs` / `shipped.rs` 模式）。
7. **实体释放**：gpui 实体在测试结束被释放（Ely 的 leak-detection 测试），M6 后可选。

## 18. 演进与决策记录

1. **决策记录**：新建 `docs/decisions.md`（时间线 + 常设决定清单两段式，ElyOffice 模式）。每个有争议的选型记一条：结论 + 当时的测量/约束 + 日期；被取代时写明替代条目。重大且独立的选型（解析器、gpui 版本策略）可单立 ADR 文件（§0.3）。
2. 【强制】本文件【强制】条款的修改 = 架构决策变更：写明动机 + 三项目先例对照 + 测试影响；参考项目的新结论**不自动**改变规范，需显式采纳（改本文件并注明出处）。
3. **门禁分阶段**：规则不一次上全，按附录 B 的里程碑接入；每条【强制】规则落地时在本文标注检查方式。
4. **明确不照搬的部分**（防止过度工程，均有实证依据）：
   - 31 个 crate 的拆分——Aline 8 个够用，拆分看测量（§1.4）；
   - 44 章全谱组件库——只做编辑器需要的子集；
   - gallery 截图/像素对比 CI——后期再说；
   - compat 共存 demo——无需求；
   - 53 条 ADR 的体量——先保证"每个有争议的决定有一条记录"；
   - 手写错误 Display 替代 thiserror——保留既定决策（§16.1）；
   - 单实例参数移交——Aline 已明确选择"双击 `.md` → 新窗口"（v1-features §1）。

---

## 附录 A：Ely 证据索引

分析基于 2026-10-26 的两个仓库快照。`ely-*` 省略了 `/Users/xian/Project/book/Ely-GPUI-Components/crates/` 前缀；`ElyOffice/` 与 `Ely-GPUI-Components/` 为完整路径。

| 主题 | 关键位置 |
|---|---|
| gpui 规则 14 条（本规范 §8 主要来源） | `ElyOffice/agents.md:94-104` |
| 应用层十条（出错暴露 / 修根因 / 500 行 / 动效 220ms） | `ElyOffice/agents.md:106-117` |
| PRD 工程条款（单一真相 / 保真 / 撤销边界） | `ElyOffice/PRD.md` §14、§17、§20、§22 |
| Markdown 引擎明文规范（单行超长条款，正反例都是它） | `ely-documents/AGENTS.md:7` |
| 组件库规则全书（主题 token / 浮层 / 焦点 / 能力） | `Ely-GPUI-Components/AGENTS.md`、`ely-core/AGENTS.md` |
| 解析与三阈值 | `ely-documents/src/markdown/build.rs:20,51,118,335`；`document/mod.rs:48,51,54,328,356` |
| alike 与 rebase | `ely-documents/src/document/parse.rs:31,63` |
| source 映射（Escapes / 尾部对齐 / LineMap） | `ely-documents/src/tree/sources.rs:9,20,159,187,201,303` |
| 打开块（waiting / 作废 / 焦点借出） | `ely-documents/src/document/edit.rs:76,161,217,271`；`mod.rs:311` |
| 命令表单一来源 | `ElyOffice/crates/ely-ui/src/commands.rs` |
| 文本输入三层分离 | `ely-core/src/forms/text/{mod,element,actions,ime}.rs`、`ely-core/src/forms/input.rs` |
| 无状态组件模板 | `ely-core/src/buttons/button.rs:150-370` |
| 浮层与层级 | `ely-core/src/primitives/layer.rs`、`ely-core/src/overlays/dialog.rs` |
| 主题 | `ely-core/src/theme/{mod,palette,tokens,tests}.rs` |
| 门禁脚本 | `Ely-GPUI-Components/scripts/check.sh`、`scripts/panics.{py,tsv}`、`src/{containers,shipped}.rs` |
| 决策记录 | `Ely-GPUI-Components/docs/adr/`（0001 拆分、0032/0039 gpui、0041 解析器、0043 重依赖、0046 API 审计、0051 树公开） |
| 性能 bench | `Ely-GPUI-Components/benches/budget.rs` |
| gpui 测试模式 | `ely-documents/src/document/tests/`（open / press / frames 夹具） |

## 附录 B：门禁分阶段清单

| 阶段 | 新增检查 | 落点 |
|---|---|---|
| M0 | fmt / clippy `-D warnings` / test（rust-style-guide 附录 C）；引擎 crate 禁 gpui 依赖（脚本查 Cargo.toml 与 `cargo tree`）；`missing_docs` | CI |
| M2 | i18n key 覆盖（en/zh key 集合相等）；文件行数上限告警（500 行） | CI / check 脚本 |
| M4 | 已交付 UI 无固定英文扫描（shipped 模式）；裸 `px()` 扫描；公开面审计（`#[doc(hidden)]` / pub 降级） | check 脚本 |
| M6 | panic 白名单（panics.tsv 模式）；性能 bench 预算（帧 16ms / 回归 10%）进 CI；实体释放测试 | CI |

## 附录 C：三项目对照速查

| 来源 | 继承（做法 + 先例锚点） | 教训（反面，明令禁止） |
|---|---|---|
| **ElyOffice / Ely-GPUI-Components** | pulldown-cmark 选型 + 预处理改写器（§12）；sources 双向映射（§12.6）；apart 命令协议（§5）；打开块四规则（§5.4）；gpui-pre 锁版本（§1.3，ADR-0032）；TestAppContext 测试法（§17）；`draws_once` bench 断言（§11）；Command 单表（§5.2）；debug_selector 约定（§8.6）；重依赖按 crate 隔离（§1.4，ADR-0043）；门禁分阶段（附录 B） | 引擎与 gpui 混层；`pub(crate)` 泛滥（§1.2）；markdown-rs（§1.3，ADR-0041 慢 90 倍）；全文快照 undo（Aline 用补丁式）；LF-only 行分割——CRLF End 键缺陷（§12.9）；PRD 与实现漂移（§0.2）；阈值文档代码双写（§3.4）；超长规范条款（§0.4）；测试夹具藏 `mod.rs`（§17.1） |
| **Zed** | 双帧 LineLayout + hash 探缓存（§7.1）；效果驱动精确失效（§7.2）；WeakEntity 回调 + 异步三件套（§3.3 / §6.2）；ElementInputHandler / UTF-16（§4.4）；list 虚拟化 + 块高度语义（§10）；HighlightKey 枚举即优先级（§10.3）；全量重搜 + 事件失效、replace_all 单事务（§5.5）；事务形状 300ms 合并（§3.1）；SaveIntent + prompt；refine 主题（§13.3）；类型化设置快照（§13.2）；FakeFs ZERO（§17.5）；防抖归属（§6.3） | CRDT undo（Aline 用补丁式）；六层 SettingsStore（§13.2）；multibuffer / collab / Panel 三侧 dock；a11y / inspector 全家桶 |
| **MarkFlow** | 设置模型、导出主题、图片粘贴、自动保存语义（迁语义不迁代码） | 三病根（§11 反面实证）：编辑事件触发全文档工作、装饰无 memo、打开路径同步无虚拟化 |
