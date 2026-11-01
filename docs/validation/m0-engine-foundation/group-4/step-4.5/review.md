# 4.5 独立复核

复核者：独立复核者（非 4.5 实现者；只读复核）。以下结论只来自本次独立执行的命令、源码阅读和文档核对；实现者报告仅作为被检查对象，不作为证据。

复核日期（UTC）：2026-10-31。

工作树状态（复核开始时）：

- `git rev-parse --short HEAD` → `fd26bd8`，exit 0。
- `git status --porcelain` → exit 0，内容为：
  - `M docs/engine-design.md`
  - `M docs/v1-architecture.md`
  - `M docs/v1-validation-plan.md`
  - `?? docs/validation/m0-engine-foundation/group-4/step-4.5/`
- `git status --porcelain -- crates/` → 空，exit 0。
- `openspec/changes/m0-engine-foundation/tasks.md:43` 仍为 `- [ ] 4.5 ...`；本次复核未勾选，也未修改该文件。

结论：**有条件接受，需修复 F1、F2、F3**。F4～F6 为轻微，建议同批修正，不单独阻断。阻断 3 项，重要非阻断 0 项，轻微 3 项。

## 1. 实际命令与结果

### 1.1 回填性质与状态

- `git diff --numstat -- docs/engine-design.md docs/v1-architecture.md docs/v1-validation-plan.md` → exit 0：`11/0`、`2/0`、`4/0`，合计 17 行新增、0 行删除。
- `git diff --unified=0 ... | grep -E '^[-+][^-+]' | grep -c '^-'` → `0`；没有删除的正文行，回填为纯追加。
- `sed -n '136,145p' docs/engine-design.md` → §5.4 标题在第 136 行，新增边界段在第 140～143 行，证据段在第 145 行。
- `sed -n '455p' docs/v1-architecture.md` → 第 455 行为新增段。
- `sed -n '83p;91p;152p' docs/v1-validation-plan.md` → V-06 在第 83 行、V-07 在第 91 行，两段末尾分别明确写“V-06 仍未通过”“V-07 仍未通过”；状态表第 152 行仍为“V-01～V-09 | 未执行”。
- `git diff --unified=1 -- docs/v1-validation-plan.md` → V-08 段未触及；V-06/V-07 只新增日期段落和空行。

### 1.2 哈希

`shasum -a 256` 计算工作树文件；回填前哈希用 `git show HEAD:<file> | shasum -a 256` 计算（HEAD 即回填前基线，因为差异全部在未暂存工作树中）。

| 文档 | report §1 声称的回填前 | 独立计算 HEAD | report §1 声称的回填后 | 独立计算工作树 | 结果 |
|---|---|---|---|---|---|
| `docs/engine-design.md` | `a4918c77fa54dc5d9fb739cadcae1537faf6e6ad9528bcd0ca23abac2bb92667` | 同左 | `840fac98aa82e263c0f70899692cc3567c79b8f05224d2717eb5011d20485f8e` | 同左 | 匹配 |
| `docs/v1-architecture.md` | `851debcba8fa9c928d449e64dd3c60d7f62ffd507089ac4e3e209c3a6d8c65fb` | 同左 | `09d260a9a47311cafdb4321e3cbaf2ca9e16fc462010703a21f8593fda74145d` | 同左 | 匹配 |
| `docs/v1-validation-plan.md` | `a0d21cfbc4ee04c26df8f264f020a221039b3efea15b4af2ba262917d698210d` | 同左 | `e60a36a7b9fa130fd6e5664d69a3b4d8d33a3adc68449f2a0ef235049d64f0a5` | 同左 | 匹配 |
| `docs/architecture-guide.md` | `f6f54d17d4f790e41cd7417e2ab34fde3e89e74d94f26b5558887668ec9465eb` | 同左 | 未修改 | 同左 | 匹配 |

报告没有引用自身或 `run-k08-audit.sh` 的哈希，因此没有对应的报告/脚本哈希可核；审计日志通过重跑核对（见 1.3）。

### 1.3 审计脚本只读性与可复现性

