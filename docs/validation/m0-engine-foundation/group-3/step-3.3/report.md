# 任务 3.3 接口基础冻结与公开消费者追踪

日期：2026-10-31（UTC 02:26～02:32 初采，02:43 修正消费者扫描命令后重跑追踪证据）。状态：八接口契约已按已确认文档汇总冻结，实际公开消费者已完成人工追踪，文档回填完成；任务 3.3 可勾选。本记录只描述 M0 接口基础，不实现第 4～8 组功能，不声称 V-01～V-09 或 M0 出口通过。

## 1. 方法与范围

- 冻结来源限于已确认的架构/引擎规范与 change delta specs：`docs/v1-architecture.md` §5.1～5.4、§7.1，`docs/engine-design.md` §3、§4、§5.1～5.3、§7.1、§9.1～9.3，`openspec/changes/m0-engine-foundation/specs/engine/{source,buffer,markdown,edit,document-core}/spec.md`。本记录不新增产品行为，也不修改已确认的 AC/TA/ED/RS/UI/PL/TH 口径。
- 实际公开面来自各 crate `lib.rs` 导出和 `rg` 对 `crates/`、`app/` 的 `pub` 声明扫描；消费者追踪来自同一扫描、跨模块坐标运算扫描、workspace 测试发现和独立外部消费者夹具。
- 状态用语只有两种：**已落地**表示有本轮可定位的实现与测试证据；**仅契约未验证**表示只有冻结的输入输出/错误/相等条件，没有实现或没有对应证据。未实现接口不写成“通过”。

## 2. 实际公开面（2026-10-31）

| crate | 当前公开项 | 说明 |
|---|---|---|
| `aline-source` | `StorageSnapshot`、`StorageRange`、`BaselineId`、`DiskVersion`、`StorageBudget`、`StorageError` | 只读原 bytes 视图与基线/磁盘版本身份；不验证 UTF-8、不剥离 BOM、不转换行尾、不写盘。`decode`/`encode` 尚不存在 |
| `aline-buffer` | `TextSnapshot`、`SrcOffset`、`SrcRange`、`SrcSelection`、`Affinity`、`DocumentRevision`、`SnapshotId`、`CheckedBatch`、`PatchBudget`、`TextBudget`、`BufferError` | `TextSnapshot` 提供受检偏移/范围/选区、切片与物化；`CheckedBatch` 只构造/校验，不 apply、不产生逆补丁或历史 |
| `aline-markdown` | `Projection`、`Mapped<T>`、`MappingPrecision`、`MappingBudget`、`DrawnOffset`、`DrawnRange`、`Utf16Offset`、`Utf16Range`、`ProjectionId`、`MappingError` | 目前只有恒等投影；`source_at`/`drawn_at`/`source_from_utf16`/`utf16_from_source`/`aligned` 已实现并受检。无 Markdown 解析、块/Gap、保护或降级诊断 |
| `aline-edit` | 无公开项 | `lib.rs` 只有 crate 文档注释；不存在 `plan_change`/`validate_change` |
| `aline-app-core` | `CatalogError`、`Locale`、`Translator` | 零 GPUI 的双语 catalog；不属于八接口，但为 UI 错误/文案边界提供依赖 |
| `aline-plugin` | 无公开项 | `lib.rs` 只有 crate 文档注释 |
| `aline-ui` | `DocumentId`、`DraftGeneration`、`IdentityError`、`InputIdentity`、`WorkingVersion`、`application_name` | `InputIdentity` 只承载文档/工作版本/快照身份，不含草稿内容；DocCore、editor、persistence 尚不存在 |
| `app` | 仅二进制入口 | 调用 `aline_ui::application_name` 并返回 `ExitCode`，不建窗口、不启动平台服务 |

`public-surface.stdout.log` 记录本次扫描的完整命中；没有发现未从 `lib.rs` 导出的旁路公开模块，也没有 `type` 别名冒充跨模块坐标分型。

## 3. 八接口冻结

