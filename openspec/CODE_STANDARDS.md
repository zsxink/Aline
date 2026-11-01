# Aline 代码与架构执行规范

生效日期：2026-10-30。状态：后续实现与评审必须遵循的工程基线；M0 已建立 Cargo workspace 和本地工程门禁，当前统一离线复核仍有未核实项，M0 出口未闭合；V-01～V-09 状态不变。

## 1. 适用范围与规范关系

本文适用于自有 Rust 实现、GPUI 接线、测试、示例、基准、构建脚本及其 OpenSpec change。后续编码前必须阅读本文及受影响的现行专项规范。

- [AGENTS.md](../AGENTS.md)规定铁律和工作规则；[PRD](../docs/v1-prd.md)规定产品结果。
- [架构指引 §4/13](../docs/v1-architecture.md)规定模块归属与里程碑；[架构规范](../docs/architecture-guide.md)的 R/K 规则和[引擎设计](../docs/engine-design.md)规定依赖、状态与机制契约。
- [Rust 规范](../docs/rust-style-guide.md)的 S/C 规则规定代码细节；UI、插件和主题按各自现行专项规范执行。
- 本文汇总执行入口，并补充模块划分、设计模式与交付要求；[engineering-governance](./specs/engineering-governance/spec.md)把这些要求表达为可验收的工程规格。工程规格约束开发行为，功能规格继续描述产品行为。
- [验证计划](../docs/v1-validation-plan.md)记录实际证据。OpenSpec change 是拟议变更，不能以 design、任务勾选或归档覆盖已确认约束。同级规范存在无法解释的冲突时，列出冲突并暂停依赖该决定的实现。

本文的“必须/禁止”是评审要求；“建议”允许有理由的替代。“待验证”不等于已启用。新增或调整工程规则必须同时维护本文、工程规格及相关 docs；涉及铁律、产品范围或已确认契约时先取得对应裁决。

## 2. 架构边界与所有权

### 2.1 crate 职责

沿用当前八 crate，不为套用架构模板增加 Domain/Application/Infrastructure 空壳 crate：

```text
app → aline-ui → aline-plugin → aline-edit → aline-markdown → aline-buffer
app / aline-ui → aline-source
其余层按需 → aline-app-core
```

主链可以向下直接消费公开 API，不要求每个相邻依赖都存在。source 独立，buffer 不依赖 source；app-core 不反向依赖业务层。GPUI 仅允许进入 app/ui/plugin，五引擎 crate 的完整依赖闭包必须无 GPUI，包括相应 target、feature、dev/build 依赖。

| 所有者 | 职责与边界 |
|---|---|
| `aline-source` | 文件字节、编码/行尾元数据、磁盘版本和受检 I/O 原语；不拥有编辑历史或任务调度 |
| `aline-buffer` | 源码、补丁、锚点、选区和历史原语；不解析、不落盘 |
| `aline-markdown` | 唯一解析器上的语义、保护范围与坐标映射；不持 GPUI、主题色或插件注册表 |
| `aline-edit` | 命令/搜索/格式化的纯计划与校验，返回 Change 或拒绝；不自行写真实 Buffer |
| `aline-app-core` | theme/settings/i18n 纯模型和校验；不读全局 locale、不注册字体或安装资源 |
| `aline-plugin` | 公共能力、注册/生命周期/分发及提供者；不反向依赖 UI、不直写 Buffer |
| `aline-ui` | DocCore、唯一输入状态机、会话/资源/持久化编排、任务调度与采纳、GPUI 适配 |
| `app` | 初始化、平台/菜单/单实例装配；复用 UI 服务，不复制状态机 |

### 2.2 数据与副作用

必须保持以下已有契约，详细判定沿用架构规范 R-3～R-8、R-12～R-16：