- 阅读 `run-k08-audit.sh`：只包含 `git rev-parse`、`cd`、`date`、`rustc --version`、`rg`、`sed`、`python3 -I`、`git diff`、`git status`、`printf`；没有写仓库文件的命令，输出由调用方重定向。
- 独立重跑（只写 `/tmp`）：
  `sh docs/validation/m0-engine-foundation/group-4/step-4.5/run-k08-audit.sh > <tmp>/k08-review-14141.stdout.log 2> <tmp>/k08-review-14141.stderr.log`
  → exit 0；`utc_start=2026-10-31T05:51:02Z`，`utc_end=2026-10-31T05:51:02Z`；stdout 410 行；stderr 0 字节；`head=fd26bd8`；`rustc 1.98.1 (48a229cea 2026-09-01)`。
- `diff -u <(sed -E 's/^(utc_start|utc_end)=.*/\1=<normalized>/' <已提交日志>) <(同样规范化 /tmp 日志)` → exit 0，0 行差异；即除 UTC 行外，重跑输出与已提交 `k08-offset-audit.stdout.log` 完全一致。
- 已提交日志：`utc_start/utc_end=2026-10-31T05:46:35Z`，410 行，stderr 0 字节，HEAD 与工具链同上。
- 关键检查一致：`rg -n "src_off|source_off|disk_off|storage_off" crates/aline-source/src` → exit 1（无命中）；`rg -n "aline_source|SourceBaseline|StorageSnapshot" crates --glob '!crates/aline-source/**'` → exit 1（无代码级消费者）；手工偏移算例输出 `source_len 8`、`source_offset_of_乙 5`、`disk_len 11`、`disk_offset_of_乙 8`、`difference 3`；`git diff --stat` 为 17 行新增；`git status --porcelain -- crates/` 为空。

结论：审计脚本只读、重跑可复现，关键结论一致。摘要中的两处精度问题见 F4/F5。

### 1.4 K-08 记录完整性

`docs/architecture-guide.md:345` 要求逐项记录：变更规则、所有者、写路径、输入键、失败出口、例外与证据；无自动化覆盖须写具体人工步骤，不填“保持合理”。

- report §4.1 七行齐全：变更规则、所有者、写路径、输入键、失败出口、例外、证据。
- report §4.2 给出了具体命令、时间、脚本、预期/实际退出码、日志行数和 diff 结果；可重复执行。
- report §4.3 明确只声称“当前 crate 外无消费者，故只能证明现在未混用”，不声称未来消费方不会混用，也不声称 RS-02 已通过；§6 另有证据边界。
- report §4.1 的失败出口阶段枚举与代码不符（F1）；§4.3 的 `src_off` 不存在声明与仓库现状不符（F3）；回填文档对已存在目标的错误分类过度概括（F2）。
- 结构、范围和“不越界”部分合格；上述三项是内容准确性问题，不是结构缺失。report 未声称完整 PRD/AC/V/TA/ED/PA 发布审计，§4.1 已限定为 4.1～4.4 的偏移/写路径边界，符合 4.5 任务范围。

### 1.5 未测边界与能力区分

抽查 report §2、§3、§5、§6 和回填段：

- create-only：只使用 `create_new` staging、完整写入、`sync_all`、`hard_link` 到不存在目标；不覆盖、不截断、不 rename 覆盖、不删除、不回退原地写。正确。
- `MetadataPreservationUnverified`：report §2/§3 明确限定为“已存在普通文件”，并写明它是拒绝、不是能力。正确；回填文档缺少“普通文件”限定（F2）。
- `hard_link`：只用于发布到尚不存在的目标，没有写成现有文件覆盖保存。正确。
- ACL/xattr/权限复制：report §3/§5 明确未实现、未测，且写明不能由 mode/`Permissions::readonly` 推断 ACL 保留。正确。
- 目录 fsync/断电：明确未测，`Written::directory_durability_unverified()` 为 true；没有断电持久性声明。正确。
- 跨进程 CAS：明确未实现；没有把一次 `hard_link` 称为 CAS。正确。
- 真实 ENOSPC：明确未测；只做受控写入/sync 失败注入。正确。
- Windows/Linux：明确未验证；非 Unix 身份比较恒拒绝。正确。
- staging 清理：未实现自动清理，无 Drop 静默删除；路径保留的范围描述不准确（F6）。
- 真实应用保存闭环、V-06/V-07/V-08、M0 出口：均明确未通过/未完成。正确。

未发现把未测能力写成已验证能力。F2 是错误分类的过度概括，不是把未测能力写成通过。

