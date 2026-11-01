# M0 独立复核整改任务表

创建日期：2026-11-01。用途：供后续会话自主分批实施、调用 Sub-agent、验证和交接。**本文件是整改执行清单，不是新的产品/架构规范，也不代表任何修复已完成。**

## 1. 执行入口与当前状态

仓库：`/Users/xian/Project/editor/Aline`。原复核基线：`2f1dcea46d5998f4e2ba213e73c11de166f943c4`。

原复核结论：18 项 finding（6 阻断、8 严重、4 一般）；原 46 条任务为 26 已核实、15 证据不足、5 与事实不符；M0 出口未闭合，尚不能进入依赖该出口的 M1 接线。原测试 312 passed 与这些缺陷同时成立。

| 运行状态 | 当前值 |
|---|---|
| 当前批次 | R07 已完成（`f87d2f0`）待独立复核；R05b、R06 两个 worker 进行中；R03 待独立复核；R02 复核返回 APPROVE_WITH_CONCERNS（既有 F-01 残留登记 R02b） |
| 下一可执行批次 | R09（依赖 R02/R03，均已就绪）；R05b、R06 返回后先由主 Agent 验证再复核 |
| 已完成批次 | R00、文档保全提交 `c7232ed`（任务表 + R00 证据 + R01–R14 契约，未触碰源码）；R01（`79c3b54`）；R02（`0820e8e`）；R03（`128954e`）；R05（`e4a6af0`，复核退回待修复）；R07（`f87d2f0`，仅测试）；R11（`ac2baaa`，只读取证，未改代码） |
| 活跃写入 Agent / 文件所有权 | 无活跃写入。R02 已收束（`crates/aline-markdown/src/pre/rewrite.rs`、`crates/aline-markdown/tests/rewrite.rs`）；R03 已收束（`crates/aline-markdown/src/{reuse.rs,lib.rs}`、`tests/reuse.rs`）；R07 已收束（`crates/aline-markdown/tests/mapping.rs`）；R05 已收束（`scripts/check_lints.py`）。R06 将独占 `scripts/check_safety.py`；R05b 独占 `scripts/check_lints.py` |
| 最近验证与审查 | R01 独立复核 APPROVE_WITH_CONCERNS（无阻断）；其 4 项 finding 逐条复核均成立，第 1 项已按 §5 追加勘误。R02 独立复核 **APPROVE_WITH_CONCERNS**：目标达成，但发现**既有 F-01 残留**（第二层空来源片锚定到 Aligned/Hidden 时返回非空 source），主 Agent 已独立复现并存探针输出，登记为 R02b；另有提交信息计数错误（称 3 条实为 4 条）与 2 条新用例缺修复前记录，已追加勘误。R05 独立复核 **REQUEST_CHANGES**：2 项严重（引号点分键与真实嵌套表撞名导致守卫 exit 0 而 Cargo 报 `unused manifest key`；键集/类型/值三条拒绝路径无 self-test 断言，弱化实现仍 exit 0），主 Agent 均已独立复现（`/tmp/r05f4`、`/tmp/weakrepo`），退回 R05b。R03 由主 Agent 实现并自验（23 条用例、全 crate exit 0、clippy/fmt 0），**待独立复核**；`check_public_api.py` 在隔离空缓存下不可用、换真实 `CARGO_HOME` 后 8/8 通过，已记录归属。R11 由主 Agent 直接执行并复核：17 条新命令均含 argv/cwd/起止 UTC/exit/原始输出；敏感标识已打码，全目录扫描无残留。R07 由主 Agent 实现并自验：9 行手写夹具（含 CRLF 未跳过）、接受集合恰等于标量集合、差集逐元素、三处临时变异均变红 exit 101 后还原（sha256 一致）；`cargo test -p aline-markdown` 12 段全 ok 合计 118 passed，clippy/fmt exit 0，**待独立复核** |
| 契约就绪 | R01–R14 共 14 份 `run-1/brief.md` 已全部落盘（R00 已完成无 brief）；R13/R14 契约本轮新写，分别覆盖 F-12/F-17/F-18 与全量门禁+出口独立裁决 |
| 待裁决事项 | T-8-17（undo 后新草稿时 Buffer redo 分支与草稿 redo 的失效时序）已按「受检拒绝」实现，仍待独立复核确认；R04 契约另含 tasks.md「redo 存在时拒绝」与 design.md:87/§9.3「实际新编辑才截 redo」的契约冲突，须带引文裁决并登记 |

