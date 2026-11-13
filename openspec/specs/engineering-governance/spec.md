# engineering-governance Specification

## Purpose

规定 Aline 后续代码实现与架构演进必须遵守的工程契约，将现行架构/Rust 规则及模块划分、设计模式、交付要求接入 OpenSpec。详细执行标准见 [CODE_STANDARDS.md](../../CODE_STANDARDS.md)，现行文档的职责与确认状态保持不变。本规格约束开发过程，不表示 Cargo workspace、CI、M 阶段或 V 验证已完成。

## Requirements

### Requirement: 实现必须遵循当前工程基线

所有相关实现与评审 MUST 遵循 AGENTS.md、现行 docs 专项规范及 CODE_STANDARDS.md；变更 MUST 标明适用 M 阶段、需求/AC/V 和相关规则。OpenSpec 拟议变更 MUST NOT 静默覆盖已确认契约。本文的检查方案 MUST 区分实际执行、人工检查与尚待建立的自动门禁。

#### Scenario: 后续开始代码实现

- **WHEN** 开发者或 Agent 开始实现一个功能或修复
- **THEN** 阅读受影响基线，识别所属模块、适用规则及验证方式，按前置阶段出口推进

#### Scenario: 变更与已确认规范冲突

- **WHEN** design 或实现需要改变铁律或已确认产品/架构契约
- **THEN** 列出冲突和影响并先取得对应裁决，暂停依赖该决定的实现，不把草案当生效例外

### Requirement: crate 依赖与引擎隔离必须可核查

实现 MUST 保持现行八 crate 的单向允许依赖及 source/app-core 独立边界。五引擎 crate MUST 保持完整依赖闭包无 GPUI，不持窗口、执行器、线程或全局状态。新增 crate、依赖或公开契约 MUST 记录架构影响并按准入流程处理。

禁用依赖 MUST 按现行架构 §3.2 的精确例外核对来源、版本、归档校验和、全部引入路径、target/features 与消费者；五引擎 MUST NOT 借用 GPUI 例外。检查 MUST 保留完整图并与实际组合 Cargo tree 交叉核对，解析失败 MUST NOT 算合规。新增路径或例外身份漂移 MUST 重新审计，不能按前缀或整个 GPUI 闭包放行。

#### Scenario: 引擎通过依赖别名间接引入 GPUI

- **WHEN** 引擎的 target、feature、dev/build 或传递依赖经别名指向 GPUI
- **THEN** 判定违反隔离要求；M0 建立的依赖检查须识别真实 package 与闭包并拒绝该变更

#### Scenario: 提出新增 crate

- **WHEN** 模块需要独立为新的 crate
- **THEN** design 说明职责、消费方、依赖图和独立验证收益，不能只以行数或目录对称为依据

### Requirement: 变更必须保持唯一真源和受检写入

实现 MUST 保持源文本唯一真源、至多一个受控块草稿、映射转换和受检区间补丁协议。文档变更 MUST 统一经 DocCore 提交；插件、子视图及平台接线 MUST NOT 持第二套可写全文、独立文档历史或可任意写 Buffer 的句柄。存储保真 MUST 按引擎与 Rust 规范验证。

#### Scenario: 新增编辑入口

- **WHEN** 快捷键、菜单或插件发起同一编辑命令
- **THEN** 复用统一预检、映射、补丁和输入分组/历史协议；拒绝时文本、选区和历史保持约定不变

#### Scenario: 保存未编辑文档

- **WHEN** 文档内容未实际改变且执行保存
- **THEN** 原文件字节原样写回；已编辑时仅按现行 BOM/行尾契约处理，不能从展示树重建 Markdown

### Requirement: 模块必须按职责和生命周期划分

新增或显著扩展模块 MUST 说明职责、状态所有者、输入输出、消费方和副作用边界。拆分 MUST 围绕连贯概念及不变量，保持最小公开面和单向依赖。职责混杂、被迫扩大公共接口或无法隔离纯规则测试时 MUST 调整边界。

#### Scenario: 保存和渲染逻辑进入同一可变模块

- **WHEN** 一个模块同时维护独立的保存事务与视图布局状态，且变化/测试边界已分离
- **THEN** 按所有者和生命周期重划边界，通过明确接口协作，不能仅通过改文件名视为完成拆分

#### Scenario: 文件较长但职责连贯

- **WHEN** 文件达到约 500 行的人工复核信号
- **THEN** 评审检查职责、接口、测试和后续增长位置，记录拆分或保留依据；不以固定行数自动要求新 crate、空目录或机械切文件

