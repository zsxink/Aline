# 原 46 条任务逐项复核

复核对象：`openspec/changes/m0-engine-foundation/tasks.md` 中原始任务 1.1–9.6。维护任务 §1 记载“原 46 条”；当前 OpenSpec 清单实有 47 项，因为 R07 后追加的 6.9 不属于原 46 条，本表不将其混入计数。任务描述及原始发现背景取自 `docs/maintenance/m0-remediation/tasks.md` §3/§4；现有勾选仅作为定位，不作完成证据。

状态只用“已核实 / 证据不足 / 与事实不符”。“已核实”仅表示现存证据支持该子任务的可核验内容，不表示相关 Requirement、Scenario、M0 出口或平台验收整体通过。当前没有取得原始 46 条任务的完整原审计逐项映射，因此不以 §1 的历史汇总计数（26/15/5）倒推任何一行；下表是本轮按留下的现行证据逐条重新裁决。R14 统一门禁尚在进行，凡依赖其最终结果的完整出口任务不预判通过。

| 编号 | 三态结论 | 证据路径与结论依据 |
|---|---|---|
| 1.1 | 已核实 | `docs/v1-validation-plan.md` 的目标/平台缺口记录及 `docs/validation/m0-engine-foundation/remediation/R11/run-3/report-addendum.md`、`R11/run-4/review.md` 的设备/工具新采集相互支持；只核实目标登记，未核实平台支持。 |
| 1.2 | 证据不足 | `R11/run-3/report-addendum.md` 与 `R11/run-4/review.md` 只核实本机工具链/target 新采集；`R11/run-1/handoff.md` 明确候选构建/MSRV/相关构建仍未跑，不能证明隔离候选组合及不兼容诊断全项。 |
| 1.3 | 证据不足 | `docs/engine-design.md` §18.2、`docs/v1-architecture.md` §3.2 与 `R11/run-4/review.md` 核实现行依赖准入措辞；未见覆盖所有目标与 feature 组合、账本字段和全部负例的原始证据。 |
| 1.4 | 证据不足 | `docs/validation/m0-engine-foundation/remediation/R11/run-4/review.md` 核实 R11 文档勘误；`docs/validation/m0-engine-foundation/remediation/progress.md` 显示最终全 workspace 闭包/门禁仍待 R14，不能据此核实任务要求的实际五引擎闭包及同步审查全项。 |
| 2.1 | 已核实 | `Cargo.toml`、`Cargo.lock` 与 `docs/validation/m0-engine-foundation/remediation/R13/run-1/workspace-members-and-lints.stdout.log` 支持八成员 workspace 及真实包名；本表不把该事实扩大为本轮逐包构建/测试已验证。 |
| 2.2 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R12/run-2/review.md` 与 `R12/run-1/check-engineering-self-test.stdout.log`、`R12/run-1/report.md` 支持工程守卫自测及历史实际结果；全量统一检查由 R14 收口。 |
| 2.3 | 证据不足 | `docs/validation/m0-engine-foundation/remediation/R12/run-1/safety-self-test.stdout.log`、`R13/run-1/member-lint-inheritance.stdout.log` 支持部分成员继承/安全守卫证据；未见本行列出的所有 lint、debug/release 诊断、全局状态路径逐一覆盖的完整证据包。 |
| 2.4 | 证据不足 | `crates/aline-app-core` 与 `docs/validation/m0-engine-foundation/` 的既有测试材料可定位 catalog 实现；当前交付目录内未取得覆盖缺键/未知键/插值/TOML 损坏及 locale 隔离全部断言的独立完整证据索引。 |
| 2.5 | 已核实 | `docs/validation/m0-engine-foundation/group-9/step-9.1/report.md` 与 `workspace-gates-after-cleanup/commands.log` 记录历史工作树统一入口 19 项均 exit 0，含 fmt/clippy/test、依赖/lint/safety/public-api 正反例；测试 312 passed。CI 配置见 `.github/workflows/m0.yml`，而该任务原文明确 CI 未运行应记未执行，故不要求声称远端执行。R14 当前隔离缓存无法复跑仅使当前最终树状态待核，不推翻历史任务证据。 |
| 2.6 | 已核实 | `docs/rust-style-guide.md`、`docs/v1-architecture.md`、`openspec/specs/engineering-governance/spec.md` 与现有守卫报告已包含规则和证据入口；只核实回填文档存在，不等于每条规则所有盲区已验证。 |
| 3.1 | 证据不足 | `docs/rust-style-guide.md` 与 `docs/engine-design.md` 定义坐标/身份概念；`R03/run-1/review.md` 核实复用坐标边界的一组外部测试，但未见本子任务所列全部身份消费签名/生成范围及 source 类型边界的全项证明。 |
| 3.2 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R03/run-1/review.md` 记录外部正例、半字符/越界/身份及编译失败负例，并有逐项诊断；核实范围是复用公开坐标边界，不把它扩大到所有坐标种类。 |
| 3.3 | 证据不足 | `docs/engine-design.md` §3/4/7、`docs/architecture-guide.md` 与 `R03/run-1/report.md` 支持契约回填的一部分；缺少覆盖所有 API 输入输出、错误分类、全部实际消费者的统一审计。 |
| 4.1 | 已核实 | `docs/validation/m0-engine-foundation/group-4/step-4.1/` 保存字节/行尾样本与采集记录；`R13/run-1/report.md` §F-17 及其追加勘误核实 32 条源码常量/金样，并纠正报告文字 31/32 差异。 |
| 4.2 | 证据不足 | `docs/engine-design.md` §5 与 `docs/validation/m0-engine-foundation/group-4/` 记录行尾语义及样例；当前未见独立证据覆盖本条所列所有新拼接 CRLF、边界编辑、限定等价及精确逆补丁组合。 |
| 4.3 | 证据不足 | `docs/validation/m0-engine-foundation/group-4/` 有既有 roundtrip 证据入口，但任务所列混合例外、Undo 基线替代和来源相等证据须逐项映射；本次可见资料未能完成该映射。 |
| 4.4 | 已核实 | `docs/validation/m0-engine-foundation/group-4/step-4.4/report.md` 的 Plan A/execution-3 追加记录及 `review.md` §F1 闭合记录，证实 `cargo test -p aline-source atomic` 10 个单元+7个集成测试通过，覆盖 staging 创建/部分写入/sync/发布前失败并核验目标 sentinel 保全。任务原文明确不要求完整 V-08 或跨平台断电保证；这些限制已明确登记，不影响该最小受检 create-only 原语任务成立。 |
| 4.5 | 已核实 | `docs/engine-design.md` §5、`docs/v1-architecture.md` §5.1 和 `docs/v1-validation-plan.md` 的 V-06/V-07 基础记录已回填；平台权限与替换竞争仍明确未测。 |
| 5.1 | 已核实 | `docs/validation/m0-engine-foundation/group-5/` 与 `R01/run-1/report.md` 的回归/逆补丁证据支持 buffer 补丁基本金样；本项不表示所有随机批次均经 R14 重跑。 |
| 5.2 | 已核实 | `docs/validation/m0-engine-foundation/group-5/` 中原子拒绝/不变性测试记录与 `R08/run-1/report.md` 的完整状态拒绝验证支持已记录的边界；本轮未重跑。 |
| 5.3 | 已核实 | `docs/validation/m0-engine-foundation/group-5/` 的 anchors/history 证据与 `R01/run-1/review.md` 的 undo/redo 状态核验支持该基础能力。 |
| 5.4 | 证据不足 | `R01/run-1/review.md` 明确 T-8-17 仍待裁决；`docs/validation/m0-engine-foundation/remediation/R01/run-1/handoff.md` 也登记待定输入 redo 时序问题，不能认定历史任务所有分支全闭合。 |
| 5.5 | 已核实 | `docs/engine-design.md` §9、`docs/architecture-guide.md` §5.4、`docs/rust-style-guide.md` §12 有回填；`docs/v1-validation-plan.md` 保留 TA-02/M2 边界。 |
| 6.1 | 已核实 | `docs/validation/m0-engine-foundation/group-6/step-6.1/` 的语法夹具与运行记录，以及 `docs/engine-design.md` 解析配置说明，支持记录的基础语法范围。 |
| 6.2 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R02/run-1/review.md` 与 `R02b/run-1/review.md` 独立核验 RewriteMap 复合/空来源合成锚点及回归。 |
| 6.3 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R09/run-2/review.md` 核验同夹具 dump 与独立事件重算，支持块/Gaps/dump 的可审查证据。 |
| 6.4 | 证据不足 | `docs/validation/m0-engine-foundation/remediation/R10/run-2/review.md` 核实 51 行独立重算，但明确限于 basic fixture；不能据此证明本任务列出的全部映射语法、方向、UTF-16 与错误场景。 |
| 6.5 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R08/run-1/report.md` 与 `docs/validation/m0-engine-foundation/group-6/step-6.5/` 保存保护/HTML 负例和测试结果；范围按报告明确的保护区间语义。 |
| 6.6 | 证据不足 | `docs/engine-design.md` §10 与已有 hostile 测试入口支持基础预算设计；R14 `brief.md` 要求最终复核并注明第三方长调用限额盲区，当前未见完整敌意样本/实际步数证据映射。 |
| 6.7 | 证据不足 | `docs/validation/m0-engine-foundation/group-6/step-6.7/` 有 reuse 入口，`R03/run-1/review.md` 覆盖重定位边界；未取得覆盖任务列出的引用/脚注/围栏/配置全维度差分的证据链。 |
| 6.8 | 已核实 | `docs/engine-design.md` §6/7/11/12、`docs/v1-validation-plan.md` 和 `docs/validation/m0-engine-foundation/group-6/` 已标注 SoftBreak/HTML 等基础缺口；不宣称 AC-09 或完整解析覆盖。 |
| 7.1 | 已核实 | `docs/validation/m0-engine-foundation/group-7/step-7.1/` 保存 plan/validate/apply 和隐藏片段证据；R04 的实际消费链由 `R04/run-2/review.md` 独立核对。 |
| 7.2 | 已核实 | `docs/validation/m0-engine-foundation/group-7/step-7.2/` 与 `R04/run-2/review.md` 支持拒绝用例和真实宿主状态检查；不把拒绝能力扩大到未覆盖平台。 |
| 7.3 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R08/run-1/report.md` 给出删除/粘贴/HTML 跨界负例、Source 允许路径及完整状态比较，并明确修正文档端点语义。 |
| 7.4 | 证据不足 | `docs/engine-design.md` §8/9/14 与 `docs/architecture-guide.md` §7 回填可见；未见独立证据对 DocCore 之外所有写入口与 R/S 场景列表做完整审计。 |
| 8.1 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R04/run-1/report.md`、`R04/run-2/review.md` 支持 DocCore 唯一状态/Buffer 消费及无窗口集成测试；真实 GPUI 不在此结论内。 |
| 8.2 | 证据不足 | `docs/validation/m0-engine-foundation/remediation/R01/run-1/review.md` 核实 undo/redo 输入组部分约束；无法从当前可见的单项报告证明 900ms 边界、切模式、快照交错等全部场景均有现行独立证据。 |
| 8.3 | 已核实 | `docs/validation/m0-engine-foundation/group-8/step-8.3/report.md`、`expectations.md`、`state-tables/composition.tsv` 与 `targeted-tests/cargo-test-document_composition.log` 记录纯状态机 25 例通过，覆盖更新/替换/确认/取消、等待与取消副作用及当前快照；报告明确真实 IME 窗口、焦点与平台 callback 未执行，符合本任务要求“标明未执行”，不是其验收遗漏。 |
| 8.4 | 证据不足 | `docs/validation/m0-engine-foundation/remediation/R04/run-2/review.md` 核实实际消费拒绝链；关闭后返回、重载基线与完整回执等仍被 `R04` 记录为未验证/保留协议，不能全项核实。 |
| 8.5 | 已核实 | `docs/engine-design.md` §8–10/17、`docs/architecture-guide.md` §5/7.1、`docs/v1-validation-plan.md` 已回填基础接口与未测列表；不表示 UI 输入或保存验收完成。 |
| 9.1 | 证据不足 | 历史时点的 `docs/validation/m0-engine-foundation/group-9/step-9.1/report.md` 记录依赖清理后统一入口 19/19 exit 0、workspace 测试 312 passed；但 R14 当前隔离缓存运行证据 `docs/validation/m0-engine-foundation/remediation/R14/run-1/gate/unified/summary.json` 显示 Clippy/test 因依赖缺失 exit 101，正常 workspace 依赖检查也未通过。旧时点结果不能证明当前最终工作树完整门禁通过，当前失败也不能推翻旧时点通过事实。 |
| 9.2 | 证据不足 | `docs/validation/m0-engine-foundation/remediation/R11/run-4/review.md` 只核实 R11 新采集；`docs/validation/m0-engine-foundation/remediation/progress.md` 与 R14 `brief.md` 表明 E1 最终闭包仍需统一复核，不能以分散历史材料视作完整 E1 包。 |
| 9.3 | 已核实 | `docs/validation/m0-engine-foundation/remediation/R09/run-2/review.md` 核实 E3 同夹具独立 dump 重算；R02/R03/R10 报告提供其余基础 API/映射证据入口。仅核实已存在基础证据，不代表 E2/E5 全出口通过。 |
| 9.4 | 已核实 | `docs/validation/m0-engine-foundation/group-9/step-9.4/report.md` 按 AC-01–04、TA-01/02、ED-01 逐项列出现有 fixtures、测试/原始日志及基础贡献，并逐项明确未测端到端 UI、真实 IME/回调、完整结构命令和文件协调。相关基础失败/修复证据见 `docs/validation/m0-engine-foundation/remediation/R01/run-1/review.md`、`R04/run-2/review.md`、`R08/run-1/report.md`。任务原文明定这是基础证据包并要求列未覆盖项；这些边界不构成任务缺证。 |
| 9.5 | 已核实 | `docs/v1-validation-plan.md` 与 `docs/validation/m0-engine-foundation/remediation/progress.md` 将 V-01/V-06/V-07 基础证据及 M1/M2 未测交接分开登记，并明确 V-01～V-09 不整项通过。 |
| 9.6 | 证据不足 | R14 当前 `docs/validation/m0-engine-foundation/remediation/R14/run-1/reviews/engine-review.md` 仅完成引擎范围复核，并明确不含守卫/文档/历史事实最终复核；其 scenario matrix 也被退回补具体测试名/日志。任务要求全部 finding、出口及适用规范/交叉引用的独立 review 与 validate/codegraph 证据，当前复核包尚未归档完整范围结论，故暂不足以核实。 |

## 汇总

- 已核实：27
- 证据不足：19
- 与事实不符：0
- 涵盖编号：1.1–9.6 中除后加的 6.9，共 46 条。
- 明确疑点：原维护任务 §1 的 26/15/5 是原始复核摘要，但当前可见资料没有它与 46 个编号逐条对应的映射；因此本表不声称复现了该历史映射。当前仍为“证据不足”的编号为：1.2、1.3、1.4、2.3、2.4、3.1、3.3、4.2、4.3、5.4、6.4、6.6、6.7、7.4、8.2、8.4、9.1、9.2、9.6。2.5 与 9.1 的裁决分别限定在原任务明示范围与证据时点：2.5 的历史本地门禁/守卫正反例和 CI 配置有证据，CI 未执行按原文记未执行；9.1 的历史 19/19、312 passed 成立，但不能证明 R14 当前最终树。4.4 的验收是最小受检 create-only 原语，不要求完整 V-08；8.3 的验收是纯状态机模拟并要求明确标记真实平台 callback 未执行，当前报告满足；9.4 只要求 E4/E7 基础证据并显式列明缺口，group-9 报告已逐项提供。9.6 当前仅有引擎范围 R14 review，守卫/文档/历史事实范围和全量最终复核仍未归档。
