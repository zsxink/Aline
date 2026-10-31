# 8.3 期望：组合与发布等待的纯状态

日期：2026-10-31

状态：实现前手写期望。`DocCore` 尚不存在；本文件与 TSV 不是测试通过声明，也不是 M0 出口证据。状态值由规范、设计文档和已有公开 API 推导，无法裁决的值写 `待定` 并注明裁决来源。

## 1. 权威与追踪

- 需求：`openspec/changes/m0-engine-foundation/specs/engine/document-core/spec.md`
  - R2/S2「Undo 与组合取消」：组合取消恢复组合前状态，不产生空步；旧投影代次不复活。
  - R3/S1「确认取消与取消请求」：组合期显式发布进入可取消等待，平台自然确认/取消后取当前快照；取消发布请求不改输入/历史；自动保存延后；恢复保护是独立协议。
  - R1/S1「快照后继续输入」：快照含最新草稿；取快照不封口输入组、不记录历史步、不清 redo。
- 设计：`docs/engine-design.md` §8.3（保存/恢复/导出快照、组合期自动保存延后、显式发布等待）、§9.3（组合取消不产生空步、不必要清 redo）、§10.1（组合状态字段与事件表：开始/更新替换/确认/取消/失焦/切模式/Escape/自动保存/保存）、§10.2（UTF-8 与字符边界）、§12.2（请求身份与旧 request、结果键）、§17 ED-01（组合期显式保存/另存/导出可取消等待，平台自然结束后取当前快照；取消发布请求不改当前输入/组合；快照不封口/不记步/不清 redo；自动保存仍延后；恢复仍保护当前输入/临时资源）。
- 架构：`docs/v1-architecture.md` §5.1（IME 组合状态属于编辑状态机，GPUI 只转发；marked 范围要映射；组合不是第二份可独立提交文档）、§5.2（不偷焦点、不伪造组合结束、保存/恢复/导出走统一当前文本快照）、§5.4（组合取消恢复组合前状态、无空历史步；保存/恢复/导出只取当前快照）、§7.1（`snapshot_current` 的 ED-01 等待说明；M0 只到内容快照契约）。
- 任务：`openspec/changes/m0-engine-foundation/tasks.md` 8.3。
- 已有 API：
  - `crates/aline-buffer/src/text.rs`：`TextSnapshot::with_overlay(base, replaced, draft, budget)`、`SnapshotId`、`Whole/Overlay`。
  - `crates/aline-buffer/src/string_buffer.rs`：`apply`（`OperationSource::ImeCommit` 的 `prepare_record` 只在 `source == Typing` 时合并）、`seal_pending`、`undo_depth`/`redo_depth`/`history_node_count`。
  - `crates/aline-ui/src/document/identity.rs`：`WorkingVersion::next_draft`（组合更新/取消推进草稿代次）、`after_commit`（真正写入后推进 revision 与草稿代次）、`InputIdentity::validate`（document → version → snapshot）。
  - `crates/aline-edit/src/plan.rs`/`validate.rs`：`EditContext` 的 `CompositionActive` 预检；命令路径不得在组合期写入，本组没有 DocCore 命令入口。

## 2. 本表采用的语义

### 2.1 组合状态与候选投影

- 给定契约没有独立的组合 start 方法；本表假定首次 `composition_update(text, now)` 以当前源码选区为组合范围开启组合，之后每次 `composition_update` 都是「替换当前候选」：逻辑文本 = Buffer 在组合范围上的原内容被候选文本替换，而不是在上一版候选上再插入。
- 组合状态包含组合前工作内容/选区身份、组合范围、候选文本、候选内源选区、组合代次（engine §10.1、arch §5.1）。Buffer 在组合期间不变；组合候选属于 OpenBlock 的唯一草稿，用 `with_overlay` 形成当前只读视图，不是第二份可独立保存文档。
- 候选更新只推进 draft generation，不产生历史步、不前进 revision；快照文本包含候选。组合范围的字节偏移在 Buffer 坐标中记录，逻辑 marked 范围为 `range.start .. range.start + cand.len()`。
- 组合范围的起始选区在本表全部用例中为折叠选区，故候选后的折叠选区为 `range.start + cand.len()`。非折叠组合范围的确认/取消选区行为未在文档中裁决，见 T-8-08。

