# 8.2 期望：显式虚拟时间下的输入分组

日期：2026-10-31

状态：实现前手写期望。`DocCore` 尚不存在；本文件与 TSV 不是测试通过声明，也不是 M0 出口证据。状态值由规范、设计文档和已有公开 API 推导，无法裁决的值写 `待定` 并注明裁决来源。

## 1. 权威与追踪

- 需求：`openspec/changes/m0-engine-foundation/specs/engine/document-core/spec.md`
  - R2/S1「同块多组与离块」：同一块内多组输入；提交后块保持打开；离块/Escape 只处理剩余组；Escape 不丢输入、不重复整块记步。
  - R2/S2「Undo 与组合取消」：存在待定组时 Undo 优先撤销待定组并恢复选区；Redo 恢复同一组及对应选区。
  - R1/S1「快照后继续输入」：快照不封口/不压缩历史，之后继续输入属于同一输入协议。
- 设计：`docs/engine-design.md` §8.2（输入循环 step 4 相邻普通输入 900ms、step 5 封口/提交）、§8.3（点击他块/离块/Escape、快照、Undo/Redo 事件表）、§9.1（单区间补丁）、§9.3（900ms 仅用于连续相邻普通输入；Undo 优先待定组；Redo 恢复选区；新增编辑使 redo 分支失效；无变化不记步）、§10.1（模式切换先等组合安全结束，切换本身零字节/零历史步）。
- 架构：`docs/v1-architecture.md` §5.2（提交与关块分离；离开/点击/Escape 提交剩余组，Escape 不丢字；切模式把当前输入安全交给同一历史/补丁通道，切换本身零新字节/零历史步）、§5.4（未封口输入组属于同一文档历史协议；900ms；IME 确认独立）。
- 任务：`openspec/changes/m0-engine-foundation/tasks.md` 8.2。
- 已有 API：
  - `crates/aline-buffer/src/history.rs`：`pub const TYPING_MERGE_WINDOW_MILLIS: u64 = 900;`；`prepare_record` 中的条件 `now_millis >= node.last_input_at && now_millis - node.last_input_at <= TYPING_MERGE_WINDOW_MILLIS`，并要求相邻插入/删除与折叠选区；`seal_pending`。
  - `crates/aline-buffer/src/string_buffer.rs`：`apply`（`prepare_record` 后发布）、`seal_pending`、`undo`/`redo`（选区取节点首步 before-selection / 末步 after-selection）、`history_node_count`/`undo_depth`/`redo_depth`。
  - `crates/aline-edit/src/plan.rs` 与 `validate.rs`：命令路径在 `PendingInput`/`CompositionActive` 时拒绝。作为交叉检查：任何命令/模式消费方都不得绕过本步骤的待定组与组合状态；本组没有 DocCore 命令入口，命令执行不在本组范围。
- 规范文本不直接规定 DocCore 的「组」存放在 OpenBlock 草稿还是 Buffer History；本表沿用 8.1 的模型假设（待定组在唯一草稿中，提交时才成为一个 History 节点），该假设列在 T-8-02。900ms 边界规则采用与 `prepare_record` 相同的 `<=` 语义，见 §2.1。

## 2. 本表采用的语义

### 2.1 900ms 边界

- 相邻普通输入在 `now >= 组内最后输入时间` 且 `now - 最后输入时间 <= 900` 时并入同一草稿组。`900` 取自 `TYPING_MERGE_WINDOW_MILLIS`；比较符取自 `prepare_record` 的 `<=`。
- 因此：
  - 899ms：并入（`899 <= 900`），同一组；
  - 900ms：并入（`900 <= 900`，闭区间边界）；
  - 901ms：不并入（`901 > 900`），构成组边界。
- 组边界上，DocCore 先提交上一组为一个单区间补丁并封口，再开启新草稿。提交产生的 History 节点数按 DocCore 组数计：一组一个节点。
- 封口用于阻止 `prepare_record` 把两个 DocCore 组误并：即使两次提交的 `now` 相差 ≤900ms、选区相邻，期望仍是两个节点。`8.2-07`/`08` 用 90ms 间隔的两次提交验证这一点。
- 一次 `type_text` 同时完成「提交旧组 + 开启新草稿」时，本表按两次版本转换计（revision +1、dg +2），见 T-8-04。

### 2.2 选区与组边界

- 组内输入按当前逻辑文本的 UTF-8 字节偏移计算；输入后折叠选区位于插入文本之后。
- 非相邻输入（例如先在块尾输入、再把光标移到块首输入）即使时间差 ≤900ms，也是组边界；`prepare_record` 的 `can_merge` 还要求前一步 after-selection 等于当前 before-selection，本表用这一点解释非相邻用例。
- 光标移动本身不产生历史步；本表把移动写入前置条件，不单独作为操作。

### 2.3 提交、离块与 Escape

