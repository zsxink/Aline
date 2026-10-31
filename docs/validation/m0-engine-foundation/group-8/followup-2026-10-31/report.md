# 组 8 用户裁决后实现补充（待独立复核）

本记录由实现者撰写，仅记录实现与实际执行，不作为独立 review 结论；tasks 8.1–8.5 未由本实现者勾选，8.5 现行正文未由本实现者回填。基准为 `16f8e30` 后未提交工作树；同一工作树中主 Agent 另行修改 change 的 spec/design 和依赖清单，最终复核应核对实际 Git 差异。

## 已批准的三项口径

用户“按你建议来”确认：R1-F1 采用 M0 最小资源引用所有权表示；Diff-1 初始打开范围等于排序后的选区范围，F2 的 `0..4` 更正为 `4..4`；Diff-2 的 SnapshotId 是快照实例/内容视图来源身份，不是 WorkingVersion 或内容哈希，相同文本可具有不同 ID。补充手写输入/期望见 [expectations.md](./expectations.md)。

## 实现范围与对应证据

| 首轮发现 | 本次处理 | 验证入口 |
|---|---|---|
| R1-F1 | ResourceReference 以同一 DocumentId 绑定不可变定位值与 Arc 身份；DocCore 仅接受当前输入身份登记，数量预算前检，登记推进草稿代次并使旧派生输入失效；CurrentSnapshot 冻结 Arc 资源集合 | document_followups 中 3 个 resource 用例：非空/旧集合冻结、错文档/预算/重复、历史与关闭生命周期 |
| Diff-1 | F2 期望笔误更正；反向选区保留方向，打开范围排序 | document_initial_open_range_is_sorted_selection |
| Diff-2 | 当前快照 rustdoc 明确来源身份；回到 Buffer 同文本时可复用 SnapshotId，WorkingVersion 仍不同 | document_snapshot_id_is_source_identity_not_content_or_version |
| R1-F2 | InputIdentity 加 ParseConfigGeneration 与 DerivedRequestId；DocCore 发放单调请求、推进配置并按所有维度拒收 | case_8_4_07/08 由占位改为真实请求、配置变化和乱序拒绝；identity 单元测试验证计数耗尽拒绝不回绕 |
| R1-F4 | 原用例保留稳定编号，函数名增加任务要求的领域关键词，过滤命令不再零匹配 | named-document/input-group/composition/publication-wait-filter 原始输出 |
| R1-F5 | switch_mode：组合活动返回 Deferred，平台自然结束后宿主重试；非组合期统一提交剩余组，切换只改模式；autosave_snapshot 组合期 None；recovery_snapshot 独立含候选读取 | case_8_2_23、case_8_3_23/24 及 followups 的模式/读取用例 |
| R1-F6 | 公共状态捕获补块范围/基线片段、SnapshotId、等待句柄与下一编号、资源集合、模式及配置/request；原拒绝测试共用该比较 | tests/common/mod.rs 的 State/capture；全组 UI 回归 |
| R1-F8 | 宿主集成用例真实消费 source create-only Written 与既有目标拒绝分类；派生结果因新输入/闭块拒绝时仍保留旧写盘事实并从 Written 建基线 | document_written_receipt_survives_stale_result_and_closed_target；该用例仅 Unix 编译、实际仅本机 macOS 运行 |

另在无草稿开块分支补齐当前快照文本预算检查：5 字节 `abcd\n` 用 4 字节预算必须拒绝；此前该分支绕过预算。对应 document_snapshot_budget_rejection_without_draft_preserves_state。

新增宿主协议方法放在 `doc_core/protocols.rs`，状态仍只在 DocCore；ResourceReference/模式枚举为具体数据，不新增 Registry/Service、依赖、线程、锁或全局状态。DocCore 继续只持一个 StringBuffer（含历史）和至多一个 OpenBlock/DraftSlot。

## 资源与阶段边界

- 宿主必须在引用生效前登记所有需要的资源；任意空集合不能被解释为已证明文本无资源。M0 不自动扫描 Markdown 资源，也不验证定位值指向可用文件。
- M0 保守保留登记过的资源至 DocCore 生命周期结束，包含草稿、Undo/Redo 可能仍需的引用；快照独立强持有冻结集合，旧快照不会随着新登记改变。不会精确计算当前/历史各节点的最小可达集合，也不会在历史裁剪、Undo 或关闭块时回收资源。
- 弱句柄升级只表示能重新取得内存强引用，不授权删除文件。真实图片加载、迁移、恢复记录、清理与保存协调仍未实现/未测。
- 一个 DocCore 仅有一个当前可替换派生请求槽位；较新请求使同槽旧结果失效。没有实现解析调度器、查询/高亮各自的完整任务服务。
- 模式切换仅为纯状态协议，未实现可视化/源码两视图及源选区重新映射。自动保存只有“延后/读取”判定，无计时器、目标队列或磁盘保存；恢复读取无恢复记录写入。
- R1-F7 的 Reload source 只通过独立节点与 Undo/Redo 行为间接验证，未为测试新增历史可变访问器；保留首轮认定的 M0 可观察性限制。
- Written 集成只验证现有 source create-only 能力和宿主保留分类事实。DocCore 没有文件协调器/保存点消费；不支持既有文件 metadata 保全替换的能力声明，不证明实际 UI 保存、恢复、导出或 V-08 闭环。