启动或恢复时先读本节和任务总表，执行 `git rev-parse HEAD`、`git status --porcelain`，记录实际状态。不得 reset、覆盖用户修改，或因 HEAD 变化从头重做已完成批次。未完成批次先恢复，不重复初始化。

可选参考：[原完整复核报告](/tmp/aline-m0-independent-review.pTFf8u/report.md)。其同目录 A/B1/B2/C/D/E 是分项报告与原始产物，`C/independent_tests.rs`、`C/repro.rs`、`C/rewrite_repro.rs`、`root/reuse_probe.rs` 为原复现源。**这些是本机临时路径，不保证长期存在；本表已保留各问题的最小事实，缺失时重新复现，不补造旧运行。**

本表只负责安排工作；旧报告结论仍须由实际代码、原始日志和新复现核实。原 [OpenSpec 任务表](../../../openspec/changes/m0-engine-foundation/tasks.md) 暂不因创建本表而改勾选，最终在 R14 逐条重新裁决。

## 2. 自主分批与上下文管理

1. **持续推进。**主 Agent 选择前置已满足的最小职责闭环，完成后更新状态并继续下一批；不为每次换批次请求用户确认，也不同时展开全部任务。必要时把某批拆成 `R04a/R04b` 等子批，保留原 finding 归属和出口，不因拆批降低验收。
2. **规范先行。**按 [AGENTS](../../../AGENTS.md)、[执行规范](../../../openspec/CODE_STANDARDS.md)、[工程治理](../../../openspec/specs/engineering-governance/spec.md)、[开发流程](../../development-process.md) 工作，再读当前批次涉及的 change 与现行专项章节。代码定位先用 CodeGraph。不要把所有文档/证据一次塞进主会话。
3. **主 Agent 负责调度与最终验证。**持有本表、当前批次摘要、所有权表和待解决问题。实现交给合适的 worker，独立审查交给未参与对应实现/证据编写的 reviewer；主 Agent 核对实际 diff、关键断言、命令与原始输出，不能仅转述“通过”。
4. **按 AGENTS 路由。**scout 只定位和收集事实；simple_worker 负责明确的局部低风险修改；complex_worker 负责状态机、映射、跨模块及方案选择；deep_thinker 用于重要架构/安全或无法可靠裁决的分析；reviewer 做独立审查。不为简单任务机械拆 Agent，不靠提高 Luna effort 代替升级复杂工作。
5. **限制并发与写入范围。**默认一个 worker 实现，结束后一个 reviewer 审查；最多同时运行两个 Sub-agent。确实独立且文件不重叠才并行。派发时明确文件所有权，告知 worker 不是唯一写入者、不得回退其他人的改动。Cargo 全量验证由一个负责人串行调度，避免重复构建和日志混用。
6. **Sub-agent 使用小上下文。**工具支持时用 `fork_turns="none"`，只传批次契约、文件路径、适用规范与验收。不得继承全部会话、整份复核和所有历史日志。禁止递归派发 Sub-agent；继续相关局部工作可复用原 worker，进入无关批次则使用新 worker。
7. **返回摘要，不倾倒输出。**每个 Agent 返回不超过约 800 个中文字：修改文件、根因/方案、验证结果、未解决问题、产物路径。完整分析、长日志、源码和命令记录落盘；主会话仅按需读取具体片段。
8. **落盘恢复。**每批在第 5 节规定的目录留下短 `handoff.md`，更新本节状态和任务表。上下文压缩后只恢复当前状态、当前批交接与必要规范，不重新读取整个历史或重新规划全部任务。Agent 中断时先检查已写 diff/日志，再续派剩余工作。
9. **失败有处理闭环。**审查未过返回同批修复；同一根因连续两轮未解决，主 Agent 汇总最小失败事实并按路由升级分析，避免反复启动同类 Agent 重读全仓库。只暂停真正依赖用户裁决的分支，其他独立批次继续。

## 3. 任务总表

状态只用：`待开始 / 进行中 / 待审查 / 完成 / 待裁决`。完成必须有第 5 节要求的实际证据、适用审查和主 Agent 验证，不以 worker 自述完成。

表中路径为定位入口，不是已批准扩大公开面的理由；具体写入范围由主 Agent 在派发前登记。下述先后只控制依赖，后续会话可以合理细分和调整独立批次顺序。