### Requirement: 抽象必须对应真实边界

实现 MUST 优先使用满足当前需求的具体类型、函数、enum 和组合；新增 trait、泛型框架或服务层 MUST 说明真实替换点、故障注入需要或已确认的架构边界。公开 API MUST 有实际消费方，使用维护不变量的最窄可见性，MUST NOT 为测试暴露可写内部状态。

#### Scenario: 只有一个生产实现但需要故障注入

- **WHEN** 文件发布流程需要模拟磁盘满或阶段失败
- **THEN** 可以在真实边界引入小接口和有价值的 fake，说明消费方与验证，不必实现第二个生产后端

#### Scenario: 无真实变化需求的多层包装

- **WHEN** 一个简单调用被包装为无独立契约的 Factory、Service 和 Repository
- **THEN** 评审要求收敛到最简单可行方案，不以设计模式名称作为保留理由

### Requirement: 设计模式必须解释问题和代价

相关 change 的 design MUST 按 CODE_STANDARDS.md §4 说明问题、最简单可行方案、采用的模式或普通实现、代价及验证。模式 MUST 适配已有 crate、状态和事务边界，MUST NOT 引入第二套历史、未立项外部插件宿主、数据库或额外运行时。

#### Scenario: 新增有取消和失败分支的保存流程

- **WHEN** 保存流程需要表示等待、冲突、执行、失败和取消
- **THEN** 明确状态和转换、唯一所有者及提交边界，覆盖非法/重复转移和回执；不能以散落布尔值组合隐式状态

#### Scenario: 简单变更无需新模式

- **WHEN** 具体类型与函数已能保持边界并完成测试
- **THEN** design 可以记录沿用现有方案及理由，无需为了规范增加 trait、Builder 或事件总线

### Requirement: 副作用与异步结果必须由所属所有者结算

实现 MUST 明确任务/订阅的所属者、输入身份/版本、取消/释放点及采纳规则；引擎返回数据，UI 编排副作用。可替换派生结果 MUST 在采纳前复验依赖；不可丢弃的写盘回执 MUST 按持久化协议结算。多资源流程 MUST 明确逐步发布、失败保全和恢复边界，不宣称跨文件物理原子性。

#### Scenario: 关闭或配置变化后派生结果晚到

- **WHEN** 解析或插件结果返回时文档、请求或配置身份已失效
- **THEN** 拒绝采纳到当前展示，按所属协议释放资源，不重新打开已关闭对象或提交重复变更

#### Scenario: 写盘期间内容继续改变

- **WHEN** 较早快照的写盘成功回执晚于新输入到达
- **THEN** 登记实际写入的版本和字节，保留新内容未保存事实，不丢掉真实回执或把新内容标记已保存

### Requirement: Rust 类型和错误必须守住不变量

跨模块源码/绘制/UTF-16 坐标及范围 MUST 用 newtype/受检封装，并经映射层转换；原始补丁表示保持不变。错误 MUST 类型化并在适当边界转换。引擎含测试/示例/基准 MUST NOT 使用 unwrap；生产引擎 MUST NOT 使用 expect、panic!、unreachable! 或发布期 assert! 处理失败。自有代码 MUST 默认 forbid unsafe，例外沿用现行裁决与独立安全审查要求。

#### Scenario: 接收非法 Unicode 端点

- **WHEN** 外部输入的 UTF-8/UTF-16 端点、范围或投影身份无效
- **THEN** 受检拒绝或按既有协议重算，不猜偏移、不 panic，debug_assert! 不能替代输入校验

### Requirement: 用户文案和配置必须保持统一来源

用户文案 MUST 经 t("key") 和 zh-CN/en catalog，翻译仅在 UI 边界完成；引擎 MUST 返回错误枚举。有效设置/主题/语言 MUST 由唯一配置服务发布类型化快照和代次，窗口或插件 MUST NOT 另立权威副本，日志 MUST 避免泄漏正文和敏感信息。

#### Scenario: 新增失败提示

- **WHEN** 引擎新增一个会展示给用户的拒绝或错误
- **THEN** 引擎返回可匹配类型，UI 映射翻译 key，双语 catalog 的 key 与插值保持一致

### Requirement: OpenSpec 工件必须追踪实现约束

涉及产品行为、依赖/架构边界、公共契约或验收变化的工作 MUST 先建立 change。proposal/design/tasks MUST 按 CODE_STANDARDS.md §6.1 记录模块、规则、模式判断、失败/兼容边界和具体验证；局部修复按开发流程豁免 change 时仍 MUST 提供适用规则与证据。重构 MUST 清理替代路径及无消费者资产，并检查引用和回归。

