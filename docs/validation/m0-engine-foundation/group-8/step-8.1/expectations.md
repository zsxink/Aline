# 8.1 期望：唯一 Buffer/History/OpenBlock 与当前快照

日期：2026-10-31

状态：实现前手写期望。`DocCore` 在写作时尚不存在；本文件与同目录 TSV 都不是测试通过声明，也不是 M0 出口证据。所有状态值由规范、设计文档与已有公开 API 推导，遇到文档无法裁决的值写 `待定`，并注明裁决来源。

## 1. 权威与追踪

- 需求权威：`openspec/changes/m0-engine-foundation/specs/engine/document-core/spec.md`
  - R1/S1「快照后继续输入」：快照冻结当时完整当前内容；快照不提交、不封口输入组、不压缩历史、不清 redo；之后继续输入只产生新的工作版本。
  - R1/S2「草稿基线冲突」：打开块基线与当前源版本不符时，保留草稿与选区并返回冲突；不丢弃草稿，不把旧区间套到新源上。
- 设计：`docs/engine-design.md` §4.1（`DocumentRevision`、`OpenBlock`、`WorkingVersion`）、§8.1（Buffer/OpenBlock/History 分层与 `snapshot_current`）、§8.2（输入循环与提交条件）、§9.1（单区间补丁与半开字节区间）、§9.3（历史与待定组）、§12.2（结果身份）、§17 ED-01。
- 架构：`docs/v1-architecture.md` §5.1（状态所有权）、§5.2（当前全文 = Buffer 前片段 + 草稿 + Buffer 后片段；单一草稿受控待提交补丁）、§5.4（统一历史协议、900ms、快照不封口）、§7.1（`snapshot_current` 公开契约，M0 只到内容快照契约）。
- 任务：`openspec/changes/m0-engine-foundation/tasks.md` 8.1（唯一 Buffer/History/OpenBlock；快照含最新草稿、工作版本、源码选区、资源可达引用；无窗口 `cargo test -p aline-ui document`；无 GPUI、无第九 crate）。
- 已有公开 API：
  - `crates/aline-buffer/src/history.rs`：`TYPING_MERGE_WINDOW_MILLIS = 900`、`prepare_record` 的 `now_millis - node.last_input_at <= TYPING_MERGE_WINDOW_MILLIS`、`seal_pending`、`undo_move`/`redo_move` 与 before/after 选区。
  - `crates/aline-buffer/src/string_buffer.rs`：`apply`、`undo`、`redo`、`reload`、`seal_pending`、`undo_depth`/`redo_depth`/`history_node_count`、`mark_saved`。
  - `crates/aline-buffer/src/text.rs`：`TextSnapshot::Whole/Overlay`、`with_overlay`、`id`/`revision`、半开 `SrcRange`。
  - `crates/aline-ui/src/document/identity.rs`：`DocumentId`、`DraftGeneration`、`WorkingVersion::next_draft`/`after_commit`、`InputIdentity::validate`。

## 2. 本表采用的 DocCore 语义（假设，非已确认实现）

本节把文档中能裁决的部分写成可计算模型；文档之间的冲突不在此自行裁决，统一列入 §5 与组级 README 的 `待确认`。

### 2.1 唯一草稿模型

- `DocCore` 只持有一个 Buffer、一个 History、一个 OpenBlock。OpenBlock 记录基线 revision、源码范围、基线片段、唯一草稿、`draft_generation`、源码选区、输入组状态（engine §4.1、§8.1）。
- 普通输入先进入 OpenBlock 的唯一草稿；草稿是「单一受控待提交源码替换」，不是第二份可独立保存文档（arch §5.2）。
- 当前全文 = Buffer 前片段 + 草稿 + Buffer 后片段（engine §8.1、arch §5.2）。因此本表同时给出两个文本列：
  - `buffer_text`：已提交 Buffer 文本；
  - `snapshot_text`：`current_snapshot` 返回的当前完整文本（含草稿/组合候选）。