以下“输入→输出”“错误类别”“唯一写入口”“相等证据”是对已确认规范的汇总；没有实现的接口只冻结契约，不冻结具体 Rust 签名。

### 3.1 `decode`

- **输入→输出**：原文件 bytes + `StorageBudget` → 编辑源码 + 存储元数据（原 bytes 身份、头部 BOM、CRLF 对数、未配对 LF 数、行尾策略）。先检查预算，再验证 UTF-8；仅剥离头部 BOM，正文 U+FEFF 保留，原始 CR/LF 原样保留，孤立 CR 与文件末尾状态保留。
- **错误类别**：预算超限；UTF-16、非法或截断 UTF-8；编码/预算错误可区别。不得有损替换、写原文件或返回伪空文档；`%PDF`/`PK` 等签名前缀不因内容启发式被拒绝。
- **唯一写入口/副作用**：无。它是“磁盘 bytes → 编辑源码”的唯一转换入口，调用方不得自行去 BOM、规范化行尾或另建解码器。
- **相等证据来源**：原 bytes + 基线身份；未改内容经 `encode` 必须逐字节还原原 bytes。
- **M0 状态**：仅契约未验证（第 4 组）。当前 `StorageSnapshot` 只保存不透明 bytes 与 `BaselineId`/`DiskVersion`，不代表已完成解码。

### 3.2 `encode`

- **输入→输出**：冻结编辑源码（source 自有文本视图/元数据，不导入 buffer 类型）+ 存储基线 + `StorageBudget` + 绑定文本与基线的内容相等证据 → 完整待写 bytes 或错误。
- **错误类别**：预算超限；基线或相等证据不匹配（拒绝无来源的 `unchanged: bool`）；编码失败。I/O、取消与冲突归最小受检写原语，不混入纯编码。
- **唯一写入口/副作用**：`encode` 只产生 bytes，不写盘、不改 Buffer/History/基线。UI persistence 是唯一保存编排者；source 的最小受检替换原语是唯一磁盘写入口；只有成功写盘才可发布新基线，失败不得更新。
- **相等证据来源**：未改内容返回逐字节原 bytes；已改内容仅重放 BOM 与多数派行尾（CRLF 或 LF，平票 LF）。CRLF 对和未配对 LF 各算一个行尾单元，含合法编辑新形成的 CRLF；未配对 CR 保留；不增删末尾换行。再解码只允许 CRLF/LF 表示等价，该等价不得替代补丁、历史、工作版本及无变化的精确源码判定。
- **M0 状态**：仅契约未验证（第 4 组）。

### 3.3 `parse`

- **输入→输出**：不可变源码快照 + 解析配置代次 + 预算 → `Parsed`（输入身份、语义树、顶层块与 Gap、映射、保护区、诊断）或类型化降级/错误。`Parsed` 不持独立可变的 `text` 真源。
- **错误类别**：扫描/节点/改写预算超限；不支持或敌意输入；可靠区域原文或整体源码回退。不得 panic、吞尾、截断或发布不完整可编辑映射。
- **唯一写入口/副作用**：无。解析只从不可变源码派生；改写只作用于解析副本；不得从 AST 重建文档写回。
- **相等证据来源**：快照 id + 解析配置代次；复用须结构与对应源码片段相等；旧解析结果不能授权对新文本的编辑或映射。
- **M0 状态**：仅契约未验证（第 6 组）。当前只有恒等 `Projection`，不存在 Markdown 解析、块/Gap、保护或诊断。

### 3.4 映射（`source_at` / `drawn_at` / `aligned` 及 UTF-16 适配）