#### Scenario: 重构已有命令入口

- **WHEN** 新实现替代旧入口并准备交付
- **THEN** 核对调用链与实际消费，删除失效实现/导出/依赖/测试替身，执行相关检查，保持规格和 docs 一致

### Requirement: 工程门禁必须证明实际有效

M0 workspace 建立后，Rust 提交 MUST 通过 cargo fmt --all --check、cargo clippy --workspace --all-targets -- -D warnings 和 cargo test --workspace。workspace lint MUST 对每个成员验证实际继承；新增或修改自动守卫 MUST 用正例和最小违规反例证明有效，说明覆盖范围与盲区。未建立的门禁 MUST 标为待建立，人工规则 MUST 给出可复核步骤。

#### Scenario: 根配置存在但成员未继承

- **WHEN** 根 workspace 定义 lint，而某成员没有继承或等价有效策略
- **THEN** 判定未满足 lint 要求，通过成员检查及最小诊断样例暴露缺口，不以根表存在视为完成

#### Scenario: 当前只有文档和工程规格

- **WHEN** 仓库尚无 Cargo workspace 或平台实现
- **THEN** 只记录实际执行的 OpenSpec/文档检查，明确 M0 工程守卫与 V 验证未完成，不虚报 Cargo 或平台通过

### Requirement: 测试和交付必须有独立可信证据

新引擎机制 MUST 先有无头测试再接 UI，金样 MUST 来自独立预期，关键拒绝/失败/取消/过期及保真路径 MUST 有适用证据。交付 MUST 记录实际命令、结果、环境、平台缺口和例外，重要变更按 AGENTS 路由独立审查。记录的存放 MUST 遵循开发流程 §5：代码、可复现测试/脚本及独立输入/预期入库，原始输出与 Agent 过程文档留本地；当前交接 MUST 只维护一份且不追踪，必要决定及阶段验证摘要回填既有规范/验证计划。归档 MUST 完成适用验证、delta 同步与文档回填，MUST NOT 被当作 M 阶段或 V 验收通过的证据。

#### Scenario: macOS 检查通过而其他平台未执行

- **WHEN** 交付仅在 macOS 运行检查，或仅执行了 all-targets
- **THEN** 明确 Windows/Linux 和其他 feature 的证据缺口，不扩大通过声明；正式 V1 仍须满足三平台出口

#### Scenario: 申请非铁律规则的临时例外

- **WHEN** 实现需要偏离工程规则
- **THEN** 记录原规则、理由、最小范围、替代保证、验证和退出条件；涉及已确认契约仍先裁决，不以 lint allow 授权绕过铁律

#### Scenario: 验证完成并准备换会话

- **WHEN** Agent 完成一轮实现、验证或审查并需要交接
- **THEN** 提交必要代码、回归测试、独立输入/预期与可复跑脚本；原始输出和逐轮报告留本地，更新唯一不追踪交接及适用的现有规范/任务状态

#### Scenario: 运行输出被包装成文档

- **WHEN** 拟提交的 Markdown、JSON、CSV 或压缩包只记录一次运行过程或复制已有交接
- **THEN** 不提交该产物，将必要决定/验收摘要回填既有入口，将有回归价值的复现整理为测试或脚本

### Requirement: 验证方法采纳必须标明来源和效力边界

QMD-01 的方法采纳 MUST 逐项标明来源，区分 quiet-md 已有机制、Aline 既有规则与 Aline 追加/补强；没有直接证据的 quiet-md 行为 MUST 标为“未确认”或“仅方向参考”，MUST NOT 写成已观察事实或 Aline 已实现能力。研究文档 MUST 只作证据索引，与现行 `docs/` 冲突时以现行规范为准。方法采纳 MUST NOT 改变产品契约、依赖图或阶段范围；改变依赖、夹具、验收方式或跨模块契约时 MUST 先建对应 OpenSpec change。关联 QMD-01、开发流程 §2/§3.1、CODE_STANDARDS §6。

#### Scenario: 引用 quiet-md 机制

- **WHEN** 规范或 change 声称采纳 quiet-md 的某项做法
- **THEN** 同时给出 quiet-md 的观察位置/证据等级与 Aline 的适配差异；若该做法是 Aline 既有或追加，明确标注，不把两者合并成“采纳 quiet-md”

#### Scenario: 方法需要新增依赖或改变验收

- **WHEN** 落实某项方法需要新增/升级依赖、增加夹具、改变 V 项报告字段或修改跨模块契约
- **THEN** 先建立独立 OpenSpec change 并完成评审，再写依赖该变化的代码；不得以 QMD-01 或本研究文档直接授权

