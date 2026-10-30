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

新引擎机制 MUST 先有无头测试再接 UI，金样 MUST 来自独立预期，关键拒绝/失败/取消/过期及保真路径 MUST 有适用证据。交付 MUST 记录实际命令、结果、环境、平台缺口和例外，重要变更按 AGENTS 路由独立审查。归档 MUST 完成适用验证、delta 同步与文档回填，MUST NOT 被当作 M 阶段或 V 验收通过的证据。

#### Scenario: macOS 检查通过而其他平台未执行

- **WHEN** 交付仅在 macOS 运行检查，或仅执行了 all-targets
- **THEN** 明确 Windows/Linux 和其他 feature 的证据缺口，不扩大通过声明；正式 V1 仍须满足三平台出口

#### Scenario: 申请非铁律规则的临时例外

- **WHEN** 实现需要偏离工程规则
- **THEN** 记录原规则、理由、最小范围、替代保证、验证和退出条件；涉及已确认契约仍先裁决，不以 lint allow 授权绕过铁律
