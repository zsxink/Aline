# M0 公共边界最小实施建议

日期：2026-10-31。状态：只读分析形成的实施建议；尚未实施任务 3、未新增 Rust API、未执行下列 API 测试。组 2 完成后才按编号实施。依据本 change design/Public Boundaries、已批准原始 CR/LF 表示、buffer/edit/document-core delta，以及引擎 §4/8/9。名称是建议，允许实现时采用等价且更清楚的名称；不允许借改名缩减边界。

## 1. 推荐结论

采用私有字段 newtype、实例局部 `Rc` 身份、具体的 Whole/SingleOverlay 只读文本表示。DocCore 持有唯一可写 Buffer/History/OpenBlock；低层只消费不可变快照。UI 身份及完整 WorkingVersion 放在 UI 请求/结果信封中，不反向传入引擎类型。

`Rc<String>` 适合共享完整已提交文本；单靠它不能解决草稿每键重建全文。SingleOverlay 只覆盖一个块，直接消费既有“至多一个草稿”需求，不是通用 rope、注册器、trait 或第二可写文档。

## 2. 当前约束与类型归属

| home | 最小类型 | 约束与消费方 |
|---|---|---|
| buffer | `TextSnapshot`、`SnapshotId`、`DocumentRevision` | UTF-8 不可变源码、实例身份、已提交单调代次；markdown/edit/UI 消费 |
| buffer | `SrcOffset`、`SrcRange`、`SrcSelection` | 私有数值及所属 SnapshotId；受检字符边界；选区保留 anchor/head 方向 |
| buffer | `CheckedBatch`、候选补丁结果 | 底层保持 `Vec<(Range<usize>, String)>`，同 before 快照；不理解存储或 Markdown |
| markdown | `ProjectionId`、`DrawnOffset`、`Utf16Offset`、映射结果 | offset 绑定投影；局部/全文/恒等/UTF-16 均通过同一映射模块 |
| source | `BaselineId`、`DiskVersion`、`StorageRange`、内容证据 | 不依赖 buffer/UI；存储范围绑定基线，不混用 SrcRange |
| UI document | `DocumentId`、`DraftGeneration`、`WorkingVersion`、请求/结果信封 | 绑定活文档、已提交 revision、不可复用草稿代次；不下沉整个 UI 类型 |

`SnapshotId`、`ProjectionId`、`DocumentId`、`BaselineId` 均可用各自模块私有 `Rc<Identity>` token；只有合法实例创建过程能新建 token，clone 保持身份，比较用 `Rc::ptr_eq`。不提供从整数/地址/string 构造身份的方法。证据保留 Rc 引用，旧 token 仍存活时不会地址复用。不需要全局计数、随机依赖、unsafe 或线程。

数字代次私有封装，递增使用受检运算，耗尽返回枚举错误。DocumentRevision 跨文档可能同值，但 SnapshotId/DocumentId 区分上下文。不要把版本整数、内容相等或相同路径当作身份。

## 3. 推荐具体方案与消费签名

下述签名是契约草图，不是当前已存在或可调用的 API；实现时为实际消费方落地并验证，不提交 `todo!()`、`unimplemented!()` 或假返回占位。

### 3.1 不可变文本与单草稿

`TextSnapshot` 内部持 SnapshotId、DocumentRevision 和具体 backing：

- `Whole`：共享完整 `Rc<String>`。
- `SingleOverlay`：共享一个 Whole 基线、被替换的受检 SrcRange、共享当前块 `Rc<String>` 草稿；禁止 backing 递归叠套。DocCore 始终从已提交 Buffer 的 Whole 建草稿视图。

建议操作：

