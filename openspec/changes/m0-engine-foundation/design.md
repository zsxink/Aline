# Design

## Context

动机和范围见 proposal。仓库当前只有文档，无 Cargo/CI/应用实现；本设计是拟议实施方案。依据架构 §3.2/4/5/7.1/13、引擎 §5～9/11/12/18.2、Rust §2/5/7/10/12/13/19、CODE_STANDARDS §2～6 及工程治理规格。m0-task-order.md 仅提供执行骨架，不赋予产品规范效力。

## Goals / Non-Goals

**Goals:** 用实际无头消费方冻结接口、来源与身份，先证明 source 表示往返，再固定 buffer/markdown/edit 和 DocCore 协议；每组可独立审查并保留可重跑证据。

**Non-Goals:** 不创建窗口、实体视图、原生输入适配或保存协调器；不实现完整命令表、表格、主题包和插件后端。DocCore 位于 UI crate 的纯状态模块，不能以无头要求为由新增第九 crate 或把宿主编排移进引擎。

## Decisions

### Ownership and Dependencies

允许方向为 `app → aline-ui → aline-plugin → aline-edit → aline-markdown → aline-buffer`，主链可向下直接消费最窄公开 API，不强制相邻边全部存在。仅 app/ui 直接编排 source；其余层按需依赖 app-core，app-core 不反向引用业务层。source 不导入 buffer/UI，buffer 不依赖 source/markdown。GPUI 只允许 app/ui/plugin；五引擎完整依赖闭包含 target/feature/dev/build 边均无 GPUI。八 crate 骨架只创建当前消费者所需模块，聚合用 mod.rs、叶模块具名；约 500 行只触发人工职责复核，不机械拆分。

| 模块 | 唯一状态所有者 / 输入输出 | 提交与副作用边界 |
|---|---|---|
| source decode/encode/metadata | 自有不可变存储元数据；bytes/预算→源码/基线，冻结文本+绑定相等证据→bytes/错误 | 纯转换不改文档；最小 read/atomic 原语同步受检，由宿主调用，无线程/队列 |
| buffer patch/anchors/history/revision | Buffer 文本/行索引、历史节点、源码选区由 DocCore 持有 | 原子批次的底层写方法只由有权所有者调用，不给插件/子控件可写句柄；无 I/O/解析 |
| markdown pre/build/tree/blocks/protection/budget | 无独立可写文本，持不可变 Parsed 与输入身份 | 同步返回语义/映射/降级诊断，不 spawn；不携主题色或注册表 |
| edit change/check/refusal | 只读快照、Parsed、命令→Change/拒绝 | 预演和 reread 在候选上完成，不写真实 Buffer |
| UI document | DocCore 唯一持 Buffer/History、OpenBlock、输入状态、Parsed 和请求采纳状态 | apply_change 是文档写入口；负责候选状态最终重验并发布，不在 render/每键做全量解析或全文哈希 |
| app-core i18n | 实例持双语 catalog 与显式 locale；错误/缺键可诊断 | rust-embed 仅打包资源；无隐藏 locale/运行期全局；UI 映射引擎枚举到 t(key) |
| plugin / app | M0 最小编译边界及启动占位消费方 | 不预建注册框架/外部宿主、不启动窗口；M1 后才接原生服务 |

持久化编排未来归 UI 的会话/资源所有者，app 只装配。source 的受检 I/O 不承担多文件资源事务。M0 最小故障样例证明原目标保全，不承诺全平台原子保存；无真实权限复制能力证据不能把 Unsupported 当一律忽略错误。

### Environment and Admission

以下全是候选及取证流程，本提案不虚构任何最终锁定数值。

2026-10-30 apply 更新：用户批准[最小例外草案](../../../docs/validation/m0-engine-foundation/admission-2026-10-30/proposed-policy.md)范围并指示完成第 1 组。正式规则已同步现行架构 §3.2：仅 crates.io gpui-pre 0.3.9 的已审 regex 实现路径及非桌面 WASI p2 绑定可例外；记录来源/精确版本/归档校验和/边/target/features/消费者。五引擎完整闭包无 GPUI、无禁用库，自有直接/其它路径禁止。正式 workspace 可换临时候选根并增加合规节点，但受审 GPUI 子图变化须重审。metadata 必须和实际 Cargo tree 分组合交叉核对，不能因一条合法路径而忽略其它非法入边；不得扩张为包前缀或整个 GPUI 子图豁免。详见实际审计，旧候选表的“待取证”由实际证据逐项收口，不代表全阶段通过。

