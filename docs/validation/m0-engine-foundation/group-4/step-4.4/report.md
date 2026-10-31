# 4.4 记录：权限复制、目标保全与 create-only 实现

> 状态（2026-10-31 追加）：本文此前的暂停阶段与 execution-2 记录描述 4.4 在 A/B/C 裁决前的证据；其中“4.4 尚未完成/未实现”“未执行 `cargo test`”等表述只描述当时状态，不是当前结论。用户选择 A（仅 create-only 原语）后，实现与 execution-3 证据见文末“Plan A 实现与执行-3（2026-10-31，追加）”。独立复核已接受 F1/F2 闭合（[`review.md`](review.md) 增量段，0 个剩余勾选前证据阻断），4.4 据此勾选；仍不宣称 V-08、M0 出口、现有文件保存或跨平台能力。

4.4 暂停阶段的状态：未实现生产文件原语，`tasks.md` 未勾选。先建立独立预期与最小反例，并在私有临时目录复现两个候选的风险；这些探针不是 `cargo test -p aline-source atomic`，不能证明该任务通过。

## 已复现的边界

1. `fixtures/std-permissions.rs` 以独占临时文件、`File::set_permissions` 和 `rename` 替换测试文件。原文件设置 mode `0600`、ACL `everyone allow read` 和测试扩展属性；替换后 mode 仍为 `0600`，但 ACL 与该扩展属性丢失。**只复制标准权限位不能证明访问权限保全**；扩展属性结果是观测，不能由本探针单独推断所有 xattr 均受同一产品策略约束。
2. `fixtures/std-copy-path.rs` 先独占创建临时文件，再模拟临时路径被换成指向目标的符号链接。随后 `std::fs::copy(target, temporary)` 返回 `Ok(0)`，目标测试文件由 `OLD\n` 变为空。该反例表明路径重新打开的候选在此竞争下不满足原目标保全，不能直接作为安全的权限复制方案；不代表所有基于标准库的其他方案都不可行。

`fixtures/expected.v1.txt` 在探针实现前手写目标状态与反例条件；`check-std-probes.py` 创建自己的 `TemporaryDirectory`，在其内部执行，不访问用户文档。探针使用系统 `chmod`、`ls`、`xattr` 验证；它们不是拟议生产依赖。

## 执行证据