- `commit_open_block` 只提交并封口当前组，不关闭块；无待定组时是无变化，不产生空步。
- `leave_block` 提交并封口「剩余组」后关闭块；已经提交过的组不重记。
- 非组合态 `escape` 与离块同样处理剩余组，不丢弃文本、不把整个开块期间压成一个块级步骤。组合态 Escape 由平台先处理组合（见 8.3），不在本表。
- 关闭后 `escape` 期望 `NoOpenBlock`；该映射由错误枚举推导，见 T-8-01。

### 2.4 Undo/Redo 与待定组

- 有待定草稿组时，`undo()` 先撤销该组：逻辑文本恢复到组起点，选区恢复到组的 before-selection；不先提交该组、不产生历史步、不前进 revision；dg 前进（`next_draft`）。
- 没有待定组时，`undo()` 走已提交历史：revision 前进、dg 前进，选区取被撤销节点首步的 before-selection；`redo()` 取末步 after-selection。
- 有待定组且历史里还有已提交节点时，Undo 的优先顺序是先待定组、再已提交节点；本表用两次 Undo 表达。
- Redo 恢复同一组的 after-selection；待定组撤销后的 Redo 只恢复草稿，不写入历史。待定组撤销是否需要 revision 前进见 T-8-03。

### 2.5 快照与模式切换

- 快照在输入序列中间发生时不封口、不提交、不压缩历史；快照后继续输入若在 900ms 内且相邻，仍并入同一草稿组。
- 模式切换没有 DocCore 入口；宿主先取得平台组合的自然结束，再把剩余输入交给同一 `type_text`/提交协议。切换动作不调用 DocCore，故「零字节、零额外历史步」是结构性期望；本表用 `8.2-23` 记录无入口的等待态，用 `8.2-24`/`25` 记录切换后剩余输入走普通协议且只计输入自身的一步。模式入口缺失列在 T-8-15。

## 3. 状态表记法

`state-tables/typing-groups.tsv` 的列、转义、夹具和标签与 8.1 相同；补充：

- `precondition` 里 `P_a` 表示 F1 上已输入 `"a"`（`type_text(1,"a",0)`）的状态：Buffer `"x\n"`、快照 `"xa\n"`、rev0、dg1、历史 0/0/0、选区 `2:2`、草稿 `"a"`。
- `P_ab` 表示 F1 上已输入 `"a"`、`"b"`（`type_text(1,"a",0)`、`type_text(2,"b",100)`）的状态：Buffer `"x\n"`、快照 `"xab\n"`、rev0、dg2、历史 0/0/0、选区 `3:3`、草稿 `"ab"`。
- `P_x` 表示 F2 上已在块尾输入 `"X"`（`type_text(4,"X",0)`）的状态：Buffer `"abcd\n"`、快照 `"abcdX\n"`、rev0、dg1、历史 0/0/0、选区 `5:5`、草稿 `"X"`。
- `outcome` 中「跨界」表示该次调用先提交旧组、再开启新草稿；`dg+2` 的说明见 T-8-04。
- `open=closed` 表示块已关闭；关闭后不会再出现第二个 OpenBlock。

## 4. 场景期望

### 4.1 900ms 前后边界（R2/S1）

- 同一前置 `P_a`，第二次输入时间分别为 899、900、901：
  - 899：`8.2-01`，同一草稿组，历史仍 0/0/0，dg2，快照 `"xab\n"`；提交后 `8.2-04` 为 1 个节点。侧别：合并侧（`899 <= 900`）。
  - 900：`8.2-02`，同一草稿组，历史仍 0/0/0，dg2；提交后 `8.2-05` 为 1 个节点。侧别：合并侧（`900 <= 900`，闭区间）。
  - 901：`8.2-03`，组边界：旧组 `"a"` 先提交（Buffer `"xa\n"`、rev1、节点 1），新草稿 `"b"`（dg3，快照 `"xab\n"`）；提交后 `8.2-06` 为 2 个节点。侧别：拆分侧（`901 > 900`）。
- 常量与比较符来源：`crates/aline-buffer/src/history.rs` 的 `TYPING_MERGE_WINDOW_MILLIS = 900` 与 `prepare_record` 的 `now_millis - node.last_input_at <= TYPING_MERGE_WINDOW_MILLIS`。本表要求 DocCore 的组边界与该闭区间语义一致。
- 封口交叉检查：`8.2-07` 在显式提交后仅 90ms（≤900）继续输入相邻字符，期望仍开启新草稿；`8.2-08` 再提交后期望 2 个节点。如果 DocCore 未封口，`prepare_record` 会把两次提交并入一个节点，与期望不符。

### 4.2 非相邻输入（R2/S1）