| 批次 | Finding / 单一闭环 | 前置 | 推荐执行与审查 | 主要文件/范围 | 完成标准 | 状态 / 交接 |
|---|---|---|---|---|---|---|
| R00 | 恢复基线、建立运行与所有权记录 | 无 | 主 Agent；定位不清时 scout | 本表、当前工作树、当前 change 状态 | 记录实际 HEAD/改动；识别已有整改；建立证据目录；不把原任务全勾选当通过 | 完成 → `docs/validation/m0-engine-foundation/remediation/R00/run-1/handoff.md` |
| R01 | F-01/F-05：待定组 Redo 输入保全与方向选区 | R00 | complex_worker → reviewer | UI document 的 doc_core/open_block、typing 测试 | 两种独立失败回归修复；不吞草稿，恢复对应 anchor/head，分支与分组符合现行契约 | 完成 → 实现 `79c3b54`；证据 `docs/validation/m0-engine-foundation/remediation/R01/run-1/`；复核 `review.md`（APPROVE_WITH_CONCERNS）；T-8-17 待裁决 |
| R02 | F-03：RewriteMap 多阶段来源复合 | R00 | complex_worker → reviewer | markdown pre/rewrite、rewrite 测试 | 恒等复合保留每片；Exact/Aligned/Hidden/Synthetic 与跨片段来源/锚点正确 | 完成 → 实现 `0820e8e`；证据 `docs/validation/m0-engine-foundation/remediation/R02/run-1/`；复核 `review.md`（APPROVE_WITH_CONCERNS，疑点 C 已裁决：不存在片段互不重叠不变式）；**R02b 待办**：既有 F-01 残留（合成片获非空 source），须在 R04/R10 前处理 |
| R03 | F-04：复用重定位的受检坐标边界 | R00 | complex_worker → reviewer | markdown reuse、公开导出及消费者/编译测试 | 旧/新快照身份与 UTF-8 边界受检；无裸公共坐标捷径；旧消费者和替代路径清理 | 完成 → 实现 `128954e`；证据 `docs/validation/m0-engine-foundation/remediation/R03/run-1/`；**待独立复核**（本批由主 Agent 实现） |
| R04 | F-02：edit→DocCore 消费与 Visual 权限 | R01/R02/R03 | 必要时先 deep_thinker 形成最小方案，再 complex_worker → reviewer | UI document、edit 计划/验证及必要内部依赖、宿主集成测试 | 实际受检消费链；普通/组合输入保护一致；Source 合法路径保留；拒绝完整状态不变；每键不全文解析 | 待开始 |
| R05 | F-08：lint 继承真实解析 | R00 | complex_worker 或范围明确后的 simple_worker → reviewer | check_lints 及对应隔离夹具 | 多行字符串假继承被拒；真实继承通过；Cargo 实际诊断与声明检查一致 | 复核退回（`review.md` REQUEST_CHANGES，2 严重已由主 Agent 复现）→ **R05b 修复中**：点分键/表头撞名须 fail-closed；补 3 条 self-test 反例；RecursionError 纳入异常元组 |
| R06 | F-09/F-15：安全扫描范围有效 | R00 | complex_worker → reviewer | check_safety、必要 safety_lex 与夹具 | 目录外实际 Cargo target 受审或被明确拒绝；空根/缺成员非零；正常范围恢复通过 | 待开始 |
| R07 | F-10：字素边界独立验收 | R03 | complex_worker → reviewer | 映射/字素纯边界及测试；依赖如需新增先准入 | 独立字素移动边界与标量/代理对分开验收；不以字符串含 ZWJ 冒充覆盖，不静默下放 scenario | 完成 → 实现 `f87d2f0`（仅测试 +293 行，`src/**` 零改动）；证据 `docs/validation/m0-engine-foundation/remediation/R07/run-1/`；9 行手写夹具、差集逐元素、三处变异自证变红；**待独立复核**（本批由主 Agent 实现）；引擎级字素适配留待准入 + V-07 金样裁决 |
| R08 | F-11：删除/粘贴/HTML 跨界负例 | R04 | simple_worker 或 complex_worker → reviewer | edit protection/refusal 与必要 DocCore 集成测试 | 空删除、替换、粘贴意图及左右/整段 HTML 跨界金样；比较真实完整状态 | 待开始 |
| R09 | F-07：E3 同夹具可审查 dump | R02/R03 | simple_worker 或 complex_worker → reviewer | markdown dump/Display、dump 测试/独立预期 | 同一 basic 夹具包含文字、来源片段、precision、源码身份、块/Gap、保护与诊断 | 待开始 |
| R10 | F-06：独立预期与探针身份勘误 | R02/R03/R07/R09 | 未参与对应实现/预期编写的 reviewer 独立复算；文档由另一负责人整理 | mapping 金样、独立计算原件、新证据与现行报告勘误 | 外部消费和独立计算分开；保全当前独立预期/输出；旧 TSV 创作来源无法追认就明确保留，不造历史 | 待开始 |
| R11 | F-13：候选/工具/设备采集补证 | R00 | scout 收集事实，主 Agent 或 simple_worker 补采集 | 保全候选输入、工具/设备命令及新证据 | argv/cwd/时间/输入身份/原始输出齐全；无法复现旧环境明确登记，只称新证据；不擅装工具或升级依赖 | 完成 → 证据 `docs/validation/m0-engine-foundation/remediation/R11/run-1/`（`ac2baaa`）；17 条命令全 exit 0；旧缺口 105/14 已登记；U-1～U-9 不可复现项已列；待独立复核 |
| R12 | F-16：嵌套 Cargo 锁定与口径 | R05/R06 | simple_worker；涉及采集协议时 complex_worker → reviewer | 相关工程脚本、实际命令与说明 | 初始生成 lock 与后续 locked/offline 检查分列；嵌套真实 argv 可核；不能用离线环境冒充 locked | 待开始 |
| R13 | F-12/F-17/F-18：现行状态与历史勘误一致 | R01–R12 | simple_worker → 独立文档核对 | AGENTS、docs 入口、工程基线/专项/流程、适用报告 | “无 workspace/守卫未启用”按事实更新或标历史；31/32勘误；scope 偏差登记；不重写 Git 历史、不提前宣布 M0 完成 | 待开始 |
| R14 | F-14及全部 findings：全量验证与出口独立裁决 | R01–R13 | 统一验证负责人；新 reviewer 分范围审查，主 Agent 汇总 | 当前全部实现/新证据、原46任务、E1–E7与现行回填 | 18项处置表、46任务三态表、全部Requirement/Scenario及E出口有证据；适用全量门禁；独立审查保全原始输出；如实给出M0/M1结论 | 待开始 |