- **输入→输出**：同一投影的源码/绘制/UTF-16 坐标 + `Affinity`（选区对齐时为 `SrcSelection`）→ `Mapped<T>`（目标坐标、来源 `SrcRange`、`MappingPrecision`、方向偏好）或 `MappingError`。
- **错误类别**：`Source(BufferError)`、`OutOfBounds`、`NotCharBoundary`、`MiddleSurrogate`、`ProjectionMismatch`、`SourceOutsideProjection`、`ReversedRange`、`BudgetExceeded`、`LengthOverflow`、`InvalidMapping`、`Allocation`。非法边界不 clamp 成可执行删除，不 panic。
- **唯一写入口/副作用**：无。转换只在 markdown 映射叶模块内完成；UI/编辑调用方不得写 `at - range.start`、自建 UTF-16 表或按字符相等回搜来源。
- **相等证据来源**：`ProjectionId` + `SnapshotId`；原样段精确往返；`aligned` 幂等；非双射关系使用规范锚点与精确性，不要求 `source_at(drawn_at(x)) == x` 对所有源码字节成立。
- **M0 状态**：恒等路径已落地，3.2 有外部消费者与编译正反例；`Projection::identity` 不另设转换通道，`source_at`/`drawn_at`/`source_from_utf16`/`utf16_from_source`/`aligned` 对恒等投影执行同一受检逻辑，测试覆盖局部/全文/UTF-16 与错误分支。`Aligned`/`Hidden`/`Synthetic`、实体/HTML/合成换行和块局部映射仅契约未验证（第 6 组）。

### 3.5 `plan_change`

- **输入→输出**：当前源码快照 + 匹配版本的 `Parsed`/工作版本 + 源选区 + Command + 预算 → `Change`（工作版本、before 快照、受检补丁、目标源选区、分组原因、必要前置条件/预期源片段）或 `EditRefusal`。
- **错误类别**：领域拒绝（保护/HTML/命令不适用/版本或前提不符）；预算超限；映射失败。拒绝、失败与超限可区别；不得半提交或生成空历史。
- **唯一写入口/副作用**：无。纯规划不写 Buffer、不写盘、不封口草稿；只有文档所有者可提交。
- **相等证据来源**：`WorkingVersion` + `SnapshotId` + 预期源片段/语义前置条件；真实写入前完成预演与语义重读；旧计划不得用于新文本。
- **M0 状态**：仅契约未验证（第 7 组）。`aline-edit` 当前无公开接口。

### 3.6 `validate`

- **输入→输出**：候选批次/计划/身份信封 + 当前快照/版本/投影/文档 → `Ok(())` 或类型化错误。
- **分层与错误类别**：buffer `CheckedBatch::validate` 先拒绝 `RevisionMismatch` 再拒绝 `SnapshotMismatch`；markdown 校验投影身份、快照与边界（`ProjectionMismatch` 等）；UI `InputIdentity::validate` 依次校验文档、工作版本、快照（`WrongDocument`/`StaleWorkingVersion`/`StaleSnapshot`）；未来 edit 层校验保护、HTML 与预期内容/语义前置条件。
- **唯一写入口/副作用**：无。校验失败零修改，不改变文本、草稿、选区、组合、undo/redo 或版本。
- **相等证据来源**：身份/版本相等，不是文本相等；同文本的新 `SnapshotId` 仍必须拒绝。
- **M0 状态**：buffer、markdown、UI 三层已落地并有 3.1/3.2 外部测试；edit 层 `validate_change` 仅契约未验证（第 7 组）。

### 3.7 `apply_change`

- **输入→输出**：已校验 `CheckedBatch`（同一 before 快照/revision）+ 按 after 文本校验的目标源选区 + 历史分组原因 + 预算 → 新文本快照/revision、新选区、逆补丁、历史步/事件；或整批原子拒绝。
- **错误类别**：`BufferError` 中与 apply 相关的变体（越界、反向、半 UTF-8、快照/版本不符、长度溢出、预算、逆序/重叠/同点歧义、补丁数量、revision 耗尽、分配失败）；`NestedOverlay`/`InvalidBacking` 属于快照/覆盖层构造边界，不是 apply 结果；整批拒绝零修改。历史预算超限时类型化失败或执行已声明的安全裁剪，不得半应用或丢弃当前输入。
- **唯一写入口/副作用**：唯一文本/选区/revision 写入口是 DocCore 的单一提交路径及其 buffer 原子应用；其他模块不得直接写 Buffer 或 History。M0 的 `CheckedBatch` 只校验，不 apply。
- **相等证据来源**：before `SnapshotId` + `DocumentRevision`；after revision 受检前进；逆补丁在冻结的 after 源码上精确恢复；历史/保存点使用内容等价证据，不用栈深或未验证指纹。
- **M0 状态**：仅契约未验证（第 5、8 组）。