### 2.2 组合确认、取消与重复事件

- `composition_commit(now)`：把候选文本作为对组合原始范围的一个单区间补丁写入 Buffer，源为 `OperationSource::ImeCommit`；产生一个独立 History 节点、一个 revision 步和一次 dg 推进；选区为候选末尾；块保持打开。`prepare_record` 只在 `source == Typing` 时合并，故 IME 确认不会与相邻普通输入误并；新节点还会把前一个 Typing 节点置为非待定（封口）。
- 无文本变化的确认（候选等于原范围内容，或空候选）走 `apply` 的 `Unchanged`：不产生节点、不前进 revision；组合结束、dg 推进。对应「无变化组不记步」。
- `composition_cancel(now)`：丢弃候选，恢复组合前文本与选区；Buffer 在组合期间未变，故文本不变；不产生历史步、不前进 revision；dg 推进，撤销后的代次不复用。旧候选的 `SnapshotId`/`WorkingVersion` 不得再被 `accept_derived_result` 接受。
- 重复 `composition_update` 事件：文本/选区/历史不变，draft generation 仍推进（engine §10.1「推进工作代次」没有为相同文本设例外）。是否必须推进见 T-8-06。

### 2.3 发布等待

- `begin_publication_wait(kind)` 在组合活动时建立等待：不取快照、不封口、不改输入/历史/版本；同一 DocCore 同时只允许一个等待，第二个 begin 期望 `PublicationInFlight`。
- `finish_publication(wait, outcome, now)` 只在平台自然确认/取消（组合已结束）后返回 `PublicationResult`；组合仍活动时期望 `CompositionActive`，等待保持。返回的快照是结束后的当前快照（含最终候选结果），发布本身不新增历史步、不封口、不清 redo。
- `request_cancel_wait(wait)` 只取消发布请求，不结束组合、不改任何输入/历史/版本；之后完成同一等待期望 `UnknownWait`。取消后组合仍可继续更新/确认/取消。
- `kind`（Save/SaveAs/Export）不改变等待状态机；本表用 Save 走完整序列，并为 SaveAs/Export 各给一条 begin 用例。`PublicationKind`、`PublicationOutcome` 的枚举形状未给出，见 T-8-14。
- 自动保存延后没有 DocCore 入口：它是宿主策略，组合活动时不得走发布快照；本表只记录协议性期望（T-8-12）。
- 恢复保护是独立协议：允许在组合期直接取 `current_snapshot`（含候选）用于恢复，不要求 begin/finish 发布等待，也不得因此确认/取消组合；恢复记录与临时资源清单不在 DocCore 契约内。

## 3. 状态表记法

`state-tables/composition.tsv` 的列、转义与标签同 8.1/8.2；补充：

- `C1`：`DocCore::new("x\n", (1,1), limits)`；Buffer `"x\n"`、rev0、dg0、历史 0/0/0、块打开于 `1..1`、选区 `1:1`。
- `open_block` 中的 `draft=组合候选` 表示草稿槽被组合候选占用，具体候选见 `composition` 列。
- `composition`：`none` / `active(buf_range=a..b,cand="…")`；`buf_range` 是 Buffer 坐标中的组合原始范围；逻辑 marked 范围由候选长度推出。
- `wait`：`none` / `W1 pending` / `W1 cancelled`；`W2`/`W3` 仅用于区分不同用例的等待句柄。
- `snapshot_id`：`n/a` 表示该操作未取快照；`same` 表示取到当前视图的同一 id；`new` 表示组合/提交/取消产生了新的当前视图。
- `outcome` 中的 `PublicationResult` 只表达「返回当前快照」的语义；具体字段形状见 T-8-14。