| 对象 | 候选 | 取证方法与准入判据 |
|---|---|---|
| Rust/Cargo/rustfmt/Clippy | 满足 GPUI 及目标要求的 stable 组合 | 在隔离候选工程记录 rustc -Vv/cargo -V/组件版本，构建支持证据，验证格式选项及 lint 正反例后登记；不自动跟最新 |
| edition / rust-version | edition 2024 候选；MSRV 候选由实际依赖约束提出 | 检查所有成员和目标；最低版本声明另跑最低版本构建，不把 rust-version 当工具链锁字段 |
| gpui-pre | crates.io =0.3.9，声明 Zed 93d745668805029476e6d7fbb19652116fed51de，包校验和/发布差异见第 1 组证据 | macOS aarch64 核心库消费方与 test-support 已编译检查；按现行架构 §3.2 的精确例外验完整图，未审 features、新路径或来源/身份漂移须重审。原生平台后端/真实 harness 事件未验证 |
| pulldown-cmark | 单解析器候选版本，关闭默认 features 后显式选项 | 分清 Cargo features 与运行期 Options，以基础/GFM/产品语法金样固定可用组合；不添加第二解析器 |
| TOML/rust-embed | catalog 所需版本候选 | 登记用途/归属/版本/许可/feature/传递图，双语解析与嵌入消费测试 |
| Unicode 字素分段 | **已批准的精确候选** `unicode-segmentation =1.13.3`，在 `aline-markdown` 增加唯一直接引擎边；该包提供 Unicode 17.0.0 数据，无运行时依赖/unsafe，锁定 SHA-256 `c6f5d3c3b1bf09027a88a6bc961fc00497d651009560b5463668dc81b0fa87a8` | 用户于 2026-11-01 批准 Unicode17 行为、Unicode18 GB9c 已知差异和查询预算方案，proposal/review 见 `docs/validation/m0-engine-foundation/remediation/R07/run-2/`；实现前仍须本 change 设计独立复核，不将 lockfile 中沿 GPUI 的传递可达当作既有准入 |
| entities/emojis、similar/proptest | 仅发现实际缺口才提出的候选 | 与现有能力对照，证明必要性及无重复库；无需求不引入 |
| ropey/资产/ZIP/导出后端 | 后续证据决定 | M0 不预装；禁 notify/regex/tokio/Wasmtime/WASI/动态宿主覆盖直接及传递所有路径，仅限现行架构 §3.2 已批准的精确例外 |

候选取证在隔离临时目录执行，保留命令/来源/hash/输出与许可证证据，不污染仓库；正式依赖仅在步骤 1 准入后于步骤 2 配置。V-01 只登记 macOS（含 Intel）、Windows 10、Linux 的目标 CPU/最低系统候选/发行版显示环境、可用设备和缺口；真实验证先 macOS，后 Windows/Linux，不凭 metadata 推断平台能力。

### Public Boundaries

| 语义 / 建议 home | 不变量及消费方 |
|---|---|
| SrcOffset/SrcRange、DocumentRevision：buffer | 跨模块分型且字段私有，受检 UTF-8 范围；markdown/edit/UI 消费；补丁底层仍为 Vec<(Range<usize>, String)>，只在受检表示边界转换 |
| DrawnOffset/Utf16Offset、ProjectionId、来源/精确性：markdown 映射模块 | 使用所属投影身份与版本转换，恒等/局部转换也经过映射服务；buffer 不导入这些类型 |
| DiskVersion/存储字节偏移、BaselineIdentity：source | 不混同源码偏移；source 只接收 &str/自有文本视图与元数据，不导入 UI 快照 |
| DocumentId、WorkingVersion、draft_generation：UI document | 文档身份不由路径哈希生成；WorkingVersion 组合单调 revision 与不复用的草稿代次；通过参数/结果信封携带，低层不导入 UI 类型 |
| Change：edit | 包含预期工作版本的纯边界信息、补丁、after 选区、分组原因、源片段/语义前提；UI 解包并复验会话身份 |

