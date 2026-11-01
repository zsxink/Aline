# architecture-guide 历史证据摘录

2026-10-30 按 PROTO-06 方案 A 保全。**非现行规范，不执行其中的代码/配置/命令；“照搬/强制/已验证/待确认”均为原稿当时语境。** 当前约束以 [文档入口](../../README.md) 和 [确认归档](../2026-10-30-migration-decisions.md)为准。

原路径：`docs_bak/docs/architecture-guide.md`；原文件 42050 bytes，SHA-256 `21b33172947617380ae639ea374b15561a99a639b9a7aa827c7326c2f8e095b3`。保留的是下列逐段原文（含源码、表、图例、示例、路径）；未迁片段的归属见 [证据索引](./README.md)。源码行号只表示该历史文件，未重验外部源码或第三方性能。

## L1–L14 · 原规范身份

原片段 SHA-256：`00617067c10fc53b10af4ad4e6daf7a4a45b28c7273950c26257a8144f11f300`。

~~~~~~~~markdown
# Aline 架构规范（architecture-guide）

> 版本：v0.2（2026-10-26）
> 适用范围：本仓库全部代码的**架构级纪律**——分层与依赖、边界、数据流、坐标系、命令与事务、GPUI 使用规则、组件形态、Markdown 引擎规则、性能、测试、演进。
> 配套文档：[engine-design.md](./engine-design.md)（为什么这么设计）、[v1-architecture.md](./v1-architecture.md)（怎么建）、[rust-style-guide.md](./rust-style-guide.md)（代码怎么写）、[zed-notes.md](./zed-notes.md)（Zed 源码实证）。
> 依据：对 ElyOffice 与 Ely-GPUI-Components 的两轮分析（2026-10-26，含成文规范、PRD 工程条款、ADR 与源码实证）+ Zed 全量克隆调研（zed-notes）。
> 冲突裁决：本文与 v1-architecture.md 的具体实现设计冲突时，以 v1-architecture 为准（蓝图更新更快）；本文与 engine-design 的机制结论冲突时，以更新的一方为准并回改另一方。

**执行方式**：与 [rust-style-guide.md](./rust-style-guide.md) 相同，规则分 **【强制】** 与 **【建议】** 两级。每条【强制】都要回答"怎么自动拦截"——能进 CI / 脚本 / 测试的必须落地。这是 Ely 最值得学的元实践：**规则不落到可失败的检查，几个月必然退化**（见 §18 与附录 B）。

v0.2 说明：本文由两份独立起草的规范合并而成（一份基于 Ely 二轮工程实践分析，一份基于 engine-design / zed-notes 的架构边界梳理），取并集、去重复，条款锚点（§1.2 跨层调用、§11 性能反面实证）与既有交叉引用保持一致。

---

~~~~~~~~

## L42–L58 · 旧文档一致性政策，已被现行权威规则替代

原片段 SHA-256：`f3328b439d8046ccfeb67bd1c2c94a18b0d75ec42b77cdf5392ea6d183ff50bd`。

~~~~~~~~markdown
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

~~~~~~~~

## L135–L168 · 旧映射/命令/调度/缓存约束原始证据

原片段 SHA-256：`bcc3af24b34a266db01d6cc3f8cfccbe9e48618b8e1c8fe234f394e838c36e8e`。

~~~~~~~~markdown
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

~~~~~~~~

## L236–L278 · MarkFlow三病根和引擎约束原始证据

原片段 SHA-256：`42ed7c06e2328b605f4292da7437188d2a88fec12bb67e39dc9bba280f654267`。

~~~~~~~~markdown
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

~~~~~~~~

## L342–L382 · Ely/ADR/测试完整索引与历史分阶段门禁

原片段 SHA-256：`427530f1a22e6afa7ffdda61847d5228d2568f29a04c6b5a7bae10a39647f68f`。

~~~~~~~~markdown
## 附录 A：Ely 证据索引

分析基于 2026-10-26 的两个仓库快照。`ely-*` 省略了 `<external>/Ely-GPUI-Components/crates/` 前缀；`ElyOffice/` 与 `Ely-GPUI-Components/` 为完整路径。

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
~~~~~~~~

## L89–L92 · Ely拆crate的旧测量与隔离研究依据；不是Aline成绩或拆分阈值

2026-10-30 独立删除准备复核补存：仅保全独有历史论证，原平台/API/性能/算法断言未经本轮外部复验，当前产品选择、里程碑与V验证状态不变。

原片段 SHA-256：`7bec3c62939c9e3ffd7ca6b54e88dfb146a5c802c07825d36be35a1995e60c1e`。

~~~~~~~~markdown
### 1.4 拆分与隔离【强制】

1. **不提前拆 crate**：当前按 v1 骨架 8 个 crate 走，拆分的触发条件是**测量数据**，不是审美——Ely ADR-0001 拆分的依据是单 crate 974 文件 / 231k 行 / `cargo check` 63 秒，拆后增量 check 1.3s vs 3.5–4.0s（ADR-0045）。出现"改一处 UI 要等一分钟编译"再拆。
2. **重资产依赖按 crate 隔离**：syntect / ratex / mermaid-rs-renderer / usvg 只允许出现在 `aline-plugin`（内置插件）的依赖里；引擎 crate 与 `aline-app-core` 不得引入（Ely ADR-0043"重依赖按 crate 分配 + check.sh 禁令"）。

~~~~~~~~