### 1.6 门禁、链接与证据边界

- `workspace-gates-execution-1/summary.json`：`passed: true`，19 条结果，全部 `exit_code: 0`；首条 `started_utc=2026-10-31T05:48:13.296145+00:00`，末条 `finished_utc=2026-10-31T05:48:30.215264+00:00`，与 report §8 一致。
- `git status --porcelain -- crates/` → 空，exit 0。
- 链接检查（`python3 -I` 提取并解析相对链接）：`docs/v1-architecture.md:455` 的 `./validation/m0-engine-foundation/group-4/step-4.5/report.md` 可解析；`report.md:49` 的 `../../../../architecture-guide.md` 可解析；回填段其余路径均为反引号代码路径，不是 Markdown 链接。
- report §6 与回填段明确不证明现有文件保存、权限/ACL 保留、目录持久性、断电安全、跨进程 CAS、Windows/Linux 语义、真实保存闭环、V-06/V-07/V-08 通过或 M0 出口通过。

## 2. 发现

### F1 [阻断] K-08 失败出口的阶段枚举与代码不符

位置：`docs/validation/m0-engine-foundation/group-4/step-4.5/report.md:59`。

报告写的是：`Cancelled`（Prepare/Publish）、`Io{stage}`（Prepare/Create/Write/Sync/Publish）。

代码实际：

- `Cancelled`：Prepare（`crates/aline-source/src/atomic.rs:482`）、Write（`:597`）、Sync（`:637`）、Publish（`:275`）。
- `Io{stage}`：Create（`:492`）、Write（`:601`、`:607`、`:614`、`:621`、`:628`）、Sync（`:641`）、Publish（`:285`、`:297`、`:330`、`:354`）；没有 `Io { stage: Prepare }`。
- `write_staging_owned` 把 `StagingFailure::{Budget,Cancelled,Io}` 映射为 `AtomicFailure`，所以 Write/Sync 取消确实会从公开 `prepare_atomic` 返回（`atomic.rs:503-524`）。

失败场景：K-08 记录被用于发布失败分类、错误处理或运维排查时，Write/Sync 阶段取消会被漏掉，`Io{Prepare}` 会被误认为存在；调用方据此只处理 Prepare/Publish 取消，会漏掉实际可发生的取消出口。

修复建议：改为精确映射，例如 `Cancelled`（Prepare/Write/Sync/Publish）、`Io{stage}`（Create/Write/Sync/Publish）。如果该行意在覆盖整个写路径，还应说明 `Written::into_baseline` 的 `BaselineError::{Storage,Decode}`；至少阶段列表必须修正。

### F2 [阻断] 回填文档把已存在目标的错误分类过度概括为 `MetadataPreservationUnverified`

位置：`docs/engine-design.md:143`、`docs/v1-architecture.md:455`。

回填文本写的是：目标已存在返回 `MetadataPreservationUnverified` / 目标已存在时原语拒绝并返回 `MetadataPreservationUnverified`。

代码实际（`crates/aline-source/src/atomic.rs:307-323`）：

- 已存在且 `metadata.is_file()` → `MetadataPreservationUnverified`。
- 已存在符号链接 → `ExistingTarget { kind: Symlink }`。
- 已存在目录 → `ExistingTarget { kind: Directory }`。
- 其他非普通文件（设备、FIFO、套接字等）→ `ExistingTarget { kind: Other }`。

report §2（第 29 行）和 §3（第 35 行）本身已正确限定为“已存在普通文件”；两份规范文档漏掉了这个限定，和代码及 report 自相矛盾。

失败场景：目标路径是符号链接、目录或 FIFO 时，调用方按文档只匹配 `MetadataPreservationUnverified`，会漏掉 `ExistingTarget{kind}`，无法给出正确的拒绝原因，或误以为所有已存在目标都走 metadata 拒绝路径。

修复建议：改为“目标已存在普通文件时返回 `MetadataPreservationUnverified`；符号链接/目录/其他非普通文件返回 `ExistingTarget{kind}`”。保留“拒绝、不覆盖、不构成现有文件保存能力”的结论。

### F3 [阻断] 报告声称代码没有公开 `src_off` 类型，与仓库现状不符

位置：`docs/validation/m0-engine-foundation/group-4/step-4.5/report.md:87`、`:90`。

