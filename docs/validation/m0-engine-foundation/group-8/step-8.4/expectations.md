# 8.4 期望：受控结果序列与拒绝零副作用

日期：2026-10-31

状态：实现前手写期望。`DocCore` 尚不存在；本文件与 TSV 不是测试通过声明，也不是 M0 出口证据。状态值由规范、设计文档和已有公开 API 推导，无法裁决的值写 `待定` 并注明裁决来源。

## 1. 权威与追踪

- 需求：`openspec/changes/m0-engine-foundation/specs/engine/document-core/spec.md`
  - R3/S2「乱序结果与目标关闭」：旧请求/旧结果被拒绝，不复活已关闭对象；磁盘写入回执归宿主。
  - R1/S2「草稿基线冲突」：基线不符时保留草稿与选区、返回冲突，不 drop_open。
  - R2/S2「Undo 与组合取消」：撤销后的代次不复用；旧投影不得复活。
- 设计：`docs/engine-design.md` §8.3（基线不符保留草稿与选区、不 drop_open）、§9.3（重载是独立历史边界；Undo/Redo 选区；内容相等不替代身份）、§12.2（结果身份：文档、WorkingVersion、快照、解析配置代次、请求 id；旧请求/旧结果拒绝）、§10.2（字符边界）。
- 架构：`docs/v1-architecture.md` §5.1（单一 Buffer/History/OpenBlock、`DocumentId`、工作版本）、§5.2（基线丢失不 drop_open；提交前完整预检）、§5.4（TA-02 重载独立历史边界；保存点按身份而非内容；历史节点正向/逆向补丁）、§7.1（M0 只到内容快照契约）。
- 任务：`openspec/changes/m0-engine-foundation/tasks.md` 8.4。
- 已有 API：
  - `crates/aline-ui/src/document/identity.rs`：`DocumentId`、`DraftGeneration`、`WorkingVersion::next_draft`/`after_commit`、`InputIdentity { document, version, snapshot }`、`InputIdentity::validate`（顺序 document → version → snapshot）；`IdentityError` 含 `WrongDocument`/`StaleWorkingVersion`/`StaleSnapshot`/`GenerationExhausted`/`RevisionNotAdvanced`/`SnapshotRevisionMismatch`。
  - `crates/aline-buffer/src/string_buffer.rs`：`reload(new_text, after_selection, now_millis, budget)`（`OperationSource::Reload`、独立节点、相同文本直接 `ReloadOutcome::Unchanged`、只改内存不触碰磁盘基线）、`undo`/`redo`（首步 before-selection / 末步 after-selection）、`history_node_count`/`undo_depth`/`redo_depth`、`mark_saved`/`is_at_save_point`/`saved_revision`。
  - `crates/aline-buffer/src/history.rs`：`prepare_record` 仅在 `source == Typing` 时合并；新记录截断 redo 分支。
  - `crates/aline-edit/src/lib.rs`/`plan.rs`/`validate.rs`：派生计划绑定快照/投影，拒绝错误含 `WrongVersion`/`WrongSnapshot`/`ProjectionMismatch`；本组没有 DocCore 命令入口。

## 2. 本表采用的语义

### 2.1 结果身份与拒绝

- `accept_derived_result` 接受一个 `InputIdentity`；校验顺序按 `InputIdentity::validate`：先文档（`DocumentId` 指针身份），再工作版本（revision 与 draft generation），最后快照 id。任何一项不符都拒绝，且不改变文本、版本、历史、选区、草稿或打开块。
- 本表把拒绝统一映射为 `DocError::StaleResult`（版本/快照旧），文档不符映射为 `DocError::WrongDocument`；`InputIdentity` 拆出的 `IdentityError` 是否作为 `DocError::Identity(...)` 透传见 T-8-11。
- 文本相同不构成接受理由：Undo 回到相同文本后，revision 或 draft generation 已前进，旧身份仍必须拒绝（spec R3/S2、engine §12.2、arch §5.4）。
- `DocumentRevision` 只随已提交内容变化（提交、已提交 Undo/Redo、重载）前进；`DraftGeneration` 随每一次工作内容/状态代次变化前进。因此「旧请求」可以表现为 dg 不符而 revision 相同（8.4-02、8.4-06），也可以表现为 revision 前进（8.4-05）。
- 拒绝是零副作用：按状态表逐列比较文本、revision、dg、undo/redo 深度、节点数、选区、打开块/组合/等待和快照 id（spec R3/S2、engine §12.2）。