WorkingVersion 的跨层承载采用低层自有的 revision/不透明代次值和 UI 结果信封组合，不能为共享整个 UI 类型新增反向边。步骤 3 须用实际消费签名核实 home、身份作用域和受检构造；这里只冻结语义和依赖约束，不冻结名称或底层整数宽度。相等证据由受检内容比较/来源生成并绑定文本与基线，不接受无来源的 unchanged bool；未经核验的哈希只快筛。

公开接口最小集合：decode/encode；只读源码快照、受检批次/逆补丁及锚点变换；parse 与 source_at/drawn_at/aligned/UTF-16 适配；plan_change/validate_change；DocCore 的 apply_change/snapshot_current/事件入口。返回错误区分非法输入、领域拒绝、内部失败、预算、取消、陈旧及草稿冲突，均为枚举；公开文档记录坐标、身份、错误和消费方，不为测试暴露可变字段。

### Source Representation Gate

2026-10-31 用户批准原始行尾表示及存储行尾等价：StringBuffer+行索引的编辑源码剥离头部 BOM，保留输入 CR/LF 字节。CRLF 占两个源码字节，磁盘/源码转换仍由 source 负责；Markdown 不再次规范化源码。候选失败与独立演算见 `docs/validation/m0-engine-foundation/source-representation-review.md`，不代表实际 source 测试通过。

编码扫描将 CRLF 对和未配对 LF 各视为一个行尾单元，包含合法编辑在边界新形成的 CRLF；未配对 CR 是普通字符。相对存储基线确有变化时，将行尾单元按基线多数派输出，平票 LF；LF 策略下紧跟在孤立 CR 之后的 CRLF 单元保留 CRLF，以免重开吞掉孤立 CR。不增加尾换行、不更改其他字符。冻结源码 `"\rX\n"` 删除 X 后为 `"\r\n"`，LF 策略下重开为 `"\n"`，只称存储行尾等价，不称严格 String 相同。

源码补丁、逆补丁、历史、版本、选区及无变化判定始终使用实际源码的精确身份，不以存储行尾等价吞掉一次真实编辑。source 绑定实际写出 bytes、冻结源码身份与经核验的行尾等价关系；成功保存建立新基线，失败不更新，Undo 不回退磁盘身份。原 bytes 通道只由同一基线对象上的精确源码相等决定，调用方不得伪造 bool；宿主另记成功写出的冻结输入事实，不以行尾等价放行该通道。先跑空/BOM/混合/平票/正文 U+FEFF/无尾换行/非法字节金样，再跑边界删除/插入/粘贴及逆补丁；不在 Buffer 外隐式改字，不拒绝正常输入。

### Parsing and Mapping

从全量不可变快照开始：字面区识别→必要的映射改写→唯一解析器事件→带原文范围 mdast→展示/块/Gap/保护/导航→校验/Parsed。优先同一解析器识别字面区，必要时再次解析改写副本，不冻结“一次解析”口号。仅为锁定解析器无法直接表达的需求引入 pass；每 pass 复合来源关系，事件跨段保留子来源，synthetic 不伪装用户文本。

映射用来源区间加 Exact/Aligned/Hidden/Synthetic 等精确性，原样段可紧凑记录，变换段明确来源；不用字符相等从尾部回搜。源序片段有序，目录/脚注副本分别记录文本来源与导航目标；不要求全树显示顺序单调。原样精确往返、规范锚点稳定、对齐幂等、UTF-16 代理对/字素/反向选区分别测试。

#### 字素边界适配（本 change 新增）