## 实际执行与失败保留

环境：`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`、`PYTHONDONTWRITEBYTECODE=1`。当前主机 macOS arm64；`--all-targets` 不代表其他 OS/target。

原始 stdout/stderr 各自原样保存；[commands.log](./commands.log) 逐项记录 `date -u +%Y-%m-%dT%H:%M:%SZ` 取得的起止 UTC、实际命令原文及退出码。runner 的 stdout 没有作为子命令结果使用。Cargo 测试和 Clippy 明确带 `--locked --offline`；cargo fmt 不解析依赖，使用任务指定语法。

- `first-receipt-expectation-failure` exit 101：新增集成用例误把既有目标拒绝放在 prepare 阶段。source 契约在 publish_new 阶段拒绝；修正测试的发布阶段调用后通过。失败原始输出保留，不修改 source 或降低拒绝期望。
- `ui-clippy` exit 101：新测试两处 `&[first.clone()]` 触发 cloned_ref_to_slice_refs；按提示换为 `std::slice::from_ref`，无 allow/lint 修改，后续通过。
- `handoff-ui-full`：103 passed，0 failed（2 unit + document22 + composition25 + results17 + typing25 + public_boundaries2 + followups10）。这是 aline-ui 局部全套，不替代 workspace 完整门禁。
- 任务指定字面过滤的实际通过数：document=34，input_group=27，composition=26，publication_wait=14；关键词可能有交叉，不能相加作为独立用例总数。全套测试才覆盖所有测试 target。
- `handoff-ui-clippy`、`handoff-fmt`、`handoff-safety` 与 codegraph sync/status 的实际退出码以 commands.log 为准。

首轮报告将 document22 + typing25 + composition25 + results17 + public_boundaries2 总数写为 89，算术应为 91（另有 unit1）。历史原文保留；本次只以上述实际测试输出计数，不据此改写旧日志。

未在本补充中执行 workspace 完整 19 项门禁、Windows/Linux/Intel Mac、真实 IME/焦点/候选窗、正式 CI、实际现有文件保存/自动保存/恢复/资源迁移；这些不得由局部测试推断通过。独立复核与后续组 9 出口取证由主 Agent 安排。


## 独立复核后的生命周期修复

独立 reviewer 指出：关闭块后重开相同范围、以及无待定组时 Visual→Source→Visual，原实现仍可能采纳先前身份。补充手写反例后，`review-lifecycle-before-fix` 实际为 12 passed / 2 failed（exit 101）；`review-lifecycle-overflow-before-fix` 的两个身份耗尽原子性测试也实际失败（exit 101）。原始失败保留，未修改审查报告或降低拒绝期望。

修复在两处明确生命周期边界推进 `DerivedRequestId`（因此它同时表示当前派生采纳代次）：关闭块、实际模式改变。先受检计算下一身份，再调用统一待定组提交；只有提交成功才发布新身份并关闭/切模式。新请求仍按既有入口推进。同模式请求、组合期 Deferred、前置/历史预算失败均不推进身份；重复打开/非法范围也保持完整状态。

新增 4 个集成用例验证关闭重开旧结果拒收、模式往返旧结果拒收、无变化/延后/失败状态保全、历史预算失败时请求仍有效。两个同模块单元用例用测试专用耗尽身份分别覆盖有/无待定组，并比较完整 OpenBlock（含普通组时间、撤销草稿）、Buffer 身份/文本/选区/历史、等待、资源、配置、模式、快照缓存身份及预算，确保溢出发生在任何提交前。未增加生产可变访问器。

修复后 `review-lifecycle-after-fix`（14 个 followups）、`review-lifecycle-overflow-after-fix`（2 例）及 `review-fix-ui-full` 全通过，UI 总数现为 **109 passed / 0 failed**。`review-fix-ui-clippy`（all-targets、-D warnings）、fmt、安全扫描、codegraph sync/status 全 exit 0。上方 103 是此前 handoff 的真实历史计数，未改原日志；最终以本节对应原始输出和后续独立复核为准。仍未由实现者勾任务或宣告 review 通过。