- 草稿存在时，B​uffer 文本可以落后于当前逻辑文本；这正是 R1/S1「快照文本等于最新草稿」的观察点。

### 2.2 分组与提交

- 连续相邻普通输入在 `now_millis >= 组内最后输入时间` 且 `now_millis - 最后输入时间 <= 900` 时并入同一草稿组；该 `<=` 与 `StringBuffer::prepare_record` 的窗口判断同义，常量取自 `TYPING_MERGE_WINDOW_MILLIS`（history.rs）。
- 非相邻输入，或时间差超过 900，构成组边界：DocCore 先把上一组作为单个单区间补丁提交到 Buffer（源为 `OperationSource::Typing`），随后封口（等价于 `seal_pending` 的语义），再以本次输入开启新草稿。
- `commit_open_block(now)` 提交当前草稿组并封口，但不关闭块；`leave_block(now)` / 非组合态 `escape(now)` 提交剩余组、封口并关闭块。提交不产生额外空步；无草稿时提交为无变化（engine §8.2、§8.3、§9.3）。
- 封口动作本身不改变文本、revision、选区或草稿；它只阻止后续组被 Buffer 的 `prepare_record` 误并。表的 `node_count` 期望按「每个 DocCore 组一个 History 节点」给出。
- 快照不执行上述任何提交/封口/裁剪/清 redo 动作（R1/S1、engine §8.1、arch §5.4）。

### 2.3 版本推进

- `WorkingVersion = (DocumentRevision, DraftGeneration)`（engine §4.1、arch §5.1）。
- 只改变草稿的转换（普通输入入草稿、组合更新、组合取消、待定草稿组的 Undo/Redo）使用 `WorkingVersion::next_draft`：revision 不变，draft generation +1。
- 真正写入已提交内容的转换（组提交、组合确认提交、已提交历史的 Undo/Redo、重载）使用 `WorkingVersion::after_commit(new_revision)`：revision 前进，draft generation +1。
- 一次 `type_text` 若同时触发「提交旧组 + 开启新草稿」，按两次转换计：旧组提交 revision +1、draft +1，新草稿再 draft +1（合计 draft +2）。该口径列入 §5 的 T-8-04，供实现确认是否保持两次可观察转换。
- 快照、等待开始/结束、结果采纳在成功时不改变版本。

### 2.4 快照

- `current_snapshot(text_budget)` 返回当前不可变一致快照：文本为 Buffer 加最新草稿/组合候选，附当前 `WorkingVersion`、源码选区、资源可达引用（spec R1/S1、engine §8.1、arch §7.1）。
- 同一工作版本上重复读取/clone 应保留同一 `SnapshotId`；草稿文本或组合候选实际变化时产生新的 `SnapshotId`（arch §5.4、group-3 `public-boundary-plan.md`）。
- 快照不调用 `seal_pending`、不 `trim_history`、不清 redo、不写保存点（R1/S1、engine §9.3、arch §5.4）。
- `CurrentSnapshot` 的具体读取访问器未在给定契约中列出；本表按内容语义给期望，访问器形状列入 §5 的 T-8-14。

### 2.5 打开块、关闭与错误

- 给定契约没有打开/激活块的入口；本表所有需要打开块的用例把「块已打开」写入前置条件，并标注打开入口待确认（T-8-01）。测试若无法只靠给定 API 建立该前置，需由裁决补充入口。
- 无打开块时，`commit_open_block`/`leave_block`/`escape` 期望返回 `DocError::NoOpenBlock`；该映射由错误枚举存在性推导，列入 T-8-01。
- 关闭入口同样不在给定契约中；本表用 `leave_block`/`escape` 作为关闭动作，目标被外部关闭的 `ClosedTarget` 路径放在 8.4。
- 草稿基线冲突的触发需要「外部变化使打开块基线 revision 落后于 Buffer 当前 revision」；契约未列出该外部变化入口（重载/宿主改源），冲突行的工作版本字段写 `待定` 并引用 T-8-13。

## 3. 状态表记法

同目录 `state-tables/open-block-snapshot.tsv` 使用制表符分隔，`#` 开头为列说明，随后是表头行。约定：

