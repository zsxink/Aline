# 4.5 记录：编码基础回填与 K-08 偏移人工复核

日期：2026-10-31。状态：引擎 §5、架构 §5.1、V-06/V-07 的编码基础证据已回填；K-08 只读人工复核已执行并留原始日志。本文不修改代码、金样、Cargo 或 lint/guard，不宣称 V-06/V-07/V-08、M0 出口或现有文件保存通过。独立复核见 `review.md`；本报告不代写复核结论，`tasks.md` 4.5 的勾选由任务所有者依其结论决定。

## 1. 回填结果

| 文档 | 回填位置 | 回填前 SHA-256 | 回填后 SHA-256 | 追加内容 |
|---|---|---|---|---|
| `docs/engine-design.md` | §5.4（第 136 行起，新增） | `a4918c77fa54dc5d9fb739cadcae1537faf6e6ad9528bcd0ca23abac2bb92667` | `675670da265f3bf5adc7d897b875fca665a1a97ee69790ede54ae0b990f37349` | 4.1～4.4 实现边界、证据位置、未验证项；create-only 语义 |
| `docs/v1-architecture.md` | §5.1（第 455 行起，追加一段） | `851debcba8fa9c928d449e64dd3c60d7f62ffd507089ac4e3e209c3a6d8c65fb` | `9bb8a98e44828e80929a7e7ea59386fa795868a1e28a7fb76b59c358f5c49ef9` | source 侧偏移边界、当前无代码级消费者、未验证的覆盖/权限/跨平台 |
| `docs/v1-validation-plan.md` | V-06（第 83 行）与 V-07（第 91 行） | `a0d21cfbc4ee04c26df8f264f020a221039b3efea15b4af2ba262917d698210d` | `e60a36a7b9fa130fd6e5664d69a3b4d8d33a3adc68449f2a0ef235049d64f0a5` | M0 已固定的编码/行尾/I/O 基础证据；明确两项仍未通过 |

`docs/architecture-guide.md`（K-08 定义与 R-0-01/R-17-02 来源）未修改，回填前后保持 `f6f54d17d4f790e41cd7417e2ab34fde3e89e74d94f26b5558887668ec9465eb`。

差异性质：`git diff --stat` 为 3 个文件、17 行新增、0 行删除。回填只追加带日期的状态与证据段，没有改写既有规范句、PRD、AC、V、TA、ED、PA 状态或链接。V-06/V-07 仍明确写为未通过；V-08 未触及。

## 2. 边界表示取舍

| 取舍 | 选择 | 代价与边界 | 证据 |
|---|---|---|---|
| 编辑源码行尾 | 保留原始 CR/LF；只分离一个头部 BOM | 源码与磁盘 bytes 不再一一对应，偏移可差 3 字节（BOM）；重开等价只在限定行尾内成立 | 4.1 解码金样、4.2 编码金样 |
| 混合行尾重编码 | 按 CRLF 对数与未配对 LF 数取多数派，平票（含均为零）LF | 未编辑行的行尾也可能改变，属 SAFE-03 全文件规范化例外；不能把结果 String 与原文严格相等当作保存成功条件 | 4.2 金样与独立 Python 核对 |
| 孤立 CR | 逐字节保留；LF 策略下紧邻孤立 CR 的 CRLF 单元保留 CRLF | 结果可以仍含混合行尾；若把该 CRLF 改成 LF，再解码会吞掉孤立 CR | 4.2 金样 `cr-x-lf`、4.3 往返 |
| 重开语义 | 只允许 CRLF/LF 行尾表示差异 | 不用于补丁无变化、历史、工作版本或原 bytes 通道判定；逆补丁对实际冻结源码精确执行 | 4.2/4.3 金样 |
| 正文首字符 U+FEFF | 基线无 BOM 时补写一个头部 BOM 保护该字符 | 写出的 bytes 与源码不逐字节相同；成功写盘后新基线按实际 bytes 记为有 BOM | 4.2 保护性 BOM 用例、4.3 新基线替代 |
| “未修改”证据 | 同一基线对象上的冻结源码逐字相等；不接受调用方 `unchanged: bool` | 调用方必须持有正确基线；旧 `EncodedBytes` 不能冒充当前基线 | 4.3 API 反例 `unchanged-bool.rs`、`old-equality.rs` |
| 无 BOM UTF-16 | 不做启发式检测；若字节本身是有效 UTF-8 则按 UTF-8 接受 | 不静默转码、不有损替换；该限制由 4.1 金样固定，留待后续独立需求 | 4.1 金样与报告 |
| 预算 | 解码/编码/写入均在分配或写入前检查；编码长度溢出防御性失败 | `StorageBudget`/`IoBudget` 是注入上限，不是磁盘容量或平台限额；真实磁盘满未测 | 4.1～4.4 报告 |
| 文件发布 | 仅 create-only：`create_new` staging、完整写入、`sync_all`、`hard_link` 到不存在目标 | 已存在普通文件拒绝（`MetadataPreservationUnverified`）；无覆盖、截断、rename 覆盖、删除回退；无目录 fsync/断电/CAS 保证 | 4.4 报告与 review |