### 2.2 关闭目标与基线冲突

- 关闭后的目标不再接受派生结果。为把「关闭」与「旧版本」分开，8.4-09 用关闭前刚取得的当前身份（版本/快照均匹配）验证 `ClosedTarget`；若实现先查版本，旧身份会先得到 `StaleResult`，顺序问题见 T-8-09。
- 草稿基线冲突（打开块基线 revision 与 Buffer 当前 revision 不符）在任何读写入口都返回 `DraftBaselineConflict`，并保留草稿、草稿选区与打开块；不把旧基线区间套到新源，不 drop_open（spec R1/S2、engine §8.3、arch §5.2）。
- 本表的冲突夹具通过 `StringBuffer::reload` 在 DocCore 之外模拟源变化；DocCore 没有重载/外部变化入口，冲突行的 dg 写 `待定`（T-8-13）。

### 2.3 重载的独立历史边界

- DocCore 契约没有重载入口；本表用 `StringBuffer::reload` 模拟宿主重载，观察 Buffer 层可确定的事实：source 为 `Reload`、产生独立节点且不与 Typing 合并、revision 前进、历史节点保留、只改内存不触碰磁盘基线。
- 撤销/重做重载节点走同一 History：Undo 取该节点首步 before-selection，Redo 取末步 after-selection；保存点身份仍由 `mark_saved` 单独记录，内容相等不等于保存点（arch §5.4 TA-02）。
- 相同文本的重复重载在 `reload` 内直接返回 `Unchanged`，不调用 `prepare_record`，因此不截断 redo 分支、不新增节点。
- DocCore 的 draft generation 在外部重载后是否推进取决于未来入口，本表该列为 `待定`（T-8-13）；表内同时给出「若按 after_commit 推进」的参考值。

### 2.4 文件协调

DocCore 不承担文件协调或磁盘写回执；这些属于宿主（spec R3/S2）。本表只用一条协议行记录该边界，不构造写盘状态。

## 3. 状态表记法

`state-tables/stale-results.tsv` 的列、转义、夹具和标签与 8.1/8.2/8.3 相同；补充：

- `D2`：另一个 `DocCore::new("x\n",(1,1),limits)`，其 `DocumentId` 与 C1 不同。
- `I0`：C1 初始身份，版本 `(rev0,dg0)`，快照 `S0("x\n")`。
- `I1`：F1 上 `type_text(1,"a",0)` 后的身份，版本 `(rev0,dg1)`，快照 `S1("xa\n")`。
- `I_current`：当前 `current_snapshot` 与当前工作版本构成的身份。
- `待定` 行只出现在文档未裁决处；同行的文本/历史等确定值仍逐列给出。
- 重载模拟行的 `dg` 为 `待定`，`notes` 给出条件值；Buffer 文本、revision、深度、节点数、选区是确定的。

## 4. 场景期望

### 4.1 身份接受与旧身份拒绝（R3/S2、engine §12.2）

- `8.4-01`：当前身份 `I_current` 被接受，状态不变。这是正路径：文档、版本、快照三者均匹配，接受动作本身不产生历史步、不封口。
- `8.4-02`：先取 `I0`（rev0/dg0），再输入 `"a"`（rev0/dg1），然后接受 `I0`：期望 `StaleResult`；完整状态保持输入后的值（快照 `"xa\n"`、rev0、dg1、历史 0/0/0、选区 `2:2`、草稿 `"a"`）。旧解析结果不得覆盖新输入。
- `8.4-03`：接受来自 D2 的身份 `I_D2`：期望 `WrongDocument`；C1 状态零变化。`DocumentId` 检查先于版本检查。
- `8.4-04`：身份版本为 `(rev0,dg99)`、快照为当前 `S0`：期望 `StaleResult`；C1 状态零变化。`InputIdentity::new` 只要求 revision 与快照一致，故该身份可构造。
- `8.4-05`：取提交前身份 `I1`（rev0/dg1），提交 `"a"`（rev1/dg2/节点 1），再接受 `I1`：期望 `StaleResult`；提交结果保留（快照 `"xa\n"`、rev1、dg2、1/0/1、选区 `2:2`、块打开）。
- `8.4-06`：先取 `I0`（rev0/dg0，文本 `"x\n"`），输入 `"a"` 后 `undo()` 撤销待定组，文本回到 `"x\n"`（rev0、dg2、历史 0/0/0、选区 `1:1`）；此时接受 `I0` 仍期望 `StaleResult`。文本相同不代表身份相同；撤销后的代次不复用。