- 唯一实现 home 为 `aline-markdown` mapping；不由 `aline-buffer` 持有分段规则，不把第三方 crate 类型泄漏到公开 API。输入/输出用绑定 `Projection` 身份的 `DrawnOffset`；Source 经 `Projection::identity`，Visual 经当前解析得到的展示投影；返回源码定位必须走现有 `source_at`/`drawn_at`。
- 仅在已验证的完整投影文本上做前进、后退和边界查询，拒绝旧投影、非法 scalar 端点、非字素边界起点；不得截取孤立子串并假装拥有完整 RI/Indic 前文。此机制只给光标移动边界，不证明删除选区来源完整或绕过 protection。
- 按每次查询最多 4096 UTF-8 字节、每块至多 256 字节约束游标供给。预算累计计算每次主动 chunk 及 `PreContext` 回扫实际交给库的字节，重复供给重复计费；达到预算仍未确定边界时返回类型化 `ScanBudgetExceeded`，不返回推测偏移。该数字是供给字节计费上限，不等同于库内部精确访存量、CPU 时间或发布性能保证。
- 锁定 Unicode17 行为。U+094D U+0915 UTF-8 字节的预期边界为 `[0,3,6]`；Unicode18 / UAX #29 Rev.49 GB9c 变更后的预期为 `[0,6]`。若 V-07 最终要求 Unicode18/当前 UAX 行为，必须另行准入新数据版本并更新金样，不沿用本批准声称满足。长 RI 上下文、超长组合簇、Indic、双向移动、非字素起点和预算拒绝均需独立预期测试。

有限 HTML 保留样式语义及 HTML 身份，注释仅隐藏显示；支持标签、引号内 >、交叉嵌套、空白来源、实体和过滤标签各有独立金样。未闭合不吞尾。MD-11 包括脚注引用子区间，不能锁死同段普通正文；HTML 原文可编辑而格式命令拒绝。

M0 建全量正确路径及来源相等的头尾配对最小验证；任何复用必须检查配置/全局语义依赖并重定位后块范围，与全量结果比较树/Gap/映射/保护/导航。允许未优化部分留后续，不以全回退样本冒充优化命中。

### Commit and Input Protocol

1. DocCore 接收意图和显式时间，读取 Buffer+至多一个草稿组成的当前一致视图；不每键复制全文。
2. edit 先做可判定预检，在候选状态中准备待定组处理、计划、预演和 reread，目标 after 选区/保护/预算全部验证。
3. 所有者复验 DocumentId、WorkingVersion、投影/配置及预期片段后，一次发布补丁、行索引、选区、正逆历史与新 revision；更新保持打开的块基线。低层失败不能泄漏半成品。
4. 拒绝相对调用前完整状态零副作用；源码定位是随后独立意图。无变化不记步、不清 redo。
5. 相邻普通输入按 900ms 分组，时间由宿主注入；提交不必关块，离块仅处理剩余组。Undo 优先待定组，Redo 恢复选区，实际新编辑才截 redo。重载是独立可撤销边界，基线不随 Undo 回退（TA-02）。
6. snapshot_current 冻结最新草稿/工作版本/选区/可达资源引用，不提交/封口历史。资源在 M0 仅表达引用所有权，不实现图片迁移/清理。

2026-10-31 裁决：组 8 在 M0 实现最小资源引用所有权表示并以独立期望和测试验证；初始打开范围等于排序后的选区范围。`SnapshotId` 是快照实例或内容视图的来源身份，可以跨工作版本复用，相同文本也不保证相同 ID；采纳派生结果须同时核验文档、工作版本和实际依赖的其他身份。具体表示、所有者及生命周期以实现和独立复核证据为准，不将此裁决视为资源迁移/清理通过。

资源引用允许由宿主在引用生效前登记，并在文档生命周期内保守持有已登记的引用；冻结快照独立持有其所需引用，即使文档关闭亦不提前释放。此方案依赖宿主完整登记，不等于从 Markdown 自动发现资源或精确计算历史可达集；实际资源协调、迁移与清理仍须后续验证。

组合状态采用显式 enum/转移：Idle→Composing→确认独立组或取消恢复前态；每次更新/取消推进代次避免旧投影复活。显式发布 Idle 时取当前快照；Composing 时进入可取消等待，平台自然确认/取消后取结束后的当前快照（ED-01），取消等待仅撤请求。自动保存延后，恢复保护当前输入是独立协议。M0 只用事件驱动验证，平台 adapter、发布/保存排队与真实焦点留后续。

### Failure and Result Admission