## 3. 未测权限与替换竞争

| 能力/场景 | 当前状态 | 证据与后果 |
|---|---|---|
| ACL、xattr、完整 permission metadata 复制 | 未实现、未测 | `MetadataPreservationUnverified` 明确拒绝现有普通文件；不能据 mode 或 `Permissions::readonly` 推断 ACL 保留 |
| 现有用户文件覆盖保存 | 不支持 | create-only 原语拒绝；不构成现有文件保存闭环，归档到 M2/V-08 |
| 检查后目标竞争 | 已测 | 目标检查后由测试钩子竞争创建，`hard_link` 返回 `AlreadyExists` 并分类 `Conflict`，竞争者 bytes 保留 |
| 检查后同 inode 原地改写 | 未检测 | 身份检查只比较 staging 路径与持有句柄；发布后不回读目标，`Written::bytes()` 报告本操作写入的 bytes（F3） |
| 跨进程 CAS / 并发保存 | 未实现 | 不把一次 `hard_link` 称为跨进程 CAS；串行写队列属 UI 保存编排 |
| 目录 fsync / 断电持久性 | 未测 | `Written` 明示目录持久性与 staging 清理未完成；不声称断电保证 |
| 真实磁盘满 | 未测 | 只做受控写入/sync 失败注入；注入位置已记录，不等同真实 ENOSPC |
| Windows/Linux | 未验证 | 句柄身份比较在非 Unix 目标未验证，当前实现恒拒绝发布（F4） |
| 网络卷/外接卷 | 未测 | 同目录/同文件系统前提只做词法检查，不 canonicalize |
| staging 清理 | 未实现自动清理 | staging 已创建且未观察到替换时，成功/失败结果携带路径（`Written::staging_path` / `AtomicFailure::staging_path`）；创建前失败与 `StagingReplaced` 返回 `None`；无 Drop 静默删除 |
| 真实应用保存闭环 | 未实现 | UI/SessionRegistry/PersistenceCoordinator 尚未进入 M0 |

## 4. K-08 人工复核

K-08 定义见 [架构指南第 345 行](../../../../architecture-guide.md)：评审逐项记录变更规则、所有者、写路径、输入键、失败出口、例外与证据；无自动化覆盖时写具体人工步骤。本节只复核 4.1～4.4 的源码/磁盘偏移边界，不扩大为完整 K-08 发布审计。

### 4.1 复核对象

| 维度 | 记录 |
|---|---|
| 变更规则 | 回填段只陈述 4.1～4.4 的实现事实与未验证边界；未新增或修改规范约束。V-06/V-07 保持未通过 |
| 所有者 | 原始 bytes、BOM/行尾统计与存储基线：`SourceBaseline`/`StorageSnapshot`（`aline-source`）；发布原语：`atomic.rs`（同一 crate）；buffer 已有自身快照坐标 `SrcOffset`/`SrcRange`，但不消费 `aline-source`；DocCore/UI 尚未实现 |
| 写路径 | 唯一文件写路径为 `prepare_atomic` → `create_new` staging → 分块 `write` → `sync_all` → `hard_link`；`Written::into_baseline` 只从已写 bytes 解码建立新基线。无 rename、truncate、`set_len`、原地写目标或删除 |
| 输入键 | `EncodedBytes` 绑定 `BaselineId`；编码相等由同一基线对象上的冻结源码比较得出；`StorageRange` 绑定 `BaselineId` 并用半开存储字节区间校验 |
| 失败出口 | `Budget`、`InvalidInput`（同路径/相对路径/缺父目录/不同目录）、`Cancelled`（Prepare/Write/Sync/Publish）、`Io{stage}`（Create/Write/Sync/Publish）、`Conflict`、`MetadataPreservationUnverified`（已存在普通文件）、`ExistingTarget`（Symlink/Directory/Other）、`StagingReplaced`；成功发布后的 `Written::into_baseline` 还可返回 `BaselineError::{Storage,Decode}`；失败不生成成功基线 |
| 例外 | 无：未新增依赖、`unsafe`、lint/guard 例外；现有文件 metadata 能力不是例外放行，而是拒绝 |
| 证据 | 4.1～4.4 金样与报告、4.4 独立复核、本目录 `k08-offset-audit.stdout.log`（410 行，stderr 0 字节，退出 0） |

