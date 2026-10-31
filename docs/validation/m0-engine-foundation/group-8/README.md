# 组 8 验证期望：DocCore 纯状态（8.1–8.4）

日期：2026-10-31

## 实现后状态（2026-10-31 追加）

`DocCore` 与四个测试目标已经落地：`cargo test -p aline-ui --test document` 22 例、`--test document_typing` 25 例、`--test document_composition` 25 例、`--test document_results` 17 例，均退出 0。逐步记录见 `step-8.1/report.md`…`step-8.4/report.md`。全量 `check_workspace.py` 19 项门禁中 17 项退出 0；`safety` 与 `safety-self-test` 因 `scripts/check_safety.py` 无法解析集成测试 crate 根的 `mod common;`（`tests/common/mod.rs` 被误判为 `tests/document/common/mod.rs`）而退出 1，原始输出在 `leader-verify/workspace-gates/`。守卫未修改，该冲突需用户裁决。`tasks.md` 8.1–8.4 保持未勾选；本目录不构成组 8 验收或 M0 出口证据。

下面的「状态声明」保留实现前原文，仅作为手写期望定稿时点的记录；其中「`crates/` 下不存在 `DocCore`」等描述已被上述实现后状态取代。

## 状态声明（实现前，保留原文）

本目录全部内容是在 `DocCore` 实现之前、依据需求规范、设计文档、架构文档和已有公开 API 手写的期望，不是测试结果，不声明任何用例通过，也不是 M0 出口证据。`crates/` 下不存在 `DocCore`；`tasks.md` 8.1–8.4 的实现尚未发生。测试落地时若实际行为与本目录不一致，先按 `待确认` 裁决，不得反向修改期望以迁就实现。

覆盖范围为 `openspec/changes/m0-engine-foundation/tasks.md` 8.1–8.4；8.5（文档回填）不在本目录。权威需求见 `openspec/changes/m0-engine-foundation/specs/engine/document-core/spec.md` 的 R1/S1、R1/S2、R2/S1、R2/S2、R3/S1、R3/S2。

## 文件与覆盖

| 步骤 | 期望文档 | 状态表 | 用例数 | 覆盖 |
| --- | --- | --- | --- | --- |
| 8.1 | `step-8.1/expectations.md` | `step-8.1/state-tables/open-block-snapshot.tsv` | 22 | 唯一 Buffer/History/OpenBlock；当前快照含最新草稿；快照不封口/不压缩/不清 redo；草稿基线冲突；唯一打开槽位；UTF-8 边界 |
| 8.2 | `step-8.2/expectations.md` | `step-8.2/state-tables/typing-groups.tsv` | 25 | 900ms 边界（899/900 合并、901 拆分）；非相邻输入；同块多组提交；离块/Escape；Undo 待定组优先；Redo 选区；快照交错；模式切换协议 |
| 8.3 | `step-8.3/expectations.md` | `step-8.3/state-tables/composition.tsv` | 25 | 组合开始/替换/重复更新/确认/取消；IME 确认独立成步；取消恢复且代次不复用；旧投影拒绝；Save/SaveAs/Export 可取消等待；取消请求零副作用；自动保存延后；恢复读取独立 |
| 8.4 | `step-8.4/expectations.md` | `step-8.4/state-tables/stale-results.tsv` | 17 | 当前身份接受；旧文档/版本/请求拒绝；Undo 回到相同文本仍拒绝；关闭目标；草稿基线冲突不 drop_open；旧解析/投影保留输入；模拟重载独立历史/基线；无文件协调 |

合计 89 个用例。所有 TSV 使用 21 列：操作、虚拟时间、前置、已提交文本、当前快照文本、revision、draft generation、undo/redo 深度、节点数、anchor/head 字节、打开块、组合、发布等待、快照 id、结果、错误和规范引用。每个拒绝行都逐列给出完整状态，便于「零副作用」比较。

## M0 范围内与范围外

范围内（纯状态、无窗口、虚拟时间）：

- OpenBlock 的唯一草稿、单区间提交、History 分组与 900ms 窗口语义。
- `WorkingVersion` 的 revision/draft generation 推进，快照 id 与内容一致。
- 组合候选的更新/确认/取消，以及发布等待的状态转移和取消请求。
- 结果身份校验、旧结果拒绝、重载的独立历史边界和保存点身份分离。

范围外（本目录只记录协议边界，不给通过期望）：

- 真实 IME 候选窗口、marked text 绘制、焦点/输入法切换、平台组合回调的真实时序。
- 真实保存/另存/导出、磁盘 IO、写盘回执与文件协调；`DocCore` 不承担文件协调。
- 自动保存的实际调度、恢复记录与临时资源清单（宿主/M2）。
- GPUI 窗口、线程/定时器、跨平台行为、第九个 crate。
- 命令执行：`aline-edit` 的 `plan_change`/`validate_change` 只用于交叉检查 `PendingInput`/`CompositionActive` 预检，本组没有 DocCore 命令入口。