## 4. 场景期望

### 4.1 更新、替换、重复更新、确认（R2/S2、R3/S1）

- `8.3-01`：C1 上 `composition_update("中",0)`：Buffer `"x\n"`、快照 `"x中\n"`（5 字节）、rev0、dg1、历史 0/0/0、选区 `4:4`、组合 `active(buf_range=1..1,cand="中")`。候选只进入草稿视图。
- `8.3-02`：`composition_update("中国",10)` 是替换候选：快照 `"x中国\n"`（8 字节）、选区 `7:7`、dg2、历史仍 0/0/0、Buffer 不变。
- `8.3-03`：重复 `composition_update("中国",20)`：文本与选区不变，dg3；无历史/版本变化。重复事件是否推进代次见 T-8-06。
- `8.3-04`：`composition_commit(30)`：Buffer `"x中国\n"`、rev1、dg4、节点 1（ImeCommit）、undo1/redo0、选区 `7:7`、组合结束、块仍打开。确认是独立一步，不是把候选留在草稿里。
- `8.3-05`：确认后再开始新组合：`composition_update("文",40)` 在 `"x中国\n"` 的选区 `7:7` 开启新组合，快照 `"x中国文\n"`（11 字节）、rev1、dg5、历史仍 1、选区 `10:10`。

### 4.2 取消与旧投影（R2/S2、R3/S1）

- `8.3-06`：C1 上 `composition_update("中",0)` 后 `composition_cancel(10)`：Buffer/快照回到 `"x\n"`、rev0、dg2、历史 0/0/0、选区恢复 `1:1`、组合结束。取消不产生空步，revision 不前进；dg 前进且旧组合代次不复用。
- `8.3-07`：在 8.3-06 取消之后，用 `8.3-01` 的组合身份（rev0/dg1/旧组合 SnapshotId）调用 `accept_derived_result`：期望 `StaleResult`，完整状态不变（快照 `"x\n"`、rev0、dg2、历史 0/0/0、选区 `1:1`）。旧投影在取消后不得重新命中（engine §12.2、R2/S2）。
- `8.3-09`：先提交普通输入 `"a"`（节点 1、rev1、dg2），再组合更新 `"中"`（dg3），随后取消：Buffer/快照回到 `"xa\n"`、rev1、dg4、节点 1、undo1/redo0、选区 `2:2`。既有输入不丢，取消没有新增空步。
- `8.3-10`：空候选的确认：`composition_update("",0)` 后 `composition_commit(10)`：文本仍 `"x\n"`、rev0、dg2、历史 0/0/0、选区 `1:1`。无变化组不记步；dg 因组合结束而推进（是否推进见 T-8-07）。

### 4.3 确认独立成步（R2/S1、R3/S1）

- `8.3-08`：先提交 `"a"`（节点 1、rev1、dg2），在 900ms 内组合更新 `"中"`（dg3），再 `composition_commit(30)`：Buffer `"xa中\n"`（6 字节）、rev2、dg4、节点 2（ImeCommit）、undo2/redo0、选区 `5:5`。IME 确认独立成组，未与 Typing 节点合并。
- 该用例同时验证：组合候选不进入 Buffer 直到确认；确认只应用一个单区间补丁；提交后块保持打开。

### 4.4 显式保存/另存/导出等待（R3/S1、ED-01）