| 分支 | M0 行为 | 后续边界 |
|---|---|---|
| 非法输入/范围/投影 | 类型化拒绝，保留文本/草稿/组合/选区/undo/redo | 不 clamp，不靠 debug_assert 替代输入校验 |
| 扫描/深度/节点/输出/映射超限 | 分配前及循环中检查，可靠局部原文或整体源码回退 | 数值待 V-06，测试用注入小预算不宣称发布限额 |
| 预演/历史预算失败 | 发布前终止候选，真实状态不变 | 不 catch_unwind 代替正确性，无通用 UI panic 回滚 |
| 取消/关闭 | 取消信号检查并结算，释放派生输入引用 | 无线程状态驱动不等于后台必已停止 |
| 陈旧解析/旧投影/配置变化 | 验存活目标和身份键，不采纳到当前状态 | Undo 回同文本不复活旧请求；待新 Parsed 时留源码入口 |
| I/O 失败/实际成功回执 | 最小原语保全目标并返回分类；成功事实绑定实际冻结 bytes | 不丢实际写盘回执，不把新输入标已保存；平台竞争/权限/资源事务属 V-08 |

所有引擎无线程、无锁/执行器/运行期全局；source 允许其职责内受检 I/O。循环接收预算/取消输入，不读全局时钟。第三方长调用先限输入并登记无法中断的成本边界；UI 后续安排任务。隐私日志不含正文/候选/敏感路径。测试模拟不证明真实 IME、平台替换或资源限额。

### Abstractions and Review

| 问题 | 最简单方案 / 取舍 | 代价与验证 |
|---|---|---|
| 纯规则与宿主副作用隔离 | 具体函数/数据加 UI 编排，采用 Functional Core 边界；不加 Service/Repository | source I/O 单独测；五引擎公共 API 无头测试/闭包守卫 |
| 坐标与版本混用 | 私有字段 newtype/受检构造；身份信封按层组合 | 有边界样板和转换成本；编译消费/错类型/错身份与半字符反例 |
| 原子命令与唯一历史 | Change+候选预演的 Transaction Script，沿用正逆补丁 | 候选内存成本；末项失败、预演失败、状态全量对比 |
| 输入/组合/发布等待交错 | enum 状态与显式事件，时间注入；不先用 typestate 或通用事件总线 | 转移覆盖成本；确认/取消/重复事件/快照/Undo 交错 |
| Buffer 可替换与磁盘故障注入 | 既有最小 Buffer 边界+StringBuffer；文件原语仅在真实故障点设小接口/fake | 不预装 rope；证明实际消费者与故障注入价值，不为凑实现造第二后端 |
| 解析派生复用 | 先全量，再受检头尾配对，普通具体函数 | 首版成本较高；差分及确实命中复用/回退的独立样本 |

没有需要开放提供者/构造器复杂组合的 M0 需求，因此不新增 Registry、Factory、Builder、数据库或外部 ABI。模块按状态生命周期拆分：source 编解码、buffer 补丁/历史、markdown 映射/语义保护和 UI 输入编排可独立验证；不为行数拆开同一不变量。公开 API 列消费方，实施前审查唯一所有者及写调用路径。

## Risks / Trade-offs

- GPUI 候选闭包违反禁令 → 步骤 1 挡住相关骨架接线，提交精确路径及裁决需求，不能放行传递依赖。
- 原首选 LF 表示边界不自洽 → 已批准原始行尾与限定存储等价；步骤 4 仍须独立金样验证边界与基线，不能将分析演算当实现证明。
- 无头模拟遗漏真实焦点/IME/文件竞争 → M1/M2 交接明确实验缺口，所有平台声明分项。
- 扫描退化/映射来源丢失 → 预算/取消检查、独立敌意金样、原文回退，不截断。
- lint 存在但未生效 → 每成员正反例与 metadata 检查，宏/第三方/运行期语义盲区由人工审查。

## Migration Plan

当前无运行数据或兼容 API，无用户数据迁移。后续按 tasks 九组小步实施，每组先测试、守卫反例在隔离夹具运行后恢复，失败不合入依赖任务。冻结接口有变先更新本 change 并评审；不得静默修改 TA/ED/RS/UI/PL/TH。回退只撤本次新工程配置/实现，保留金样和失败证据，不触碰用户文档。M0 出口完整取证并回填后才交接 M1，当前只提交规划供评审。

## Open Questions

待确认问题：无。实验待定项为具体工具链/edition/MSRV/GPUI/依赖版本与平台补丁、已批准原始行尾表示的实际编码与身份验证、有限 HTML/SoftBreak 等尚待 V-07 固化金样及预算数值。它们均有先决任务和准入判据；影响已确认行为时升级冲突并暂停依赖实现，不擅自选降级产品方案。