```text
TextSnapshot::new(text: String) -> Result<TextSnapshot, BufferError>
TextSnapshot::with_overlay(base: &TextSnapshot, replaced: SrcRange,
                          draft: Rc<String>, budget: TextBudget)
    -> Result<TextSnapshot, BufferError>
snapshot.id() -> SnapshotId
snapshot.revision() -> DocumentRevision
snapshot.len_bytes() -> usize
snapshot.is_char_boundary(src_off: usize) -> bool
snapshot.iter_chunks() -> 三段以内的只读 &str 迭代
snapshot.slice(range: &SrcRange) -> Result<Cow<str>, BufferError>
snapshot.materialize(budget: TextBudget) -> Result<TextSnapshot, BufferError>
```

`with_overlay` 检查基线身份、范围、UTF-8 边界、禁止叠套、预算及 `prefix + draft + suffix` 的受检长度。`slice` 在片段内借用，跨片段只物化所请求范围；迭代和物化不改变文本。`materialize` 仅改变 backing，保留 SnapshotId，因为文本与输入身份没有变化。

每次实际草稿变化创建新的 SnapshotId；同版本读取/clone 保留原 id。DocCore 在输入发布时更新其当前只读视图，`snapshot_current` 克隆该视图，不能每次读取都另造 id。这样同一 WorkingVersion 的计划和映射可稳定消费同一快照。

OpenBlock 的可编辑草稿可持 `Rc<String>`，输入时只对块草稿作 copy-on-write；旧快照留下的是不可变旧草稿。已提交全文仍共享。显式 parse、encode 或命令需要连续 `&str` 时再在预算内物化；不在每键、命中或选区移动中物化全文。源码/存储仅由 source 转换；原始 CR/LF 不在这里规范化。

### 3.2 源码坐标、补丁与候选

```text
snapshot.src_offset(byte: usize) -> Result<SrcOffset, BufferError>
snapshot.src_range(bytes: Range<usize>) -> Result<SrcRange, BufferError>
SrcSelection::new(anchor: SrcOffset, head: SrcOffset)
    -> Result<SrcSelection, BufferError>
CheckedBatch::new(before: &TextSnapshot,
                  patches: Vec<(Range<usize>, String)>)
    -> Result<CheckedBatch, BufferError>
prepare_batch(before: &TextSnapshot, batch: &CheckedBatch,
              before_selection: &SrcSelection, budget: PatchBudget)
    -> Result<BatchPreview, BufferError>
preview.after_snapshot() -> &TextSnapshot
preview.with_after_selection(selection: SrcSelection)
    -> Result<PreparedBatch, BufferError>
buffer.publish(prepared: PreparedBatch) -> Result<ApplyOutcome, BufferError>
```

构造 offset/range 时检查实际快照 UTF-8 边界；使用时再次核对身份。SrcSelection 不把反向选区改成正向。tuple 补丁是受检表示边界，不把 newtype 改成 type alias，不给 CheckedBatch 可变 Vec。

`prepare_batch` 完整验证排序/不重叠/同点歧义/所有补丁/累计长度、构建 after 文本、行索引和 after 坐标逆补丁；此时真实 Buffer 不变。调用方在 after_snapshot 上构造目标 SrcSelection，再形成可发布候选，避免拿 before 坐标或未经检查的裸 after 下标当选区。

实际发布复验 before SnapshotId/revision。必要内存、历史预算、选区与组处理都须在候选阶段完成。无变化返回明确 NoChange，不制造新 revision/历史步或清 redo。真正提交、Undo/Redo/重载每次推进受检 revision 并创建新 SnapshotId，即使新内容与更早文本相同，也不复活旧计划。

公开 buffer 写方法供文档所有者使用；安全边界是 DocCore 的私有 Buffer 字段且不给其他消费者 `&mut Buffer`。edit 仅拿 `&TextSnapshot`，不会得到真实 Buffer。不要为限制调用 crate 引入无需求的能力注册框架。

### 3.3 映射与投影