1. 源码是唯一事实来源；Buffer 加至多一个受控块草稿形成当前内容。解析树和布局是派生数据，禁止从 AST 重建文档或维护第二份可独立编辑全文。
2. 变更统一经 edit/DocCore 受检入口，补丁表示保持 `Vec<(Range<usize>, String)>`；多补丁原子变更和输入分组按引擎契约执行。菜单、快捷键、插件与子编辑器共享提交协议和历史。
3. 源码/绘制/UTF-16 跨模块坐标和范围使用 newtype/受检封装；调用方通过映射层转换，禁止手算偏移。版本、单位、方向和投影身份须可追踪。
4. 文件、恢复、主题、设置、资源、导出各有明确提交所有者。source 提供文档字节原语，UI 编排多资源流程；不得把所有 I/O 强塞进 source。
5. 引擎零线程、零全局状态；UI 负责调度。后台结果携带实际依赖的身份/版本，采纳前复验；取消和关闭后须有明确结算与释放路径。不可丢弃的写盘回执按持久化协议结算，不能套用展示结果过期即丢弃的规则。
6. render/布局读取派生快照，不提交业务变更；render/布局、按键和选区等高频 UI 路径禁止全文解析、指纹/等价比较、统计、序列化、配置读取、文件 I/O 及同步资源解码。组提交/显式命令的昂贵计算由所有者调度，某段纯计算是否可同步须有 V-05 预算证据，不能用预算放开高频路径禁令。缓存须定义完整依赖键、失效和容量/生命周期。

## 3. 模块、文件与函数的大小

### 3.1 按职责逐层划分

| 层级 | 划分依据 | 必须避免 |
|---|---|---|
| crate | 既定职责、依赖隔离、可独立验证边界；新增/合并需在 change 中说明必要性和图影响 | 为行数、目录对称或未来设想增加 crate |
| 功能模块 | 一个领域能力或完整状态生命周期，明确所有者、输入输出、消费方与副作用 | `utils`、`common`、`manager` 汇聚无关职责；按“类型/函数”机械分桶 |
| 子模块/文件 | 一个连贯概念及其 impl/辅助类型，或一个明确适配/状态转换/策略边界 | 把一条状态不变量切散到多文件，也把编辑、保存、渲染混在同一文件 |
| 函数 | 一个可描述的动作，错误出口可追踪；编排函数可以组织多个具名步骤 | 交错多种副作用、靠长布尔参数列表选择无关行为、只为缩行制造一次性跳转 |

新建或显著扩展模块时，必须在 `//!` 文档及设计/评审记录中说明职责、状态所有者、输入输出、消费方和禁止承担的副作用。内部纯辅助模块可引用父模块契约补充自身职责，不重复整张账本。

拆分或继续扩展前，必须判断：

- 是否同时承担多个独立变化原因或拥有不同生命周期的可变状态？
- 调用方是否需要无关能力，或通过跨模块访问内部字段维持不变量？
- 测试是否必须启动窗口/真实磁盘才能验证纯规则？
- 模块内部是否出现可独立命名、能单独验证的连贯概念？
- 拆分能否保持单向依赖、唯一状态所有者和现有公开入口？

职责混杂、公开接口被迫扩大或无法隔离测试时必须调整边界。行数、参数数和嵌套深度用于发现问题；沿用 Rust 规范 §3 的约 500 行人工复核信号，**不设按 500/1000 行自动拆分或 trait 方法数上限的门禁**。超过信号而保留时，评审说明职责为何仍连贯、后续增长放在哪里；也不得以未达行数为由保留混杂职责。

目录遵循架构 §4.3：聚合模块采用 `mod.rs`，叶模块采用具名 `.rs`。模块根保持精简，以模块声明、显式导出和必要装配为主；测试就近组织，跨模块契约测试放所属 crate 的 `tests/`。不新建无人消费的空目录、通用框架或预留实现。

### 3.2 公共 API 与抽象