### 4.2 实际步骤

2026-10-31T05:46:35Z 在仓库根执行：

```sh
sh docs/validation/m0-engine-foundation/group-4/step-4.5/run-k08-audit.sh \
  > docs/validation/m0-engine-foundation/group-4/step-4.5/k08-offset-audit.stdout.log \
  2> docs/validation/m0-engine-foundation/group-4/step-4.5/k08-offset-audit.stderr.log
```

退出 0；`utc_start` 与 `utc_end` 均为 `2026-10-31T05:46:35Z`；`HEAD=fd26bd8`；`rustc 1.98.1 (48a229cea 2026-09-01)`。脚本只读源码、文档和 git 状态，末尾 `git status --porcelain -- crates/` 为空，证明复核未改代码。

| 检查 | 实际结果 |
|---|---|
| `rg` 搜索 `src_off|source_off|disk_off|storage_off` | 无命中（退出 1，符合预期）；当前 crate 尚无源码/磁盘偏移同名或易混符号 |
| `rg` 搜索 `StorageRange|SourceRange|offset` | 仅 `storage.rs` 的 `StorageRange` 与错误偏移字段、`lib.rs:17` 的公开 re-export、`atomic.rs` 写循环的局部 `offset`；无第二种文档坐标 |
| crate 外消费者 | 无代码级消费者：`rg` 搜索 `aline_source|SourceBaseline|StorageSnapshot` 在 crate 外无命中（退出 1）；但 `aline-ui/Cargo.toml:15` 已声明尚未被 Rust 代码使用的 `aline-source.workspace = true`。`codegraph_explore` 结果未纳入可重放日志，故不作为本表证据 |
| `StorageRange` 契约 | 字段私有；`baseline: BaselineId` 与 `bytes: Range<usize>`；文档明确“不等同源码 UTF-8 范围”“不把存储字节误作 UTF-8 字符边界”；`slice` 复验基线身份 |
| `decode` | 先预算，再拒绝 UTF-16 BOM/非法 UTF-8；只 `strip_prefix('\u{feff}')` 一次，源码保留其余 CR/LF；不产生磁盘偏移回写 |
| `encode` | 以整段 `&str` 和基线对象比较；`walk_body` 只在源码字节上顺序扫描并输出磁盘字节，不把源码偏移映射成磁盘偏移或反向映射 |
| `atomic` | 以整个 `EncodedBytes` 分块写入；局部 `offset` 只是当前 chunk 的写入游标，不是文档/存储坐标 |

### 4.3 偏移边界结论

- 未发现 `aline-source` 内部把源码偏移与磁盘/存储偏移混用：该 crate 未公开 `src_off` 类型或换算函数，编辑源码以整段 `&str` 进出，存储侧只有绑定 `BaselineId` 的 `StorageRange`。仓库其他 crate 已有独立的源码坐标：`aline-buffer` 的 `SrcOffset`/`SrcRange`/`TextSnapshot::src_offset`，`aline-markdown` 已使用；它们不依赖 `aline-source` 的 `StorageRange`，当前不构成混用。
- 手工算例：磁盘 bytes `BOM + 甲\r\n乙` 共 11 字节，`乙` 的磁盘偏移为 8；编辑源码 `甲\r\n乙` 共 8 字节，`乙` 的 `src_off` 为 5；差值为 BOM 的 3 字节。`python3 -I` 实际输出 `source_len 8`、`source_offset_of_乙 5`、`disk_len 11`、`disk_offset_of_乙 8`、`difference 3`，与引擎 §5.1 的例子一致。
- 残余风险：`StorageSnapshot::bytes()` 和 `StorageRange::byte_range()` 公开存储坐标；未来 buffer/markdown/UI 取得原始 bytes 后仍可能自行计算偏移。当前 crate 外无代码级消费者（`aline-ui` 仅在 manifest 声明未使用的依赖），故只能证明“现在未混用”，不能证明未来消费方不会混用。接入下游 crate 时必须重新做 K-08，并优先使用受检转换而不是 `at - start` 式算术。
- 4.5 未在 `aline-source` 引入 `src_off` 类型，也未新增跨坐标换算；buffer 的 `SrcOffset` 属于其自身快照坐标，不能替代存储坐标。4.5 不预建该坐标类型，也不把这次复核写成 RS-02 已通过。

## 5. 未执行/未覆盖

