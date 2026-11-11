# Design

## Context

动机与范围见 `proposal.md`。本变更创建于 2026-11-02 并自 2026-11-03 获授权 apply，当前按 `tasks.md` 部分实施：纯机制、原生准入与骨架无头部分已完成，真实窗口观测与阶段出口仍在进行；下列模块表是实施落位说明，不是"尚未开始"的声明。建仓时主规格包含 engine/*、engineering-governance 和 workspace/engineering-gates；M0 已归档，验收豁免不补证。CODE_STANDARDS §2～6、架构规范 R/K 与 Rust S/C 继续约束实现。

2026-11-06 用户决定把治理 change `qmd-01-engineering-practices` 并入本变更并删除。本节 §10 说明合并结构与边界；其 `engineering-governance` delta 见 `specs/engineering-governance/spec.md`。该范围没有运行时模块、状态所有者或依赖变化，不影响 §1～9 的 M1 实现设计。

CodeGraph 与磁盘源码核对得到的实施起点：

| 入口 | 已存在的事实 | M1 缺口 |
|---|---|---|
| `app/src/main.rs`、`app/Cargo.toml` | `ExitCode` 薄入口，只调用 `application_name`；仅依赖 ui/app-core | 主循环、原生单窗及最小菜单装配 |
| `crates/aline-ui/src/document/doc_core.rs` | 唯一 `StringBuffer`/历史/草稿；`new` 按初始选区**自动开块**；`set_selection` 及输入基于开块协议 | 不开块初始化、文档级只读选区/模式及受检只读能力 |
| 同目录 `protocols.rs` 与 `accept_derived_result` | 现有切模式需活动开块；结果采纳以 `open.is_none()` 拒绝；`current_snapshot` 可读取无草稿内容 | 只读文档存活与编辑块存活分开，不能绕过检查临时开块 |
| `crates/aline-markdown/src/parse.rs`、`mdast.rs`、`build.rs` | `Parsed` 含快照、顶层块/Gap、全文 `Projection`、HTML 样式/保护/导航；`Block` 仅 range/kind | 不能假设已含嵌套块树、Markdown 全样式、代码行/表格单元格或局部投影 API |
| `mapping/projection.rs` | `source_at`、`drawn_at`、`aligned`，带身份/精确性的受检坐标；映射对象属全文投影 | 局部文本布局窗口与全文映射的受检适配，Source 行投影 |
| `navigation.rs`、`tests/navigation.rs` | 顶层标题/定义、TOC 与脚注双向来源/目标；嵌套/大小写已知有限边界 | UI 主动导航与回跳；不能把未索引目标或大小写空目标伪造成有效导航 |
| `crates/aline-buffer/src/line_index.rs`、`string_buffer.rs` | 已有 LF/原始行尾行索引和只读快照；具体访问通过所有者 | 窗口只读行访问，不向视图暴露可写 buffer |
| `crates/aline-app-core/src/i18n.rs` | `Translator::embedded(locale)`、`t`，rust-embed 双语 TOML | 新增菜单/状态/错误键及系统 locale 到已有 Locale 的装配 |
| 根 `Cargo.toml`、`Cargo.lock` | 八成员，ui 已有 source/buffer/markdown/edit/app-core/gpui 边；`gpui-pre =0.3.9`，关闭默认 features | 只复用现有依赖；原生 feature/实际选包是否改变受审图须核对 |

锁定 GPUI 包源码已静态确认以下机制，下面路径相对 `gpui-pre-0.3.9` 包根，不记录本机缓存位置。它们是 API 依据，不是 Aline 原生运行证据：

- `src/app.rs`：`Application::with_platform(Rc<dyn Platform>)`、`Application::run`、`App::open_window`、`App::prompt_for_paths(PathPromptOptions)`；路径选择返回 `oneshot::Receiver<Result<Option<Vec<PathBuf>>>>`，`None` 是取消。包内没有 `Application::new` 或默认原生 Platform 实现；原始归档与解包源码一致，校验和匹配 Cargo.lock，故不能将此当作仅缺工具/feature 的问题，详见下方原生前置门。
- `src/platform.rs`：`PathPromptOptions` 可限制文件/目录/多选；仍由宿主检查返回路径数量、扩展名及类型。
- `src/elements/list.rs`：`list(state, render_item)` 为变高列表；高度变化须通知 `ListState::splice`/`reset`。`uniform_list` 为等高列表，不可用其行高假定替代变高块。
- `src/elements/text.rs`：`TextLayout::index_for_position(Point<Pixels>) -> Result<usize, usize>`，结果为布局输入文本的字节索引；`Err` 表相邻/范围外候选，不能当精确命中。`LineLayout::closest_index_for_x` 也是布局候选。
- `src/text_system.rs` 与 `src/styled.rs`：`TextSystem::add_fonts(Vec<Cow<'static, [u8]>>)`、font family/fallback 样式；静态字体可注册，缺字实际回退及缓存刷新必须由 V-04 证明。
- `src/app.rs`、`src/app/context.rs`、`src/app/async_context.rs`：`background_executor`、`AppContext::background_spawn`，后台 future/结果需 `Send + 'static`；UI 只捕获最小不可变输入。包内未找到系统首选语言 API，不能假定 GPUI 会提供 locale。

原生前置门（已在 2026-11-02/03 结案）：建仓时 lock 没有 `gpui_platform` 或对应原生后端包，GPUI 的 `macos` feature 也不存在。当时保留 GPUI 0.3.9 和自有 unsafe forbid，要求先提交最小平台后端准入工件、核对与 GPUI 公共类型的兼容性及完整依赖/许可/目标，再获具体裁决后接线；裁决前不改 lock、加依赖或改指纹，窗口及依赖真实 UI 的任务阻塞，纯逻辑任务继续。该门已按下节 2026-11-02/03 的裁决关闭，不再是当前阻塞。系统 locale 来源须使用真实系统首选语言 API；不能假用 LANG 等同于 GUI 首选语言或硬选中文/英文当随系统完成。

2026-11-02/03 apply 调查与裁决更新：候选为同源发布 `gpui-pre-macos =0.3.9` + `font-kit`，系统首选语言使用 `sys-locale =0.3.2`。精确 package identity 与来源、target 闭包、许可证及用户逐项裁决见[原生准入候选方案](./native-admission-proposal.md)。task 4.1 的精确策略守卫、Cargo.lock 与七个 M1 图基线已重跑通过；例外仅限裁决列出的包身份/校验和/路径/目标。后续 Metal Toolchain 安装后，`cargo build -p aline --locked --offline` 已通过，macOS 单个空窗口实际显示并正常退出；4.2 完成。此证据只覆盖原生入口/空状态，不覆盖系统语言切换、文件选择或跨平台窗口（该限定描述 4.2 完成时的边界；随后 tasks 4.3/4.4 已分别取得中文界面启动与文件选择/取消/关闭的真实操作证据，跨平台窗口仍属未测）。

## Goals / Non-Goals

**Goals:** 在现有 DocCore/Parsed 真源链上建立可无窗口验证的只读会话，再装配 GPUI 单窗；UI 只做调度、布局、命中与呈现，新的语义/映射仍在既有纯引擎；让生命周期、受检坐标、虚拟化成本与平台证据分别可查。

**Non-Goals:** 不改既有输入组/IME/保存协议，不建立通用工作区/注册服务/事件总线，不维护第二份可写全文，不以复制现有构树输出或源码重扫在 UI 建另一套 Markdown 解释。完整主题/设置、第三方资产和平台安装不进入 M1。

## Decisions

### 1. 模块职责与唯一状态所有者

拟新增的模块名是落位方案，非已存在 API；按职责/生命周期拆分，不按行数机械拆文件。嵌套聚合用 `mod.rs`，叶文件具名；不开无消费者空目录。

| 所属/建议落位 | 唯一持有的数据与输入输出 | 提交入口/禁止事项 |
|---|---|---|
| `app/src/main.rs` | 启动参数中的运行环境与 UI 启动调用；app 保持薄入口 | 调用 ui 的一个启动入口；不复制打开、解析或文档状态机；M1 不处理命令行文件/IPC |
| `aline-ui/src/window/` | 根实体、打开/切视图/大纲/关闭 actions、焦点和窗口布局；持有一个可选文档会话 | 统一意图转发到只读会话；仅 UI 原生窗口创建一次；无 tabs/workspace 状态 |
| `aline-ui/src/document/readonly_session.rs` | 存储基线 + 一个 DocCore + 当前 Parsed + 文档生命周期、读取/解析请求任务；只读加载候选与当前会话分开 | `request_open`/`cancel_open`/`accept_open`/`close_document` 等拟议私有入口串行发布；不能暴露 Buffer 可写句柄 |
| 现有 `document/doc_core` | 唯一源码/历史/源码选区/模式与文档身份；新增只读能力分支 | 只读构造/导航/切模式/当前结果复验入口；编辑模式保持既有入口与语义 |
| `aline-markdown` 现有 build/parse + 按需 `presentation.rs` | 唯一解析事件链产生不可变结构/样式与来源，包含列表层级、Alerts、代码信息/行、表格行列 | 不输出颜色/GPUI；不重新序列化；保留预算、保护/Gap/导航账本 |
| `aline-markdown/mapping/` | 片段窗口、Source 行恒等投影、局部 drawn ↔ 全文 drawn/src 受检适配 | 所有局部/全文加减只在本层，调用方只用带身份类型 |
| `aline-ui/src/render/`、`source_view/` | 每模式虚拟列表、可见文本布局、受限高度缓存和几何；借用源快照与语义 | 不解析、不持有可写全文/历史；只发导航意图；主滚动条位置共同由窗口内容区控制 |
| `aline-ui/src/fonts/` | 原生字体注册/有效字体代次与失败反馈 | 启动/实验显式加载，发布字体环境代次；不在 render 读字体文件；不建主题安装服务 |
| `aline-app-core` 的 catalog/最小配置纯模型 | locale/正文与代码换行、>4 MiB Source 阈值等类型化启动配置 | 显式参数，不加全局 locale 或配置文件服务；无设置页/持久化 |

依赖沿 CODE_STANDARDS §2：app→ui；ui→source/markdown/buffer/edit/app-core/既有 gpui；markdown→buffer 与既有纯依赖。plugin 不参加 M1，现有 edit 边不因此变成窗口可写入口。五引擎继续无 GPUI/线程/全局状态/unwrap，生产无 expect/panic/unreachable/发布 assert。原生启动放 ui 以复用已存在 GPUI 依赖，不为一条启动调用新增 app→gpui 直接边。

问题是独立生命周期与副作用的混杂，最简单方案是几个具体 owner 和函数。仅在真实文件选择/读取故障与任务乱序测试边界保留小适配接口；不建 Factory/Repository/Service 三层，不新增 crate。代价是需显式传配置/预算和处理错误；收益由无头公开边界测试及调用图审查证明。约 500 行只触发职责复核，记录保留/拆分理由，不变成门禁阈值。

### 2. 最小打开动作与候选发布

窗口按钮、文件菜单和 Cmd/Ctrl+O 共享同一 action。使用 GPUI `prompt_for_paths`，设文件可选、目录不可选、多选关闭；多路径等异常仍拒绝，不悄悄取第一个。不加 rfd、原生 FFI/unsafe 或 WebView。M1 不开启 URL/图片/目录入口。

状态为 `NoDocument | Ready(session)` 加一个可选、带代次的打开请求（Selecting/Reading/Parsing），窗口关闭有独立存活标记。当前 Ready 不因选择或读取开始而被清空。文件读取在 UI 后台任务中完成，先检查文件类型/扩展名/输入预算；检查 metadata 后仍进行受限读取，不能 `read_to_end` 无界增长。读取 bytes 经 source::decode 后建立存储基线与只读 DocCore，一次生成当前快照并解析。成功后先验请求/窗口代次，再替换整个 session；旧 session 关闭并释放其派生资格。

| 事件 | 结算 |
|---|---|
| 选择取消/取消读取 | 作废该请求，无错误/成功提示，保留当前文档/位置，归还焦点 |
| 新请求 B 到达 | 取消旧 A 的资格、合作检查；最新有效意图优先，队列有界，不并行积累任意任务 |
| I/O/解码/构造失败 | 类型化错误→UI 翻译；保留当前 Ready，首次失败回可操作无文档状态 |
| 解析预算回退且 source 完整 | 候选可带明确诊断显示安全原文；不能把回退算基础渲染完成；解析输入硬超限仍拒绝 |
| 关闭文档/窗口 | 推进生命周期与请求代次、取消任务/订阅，释放会话；晚到结果不复活 |

不存在文档写盘任务，所以没有不可丢弃的保存回执。正在执行的 OS 读取未必能立刻取消：以有限字节、有限并发、逐段检查合作取消约束资源；关闭后保持最小任务结算数据直至实际结束。metadata/时间戳不能证明磁盘未变化；M1 只承诺本次完整成功读取快照及不写源文件，不承诺持续跟踪磁盘或外改重载（M2）。

### 3. 不开块的 DocCore 及公共边界

拟新增只读构造入口，直接建立 `open=None` 的状态，并以内部能力枚举区分 Editing/Readonly。保留现有 `DocCore::new` 的 M0 行为，禁止用“先 new 再 leave”临时开块。只读会话所有修改入口（开块、普通/组合输入、命令、reload、Undo/Redo 等）在任何状态变更前拒绝；公共测试覆盖入口族，避免只靠 UI 未绑定键位。

文档级导航受检接受所属快照的 `SrcSelection`，由核心更新唯一选区；模式也只由核心持有。只读切模式不调用编辑输入组提交，推进派生/视图资格以防模式往返恢复旧结果，但不推进文本 revision 或历史。提供只读行查询/当前快照，不复制第二份持久源码选区到子视图；滚动锚点属于视图状态，可与核心选区不同，但有明确用途/身份，不是另一个可编辑选区。

结果采纳拆为两类目标：文档级 Parsed 看只读文档存活与完整依赖键；编辑投影继续要求具体编辑块有效。不得全局删除原有 `ClosedTarget` 检查。生命周期用 session 存活代次，输入键包括 DocumentId、WorkingVersion、SnapshotId、解析配置和 request；布局再加 view/layout generation。`SnapshotId` 不是工作版本或内容哈希，不能单独判有效。

仅跨 crate 的真实 app→ui 启动及 markdown→ui 数据需要 `pub`；其余尽量私有/pub(super)。新增公开错误枚举、构造/返回值带 rustdoc，公共 API 守卫与文档消费者同步；不为测试开放可写字段。先让无窗口用例证明 byte/revision/history/拒绝原子性，再接窗口。

### 4. 补足结构投影，保持解析与映射单一来源

现有 pulldown-cmark 事件构建链增加基础展示所必需的不可变节点/样式信息：标题等级、列表/引用层级、列表起始序号/任务状态、表格单元格与对齐、Alerts 类型、代码正文来源/语言及逻辑行边界。全部范围绑定同一源快照或全文绘制投影，字段私有/受检访问；结构节点数、深度、单元格与输出文字纳入既有预算扩展。不能在 UI 再解析 Markdown，也不能只给 `BlockKind::Table` 就声称表格展示已完成。

渲染树可作为 Parsed 的只读附属数据；它不是第二份源码，不从 AST 序列化。HTML 消费已有样式/保护与投影文字，Markdown 行内语义使用同一事件链；新增展示数据的构建同步更新 raw fallback 与取消路径。复用差分覆盖新结构/样式，不能只依赖生产 `same_block` 的 kind/text 比较而把旧绝对范围挂回新快照。

最小局部布局适配是映射模块持有一个受检“投影窗口”及其到父投影的范围关系。布局输入必须与窗口的 drawn 文本逐字节相同；窗口返回自己的 DrawnOffset，再由映射层经父关系返回 `Mapped<SrcOffset>`，保留 precision/source/affinity。Source 行也建立带源码来源的恒等投影；CRLF 两字节、孤立 CR、tab 展开/代码缩进去除若影响绘制，均由映射层表达，不由 UI 减行首或块首。窗口类型/名称实施时可调整，契约在 engine/markdown delta。

此方案比给 UI 暴露裸 range 然后手算多一个受检窗口对象，但真正解决局部文本排版与全文映射错配。测试分别比较全文/局部来源和精确性，不对实体内部所有字节要求双射。`NavigationIndex` 的顶层标题/定义边界保持；不顺带承诺所有嵌套目标。已由解析器识别配对的脚注必须获得同语义的有效导航目标，不能沿用字节精确匹配导致的大小写空目标；在同一解析链以与解析器一致的标签等价关系配对，保留重复目标的源码顺序，不新增依赖。完整导航 requirement 的 MODIFIED delta 明示该修复，现有现状测试改为失败回归和独立预期。确实未定义/未索引的目标仍给明确状态和源码定位，不能与解析器已配对却漏索引的缺陷混为正常失败。

### 5. 虚拟化、滚动和命中

Visual 用锁定 GPUI `list`/`ListState` 的变高测量；估算只用于未测项，测量结果入带环境键的缓存并按框架 reset/splice 使高度失效。Source 以逻辑行作为虚拟 item；正文/源码默认软换行，所以一个逻辑行可有多视觉行且高度变化，同样优先用变高 `list`。仅在显式关闭换行且全部行高固定的配置中才可评估 `uniform_list`，不是 M1 必需优化。预取、布局行/字节量和缓存均有注入上限，数值由 V-05/V-06 测试记录，不复制旧 overdraw 参数。

两视图共用中央列宽、同一外侧主纵向滚动条槽位，任何时刻只有当前视图的纵向列表活动；不在 Source 再嵌一根纵向滚动条。视图滚动位置记为源锚点、affinity 和相对视口位移；模式/宽度/字体改变先映射源锚点，再让目标 item 被 list 揭示，测量完成后做一次受限校正，不沿用旧像素高度或每帧 notify 重测。缓存键至少含文档/快照/解析配置/投影窗口、内容宽度、字体代次、缩放、换行和有效排版参数；元素 ID 绑定会话和来源节点，索引仅是列表游标。

实际命中链：像素事件→当前 text layout 的 `index_for_position`→受检局部 DrawnOffset→映射窗口→全文 `source_at`/`aligned`→核心只读导航。`Err(index)` 只作边缘候选，须校验/标精确性；布局或身份过期时拒绝并重测。只读阶段没有平台 UTF-16 输入回调，不注册可编辑 handler；如布局 API 的候选不满足字素边界，通过已有 Unicode17 投影查询对齐，超预算拒绝，不猜边界。

巨段/超长行/巨表即使只有一个 item 也要受限：按可见逻辑行/单元片段布局并保留原文入口，先检查布局字节/节点预算再 shape；不允许同步 shape 一个无界 item。安全回退保全内容，但 1 MiB 典型可视化基线仍必须真实成功；不能通过调低阈值把该基线全部退 Source。

### 6. 内容、导航与普通代码的阶段呈现

渲染器按封闭语义枚举穷尽分发，基础列表/表格/Alerts 要有可辨结构。脚注/目录/Front matter 用独立中性灰色表面与只读说明，脚注引用保持独立导航范围，不能把含引用的整段伪装成保护块。HTML 无浏览器/CSS/脚本执行路径；过滤标签按字面、注释只展示隐藏，零可见内容保留源码定位入口。

大纲由 `NavigationIndex` 的 heading_level 建层级，条目目标保留源范围；仅用户点击/键盘激活后滚动，不订阅正文滚动做反向高亮。目录/脚注副本必须通过引擎同源窗口或显式合成来源生成，不复制显示文字再按搜索回找。未定义/多目标按冻结样例呈现明确状态；不将当前字节精确标签匹配的不一致写成已解决。

普通代码围栏/缩进同一 renderer，行号取代码正文逻辑行，不含围栏、不随软折行增加；空行与末尾换行由独立期望区分。语言为只读标识；不建语言搜索/修改菜单。M1 的必需代码操作是阅读、行号和可访问横滚，复制控件本轮不新增；这不取消 UI §7.2 后续代码复制及反馈要求。任务框仅展示不可切换。Source 的核心 Markdown 标记着色与编程语言高亮分开，前者消费已有/新增语义样式，后者仍在 M4。

公式/Mermaid 保留原文；图片显示引用/占位，不加载图片、不请求远程资源，不建图片 tab。V-03 准备格式/失败/协议夹具但不执行未知协议；外部链接打开与完整资源接线留后续，M1 点击链接文字仍以源码定位为默认。基础语法回退不能算可视化完成。

### 7. 字体、本地化与主窗基准

启动由经前置门核对的系统首选语言适配输出映射到现有 Locale，其他语言回退英文；GPUI 核心无该 API，检测来源/失败回退必须实证，未具备时标阻塞而不能宣称随系统已完成（该准入要求已于 tasks 4.3 用真实 `zh-Hans-CN` 系统首选语言启动验证，后续人工取证按决定 §24 只做中文，英文启动记录保留为裁决前历史）。双语 catalog 的键集合/占位符自动校验继续保留，可注入显式 Locale 做覆盖测试，但不替代真实默认语言检测。所有标题/菜单/按钮/错误/加载/只读/降级/无文档文字走 `Translator::t`，catalog key/插值测试继续双语一致。文档正文/文件名不翻译。完整语言覆盖设置和即时切换服务属于后续，不造无效开关。2026-11-03 用户裁决界面语言人工/原生取证只做中文（`docs/decisions.md` §24）：启动语言观测、窗口文案与键盘矩阵只核对中文，不切换系统首选语言复验英文；英文实现由 `Translator::embedded` 的键集合/占位符校验与缺键/插值漂移反例自动覆盖，不降级或删除该自动检查。

M1 提供受保护默认 GitHub 语义配色的最小内容样式，随系统亮暗的启动环境输入保留；不实现四主题三模式选择、主题安装或配置持久化。排版/宽度/侧栏数值标实验候选，不从原型抄冻结值；内容列宽、留白与正文度量的唯一取数与纯计算见 §9.3，取值仍标实验候选。默认与字体配置显式从 UI 启动配置传入，app-core 无 GPUI 类型。

V-04 通过同一字体适配边界验证系统字体、font_fallbacks 与有授权的静态 TTF/OTF 的 `add_fonts`：正常、缺失字族、损坏字体、缺汉字、重注册/代次改变后的重排与命中。静态字体实验不要求建设主题导入；资源仅使用已确认授权的独立夹具，无法取得时记阻塞，不下载来历不明字体。记录 OS/字族/回退观测方法、中文/拉丁/标点/emoji 基线、宽度和像素命中前后；API 成功不能单独证明指定字体被使用。注册失败/缺字回退异常必须可见，不静默宣称激活成功。

V-09 样本包含无文档/加载/正常/读取失败/取消/解析回退、窄窗/缩放、长文件名/英文、侧栏与主内容、焦点归还、滚动/跳转、普通代码长行。文件菜单只接本阶段真实动作，V1 后续写动作置灰并有只读原因或按阶段隐藏，不能显示假成功。设置/完整工具栏、tabs/文件树不为“看起来完整”先建；§9 只建区域骨架与当前阶段可用的编辑模式组，不建完整工具栏、标签栏、侧栏与文件树。系统关闭/显式关闭文档汇到同一只读关闭意图；退出无保存任务，不复用 M2 未保存对话流程。

### 8. 验证顺序、证据与治理复用

先测试新结构/映射/只读生命周期与独立夹具，再做无内容单窗 macOS 冒烟，再接内容与真实命中/字体/长文。纯单测、GPUI 交互/布局与真实平台观测分别记录。新增测试消费者落 `crates/aline-markdown/tests/`、`crates/aline-ui/tests/`；稳定夹具在对应 `tests/fixtures/`；复跑入口在 `scripts/`，不新增测试库。

V-05 使用 Release 与 32/128 KiB、1/4 MiB、>4 MiB、许多短块/单巨块/巨表/密集实体等固定语料；分别测读取/解码、快照、解析/映射、首布局、主线程采纳、首次呈现、模式切换与滚动，包含等待排队。首轮每个可自动测的样本/冷热条件至少 20 次，报告样本数及 nearest-rank P50/P95、最长主线程停顿、峰值内存和虚拟 item 数；冷/热如何清理缓存需可重现，不声称完全排空 OS 缓存。真实滚动另记录从头到尾及往返、PageUp/PageDown/Home/End 或菜单等价入口与可响应动作；不能只有微基准或平均 FPS。20 次是首轮采样方案，不是新增发布性能门槛。

不开编辑/IME/查找/资产后端，对应时延项标不适用 M1、未测留后续，不能填零。>4 MiB Source 是已确认默认策略，不是硬上限；硬预算与 layout chunk 数值通过 V-05/V-06 参数实验确定并记录，不能无限输入或降目标。

继续跑完整 Cargo 门禁与既有 workspace/工程/lint/安全/公共 API/catalog 检查。若原生 features 导致实际图变化，登记精确差异并独立审计再接线，禁止自动置 reviewed 或扩禁用例外；无合规现有组合则暂停受影响任务，提出最小准入问题。新/改守卫只扩既有 `workspace/engineering-gates` 能力，必须正例/最小反例。M1 已取得的部分无头与原生证据见 `tasks.md`：4.2/4.4 的真实空窗与文件选择、5.1 的 1 MiB 两视图滚动、11.3 的品牌区截图均已实测，但窄窗/统计序列/键盘焦点等真实窗口分项受会话锁定阻塞，V-05/V-09 整项与阶段出口仍未完成，不写成已验证。

原始输出按 `logs/validation/m1-single-window-readonly/<run-id>/` 的分项子目录保存，使用 `scripts/evidence_paths.py` 归一化；不得把本机绝对路径写进工件/证据。独立输入与预期可入库，临时副本/截图/计时数据/Agent 审查原文不入库。最终摘要回填原验证计划和架构，保留 M0 豁免缺口及 Windows 10/Intel Mac/Linux 未测；`--all-targets` 不证明跨平台。详细执行命令与每组关联在 tasks。

#### 8.1 真实采样入口与观测边界（2026-11-09 补充）

`scripts/sample_m1_performance.py` 负责真实构建、逐次启动、收据与矩阵观察摘要；`collect_m1_readonly.py` 只做格式和身份核对，`run_m1_readonly.py` 继续承担严格验收，不因观察摘要存在而放宽。问题是单个 JSON 声明无法证明实际进程、源工件或计时位置；采用具体 launcher + 每次一个原生子进程的隔离 Python worker，保存 Cargo JSON executable/profile、commit、全部源码文件哈希清单、二进制/spec/fixture/原始输出哈希、原生 pid 与起止/退出回执。没有新增依赖或通用服务抽象。

- 后台三段、主线程采纳、首 render→子节点 prepaint，以及后台提交排队/结果回到主线程，使用同一 `StageTimer` 分别捕获墙钟起止、真实 `Instant` 起止及墙钟捕获开销；同一探针原点导出单调起止偏移并精确核对耗时。拒绝缺失/不一致的单调边界、墙钟倒退和过大捕获开销。CLOCK_REALTIME 与 Instant 可能有不同分辨率或速率，不能强制两者纳秒差相等：本机实测前者 1 µs、后者约 42 ns，分辨率随元数据记录，不通过加猜测阈值修复。后台起点不能从稍后的采纳时刻回推。排队时长保留为具名原始诊断。
- 平台 draw 提交后的 effect 仅输出 `first_draw_submission_ns` 诊断，不能填 `first_present`。模式切换/滚动当前测量口径为程序化真实动作请求→draw 提交回调，明确不证明屏幕呈现或人工鼠标/键盘操作。事件循环间隔仅作诊断，不填精确 `main_thread_stall_ns`。
- RSS 由只启动一个原生子进程的 worker 在等待退出后读取 `getrusage(RUSAGE_CHILDREN).ru_maxrss`；macOS 单位为字节，绑定该 pid。指标作用域是整个原生进程生命周期高水位，热条件含预热，不能写成逐阶段峰值；零值拒绝。所有大文件证据导出延后到受测帧/动作结束，避免首帧前同步证据 I/O 污染时间线。
- 冷条件每次新进程且不预热同一文档，正常空窗与字体启动仍会触及 GPUI 字形/管线，不声明这些启动缓存仍为空；热条件先在同一进程完成同文件打开、draw 提交、滚动、双视图往返、关闭，再测重开。两者均不清空 OS 文件缓存；事前哈希本身会读文件，不能宣称 OS 冷缓存。协议文字与每次 spec/收据共同保存，便于复跑。
- 尺寸组使用确定性重复短块；大于 4 MiB 使用多字节中文短块，以真实检查默认 Source 回退，避免 ASCII 映射点预算超限让该维度只有拒绝而无可读样本。四类复杂度仍独立采样，不替换原有敌意输入/安全回退样例。生成器和每次运行记录尺寸/哈希。

缺少真实首次屏幕呈现或精确主线程停顿时，矩阵观察摘要 `passed` 保持 false，严格性能任务仍不勾选；可用分段/排队、RSS 与冷热观察仅作为已执行分项。上述协议不改变 §8 的样本数、完整指标、真实操作或 M1 出口要求。

#### 8.2 真实只读回调的补充证据（2026-11-09）

显式 `ALINE_M1_READONLY_EVIDENCE_DIR` 只允许已存在的 logs/validation 内目录，且拒绝与自动 probe 同启。`readonly_evidence` 由窗口独占当前文档身份及取证生命周期，Rc 仅把真实 ListScrollEvent 反馈到同一状态；真正成功的 HitTarget 回调、目录/脚注激活和实际模式改变分别记录，程序化 probe 不参与。滚动按 wheel/key 来源保留，不把按键请求、像素命中或主动导航相互冒充。

打开后保存实际磁盘基线和核心源码；关闭前冻结当前核心源码、revision、草稿/组合、undo/redo/历史，关闭会话后后台重新读取磁盘并导出独立 bytes。窗口热路径只记小型动作，编码与写盘不在渲染或命中路径；只创建新文件、不覆盖夹具。辅助入口 `capture_m1_readonly_actions.py` 验证严格状态字段、非负计数和已知动作，事件溢出、无效字段及缺步骤均拒绝。它是补充观测，不代替标准汇总器的启动身份核对。

用户明确每个原始保真样本都必须完整七步，拒绝配对夹具替代；因此不引入配对的替代样本，每个原样本自身须具备主动导航入口。2026-11-11 用户裁决「修改原样本使其可跳转」：保留每个样本各自的保真特征（BOM / 混合行尾 / 孤立 CR / 空白 / 无尾换行），在同一文件内补入 `# 标题` 与 `[toc]` 作为跳转目标，样本数量与保真维度不变，字节边界特征不被削弱。首轮五样本取得实际字节/状态快照且均不变，全部缺主动跳转，所以 7.3 仍失败。

### 9. 主窗口区域骨架、工具栏行与状态栏统计（2026-11-03 并入）

2026-11-03 用户裁决：原生 UI 实现按 `docs/prototype/legacy/` 的 HTML/CSS 对照布局、控件邻接与图标语义（UI §4.1、`docs/decisions.md` §22）。原独立变更 `m1-main-window-skeleton` 已并入本变更并删除，其任务编号为 9～15；本节规定该范围，其余既有契约不变。

原型骨架（`prototype.css` 的 `#app`）：标题栏 32px 在 `#app` 之外；`#app` 为四行两列网格 `"toolbar toolbar" / "sidebar tabs" / "sidebar editor" / "status status"`，行高 44 / 36 / `minmax(0,1fr)` / 28px，列宽 250px + `minmax(0,1fr)`；`.editor` 为 `max-width:880px; padding:40px 56px 100px; font-size:16px; line-height:1.75`，`box-sizing:border-box`；窄窗档位 `@media(max-width:900px)` → `30px 28px 80px`、`@media(max-width:600px)` → `24px 20px 60px`。结构照此实现；像素、字号与行高是实验候选（§7 与 UI §3.3，待 V-09）。

#### 9.1 模块职责与唯一状态所有者

本节模块名按当前实现落位（`native_window`），与 §1 的拟议落位名对应，实施以本节为准。

| 模块 | 职责 | 状态所有者 | 允许依赖 | 公共边界 |
|---|---|---|---|---|
| `document/statistics.rs`（新增） | 从一份源码文本算出统一统计（字符数、行数），给出可复核的口径定义 | 无（纯函数） | 无（不依赖 GPUI、线程/全局） | `pub(crate)`，不导出到 `lib.rs` |
| `native_window/geometry.rs`（新增） | 区域高度、内容列宽/内边距、窄窗档位、字号行高的**唯一**来源；`content_metrics` 纯计算 | 无（纯数值） | 无（不依赖 GPUI） | `pub(super)`，只被 `native_window` 子树消费 |
| `native_window/shell.rs`（新增） | 工具栏行、内容区、状态栏行的元素装配与表面配色；几何一律取自 `geometry.rs` | 无（元素构造） | GPUI、`geometry.rs`、catalog 文案 | `pub(super)`，只被 `native_window.rs` 消费 |
| `native_window/readonly_reader.rs`（改） | 只读列表的排版与虚拟化；内容列几何**由 `geometry.rs` 提供** | 列表状态、高度缓存 | GPUI、映射层、`geometry.rs` | `pub(super)`（不变） |
| `native_window.rs`（改） | 窗口状态所有者：区域装配、工具栏/状态栏交互、统计的调度与采纳 | 统计结果与其绑定的文档身份、统计任务 | shell、geometry、statistics、GPUI | `pub(crate)` 的窗口入口（不变） |

统计是**文档派生数据**（知道源码，不知道窗口），骨架几何是**窗口派生数据**（知道窗口，不知道文档），两者生命周期与变化原因不同；放进同一模块会让"排版取值"和"内容口径"互相污染。统计函数是纯的、无 GPUI，因此放在 `document/` 下并被无头测试直接覆盖。`geometry.rs` 不含任何 GPUI 类型，其无头测试放在同文件的 `#[cfg(test)] mod tests` 内（safety 门禁禁止 `#[path]` 引入源模块，也不允许 `tests/` 根反向解析 `src`）；`shell.rs` 只做元素构造，不持有第二套数值。统计保持 `pub(crate)`，不新增跨 crate 公开面（CODE_STANDARDS §3.2）。

#### 9.2 骨架：flex 三段，不做网格模拟

原型用 CSS Grid 同时表达行与列（侧栏跨标签栏与内容区两行）。GPUI 无 CSS Grid；用 `flex_col` 表达三段垂直结构，内容区用一个 `flex_row` 容器表达"未来可插入侧栏列"。采用具体类型 + 普通函数（`div().flex().flex_col()`），不引入 Grid 抽象、不用绝对定位、不自绘标题栏。原型的"侧栏跨两行"在 M1 无法表达——但 M1 没有侧栏，M5 引入侧栏时按 `flex_row` + 侧栏列 `flex_col`（标签栏 + 内容）组合即可，不需要重排工具栏行与状态栏行。

#### 9.3 内容列几何与窄窗档位

原型 `.editor` 是 `border-box`，因此 880px 是**含内边距**的列宽，文本宽 = 880 − 2×56 = 768px。采用纯函数 `content_metrics(window_width, area_width) -> ContentMetrics`：

- 档位按**窗口宽度**选择（原型媒体查询作用于视口；M1 无侧栏，窗口宽即内容区宽；M5 有侧栏时媒体查询仍按窗口宽）：`> 900` → `56 / 40 / 100`；`≤ 900` → `28 / 30 / 80`；`≤ 600` → `20 / 24 / 60`（水平/上/下）。
- 列宽 = `min(max(area_width − 滚动条宽, 0), 880)`；内容区不小于滚动条槽位时，列宽与滚动条槽位之和不得超过内容区宽度，窄到放不下时先收内边距，仍放不下则列宽退到可用宽度（不横向溢出优先于最小文本宽）；内容区窄于滚动条槽位属不可用退化尺寸，列宽为 0，由平台窗口尺寸下限兜底；文本宽 = 列宽 − 2×水平内边距。
- 正文 16px / 行高 28px（16 × 1.75）。源码视图沿用同字号与行高，只换等宽字体族。

列宽算式放在 `geometry.rs` 而不是 reader：reader 当前从窗口视口反推宽度，M5 出现侧栏列后视口宽不再等于内容区宽；由 `geometry::content_metrics` 依据实际区域宽算出文本宽再交给 reader，reader 只消费"我的文本有多宽"，两视图因此天然共用同一列宽。`ReaderSurface::prepare` 的签名相应从"读窗口视口"改为"接受显式内容几何"。

垂直内边距：原型的 40px 上 / 100px 下是 `.editor` 的内边距，落在首段与末段内容上。虚拟列表里没有"整个 editor 盒子"，因此按行号判定：首行加 `padding_top`，末行加 `padding_bottom`。行高缓存记录的是**实测高度**（含内边距），所以加内边距不会与高度缓存冲突。

#### 9.4 工具栏行：只放得起的组，范围外不占位

已确认 §10.5 的分组顺序是"侧栏 → 编辑模式 → 行内格式 → 段落/插入 → 导出 → 设置"。M1 能兑现的只有**编辑模式**（可视化 / 源码切换），其余组的能力分属 M2～M5。采用：只渲染编辑模式组，其余分组不出现——不是置灰占位。编辑模式组只在**文档已打开时**出现：无文档时没有可切换的视图，渲染它只会得到无反应的按钮。当前模式以原型的 `.icon-btn.active` 语义呈现（强调色弱背景 + 强调色文字），非当前模式为普通按钮。控件用**文字标签**而非原型 SVG：原型图标缺少许可证明（PROTO-03），不得拷入运行时。代价是 M1 工具栏看起来空，这是阶段能力限制的真实反映，不通过增加范围外入口或占位来掩盖。

工具栏行左侧同时保留**应用品牌区**：图标取 `docs/assets/app-icon/aline.png`（PRD §6.1、UI §2 已确定的应用内品牌展示资源），与产品名 `Aline` 一起显示，无文档时也可见。品牌区不是命令组，不改变上述分组顺序，也不替代打开/关闭入口。原型 `.titlebar` 内的图标属于浏览器模拟；原生 macOS 标题栏不显示应用图标，窗口内的品牌呈现以工具栏品牌区为准。原型 `icons.js` 的 SVG 仍因 PROTO-03 不进入运行时；macOS 应用包/Dock 图标由 `aline.icns` 在打包阶段接入，Windows 使用 `aline.ico`，Linux 使用 `aline.png`，M1 不声称打包图标已接入。

#### 9.5 状态栏统计：口径、异步、身份

口径（候选，待 V-07 核对）：

- 字符数 = 源码中**非空白 Unicode 标量值**的个数（Rust 侧用 `char::is_whitespace`）。原型当前有两套脚本：`prototype.js` 对渲染后的正文计数，`interactions.js` 的 `stats()` 对 `doc.source` 计数；本口径与后者同方向，使两种只读视图得到同一数值。它不是逐字符等价——JavaScript `\s` 与 `char::is_whitespace` 在 U+FEFF、U+0085 等字符上边界不同，这些样例与 emoji ZWJ、组合字符一并归 V-07 核对。按标量值计，不按字素簇计。
- 行数 = 按 LF 分行的行数（`max(1, LF 数 + 1)`），与 `aline_buffer::LineIndex::line_count()` 同口径：孤立 CR 不是换行，CRLF 不额外计行，末尾 LF 产生一个空行。

计算位置：`document/statistics.rs` 是纯函数；调用发生在**后台执行器**（`cx.background_executor()`），不在 render 路径（CODE_STANDARDS §2.2 第 6 条）。输入是当前文档基线解码后的源码文本。身份绑定：统计结果连同其 `DocumentId` 一起回到窗口；只有与当前文档身份相同的结果才被采纳并显示；文档被替换或关闭时统计立即作废。

未计算 vs 零值：窗口持有**已渲染的两条标签**（字符数、行数）的 `Option`：`None` 显示占位（catalog 键 `status.pending`），`Some` 显示真实数字——零字符文档显示 0，不显示占位。标签在结果到达时用 `Translator::t_with` 渲染一次并缓存，render 路径只做取用，不再做字符串格式化。无文档状态不显示任何文档统计（UI §4 明确"不得显示可编辑的假文档数据"），因此不显示"0 字 / 1 行"。原型的"行 X, 列 Y"需要字符列计算，UI §5.4 明确不能用 UTF-8 字节数冒充字符列，M1 不显示。

#### 9.6 文案与 catalog

新增 catalog 键（zh-CN/en 同键同占位符）：状态栏的字符数/行数（`{count}` 插值）、模式标签（复用已有 `action.visual_view` / `action.source_view`）、统计占位符。`Translator::t_with` 要求占位符集合与实参精确相等，双语 catalog 在 `Translator::embedded` 构造时校验键集合与占位符集合一致——新增键任一侧缺失都会在启动时失败，不会静默显示原始键。`Translator` 不实现 `Clone`，因此 `NativeWindowCopy` 改为**持有**翻译器（按值接收）；启动路径在移交前先取出自己仍需使用的两条 `startup.*` 文案。

#### 9.7 失败、取消、过期与平台影响

| 情形 | 行为 |
|---|---|
| 统计任务在文档替换后返回 | 按 `DocumentId` 拒绝，不显示旧值；当前文档的统计重新调度 |
| 统计任务在窗口关闭后返回 | 随实体生命周期丢弃，不触碰已释放状态 |
| 统计计算本身无失败路径 | 纯函数对任意 UTF-8 文本都产出结果；不新增错误枚举 |
| 布局预算不足 | 沿用既有 `reading.layout_budget` 反馈路径，本范围不新增失败分支 |
| 平台 | 窗口装饰用平台原生实现（macOS 现有后端）；本范围不含平台特定代码，Windows/Linux 仍为"未启用原生后端" |

#### 9.8 验证顺序与证据

1. **纯机制先行**（无头）：`content_metrics` 的档位与列宽算式；`DocumentStatistics::compute` 的字符数/行数口径（空文本、纯空白、中英文、标点、emoji ZWJ、组合字符、CRLF、孤立 CR、无末尾换行）。
2. **i18n**：新增键在 zh-CN/en 双侧存在且占位符一致（`Translator::embedded` 的既有校验覆盖；补一条针对新键的断言）。
3. **既有门禁**：`cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`。
4. **真实窗口**：macOS 实测截图（无文档态、阅读态、窄窗），核对三段骨架、列宽共用、状态栏数值与"未计算"占位。
5. **回填**：`docs/ui-spec.md` §4.1 实现状态、`docs/v1-validation-plan.md` V-09 分项、`docs/v1-architecture.md` §13；原始输出留 `logs/validation/`，与既有证据同一根目录分项存放，不倒改旧证据。

### 10. 并入的 QMD-01 工程治理范围（2026-11-06）

2026-11-06 用户决定把治理 change `qmd-01-engineering-practices` 并入本变更并删除，沿用 2026-11-03 并入 `m1-main-window-skeleton` 的先例。原 change 是 2026-11-05 决定 QMD-01 的治理入口：方法正文已回写现行 `docs/`，但当时缺少对应 OpenSpec 工件；它只追认文档与方法边界，不实施 W1～W7。

#### 10.1 合并结构

| 原工件 | 合并后落点 | 处理 |
|---|---|---|
| `specs/engineering-governance/spec.md`（6 条 ADDED 要求、17 个场景） | 本变更 `specs/engineering-governance/spec.md` | 整体移入，正文不改；仍为独立 capability delta，不与 M1 的 `workspace/engineering-gates` 混写 |
| proposal / design 的方法来源与效力边界 | 本 `proposal.md` 的合并说明与本节 | 压缩为范围、效力与验收边界，不复制方法正文 |
| tasks 1～5（15 项，全部完成）与执行记录 | 本 `tasks.md` 第 16 组 | 合并为已完成组；第 17 组承接范围外的 W 项门 |
| 原 `.openspec.yaml`（created 2026-11-06） | 删除 | 本变更沿用 created 2026-11-02 |

各 capability 的 delta 独立成文件：产品实现侧为 `ui/*`、`engine/*`、`workspace/engineering-gates`，治理侧为 `engineering-governance`。归档时分别同步到对应的主规格，互不覆盖；`engineering-governance` 的 6 条要求均为 ADDED，不修改既有 12 条主规格要求。

#### 10.2 决策：一个 change 同时承载产品实现与治理 delta

QMD-01 的方法采纳是一次决定，来源、效力和边界共享，单独建 change 是为了补齐"方法已回写规范、治理入口缺失"的缺口；并入后该缺口仍由本 change 内的独立 delta 承载，不需要第二份背景与边界。代价是审阅面较宽、归档需等 M1 实现与平台证据齐备，因此治理 delta 同步到主规格也被推迟；收益是 `changes/` 只有一个进行中变更，W 项的立项门与本变更的阶段出口在同一处可查。W0～W7 仍视为工作分解与建议顺序，实施前逐项另建可独立验收的 change，不因并入而被视为已批准。

#### 10.3 边界与验证

- 方法正文继续留在 `docs/` 对应章节与决定 §25，delta 只写工程治理必须满足的结果与证据边界，避免同一要求出现两个措辞版本；研究文档只作证据索引。
- 本范围没有运行时模块、状态所有者、依赖、feature、夹具或平台证据；校验手段是 `openspec validate`、链接/锚点检查、`git diff --check` 与 `git diff --stat`。原 change 曾记录 9 passed / 0 failed、`checked_links=643`、`LINKS_OK`；合并后重新执行，全量项因治理 change 删除由 9 变 8，链接/锚点检查按现行规范/文档范围重跑为 `checked_links=1033`、`LINKS_OK`，具体命令与结果见 `tasks.md` 第 16 组。未改代码、Cargo 或主规格。
- 未新增自动门禁，不把未实施方法写成已启用；原始输出留在未跟踪的 `logs/validation/`。
- 界面语言人工/原生取证只做中文（决定 §24）；zh-CN/en 双语 catalog 的键集合/占位符校验与缺键/漂移反例自动检查保留。本合并既不新增英文界面取证任务，也不降级该自动检查。

## Risks / Trade-offs

- [治理 delta 与 M1 实现共用归档门] → 治理 delta 已具备同步条件但须等 M1 实现与平台证据齐备才随本 change 归档；期间 `engineering-governance` 主规格不含这 6 条要求，查阅以本 change 的 delta 为准。
- [API 比蓝图更小] → 先补真实消费者所需的只读结构与生命周期；不假设已有 Inline/嵌套树或“不开块也可 switch_mode”。公共边界独立审查，原输入组回归必须通过。
- [锁定 GPUI 无原生 Platform 实现] → 平台后端准入在规划期是已知阻塞，不是稍后再查 feature；保留锁版及 unsafe 基线，先提供具体最小审计方案再获裁决，之后才有 macOS 单窗/Metal 工具验证，不自动引依赖绕过。该风险已按 Context「2026-11-02/03 apply 调查与裁决更新」与 tasks 4.1/4.2 关闭，不再是当前阻塞；此处保留条目以免把审计路径从风险记录中抹去。
- [块级虚拟化挡不住巨块] → 预算前检、局部 shape 与错误入口单独测试；回退与基础成功两类证据分列。
- [字体重排影响命中/滚动] → 字体代次入缓存键并按源码锚点恢复，V-04 与 V-09 共同验收。
- [V-07 未冻结样例] → 使用现行规范能裁决的最小语义；不能裁决时只暂停依赖语义，提交最小输入/两种结果供裁决，不能更新现状金样冒充决定。
- [M0 豁免证据缺口] → M1 发现影响阅读安全的缺陷须保留失败回归并修复/阻塞；不反推 M0 全量验证或重写历史。
- [工具栏阶段性稀疏] → M1 只有编辑模式组，视觉上比原型空；接受这一阶段事实，不以范围外入口或置灰占位填补。
- [骨架数值影响既有虚拟化证据] → 文本宽由 848 变 768、行高由 24 变 28 后，1 MiB 样本的可见项与高度会变；既有实测记录保留为当时事实，新实测另记，不倒改旧证据。
- [统计口径未冻结] → 字符数按标量值、行数按 LF 的口径已可复核，但"字数是否排除空白/是否按字素簇"属 V-07 待核对项；实现处显式标注候选口径，不写成已冻结。
- [首末行内边距与虚拟化的交互] → 首行/末行的额外内边距在滚动到底时才被测量，可能造成一次末尾滚动跳变；属原型几何与虚拟列表的固有交互，V-09 实测记录。
- [无侧栏的网格退化] → M1 没有标签栏行，内容区直接顶到工具栏行下方；视觉上比原型少一格，属阶段范围，不预留空白占位。

## Migration Plan

实施从增量 API 和测试开始，不改变现有用户数据格式；当前没有持久化设置迁移，M1 原生窗口已按准入裁决接通，持久化设置仍待后续阶段。保持 `DocCore::new` 与已有公开语义兼容，新增只读构造/结果目标类型；先让现有无头测试通过，再替换 app 薄入口为 ui 启动装配。回退可撤回新窗口接线并保留引擎无头边界，无源文件写入或磁盘迁移；回退不算 M1 出口完成。实施结束的 docs 回填不包含 archive；归档另等用户指令。

并入的骨架范围按以下顺序迁移，均不改变既有 `readonly_layout` 契约与测试语义（`LayoutEnvironment` 的字段与键不变）：

1. `native_window.rs` 的 `render` 从"竖排一摞"改为三段结构，装配集中在新增的 `native_window/shell.rs`；原头部行（文件名 + 可视化/源码按钮）移除——文件名改由既有 `set_window_title` 承担，视图切换按钮迁入工具栏行。
2. 打开文档按钮保留在**内容区的无文档态**内（PRD §6.1 要求无文档时保留新建/打开入口），不再占据内容区之外的行位；关闭文档不新增内容区常驻按钮，继续由文件菜单与既有 action 承担——spec 只要求无文档态保留打开入口，而常驻关闭按钮在无文档态没有作用对象，属未确认的新控件。
3. `readonly_reader.rs` 删除自带的 `COLUMN_WIDTH`/`SCROLLBAR_WIDTH`/`FONT_SIZE`/`LINE_HEIGHT` 常量，改由 `geometry.rs` 提供；`prepare` 接受显式内容几何。
4. `NativeWindowCopy` 按值持有 `Translator` 并增加状态栏与工具栏文案；`NativeDocumentWindow` 增加统计标签与统计任务字段。无既有公开 API 变化。

## Open Questions

没有阻塞本变更实施的范围问题；原生后端及系统首选语言来源已按 proposal 的前置门于 2026-11-02/03 获具体裁决。实验收口预算/布局候选数值、实际字体可用性和未覆盖平台，不授权修改既定产品目标。并入的骨架范围同样没有阻塞实施的问题：统计口径的最终冻结、像素冻结与图标库选型分别归 V-07、V-09 与后续主题/图标决定；按 UI §3.3 与 §4.1 的候选口径执行，遇到不能裁决的呈现细节列入最小冲突并暂停依赖项，不自行扩大产品范围。用户已于 2026-11-03 裁决 SoftBreak 为带来源可见换行、有限 HTML 子集之外的 void 标签显示原文、未知非 void 容器整段显示原文；后续按用户裁决和 V-07 样例执行。其他未决导航样例继续沿 proposal 的冲突处理门；解析器已识别脚注的标签不一致必须修复，不能当正常失败收口。