```text
parse(snapshot: TextSnapshot, config: &ParseConfig, budget: ParseBudget,
      cancellation: &Cancellation) -> Result<Parsed, MarkdownError>
Projection::identity(snapshot: TextSnapshot, source: SrcRange)
    -> Result<Projection, MappingError>
projection.drawn_offset(byte: usize) -> Result<DrawnOffset, MappingError>
projection.utf16_offset(unit: usize) -> Result<Utf16Offset, MappingError>
projection.source_at(drawn: DrawnOffset, affinity: Affinity)
    -> Result<Mapped<SrcOffset>, MappingError>
projection.drawn_at(source: SrcOffset, affinity: Affinity)
    -> Result<Mapped<DrawnOffset>, MappingError>
projection.source_from_utf16(offset: Utf16Offset, affinity: Affinity)
    -> Result<Mapped<SrcOffset>, MappingError>
projection.utf16_from_source(offset: SrcOffset, affinity: Affinity)
    -> Result<Mapped<Utf16Offset>, MappingError>
projection.aligned(selection: SrcSelection)
    -> Result<SrcSelection, MappingError>
```

映射返回 Exact/Aligned/Hidden/Synthetic 等精确性，不能把所有关系假装双射。identity projection 也走相同转换入口；Source 全文和打开块局部投影各有独立 ProjectionId。源码坐标始终是全文当前视图坐标，局部起点差、片段定位、字素/UTF-16 表均封在映射模块。DrawnOffset 在另一投影中使用必须失败；UTF-16 中间代理对、半 UTF-8、范围外来源、旧快照一律受检失败，不 clamp/backscan。

任务 3 可先用真实 identity projection 外部消费者固定类型与失败边界；完整 parse、Inline 语义、来源精确性与复杂映射属于组 6，文档应写“契约未验证”，不能用 identity 测试冒充完成。

### 3.4 纯计划及 UI 信封

```text
plan_change(snapshot: &TextSnapshot, parsed: &Parsed,
            selection: &SrcSelection, command: Command, budget: EditBudget)
    -> Result<Change, EditError>
validate_change(current: &TextSnapshot, parsed: &Parsed,
                change: &Change, budget: EditBudget)
    -> Result<ValidatedChange, EditError>
DocCore::snapshot_current(&self) -> Result<DocumentSnapshot, DocumentError>
DocCore::apply_change(&mut self, planned: ChangeEnvelope)
    -> Result<ApplyOutcome, DocumentError>
```

Change 绑定低层 SnapshotId/revision、用过的 ProjectionId、预期片段、补丁/候选目标选区及命令保留条件；plan/validate 只读，在候选上预演/reread。UI 的具体 ChangeEnvelope 另持 DocumentId、完整 WorkingVersion、模式/配置/请求上下文。Parsed 请求/结果也由 UI 具体信封携带完整键，不增加通用消息总线。

WorkingVersion 在 UI 定义为已提交 DocumentRevision 加会话内不可复用 DraftGeneration；取消组合、更新草稿也推进后者，不能因为文本恢复就复用代次。`DocumentSnapshot` 包含当前 TextSnapshot、该信封身份、SrcSelection 和不可变可达资源引用；读取本身不提交、不封口、不改历史。

DocCore 最终入口在完整候选状态上准备“待定组处理 + 命令 + Buffer/History/选区/OpenBlock 更新”，复验文档/工作版本/快照/投影/配置后一次发布。任何拒绝保留真实状态。不要先封口组再验证命令，也不让 UI 手工修正旧范围后绕过验证。

### 3.5 历史上下文重绑定

HistoryStep 可以保留底层正逆 tuple 补丁、原选区数值、预期源片段与 before/after 历史节点身份。Undo/Redo 先确认当前历史节点上下文，再由 buffer/history 内部对当前 Snapshot 重建受检 batch 与选区。不能原样重放绑定旧 SnapshotId 的 CheckedBatch，也不能让 UI 重新贴 id。