### 4.2 身份字段缺口（engine §12.2）

- `8.4-07`：身份携带错误的解析配置代次。`InputIdentity` 只有 document/version/snapshot，没有 config 字段，无法用给定 API 构造该用例；期望结果写 `待定`，状态保持当前的 `"xa\n"`/rev0/dg1 不变。裁决来源 T-8-10。
- `8.4-08`：身份携带旧 request id。同样没有字段可表达；期望结果写 `待定`，状态不变。裁决来源 T-8-10。

### 4.3 关闭目标与基线冲突（R3/S2、R1/S2）

- `8.4-09`：输入 `"a"` 并提交后取当前身份 `I_closed`（rev1/dg2，"xa\n"），随后 `leave_block` 关闭块（无剩余组，版本不变；无剩余组的 leave 是否直接关闭见 T-8-01），再接受 `I_closed`：期望 `ClosedTarget`；文本/版本/历史/选区不变，块保持关闭。关闭对象不得被结果复活；关闭入口与检查顺序见 T-8-09。
- `8.4-10`：夹具 X 上 `current_snapshot`：期望 `DraftBaselineConflict`，不产生快照；Buffer `"xZ\n"`、rev1、历史 1/0/1、草稿 `"a"`、选区 `2:2`、块仍打开（基线 rev0）。不把旧区间 `1..1` 套到新源。
- `8.4-11`：同一夹具上 `commit_open_block`：期望 `DraftBaselineConflict`；完整状态与 8.4-10 相同，块仍打开（不 drop_open），无历史步、无文本变化。

### 4.4 旧解析/投影保留输入（R3/S2）

- `8.4-12`：`I1` 取自输入 `"a"` 之后（rev0/dg1），再输入 `"b"`（rev0/dg2），接受 `I1`：期望 `StaleResult`；快照保持 `"xab\n"`、dg2、草稿 `"ab"`、选区 `3:3`。中间版本的解析/投影不得套到新草稿上；输入不丢。

### 4.5 模拟重载的独立边界（engine §9.3、arch §5.4 TA-02）

- `8.4-13`：已提交 `"a"` 并关闭块后，宿主用 `StringBuffer::reload("xZ\n", sel=2:2, now=30)` 模拟重载：Buffer `"xZ\n"`、rev2、历史 2/0/2、选区 `2:2`；新节点 source 为 `Reload`，不并入 Typing 节点；旧节点保留；只改内存，不触碰磁盘基线。DocCore 的 dg 是否推进无入口裁决，写 `待定`（若按 `after_commit` 则为 3）。
- `8.4-14`：随后 `undo()`：Buffer/快照回到 `"xa\n"`、rev3、历史 1/1/2、选区 `2:2`（Reload 首步 before-selection）；节点不删除、磁盘基线不变。
- `8.4-15`：再 `redo()`：Buffer/快照 `"xZ\n"`、rev4、历史 2/0/2、选区 `2:2`（Reload 末步 after-selection）。
- `8.4-16`：在撤销后的 `"xa\n"` 上模拟相同文本重载：`reload` 返回 `Unchanged`，rev3、节点 2、undo1/redo1、选区 `2:2` 全部不变；不调用 `prepare_record`，故 redo 分支保留。这验证「相同内容不截断 redo」与「保存点/基线分离」。
- `8.4-17`：宿主写盘与回执协调。DocCore 没有文件协调入口；状态不变，结果写 `不在 DocCore 范围`，错误写 `待定（无入口）`。磁盘写入回执归宿主（spec R3/S2）。

## 5. 本步骤相关的待确认

- T-8-09：`ClosedTarget` 的触发与检查顺序；关闭入口只用 `leave_block`/`escape`；`InputIdentity` 不含目标/块身份。
- T-8-10：`InputIdentity` 缺少解析配置代次与 request id，8.4-07/08 无法构造；错误是否需扩展。
- T-8-11：`DocError::WrongDocument` 与 `DocError::Identity(IdentityError::WrongDocument)` 的映射未定；本表用前者。
- T-8-13：DocCore 无重载/外部变化入口；冲突触发与重载后的 dg 待裁决，8.4-10/11/13–16 的 dg 为 `待定`。
- T-8-14：`accept_derived_result` 的成功返回值与 `CurrentSnapshot` 访问器未给出。
- T-8-17：撤销后开启新草稿时，Buffer 的 redo 分支是否立即截断、以及 DocCore 草稿 redo 的失效时序。