- `8.3-11`：组合活动时 `begin_publication_wait(Save)` 成功建立 `W1`；状态不变、组合保持、未取快照。
- `8.3-12`：同一组合上再次 `begin_publication_wait(SaveAs)`：期望 `PublicationInFlight`，`W1` 仍 pending，状态不变。
- `8.3-13`：`W1` 等待期间继续 `composition_update("中国",10)`：候选更新正常生效（dg2、快照 `"x中国\n"`、选区 `7:7`），`W1` 仍 pending；发布必须等到自然结束并取最终快照，而不是 begin 时的旧候选。
- `8.3-14`：组合仍活动时 `finish_publication(W1, PlatformConfirmed, 20)`：期望 `CompositionActive`，状态不变、`W1` 仍 pending；不允许发布编辑前候选或伪造组合结束。
- `8.3-15`：平台自然确认（`composition_commit(30)`）后：Buffer `"x中国\n"`、rev1、dg3、节点 1、组合结束，`W1` 仍 pending。
- `8.3-16`：`finish_publication(W1, PlatformConfirmed, 40)`：返回 `PublicationResult`，快照文本 `"x中国\n"`、rev1、dg3、选区 `7:7`、节点 1；等待移除。发布不新增节点、不封口、不清 redo。
- `8.3-17`/`8.3-18`：组合活动时分别 `begin_publication_wait(SaveAs)`/`(Export)` 均成功建立等待（`W2`/`W3`），状态不变。三种 kind 共用同一等待状态机；Save 的完整结束序列见 11–16。

### 4.5 取消发布请求（R3/S1）

- `8.3-19`：`W1` pending、组合活动时 `request_cancel_wait(W1)`：期望成功，等待变为 `W1 cancelled`；Buffer、快照、rev、dg、历史、选区、组合全部不变（零副作用）。
- `8.3-20`：随后 `finish_publication(W1, PlatformCancelled, 20)`：期望 `UnknownWait`，状态不变。
- `8.3-21`：取消发布请求后组合仍可更新：`composition_update("中国",10)` 得 dg2、快照 `"x中国\n"`、选区 `7:7`、组合活动、无等待。
- `8.3-22`：随后 `composition_cancel(20)`：回到 `"x\n"`、rev0、dg3、历史 0/0/0、选区 `1:1`；没有发布、没有历史步。

### 4.6 自动保存延后与恢复保护（协议性）

- `8.3-23`：组合活动时宿主请求自动保存。DocCore 契约没有自动保存入口；期望延后，不取发布快照、不改状态，返回/错误写 `待定`，见 T-8-12。实际自动保存调度属于 M2，不在 M0。
- `8.3-24`：组合活动时直接 `current_snapshot` 作恢复读取：返回含候选的快照 `"x中\n"`、rev0、dg1、选区 `4:4`，组合仍活动、无等待。该路径与发布等待相互独立：不建立等待、不确认/取消组合；恢复记录与资源清单不在 DocCore 内。
- `8.3-25`：组合开始时已有普通待定组，期望状态写 `待定`：一种实现先提交/封口该组再进入组合（不误并，但多一个普通组步），另一种返回 `PendingGroup` 且状态不变；规范只规定二者不得误并（engine §10.1），未裁决状态，见 T-8-05。

## 5. 本步骤相关的待确认

- T-8-05：组合开始时已有普通待定组，是先提交/封口该组、继续组合，还是返回 `PendingGroup`；本表该行状态为 `待定`。
- T-8-06：重复且文本相同的 `composition_update` 是否仍推进 draft generation；本表按推进处理。
- T-8-07：无文本变化的 `composition_commit` 是否仍推进 dg；本表按推进处理（组合结束本身是状态变化）。
- T-8-08：组合范围是否只能折叠、首次 update 是否隐式开启组合、非折叠范围确认/取消后的选区；契约没有 start/range 参数。
- T-8-11：旧组合投影的拒绝写 `StaleResult`；是否以 `Identity(IdentityError::StaleSnapshot/…)` 透传未定。
- T-8-12：自动保存延后与恢复保护没有 DocCore 入口；本表只给协议性期望。
- T-8-14：`CommitOutcome`、`PublicationResult`、`WaitHandle`、`PublicationKind`/`PublicationOutcome` 的字段与枚举形状未给出。
- T-8-16：`begin_publication_wait` 在非组合期是否允许/立即完成；ED-01 只定义组合期等待。