报告写的是：`当前代码没有公开 src_off 类型或换算函数`；`src_off 作为独立坐标类型尚未在代码中引入（下游 crate 未实现）`。

实际证据：

- `docs/engine-design.md:63`（4.5 差异之外的既有记录）已列出 buffer 的 `TextSnapshot`/`SrcOffset`/`SrcRange`/`SrcSelection` 为已落地公开面；`:97` 再次列出这些类型。
- `crates/aline-buffer/src/coordinates.rs:73` 为 `pub struct SrcOffset`，`:92` 为 `pub struct SrcRange`；`crates/aline-buffer/src/text.rs:253` 为 `pub fn src_offset(&self, src_off: usize) -> Result<SrcOffset, BufferError>`。
- `crates/aline-markdown/src/mapping/projection.rs:260` 等已使用 `SrcOffset`/`SrcRange`；`docs/rust-style-guide.md:136` 也记录了这些受检 newtype 已落地。
- 另一方面，`rg -n "aline_source|SourceBaseline|StorageSnapshot|StorageRange" crates app --glob '!crates/aline-source/**' --glob '!Cargo.toml'` → exit 1，说明当前没有 aline-source 存储类型的代码级消费者；因此“当前 aline-source 未混用”的结论仍成立。

失败场景：复核者或后续实现者按报告认为仓库没有 `src_off` 类型，会跳过 buffer/markdown 已有的 `SrcOffset`/`SrcRange` 使用，误判 RS-02 相关实现状态；报告的范围声明也不足以支撑“未来消费方”风险分析。这里没有发现实际混用，但报告的事实陈述错误，K-08 结论必须重新限定范围。

修复建议：改为“aline-source 未公开 `src_off` 类型或换算函数；buffer 已有 `SrcOffset`/`SrcRange`/`TextSnapshot::src_offset`，markdown 已使用它们，但这些 crate 不依赖 aline-source 的 `StorageRange`，故当前未混用”。如果要表达“没有 aline-source 消费者”，应明确写“没有 aline-source 存储类型的消费者”，不要写“没有 src_off 类型”。

### F4 [轻微] 审计摘要漏掉 `lib.rs` 的 `StorageRange` 公开 re-export

位置：`docs/validation/m0-engine-foundation/group-4/step-4.5/report.md:78`。

报告写的是：`仅 storage.rs 的 StorageRange 与错误偏移字段、atomic.rs 写循环的局部 offset`。

实际审计日志第 2 节还包含 `crates/aline-source/src/lib.rs:17` 的 `StorageRange` re-export。该类型定义在 `storage.rs`，在 `lib.rs` 公开导出；没有产生第二种文档坐标，也没有改变“无第二种坐标”的结论，但摘要与原始日志不完全一致。

失败场景：复核者对照摘要和日志时会遇到未解释的 `lib.rs:17` 行，降低审计可追溯性；不影响功能结论。

修复建议：在摘要中补“以及 `lib.rs:17` 的公开 re-export”。

### F5 [轻微] “crate 外无消费者”是代码级结论，且 `codegraph_explore` 调用者声明未留可重放证据

位置：`docs/validation/m0-engine-foundation/group-4/step-4.5/report.md:79`、`:89`；`run-k08-audit.sh:36-38`；`crates/aline-ui/Cargo.toml:15`。

实际：

- `rg -n "aline_source" crates app --glob '!crates/aline-source/**' --glob '!Cargo.toml'` → exit 1，确认没有代码级消费者。
- 但 `rg -n "aline-source" crates app --glob 'Cargo.toml'` → exit 0，命中 `crates/aline-ui/Cargo.toml:15: aline-source.workspace = true`。该依赖当前未被 Rust 代码使用，但“crate 外无消费者”应限定为“无代码级消费者”，并记录这个未使用的 manifest 依赖。
- report 第 79 行还声称 `codegraph_explore` 的调用者只有 aline-source 内部模块与测试；审计脚本没有运行 `codegraph_explore`，已提交日志也没有该查询/输出，因此这条声明不能从证据目录重放。

失败场景：未来 aline-ui 代码一旦开始使用 aline-source，现有“无消费者”结论会失效；manifest 依赖已经是潜在入口。`codegraph_explore` 声明无法独立复核，属于证据可追溯性问题。当前无代码消费者和未混用的结论仍然成立。