- 文本一律用双引号包裹，内部转义 `\n`、`\t`、`\\`、`\"`；偏移全部是 UTF-8 源码字节，区间半开。
- `now_millis` 为虚拟毫秒；`-` 表示该操作没有时间参数。
- `buffer_text` 是已提交 Buffer 文本；`snapshot_text` 是当前逻辑全文（无草稿时两者相同）。
- `rev` 从 0 开始；`dg`（draft generation）从 0 开始；`undo`/`redo` 是深度；`node` 是历史节点数。
- 选区 `a:h` 为字节偏移；本组用例均为折叠选区，未特别说明时 anchor = head。
- `open_block`：`open(base=R, draft="…"|-, sel=a:h)` / `closed` / `none`。
- `composition`：`none` / `active(buf_range=a..b, cand="…")`；`buf_range` 是组合开始时的 Buffer 原始区间，逻辑 marked 范围为 `a .. a+cand.len()`。
- `wait`：`none` / `W1 pending` / `W1 cancelled`。
- `snapshot_id`：`new`（相对上一条同表用例）、`same`（与上一条相同版本读取相同）、`n/a`（未产生快照）。
- `outcome` 为语义结果（提交一组、无变化、返回快照等）；`CommitOutcome`/`CurrentSnapshot` 的具体枚举与访问器形状列入 T-8-14。
- `error` 使用 `DocError` 契约中的变体名；Buffer 错误写 `Buffer(BufferError::…)`。
- `spec_ref` 图例：`spec:R1/S1`、`spec:R1/S2`、`spec:R2/S1`、`spec:R2/S2`、`spec:R3/S1`、`spec:R3/S2`；`engine:8.1`…；`arch:5.1`…；`buf:history`、`buf:string_buffer`、`ui:identity`。

固定夹具（TSV 的 `precondition` 列会重复关键状态，避免跨行猜测）：

- F1：`DocCore::new("x\n", (1,1), limits)`；Buffer `"x\n"`（2 字节），rev0，dg0，无历史，块已打开于 `1..1`，草稿为空，选区 `1:1`。
- F2：`DocCore::new("abcd\n", (4,4), limits)`；Buffer `"abcd\n"`（5 字节），rev0，dg0，无历史，块已打开于 `0..4`，草稿为空，选区 `4:4`。
- F4：`DocCore::new("中\n", (3,3), limits)`；Buffer `"中\n"`（4 字节：`中` 为 0..3，`\n` 为 3..4），rev0，dg0，块已打开于 `3..3`，选区 `3:3`。
- 外部变化夹具 X：F1 上先输入 `"a"`（草稿组，rev0/dg1，选区 `2:2`），随后宿主在 DocCore 之外把源改为 `"xZ\n"`（rev1，Reload 节点），块基线仍为 rev0，草稿与草稿选区保留。

## 4. 场景期望

### 4.1 开块输入 → 快照 → 继续输入（R1/S1）

序列：F1 → `type_text(1,"a",0)` → `current_snapshot` → `type_text(2,"b",100)` → `current_snapshot` → `commit_open_block(200)` → 快照 → `type_text(3,"c",300)`。

- 首次输入只推进草稿：Buffer 仍为 `"x\n"`，快照文本 `"xa\n"`，rev0，dg1，历史 0/0/0，选区 `2:2`。
- 快照后继续输入 `"b"` 仍属同一草稿组（相邻且 100 ≤ 900），历史保持 0/0/0；若快照曾提交或封口，提交时会多出节点/多出 revision，故该行同时验证「快照不提交、不封口输入组」。
- 继续输入产生新的 `draft_generation`（dg1 → dg2）；快照文本始终等于最新草稿（`"xab\n"`），而不是 Buffer 的 `"x\n"`。
- `commit_open_block` 把 `"ab"` 作为一个单区间补丁写入 Buffer，revision 1，dg3，节点 1，选区保持 `3:3`，块仍打开；提交不产生第二节点。
- 提交后的快照文本与 Buffer 文本相同；再次输入 `"c"` 开启新草稿，Buffer 仍为 `"xab\n"`，快照为 `"xabc\n"`，rev1，dg4，节点仍为 1。
- 对应用例：`8.1-01`、`8.1-02`、`8.1-03`、`8.1-04`、`8.1-05`、`8.1-06`、`8.1-07`、`8.1-08`、`8.1-09`。