- 默认私有，逐级使用满足实际消费的最窄可见性；跨 crate API 从 `lib.rs` 明确导出。每个公开项必须有真实消费方及失败/版本语义，不为测试暴露可写内部状态。
- 优先具体类型、函数、enum 和组合。trait 必须对应真实替换点、故障注入需要或已确认的架构边界；一个生产实现加有价值的 fake 可以成立，不要求为凑数量实现第二个后端。
- trait 按调用方能力拆分；需要大量默认空实现、运行时“不支持”分支或无关方法时复核接口。不把所有能力放进万能 Service/Manager。
- 依赖通过参数、构造器或明确装配层传入；引擎不通过全局单例、Service Locator 或反向回调获取 UI。
- 泛型/`impl Trait` 与 `dyn Trait` 根据封闭/开放集合、异构注册、所有权和编译成本选择；不得宣称静态或动态分派始终更优。新宏、共享可变锁和内部可变性须说明具体必要性。

## 4. 设计模式的选用

设计模式服务于真实边界。每次相关 change 的 design 必须写出“问题 → 最简单可行方案 → 采用的模式/普通实现 → 代价与验证”；简单变更可以明确记录“采用具体类型与函数，现有边界足够”。

| 模式/惯用法 | Aline 中的适用场景 | 采用条件与约束 | 验证重点 |
|---|---|---|---|
| Functional Core / Imperative Shell | 解析、映射、编辑计划与 UI/磁盘编排分离 | 引擎显式输入和类型化输出；source 的 I/O 原语仍按其职责保留 | 无头验证；纯规则不需窗口/执行器 |
| Ports and Adapters / Adapter | 平台输入、剪贴板、文件故障注入、插件/导出后端 | 在真实边界适配，接口留在有权定义契约的既有层；不新增反向依赖 | fake/真实适配器契约，错误和关闭路径 |
| Command / Transaction Script | 菜单/快捷键统一意图，edit 预演，DocCore 提交补丁 | 使用既有命令和历史协议，不另建事件溯源或第二套 undo；明确拒绝时零写入 | 入口等价、多补丁原子性、撤销/redo、拒绝不变性 |
| State Machine | IME/输入组、保存/冲突/关闭、插件生命周期 | 用 enum/明确状态与转移表达非法组合；状态所有者唯一。只有编译期状态约束确有收益时用 typestate | 转移表，取消、失败、重复事件、过期回执 |
| Strategy / Provider Registry | 已有插件能力、可替换导出提供者 | 封闭变体优先 enum；开放提供者按能力注册；不把内置 Rust trait 当未来外部 ABI | 重复 ID、缺失能力、启停/重启、结果晚到 |
| Newtype / Value Object | 三坐标、范围、不同版本/单位和身份 | 构造/转换维护不变量，模块内部单坐标可用明确命名的 usize；不改变补丁原语或制造反向依赖 | 非法边界、UTF-8/UTF-16、不可混用、快照身份 |
| Builder / Factory | 多参数且有组合校验的对象或真实多实现装配 | 简单对象用构造器；build 返回受检结果，不用默认值掩盖缺失前提 | 缺参数、非法组合、失败不泄漏半成品 |
| Observer / Event | 所有者提交后的带版本变化通知 | 意图与事实分开；订阅只失效派生数据，声明生命周期、释放和重入处理 | 无回写循环、无重复提交、关闭后释放 |

Repository、Unit of Work 等仅在出现对应领域存储或事务需要时评估；当前不得因此引入数据库、tokio、通用仓储层或全局事件总线。多文件写入不构成物理原子事务，必须表达逐步发布、失败保全与可恢复中间态，不能以模式名称承诺不存在的原子性。

## 5. Rust、安全与依赖

以下为必读索引，具体例外和检查沿用 Rust 规范，不新增默认豁免：

