# Design

## Context

动机见 proposal。当前 source `prepare_atomic` 准备同目录 staging，`publish_new` 仅用 hard link 发布不存在目标；既有目标替换、权限/目录同步与保存点未接。DocCore 有当前快照、组合期发布等待及恢复读取，但没有文件协调器。调查依据为源码 `crates/aline-source/src/atomic.rs`、`crates/aline-ui/src/document/doc_core.rs` 与 V-08，不能把它们描述成已完成保存。

M1 出口已于 2026-11-13 按[决定 §43](../../../../docs/decisions.md#43-2026-11-13-m1-阶段出口裁决macos-arm64-范围)在 macOS arm64 范围收口；M2 可编辑会话仍是生产接线前置，当前可准备本方案、故障输入和纯规则。依据架构 §7.1/7.3/7.4/13、引擎 §8/17、R-15-01～13、CODE_STANDARDS §2～6。W5 负责原生平台组合结束接线，W4 消费同一发布协议，不复制组合状态机。

## Goals / Non-Goals

**Goals:** 单文档一个可审查的串行写盘所有者；保存/恢复分离，实际写盘事实可结算，失败保持可恢复。先无头故障矩阵和独立审查，再真实平台接线。

**Non-Goals:** 不扩大依赖、unsafe 或运行时；不重建历史/文本；不以 M2 最小样例替代 M4/M5 完整资源/路径矩阵；不提前开放 M1 保存按钮。

## Decisions

### 0. 恢复记录 V1 身份、格式与发布门槛

恢复数据位于应用管理的专用根目录；根目录和其子项均按普通文件/目录、应用所有权与路径代次进行受检打开。路径字符串只用于恢复关联，绝不作稳定 ID、资源文件路径或写入授权。恢复根被替换、链接化、不可排他访问或超出预算时，拒绝写入和清理。启动只枚举最多 256 个活动谱系；永久 ID tombstone 不参与该枚举。

身份分三层，均为非安全令牌：

- `RecoveryDocumentId` 是恢复根内以 `create_dir` 独占预留的非零 `u64`，目录名为固定 16 位小写十六进制。系统时钟纳秒只作候选起点；身份只在独占创建成功后成立，碰撞时受检递增并再次创建。永久 `ids/<id>` tombstone 不删除，ID 不复用；时钟不可读取或 u64 分配溢出时拒绝新身份。
- `RecoveryRecordId = (RecoveryDocumentId, generation)`；`ResourceRecordId = (RecoveryDocumentId, ordinal)`。generation/ordinal 均为非零受检 `u64`，在谱系锁内以受检 high-water 记录先分配，且随后独占预留代次目录。high-water 只可向前更新；更新回执不确定时保留 reservation、停止该谱系进一步写入并在重启后以 high-water 与所有受检代次名的较大值重建，避免崩溃后覆盖。清理不删除稳定身份 tombstone。
- 每个永久稳定身份目录保留唯一 `owner.lock`。写入、恢复接管、逐文档放弃和资源清理都须持有该文件句柄的标准库非阻塞独占锁；锁文件本身不删除。占用、能力不支持、身份/父目录变化或锁后复验失败时拒绝该谱系操作。锁只约束遵守本协议的 Aline 实例，不是跨进程磁盘 CAS。

V1 使用固定的小端二进制格式，不依赖序列化 crate：

| 位置 | 字段 | 验收规则 |
|---|---|---|
| 表头 | 8-byte magic `ALINEREC`、`u16 version=1`、`u8 record_kind`、`u8 flags=0`、`u64 body_len`、`u32 body_crc32`，合计 24 bytes | 拒绝未知 kind/flag、长度溢出、预算超限和尾随字节；CRC 为 CRC-32/ISO-HDLC，仅检出意外损坏，不认证来源、不代表内容等价 |
| Snapshot 正文固定前缀 | `u64 document_id`、`u64 generation`、`u64 parent_generation`（无则 0）、`u64 path_generation`、`u8 input_flags`、`u8 path_kind`、`u32 path_len_bytes`、`path_len_bytes` 个路径字节、`u64 anchor`、`u64 head` | 所有整数小端；`input_flags` 仅 bit0 表示捕获时处于组合态，恢复为普通文本；`path_kind` 为 0 无路径（长度必须 0）、1 原始 Unix bytes、2 Windows UTF-16LE（长度必须为偶数）。ID/generation 非零；路径字节上限 32 KiB |
| Snapshot 正文文件表 | 按顺序写 `u64 source_len + u32 source_crc32`、`u8 logical_saved_present`，若为 1 再写 `u64 logical_saved_len + u32 logical_saved_crc32`、`u8 baseline_present`，若为 1 再写 `u64 baseline_len + u32 baseline_crc32` | 对应正文同代固定文件 `source.utf8`、`logical-saved.utf8`、`baseline.bytes`；presence 仅允许 0/1。文件 length/CRC 与磁盘独立复验 |
| Snapshot 正文资源表 | `u32 count`，每项顺序为 `u64 ordinal`、`u32 reference_key_len`、reference bytes、`u64 bytes_len`、`u32 bytes_crc32` | ordinal 非零且唯一；引用键唯一且非空、最大 4096 bytes；资源文件路径只由受检 ID 派生；任一资源不可读/校验失败，该代不完整，不得据此清理旧代 |
| Retired 正文 | `u64 document_id`、`u64 generation`、`u64 retired_through_generation` | `record_kind=1` 时正文严格只有此三字段；V1 终态 marker 要求 `retired_through_generation = generation - 1`，generation 必须非零且不得溢出。`record_kind=0` 为 Snapshot；其它类型拒绝 |
| 结束 | 正文结束位置 | 文件总长必须恰等于 `24 + body_len`；正文 CRC 和所有正文/资源文件独立校验 |

目录布局固定为：

```text
recovery/v1/ids/<document-id>/                  # 永久稳定身份 tombstone directory
recovery/v1/ids/<document-id>/owner.lock           # 永久锁，与 active 成员资格解耦
recovery/v1/ids/<document-id>/retired              # 可选、同步成功的 kind=1 manifest
recovery/v1/quota.lock                             # 所有谱系共用的容量分配锁
recovery/v1/reservations/<document-id>-<generation> # 写入前预留的容量上界
recovery/v1/active/<document-id>/next-id          # generation/resource high-water
recovery/v1/active/<document-id>/g/<generation>/source.utf8
recovery/v1/active/<document-id>/g/<generation>/logical-saved.utf8
recovery/v1/active/<document-id>/g/<generation>/baseline.bytes
recovery/v1/active/<document-id>/g/<generation>/r/<ordinal>/bytes
recovery/v1/active/<document-id>/g/<generation>/manifest
recovery/v1/active/<document-id>/trash/<generation> # 已移出恢复扫描集合的清理残留
```

manifest V1 的资源表项按 ordinal 升序序列化；路径/引用键是原始路径单元/UTF-8 字节，非文本字段不得隐式转码。CRC 实现固定为 CRC-32/ISO-HDLC（反射多项式 `0xedb88320`、初值/最终异或 `0xffffffff`），不使用 Rust `DefaultHasher`。

`next-id` 是 32-byte high-water 记录：8-byte magic `ALINESEQ`、`u16 version=1`、`u16 flags=0`、`u64 next_generation`、`u64 next_resource_ordinal`、`u32 CRC-32`（覆盖前 28 bytes）。两数必须非零并只增不减。首次创建用同目录 create-only staging、无覆盖发布、文件同步和活动谱系目录同步后才可分配；更新时受检重读旧记录、写同目录 staging、文件同步、以 source 已验证的既有目标替换、再同步谱系目录。任何阶段不确定都停止该谱系操作，重启后计算 `next_generation = max(stored_next_generation, max_reserved_generation + 1)` 与 `next_resource_ordinal = max(stored_next_resource_ordinal, max_reserved_resource_ordinal + 1)`；无已用 ID 时其 max 为 0，加法溢出拒绝续写。若 `next-id` 损坏/未知版本，保留它及全部代次，只允许用户另存/显式恢复，不自动续写或清理。

容量 reservation 是 40-byte 文件：8-byte magic `ALINEQUO`、`u16 version=1`、`u16 flags=0`、`u64 document_id`、`u64 generation`、`u64 reserved_bytes`、`u32 CRC-32`（覆盖前 36 bytes），路径由文档 ID 和 generation 派生。根锁内只用 create-only 发布，不能重复覆盖；`reserved_bytes` 是该代源/逻辑保存/存储基线/资源/最多 1 MiB manifest 的总 payload 精确上界。预约文件本身及计数器 staging 另按本节容量规则预检。CRC 错误或未知版 reservation 保留并使配额不可用，不能授权删除其他恢复数据。

仅 `manifest` 是该代的提交标记。所有需要两种锁的路径固定先取得 `quota.lock`，再取得 `ids/<id>/owner.lock`，不得反向取锁。开始恢复写入时，在两锁下选取候选 ID、复验容量并先更新 generation/resource high-water 到严格大于本次将使用的 ID；同步 high-water 文件与谱系目录后，才可独占发布 reservation 并同步文件和 reservation 目录。这样即使 high-water 后、reservation 前退出也只会跳过 ID；反向顺序禁止，因为清理 reservation 会丢失唯一的代次证据并导致重启复用。之后释放 quota 锁，保留 owner 锁并独占预留代次目录。逐文件同步源码、逻辑保存基线、存储基线与当前代次所需资源，再逐级同步新建的资源子目录、generation、`g` 和活动谱系目录。独占写入并同步 manifest staging，无覆盖发布为正式 `manifest`，再同步 generation 目录及其 `g` 父目录。首次创建 `active/<id>` 时，先同步谱系内容，再逐级同步 `active` 与恢复根目录，完成后才能报告首次发布成功。

清理旧代或结算 reservation 需要两锁。若曾释放 owner 锁，结算按固定顺序重新取得两锁，重验同一谱系、代次、reservation、正式 manifest 及其所有文件的身份与内容。启动扫描或重新取得 owner 锁后，不能凭可读的 manifest、匹配 CRC 或前一持锁者的结果推定持久性。用该代作为清理依据或以已发布状态释放预约前，须重新同步正式 manifest 及其引用文件、资源子目录、generation、`g` 和适用的祖先目录。全部同步成功后才在根锁内将预约转为实际占用，删除 reservation 并同步 reservation 目录。任何未发布 reservation 都只有在确认相应 high-water 已同步推进到大于该代及其资源 ordinal 后，才能删除其最后一份唯一证据；若推进或同步失败，保留 reservation/代次目录并停止该谱系写入。确认不可继续且未发布的代次只在持两锁、复验同一代并同步确认正式 manifest 缺失后，才按受检半成品处理。其现存文件改按实际占用计入，预约释放也须同步。若发布已发生而同步结果仍不确定，保留发布事实、前后代及资源和预约，不授权删除前代或回报“未写入”。

回收时保留最新且已确认同步的完整代，只移动无当前/历史/恢复/在途引用的经验证材料。先把待回收代原子移入该谱系 `trash/`，同步源目录 `g` 和目标目录 `trash`，以及此次新建目录涉及的父目录项，全部成功后才递归删除内容。移动或同步失败时保留内容。容量释放只能发生在递归删除后，逐级同步所有受影响目录项直至活动谱系目录，并在重新取得 `quota.lock` 后完成目录同步与全量重扫；预约删除也须先同步 reservation 目录，再同步恢复根目录项并重扫。此规则同样适用于接管者：任何进程每次新取得 `quota.lock` 并准备复验或计算可用容量前，必须在锁内自底向上 `sync_all` 所有可能存在未确认删除、且参与容量计量的当前可见目录（`active` 及其代次/`trash` 子目录、`reservations`），再枚举并计量；同步其祖先目录至恢复根。`ids` 下 tombstone/owner/retired 只新增不删除，不需随每次配额复验全量遍历；退役 marker 在创建/接管时单独同步。任一相关目录同步失败则关闭新预约和清理。这样即使前一进程在 unlink/rmdir 后、父项同步前退出，接管者也先把当前可见目录项状态确立为耐久事实，不能把尚未确立的释放容量供新代使用。删除或同步失败保留可识别的 trash 残留，不把未确认消失的字节视为可用空间。已发布资源和记录始终按实际事实结算。

稳定 ID 预留需 `create_dir(ids/<id>)` 后同步 `ids` 和恢复根，再创建 `active/<id>` 并同步 `active` 和恢复根；目录项任一步不确定前不可发布文档快照。资源只在每代目录内存一份，manifest 该代资源表是唯一验收来源，下一代从当前快照复制并校验；不另存全局 resource bytes 副本。成功保存关闭或逐文档放弃时，按 `quota.lock → owner.lock` 同时持锁，结算该谱系所有写盘回执；在已持久 high-water 的 `next_generation` 取退役记录自身 generation，并把 `retired_through_generation` 设为 `generation - 1`，即覆盖 marker 之前全部已分配快照/预约 ID。先同步推进 `next_generation` 到大于 marker generation，再无覆盖发布 `ids/<id>/retired`，依次同步 marker 文件、`ids/<id>` 和 `ids`，之后持锁清理活动 generations 并同步 `active`。确认的退役是该稳定 ID 的终态，之后不得再为它分配代次或更新 marker；恢复数据时必须创建新 RecoveryDocumentId，并迁移所选快照。若退役发布/同步不确定，接管者复验并同步正式 marker 文件，再同步 `ids/<id>` 和 `ids`，确认后才隐藏其覆盖范围；若发现高于 `retired_through_generation` 的 snapshot/reservation，视为协议不一致，保留所有材料并禁写禁清。未知/损坏记录不自动清理。

启动扫描先检查应用恢复根与预算，再枚举活动谱系目录（最多 256）逐个持锁验证 ID、代次、manifest、source/baseline 和全部资源引用；永久锁位于 `ids/<id>`。有效终态退役 marker 必须覆盖 marker generation 之前所有已分配代次；接管时复验并同步正式 marker 文件，再同步 `ids/<id>` 与 `ids`，确认后才隐藏覆盖范围。若发现覆盖范围外的 snapshot/reservation，视为协议不一致并禁写禁清，不将其当作可恢复更新。活动谱系任一代、目录项、high-water、退役 marker 或 reservation 出现未知 V1 不能理解的版本/kind/path 编码时，该谱系禁写禁清并保留原 bytes，即使存在 V1 完整代也不自动恢复；用户可明确选择已验证的 V1 代，但该谱系仍禁写禁清。损坏的最高 V1 snapshot 隔离并报告，继续保留且可呈现更早完整代；坏退役 marker 不隐藏任何代并使该谱系禁写禁清。无正式 manifest 的已知半成品只有在持锁、结构完全属于 V1 且确认不是已发布数据时才可回收，否则报告/保留，不进入恢复列表。未知格式记录不授权清理。显式恢复先进入内存，组合候选按普通文本恢复且不伪造活动 marked 状态；只有所有数据重新校验后才呈现该文档，源文件不被改写，原关联路径默认禁用自动保存并只在用户重新确认/选取后授予路径代次。显式放弃使用同格式的退役 manifest；只有退役 marker 文件和父目录同步均确认后才隐藏覆盖范围内旧代并允许资源回收。确认无更高代和在途引用后，删除 `active/<id>` 目录并同步 `active`，释放活动名额；锁与退休凭证仍留在 `ids/<id>`。

预算随 V1 固定：单个 manifest 最大 1 MiB；每个活动谱系最多并存 2 个完整 snapshot、1 个正写入代次、最多 32 个已知不完整 reservation（超过后只回收经持锁确认的 V1 未发布半成品；未知项永不自动清理）；generation 与 resource ordinal 可持续递增至 u64 上限但不允许复用。每代资源 ID 最多 256 个；路径最多 32 KiB；`source.utf8`、`logical-saved.utf8`、`baseline.bytes` 各最多 10 MiB；单资源最多 64 MiB、单代所有资源合计最多 256 MiB；活动谱系最多 256 个；恢复根 payload 总上限为 1 GiB。

容量在 `quota.lock` 下按文件 payload 与尚未兑现的承诺合计。每份未结算预约的代次占用为 `max(reserved_bytes, actual_generation_bytes)`，预约记录本身另外按实占用计入。其余已发布、trash、孤儿和控制文件按实际 payload 计入，包含 `ids` 下永久保留的 retired marker；它们可不参加逐次同步重扫，但不能从 1 GiB 总额中漏计，计数必须由持久台账或枚举实际文件得出。对经句柄身份核验、确属同一次 no-clobber 发布的 staging 和正式文件硬链接，只计同一底层文件一次；不同文件即使 bytes 相同也各计。若无法核验身份，保守分别计费；容量不足时拒绝发布并保留旧材料。

准入按并存峰值计算。创建活动谱系前预留首次 `next-id` 的 32 bytes，并为该谱系保留 80 bytes 的终态退役余量（48-byte retired manifest 加 32-byte `next-id` staging）；退役 marker 的 generation 从当前持久 `next_generation` 分配，先持久推进 high-water，再发布 marker，不另建第二条退役记录。活动期间退役文件、high-water staging 共用此余量，峰值按 48+32 bytes 计入，同一受检硬链接按上述规则只计一次；退役确认且删除活动成员后改按永久退役文件与最新 high-water 的实际占用计入。每次新代准入检查“当前计量总额 + `reserved_bytes` + 40-byte 预约文件 + 32-byte `next-id` 更新 staging”不超过上限；旧 `next-id` 已在当前计量总额内。控制记录的额外空间在根锁内检查并消费，写后以实际文件和持久预约重新计量，再释放根锁。预约、首次计数器、正式 manifest 和退役的无覆盖发布使用经验证的无复制原语，载荷从 staging 移至正式项或由二者共享受检文件身份；平台不能证明此行为则拒绝该发布。没有空间时可按前述已确认同步的回收协议移除更旧代；只移动到 trash 未释放容量。清理确认后重算，仍不足即拒绝并保留最新保护。任一计数、长度、累加或 ID 溢出、预算超限都拒绝该项且不清理任何未被证明可回收的材料。此版本不得通过提高实现常量改变上述上限，需新版本格式/预算审查。

任务夹具以固定手写字段和独立预期为准，至少有完整 V1、截断/错误 CRC、未知版本三个样本；成功样本覆盖无路径/有路径、方向选区、当前文本和保存基线分离。扩展夹具需含混合换行、首字符 U+FEFF、非法选区边界、超预算、重复资源键、缺图、错误资源 CRC、符号链接/非普通文件、ID 不符及恢复后源文件 bytes 不变。CRC 独立标准向量为 `123456789 → 0xcbf43926`。

### 1. 职责与写入口

| 所有者 | 输入/输出与职责 | 禁止承担 |
|---|---|---|
| source 受检 I/O | 冻结编码字节、预期目标版本、取消/预算 → 成功事实或阶段失败/冲突 | UI 任务/历史/恢复策略 |
| UI 持久化协调器 | 文档/工作版本、路径代次、资源计划、基线；唯一排队与结果结算 | 直接改 Buffer、丢弃过期写盘事实 |
| DocCore | 取得包含草稿的当前快照、受检提交重载和统一历史 | 文件 I/O、保存 timer |
| UI 恢复/资源边界 | 独立恢复目标与资源清单的逐步发布/保留/清理 | 把资源当可随意淘汰缓存 |

模块按以上生命周期分离：持久化转换不放 render；source 平台原语和 UI 状态可各自无窗口测试。采用具体 struct/enum 和函数；仅在文件调用确需故障注入时引入最小接口。无通用 Repository/事件总线/额外 crate。跨 crate 公开面有真实 UI 消费者才导出；既有 create-only 保留兼容。

### 2. 状态与身份

协调器显式表示空闲、等待组合、排队、准备/发布、冲突、失败；保存反馈依据实际基线与当前内容，不能由状态枚举替代等价判断。请求带 DocumentId、WorkingVersion、路径代次、请求 ID、目标 DiskVersion、编码来源基线与资源计划身份；只有同文档目标可更新当前路径，任何已发生写盘都必须记录。较新的当前内容相对实际保存基线重新判定，旧失败不得覆盖新成功。取消只撤执行资格，实际停止/回执结算后再释放资源。

### 3. 受检替换与故障边界

用已有编码契约生成冻结 bytes，先独占创建同目录 staging、完整写入，并在发布前读取目标权限、按 WC-04 复制/验证安全访问语义；只有错误明确属于 Unsupported/ENOTSUP 且 staging 创建与替换访问安全已由平台证明时才可继续，其他权限失败必须停止并保全原目标。权限阶段成功后同步 staging，再核验目标观察并执行平台受检替换，替换后再做目录同步和 staging 清理。目录同步失败返回 `PublishedDurabilityUncertain`，不得伪装成“未写入”或普通成功。禁止先删、截断和静默回退。mode 不代替 ACL/xattr。平台所需能力若必须新依赖/unsafe，先准入/裁决，未获准则失败关闭；不在设计中默许例外。写前检查与替换有外部竞争窗口，报告实际保证，不能命名为跨进程 CAS。

### 4. 保存、重载和恢复

1 秒防抖只排用户文档保存；组合期延期，显式请求通过既有 ED-01 等待，结束后重新冻结与验证。10 秒恢复计时独立，独立目标包含当前版本/身份/存储元数据和可达临时资源。禁止将恢复周期作为自动保存参数，也不创建文档旁 `.bak`/`.corrupt-*`。

聚焦/写前检查基线内容，mtime/size 只快筛；无本地改动自动重载，有改动停自动保存并显示三方内容及四个出口。自动重载候选返回时必须重验文档存活、路径/磁盘观察代次和当前 `WorkingVersion`，并以包含最新草稿的当前内容重新证明无本地修改；草稿仍打开本身不阻止重载。组合活动或暂不能安全建立历史边界时延后，保留输入/选区，组合自然结束后按同一协议重验；其他条件不匹配则拒绝候选并按当前状态重读。显式载盘绑定用户确认时的本地与磁盘视图，任一变化都重新确认。自动/显式载盘提交时由 UI 会话协调器与 DocCore 把新磁盘基线和可撤销历史边界作为同一逻辑提交，新基线不随 Undo/Redo 回退。恢复先入内存；损坏条目隔离。首次保存/另存为逐步确保资源、规划引用补丁、写盘后由 DocCore 再验证当前工作版本、选区/预期片段与资源语义；合法引用补丁、路径关联、新存储基线和资源关联一致时才更新当前关联。输入变化则重规划/重试；不能安全同步时保留旧关联并结算部分成功。临时资源当前/历史/恢复/在途仍引用时保留。

## Risks / Trade-offs

- 平台替换/权限/同步有差异 → 无头反例加 macOS 真实文件实验，明确 Windows/Linux/Intel 未测；不能泛化断电保证。
- 资源多文件无法物理原子 → 明确逐步发布、可恢复副本和幂等重试，失败不提前清理。
- 高成本指纹/编码阻塞界面 → UI 后台调度，按键/render 不全文比较/写盘；以 V-05 实测约束。
- 尚无 M2 可编辑/资源闭环 → 保留任务前置，不能以方案勾选实现或宣布 V-08 通过。

## Migration Plan

先增加无头机制及独立测试，完成源 I/O/状态一致性独立审查，通过 M1 和 M2 前身份方案后再接 UI。恢复 V1 的格式、ID、预算、目录同步与启动选择按本文 Decision 0 冻结，并由 task 1.2 的独立 hex 金样锁定；不推断旧记录存在。撤回 UI 接线不得删除已有恢复材料；在途写盘须结算，已发布事实不可用代码回滚抹去。无数据库迁移或依赖升级。

## M2 V-08 共同身份/事务门槛

共同身份元组、唯一状态所有者、发布阶段和回执结算以 `../m2-editable-core/design.md` 决定 1 的冻结方案为准。本 change 专属实现继续负责 source 受检替换、串行持久化协调、外改/重载、恢复和资源迁移，不另立相异身份模型。特别遵守：`DiskVersion` 只是会话代次；`mtime/size` 仅快筛；实际写入后的结果即使旧、取消或文档关闭仍须结算；保存回执同时保留冻结输入和由实际写入 bytes 建立的存储基线；多资源不是物理原子事务。此方案仍须完成伞变更 task 1.2 独立审查后，才允许本 change 的真实文件/恢复接线。