`python3 docs/validation/m0-engine-foundation/group-4/step-4.4/check-std-probes.py` 于 2026-10-31T04:04:54.688192Z 开始、04:04:55.503432Z 结束，退出 0（两个反例均按预期复现）；完整子命令和输出见 `probe-logs/probes.log`，命令环境与起止/退出码见 `probe-logs/probes.json`。探针位置位于 macOS 27.0.1、Apple arm64 的 APFS Data 卷，Rust 1.98.1；Cargo 环境为离线，`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。测试用 ACL/xattr 及路径故障均在临时目录内生成，退出后删除。

## 未解决问题与下一步

SAFE-06、架构 §7.3 与 R-15-07 要求权限失败受检、替换失败保全原文件，且明确 mode 不等于 ACL/xattr；源码默认 `unsafe_code = "forbid"`。当前两个未经验证的候选分别暴露权限事实丢失与截断风险。Rust 标准库的 `File::set_permissions` 只覆盖平台权限位；锁定工具链的 Apple `fs::copy` 重新打开目标路径。尚未证明可用的、无需自有 unsafe 的完整平台元数据安全后端；本机缓存的 `rustix` 句柄级 `fcopyfile` 声明为 unsafe，不能仅因已缓存就当作准入或例外。

下一步应先评估并审计可提供安全句柄级能力的封装、依赖及三平台适配，按依赖准入规则提出具体方案；若确需自有 unsafe，须另提最小平台例外、独立安全评审与验证。之后才能按 4.4 实现并验证临时创建、部分写入、替换前失败、取消/冲突、成功回执与实际文件系统边界。不能以拒绝普通含 ACL 的文件、静默降级或未审计依赖作为解决办法。暂停阶段当时未执行 `cargo test -p aline-source atomic`、本组 Cargo/workspace 门禁或完整 V-08，未提交或推送本组；A 方案执行-3 的实际命令与证据见文末。

只读候选调查：缓存中的 `rustix` 1.1.4/1.1.5 提供部分安全句柄级 xattr 接口，但其 `fcopyfile` 与 `copyfile_state_free` 为 unsafe，且不能仅由 xattr 接口推断已覆盖 macOS ACL。`windows-permissions` 0.2.4 提供安全签名的 Windows 安全描述符接口，但仅覆盖 Windows，且其实现含成功后空指针的 `expect`；尚未审计其可达性、依赖闭包或许可。上述均为调查线索，不是准入建议或验证结果，未添加依赖。

用户随后指示“你的建议来吧”，本轮继续做安全封装的隔离源码初筛；见 `backend-evaluation.md`。目前具体候选仍未完成准入，4.4 未实现；最小平台 unsafe 例外只是待独立评审的可能方案，不是用户已批准的实现。

## macOS 隔离探针 execution-2：实际执行结果

### 授权与源码身份

用户明确回复“准你完成”，该授权只覆盖 `macos-probe-proposal.md` 所述隔离实验，不构成生产 unsafe、`libc` 直接依赖、lint/守卫例外或现有文件覆盖授权。仓库内没有用户原始对话，独立复核按任务说明接受该授权范围，不能从 Git 历史补证。探针源码身份如下（`macos-probe-execution-2/source-hashes.json` 记录值；独立复核者用 `shasum -a 256` 重新计算并与记录一致，见 `review.md`）：

| 文件 | SHA-256 |
|---|---|
| `macos-probe/Cargo.toml` | `9bbcfd0663ce735f60f06eba544f0679da882751598b0db9d42e93b087d4b10d` |
| `macos-probe/expected.v1.txt` | `f1216f2ba3f39bc46a8e5d079fdc189026685b67b69745ba28ecddf283807a1d` |
| `macos-probe/main.rs` | `624d8e02311e71173d04ddaea9a44a6a05428a30f8baa1d0a818643aa5360369` |

`macos-probe-execution` 是早期 `main.rs`（哈希 `b5da6111…`）的执行，运行以清理错误退出，不是当前源码证据；只有 execution-2 对应上表版本。两份探针日志分别为 `probe-logs/probes.log`（两个 std 反例）与 `macos-probe-execution-2/probe.stdout.log`（机制探针）。

### 构建与执行

环境：macOS 27.0.1 (26A434)；`rustc 1.98.1 (48a229cea 2026-09-01)`，host `aarch64-apple-darwin`（`system.stdout.log` 只记录 `sw_vers`，未记录 `uname -m`；arm64 是工具链 host 事实，不是独立硬件/内核记录）。构建根位于 `/System/Volumes/Data` 挂载下；`filesystem.stdout.log` 的 `df -P` 未记录文件系统类型，不能据此写成“APFS”。`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。

| 命令 | UTC 起止 | 退出码 |
|---|---|---:|
| `cargo generate-lockfile --offline` | 2026-10-31T04:29:07.283539Z–04:29:07.376229Z | 0 |
| `cargo build --locked --offline` | 2026-10-31T04:29:07.376737Z–04:29:08.444653Z | 0 |
| 探针可执行文件（隔离构建目录内） | 2026-10-31T04:29:08.445151Z–04:29:08.879476Z | 0 |

锁文件中的唯一第三方依赖是 `libc 0.2.191`，校验和 `65cd87c9a6d60536580b316ad985726104730a5151c99845f2490428ef91240a`；锁文件还包含探针自身包 `aline-macos-metadata-probe`。构建后隔离构建目录已删除，`build-cleanup.json` 的 `cleanup_error` 为 `null`。

### 实际观察结果

探针输出 `FLAGS metadata=7 data=8 state=null`，即固定使用 `COPYFILE_METADATA = COPYFILE_STAT | COPYFILE_ACL | COPYFILE_XATTR`，不含 `COPYFILE_DATA`。