| 事项 | 执行要求 | 依据 |
|---|---|---|
| 命名/文档 | 标识符英文、注释/文档中文；公开 API 记录输入、输出、错误、不变量/坐标/版本；TODO 为 `// TODO(xian): 说明` | S-4、S-11 |
| 类型与错误 | 强类型维护跨边界不变量；非法输入受检拒绝，错误为可匹配枚举；不得以 `.ok()`/空分支吞掉仍需处理的失败 | S-5、S-7 |
| panic | 引擎含测试/示例/基准禁 unwrap；引擎生产禁 expect/panic!/unreachable!/发布期 assert!；debug_assert! 不能代替输入校验 | S-7，RS-03 |
| 所有权 | 优先借用和单一可变所有者；clone、Arc、锁和内部可变性有明确来源/生命周期及实际需要 | S-8/9/13 |
| unsafe | 自有代码默认 forbid；平台例外必须另有具体裁决、独立安全评审和验证，不以局部 allow 绕过 | S-10，RS-04 |
| i18n/日志 | 用户文案一律 `t("key")`，zh-CN/en TOML 通过 rust-embed 打包；引擎不翻译，日志不泄漏正文/隐私 | S-7/11，R-13/16 |
| 依赖/构建 | 按引擎 §18.2、架构 §3.2 准入；唯一 pulldown-cmark，锁定 GPUI 快照；工具链/edition 在 M0 取证，不复制外部版本或 lint 全表 | S-2/15 |

格式由登记环境 rustfmt 处理；Clippy 采用默认组加逐项铁律/高价值规则，保留 `-D warnings`，不整组开启 pedantic。workspace lint 必须验证每个成员实际继承，禁止全局放行坐标转换。外部指南列出的 cargo-audit、Miri 等只作按需评估工具，不自动新增当前必跑命令。

依赖禁令的唯一精确例外见现行架构 §3.2（2026-10-30 GPUI 审计裁决）：仅限锁定 GPUI 的 regex 路径及非桌面 WASI p2 绑定，五引擎隔离不变。门禁保留完整图并与实际 target/features 的 Cargo tree 交叉核对，检查所有引入路径和包身份/校验和，禁止全闭包或包名前缀放行；解析失败不能判合规。

## 6. OpenSpec、评审与交付

### 6.1 每次变更的记录

影响产品行为、依赖/架构边界、公共契约或验收口径时，先建 change；局部修复按开发流程可直接实施，但仍提供相应规范和验证记录。

| 工件/环节 | 必须记录的内容 |
|---|---|
| proposal | M 阶段、目标/非目标、受影响 crate/module、PRD/AC/V 和相关 R/S/本文条款 |
| specs | 功能规格描述可观察产品行为；工程治理规格描述可验收开发约束；均有正常与关键拒绝/失败场景，不把具体私有类型名写成产品需求 |
| design | 模块/状态所有者、允许依赖、输入输出/写入口；抽象/模式及不用更复杂方案的理由；失败、取消、过期、兼容和平台影响。无适用项写明理由 |
| tasks | 按依赖拆成可独立审查的闭环；先无头机制后 UI；关联需求/场景/规则、实际检查命令及 docs/验证记录回填 |
| 实现与评审 | 核对真实实现与设计的边界、模式和规模判断；重构删除替代路径、无消费者导出/依赖/测试替身，并检查引用及回归 |
| 交付与归档 | 列实际命令/结果、环境、人工步骤、平台缺口和例外；delta 经同步校验后归档。归档不代表 M 阶段或 V 验证通过 |

重要架构、公共 API、安全、并发/一致性或大范围重构按 AGENTS 路由做独立审查；禁止让实现者的自述代替应有的独立证据。

### 6.2 可检查的门禁

M0 建立 workspace 后，Rust 变更提交前必须执行：

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

`--all-targets` 不代表所有操作系统或所有 feature 组合通过。按风险补相关 feature/平台/性能/产物验证，具体证据回填原 V 项。

