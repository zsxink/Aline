# Design

## Context

动机与范围见 `proposal.md`。本轮为规划，不表示下列拟新增接口或实验已实现。2026-11-02 通过 `openspec list --json` 确认无进行中 change，主规格包含 engine/*、engineering-governance 和 workspace/engineering-gates；M0 已归档，验收豁免不补证。CODE_STANDARDS §2～6、架构规范 R/K 与 Rust S/C 继续约束实现。

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

原生前置门：当前 lock 没有 `gpui_platform` 或对应原生后端包，GPUI 的 `macos` feature 也不存在。保留 GPUI 0.3.9 和自有 unsafe forbid；实施须先提交最小平台后端准入工件，核对与该 GPUI 公共类型的兼容性及完整依赖/许可/目标，再获具体裁决后接线。用户明确裁决前不改 lock、加依赖或改指纹；窗口及依赖真实 UI 的任务在准入完成前阻塞。现有纯逻辑任务可继续。系统 locale 来源须使用真实系统首选语言 API；不能假用 LANG 等同于 GUI 首选语言或硬选中文/英文当随系统完成。

2026-11-02/03 apply 调查与裁决更新：候选为同源发布 `gpui-pre-macos =0.3.9` + `font-kit`，系统首选语言使用 `sys-locale =0.3.2`。精确 package identity 与来源、target 闭包、许可证及用户逐项裁决见[原生准入候选方案](./native-admission-proposal.md)。task 4.1 的精确策略守卫、Cargo.lock 与七个 M1 图基线已重跑通过；例外仅限裁决列出的包身份/校验和/路径/目标。后续 Metal Toolchain 安装后，`cargo build -p aline --locked --offline` 已通过，macOS 单个空窗口实际显示并正常退出；4.2 完成。此证据只覆盖原生入口/空状态，不覆盖系统语言切换、文件选择或跨平台窗口。

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

启动由经前置门核对的系统首选语言适配输出映射到现有 Locale，其他语言回退英文；GPUI 核心无该 API，检测来源/失败回退必须实证，未具备时标阻塞而不能宣称随系统已完成。双语测试可注入显式 Locale，但不能替代真实默认语言检测。所有标题/菜单/按钮/错误/加载/只读/降级/无文档文字走 `Translator::t`，catalog key/插值测试继续双语一致。文档正文/文件名不翻译。完整语言覆盖设置和即时切换服务属于后续，不造无效开关。

M1 提供受保护默认 GitHub 语义配色的最小内容样式，随系统亮暗的启动环境输入保留；不实现四主题三模式选择、主题安装或配置持久化。排版/宽度/侧栏数值标实验候选，不从原型抄冻结值。默认与字体配置显式从 UI 启动配置传入，app-core 无 GPUI 类型。

V-04 通过同一字体适配边界验证系统字体、font_fallbacks 与有授权的静态 TTF/OTF 的 `add_fonts`：正常、缺失字族、损坏字体、缺汉字、重注册/代次改变后的重排与命中。静态字体实验不要求建设主题导入；资源仅使用已确认授权的独立夹具，无法取得时记阻塞，不下载来历不明字体。记录 OS/字族/回退观测方法、中文/拉丁/标点/emoji 基线、宽度和像素命中前后；API 成功不能单独证明指定字体被使用。注册失败/缺字回退异常必须可见，不静默宣称激活成功。

V-09 样本包含无文档/加载/正常/读取失败/取消/解析回退、窄窗/缩放、长文件名/英文、侧栏与主内容、焦点归还、滚动/跳转、普通代码长行。文件菜单只接本阶段真实动作，V1 后续写动作置灰并有只读原因或按阶段隐藏，不能显示假成功。设置/完整工具栏、tabs/文件树不为“看起来完整”先建。系统关闭/显式关闭文档汇到同一只读关闭意图；退出无保存任务，不复用 M2 未保存对话流程。

### 8. 验证顺序、证据与治理复用

先测试新结构/映射/只读生命周期与独立夹具，再做无内容单窗 macOS 冒烟，再接内容与真实命中/字体/长文。纯单测、GPUI 交互/布局与真实平台观测分别记录。新增测试消费者落 `crates/aline-markdown/tests/`、`crates/aline-ui/tests/`；稳定夹具在对应 `tests/fixtures/`；复跑入口在 `scripts/`，不新增测试库。

V-05 使用 Release 与 32/128 KiB、1/4 MiB、>4 MiB、许多短块/单巨块/巨表/密集实体等固定语料；分别测读取/解码、快照、解析/映射、首布局、主线程采纳、首次呈现、模式切换与滚动，包含等待排队。首轮每个可自动测的样本/冷热条件至少 20 次，报告样本数及 nearest-rank P50/P95、最长主线程停顿、峰值内存和虚拟 item 数；冷/热如何清理缓存需可重现，不声称完全排空 OS 缓存。真实滚动另记录从头到尾及往返、PageUp/PageDown/Home/End 或菜单等价入口与可响应动作；不能只有微基准或平均 FPS。20 次是首轮采样方案，不是新增发布性能门槛。

不开编辑/IME/查找/资产后端，对应时延项标不适用 M1、未测留后续，不能填零。>4 MiB Source 是已确认默认策略，不是硬上限；硬预算与 layout chunk 数值通过 V-05/V-06 参数实验确定并记录，不能无限输入或降目标。

继续跑完整 Cargo 门禁与既有 workspace/工程/lint/安全/公共 API/catalog 检查。若原生 features 导致实际图变化，登记精确差异并独立审计再接线，禁止自动置 reviewed 或扩禁用例外；无合规现有组合则暂停受影响任务，提出最小准入问题。新/改守卫只扩既有 `workspace/engineering-gates` 能力，必须正例/最小反例。本设计不声称已执行任何实验。

原始输出按 `logs/validation/m1-single-window-readonly/<run-id>/` 的分项子目录保存，使用 `scripts/evidence_paths.py` 归一化；不得把本机绝对路径写进工件/证据。独立输入与预期可入库，临时副本/截图/计时数据/Agent 审查原文不入库。最终摘要回填原验证计划和架构，保留 M0 豁免缺口及 Windows 10/Intel Mac/Linux 未测；`--all-targets` 不证明跨平台。详细执行命令与每组关联在 tasks。

## Risks / Trade-offs

- [API 比蓝图更小] → 先补真实消费者所需的只读结构与生命周期；不假设已有 Inline/嵌套树或“不开块也可 switch_mode”。公共边界独立审查，原输入组回归必须通过。
- [锁定 GPUI 无原生 Platform 实现] → 平台后端准入是已知阻塞，不是稍后再查 feature；保留锁版及 unsafe 基线，先提供具体最小审计方案再获裁决，之后才有 macOS 单窗/Metal 工具验证，不自动引依赖绕过。
- [块级虚拟化挡不住巨块] → 预算前检、局部 shape 与错误入口单独测试；回退与基础成功两类证据分列。
- [字体重排影响命中/滚动] → 字体代次入缓存键并按源码锚点恢复，V-04 与 V-09 共同验收。
- [V-07 未冻结样例] → 使用现行规范能裁决的最小语义；不能裁决时只暂停依赖语义，提交最小输入/两种结果供裁决，不能更新现状金样冒充决定。
- [M0 豁免证据缺口] → M1 发现影响阅读安全的缺陷须保留失败回归并修复/阻塞；不反推 M0 全量验证或重写历史。

## Migration Plan

实施从增量 API 和测试开始，不改变现有用户数据格式；当前没有可用原生 UI 或持久化设置迁移。保持 `DocCore::new` 与已有公开语义兼容，新增只读构造/结果目标类型；先让现有无头测试通过，再替换 app 薄入口为 ui 启动装配。回退可撤回新窗口接线并保留引擎无头边界，无源文件写入或磁盘迁移；回退不算 M1 出口完成。实施结束的 docs 回填不包含 archive；归档另等用户指令。

## Open Questions

没有阻塞本规划创建的范围问题；原生后端及系统首选语言来源按 proposal 的前置门处理。实验收口预算/布局候选数值、实际字体可用性和未覆盖平台，不授权修改既定产品目标。用户已于 2026-11-03 裁决 SoftBreak 为带来源可见换行、有限 HTML 子集之外的 void 标签显示原文、未知非 void 容器整段显示原文；后续按用户裁决和 V-07 样例执行。其他未决导航样例继续沿 proposal 的冲突处理门；解析器已识别脚注的标签不一致必须修复，不能当正常失败收口。