| 样本 | 实际观察 | 调用计数 |
|---|---|---:|
| `metadata` | 源 mode `0640`、ACL `everyone deny write` / `allow read`、xattr `com.aline.probe`；目标调用前 mode `0600`、无 ACL。调用后目标 mode `0640`、ACL 与源一致、测试 xattr 已复制；源/目标正文分别保持 `SOURCE\n` / `TARGET\n` | 1 |
| `acl-only` | 源与目标 mode 均 `0600`、正文不同，源仅多出 ACL；调用后目标 ACL 与源一致，双方正文不变 | 2 |
| `same-inode` | 源与其硬链接的另一句柄同 inode；包装层在 unsafe 调用前返回 `SameInode`，调用计数保持 2，正文不变 | 2 |
| `non-regular` | 目标为目录的合法句柄；包装层在 unsafe 调用前返回 `NonRegular`，调用计数保持 2 | 2 |
| `readonly-target` | 目标以只读方式持有；源码结构保证传给 unsafe 的是仍存活的 `File` 借用，日志没有 fd 编号/生命周期记录，因此“未关闭/未悬空/未复用”是静态核对结论，不是运行时日志事实。本机观察为成功，mode/ACL/xattr 与源一致，正文不变 | 3 |
| `immutable-target` | 目标设置 `uchg`；系统调用失败，`kind=PermissionDenied`、`errno=1`（`EPERM`）；未发布成功回执，目标正文仍为 `IMMUTABLE-TARGET\n` | 4 |

`run()` 输出中倒数第二行为 `SUMMARY passed=6 unsafe_calls=4 production_admission=false`，整个进程最后一行是 `CLEANUP_ROOT removed=...`。`SUMMARY` 的 `passed=6` 是探针内部硬编码字面量，不是由 `expected.v1.txt` 或测试框架计算的独立 oracle；`unsafe_calls=4` 是调用前递增的尝试次数（成功 3 次：metadata、acl-only、readonly；另 1 次 immutable 目标 `EPERM`），不是“四次成功”。`CLEANUP_ROOT` 成功删除，未记录清理错误。

### 限制与未覆盖项

1. 这是 macOS 27.0.1、`aarch64-apple-darwin` 工具链、`/System/Volumes/Data` 挂载下的单机观察（`df` 日志未记录文件系统类型）；未覆盖 Intel Mac、Windows、Linux、网络卷、外接卷或其他文件系统。
2. `readonly-target` 成功只说明本机该系统调用可对只读 fd 执行元数据复制，不是跨平台或跨文件系统的保证；`immutable-target` 只形成 `EPERM` 失败样本，不构成完整权限错误矩阵。
3. 未测试 owner/group、setuid/setgid 等特殊模式位、ACL 继承标志、ACL allow/deny 变体、资源限制与跨进程并发。
4. 未测试父目录替换、临时路径被替换、发布前后路径竞争、跨进程 CAS、目录同步或断电保证；探针不替换用户文件，也不执行真实应用保存。
5. 测试用 ACL/xattr 与观测工具（`chmod`、`ls`、`xattr`）只用于隔离取证，不是拟议生产依赖；`COPYFILE_METADATA` 的系统语义仍由独立复核与后续最小实现验证。
6. 该成功**不是生产准入**：生产直接依赖、unsafe 例外、lint/守卫变更和现有文件覆盖能力均未获授权，4.4 生产实现仍未开始。

### 记录缺口与更正（独立复核后）

独立复核见 `review.md` 的“独立复核：execution-2 源码、预期与日志（2026-10-31）”。复核确认当前源码哈希与 execution-2 记录一致、单一 unsafe 调用点与调用前拒绝逻辑可在源码核对，但也指出以下记录缺口，本节已据此更正正文：

1. `expected.v1.txt` 未被探针或 runner 读取；`SUMMARY passed=6` 是探针内部硬编码字面量，不是独立 oracle。六项 CASE 与零退出支持“检查被走到”，expected 与实际的一致是人工逐项对照；4.4 目录未被 Git 跟踪，mtime 只能弱支持“预期先于实现”，不能证明写作时序。
2. execution-2 只覆盖清理成功路径；没有触发清理失败或 `run()` 返回 `Err` 的当前源码路径。`expected.v1.txt` 的“所有退出分支”应理解为可由程序正常/错误返回的分支，不包括 panic、abort 或 `SIGKILL`。
3. 没有执行前独立源码复核的可核验记录；`run-macos-probe.py` 的 `--execute` 是自述式开关，现有日志/报告未记录执行前对 `624d8e…` 源码的复核时间、复核者或范围。本次独立复核是执行后的只读核对，不能追溯补证。
4. runner 未记录自身 argv（含 `--output-dir`）、自身起止、完整环境、主机架构与文件系统类型，也未对复制后的构建输入或最终二进制再做哈希；“源码哈希 → 实际编译二进制”的绑定依赖构建根生命周期、离线日志与复制逻辑，不是逐字节的构建输入/输出证明。
5. 正文已把“APFS”“Apple arm64”“fd 未关闭/未悬空/未复用”“SUMMARY 为最后一行”降级/更正为日志或源码结构能够支持的表述；`unsafe_calls=4` 明确为尝试次数而非成功次数。
6. 关于“不能以拒绝普通含 ACL 的文件作为解决办法”：若采用 create-only 方案，拒绝的是**未经验证的现有目标替换操作**，不是按 ACL 存在与否拒绝普通文件。普通含 ACL 文件仍可打开、解码和编辑；M0 只是不提供现有文件覆盖保存。该区别不把拒绝包装成成功保存，也不改变 4.4 的能力边界。