### 3.8 `snapshot_current`

- **输入→输出**：当前 Buffer + 唯一 `OpenBlock` 草稿 + `WorkingVersion` + 所需资源引用 → 含最新草稿的不可变当前文本快照、版本、选区与资源引用；不提交、不封口输入组、不生成历史步。组合期显式保存/另存为/导出必须先可取消地等待平台自然结束再取快照（ED-01，编排归第 8 组）。
- **错误类别**：仅草稿基线与当前源版本不符（保留草稿与选区返回冲突）。已确认规范未定义 snapshot 层“旧工作版本”或预算错误；工作版本身份校验属于 UI `InputIdentity` 层，不在此重复，也不新增未确认错误类别。冲突时不得 `drop` 草稿或套用旧区间。
- **唯一写入口/副作用**：无。只读冻结；DocCore 是唯一编排者；调用方不得分别读取 Buffer 和草稿后自行拼接当前文本。
- **相等证据来源**：`WorkingVersion(DocumentRevision, DraftGeneration)` + `SnapshotId` + 草稿内容；内容比较受版本保护，不用栈深或未验证哈希。
- **M0 状态**：仅 `InputIdentity` 身份信封已落地（3.2）；`snapshot_current` 的内容快照仅契约未验证（第 8 组）。

## 4. 共同边界

- 五引擎（source、buffer、markdown、edit、app-core）无 GPUI、无线程、无运行期可变全局状态，生产代码禁止 `unwrap`/`expect`/`panic!`/`unreachable!` 及 release `assert!`。
- 源码、绘制、UTF-16、存储坐标只经受检 newtype 转换；裸整数和 `Vec<(Range<usize>, String)>` 只在统一受检边界出现，不成为跨模块坐标通道。
- 错误以类型化枚举返回，UI 用 `t("key")` 翻译；不新增第二解析器、未准入依赖或隐藏的规范化/整理通道。
- 本记录只冻结 M0 所需的最小协议；具体签名、底层宽度和内部表示仍可由实现按既定分层调整，但不得削弱上述输入、错误、写入口与相等证据。

## 5. 公开消费者追踪