- 未运行真实权限/ACL/xattr、目录 fsync、断电、真实磁盘满、跨进程 CAS、Windows/Linux、网络卷实验。
- 未实现现有文件覆盖保存、保存编排、恢复清理、串行写队列；`MetadataPreservationUnverified` 是拒绝，不是能力。
- 未开始 V-06 的资源上限测量、降级汇总与 UI 呈现；未开始 V-07 的块/IME/格式化/只读保护夹具；V-08 未开始。
- 未做完整 PRD/AC/V/TA/ED/PA 发布审计；本次 K-08 只覆盖 4.1～4.4 的偏移与写路径边界。
- 未修改代码，因此没有新增代码级测试；4.1～4.4 的既有测试与门禁证据不因本次文档回填重新判通过。

## 6. 证据边界

本文与回填段只说明编码基础和 create-only 原语的当前证据。它们不证明：现有文件保存、权限/ACL 保留、目录持久性、断电安全、跨进程 CAS、Windows/Linux 语义、真实应用保存闭环、V-06/V-07/V-08 通过或 M0 出口通过。`tasks.md` 勾选不等于出口通过。

## 7. 独立复核

本节不写复核结论。独立复核者应在 `review.md` 中自行检查：回填是否只追加状态而未改规范；哈希与命令是否可复现；K-08 步骤是否足以支持“当前 crate 未混用偏移”；未测权限/替换竞争是否被误写成已验证能力。任务所有者据其结论决定是否勾选 4.5。

## 8. 提交前本地门禁（2026-10-31，追加）

2026-10-31T05:48:13.296145+00:00 至 05:48:30.215264+00:00，在仓库根以隔离环境运行统一门禁：

```sh
CARGO_HOME=<cargo-home> \
RUSTUP_TOOLCHAIN=1.98.1 CARGO_NET_OFFLINE=true \
python3 scripts/check_workspace.py \
  --cargo-home <cargo-home> \
  --output-dir docs/validation/m0-engine-foundation/group-4/step-4.5/workspace-gates-execution-1
```

19 项全部 exit 0：工具链/工具存在性 4 项、版本 4 项、`fmt-effective-config`、`fmt`、`clippy`、`test`、`dependency`、`dependency-self-test`、`lint`、`lint-self-test`、`safety`、`safety-self-test`、`public-api`。原始 stdout/stderr、命令 JSON 与 `summary.json` 均保存在该目录，未改写。该次运行只覆盖当前工作树（代码未改，仅含已回填文档），不证明其他 OS/target，也不替代 9.1 的最终全门禁。

复核后修订（2026-10-31T05:59:15.716457+00:00 至 05:59:31.414579+00:00）再次运行同一命令到 `workspace-gates-execution-2/`，19 项仍全部 exit 0；修订仅涉及文档与报告，未改代码。

## 9. 复核后修订（2026-10-31）

独立复核（`review.md` F1～F6）后，本文与回填文档修正如下；复核结论仍由复核者在 `review.md` 增量段给出，本节不代写结论。

- F1（`report.md` §4.1 失败出口）：`Cancelled` 改为 Prepare/Write/Sync/Publish，`Io{stage}` 改为 Create/Write/Sync/Publish，补充 `Written::into_baseline` 的 `BaselineError::{Storage,Decode}`。
- F2（`docs/engine-design.md` §5.4、`docs/v1-architecture.md` §5.1）：已存在目标按代码分类——普通文件 `MetadataPreservationUnverified`，符号链接/目录/其他非普通文件 `ExistingTarget{kind}`；保留拒绝与不构成现有文件保存能力的结论。
- F3（`report.md` §4.3）：改为 `aline-source` 内未公开 `src_off` 类型或换算；`aline-buffer` 已有 `SrcOffset`/`SrcRange`/`TextSnapshot::src_offset`，`aline-markdown` 已使用，但它们不依赖 `StorageRange`，结论限定为当前未混用。
- F4（`report.md` §4.2）：审计摘要补 `lib.rs:17` 的 `StorageRange` 公开 re-export。
- F5（`report.md` §4.2/§4.3）：改为“无代码级消费者”，记录 `aline-ui/Cargo.toml:15` 尚未被 Rust 代码使用的 manifest 依赖，并撤回未留可重放日志的 `codegraph_explore` 声明。
- F6（`report.md` §3）：staging 路径仅在已创建且未观察到替换时随成功/失败返回；创建前失败与 `StagingReplaced` 返回 `None`。

修订后回填 diff 仍为 17 行新增、0 行删除；`docs/engine-design.md` 与 `docs/v1-architecture.md` 的回填后哈希已更新至 §1，`docs/v1-validation-plan.md` 与 `docs/architecture-guide.md` 不变。另按增量复审 N1/N2 更新第 3 行状态句与“成功发布后”措辞；复核者已明确 N1/N2 不要求再次复审。