- `8.2-09`：F2 块内先在偏移 4 输入 `"X"`，再把光标移到偏移 0，在 100ms 内输入 `"Y"`。时间差在窗口内，但两次输入不相邻（前一 after-selection `5:5` 不等于后一 before-selection `0:0`），故 `"X"` 组先提交、`"Y"` 开启新草稿。
- 期望：Buffer `"abcdX\n"`、快照 `"YabcdX\n"`、rev1、dg3、节点 1、选区 `1:1`；提交后 `8.2-10` 为 2 个节点。时间不是唯一条件，相邻性与选区连续同样是组边界条件（engine §8.2-4、history.rs `can_merge`）。

### 4.3 同块多组提交、离块与 Escape（R2/S1）

- `8.2-04`/`05`/`08`/`10` 中每次 `commit_open_block` 后块都保持 `open`；无待定组时 `8.2-11` 的提交结果为无变化，状态不变，不产生空步。
- `8.2-12`：F2 已提交 `"X"`（节点 1），再输入 `"Y"`（草稿），`leave_block` 只提交 `"Y"`：Buffer `"abcdXY\n"`、rev2、dg4、节点 2、选区 `6:6`、块关闭。节点数不是 3（未重记 `"X"`），也不是 1（未把整块压成一步）。
- `8.2-13`：同一前置下 `escape` 与离块结果相同：提交剩余 `"Y"`、关闭块、文本不丢、无额外步。`8.2-14`：关闭后再 Escape 期望 `NoOpenBlock`，状态不变。

### 4.4 Undo 待定组优先与 Redo 选区（R2/S2）

- `8.2-15`：`P_ab` 上 Undo 撤销待定组 `"ab"`：Buffer/快照回到 `"x\n"`、rev0、dg3、历史 0/0/0、选区组起点 `1:1`、草稿清空。没有先提交整组，也没有新增节点。
- `8.2-16`：Redo 恢复同一草稿组：快照 `"xab\n"`、选区 `3:3`（组 after-selection）、历史仍 0/0/0、rev0、dg4。
- `8.2-17`：已有节点 1（`"a"`）且存在草稿 `"b"` 时，Undo 先撤销草稿：Buffer/快照 `"xa\n"`、rev1、dg4、节点 1、选区 `2:2`（草稿 `"b"` 的 before-selection），节点 1 保持。
- `8.2-18`：第二次 Undo 才撤销节点 1：Buffer `"x\n"`、rev2、dg5、undo0/redo1、节点 1、选区 `1:1`（节点 1 before-selection）。
- `8.2-19`：Redo 恢复节点 1：Buffer `"xa\n"`、rev3、dg6、undo1/redo0、选区 `2:2`（节点 1 after-selection）。该序列对应 engine §9.3「Undo 优先待定组、不先制造重复整块提交；Redo 恢复同一组及选区」。

### 4.5 快照与继续输入交错（R1/S1）

- `8.2-20`：`P_a` 上取快照，状态不变，`SnapshotId` 为当前草稿视图的 id；不封口。
- `8.2-21`：快照后 100ms 输入 `"b"`，仍并入同一草稿组，历史 0/0/0，dg2，快照文本 `"xab\n"`。
- `8.2-22`：提交后只有 1 个节点。若快照曾封口/提交，节点数或 revision 会不同，故该序列是 R1/S1 在输入分组上的交叉验证。

### 4.6 模式切换（协议性，无 DocCore 入口）

- `8.2-23`：组合活动时宿主请求切模式。DocCore 契约没有模式入口，期望状态不变、组合保持 `active`，不猜测平台确认/取消；返回/错误写 `待定` 并注明 T-8-15。依据 engine §8.3 与 §10.1：模式切换先取得组合安全结束，不偷焦点、不伪造组合结束。
- `8.2-24`：平台自然确认（`composition_commit`）后，宿主再输入 `"a"`：Buffer 仍为 `"x中\n"`，快照 `"x中a\n"`，rev1、dg3、节点 1、选区 `5:5`、草稿 `"a"`；`8.2-25` 提交后 Buffer `"x中a\n"`、rev2、dg4、节点 2。
- 切换动作本身不调用 DocCore，因此不产生字节、不产生历史步；`8.2-24`/`25` 的两次版本推进都来自组合确认与随后输入，不是切换。该结论依赖「模式入口不存在」这一契约事实，缺口列在 T-8-15。

## 5. 本步骤相关的待确认

- T-8-01：打开/关闭块入口缺失；本表以 `leave_block`/`escape` 作为关闭动作，`NoOpenBlock` 的映射待裁决。
- T-8-02：待定组位于 DocCore 草稿还是 Buffer History；本表采用草稿模型。若改为逐键写 Buffer，未提交时的 revision/dg 与本表不同。
- T-8-03：待定组 Undo/Redo 是否只推进 draft generation；本表按 `next_draft`。
- T-8-04：跨 900ms 边界的单次 `type_text` 是否按两次版本转换（dg+2）暴露；本表按两次计。
- T-8-13：外部源变化/重载入口缺失（本步骤未用到冲突触发，但组提交的基线检查依赖它）。
- T-8-15：DocCore 无模式切换入口；本表只能给出协议性期望。
