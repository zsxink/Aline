# 9.4：E4 / E7 基础证据包

本包按 [proposal 的 E4/E7 定义](../../../../../openspec/changes/m0-engine-foundation/proposal.md)归集基础证据。判断依据是当前源码、固定夹具、实际测试输出和 Git 对象；旧报告与任务勾选仅作索引。本步骤只读核查已有证据，没有重新运行 Cargo，亦不单独给出 M0 出口通过结论。

## 执行与可追踪输入

- 本次源码基准为 `16f8e30` 后尚未提交的工作树，实际 HEAD、主机和差异见 [inspection](./inspection/)、[commands.log](./commands.log)。源码与原始日志 SHA-256 见 [source-fingerprints.tsv](./source-fingerprints.tsv)；文档后续回填可能变化，指纹是本次检查时快照。
- 首轮正式执行来自 [9.1 commands](../step-9.1/formal-run/commands.log)、[19 项汇总](../step-9.1/workspace-gates/summary.json)、[测试 stdout](../step-9.1/workspace-gates/test.stdout.log)和[测试 stderr](../step-9.1/workspace-gates/test.stderr.log)。2026-10-31T14:52:24Z–14:53:10Z 的统一门禁 exit 0；实际 19/19 exit 0，workspace 合计 312 passed、0 failed。该次是依赖清理前历史执行，不代表最终依赖图。
- **依赖清理后的当前来源：** [9.1 最终命令](../step-9.1/formal-run-after-cleanup/commands.log)、[最终汇总](../step-9.1/workspace-gates-after-cleanup/summary.json)、[最终 stdout](../step-9.1/workspace-gates-after-cleanup/test.stdout.log)及[最终 stderr](../step-9.1/workspace-gates-after-cleanup/test.stderr.log)为 2026-10-31T15:02:22Z–15:03:05Z 的实际执行，仍为 19/19 exit 0、312 passed、0 failed；本包表格的测试执行依据采用这次最终日志。
- [当前测试观察清单](./observed-tests-after-cleanup.json)由 [inspect_evidence.py](./inspect_evidence.py)以 `--after-cleanup` 重新读取最终原始 stdout 生成：source/buffer/edit/ui 四 crate 的 180 个外部 `#[test]` 函数均存在 `... ok` 记录。[最终指纹](./source-fingerprints-after-cleanup.tsv)加入 `Cargo.lock` 和各成员 manifest，覆盖清理后的依赖输入；新原始检查输出见 [inspection-after-cleanup](./inspection-after-cleanup/)。[首次观察清单](./observed-tests.json)、旧指纹和旧检查输出均保留。以上均是日志检索，不是新增 Cargo 执行，也不把函数内部金样行数算成 Cargo 测试数。
- 实际环境为 macOS arm64、Rust 1.98.1、指定离线 Cargo home；`--all-targets` 不代表 Windows/Linux/Intel Mac 或其他 target 已构建。

最终空白修正补充：[manifest-whitespace-final.json](./manifest-whitespace-final.json)重新计算锁文件和全部成员 manifest，确认相对清理后指纹仅 `crates/aline-plugin/Cargo.toml` 改变，且与原始快照逐字节比较恰好少一个末尾 LF；当前 manifest 指纹以该补充为准。原清理后指纹/快照未覆盖。此检查未运行 Cargo，不改写 15:02 门禁的执行输入；原始检查输出位于 `inspection-manifest-whitespace-final/`，命令及退出码追加于本目录 commands.log。

## E4：AC-01～04 的基础贡献

| PRD 原口径 | 当前可核查输入、断言及实际执行 | 基础结论与端到端缺口 |
|---|---|---|
| AC-01：打开不编辑，保存并逐字节比较 | [roundtrip 测试](../../../../../crates/aline-source/tests/roundtrip.rs)直接读取 [16 行固定金样](../../group-4/step-4.3/roundtrip-golden.v1.txt)，逐例比较原 bytes 通道、冻结源码、编码 bytes、重开解码及逆补丁；[decode](../../../../../crates/aline-source/tests/decode_golden.rs) / [encode](../../../../../crates/aline-source/tests/encode_golden.rs)覆盖 BOM、正文 U+FEFF、混合/平票行尾、孤立 CR、无尾换行、非法编码与预算。相应函数均在 9.1 原始输出通过 | 证明无头编解码和原字节通道基础；没有真实 UI 打开→现有文件保存链路。AC-01 未通过 |
| AC-02：改一段、切两模式、保存重开 | [补丁金样消费者](../../../../../crates/aline-buffer/tests/patch_goldens.rs)读取 [134 例正逆补丁](../../group-5/step-5.1/goldens/forward-inverse.tsv)并比较 after 坐标、选区及逆补丁；source roundtrip 检查未改前后缀与限定行尾例外；[模式测试](../../../../../crates/aline-ui/tests/document_followups.rs)验证剩余组只提交一次，模式动作无额外文本/历史变化 | 三个原语分别已测，尚未形成真实双视图→保存→重开闭环；CR-X-LF 等只保证已批准的存储行尾等价，不声称重开 String 总相等。AC-02 未通过 |
| AC-03：中文 IME、emoji、转义/实体附近编辑 | [映射用例](../../../../../crates/aline-markdown/tests/mapping.rs)检查 UTF-16 中间代理对拒绝、实体/转义、ZWJ、组合附加符、选区方向；[组合用例](../../../../../crates/aline-ui/tests/document_composition.rs)模拟更新/替换/确认/取消，检查独立 ImeCommit 与取消恢复前态/无空步；[锚点](../../../../../crates/aline-buffer/tests/anchors.rs)比较方向和 affinity，全部由 9.1 实际执行 | 证明字节/UTF-16 映射及纯事件历史基础；未测系统字素导航、像素命中、marked text 绘制、候选窗、焦点和真实平台回调。AC-03 未通过 |
| AC-04：连续输入、格式/列表/表格结构命令 | [输入组用例](../../../../../crates/aline-ui/tests/document_typing.rs)验证 899/900ms 合并、901ms 分组、非相邻分组、离块/Escape、待定组优先撤销；[edit 金样](../../../../../crates/aline-edit/tests/goldens.rs)的 `golden_consume_single_history_step` 实际消费计划并验证单步 Undo/Redo；[拒绝用例](../../../../../crates/aline-edit/tests/refusals.rs)比较真实 Buffer 前后状态 | 代表命令仅 Replace、Strong/Strike toggle。列表/表格/任务框/语言/全文格式化、查找替换事务、跨块操作和通用 DocCore 命令消费者未完成；不能把输入组和代表命令的独立测试合成完整命令集。AC-04 未通过 |