追踪脚本：`trace/run-trace.sh`（只读；设置隔离 `CARGO_HOME`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`）。命令、UTC 起止与退出码见 `trace/commands.log`，原始输出与过滤结果见同目录 `*.stdout.log`/`*.log`。

| 检查 | 实际结果 | 结论 |
|---|---|---|
| 公开声明扫描 | exit 0 | 公开面与第 2 节一致；edit/plugin 无公开项 |
| 映射 API 全仓扫描 | exit 0；命中仅在 `aline-markdown/src/mapping/` 和测试中；过滤 mapping 与 tests 后 0 行 | 正式生产代码没有 `source_at`/`drawn_at`/`aligned`/UTF-16 适配调用 |
| UTF-16 扫描 | exit 0；过滤 mapping 与 tests 后仅 `crates/aline-markdown/src/lib.rs:7` 的重导出 1 行 | 无映射外自建 UTF-16 表或转换 |
| 坐标字段减法扫描 | exit 1（无匹配，预期为空）；过滤 buffer/mapping 后 0 行 | 无 `at - range.start` 一类调用点手算 |
| 减运算 API 扫描 | exit 0；过滤 buffer/mapping 后 0 行 | 跨模块减法只出现在 mapping/buffer 内部受检运算 |
| ui/edit/plugin/app-core/app 的 `.start`/`.end` 使用 | exit 1（无匹配，预期为空） | 上层没有直接拆 range 字段 |
| 关键类型消费者扫描 | exit 0；`TextSnapshot` 的跨 crate 生产消费者只有 `aline-markdown/src/mapping/projection.rs` 与 `aline-ui/src/document/identity.rs`（同 crate 的 `aline-buffer/src/batch.rs` 在 `CheckedBatch` 校验中消费它）；`StorageSnapshot`/`CheckedBatch`/`InputIdentity` 只有自身 crate 与测试 | 与“无额外跨 crate 生产消费者”的结论一致；见 `consumer-types.stdout.log` |
| workspace 测试发现 | exit 0，`--list` 实际列出 20 个测试 | 与当前 workspace 测试目标一致；本记录不把 `--list` 当执行 |
| 独立外部消费者 | exit 0，5 passed、0 failed | 仅使用公开 API 编译并运行；覆盖 overlay Unicode、局部/全文/UTF-16 映射、预算、存储/文档身份、`CheckedBatch`、恒等映射 |

解释边界：

- 当前生产消费者只有 `aline-markdown` 对 `aline-buffer` 的恒等投影，以及 `aline-ui::InputIdentity` 对 `aline-buffer` 快照身份的校验。没有 UI editor、命中测试、IME 或 DocCore，因此“映射 API 无生产调用点”只能证明当前仓库扫描未发现映射外调用或调用方手算，不能证明公开 API 无法被绕过：`SrcOffset::byte_offset`、`SrcRange::byte_range`、`StorageRange::byte_range`、`CheckedBatch::patches` 等原始访问器仍然公开；这也不证明真实点击、输入法或平台坐标已验证（CODE_STANDARDS §6.2）。
- `CheckedBatch` 的消费者只有测试与外部夹具，没有生产 apply 路径；`StorageSnapshot` 只有 source 自身测试与外部夹具，没有 decode/encode/写入路径；`aline-edit`/`aline-plugin` 没有公开 API。
- 外部夹具是独立 Cargo workspace，不依赖未公开模块；它的 5 项通过只覆盖已落地类型，不覆盖第 4～8 组契约。

## 6. 文档回填

以下只增加“2026-10-31 任务 3.3 接口基础记录”及状态说明，不改动原契约文字，也不把未实现接口写成完成：

| 文档 | 回填内容 |
|---|---|
| `docs/rust-style-guide.md` §5 | 已落地受检类型、仅契约接口、消费者追踪结论；不改变 S-5-01～06 |
| `docs/engine-design.md` §3 | source decode/encode 未实现、buffer 不 apply、markdown 仅恒等投影、edit/plugin 无公开接口、DocCore 未实现 |
| `docs/engine-design.md` §4 | `SourceBaseline`/`Parsed`/`OpenBlock`/`Change`/`HistoryStep` 仅契约；已实现类型与仅契约模型的状态说明（非穷举，完整公开面见第 2 节）；块/Gap/保护留第 6 组 |
| `docs/engine-design.md` §7.1 | 恒等映射已实现并测试；复杂来源关系与块局部映射仅契约；无映射外调用点，不证明真实命中/IME |
| `docs/v1-architecture.md` §4.1 | 将“source/buffer/markdown/edit 仍为骨架”更新为 3.1～3.3 实际状态，并链接本记录 |
| `docs/v1-architecture.md` §5 | `SourceBaseline`/`OpenBlock`/DocCore/`Parsed`/History 仅契约；已实现身份信封与只读文本；`InputIdentity` 不带草稿内容 |
| `docs/v1-architecture.md` §7.1 | 公共契约表新增 M0 状态列；未实现接口明确标注仅契约 |
| `docs/v1-validation-plan.md` V-07 | 接口基础记录：八接口已冻结、仅身份/恒等投影实际落地、其余契约未验证；不证明任何 V-07 用例通过 |

`docs/README.md` 第 3/25 行仍写“当前无 Cargo workspace”“仓库尚无 Cargo workspace 或 CI”，`AGENTS.md` 第 9 行仍写“尚无应用实现”；这些状态行属于后续文档收口（9.6），本任务不越权改动，已在此记录已知过时项。

## 7. 仅契约未验证清单

- `decode`、`encode`、source 最小受检写盘原语与 BOM/行尾往返（第 4 组）。
- 完整 Markdown `parse`、块/Gap 覆盖、保护范围、HTML/实体/合成换行、`Aligned`/`Hidden`/`Synthetic` 映射（第 6 组）。
- `plan_change`、`validate_change`、edit 层拒绝/预演（第 7 组）。
- `apply_change`、逆补丁、历史、锚点变换、revision 推进与历史预算（第 5、8 组）。
- `snapshot_current` 的内容快照与草稿冲突（第 8 组）。
- `save`、`recover`、`render_asset`/`export`，以及 DocCore、900ms 输入组、IME、发布等待、真实 UI/平台与 V-01～V-09（后续任务）。

这些条目只有冻结的契约，没有实现或运行证据；不得引用本记录作为其完成、M0 出口或 M1 交接已就绪的证明。

## 8. 证据与环境

- 追踪脚本与输出：`trace/run-trace.sh`、`trace/commands.log`、`trace/*.stdout.log`（含 `trace/consumer-types.stdout.log`）、`trace/*.stderr.log`、`trace/mapping-consumers-outside-mapping.log`、`trace/utf16-outside-mapping.log`、`trace/coordinate-arithmetic-outside-buffer-mapping.log`、`trace/subtraction-calls-outside-buffer-mapping.log`。
- 环境：macOS Darwin 27.0.0 arm64；Rust/Cargo 1.98.1；隔离 `CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`；离线 Cargo。见 `trace/environment.stdout.log`。
- 基线：重跑时 HEAD 仍为 `c925166c57a4c1e6c42b362a8c008d7b0a7e07ae`；工作区包含四份回填文档修改与未跟踪的 `docs/validation/m0-engine-foundation/group-3/step-3.3/`（首次追踪时仅后者）。`tasks.md` 在本记录与复核处理完成后才勾选。见 `trace/git-state.stdout.log`。
- 本任务没有 Rust 代码改动、没有新增依赖、没有开窗口。追踪阶段没有重跑 workspace 门禁；外部消费者夹具的 5 passed 只用于公开面追踪，不等同于 workspace 门禁结论。提交前另行实际运行的三条门禁与统一入口结果如下，原始命令/UTC/退出码在 `gates/` 与 `workspace-gates/`。

| 门禁 | 实际结果 | 证据 |
|---|---|---|
| `cargo fmt --all --check` | exit 0 | `gates/commands.log`、`gates/fmt.*` |
| `cargo clippy --workspace --all-targets -- -D warnings` | exit 0 | `gates/commands.log`、`gates/clippy.*` |
| `cargo test --workspace --locked --offline` | exit 0，20 passed、0 failed | `gates/commands.log`、`gates/test.*` |
| `python3 scripts/check_workspace.py --cargo-home /Users/xian/Library/Caches/aline-m0/cargo-home --output-dir /Users/xian/Project/editor/Aline/docs/validation/m0-engine-foundation/group-3/step-3.3/workspace-gates` | exit 0，19 项全部 0，`summary.json.passed=true`；`public-api` self-test 8/8（夹具自测，不是对仓库实际公开面的检查，后者见 `trace/public-surface.stdout.log`） | `workspace-gates/`，实际 argv 见各 `*.command.json` |

## 9. 下一步

按任务组顺序进入第 4 组：先写 source `decode`/`encode` 的独立金样与最小反例，再按 2026-10-31 已批准的“仅剥离头部 BOM、保留原始 CR/LF、存储多数派行尾、未配对 CR 保留、存储等价不替代精确源码身份”实现，最后补最小受检写盘原语与失败保全证据。