#### Scenario: 研究结论与现行规范冲突

- **WHEN** 研究文档的推论、建议或数值与现行 `docs/` 不一致
- **THEN** 暂停受影响实现并列出冲突，由现行规范或用户裁决；不按目录、日期或“参考实现如此”自行选择

### Requirement: 随机不变量与性能证据必须可复现和可追溯

引擎随机测试 MUST 使用确定性 RNG/固定 seed，生成器 MUST 按 Aline 语法只产出合法或预期拒绝输入；每步 MUST 检查该层不变量，并周期性执行保存往返或与全量权威路径差分。失败 MUST 保留 `seed、步号、操作日志、最小输入`；最小输入整理为正式夹具后提交，原始日志只留 `logs/validation/`。已知语义丢失 MUST NOT 用测试辅助函数规范化后当作通过。默认 seed/步数 MUST 在 W3 立项时按 CI 时间确定，扩量运行只作补充证据。性能结论 MUST 记录环境与版本、源码 commit/锁定构建、样本 SHA-256、方法/操作序列、冷热缓存、计时起止、P50/P95、最长停顿、峰值内存、原始日志入口、复现命令及明确不测量项；数值门槛 MUST 由 V-05 实测与 PRD 目标冻结，MUST NOT 由本规格或 quiet-md 数值给出。关联 R-11-01、R-17-03、S-12-06、S-14-03、E-T20、V-05/V-07/V-08。

#### Scenario: 随机测试失败

- **WHEN** fuzz 或属性测试发现失败
- **THEN** 记录 seed、步号、操作日志与最小输入，提供可复跑命令；把最小输入和独立预期整理为回归夹具，不删除操作日志或只保留失败断言

#### Scenario: 性能报告缺少追溯信息

- **WHEN** V-05 报告只有汇总数字，缺少设备/版本、样本指纹、计时边界、冷热、原始输出或复现命令
- **THEN** 判定报告不完整，不用该数字冻结门槛或宣称性能通过；补测或明确标为未完成

#### Scenario: 用 fuzz 或微基准替代验收

- **WHEN** 只有随机测试或局部微基准数据，缺少金样、真实 IME、文件故障或端到端呈现证据
- **THEN** 只记为补充证据，不替代对应 V 项或 AC；明确未覆盖平台与失败路径

### Requirement: 保存与恢复必须分离且故障安全

自动保存（1 秒防抖）MUST 只写用户文档；10 秒恢复快照 MUST 写应用管理的独立目标，二者共用版本/资源协议但 MUST NOT 互相代替。写入 MUST 至少核对：同目录临时文件 → 写完 → 同步 → 平台受检替换；写前 MUST 核对磁盘基线，失败 MUST 保留原文件、内存与恢复材料。恢复 MUST 先进入内存、不覆盖磁盘；恢复快照 MUST NOT 以文档旁 `.bak`/`.corrupt-*` 副本作为恢复手段。原子替换与目录同步的保证范围 MUST 按平台实测说明，MUST NOT 称为跨进程 compare-and-swap。关联 R-15-13、SAFE-04/05/07/08、V-08。

#### Scenario: 自动保存与恢复快照被合并

- **WHEN** 实现把恢复快照写入用户文档、用文档旁 `.bak` 代替恢复目标，或用自动保存状态宣称恢复已受保护
- **THEN** 判定违反分离要求；恢复写入失败单列诊断，旧记录不得看起来仍保护最新内容

#### Scenario: 平台替换能力未验证

- **WHEN** 目标平台的文件系统、权限、占用或断电语义尚未实测
- **THEN** 只声明已观察到的保证范围，保留失败/未测项；不把 `rename`、`sync_all` 或一次 stat 泛化为所有文件系统的原子承诺

#### Scenario: 失败后丢失原文件或内存状态

- **WHEN** 写盘、替换或恢复写入失败
- **THEN** 保留原文件、当前内存内容与恢复材料，提供重试/另存为/恢复路径；不先删原文件、不原地截断、不在失败后静默改为非原子写

### Requirement: IME 集成必须有显式清理和受检坐标