复核的证据层面判断是：execution-2 成立为窄范围隔离机制证据，不支持 A（A 不依赖该探针），不足以支持 B 的生产准入，C 与现有证据相容；这些不是产品/排期裁决，A/B/C 仍由用户决定。

### 结论边界

本探针只证明在所记录的 macOS/APFS 环境中、对探针自持的两个不同 inode 普通文件，固定调用 `fcopyfile(COPYFILE_METADATA)` 可以观察到 ACL、mode 与测试 xattr 复制，并能在同 inode/非普通文件调用前拒绝、在不可变目标上受检失败且保留正文。它不证明路径级替换安全、跨平台能力、完整 metadata 等价或现有文件保存闭环；不得据此宣称 V-08、M0 出口或 4.4 已通过。

## Plan A 实现与执行-3（2026-10-31，追加）

### 裁决与实现范围

用户在 A/B/C 三案中裁决 **A：仅 create-only 原语**。本轮按 A 实现最小受检文件原语；未添加依赖，未修改 `unsafe_code`、lint 或安全守卫，未实现现有文件覆盖、ACL/xattr 保留、目录同步、断电保证、跨进程 CAS 或 Windows/Linux 行为。此前的 macOS 隔离探针授权仍是探针授权，未被转写为生产准入；本实现不依赖 `libc`、`fcopyfile` 或任何 `unsafe`。

实现文件与内容身份（工作树实际文件，`shasum -a 256`）：

| 文件 | SHA-256 |
|---|---|
| `crates/aline-source/src/atomic.rs` | `95053e1726c096f91969848887df8d074839e74afdc6d65a792f76ed8237f298` |
| `crates/aline-source/src/encode.rs` | `94dc73dd5b6a9b6320597a646bf7f7d2979aadc78cf7a83f1ee081f9038ebbf9` |
| `crates/aline-source/src/lib.rs` | `d9c6a66b9667334bbcabb3be1f90a27b0868503b8adb2a5da4cb0747995593e8` |
| `crates/aline-source/tests/atomic.rs` | `4918918a6638cd9e0222712a894eff0d2fa7e4e3504d95473b60376c514b5b7a` |
| `docs/validation/m0-engine-foundation/group-4/step-4.4/atomic-golden.v1.txt` | `d49465a4d18426fb5e28bf0c48192a20d0504f813257aabfe20f390bdb1b4015` |

`atomic-golden.v1.txt` 是实现前手写且本轮未修改的独立预期；其哈希只证明当前内容与复核记录一致，目录未纳入 Git，不能证明写作时序（见 F7）。

### 实际执行的步骤

**prepare（[`prepare_atomic`](../../../../../crates/aline-source/src/atomic.rs)）**

1. 先检查完整 encoded bytes 是否超过 `IoBudget`；超限不创建任何路径。
2. 对 target/staging 做词法前提检查：绝对路径、不相同、父目录相同；不 canonicalize（见 F8）。
3. 检查取消令牌；已取消时返回 `Cancelled { stage: Prepare }`。
4. 以 `OpenOptions::new().write(true).create_new(true)` 创建 staging；Unix 设置 mode `0600`。已存在路径返回 `Io { stage: Create }`，不覆盖、截断、删除或复用。
5. 以 64 KiB 块写入完整 bytes；每次写前检查取消；`Interrupted` 重试；短写推进偏移；`Ok(0)` 转为 `WriteZero`；偏移/长度溢出防御性失败。任一步失败都不触碰 target。
6. 完整写入后、`sync_all` 前再次检查取消；随后 `sync_all`。sync 失败不发布。
7. 失败结果在 staging 已创建且未观察到替换时携带 `Some(staging_path)`；target 在 prepare 阶段从未被创建、打开或改写。