这使合法历史能在新 revision 上恢复，而普通旧计划依然失败。重绑定必须核验历史分支、当前预期片段、合法范围及目标文本/选区，不能只是复制旧数字。磁盘基线和 DiskVersion 不随历史游标倒退。

## 4. 关键取舍

- SingleOverlay 增加少量三段文本读取代码，换来不每键复制全文；只支持现有单草稿，不预建多 overlay/rope。
- Rc token 不可序列化为跨进程身份，适合 M0 会话内真实消费者。恢复/平台并发不因此被宣称完成。
- 源码范围绑定快照使 stale 错误显式，也要求历史通过受检上下文重绑定。不能为省样板改成裸 usize。
- 连续文本物化与候选内存必须有显式预算；M0 注入预算只验证边界，不代表 V-05/V-06 的发布数值已确定。

## 5. 风险和回滚方式

本文件仅新增建议，无 Rust 改动。实施时若类型无法被实际消费者使用，保留外部失败用例再调整名称或内部表示；不得绕开 identity 检查、下沉 UI 类型或加入反向依赖。关键风险是 overlay 每次读取误造 id、取消后复用草稿代次、历史重放旧绑定、物化导致每键全文复制，以及候选阶段修改真实组状态；均由下列独立反例验证。

## 6. 任务 3 实际外部消费者测试点

实施后保存实际命令、退出码、夹具路径、独立预期；目前全部未执行。

- buffer 外部集成测试：合法空范围/文末/反向选区；半中文 UTF-8、越界、反范围、长度溢出；两份同文本快照坐标交叉使用失败，clone 坐标可用。
- 编译失败夹具：SrcOffset、DrawnOffset、Utf16Offset、StorageRange 不能互传，不能直接构造私有字段或用类型别名绕过；合法外部消费例单独编译成功。
- Whole/SingleOverlay：前/草稿/后各段与跨段 slice 手工预期，中文边界、空替换/末端、嵌套 overlay 拒绝；旧快照在继续输入后保持不变，物化前后文本和 SnapshotId 相同。
- snapshot_current 的同版本读取保留 id；输入变化产生新 id，不复制基线全文。可用 `Rc::ptr_eq` 的测试内观察或受限诊断证明共享，而非只凭函数名宣称。
- identity mapping：打开块局部 UTF-16 0 与全文非零 src 起点经映射正确转换；中文/emoji、代理对中间失败、同文本不同投影失败；调用端不手算局部差。
- 正逆补丁形状保持 tuple；旧 Snapshot 的 batch 对新 Snapshot 拒绝。真实批次应用/历史恢复属于组 5；组 3 未实现者只能标契约未验证。
- UI 信封实际外部消费者：错误 DocumentId、WorkingVersion/草稿代次、ProjectionId 拒绝；低层依赖图仍无 UI/GPUI 反向边。DocCore 完整状态消费属于组 8，不能预报通过。
- source 自有身份边界：不同基线相同字符串不能互借证据；StorageRange 与 SrcRange 编译不互通。真实 decode/encode/基线替代属于组 4。

阶段 3 若先落地基础类型和 identity mapping，parse/plan/apply/snapshot 的尚未实现部分通过本契约及实际消费签名记录“契约未验证”，后续按任务实现；不预建无消费者后端，不提交占位方法充完成数。

## 7. 不推荐方案及原因

- 只用 `Rc<String>` 却每键生成完整当前文本：共享指针不能消除物化成本。
- 叠加 overlay、通用 rope/trait 或身份注册器：超出现有单草稿需求。
- 只比较 revision 或 String：不能拒绝错文档、错投影、取消后旧请求与 Undo 同文本旧计划。
- UI 导出 `buffer_mut` 或 edit 接受真实 `&mut Buffer`：破坏唯一写所有者。
- UI 为历史旧范围换 id、手算块起点差或 UTF-16：绕过受检上下文及映射边界。
- 未实现 parse/plan/DocCore 却写空壳方法和成功测试：无法证明实际消费者契约。