修复建议：写明“无代码级消费者；`aline-ui` 的 Cargo.toml 已声明未使用的 `aline-source` 依赖”；删除 `codegraph_explore` 的未留证声明，或把查询/输出加入审计日志。

### F6 [轻微] “成功/失败均保留 staging 路径”与 API 契约不符

位置：`docs/validation/m0-engine-foundation/group-4/step-4.5/report.md:44`。

报告写的是：`成功/失败均保留调用方提供的 staging 路径；无 Drop 静默删除`。

代码实际：

- `AtomicFailure::staging_path()` 文档写明“仅在 staging 已创建且未观察到替换时存在”（`crates/aline-source/src/atomic.rs:173-177`）。
- `Budget`（`:464-470`）、`InvalidInput`（`:473-477`）、Prepare 取消（`:479-485`）、`Io{Create}`（`:487-497`）在 staging 创建前失败，携带 `None`。
- `StagingReplaced` 明确携带 `None`（`:305`）。
- 只有 staging 已创建后的 Write/Sync/Publish 失败，以及成功返回的 `Written`，才携带 `Some(staging)`。

失败场景：调用方按“成功/失败均保留”在早期失败后对 `staging_path()` 直接 `unwrap()` 或按路径清理，会 panic 或误删不存在的路径；该描述把“保留调用方持有的路径”与“失败结果携带路径”混为一谈。

修复建议：改为“staging 已创建且未观察到替换时，成功/失败结果保留路径；早期失败与 `StagingReplaced` 返回 `None`”。

## 3. 结论

**有条件接受，需修复 F1、F2、F3。** F4～F6 为轻微，建议同批修正，不单独阻断。

- F1 和 F3 是 `report.md` 中的事实错误；F2 是 4.5 回填规范文档中的事实错误。按本次复核要求，报告存在虚假陈述必须阻断；F2 虽在文档而非 `report.md`，但它是本任务的主要交付物，且与代码及 report 自身矛盾，同样应在勾选前修正。
- 其余硬性检查通过：回填为纯追加（17 行新增、0 行删除）；哈希全部匹配；审计脚本只读且重跑输出与已提交日志一致；V-06/V-07 仍明确未通过，V-08 未触及；未测权限/替换竞争未被写成已验证；门禁 19/19 exit 0，UTC 与 report §8 一致；`crates/` 干净；相对链接可解析。
- 修复 F1～F3 后，任务所有者可按流程决定是否勾选 `tasks.md` 4.5；本次复核不代替修复后的复审。

本次复核未修改代码、文档、报告、脚本、日志或 `tasks.md`；唯一写入是新建本文件。审计重跑临时输出在 `<tmp>/k08-review-14141.stdout.log` 和 `<tmp>/k08-review-14141.stderr.log`。

## 4. 增量复审（2026-10-31）

复审范围：F1～F6 是否闭合、修订是否引入新的事实错误，以及哈希/diff、两份文档的目标分类、`src_off`/消费者表述、`workspace-gates-execution-2` 与 `crates/` 状态。只读复核；未修改 report/docs/code，未提交。

### 4.1 实际命令与结果

UTC：本轮命令执行于 `2026-10-31T06:00:01Z`–`2026-10-31T06:02:53Z`；审计重跑 `utc_start`/`utc_end` 均为 `2026-10-31T06:01:25Z`。`HEAD=fd26bd8`。`git status --porcelain` 仍为 `M docs/engine-design.md`、`M docs/v1-architecture.md`、`M docs/v1-validation-plan.md`、`?? docs/validation/m0-engine-foundation/group-4/step-4.5/`；`git status --porcelain -- crates/` 为空（exit 0）。