**publish（[`PreparedAtomic::publish_new`](../../../../../crates/aline-source/src/atomic.rs)）**

1. 检查取消；取消返回 `Cancelled { stage: Publish }`，保留 staging。
2. 比较 staging 路径的 `symlink_metadata` 与 prepare 持有的 `File::metadata` 的 dev/ino；不一致返回 `StagingReplaced`。非 Unix 目标尚未验证该比较，当前恒拒绝（见 F4）。
3. `symlink_metadata(target)`：符号链接、目录、其他非普通文件分别拒绝；普通文件返回 `MetadataPreservationUnverified`；`NotFound` 才继续。所有拒绝都不修改目标。
4. 用 `std::fs::hard_link(staging, target)` 发布。`AlreadyExists` 转为 `Conflict { target_exists: true }`；其他 I/O 失败转为 `Io { stage: Publish }`。不 rename、不覆盖、不删除 staging。
5. 成功返回 `Written`，其中 `bytes()` 是本操作写入的 encoded bytes，`directory_durability_unverified()` 恒为 true；`into_baseline` 用这些 bytes 建立新基线，不重新读取目标（见 F3）。

**失败边界**

| 故障 | target | staging | 分类与路径 |
|---|---|---|---|
| 部分写入失败 | 不存在或保持 sentinel | 保留已写前缀 | `Io { Write }`，`Some(staging)` |
| 完整写入后 sync 失败 | 保持 sentinel | 保留完整 bytes | `Io { Sync }`，`Some(staging)` |
| prepare 阶段取消 | 不移除/改写 | 尚未创建或保留已写前缀 | `Cancelled { Prepare }`，按创建状态 |
| publish 前取消 | 保持 sentinel | 保留完整 bytes | `Cancelled { Publish }`，`Some(staging)` |
| staging 创建时已存在 | 保持 sentinel | 原文件保持 | `Io { Create }`，`None` |
| prepare 后 staging 被替换 | 保持原状态 | 替换者内容保持 | `StagingReplaced`，`None` |
| 目标普通文件已存在 | 保持原 bytes | 保留完整 bytes | `MetadataPreservationUnverified`，`Some(staging)` |
| 目标为符号链接/目录 | 保持原状态 | 保留完整 bytes | `ExistingTarget { kind }`，`Some(staging)` |
| 检查后目标竞争出现 | 竞争者的 bytes 保持 | 保留完整 bytes | `Conflict { target_exists: true }`，`Some(staging)` |

`MetadataPreservationUnverified` 是“拒绝未证明的现有文件替换”，不是 OS `Unsupported`，也不是现有文件保存成功；不得据此宣称 V-08 或 M0 出口。

### 实际命令与结果

环境：macOS 当前主机；`rustc 1.98.1`；`CARGO_HOME=/Users/xian/Library/Caches/aline-m0/cargo-home`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。以下均为实际执行，不是手工补写。

| 命令 | UTC 起止 | 退出码 | 证据 |
|---|---|---:|---|
| `cargo test -p aline-source atomic` | 2026-10-31T05:35:29.420976Z–05:35:29.515896Z | 0 | `targeted-test-execution-3/targeted-test.command.json`、stdout/stderr |
| `python3 scripts/check_workspace.py --cargo-home /Users/xian/Library/Caches/aline-m0/cargo-home --output-dir docs/validation/m0-engine-foundation/group-4/step-4.4/workspace-gates-execution-3` | 2026-10-31T05:35:05.861970Z–05:35:25.634078Z | 0（19 条记录全部 0） | `workspace-gates-execution-3/summary.json`、逐命令日志 |

定向测试日志先显示 `10 passed`（`atomic::tests`）和 `7 passed`（`tests/atomic.rs`）；随后四个 0-test 的测试二进制是 cargo 对同一过滤词在其他集成测试目标中的“0 passed / N filtered”，不是失败。定向测试不能替代工作区门禁；工作区门禁也不能替代 V-08 或跨平台验证。