| 要求 | 验证手段/交付证据 | 当前状态与接入时点 |
|---|---|---|
| 工程规格格式 | `openspec validate --all --strict --no-interactive`；核对本文与 docs 引用 | 本次初始化可执行；仅验证规格结构 |
| 依赖边与 GPUI 隔离 | Cargo metadata 检查真实包名/别名/传递及 target/feature/dev/build 边；违规反例须失败 | 已由 `scripts/check_engineering.py` 接入并提供 `--self-test`；覆盖与本轮验证见 M0 整改记录，未宣称完整出口通过 |
| lint/unsafe/panic 策略 | 成员继承及生产/测试作用域检查；最小违规样例验证诊断 | 已由 `scripts/check_lints.py` 接入并提供 `--self-test`；覆盖与本轮验证见 M0 整改记录，未宣称完整出口通过 |
| 保真/映射/补丁/拒绝 | 独立金样、非法端点/输入、拒绝前后状态与历史检查 | M0 起随机制建立；接 UI 前先有无头证据 |
| i18n | key 集、插值一致性及用户文案来源检查 | M0 catalog 接线起建立 |
| 模块/模式/所有者 | 评审核对本文件 §2～4；记录真实调用路径与适用/不适用理由 | 即日起人工执行；行数不代替判断 |
| UI/IME/文件失败/性能 | 真事件、逐步故障、端到端/平台样本；记录未覆盖项 | 按 M0～M6 / V-01～V-09 执行，当前未完成 |

新增或修改自动守卫必须有正例和最小违规反例，证明既可正常通过又能拒绝目标违规；说明覆盖范围和盲区。不能仅凭脚本退出 0、grep 无命中或配置表存在宣称架构合规。尚无自动化时给出具体人工步骤和观察结果。

完成前逐项核对：职责/依赖是否正确，状态写入口是否唯一，类型/错误/模式是否必要且受检，拆分/保留是否有依据，关键失败/取消是否验证，文案/安全/保真是否符合规则，规范与实际证据是否一致。任何不适用项须有原因；非铁律例外按 S-1-01/R-0-01 记录原规则、最小范围、替代保证、验证和退出条件。

## 7. 参考来源与采纳边界

以下为 2026-10-30 查阅的一手公开资料；只借鉴适用原则，外部页面更新不会自动修改 Aline 规范。表中的 Aline 应用是本次工程选择，不代表原作者对 Aline 的要求。

| 来源 | 借鉴内容 | Aline 的适配与边界 |
|---|---|---|
| [Microsoft Pragmatic Rust Guidelines：UX](https://microsoft.github.io/rust-guidelines/guidelines/libs/ux/index.html) | M-BALANCED-MODULES 的模块规模与范围平衡 | 按职责和维护成本拆分，见 §3 |
| [Microsoft：Resilience](https://microsoft.github.io/rust-guidelines/guidelines/libs/resilience/index.html) | M-MOCKABLE-SYSCALLS、M-STRONG-TYPES、M-STRONG-TYPES-GUARD、M-BUILD-RESULT | 故障注入、受检类型/构造；不复制其 panic 政策或引入并发运行时 |
| [Google Rust Style Guide](https://google.github.io/styleguide/rustguide.html) | 应用按即时需求控制泛化；叶模块按核心概念命名，聚合入口精简 | 不采用 Chromium 专属单文件 crate / GN 布局，沿用八 crate 与 mod.rs |
| [PingCAP / TiKV：Modules and crates](https://pingcap.github.io/style-guide/rust/modules.html) | 模块围绕一个连贯概念、模块文档、最小 API、避免 util 堆积 | 可见性按 Aline S-3 的最窄原则；不复制其 edition/版本或 prelude 偏好 |
| [Rust Style Guide](https://doc.rust-lang.org/style-guide/) / [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/checklist.html) | rustfmt、惯用命名、转换与类型安全、公开 API 文档 | 具体 lint/工具链沿用 RS-01 与 M0 取证 |
| [Cargo workspace lints](https://doc.rust-lang.org/cargo/reference/workspaces.html#the-lints-table) | 根 lint 定义需成员显式继承 | M0 用正反例核实实际配置，不将文档当门禁 |

本地 Echo 参考范围：仓库外的 Echo 参考克隆中的 `openspec/CODE_STANDARDS.md`、`openspec/config.yaml`、`openspec/specs/engineering-governance/spec.md`（克隆不随本仓库分发）。借鉴“规范 → change 设计 → 可验收场景 → 有效门禁”的组织；其音乐库/SQLite/Tauri/Flutter、tokio、500/1000 行和覆盖率政策不适用于本项目，也不将 Echo 的实现状态当 Aline 的验证证据。规范执行不依赖 Echo 路径可用。