允许的并行例：R01 与 R02、R05 与 R06，在实际所有权无重叠时最多二选二。R02 与 R03 同属 markdown，只有确认导出/坐标/测试辅助文件无重叠时才并行。R04 涉及多个层级，默认独占相关写入范围。不能把所有前置为 R00 的行同时启动。

## 4. 每项问题的最小事实与验收提示

### R01：Redo 与方向选区

- **F-01（阻断）**：`x → 输入a并提交 → Undo → 输入b未封口 → Redo`，原实测 `xb / draft b / redo_depth1` 变成 `xa / draft None / Ok`。新输入必须保全，redo 分叉按引擎 §9 的既定语义处理；不能为了通过测试临时选一个无规范依据的行为。
- **F-05（阻断）**：`x → 在末端输入abc → set_selection(4,2) → Undo → Redo`，原选区变成 `(4,4)`。保存和受检恢复对应 after 选区，覆盖正/反向及非折叠情况。
- 两者先有失败回归，再修复；检查文本、草稿、Buffer、选区、版本、undo/redo，保持单历史、快照不封口和无空历史步。

### R02：多阶段改写

- **F-03（阻断）**：`abc` 在1插入 Synthetic `!` 得到 `a!bc`，再复合 identity。原三片 Exact/Synthetic/Exact 被压成 `source0..3 → rewritten0..4 Synthetic`；`b` 从 `Exact1..2` 变 `Synthetic0..3`。
- 按两层片段边界求交复合；不能以宽并集或调用同一实现生成预期来验收。合成的空来源锚点和各合法端点需独立比较。

### R03：公开坐标

- **F-04（阻断）**：旧 `"😀\n"`，新 `"x\n\n😀\n"`，旧 `src_offset(1)` 拒绝，但 `ReusedBlock::translate(1)` 返回4，新 `src_offset(4)`仍拒绝。入口原为裸 `usize → Option<usize>`，范围访问也无快照身份。
- 遵循 RS-02，绑定来源/目标快照、校验 UTF-8；增加错快照/半字符/越界与外部类型负例。不要仅在调用方补二次检查或预装无消费者 API。

### R04：文档所有者实际消费

- **F-02（阻断）**：默认 Visual 文本 `"---\nt: x\n---\n"`，偏移7输入Y并提交，原实测都成功，得到 `t: Yx`。普通/IME提交直接 Buffer.apply，UI 没有实际消费 edit 的 plan/validate。
- 先追踪原有公开边界和唯一写入口，说明最简单可行方案/代价/验证，再实施。真实 DocCore 测试须覆盖保护/HTML、Source允许、待定组/组合/redo存在时拒绝；仅 Boolean EditContext 不足以证明宿主集成。
- 禁止每键全文解析、引入第二份可写全文或第二历史。需要改变公共契约/已确认策略时先按开发流程处理，不用本表授予例外。