| 检查 | 实际命令（仓库根） | 结果 |
|---|---|---|
| 当前哈希 | `shasum -a 256 docs/engine-design.md docs/v1-architecture.md docs/v1-validation-plan.md docs/architecture-guide.md` | engine `675670da265f3bf5adc7d897b875fca665a1a97ee69790ede54ae0b990f37349`；arch `9bb8a98e44828e80929a7e7ea59386fa795868a1e28a7fb76b59c358f5c49ef9`；validation `e60a36a7b9fa130fd6e5664d69a3b4d8d33a3adc68449f2a0ef235049d64f0a5`；guide `f6f54d17d4f790e41cd7417e2ab34fde3e89e74d94f26b5558887668ec9465eb`；exit 0 |
| 回填前哈希 | `git show HEAD:docs/<file> \| shasum -a 256` | engine `a4918c77fa54dc5d9fb739cadcae1537faf6e6ad9528bcd0ca23abac2bb92667`；arch `851debcba8fa9c928d449e64dd3c60d7f62ffd507089ac4e3e209c3a6d8c65fb`；validation `a0d21cfbc4ee04c26df8f264f020a221039b3efea15b4af2ba262917d698210d`；guide 与当前相同；exit 0 |
| diff | `git diff --numstat -- docs/engine-design.md docs/v1-architecture.md docs/v1-validation-plan.md`；`git diff --check -- ...` | engine 11/0、arch 2/0、validation 4/0；合计 17 新增、0 删除；`diff --check` exit 0 |
| 审计重跑 | `sh docs/validation/m0-engine-foundation/group-4/step-4.5/run-k08-audit.sh > <tmp>/k08-review-incr.stdout.log 2> <tmp>/k08-review-incr.stderr.log` | exit 0；stdout 410 行，stderr 0 字节；`utc_start`/`utc_end` 均为 `2026-10-31T06:01:25Z`；`HEAD=fd26bd8`；归一化 UTC 后与已提交 `k08-offset-audit.stdout.log` 的 diff 为 0 行（diff exit 0） |
| 源码偏移符号 | `rg -n 'src_off\|source_off\|disk_off\|storage_off' crates/aline-source/src` | exit 1（无命中） |
| crate 外消费者 | `rg -n 'aline_source\|SourceBaseline\|StorageSnapshot' crates app --glob '!crates/aline-source/**' --glob '!Cargo.toml'`；`rg -n 'aline-source' crates app --glob 'Cargo.toml'`；`rg -n 'StorageRange\|SourceBaseline\|StorageSnapshot' crates --glob '!crates/aline-source/**'` | 代码级消费者 exit 1；manifest 查询 exit 0，仅 `crates/aline-source/Cargo.toml:2` 自身包名与 `crates/aline-ui/Cargo.toml:15` 未使用依赖；crate 外 `StorageRange` 等 exit 1 |
| 旧虚假表述 | `rg -n '没有公开\|尚未在代码中引入\|尚未实现.*src_off\|无消费者' docs/validation/m0-engine-foundation/group-4/step-4.5/report.md`；`rg -n '尚未追加' .../report.md` | 前者 exit 1（无命中）；后者 exit 0，仅命中第 3 行陈旧状态句（见 N1） |
| 目标分类与失败阶段 | `sed -n '307,323p' crates/aline-source/src/atomic.rs`；`rg -n 'AtomicFailureKind::(Cancelled\|Io\|MetadataPreservationUnverified\|ExistingTarget\|StagingReplaced\|Conflict)' crates/aline-source/src/atomic.rs` | 普通文件 `MetadataPreservationUnverified`；Symlink/Directory/Other `ExistingTarget{kind}`；检查后竞争 `Conflict`。`Cancelled` 出现在 Prepare/Write/Sync/Publish；`Io` 出现在 Create/Write/Sync/Publish；无 `Io{Prepare}`。exit 0 |
| 门禁 2 | `python3 -I -` 读取 `workspace-gates-execution-2/summary.json` | `passed=True`；19 项；唯一 exit code 0；首项 `started_utc=2026-10-31T05:59:15.716457+00:00`；末项 `finished_utc=2026-10-31T05:59:31.414579+00:00`；exit 0 |
| 链接 | `test -f docs/architecture-guide.md`；`test -f .../report.md`；`test -f .../review.md` | exit 0；本轮新增 §9/§4 无新链接 |

### 4.2 F1～F6 逐项结论