source 的 16 行独立期望直接由测试消费；buffer 固定金样还包含 11 行行索引。edit 的 30 行金样、选区勘误及隐藏纯空白语义限制保留在原始 [step-7.1](../../group-7/step-7.1/report.md) 和 [step-7.4](../../group-7/step-7.4/report.md) 中：本包没有刷新金样迁就实现，也不宣称隐藏片段的边界、位置和全部空白都已完整保留。

非法 UTF-8/UTF-16、半字符、越界、错基线、预算、重叠补丁等用例在 9.1 未发生测试失败或进程 panic；[rejections](../../../../../crates/aline-buffer/tests/rejections.rs)、edit refusals 与 [UI 完整可观察状态捕获](../../../../../crates/aline-ui/tests/common/mod.rs)分别检查所属状态。此证据覆盖列明输入和代码路径，不构成“所有输入绝不 panic”的穷尽证明；第三方调用、宏和真实平台行为仍有工程守卫/人工评审边界。

## E7：TA-01 / TA-02 / ED-01 纯协议

| 契约 | 可核查事实与原始来源 | 尚未验证 |
|---|---|---|
| TA-01：同块多组、提交/关块分离 | `input_group_case_8_2_01..25` 与 `document` 用例检查多个节点、选区、离块不重复；快照后继续输入仍能合并；9.1 原始日志均通过。手写事件表见 [8.1 期望](../../group-8/step-8.1/expectations.md)、[8.2 期望](../../group-8/step-8.2/expectations.md)，本轮补充期望见 [followup](../../group-8/followup-2026-10-31/expectations.md) | 真实按键时钟、视图开块、完整命令/插件/子编辑器接线 |
| TA-02：重载独立历史、磁盘基线不随 Undo 回退 | [buffer history](../../../../../crates/aline-buffer/tests/history.rs)直接断言 `OperationSource::Reload`、正逆补丁、保存点 revision 与独立节点；UI results 的 8.4-13～16 检查内存 Undo/Redo/无变化保留 redo；source roundtrip 的最小 Host 检查成功/失败冻结输入和新基线。9.1 均通过 | DocCore 不持有实际磁盘基线；UI 层 Reload 来源只间接可观察。真实外改检测、载入磁盘、暂停自动保存/提示和竞争下保存均未测 |
| ED-01：组合期显式发布等待、自然结束再取快照 | [composition](../../../../../crates/aline-ui/tests/document_composition.rs)覆盖 Save/SaveAs/Export 等待、重复等待拒绝、继续候选更新、自然确认/取消、取消请求；[followups](../../../../../crates/aline-ui/tests/document_followups.rs)检查 autosave 延后与独立恢复读取。读取不修改输入/历史；9.1 均通过 | 平台自然结束事件、真正保存/导出队列、文件 I/O 与恢复记录未接线；不宣称已保存候选或生成产物 |
| 资源与结果生命周期 | 非空资源登记、快照冻结、Undo/Redo/关闭后存活，错文档/预算拒绝；配置/request、关闭重开、模式往返旧结果拒绝，以及计数耗尽/提交失败不变均有代码和原始测试。见 [followup 原始执行](../../group-8/followup-2026-10-31/commands.log)、[独立复判](../../group-8/review-round-3.md) | 宿主须在引用生效前完整登记；仅保守持有，不自动扫描文本、不精确追踪节点最小集合或清理；独立 reviewer 的局部运行未落原始文件，正式全量原始证据采用 9.1 |

source `Written` 集成测试只在 Unix 条件编译、实际在当前 macOS 主机运行，验证 create-only 写入事实即使派生结果过期/块关闭也能由宿主保留；现有目标拒绝以 `MetadataPreservationUnverified` 分类。不支持由此宣称现有文件安全替换或完整文件协调器已实现。

## 本包结论

E4/E7 所列基础贡献已有可追踪测试与原始日志；AC-01～AC-04、V-01～V-09 及 M0 出口均未由本包整项判过。9.6 仍须独立审查本包与所有出口材料，后续能力和平台缺口由 [E6 / M1 交接](../step-9.5/report.md)承接。