### R05/R06：守卫必须能拒绝真实违规

- **F-08（严重）**：成员 description 多行字符串中放假 `[lints]`、`workspace=true`，实际没有继承，check_lints 仍成功。以真实 Cargo 解析和未文档化公开函数的诊断作伪/真继承对照。
- **F-09（严重）**：实际八成员副本将 buffer `[lib] path="../../outside.rs"`；该文件含生产 unwrap 及局部 Clippy allow。原安全守卫只扫成员目录，遗漏真实 target；扫描和 Clippy均exit0。实际 targets/build-script 入口与模块路径必须纳入受审范围或明确拒绝。
- **F-15（一般）**：安全扫描根不存在仍 exit0，`failures/scanned/member_reads`均空。根、八成员集合与扫描范围应受检；不声称原默认正常仓库已经空跑。
- 违规只放临时副本；成功拒绝需具体规则/路径，不能把 Cargo 环境失败或任意非零当正确拒绝。正常 workspace 与已有有效反例不得回归。

### R07/R08：scenario 不能被“已登记未测”代替

- **F-10（严重）**：markdown“重复文字与字素”要求字素移动与标量边界分开验证；现有 ZWJ/组合字符只验来源、UTF-8/UTF-16。先明确纯边界和独立期望，不以原生 UI 尚未接线跳过 M0 scenario；若确需改 scope，列冲突走规范流程。
- **F-11（严重）**：7.3 跨脚注仅一种非空 Replace；缺空删除/独立粘贴，HTML 格式缺右端/整个元素跨界。用现有纯命令补表驱动负例，无需提前做剪贴板 UI。

### R09/R10：dump 与独立性

- **F-07（严重）**：原 basic dump25行只有 `config/source/drawn/block/gap/protected`；`source319/drawn197`是长度，没有实际文字/来源关系/precision/源码身份。补同夹具完整产物和独立预期，不能拿另一夹具测试替代 E3。
- **F-06（阻断）**：6.4报告称“独立工具复算”，实际仓外探针调用同一 parse/segments/source_at；旧 Python 重算原件未保全。纠正该表述，保全当前独立推导/工具/原始输出。不能追认旧 TSV 的创作方式或补造先行时间。

### R11/R12/R13：历史与当前事实分开

- **F-13（严重）**：早期候选 safety-build/clippy/fmt/negative-edition、部分 tree 命令缺时间/cwd；环境/设备观察缺完整采集链。新运行可补当前证据，不能恢复未保全的旧事实；缺输入/设备时明确证据不足。
- **F-16（一般）**：check_lints 等嵌套 Cargo 命令只有 offline，部分旧统一命令只有环境 offline。生成初始 lock 与后续检查分开；检查最终真实 argv，不能泛称全程 locked/offline。fmt等不适用的参数不机械添加。
- **F-12（严重）**：AGENTS/README/CODE_STANDARDS/Rust/流程仍有无历史限定的“当前无 workspace/配置/CI、守卫未启用”。同步当前状态，区分 CI 配置存在和远端已运行；保留明确历史语境。
- **F-17（一般）**：4.1报告第33行31例，实际32例；追加勘误，原始输出不改。
- **F-18（一般）**：如 `16f8e30` 仅修改 tasks 勾选却用 `docs(workspace)`，与纯文档 scope约定不符。登记已有偏差，后续遵守 RS-05，不重写历史或专为此制造提交。

### R14：最终审查有自己的原始证据

- **F-14（严重）**：旧9.6独立运行只有主 Agent 转述/观察记录，原始 stdout/stderr未独立落盘。本轮独立审查者须实际检查实现与原始证据，执行所需检查并保全输出；不能用旧review结论替代。
- 审查覆盖全部18项、原46任务、六新增capability的16 Requirement/44 Scenario及修改的工程治理delta；允许分范围审查，但不得把抽样升级为全量。主 Agent负责去重、追踪漏项及最终裁决。

## 5. 每批执行与证据协议

新证据放 `docs/validation/m0-engine-foundation/remediation/<批次>/<运行编号>/`。目录已存在就新增运行编号，不能覆盖。旧原始日志、旧失败、旧金样及提交历史保留；现行报告可追加清楚的勘误和新证据链接，不伪装为当时记录。