UI MUST 在实际失焦、点击其他块、切模式或窗口失活时显式结束或清理 marked 与组合状态，MUST NOT 只靠 Blur 事件推断平台已确认或取消。等待平台组合完成结果期间 MUST 保留原上下文，MUST NOT 抢焦点、丢草稿或立即取消组合。候选窗位置 MUST 按元素 frame、文本布局、滚动偏移与窗口坐标逐层换算，并在换行、DPI、多屏变化后重新定位。平台 glyph 索引、UTF-16 端点与字素边界 MUST 只在其声明投影内使用，MUST NOT 互相冒充；跨投影 MUST 经统一映射层。真实中文输入法行为 MUST 由 V-01/V-07 取证，MUST NOT 由纯状态机或模拟替代。关联 ED-01、TA-01、E-T15、UI §6/§13、V-01/V-07。

#### Scenario: Blur 到达但平台尚未完成

- **WHEN** 焦点变化或窗口失活，而平台组合确认/取消结果尚未返回
- **THEN** 等待平台结果并保留当前输入与草稿；不立即 unmark/cancel，不显示保存/导出成功，不生成基于半组合内容的产物

#### Scenario: 坐标投影混用

- **WHEN** 实现把 glyph 索引当字素索引、把 UTF-16 端点当 UTF-8 字节偏移，或跨投影手算偏移
- **THEN** 判定违反坐标边界；改用声明投影与统一映射，并补最小失败样例覆盖 emoji ZWJ、组合符、CJK 标点、CRLF

#### Scenario: 只用模拟状态机证明 IME

- **WHEN** 没有真实中文输入法的候选窗、marked 绘制、焦点与平台 callback 证据
- **THEN** 只记为无头/模拟结果，不宣称 V-01/V-07 或相关 AC 通过

### Requirement: UI 语义 token、中文排版与离屏证据必须按边界使用

UI 组件 MUST 只消费语义 token，原始色值 MUST 集中在 token 模块并注明来源；token 重构 MUST NOT 改变已确认视觉与主题包 schema。中文排版 MUST 先测量后决定：全角标点避头尾、合成斜体、CJK 标点字体回退 MUST 先进入 V-04 夹具，quiet-md 的数值 MUST NOT 写成 Aline 规范值；是否 fork/vendor 锁定 GPUI 包 MUST 另立决定并走依赖准入。离屏视觉证据 MUST 只作补充，MUST NOT 替代真实窗口、IME 与平台证据；引入锁定 GPUI 的 `test-support` feature、`gpui_macos` 平台实现（含其 `test-support` 能力）与 `image` 等 dev-dependency 前 MUST 先完成能力 spike 与依赖准入。离屏截图与真实窗口截图的文件名、证据目录和结论 MUST 分开记录。关联 R-9-02、R-11-01、UI §3.2、V-04/V-09。

#### Scenario: 语义 token 重构

- **WHEN** 组件直接使用原始色值，或 token 重构改变已确认视觉/主题包 schema
- **THEN** 判定违反 token 边界；把色值收回 token 模块并注明来源，重构前后用截图/像素对比作为审查证据，不静默改视觉

#### Scenario: 中文排版数值被直接采纳

- **WHEN** 规范或实现直接采用 quiet-md 的避头尾角度、字号、行高或字体补丁值
- **THEN** 改为 V-04 夹具中的待测候选；先测量锁定 gpui-pre 的实际行为，再决定是否 vendor/fork，数值不因引用而冻结

#### Scenario: 离屏证据被当作平台通过

- **WHEN** 只有 `VisualTestAppContext` 或离屏截图，缺少真实窗口/IME/平台操作
- **THEN** 只记录为补充证据并标明门控与依赖状态；不宣称 V-09、AC-15/AC-16 或对应 UI 行通过，不把离屏与真实截图混在同一结论

### Requirement: W0～W7 必须逐项立项

quiet-md 调研中的 W0～W7 MUST 视为工作分解与建议顺序，MUST NOT 视为已批准任务。每个 W 项实施前 MUST 建立可独立验收的 OpenSpec change，写明范围、非目标、依赖准入、证据表与适用 V 项；W0 未批准 MUST NOT 写代码。依赖准入、验收方式、跨模块契约或阶段范围变化 MUST 在对应 change 中评审后再实施。关联开发流程 §2/§3.1、CODE_STANDARDS §6、QMD-01。

#### Scenario: 直接开始 W 项

- **WHEN** 未建立对应 change 或未获用户批准，就新增依赖、测试夹具、平台代码或改动验收方式
- **THEN** 停止该实施；先补 change 并完成评审，研究文档中的建议 change 名称不自动生效

#### Scenario: 把 W0～W7 当作已批准计划

- **WHEN** 任务表、文档或评审把 W0～W7 写成已立项、已排期或已实现
- **THEN** 改回“工作分解/建议顺序”，保留各项待确认问题；本规格不替用户裁决 test-support 准入、vendor 顺序、fuzz 规模或阶段时点