### 4.2 快照不压缩历史、不清 redo（R1/S1）

序列：F1 输入 `"a"` → 提交（节点 1）→ 输入 `"b"` → 提交（节点 2）→ Undo（撤销节点 2，redo=1）→ `current_snapshot` → Redo。

- 快照发生在 Undo 之后：历史节点数仍为 2，undo=1、redo=1；快照文本为 `"xa\n"`，rev3，dg5。
- 快照后 Redo 仍可执行，恢复到 `"xab\n"`，rev4，dg6，undo=2、redo=0，选区为节点 2 的 after-selection `3:3`。若快照清了 redo，Redo 会失败，故该序列验证 R1/S1 的 redo 保持。
- 快照不压缩历史：两条节点的深度与节点数在快照前后相同。
- 对应用例：`8.1-10`、`8.1-11`、`8.1-12`、`8.1-13`、`8.1-14`、`8.1-15`、`8.1-16`。

### 4.3 草稿基线冲突（R1/S2）

夹具 X 上分别执行 `current_snapshot`、`type_text`、`commit_open_block`、`leave_block`。

- 期望均返回 `DocError::DraftBaselineConflict`，且完整状态逐列保持不变：Buffer `"xZ\n"`、rev1、历史 1/0/1、草稿 `"a"`、草稿选区 `2:2`、块仍打开（基线 rev0）。
- 不把草稿的旧区间 `1..1` 套到新源上；不丢弃草稿；`leave_block` 冲突时也不关闭/丢弃块（`不 drop_open`）。
- 工作版本中的 dg 取决于外部变化入口是否推进版本；该入口未在契约中，故冲突行 dg 写 `待定`，裁决来源为 T-8-13。其余列不受该未知值影响。
- 对应用例：`8.1-17`、`8.1-18`、`8.1-19`、`8.1-20`。

### 4.4 唯一打开槽位与输入校验

- 块关闭后再 `commit_open_block` 期望 `NoOpenBlock`，状态不变；这验证不存在隐藏的第二个打开槽位（一个 DocCore 只有一个 OpenBlock）。
- 多字节字符中间偏移输入：F4 上 `type_text(1,"a",0)` 期望 `Buffer(NotCharBoundary { offset: 1 })`，文本、版本、历史、选区、块状态全部不变；对应 engine §9.1 的 UTF-8 起点校验与 §10.2 的字符边界要求。
- 对应用例：`8.1-21`、`8.1-22`。

## 5. 本步骤相关的待确认

完整跨步骤清单见 `group-8/README.md`；与本步骤直接相关的条目：

- T-8-01：给定契约没有打开/激活块入口；所有「块已打开」前置无法只靠契约 API 建立。`NoOpenBlock` 的映射也依赖该入口。
- T-8-02：普通输入在提交前只改草稿（本表采用），还是逐键写入 Buffer 并由 `prepare_record` 合并？两者在未提交时的 revision/dg 与待定组归属不同。
- T-8-03：待定草稿组的 Undo/Redo 只改草稿，本表用 `next_draft`；`identity.rs` 对 `after_commit` 的注释把 Undo/Redo 一并列为 revision 前进，是否包含待定组撤销需裁决。
- T-8-04：一次 `type_text` 跨 900ms 边界时，本表按「提交旧组 + 新草稿」两次版本转换计（dg+2）；若实现合并为一次公开转换，表中的 dg 需调整。
- T-8-13：外部源变化/重载入口缺失；草稿基线冲突的触发方式与冲突行 dg 待裁决。
- T-8-14：`CurrentSnapshot` 的读取访问器、`CommitOutcome` 变体形状未给出；TSV 只表达语义。