每个实现批次至少具备：

- 修改前最小失败复现（适用代码缺陷时），修复后回归与相关测试发现清单；预期来自契约/独立推导。
- 实际 argv、cwd、必要环境、起止时间、退出码、原始 stdout/stderr、受测输入/代码身份。未提交改动记录 HEAD及相关diff或文件指纹，不能仅写一个不能覆盖工作树的SHA。
- 受影响规则/Requirement/Scenario、公开消费者、唯一所有者与失败/兼容边界。新增依赖依现行准入，不从本表推断许可。
- 重要批次独立 review 的人员/Agent职责与产物，指出其未参与对应实现；主 Agent 的 diff/断言/原始输出核对记录。
- `handoff.md`：建议不超过80行，只写已做事项、修改文件、验证结果/路径、审查结论、剩余问题、下一步；不粘长日志。

每批适用检查按 Rust规范执行。日常先跑当前回归和受影响crate测试/Clippy、格式与diff检查；**每次准备提交Rust变更仍执行仓库规定的完整门禁**，不得以本表免除提交检查。未新增变化、失败或疑点时，不在多个 Agent 重复同套全量运行。R14对最终状态统一取证。

使用登记工具链1.98.1与`CARGO_NET_OFFLINE=true`；涉及Cargo解析/构建/check/clippy/test显式`--locked --offline`。初始新夹具生成lock独立登记；工具缺失/离线失败记未核实，不悄悄联网安装。自测先审清写入位置，在受控新目录/临时副本生成夹具，不覆盖历史证据。

批次完成检查：

1. 该批验收逐项对应实现/测试/证据，原失败原因已消除；如 finding 不成立，给出可复现反证与规范依据，不能直接删项。
2. 适用检查真实通过，独立审查问题已处理；主 Agent 验证结果。
3. 更新任务表状态/交接路径及第1节当前状态；进入下一批，不能只写“代码完成”而遗留验证。
4. 无法验证旧历史时可完成“准确勘误/补证工作”，但原任务或出口仍证据不足就保持该判定；整改批完成不等于 M0 已闭合。

## 6. R14 全量门禁与最终产物

在最终代码/文档状态执行并保全：

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked --offline -- -D warnings
cargo test --workspace --locked --offline
python3 scripts/check_engineering.py --self-test
python3 scripts/check_lints.py --self-test
python3 scripts/check_safety.py --self-test
python3 scripts/check_public_api.py --self-test
openspec validate m0-engine-foundation --strict --no-interactive
git diff --check
```

另阅读当前CLI后运行实际统一检查入口及正常workspace依赖/lint/safety/public-api/catalog检查，记录真实参数和输出；不能只运行隔离self-test。守卫须同时有正常范围通过、每个新增违规明确拒绝、移除违规恢复通过的证据。代码变化后按开发流程`codegraph sync/status`，确认无遗漏源文件。

最终交付文件放新证据批次，并从本表填入链接：

| 交付物 | 必备内容 | 当前产物 |
|---|---|---|
| 18项处置表 | finding→根因→修改→失败回归→通过证据→独立审查→残留限制 | 待生成 |
| 原46任务重新核对表 | 每条已核实/证据不足/与事实不符，逐条证据；据实维护原tasks，不批量勾选 | 待生成 |
| Requirement/Scenario矩阵 | 全部对应实现与测试，不能把未测登记算覆盖 | 待生成 |
| E1–E7出口表 | 每项全部要素、证据、未闭合原因 | 待生成 |
| 最终命令包 | 实际统一入口及各门禁、环境、逐条exit/原始日志、新增测试清单与相对原312的差异 | 待生成 |
| 独立复核 | 真实范围、命令/原始输出、findings及关闭依据，不采信旧转述 | 待生成 |
| 最终结论 | 分别回答M0是否闭合、M1能否接线、阻断数、平台和完整AC/V状态 | 待生成 |

真实IME/焦点/窗口、Intel Mac/Windows10/Linux、远端CI、Release性能/发布限额、现有文件安全替换/文件协调、资源迁移清理等未执行项继续显式登记。macOS无头、all-targets、create-only发布与引用保活不证明上述能力。AC基础不升格完整验收；V-01～V-09不因本轮修复自动整项通过。

原46任务仍有证据不足/事实不符或阻断未关闭时，不能宣布M0出口闭合。只有前置出口按规范满足后才可提出依赖它的M1接线；本任务表不授权自动实施M1或归档change。