| 项 | 结论 | 独立证据 |
|---|---|---|
| F1 | 闭合 | `report.md:59` 已列 `Cancelled`（Prepare/Write/Sync/Publish）、`Io{stage}`（Create/Write/Sync/Publish）、`MetadataPreservationUnverified`、`ExistingTarget`、`StagingReplaced` 及 `BaselineError::{Storage,Decode}`，与 `atomic.rs` 的构造点一致（Cancelled：Prepare `:481`、Write `:596`、Sync `:636`、Publish `:274`；Io：Create `:491`、Write `:601`/`:607`/`:614`/`:621`/`:628`、Sync `:641`、Publish `:284`/`:296`/`:329`/`:353`）；`Written::into_baseline` 在 `:419` 返回 `Result<SourceBaseline, BaselineError>`，`BaselineError` 变体在 `:193-197`。没有 `Io{Prepare}`。 |
| F2 | 闭合 | `docs/engine-design.md:143` 与 `docs/v1-architecture.md:455` 均写明：已存在普通文件返回 `MetadataPreservationUnverified`，符号链接/目录/其他非普通文件返回 `ExistingTarget{kind}`，检查后的竞争返回 `Conflict`；与 `atomic.rs:307-323` 及 `:346` 逐分支一致。 |
| F3 | 闭合 | `report.md:87` 和 `:90` 已限定在 `aline-source` 内未公开 `src_off` 类型/换算；同时明确 `aline-buffer` 的 `SrcOffset`/`SrcRange`/`TextSnapshot::src_offset`（`coordinates.rs:73`、`:92`；`text.rs:253`）与 `aline-markdown` 的使用，并限定“当前未混用”。`src_off` 在 `aline-source/src` 无命中（exit 1），crate 外也无 `StorageRange` 消费者（exit 1）；报告不再声称没有该类型或没有消费者，也没有 RS-02 通过声明。 |
| F4 | 闭合 | `report.md:78` 已补 `lib.rs:17` 的公开 re-export；已提交审计日志第 2 节确实包含该行，摘要与日志一致，结论不变。 |
| F5 | 闭合 | `report.md:79`、`:89` 已写为“无代码级消费者”，记录 `aline-ui/Cargo.toml:15` 的未使用 manifest 依赖，并撤回未留可重放日志的 `codegraph_explore` 声明；独立 `rg` 结果与之一致（代码查询 exit 1，manifest 命中 aline-ui:15）。 |
| F6 | 闭合 | `report.md:44` 已改为“staging 已创建且未观察到替换时，成功/失败结果携带路径；创建前失败与 `StagingReplaced` 返回 `None`”，与 `atomic.rs:173-177`、`:305`、`:464-497` 一致。 |

### 4.3 新发现（轻微，非阻断）

- **N1［轻微］`report.md:3` 的“独立复核尚未追加”已陈旧/有歧义。** 本轮复审已存在于 `review.md`；若该句指“本报告未追加复核结论”，仍成立；若指“尚未进行独立复核”，则与 `review.md` 及本报告 §9 矛盾。建议改为“独立复核见 `review.md`；本报告不代写结论”。这是状态文案，不是能力或证据声明，不影响 F1～F6 的闭合判断。
- **N2［轻微］`report.md:59` 的时间限定不完整。** 该句写“成功 prepare 后的 `Written::into_baseline`”，但 `Written` 只在 `publish_new` 成功 `hard_link` 后产生（`atomic.rs:362-364`、`:419`），`prepare_atomic` 只返回 `PreparedAtomic`。建议精确为“成功发布后的 `Written::into_baseline`”。这不改变失败出口清单的实质，也不把 `BaselineError` 误列为 `prepare_atomic` 的直接返回值。

### 4.4 结论

F1～F6 全部闭合。修订后的哈希与 `report.md` §1 一致，回填 diff 仍为 17 行新增、0 行删除；`docs/engine-design.md:143` 与 `docs/v1-architecture.md:455` 的分类与 `crates/aline-source/src/atomic.rs:307-323` 一致；`report.md` 不再错误声称全仓没有 `src_off` 类型或没有消费者（只保留“`aline-source` 内未公开 `src_off` 类型/换算”与“当前 crate 外无代码级消费者”）；`workspace-gates-execution-2` 为 19/19 exit 0；`crates/` 干净；V-06/V-07 仍明确未通过，V-08 未触及，未出现 V-06/V-07/V-08、M0 或现有文件保存通过声明。未发现新的阻断性事实错误。

**接受，0 个勾选前阻断。** N1、N2 仅为轻微状态/措辞建议，不改变能力与证据边界，不要求再次复审即可由任务所有者按流程决定是否勾选 `tasks.md` 4.5。