`workspace-gates-execution-3/summary.json` 的 19 条记录包括 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、依赖、lint、safety、public-api 及其 self-test，全部 exit 0。`--all-targets` 只代表当前 macOS 主机和当前工具链，不代表其他 OS/target 或 CI 已执行。

`targeted-test/` 与 `workspace-gates/` 是执行-1，`*-execution-2` 是 F1 修复后的中间运行；最终引用执行-3。旧日志原样保留，未覆盖或删除。

### 测试覆盖矩阵

| 证据 | 执行方式 | 覆盖 |
|---|---|---|
| `atomic_golden_publish_new_and_existing_target_match_independent_expectation` | 真实文件系统；手写 3 case 金样 | `unchanged-lf`、`edited-crlf`、`bom-edit` 的 encoded bytes、新目标发布、Unix dev/ino 相同、`into_baseline` 存储 bytes、现有目标拒绝后目标 sentinel 与 staging 完整 bytes |
| `atomic_staging_creation_failure_preserves_target_and_existing_staging` | 真实文件系统 | staging 已存在时 `AlreadyExists`，target 与已有 staging 均不变 |
| `atomic_invalid_paths_budget_and_prepare_cancellation_create_nothing` | 真实文件系统 | 同路径、相对路径、不同目录、缺失父目录、预算、prepare 取消；不创建 target/staging |
| `atomic_pre_publish_cancellation_preserves_target_and_staging` | 真实文件系统 | publish 前取消保留 target 与完整 staging |
| `atomic_replaced_staging_path_is_rejected_without_touching_target` | 真实文件系统 | prepare 后替换 staging 路径；`StagingReplaced`，target 不存在，替换者内容保留 |
| `atomic_existing_symlink_and_directory_are_rejected_without_touching_target` | 真实文件系统 | 符号链接（Unix）与目录目标；sentinel 与链接/目录保持 |
| `atomic_large_source_crosses_write_chunks_and_publishes_exact_bytes` | 真实文件系统 | 70,000 字符正文跨 64 KiB 块，完整发布并核对 bytes |
| `atomic_existing_target_survives_staging_partial_write_failure` | 真实文件系统 + 注入 writer；完整 `prepare_atomic_with` | 目标 sentinel 预先存在；写入 3 字节后失败；target 不变、staging 前缀、`Io { Write }`、`Some(staging)`、sync 调用 0 次 |
| `atomic_existing_target_survives_staging_sync_failure_after_full_write` | 真实文件系统 + 注入 writer；完整 `prepare_atomic_with` | 目标 sentinel 预先存在；完整写入后 sync 失败；target 不变、staging 完整、`Io { Sync }`、`Some(staging)`、sync 调用 1 次 |
| `atomic_zero_write_is_rejected_as_write_zero_without_sync` | 假 writer | `Ok(0)` → `WriteZero`，sync 未被调用 |
| `atomic_partial_write_failure_reports_write_stage_without_sync`；`atomic_sync_failure_reports_sync_stage_after_full_write`；`atomic_write_cancellation_stops_before_next_write`；`atomic_interrupted_write_is_retried_without_losing_bytes`；`atomic_cancellation_after_complete_write_stops_before_sync`；`atomic_write_budget_rejects_before_any_write` | 假 writer | 写入/同步阶段、取消、`Interrupted` 重试、预算的单元级分类 |
| `atomic_target_race_after_check_is_conflict_and_preserves_racer_bytes` | 真实文件系统 + `publish_new_with_hook` | 目标检查后竞争出现；`Conflict`，竞争者 bytes 不被覆盖 |

F1 的闭合方式：新增的两个目标保全测试不是只调用 `write_staging`，而是调用生产 `prepare_atomic` 使用的同一私有 `prepare_atomic_with` 路径（预算、路径、取消、`create_new` 均实际执行），只通过闭包把 staging 的 `File` 包装成可注入写入/同步失败的 writer。断言同时覆盖失败阶段、`staging_path() == Some(...)`、target sentinel 不变、staging 前缀/完整 bytes 与 sync 调用次数。边界：这两个测试直接调用私有 `prepare_atomic_with`，不是公开 `prepare_atomic` 的端到端故障注入；公开入口由集成测试覆盖成功发布与现有目标拒绝。测试只比较 target 字节，不比较 mode、mtime、xattr 或文件类型，因此能发现改写 target 内容或删除 target 的回归，但不能发现只读探测或仅改 metadata 的重构；当前 prepare 对 target 没有任何文件操作，target 只进入词法 `validate_paths`。