## 采用的计算口径

文档可裁决的部分被固定为可计算模型，便于后续测试逐列断言：

- 普通输入先进入 OpenBlock 的唯一草稿；提交时才作为一个单区间 `OperationSource::Typing` 补丁写 Buffer 并封口（T-8-02）。
- 900ms 取 `crates/aline-buffer/src/history.rs` 的 `TYPING_MERGE_WINDOW_MILLIS = 900`，比较符取 `prepare_record` 的 `<=`：899/900 合并，901 拆分。
- 只改草稿的状态用 `WorkingVersion::next_draft`；真正写入/撤销/重做/重载后用 `after_commit`。跨 900ms 的一次 `type_text` 计两次转换（dg+2，T-8-04）。
- 快照不提交、不封口、不裁剪、不清 redo；同一工作版本重复读取保持同一 `SnapshotId`。
- 组合确认产生独立 `ImeCommit` 节点；组合取消恢复组合前文本/选区、无空步、dg 前进；重复/无变化组合事件的 dg 推进见 T-8-06/07。
- 编辑期间显式发布先建立可取消等待，平台自然结束且组合结束后才取当前快照；取消发布请求不结束组合、不改输入/历史。

## 待确认（跨步骤统一编号）

这些条目是规范/设计/公开 API 之间尚未裁决的点，已在各 `expectations.md` 第 5 节和 TSV 的 `待定` 值处引用。实现前需由架构/规范负责人裁决，不能按目录、日期或历史措辞自行取舍。

| 编号 | 待确认问题 | 主要位置 |
| --- | --- | --- |
| T-8-01 | 契约缺少打开/激活/关闭块入口；「块已打开」前置无法只靠 API 建立；`NoOpenBlock` 的映射 | 8.1、8.2 |
| T-8-02 | 普通输入在提交前只改草稿，还是逐键写 Buffer 并由 `prepare_record` 合并 | 8.1、8.2 |
| T-8-03 | 待定草稿组的 Undo/Redo 是否只推进 dg；`after_commit` 注释是否包含待定组撤销 | 8.1、8.2、8.4 |
| T-8-04 | 一次 `type_text` 跨 900ms 是否暴露两次版本转换（dg+2） | 8.1、8.2 |
| T-8-05 | 组合开始时已有普通待定组：先提交/封口该组，还是返回 `PendingGroup` | 8.3 |
| T-8-06 | 重复且文本相同的 `composition_update` 是否仍推进 dg | 8.3 |
| T-8-07 | 无文本变化的 `composition_commit` 是否仍推进 dg | 8.3 |
| T-8-08 | 组合范围是否只能折叠；首次 update 是否隐式开始组合；非折叠范围的确认/取消选区 | 8.3 |
| T-8-09 | `ClosedTarget` 的触发与检查顺序；`InputIdentity` 不含目标/块身份；关闭入口 | 8.4 |
| T-8-10 | `InputIdentity` 缺少解析配置代次与 request id；错误配置/旧请求无法构造 | 8.4 |
| T-8-11 | `WrongDocument`/`StaleResult` 是否以 `Identity(IdentityError::…)` 透传 | 8.4 |
| T-8-12 | 自动保存延后与恢复保护没有 DocCore 入口；`PublicationKind` 是否含自动保存 | 8.3 |
| T-8-13 | DocCore 无重载/外部变化入口；草稿基线冲突的触发与冲突/重载后的 dg | 8.1、8.2、8.4 |
| T-8-14 | `CurrentSnapshot` 访问器、`CommitOutcome`、`PublicationResult`、`WaitHandle`、`DocError` 具体形状 | 8.1–8.4 |
| T-8-15 | DocCore 无模式切换入口；只能给协议性期望 | 8.2 |
| T-8-16 | `begin_publication_wait` 在非组合期是否允许或立即完成；ED-01 只定义组合期 | 8.3 |
| T-8-17 | Undo 后开启新草稿时，Buffer redo 分支与草稿 redo 的失效时序 | 8.4 |

## 使用方式

1. 先裁决上表，再固定公开 API 的签名与返回形状（T-8-14），然后才写测试。
2. TSV 的 `待定` 不得当作失败；测试实现应跳过或先补裁决。
3. 断言时逐列比较文本、revision、dg、undo/redo、节点数、选区、打开块/组合/等待和快照 id；拒绝用例必须验证零副作用。
4. 本目录只提供实现前期望；实际通过状态以后续验证计划与证据为准。

## 2026-10-31 用户裁决后的补充入口

三项裁决后的实现/手写期望与原始局部验证见 [followup-2026-10-31/report.md](./followup-2026-10-31/report.md)。该记录由实现者撰写，待新的独立 reviewer 核查；不替代首轮历史报告，不表示任务 8.1–8.5 或 M0 出口已通过。