F2 的闭合方式：本节补上实现后的源文件/金样哈希、实际命令与 UTC/退出码、17 个测试的名称与覆盖类型、真实 FS/假 writer/静态未执行的分界；`group-4/atomic-plan.md` 的状态行也同步更新，不再把“未实现/未执行”作为当前事实。

### 已知限制与未覆盖项

1. **F3：身份检查后的原地改写。** publish 只比较 staging 路径 metadata 与已打开句柄的 dev/ino；检查后外部进程仍可原地改写同一 inode，`hard_link` 会发布被改写的内容，而 `Written::bytes()`/`into_baseline` 报告本操作写入的 bytes。M0 不做发布后回读或 fd 级链接；文档已收紧，不再声称“与目标当前内容一致”。
2. **F4：非 Unix 身份比较。** `#[cfg(not(unix))] same_file_identity` 恒为 false，因此非 Unix 目标会在 publish 返回 `StagingReplaced` 且不携带 staging 路径；M0 只在 macOS 验证，Windows/Linux 未验证。若未来要宣称这些平台支持，需要显式的 `PlatformUnverified`/`Unsupported` 分类并保留 staging 路径，而不是复用本错误。
3. **F5：标准错误链。** 为避免安全守卫把 trait object 的 `'static` 词法误判为可变静态，`AtomicFailure`/`BaselineError` 未覆写 `Error::source`；原始错误仍由公开枚举变体持有，但不进入标准错误链。守卫修复后可恢复。
4. **F6：Debug 内容。** `PreparedAtomic`、`Written`、`EncodedBytes` 已改为自定义 `Debug`，只显示路径/长度/`reused_original`，不打印文档 bytes；`AtomicFailure` 的 Debug 只含分类与路径，不含正文。
5. **F7：金样历史。** `atomic-golden.v1.txt` 未纳入 Git，哈希与 mtime 只能弱支持“实现前手写”；本轮未改写金样，测试从文件读取而非从实现输出复制。
6. **F8：路径词法比较。** 同目录的等价别名（例如不同拼写或符号链接父目录）可能被 `DifferentDirectory`/`MissingParent` 拒绝；这是保守拒绝，不是路径解析或跨目录能力。
7. **F9：staging 路径可能已不存在。** 失败结果携带的是调用方提供的路径；外部删除后，`staging_path()` 仍可能返回该路径。M0 不重新验证存在性，也不在 Drop 中清理。
8. **F10：零写入与 sync 计数。** 新增 `ZeroWriter` 测试和真实文件 writer 的 sync 计数断言；旧假 writer 测试仍只检查阶段/bytes，不单独计数 sync。
9. **未测目标类型与边界。** 悬空符号链接、FIFO/设备/套接字（`ExistingTargetKind::Other`）、publish 阶段除 `AlreadyExists` 外的 I/O 失败、空 bytes、恰好 64 KiB 边界未单独执行；这些只由代码路径或静态检查覆盖。
10. **F12：测试目录。** 临时目录未显式设为 `0700`，Drop 清理错误被忽略；目录位于系统临时目录，测试不写用户文档。
11. **未实现/未验证的完整能力。** 目录 `fsync`/断电、ACL/xattr/owner/group/setuid、权限复制、跨进程 CAS、保存队列/外部变化协调、Windows/Linux、网络/外接卷、真实应用保存闭环均未验证；`MetadataPreservationUnverified` 明确拒绝现有普通文件覆盖。V-01～V-09 与 M0 出口不因本最小原语或本地门禁而通过。

### 记录边界

本节只支持以下结论：在当前 macOS 主机和锁定的 Rust 1.98.1 上，A 方案的 create-only 原语能完整写入并在新目标上发布，能在 staging 创建、部分写入、sync、取消、替换、现有目标与发布竞争等边界保全目标；17 个定向测试和当前工作区门禁实际通过。它不支持“现有文件保存已支持”“V-08 已通过”“M0 出口已通过”“跨平台/断电安全”或“ACL/xattr 已保留”。`tasks.md` 是否勾选，须待独立复核接受 F1/F2 后由任务所有者决定；本报告不代写 review 结论。
