# 4.4 反例独立只读复核

2026-10-31 独立 reviewer 检查 `fixtures/`、`check-std-probes.py`、`probe-logs/probes.log` 与锁定 Rust 1.98.1 的标准库实现，确认两例足以阻止将上述两种直接候选当作安全实现：仅复制标准权限位会丢失测试 ACL；先创建临时路径再用 `fs::copy` 重新按路径打开，在所模拟的符号链接替换竞争下会截断原目标。后者不满足原目标保全。

复核明确限制：任意 xattr 全量保留不是 PRD 单独明定的门禁；测试标记属性的丢失只作为标准权限复制范围的观测。两个反例不证明所有 safe std 或外部安全封装都不存在。尚无 4.4 实现可作完成性评审；真实权限/平台后端、故障与成功回执仍须独立复核。

后续独立复核 `backend-evaluation.md`：核对两份原包 SHA-256、公开 API、许可证声明与相关源码，并确认未将依赖或 unsafe 提案表述为已批准。复核提醒 `COPYFILE_SECURITY` 不含 XATTR，特殊权限位默认行为和源/临时句柄不能同 inode 均需后续具体证明；这些限制已补入候选评估。此复核是候选初筛，不构成对任何实现的安全批准。

对 `macos-exception-proposal.md` 的独立初审认为它是未批准草案，尚不足以直接裁决实施：具体 flags、元数据政策、模块/调用点和有效 lint 配置仍待明确。裁决前静态审查与须先授权的自有 unsafe 代码/执行探针需分开；目标复验还须覆盖仅权限或 ACL 变化。以上已回写提案。初审不是 unsafe 安全批准。

对 `macos-probe-proposal.md` 的独立安全初审认为可作为隔离实验的具体授权对象，但不是生产例外：应限定独占 0700 临时根、记录清理失败；无效目标仅在调用前拒绝或使用仍持有的合法句柄，不构造悬空/复用 fd；一个固定 unsafe 调用点可按样本重复执行。上述条件已回写。实际实验前仍须取得用户明确授权。

上述探针提案经补清后，用户明确回复“准你完成”，授权该隔离实验；此授权不扩展为生产 unsafe 例外或项目依赖准入。实验源码仍须在执行前独立复核。

---

## 独立复核：execution-2 源码、预期与日志（2026-10-31）

### 复核者身份与独立性

本次复核由独立子代理执行，未参与 `macos-probe/main.rs`、`macos-probe/Cargo.toml`、`macos-probe/expected.v1.txt` 的编写，未执行 execution-1/execution-2，也未参与 `report.md` 的撰写。以下结论基于当前工作树中的源码、预期、执行日志、执行脚本和仓库规则；不以 `report.md` 的叙述作为事实前提。复核过程只读：未运行探针，未运行 `cargo build`/`cargo run`，未修改源码、依赖、lint 或守卫，未执行 `git commit`。

本次复核能确认的是任务说明所述的授权范围；仓库内没有用户原始对话，无法独立核对 `report.md` 第 30 行“用户明确回复”的原文。授权仅覆盖 `macos-probe-proposal.md` 所述隔离实验，这一点与任务说明一致，但不构成生产 `unsafe`、直接 `libc` 依赖、lint/守卫例外或既有文件覆盖授权。

### 复核范围

已独立读取或核对的材料：

- `docs/validation/m0-engine-foundation/group-4/step-4.4/macos-probe/{main.rs,Cargo.toml,expected.v1.txt}`
- `macos-probe-execution-2/{source-hashes.json,probe.stdout.log,probe.stderr.log,Cargo.lock,*.command.json,*.stdout.log,*.stderr.log,build-cleanup.json}`
- `macos-probe-execution/{source-hashes.json,probe.stdout.log,probe.stderr.log,probe.command.json,build-cleanup.json}`
- `run-macos-probe.py`、`macos-probe-proposal.md`、`report.md`、本 `review.md`
- `AGENTS.md`、`docs/rust-style-guide.md`、`openspec/CODE_STANDARDS.md`、根 `Cargo.toml`
- `scripts/check_safety.py`、`scripts/check_lints.py`
- `docs/v1-validation-plan.md` 的 V-08 条目、`openspec/changes/m0-engine-foundation/tasks.md` 的 4.4 条目
- Cargo 缓存中的 `libc 0.2.191` Apple 源码（常量与 `fcopyfile` 签名）

按仓库实际章节，`docs/rust-style-guide.md` 的 unsafe 治理在 §10（RS-04/S-10-01）；§15 是依赖准入。两者均与本次判断有关，不能把 §15 误写成 unsafe 治理章节。

### 实际执行的只读命令与关键结果

#### 1. 源码身份与历史

```text
$ cd <repo>/docs/validation/m0-engine-foundation/group-4/step-4.4/macos-probe
$ shasum -a 256 Cargo.toml expected.v1.txt main.rs
9bbcfd0663ce735f60f06eba544f0679da882751598b0db9d42e93b087d4b10d  Cargo.toml
f1216f2ba3f39bc46a8e5d079fdc189026685b67b69745ba28ecddf283807a1d  expected.v1.txt
bdc374c7b03bd649c2420dcec2eb6d246267b873e0de1765e79fb9c5dcf0262e  main.rs
# exit 0
```

这三个值与 execution-2 的 `source-hashes.json` 完全一致，因此 execution-2 的源码身份与当前工作树一致。execution-1 的 `main.rs` 哈希为 `b5da61113c9f5d76c3eb43bbdd58d5a640e258e20fc1f7f90ea941f48a7233c7`，与当前源码不同；execution-1 的 `Cargo.toml`、`expected.v1.txt` 哈希与 execution-2 相同。

```text
$ git status --porcelain=v1 -- docs/validation/m0-engine-foundation/group-4/step-4.4
?? docs/validation/m0-engine-foundation/group-4/step-4.4/
$ git ls-files -- docs/validation/m0-engine-foundation/group-4/step-4.4
# 无输出
$ git log --all --oneline -- docs/validation/m0-engine-foundation/group-4/step-4.4
# 无输出
```

整个 4.4 目录未被 Git 跟踪，没有历史可证明 expected 与实现的先后顺序。`stat` 只能给出弱时间线索：`expected.v1.txt` 的 mtime 为 12:27:59，`main.rs` 为 12:29:07；execution-1 的 `source-hashes.json` 为 12:28:46，execution-2 为 12:29:07。该顺序与“expected 先写、再实现/执行”相容，但 mtime 可被修改，且 expected 也可能在更早的实现之后写成；不能证明“先于探针实现手写”。

#### 2. unsafe 边界

```text
$ grep -n 'unsafe' main.rs
39:    let result = unsafe {
$ grep -n 'fcopyfile' main.rs
40:        libc::fcopyfile(
$ grep -n 'copy_metadata(' main.rs
156:    match copy_metadata(source, target, calls) {
243:        copy_metadata(&source, &alias, &mut calls),
256:        copy_metadata(&source, &directory, &mut calls),
268:    match copy_metadata(&source, &readonly_target, &mut calls) {
296:    match copy_metadata(&source, &immutable, &mut calls) {
```

逐项核对源码后确认：

- 只有一个 `unsafe` 块（第 39–46 行）和一个 `libc::fcopyfile` 文本调用点（第 40 行）；没有 `from_raw_fd`、`as_raw_fd` 之外的自建 fd，也没有第二个 unsafe 入口。
- `state` 固定为 `std::ptr::null_mut()`（第 43 行），`flags` 固定为 `libc::COPYFILE_METADATA`（第 44 行）；不接收外部 `state`、flags、路径或 fd 参数。
- 两个 fd 均来自函数参数 `&File`，函数签名只接受借用句柄；`copy_metadata` 没有路径参数，调用方也没有把外部路径传进来。`main.rs` 的五个调用点都在探针私有根内的自有文件/目录句柄上。
- 调用前检查 `source.metadata()`、`target.metadata()` 均成功，且两者都是普通文件（`is_file()`）；随后检查 `dev()`/`ino()` 相同则返回 `SameInode`。这两个拒绝发生在计数递增和 unsafe 调用之前（第 25–35 行）。
- `calls` 在调用前用 `checked_add` 递增，溢出返回 `CallCounterOverflow`；返回 `0` 映射为 `Ok(())`，非 `0` 用 `io::Error::last_os_error()` 映射为 `CopyError::System`。没有把未知错误吞掉或伪装成成功。
- `readonly-target` 在 `drop(readonly_writable)` 后重新 `File::open` 得到只读句柄；传给 unsafe 的是仍存活的 `readonly_target`，不是已关闭的写句柄。源码层面没有悬空 fd 或裸 fd 复用。日志没有记录 fd 编号/生命周期，因此“fd 未关闭、未悬空、未复用”只能由源码结构确认，不能由 execution-2 日志运行时证明。另需注意，写句柄被显式关闭后，操作系统可能把同一数字 fd 分配给新打开的只读句柄；这不是把已关闭的 fd 交给 unsafe，但“未复用”若被理解为数字 fd 永不复用则无法从本探针确认。
- `libc::COPYFILE_METADATA` 在缓存源码中的定义为 `COPYFILE_SECURITY | COPYFILE_XATTR`，而 `COPYFILE_SECURITY = COPYFILE_STAT | COPYFILE_ACL`，数值为 7；`COPYFILE_DATA = 1 << 3 = 8`。`fcopyfile` 签名为 `(c_int, c_int, copyfile_state_t, copyfile_flags_t) -> c_int`，其中 `copyfile_state_t = *mut c_void`。参数类型与调用方式一致。运行输出 `FLAGS metadata=7 data=8 state=null` 且检查 `metadata & data == 0`，说明本机常量符合预期；这只是本机缓存头文件与运行观察，不是跨平台语义证明。

```text
$ python3 scripts/check_safety.py --file docs/validation/m0-engine-foundation/group-4/step-4.4/macos-probe/main.rs --kind ui-production
FAIL  docs/.../macos-probe/main.rs:39 unsafe-owned ...
exit=1
```

这项只读检查说明：若探针被当作生产源文件，仓库 `unsafe-owned` 规则会拒绝第 39 行的 unsafe。探针是独立 `[workspace]` 包、不是根工作区成员，根 `Cargo.toml` 仍为 `unsafe_code = "forbid"`，探针的 `Cargo.toml` 没有 `[lints]`，因此探针既没有获得生产 lint 继承，也没有获得守卫例外。`scripts/check_lints.py` 的源码也显示其检查根工作区及成员继承；本次未运行该脚本，只核对了规则和清单。

#### 3. expected.v1.txt 与实际输出逐项对照

execution-2 的 `probe.stdout.log` 有 19 行、2336 字节，`probe.stderr.log` 为 0 字节；六个 `CASE` 行分别在第 6、10、11、12、14、16 行，`SUMMARY` 在第 18 行，`CLEANUP_ROOT` 在第 19 行。逐项实际值如下：

| expected 条目 | execution-2 实际观察 | 结论 |
|---|---|---|
| `metadata`：source `SOURCE\n`、target `TARGET\n`、source mode `0640`、ACL deny write/allow read、xattr `com.aline.probe=aline-preserve`；预期 `fcopyfile(null,COPYFILE_METADATA)` 成功，target 对应 mode/ACL/xattr 相同；不含 DATA | source mode `640`，source ACL 含 `group:everyone deny write` 与 `group:everyone allow read`；target 调用前 mode `600`、无 ACL；调用后 target mode `640`、ACL 与 source 相同、target 的 `com.aline.probe` 值为 `aline-preserve`；source/target 正文仍为 `SOURCE\n`/`TARGET\n`；调用计数 1 | 符合。source xattr 的值没有在 FACT 中回显，但 `xattr -w` 成功且 target 值被 `xattr -p` 核对。 |
| `acl-only`：source/target 均 `0600`，正文不同，source 仅多 ACL；预期成功、target ACL 匹配 source、双方正文不变 | 两者 mode `600`，正文不同；source 仅多 ACL；调用后 target ACL 与 source 相同，双方正文不变；调用计数 2 | 符合。 |
| `same-inode`：source 与 target 为同一文件的不同持有句柄（另有不同路径硬链接）；预期调用前拒绝、计数不变、正文不变 | 包装层返回 `SameInode`，调用计数保持 2，source 正文仍为 `SOURCE\n`；使用 `source-alias.md` 硬链接另开句柄 | 符合。 |
| `non-regular`：target 为探针根内目录的合法持有句柄；预期调用前拒绝、计数不变 | 包装层返回 `NonRegular`，调用计数保持 2 | 符合。 |
| `readonly-target`：target 普通文件仅以只读方式持有，fd 不关闭/不悬空/不复用；不预设必须失败；成功则核对 mode/ACL/xattr | 本机观察为成功；target mode `640`、ACL 与 source 相同、`com.aline.probe` 值 `aline-preserve`、正文仍为 `READONLY-TARGET\n`；调用计数 3 | 符合“允许成功”的分支。该成功不是失败覆盖，也不能外推为跨平台保证。 |
| `immutable-target`：私有根内自有目标设 `uchg`；预期系统调用受检失败并保留 errno、不得发布成功回执、正文仍为 `IMMUTABLE-TARGET\n` | `kind=PermissionDenied errno=Some(1)`（`EPERM`），输出 `CASE immutable-target system-failure`，target 正文仍为 `IMMUTABLE-TARGET\n`；调用计数 4 | 符合。 |
| `cleanup`：所有退出分支先 `nouchg`、再清 ACL、再删独占临时根；清理错误单独输出并导致非零退出 | execution-2 没有清理错误，`CLEANUP_ROOT` 成功；`build-cleanup.json` 的 `cleanup_error` 为 `null`；探针退出 0 | 只验证了成功清理路径。失败路径是否执行/报告须看源码；execution-2 没有触发 `run()` 返回 Err 或清理失败，不能用该日志证明“所有退出分支”。 |

源码层面，`main()` 先保存 `run(&root)` 的结果，再依次执行 `nouchg`、对存在文件 `chmod -N`、`remove_dir_all`；每项失败打印 `CLEANUP_*_ERROR` 并置 `cleanup_failed`，最终有清理错误时返回 Err。源码中的 `run()` 位于 `main` 内并始终在清理前执行，因此对可返回的 `Result` 错误路径，清理逻辑会执行。`panic`、进程 abort、`SIGKILL`、stdout/stderr 写失败导致的 panic 不在该保证内；`expected.v1.txt` 的“所有退出分支”应理解为“所有可由本程序正常返回/错误返回的分支”，不能理解为包括被强杀或 panic 的所有进程退出。这个界限适用于源码，execution-2 也没有覆盖它。

expected 与实际的对照有一处重要证据限制：`main.rs` 和 `run-macos-probe.py` 均未读取或解析 `expected.v1.txt`，`grep -rn 'expected.v1' macos-probe run-macos-probe.py` 无匹配（exit 1）；runner 只把 expected 作为 `SOURCE.iterdir()` 的一个文件计算哈希，探针构建目录也只复制 `Cargo.toml` 和 `main.rs`。因此 `SUMMARY passed=6` 不是由 expected 驱动的独立判定，而是探针内部断言全部走完后的硬编码字符串（`passed=6` 为字面量；`unsafe_calls` 为计数器）。六个 CASE 行的存在和零退出支持“六项检查确实被走到”，但 expected 不是自动化 oracle；本次对照是人工核对，符合项不能表述为“expected 自动校验通过”。

expected 的内容没有出现 execution-2 输出特有的 `CASE` 名称、`errno=1` 数值或调用计数，且允许 readonly 两种结果，这与“先写预期、不从输出回录”相容；但这不是时序证明。结论只能是：内容上未见回录痕迹，mtime 与内容顺序相容，Git 历史/文件系统无法证明；expected 可作为人工预期，不能作为机械独立门禁。

#### 4. 构建、执行与清理记录

从 `*.command.json` 读取的关键记录：

```text
rustc-version  exit=0
cargo-version  exit=0
system         exit=0
filesystem     exit=0
lock           exit=0  2026-10-31T04:29:07.283539Z–04:29:07.376229Z
build          exit=0  2026-10-31T04:29:07.376737Z–04:29:08.444653Z
probe          exit=0  2026-10-31T04:29:08.445151Z–04:29:08.879476Z
build-cleanup  ended 2026-10-31T04:29:08.891608Z, cleanup_error=null
```

- 构建 stderr 只有 `Compiling libc v0.2.191`、`Compiling aline-macos-metadata-probe`、`Finished dev profile ... in 1.05s`，无 warning；构建 stdout 为 0 字节。锁命令为 `cargo generate-lockfile --offline`，构建为 `cargo build --locked --offline`，环境含 `CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。
- `Cargo.lock` 中只有探针包与 `libc 0.2.191`（唯一第三方依赖）；libc 校验和为 `65cd87c9a6d60536580b316ad985726104730a5151c99845f2490428ef91240a`，与缓存 `.crate` 的 SHA-256 一致。构建日志没有下载/联网失败记录。
- execution-2 源码哈希与当前文件一致；execution-1 的 `probe.command.json` 退出码为 1，`probe.stdout.log` 只有 5 个 `CASE` 而无 `SUMMARY`，`probe.stderr.log` 有 `CLEANUP_FLAGS_ERROR .../immutable.md No such file or directory`。这与 `report.md` 所说“execution-1 以清理错误退出”一致；因 `main.rs` 哈希不同，execution-1 不是当前源码证据。
- 执行后 temp 根检查：execution-1 的 `aline-44-probe-build-t906prq6`、`aline-macos-metadata-65982-1793420930024383000-0` 均不存在；以 `aline-44-probe-build-*`、`aline-macos-metadata-*` 为前缀的 `find` 计数为 0。execution-2 的 build/probe 根同样无残留。构建根和探针根的清理记录与文件系统观察一致。

### 发现的不一致、缺口或过度解读

1. **“最后一行输出为 SUMMARY”不准确。** `report.md` 第 65 行称 SUMMARY 为最后一行；实际 `probe.stdout.log` 第 18 行是 `SUMMARY passed=6 unsafe_calls=4 production_admission=false`，第 19 行是 `CLEANUP_ROOT removed=...`。这不影响“探针退出 0、清理成功”的结论，但该表述应按日志更正。就 `run()` 函数范围而言 SUMMARY 确实是其最后一行，但报告没有限定这个范围，读者会把它理解为整个进程的最后输出。
2. **“APFS Data 卷”没有直接日志证据。** `filesystem.stdout.log` 的 `df -P` 只显示 `Filesystem /dev/disk3s5` 与挂载点 `<system-volume>/Data`，没有 `apfs` 字样；报告把常见的 macOS Data 卷推断写成事实。更准确的表述是“构建根位于 `<system-volume>/Data` 挂载下，日志未记录文件系统类型”。这不改变探针结果，但影响跨文件系统结论的证据强度。
3. **“Apple arm64”是推断而非直接系统记录。** `system.stdout.log` 只有 `sw_vers`，没有 `uname -m`/`arch`；arm64 来自 `rustc -Vv` 的 host `aarch64-apple-darwin`。这通常足以说明工具链运行在 arm64，但“Apple arm64 主机”本身没有独立硬件/内核架构记录。报告应写成“rustc host 为 aarch64-apple-darwin”，或补充 `uname -m`。
4. **“fd 未关闭、未悬空、未复用”是源码结构结论，不是日志观察。** execution-2 没有 fd 编号或生命周期记录；只读目标句柄在调用期间由 Rust 借用持有，源码没有裸 fd 重建，但日志不能独立证明该运行时属性。报告第 62 行应标明“由源码保证/静态核对”，而不是与 FACT 同等的事实。
5. **`unsafe_calls=4` 是尝试计数，不是成功调用数。** 计数在 unsafe 调用之前递增，`immutable-target` 的 `EPERM` 也计入。实际成功调用为 3 次（metadata、acl-only、readonly），包装层还提前拒绝了同 inode 和非普通文件两次。报告的表格用“调用计数”尚可，但若被读成“四次成功 fcopyfile”则是过度解读。
6. **`SUMMARY passed=6` 是硬编码字符串，不是动态计数。** 六个 CASE 行和零退出支持六项检查被走到，但 `passed=6` 不是由 expected 或测试框架计算的；如果将来修改代码而遗漏某项检查，字面量不会自动变化。当前源码路径使这种风险较低，但证据级别应写清。
7. **没有执行前独立源码复核的记录。** `macos-probe-proposal.md` 和 `run-macos-probe.py` 都要求先完成独立安全复核，runner 的 `--execute` 只是一个自述式开关；execution-2 的日志、`report.md` 和现有 `review.md` 都没有记录“执行前对 `624d8e...` 源码做过独立复核”的时间、复核者或范围。现有 `review.md` 第 11、13 行只是对提案和授权的前置意见，不是对最终源码哈希的执行前复核。因此本复核能确认“execution-2 的源码现在可以被独立复核”，但不能确认“执行前已经完成过该复核”。这是治理记录缺口，不是探针技术结果的矛盾。
8. **expected 不是执行链的独立 oracle。** 如第 3 节所述，expected 未被探针或 runner 读取；人工对照一致不等于自动门禁通过。它的“先于实现手写”也无法由 Git 历史证明，只能由 mtime（可修改）和内容（未见回录特征）弱支持。
9. **执行脚本的记录范围有缺口。** `run-macos-probe.py` 会记录每条子命令的 cwd、started/ended UTC、退出码、三个环境变量及 stdout/stderr，并在 `finally` 清理构建根、记录 `cleanup_error`；这些都已核对。但它没有记录自身 argv（包括 `--output-dir`）、自身起止时间、完整环境、主机架构/文件系统类型，也没有比较 `expected.v1.txt`；`source-hashes.json` 是脚本对目录文件的哈希，没有对复制后的构建输入或最终二进制再做哈希。因此“源码哈希 → 实际编译二进制”的绑定主要依赖构建根生命周期、离线日志和复制逻辑，不是逐字节的构建输入/输出证明。默认 `--output-dir` 会覆盖同名目录；execution-2 使用了独立目录名，但实际 runner 调用参数未记录。
10. **execution-2 没有覆盖清理失败或 `run()` 返回 Err 的日志路径。** 源码显示这些路径会尝试清理并报告错误，但只有 execution-1（不同源码）出现过清理错误；execution-2 的清理成功不能证明当前源码清理失败分支的实际行为。`expected.v1.txt` 的“所有退出分支”若按字面涵盖 panic/abort/SIGKILL，也超出源码保证。
11. **报告第 50 行“锁文件只解析 libc 0.2.191”严格说省略了探针自身包。** Cargo.lock 还包含 `aline-macos-metadata-probe`；作为“唯一第三方依赖”表述是准确的，作为“文件里只有 libc”则不完全准确。此点不影响依赖准入判断。
12. **“用户明确回复”无法从仓库独立核验。** 任务说明和现有 review 的转述支持授权存在；但仓库没有原始对话。本复核按任务说明接受授权范围，不把 `report.md` 的转述当成额外证据。

### 证据层面：A/B/C 的适配性

以下只做证据适配性评估，不替产品做方案裁决。

#### A（仅 create-only 检查原语的替换，含 staging、完整写入 + `sync_all`、`hard_link` 到新目标，已存在目标拒绝）

- execution-2 的探针不使用 `create_new` 目标发布、`hard_link`、`sync_all` 目标目录或 `EEXIST` 冲突路径，因此不能为 A 提供直接验证。A 不依赖本探针，execution-2 既不验证也不否定 A。
- 本探针的 `new_file` 确实使用 `create_new(true)`、写入后 `sync_all`，但这只是隔离样本文件的创建，不是“staging → 同目录 hard_link → 目标不存在”的发布语义，也没有测试目标已存在、硬链接失败、目录同步、断电或并发冲突。不能把 `new_file` 的行为外推为 A 的替换原语。
- A 的证据适配性取决于它自己的失败/冲突/持久化测试，以及产品是否接受“既有目标不覆盖”的语义边界；这不是本探针能回答的。A 的“现有文件保存”能力也不能由本探针支持。

#### B（生产最小 unsafe 例外：直接 `libc` + 句柄级 `fcopyfile(COPYFILE_METADATA)`，再加上依赖/lint/守卫例外与独立安全评审）

- execution-2 对 B 的支持仅限于窄机制可行性：在本机、本文件系统、两个由探针持有的不同 inode 普通文件句柄上，`fcopyfile` 固定 `COPYFILE_METADATA`、`null` state 可观察到 mode/ACL/测试 xattr 复制，且不复制正文；调用前能拒绝同 inode/非普通文件；不可变目标形成 `EPERM` 且正文不变；清理成功。这与 B 的技术前提相关。
- execution-2 不足以支持 B 的生产准入。至少缺少：`libc` 直接依赖的正式准入；根工作区 `unsafe_code = "forbid"` 的 lint/守卫例外审批；生产代码（而非独立探针）的独立安全评审；生产调用点的错误分类、资源限制和 fd 生命周期设计；owner/group、setuid/setgid/sticky、时间戳、flags、ACL 继承/变体、xattr 策略的完整覆盖；路径级替换、符号链接/目录替换、跨进程 CAS、目录同步/断电；Windows/Linux 或 Intel Mac；真实保存流程和成功回执；完整失败矩阵与跨文件系统行为。`check_safety.py --file` 对探针报 `unsafe-owned` 也说明探针不是“已获生产守卫例外”的证明。
- 因此，execution-2 可作为 B 的“机制可行性数据点/后续最小实现输入”，不能写成“B 已获生产准入”或“生产 unsafe 已批准”。报告第 74 行的“不是生产准入”与这一判断一致。

#### C（暂停 4.4，先推进第 5–8 组，把 4.4 留给 V-08）

- `tasks.md` 的 4.4 仍未勾选；没有生产 `aline-source` 原子写实现，也没有 `cargo test -p aline-source atomic` 结果。execution-2 是独立探针，不是 4.4 的测试，不改变这一事实。C 的前提“4.4 尚未完成、可暂缓”与当前证据相符。
- C 的选择本身是范围/排期决定，不需要本探针证明；execution-2 既不验证也不否定 C。若选择 C，应保留探针作为未来 V-08/M2 的隔离输入，但不能把它记作 4.4 完成或 V-08 通过。C 没有来自本探针的技术阻断项。
- 反过来，若选择 A 或 B，也不能把“执行-2 成功”当成 4.4/V-08 已完成；两者都需要各自的生产实现、测试与评审。

### 观察、支持与明确不支持

**本探针实际观察到的：**

- 在本机 execution-2 环境、私有 0700 临时根内，对探针自持的两个不同 inode 普通文件，固定 `fcopyfile`（`null` state、`COPYFILE_METADATA`）后，测试样本的 mode、ACL 与 `com.aline.probe` xattr 与源一致，正文未被复制/改变。
- 同 inode 与目录句柄在 unsafe 调用前被包装层以 `SameInode`/`NonRegular` 拒绝，调用计数不变。
- 只读目标句柄在本机成功；不可变目标产生 `PermissionDenied`/`EPERM`，目标正文保持。
- 六个 CASE 全部出现，进程退出 0，构建/锁/探针命令退出 0，清理根成功且无构建根残留。

**这些观察支持的：**

- 仅限 macOS 单机、该日志环境、该样本集上的“句柄级 `fcopyfile(COPYFILE_METADATA)` 机制可行”，以及包装层在调用前拒绝同 inode/非普通文件的实现方式。
- 可作为 B 的机制可行性输入，或作为后续最小实现和 V-08 测试设计的参考；不能作为生产准入证据。
- 可作为 A 无关的背景对照，但不构成对 A 的实现验证。

**明确不支持的：**

- 不证明生产 `unsafe` 例外、直接 `libc` 依赖、lint/守卫例外或独立安全评审已获批准。
- 不证明路径级替换安全、临时路径/符号链接/父目录替换安全、跨进程 CAS、目录同步或断电/崩溃恢复；不证明现有文件保存闭环。
- 不证明跨平台、跨文件系统、Intel Mac、Windows、Linux、网络卷或外接卷行为。
- 不证明完整 metadata 等价：未覆盖 owner/group、setuid/setgid/sticky、时间戳、文件 flags、ACL 继承/变体、xattr 全量策略、资源限制与并发。
- 不证明 V-08、M0 出口或 4.4 完成；`tasks.md` 4.4 未勾选，且没有 `cargo test -p aline-source atomic`。
- 不证明失败矩阵完整；`readonly-target` 的成功只说明“允许成功”的一个分支，`immutable-target` 只提供一个 `EPERM` 样本。

### 限制

- 本次复核是只读静态核对加日志核对，没有重新执行探针，也没有运行 `cargo build`，因此无法发现“日志生成时的二进制与当前源码不一致”这类只有在重新构建时才会暴露的问题；只能确认当前源码哈希与 execution-2 记录一致、构建输入复制逻辑与日志自洽。
- 未重新核对 `probe-logs/` 中两个 std 反例的全部日志；它们不是 execution-2 的证据，本复核仅用其既有结论作为 A 的背景，不把其重启为已验证事实。
- `expected.v1.txt` 的写入时序无法由 Git 历史或不可篡改元数据证明；本复核只做内容与 mtime 的弱判断。
- 执行前独立源码复核没有可核验记录；本复核不能追溯补证。
- 环境中的文件系统类型、硬件架构、完整环境变量和 runner 自身参数没有完整记录；报告中的 APFS/Apple arm64 应降级为推断。
- 探针只使用 macOS 工具（`chmod`、`ls`、`xattr`、`chflags`）作为观测/夹具，不证明这些工具或测试 ACL/xattr 是生产依赖；也不证明 `COPYFILE_METADATA` 在其它 macOS 版本、文件系统或目标类型上的语义。

### 最终结论

1. **探针作为“隔离机制证据”成立，但仅限窄范围。** execution-2 的当前源码哈希与记录一致，单个 unsafe 调用点、固定 `COPYFILE_METADATA`、`null` state、两个存活 `File` 句柄、调用前同 inode/非普通文件拒绝、调用计数和错误分类均在源码中可核；日志六项 CASE、零退出、离线构建与清理记录自洽。未发现足以否定该窄结论的技术矛盾。其证据级别应表述为“单机、单文件系统、单一样本集的机制可行性”，不是产品能力。
2. **execution-2 不支持 A。** A 与该探针的调用路径不同，探针没有测试 create-only 发布、hard_link、目标已存在拒绝、目录同步或失败保全；A 是独立的方案候选，需要自己的测试与产品边界决定。探针既不验证也不否定 A。
3. **execution-2 不足以支持 B 的生产准入。** 它最多支持 B 的机制可行性数据点；`libc` 依赖准入、生产 lint/守卫例外、生产代码独立安全评审、路径/竞争/持久化、完整 metadata、跨平台和失败矩阵都未具备。不得把“探针成功”写成“生产能力已准入”或“现有文件保存已支持”。
4. **C 与现有证据相容。** 4.4 未完成、`tasks.md` 未勾选、没有生产原子测试；本探针不关闭 4.4/V-08。选择 C 的证据前提成立，但这是产品/排期决定，不是本复核的裁决。
5. **阻断项分为两类：** 生产/B 的准入阻断项仍然存在（依赖、lint/守卫、安全评审、平台/覆盖范围、路径/持久化、现有文件保存等），不能由本探针解除；探针记录层面另有非致命缺口（执行前独立复核无记录、expected 未接入执行链、报告对“最后一行”的表述错误、APFS/arm64 由推断写成事实、清理失败路径未由 execution-2 覆盖、`SUMMARY passed=6` 为硬编码）。后者应更正/补记，后者不改变“窄机制证据成立”的判断；前者决定 B 和生产实现仍不能启动或宣称完成。
6. 本次复核没有修改 `tasks.md`、`report.md` 或其他文件，没有运行探针/构建，也没有 commit；只在本节追加独立复核结论。

---

## 独立复核：4.4 create-only 受检发布原语（2026-10-31，追加）

本节是对工作树中新增 `crates/aline-source/src/atomic.rs` 及其测试的追加复核。历史段落保留其当时的 scope（当时确无生产实现）；本节不把历史“尚无 4.4 实现”的结论改写为现在的事实，也不把实现者或 `report.md` 的叙述当作结论。

### 复核者身份与写入边界

本次复核由独立子代理执行，未参与 `atomic.rs`、`lib.rs`、`tests/atomic.rs`、金样或 `report.md` 的编写，未参与实现者的测试设计 Discussions，也未执行任何探针作为生产准入。复核期间未修改上述任何被审文件、`tasks.md`、Cargo 配置、lint/guard 脚本或 `.codegraph/`；未运行会改写文件的 `cargo fmt`（只运行 `cargo fmt --all --check`），未运行 `scripts/check_workspace.py`，未执行 `git commit`/`git push`。仓库内唯一写入是本节对 `review.md` 的追加；安全守卫输出和合成 `Error::source` 片段只在 `/tmp` 下生成并已删除，不在仓库内留下文件。

### 复核范围与材料

已独立读取或核对：

- `openspec/changes/m0-engine-foundation/tasks.md` 的 4.4 条目（第 42 行，仍为 `- [ ]`）
- `docs/validation/m0-engine-foundation/group-4/atomic-plan.md`（方案 A 的 prepare/publish/error/测试设计）
- `crates/aline-source/src/atomic.rs`、`crates/aline-source/src/lib.rs`、`crates/aline-source/tests/atomic.rs`
- `docs/validation/m0-engine-foundation/group-4/step-4.4/atomic-golden.v1.txt`
- `docs/validation/m0-engine-foundation/group-4/step-4.4/report.md`（含 execution-2 历史段落）与本 `review.md`
- `step-4.4/targeted-test/` 与 `step-4.4/workspace-gates/` 的 command JSON、stdout/stderr 日志和 `summary.json`
- `crates/aline-source/src/{encode,decode,storage,baseline}.rs`、`crates/aline-source/Cargo.toml`、根 `Cargo.toml`、`Cargo.lock` 的 aline-source 段
- `scripts/check_safety.py`、`scripts/check_lints.py` 的规则与 clippy/unsafe 配置
- `AGENTS.md`、`docs/rust-style-guide.md` 的错误/source 治理条款、`docs/v1-architecture.md` §7.3、`docs/architecture-guide.md` R-15-07、`docs/v1-validation-plan.md` V-08、`openspec/CODE_STANDARDS.md`

### 源码身份、工作树与时间证据

复核开始时的只读记录（本次复核自己的 `git`/`shasum`/`stat` 命令，均 exit 0）：

```text
$ git status --porcelain=v1
 M crates/aline-source/src/lib.rs
?? crates/aline-source/src/atomic.rs
?? crates/aline-source/tests/atomic.rs
?? docs/validation/m0-engine-foundation/group-4/step-4.4/

$ git diff --stat
 crates/aline-source/src/lib.rs | 5 +++++
 1 file changed, 5 insertions(+)

$ git ls-files -- docs/validation/m0-engine-foundation/group-4/step-4.4
# 无输出；整个 4.4 证据目录未被 Git 跟踪
```

| 文件（复核时内容） | SHA-256 |
|---|---|
| `crates/aline-source/src/atomic.rs` | `dd62e2e00c4f339d54cfc63cc110c5fffd0a4ea26019ac324a017274f154555f` |
| `crates/aline-source/src/lib.rs` | `d9c6a66b9667334bbcabb3be1f90a27b0868503b8adb2a5da4cb0747995593e8` |
| `crates/aline-source/tests/atomic.rs` | `4918918a6638cd9e0222712a894eff0d2fa7e4e3504d95473b60376c514b5b7a` |
| `step-4.4/atomic-golden.v1.txt` | `d49465a4d18426fb5e28bf0c48192a20d0504f813257aabfe20f390bdb1b4015` |
| `step-4.4/report.md` | `8f7e21c5d0dcfd3e4996e7133e2423d6f5b4bc0ad05d906d062e3b5f5f633b7d` |
| `step-4.4/review.md`（追加前） | `45187cbbfcb52c4c81d17056cc1b897a10ae2e6555b2ad04acc98880420299d8` |
| `step-4.4/targeted-test/targeted-test.command.json` | `30f0ec77a472729fced926a4a35b4095b21daac492fc57e7769c3934d1cd9d5f` |
| `step-4.4/workspace-gates/summary.json` | `c7914c0b63bd7a5d8f336ead161acba40070ed7e12ff62a068f1640793ea8498` |

文件系统 mtime（本地时区，仅作弱时间线索）：`atomic-golden.v1.txt` 13:14:09，`atomic.rs` 13:19:20，`tests/atomic.rs` 13:19:26，`report.md` 12:50:14，`review.md` 12:49:34。金样早于源码/测试与“实现前手写”相容，但目录未被 Git 跟踪，mtime 可改，不能作为时序证明。

实现仅改动 `lib.rs` 的模块声明与 re-export（`mod atomic;` 和 `pub use atomic::{...}`），没有改动其他已跟踪文件；`Cargo.toml`/`Cargo.lock` 没有新增 aline-source 依赖。`crates/aline-source/Cargo.toml` 的依赖为空；根工作区 lint 仍为 `unsafe_code = "forbid"`、clippy `unwrap_used`/`expect_used`/`panic`/`unreachable` 均为 deny。

### 本次实际执行的命令

以下命令均设置 `CARGO_HOME=<cargo-home>`、`RUSTUP_TOOLCHAIN=1.98.1`、`CARGO_NET_OFFLINE=true`。时间为本次复核的 UTC 记录：

| 命令 | UTC 起止 | exit | 实际结果 |
|---|---|---:|---|
| `cargo test -p aline-source atomic` | 2026-10-31T05:27:52Z–05:27:52Z | 0 | 7 个单元测试 + 7 个集成测试全通过；0 failed；其他测试二进制被过滤为 0 |
| `cargo fmt --all --check` | 2026-10-31T05:27:52Z–05:27:52Z | 0 | 无输出；未改写文件 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 2026-10-31T05:27:52Z–05:27:52Z | 0 | `Finished`，无 warning |
| `python3 scripts/check_safety.py` | 2026-10-31T05:27:52Z–05:27:53Z | 0 | 32 个文件；`failures: []`；`atomic.rs` 为 `engine-production`，`tests/atomic.rs` 为 `engine-test` |

与仓库记录交叉核对（不是本次重新执行）：

- `targeted-test/targeted-test.command.json`：argv 为 `cargo test -p aline-source atomic`，cwd 为仓库根，started `2026-10-31T05:20:00.284121+00:00`，finished `2026-10-31T05:20:00.480211+00:00`，exit 0；stdout 显示 7+7 通过。
- `workspace-gates/summary.json`：`passed: true`，20 条记录，UTC 05:19:38.092791–05:19:51.370672，全部 exit 0；其中包括 `cargo fmt --all --check`、`cargo clippy --workspace --all-targets -- -D warnings`、`cargo test --workspace`、dependency/lint/safety/public-api 及其 self-test。注意 `workspace-gates/test.command.json` 的 argv 是 `cargo test --workspace`，不是任务指定的 `cargo test -p aline-source atomic`；后者只有 `targeted-test` 记录。两者不能被合并成“完整门禁”或“跨平台/CI 已运行”。
- `workspace-gates/safety.stdout.log` 的扫描分类包含 `crates/aline-source/src/atomic.rs`（engine-production）与 `tests/atomic.rs`（engine-test），failures 为空；这不等于安全语义审查，只说明守卫未命中已知规则。

另外做了一个不写仓库的守卫复现：把标准 `Error::source` 写法（返回 `Option<&(dyn Error + 'static)>`，并在 match 中使用 `AtomicFailureKind`）放入 `/tmp` 合成文件，执行 `python3 scripts/check_safety.py --file <tmp> --kind engine-production`，exit 1，命中 `engine-mutable-static`（合成文件第 2 行）；删除合成文件。这个结果用于核对 `atomic.rs` 第 178–180、197–199 行“省略 source 以避开误报”的原因，不代表实际实现文件被守卫拒绝。

### 约束结论

- **A 方案/授权边界：符合。** 生产实现只使用同目录 `create_new` staging、完整写入、`sync_all`、`hard_link` 到新目标；没有 rename、truncate、`set_len`、`fs::copy`、`File::create`、删除或原地覆盖。没有实现 B/C，没有新增 libc/任何依赖，没有 `unsafe`，没有修改 lint/guard/Cargo。`prepare_atomic` 在创建 staging 前做预算、路径与取消检查；`publish_new` 对已存在目标按 symlink/directory/regular/Other 分类拒绝，检查后竞争由 `hard_link` 的 `AlreadyExists` 分类为 `Conflict`，目标不被覆盖。
- **完整写入：静态与 fake 测试均支持。** `write_staging` 使用 64 KiB 分块，处理短写、`Interrupted` 重试、`WriteZero`、偏移溢出、取消和 `sync_all` 失败；生产代码没有 `unwrap`/`expect`/`panic!`/`unreachable!`。`sync_all` 只在全部块写完后调用，失败不构造 `PreparedAtomic`。
- **目标保全：代码结构支持，测试部分覆盖。** 所有 prepare 失败发生在目标被访问之前；publish 在 `hard_link` 前只对目标做 `symlink_metadata` 分类，不写目标；已有普通文件返回 `MetadataPreservationUnverified`，symlink/directory 返回 `ExistingTarget`，竞争返回 `Conflict`。已有目标 bytes、staging 内容和调用方 `EncodedBytes` 在不涉及外部修改的前提下保持。
- **成功回执：正常路径只陈述可实现事实。** `Written` 暴露 target/staging 路径、实际写入的冻结 bytes、长度、输入 `BaselineId`；`directory_durability_unverified()` 恒为 true；`into_baseline` 以 `self.encoded.bytes()` 构造 `StorageSnapshot` 并解码，不从调用方文本拼接，也不把内存快照冒充目标文件的再次读取。它没有宣称目录持久性、断电恢复、跨进程 CAS、跨平台或现有文件覆盖保存。
- **范围声明：基本正确。** 模块头明确 create-only、无 rename/truncate/原地覆盖、无 Drop 静默删除、无目录同步、无现有文件 metadata/ACL/xattr、无断电/CAS/Windows/Linux 已验证行为；`MetadataPreservationUnverified` 使现有普通文件不能进入成功保存。

### 发现

以下共 12 项：**0 个已发现的代码级/安全阻断（没有找到会覆盖或截断现有目标的生产路径，也没有违反 A 授权边界）**；**2 项勾选 4.4 前必须补证或补记的证据阻断（F1、F2）**；**2 项重要限制（F3、F4）**；**8 项次要/观察（F5–F12）**。

#### F1（重要；阻断勾选 4.4 的证据缺口）缺少“目标已存在 + 写入/同步失败”的可执行反例

- 证据位置：`crates/aline-source/src/atomic.rs:768-817` 的 fake writer 测试只直接调用 `write_staging`，测试目录里没有目标文件；`crates/aline-source/tests/atomic.rs:214-242` 覆盖的是 staging 创建失败，`:331-359` 覆盖 publish 前取消，`:362-387` 覆盖 staging 替换，`:117-211`/`:390-443` 覆盖已有目标拒绝；没有任何测试把“目标 sentinel 已存在”与“部分写入失败”或“sync 失败”组合起来。
- 具体未覆盖场景：目标 `TARGET-ORIGINAL\n` 已存在，staging 不存在；`prepare_atomic` 注入在写入 3 字节后返回错误，应断言目标 bytes 完全不变、staging 只含部分 bytes 且被保留、失败分类为 `Io { stage: Write }`；sync 失败同理应为 `Io { stage: Sync }` 且目标不变。当前只能由源码结构推断“prepare 不打开目标”，这是**静态论证**，不是任务要求的文件系统故障步骤记录。
- 为什么重要：任务 4.4 原文明确要求“反例临时创建/写入/替换前失败保全目标”（`tasks.md:42`）；代码路径当前正确，但缺少定向证据会使未来一次看似无害的重构（例如在 prepare 中提前探测/清理目标）无法被测试发现。`atomic-plan.md:83` 也把“真实 FS 故障注入”列为本实现后要运行的测试。
- 建议：在测试可控的前提下增加内部 `#[cfg(test)]` 的 writer 注入 seam（不改变生产 API），或增加真实文件系统的故障注入测试；断言目标 sentinel、staging 的部分/完整内容和 `AtomicFailure::staging_path()`。在补齐前不要把“写入/同步失败保全目标”写成已验证事实。

#### F2（重要；阻断勾选 4.4 的证据缺口）`report.md` 与 `atomic-plan.md` 仍是实现前的状态，缺少实现证据叙述

- 证据位置：`report.md:3` 仍写“4.4 尚未完成，未实现生产文件原语”，`:20` 仍写“未执行 `cargo test -p aline-source atomic`、本组 Cargo/workspace 门禁”；`atomic-plan.md:3` 写“尚无本方案 Rust 实现或文件系统测试通过记录”，`:112` 写“真实 FS 写入、故障注入……均未执行/未验证”。这些文件的 mtime（12:50:14、12:49:34）早于 `atomic.rs`（13:19:20）和测试记录（13:20），是历史文本而非当前实现的报告。
- 具体风险：`targeted-test` 与 `workspace-gates` 的机器记录确实存在且相互一致，但 `report.md` 没有新增“Plan A 实现”章节，无法从报告看出哪些是**实际执行**、哪些是**人工核对**、哪些是**仅静态/未执行**；也没有按任务要求记录本次实现的文件系统与故障步骤、边界和未覆盖项。交接者若先读报告，会得到“尚未实现/未运行”的矛盾结论，或把 `targeted-test` 误当完整门禁。
- 建议：保留历史段落，追加带日期的实现记录，列出源码/金样哈希、实际命令、UTC/退出码、14 个测试名称、真实 FS vs fake writer vs 静态核对的矩阵，以及“未覆盖：磁盘满、权限/ACL、目录 fsync/断电、CAS、跨平台、并发进程”；明确 `cargo test -p aline-source atomic` 与 `cargo test --workspace` 的区别。补齐前 4.4 不宜勾选，`tasks.md` 保持未勾选。

#### F3（重要限制）staging 在身份检查后被原地修改时，成功回执的“bytes 与 staging/target 一致”可能不成立

- 证据位置：`atomic.rs:261-287` 只比较 staging 路径的 `symlink_metadata` 与持有的 `File` 的 `dev`/`ino`；`:288-317` 检查目标；`:318` 调用测试 hook；`:319` 才 `hard_link`。检查之后没有再次验证 staging 的 size/mtime/内容，也没有在成功回执中重新读取目标。`Written::bytes` 的文档在 `:363` 声称“与 staging/target 内容一致”，`Written::into_baseline` 在 `:383-387` 使用内存中的 `encoded.bytes()` 建立基线。
- 具体失败场景：调用方或其他同用户进程在 `prepare` 成功后、`publish_new` 前，通过公开的 `PreparedAtomic::staging_path()` 或已知的 staging 路径，把同一 inode 原地改写为等长其他内容（inode 不变）；身份检查仍通过，`hard_link` 把被改写的内容发布为目标；`Written.bytes()`/`into_baseline` 仍报告操作最初的 encoded bytes，而 target 内容已不同。测试 hook 也能模拟“目标检查后替换 staging”的窗口（当前没有该测试）。
- 为什么重要：create-only 并不允许覆盖既有目标，因此这不是 `tasks.md` 所禁止的“现有文件覆盖”代码路径；但成功回执是宿主登记“实际写出字节/基线”的依据，文档应明确它绑定的是**本操作写出并在 sync 前观察到的 bytes**，而不是对任意外部并发修改后的磁盘事实的保证。`atomic-plan.md` 已声明不证明恶意并发安全，但 `atomic.rs` 的公开文档没有同样显式声明。
- 建议：在 `PreparedAtomic`/`Written` 文档中明确该限制；增加“prepare 后原地改写 staging”测试（断言不会误称一致，或至少记录已知限制）；如需增强，可在 link 前重查 size/mtime 并在 link 后再 stat，但 std 无 fd 级 link 语义，不能宣称完全消除 TOCTOU，且不应为此引入 unsafe/依赖。

#### F4（重要；跨平台限制）非 Unix 的 `same_file_identity` 恒为 false，导致每次发布都报 `StagingReplaced` 且丢失 `staging_path`

- 证据位置：`atomic.rs:585-589` 的 `#[cfg(not(unix))]` 直接返回 false；`publish_new` 在 `:285-287` 将其映射为 `AtomicFailureKind::StagingReplaced` 且 staging 参数为 `None`。`staging_path()` 的文档在 `:166-169` 说“仅在 staging 已创建且未被替换时存在”，但非 Unix 上 staging 确实由本操作创建且未被替换。
- 具体失败场景：在 Windows 等非 Unix 目标上，`prepare_atomic` 可以创建 staging（`open_staging` 的 Unix mode 分支不生效），但 `publish_new` 一定返回 `StagingReplaced`，调用方无法通过失败回执取得本操作拥有的 staging 路径；错误分类还错误地暗示发生了替换。目标不会被修改，所以没有覆盖风险，但 API 语义和可清理性不一致。模块头已写 Windows/Linux 未验证（Linux 实际走 Unix 分支），因此这是**已声明的跨平台限制**，不是当前 macOS 的行为回归。
- 建议：为非 Unix 增加明确的 `PlatformUnverified`/`Unsupported` 失败类别并在失败中携带 `Some(staging)`，或至少保留 owned staging 路径并修正文档；在 `#[cfg(not(unix))]` 下增加编译/行为测试。若 4.4 仅按当前 macOS 验收，应明确不把该分支写成跨平台可用；若宣称 Windows/Linux 支持，则必须先修。

#### F5（次要/限制）`Error::source` 的缺失是可解释的 guard 规避，但仍丢失标准错误链

- 证据位置：`atomic.rs:178-180`、`:197-199` 都是空 `impl std::error::Error`，注释说明安全守卫把 trait object 的 `'static` 词法误判为可变静态；原始 `io::Error`、`StorageError`、`DecodeError` 分别由 `AtomicFailureKind::Io`、`BaselineError::Storage`、`BaselineError::Decode` 公开持有。
- 复现：合成标准 `source` 写法在 `check_safety.py --file ... --kind engine-production` 下 exit 1、命中 `engine-mutable-static`（第 2 行）；不含 `AtomicFailureKind` 词的变体未命中。因此该规避不是无根据的借口，而是在“不得改 guard/lint”约束下的实际取舍。
- 判断：对 M0 可接受，且 `kind()`/公开变体保留了错误类别和原始错误；但标准 `Error::source()` 为空会使 `anyhow`、错误报告器和跨 crate 错误链不能自动展开，和 `docs/rust-style-guide.md` S-7-01 的“保留 source 链/不丢上下文”意图存在偏差。建议在报告中明确列为已知限制，不把空 source 写成“符合完整错误链”；待 guard 修复后再实现 `source()`，本轮不应为此修改 guard。

#### F6（次要/安全观察）派生 `Debug` 会把完整文档 bytes 打日志

- 证据位置：`PreparedAtomic` 在 `atomic.rs:214-220` `#[derive(Debug)]`，`Written` 在 `:345-350` 同样 derive；其字段 `EncodedBytes` 在 `crates/aline-source/src/encode.rs:47-48` 也 derives `Debug`，因此 `{:?}`/`tracing` 会展开整个 `Vec<u8>`。`StorageSnapshot`/`SourceBaseline` 使用自定义 Debug 避免直接打印正文。
- 具体风险：`eprintln!("{written:?}")` 或 `tracing::debug!(?prepared)` 会把整篇文档内容写入日志；这不是磁盘格式错误，但和仓库对敏感正文的最小暴露原则不一致，且调试日志可能被采集。建议为 `PreparedAtomic`/`Written` 实现自定义 `Debug`，只显示路径、长度、基线 ID 和 `directory_durability_unverified`，不显示 bytes。

#### F7（次要/证据限制）金样未被 Git 跟踪，独立来源只能弱证明

- 证据位置：`tests/atomic.rs:118-120` 用 `include_str!` 读取 `atomic-golden.v1.txt`，`:122-137` 解析 5 个字段、`:209` 断言恰有 3 个 case；金样 `unchanged-lf`/`edited-crlf`/`bom-edit` 的 expected 值逐项复算与编码策略相符（CRLF 保留、BOM 保留、未编辑复用原始 bytes），不是明显由实现输出回填的形态。
- 限制：整个 `step-4.4/` 未被 Git 跟踪，没有历史；金样 mtime（13:14:09）早于源码（13:19:20）和测试（13:19:26）是弱证据，可被修改。测试对编码 bytes 和目标 bytes 使用同一个 expected，若将来把实现输出回填金样，测试会自洽通过；因此它是“人工独立预期”，不是机械化外部 oracle。
- 建议：把金样纳入提交并记录哈希，报告说明“内容人工复算、无 Git 时序证明”，不要写成“已由不可篡改历史证明先于实现”。

#### F8（次要）路径校验是词法比较，可能错误拒绝同目录的不同写法

- 证据位置：`atomic.rs:461-474` 只比较 `target == staging`、绝对路径和两个 `parent()` 的词法相等；没有 canonicalize，也不处理大小写/符号链接等价。
- 具体场景：`<tmp>/d/file.md` 与 `<tmp>/d/./staging.tmp`、或 `<tmp>/d/sub/../staging.tmp` 可能被分类为 `DifferentDirectory`，即使它们实际同目录；反过来，大小写不敏感文件系统上不同拼写可能指向同一路径。后者不会造成覆盖：`create_new` 和发布前目标分类会拒绝已存在的同一 inode，最坏是安全失败；前者是可用性/分类问题。
- 建议：在文档中写明“路径前提按词法比较，不承诺 canonicalize 语义”，并增加上述路径的测试；不要为了消除词法差异而引入有 TOCTOU 风险的 canonicalize 更新。

#### F9（次要）staging 在 prepare 后被删除时，失败回执仍报告 `Some(staging)`，与“路径存在”文档不一致

- 证据位置：`atomic.rs:261-271` 在 `symlink_metadata(&staging)` 失败时直接返回 `Io { stage: Publish }` 和 `Some(staging)`；`:166-169` 的 `staging_path()` 说路径仅在 staging 已创建且未被替换时返回。
- 具体场景：prepare 成功后、publish 前外部删除 staging 路径；本操作仍持有已 unlink 的 inode，但回执给出的路径已经不存在，调用方按该路径清理会得到 `NotFound`。这是清理/诊断语义问题，不是目标覆盖；非 Unix 的对应问题是 F4。
- 建议：区分“路径被移除”和“路径被替换”，或在失败文档中说明“返回的是调用方提供且本操作曾拥有的路径，不保证当前仍存在”。

#### F10（次要/覆盖）`WriteZero` 分支未被执行，fake 测试也未断言“失败后未调用 sync”

- 证据位置：`atomic.rs:538-543` 处理 `Ok(0)`，但 `FailingWriter`（`:648-673`）在 `remaining == 0` 时返回 `Err`，不会返回 `Ok(0)`；`atomic_partial_write_failure...`（`:768-793`）只验证 3 字节后返回 Other 错误。`FailingWriter::sync_all` 和 `CancelOnCompleteWriter::sync_all` 都返回 Ok，测试名称中的 “without sync/before sync” 没有 sync 调用计数器支撑（`:769-817`、`:865-888`），无法区分“在 sync 前取消”和“先 sync 再取消”。
- 具体风险：真实 `File` 很少返回 `Ok(0)`，但防御分支无测试；未来改动若把 `Ok(0)` 当成“继续重试”可能造成死循环。sync 顺序的测试也可能在实现错误地先 sync 后检查取消时仍通过。
- 建议：增加返回 `Ok(0)` 的 fake writer，断言 `Io { stage: Write, source.kind() == WriteZero }`；给 fake writer 增加 `sync_calls` 并在 partial/cancel 用例中断言为 0。

#### F11（次要/覆盖）若干目标类型、发布 I/O 和边界条件只有静态核对

- 证据位置：目标分类在 `atomic.rs:288-305` 覆盖 `Symlink`/`Directory`/`File`/`Other`，但集成测试 `tests/atomic.rs:390-443` 只测“symlink 指向普通文件”和 directory；没有 dangling symlink、FIFO/device/socket 的 `Other`。`hard_link` 的 `AlreadyExists` 在 `:325-332` 有测试（unit `:910-936`），但其他发布 I/O 失败（permission、EXDEV、不支持硬链接）在 `:333-339` 没有测试。空 bytes 和恰好 64 KiB 边界没有测试（large 测试是 70,001 bytes，`:446-467`）。
- 影响：分类分支和失败映射未被执行；这些分支的代码看似正确，但不能写成“故障矩阵已覆盖”。其中“目标为 FIFO/device”不应由普通文件测试推导；`hard_link` 的非 `AlreadyExists` 错误也没有真实 FS 证据。
- 建议：在安全可控的前提下增加 dangling symlink、`Other`（用临时 FIFO 或记录“仅静态”）、权限拒绝目录、空/精确分块测试；对无法在测试中安全构造的分支在报告中标注“静态核对/未执行”，不要用局部测试替代。

#### F12（次要/测试卫生）测试临时目录未显式 0700，Drop 清理错误被忽略

- 证据位置：集成测试 `tests/atomic.rs:34` 与单元测试 `atomic.rs:617` 都只调用 `fs::create_dir`，没有设置 0700；测试文件头称“私有临时目录”，但默认 mode 受 umask 影响。`tests/atomic.rs:58`/`atomic.rs:637` 的 `Drop` 使用 `remove_dir_all` 并忽略错误。
- 具体风险：这是测试数据（可能含 sentinel/正文）而非生产文件；在共享 `/tmp` 上，其他本地用户可能读取测试 staging/target；清理失败不会让测试失败，可能留下目录。生产实现没有 Drop 删除，staging 清理由宿主负责，这一点与方案一致。
- 建议：Unix 上用安全 std 创建 0700 目录并断言；若保留忽略清理错误，在测试输出中记录并在报告中说明。生产侧应在报告/API 文档中明确“staging 保留，宿主负责取证/清理”。

### 测试覆盖矩阵

| 场景 | 证据类型 | 现有断言 | 缺口 |
|---|---|---|---|
| 金样成功发布 + 已有普通目标拒绝 | 真实 FS（3 cases） | target 未在 prepare 创建；staging/target bytes；Written 路径/bytes/长度/输入基线；dev/ino 相等；`into_baseline`；sentinel 与 staging 保留；调用方 bytes 不变（`tests/atomic.rs:117-211`） | 仅 3 个简单编码 case；未覆盖权限/ACL/mode |
| staging 创建冲突 | 真实 FS | target 与 existing staging bytes 不变；`Io{Create, AlreadyExists}`；staging_path None（`:214-242`） | 无权限拒绝、staging 为目录/symlink、parent 竞争 |
| 非法路径/预算/prepare 取消 | 真实 FS | 四类 InvalidInput、Budget、Cancelled Prepare；目标/staging 不创建（`:245-328`） | `/` 用例的平台差异；无真实 create 权限失败 |
| publish 前取消 | 真实 FS | target/staging 保留；`Cancelled{Publish}`；staging_path Some（`:331-359`） | 无目标不存在时的取消用例 |
| staging 路径被替换 | 真实 FS | `StagingReplaced`；target 不存在；replacement bytes 保留（`:362-387`） | 无检查后替换、原地等长改写（F3） |
| 已有 symlink/directory 目标 | 真实 FS | 分类正确；sentinel/链接/目录保持；staging_path Some（`:390-443`） | 无 dangling symlink、Other、staging 内容断言 |
| 目标检查后竞争 | 真实 FS + 测试 hook | `Conflict{target_exists=true}`；racer bytes 保留（unit `:910-936`） | hook 非生产并发；未测试真实跨进程竞争 |
| 跨 64 KiB 分块 | 真实 FS | 70,001 bytes 完整写入和发布（`:446-467`） | 空 bytes、恰好 64 KiB、分块中短写成功 |
| 部分写入/同步失败 | fake writer | stage Write/Sync；写入字节数；无目标（unit `:768-817`） | **目标不存在**（F1）；未断言 sync 未调用；未测 WriteZero（F10） |
| 写中取消/完整写后取消 | fake writer | stage Write/Sync；写入计数（`:819-888`） | 无目标；sync 调用未计数；未测取消发生在 sync 中 |
| Interrupted/预算 | fake writer | 重试后完整；预算在写前拒绝（`:845-907`） | 无真实 FS；预算无 staging 失败路径 |
| 仅静态核对 | 源码/文档 | create_new、无 rename/truncate/remove、hard_link、目标拒绝、失败顺序、非 Unix 恒拒绝、成功回执字段 | 无执行证据：目标+写/同步失败（F1）、原地改写（F3）、publish 其他 I/O（F11）、跨平台（F4） |
| 完全未覆盖/未执行 | 无 | — | 磁盘满、权限/ACL/xattr、目录 fsync/断电、跨进程 CAS、Windows/Linux/网络/外接卷、真实并发进程、真实保存闭环 |

### 专项判断

- **`Error::source` 缺失：** 在“不得修改安全守卫”的本次约束下可接受的规避；`AtomicFailureKind`/`BaselineError` 公开变体保留了原始错误，`check_safety.py` 的误报已用合成样例复现（F5）。但它不等同于完整错误链，应该作为已知限制记录；将来 guard 修复后应恢复 `source()`。它不是 4.4 的代码阻断项。
- **非 Unix `same_file_identity`：** 当前 macOS 上是正确的 Unix dev/ino 比较；非 Unix 恒 false 是明确的“拒绝而非伪造”安全策略，但错误分类为 `StagingReplaced`、且不返回 owned staging 路径，API 语义不准确（F4）。若 4.4 仅按当前环境验收，可接受为已声明的跨平台限制；若宣称 Windows/Linux，必须先修。
- **金样来源：** 测试确实通过 `include_str!` 使用金样，不是硬编码在测试内的 expected；3 个 case 的字节内容与编码策略独立复算一致，未见到回填特征（F7）。但目录无 Git 历史，mtime 只能弱支持“实现前手写”，不能证明绝对先后；应在报告/提交中记录哈希并明确证据级别。
- **staging 保留/清理：** 生产代码没有 Drop 删除、没有 rename/删除清理，成功和失败都把 staging 路径交给宿主；发布成功后 staging 是目标的另一硬链接，删除它不会改变目标内容，但本模块不执行删除。这与方案“保留供取证/宿主清理”一致，也避免误删非本操作路径；代价是失败/取消后的 partial staging 需要宿主清理，非 Unix 的路径回执还有 F4 问题。该策略对 M0 可接受，但报告应写出宿主责任和泄漏风险。
- **成功回执与“现有文件保存”：** 正常路径的 target/staging、bytes、长度、输入基线 ID、`into_baseline` 均来自实际 encoded bytes 和 `hard_link` 成功事实；`directory_durability_unverified()` 明确为 true。没有发现把 `MetadataPreservationUnverified` 包装成成功、把 prepare 当发布、把目录同步/断电/CAS/跨平台能力写成已实现的表述。F3 要求把“外部修改后仍一致”的措辞收紧为“本操作写出的 bytes”，不能把成功回执当作目标内容在任意时刻的证明。
- **门禁与证据级别：** `cargo test -p aline-source atomic` 只是局部定向测试；`workspace-gates` 的 `cargo test --workspace` 是另一条记录。两者都通过不等于 CI、跨平台、真实磁盘满/权限/ACL、断电或 V-08 已通过。`tasks.md:42` 仍未勾选，不能把本复核或测试通过当作任务出口。

### 证据层面的最终结论

1. **作为 A 方案的最小 create-only 原语，代码在当前 macOS 工作树和已声明边界内可以接受。** 我没有找到生产路径使用 rename、truncate、`set_len`、原地写、删除或 Drop 清理，也没有找到覆盖已存在目标的路径；`create_new` + 完整写入 + `sync_all` + `hard_link` 的结构、目标分类、竞争 `AlreadyExists` 处理、staging 身份检查和错误分类与方案 A 一致。没有新增依赖、`unsafe`、lint/guard 变更，也没有把 execution-2 探针写成生产准入。
2. **但 4.4 现在仍不宜勾选，阻断的是证据而不是上述代码不变量：** F1 缺少“目标已存在 + 写入/同步失败”的必需反例；F2 的 `report.md`/`atomic-plan.md` 仍停留在实现前状态，未记录本次实现的文件系统/故障步骤和实际/人工/静态/未执行分类。补齐这两项后，再由任务所有者决定是否勾选；`tasks.md` 当前保持未勾选是正确的。
3. **仍远不足以宣称 V-08、M0 出口或现有文件覆盖保存。** 没有目录 fsync/断电、权限/ACL/xattr、磁盘满、跨平台、CAS、并发进程、真实保存闭环的证据；`MetadataPreservationUnverified` 明确拒绝现有普通文件覆盖。`targeted-test` 和 `workspace-gates` 只是当前 macOS 的局部/工作区记录，不是 V-08 或 CI 全平台门禁。
4. **阻断项计数：** 0 个代码级/安全阻断；2 个勾选前必须补证/补记的证据阻断（F1、F2）；2 个重要但当前不阻断 macOS 最小原语接受的问题（F3、F4）；8 个次要/观察（F5–F12）。F3/F4 在任何“完整成功回执/跨平台”宣称前应修复或明确降级，F5–F12 应进入后续测试/文档清单。
5. 本次复核未参与实现，未修改 `atomic.rs`、`lib.rs`、`tests/atomic.rs`、金样、`report.md`、`tasks.md`、Cargo 配置或 lint/guard；只在本节追加 `review.md`，仓库内没有 commit/push。

## 增量复核：F1/F2 修复、注入缝与 execution-3（2026-10-31，追加）

本节只做增量复核，不改写前文；前文对 F1/F2 的阻断判断由本节更新，前文保留为历史记录。复核范围仍为 Plan A、当前 macOS 主机与锁定 Rust 1.98.1；不把实现者或协调者的说法当作结论。

### 工作树身份与边界

复核时工作树实际哈希（`shasum -a 256`）：

| 文件 | SHA-256 |
|---|---|
| `crates/aline-source/src/atomic.rs` | `95053e1726c096f91969848887df8d074839e74afdc6d65a792f76ed8237f298` |
| `crates/aline-source/src/encode.rs` | `94dc73dd5b6a9b6320597a646bf7f7d2979aadc78cf7a83f1ee081f9038ebbf9` |
| `crates/aline-source/src/lib.rs` | `d9c6a66b9667334bbcabb3be1f90a27b0868503b8adb2a5da4cb0747995593e8` |
| `crates/aline-source/tests/atomic.rs` | `4918918a6638cd9e0222712a894eff0d2fa7e4e3504d95473b60376c514b5b7a` |
| `docs/validation/m0-engine-foundation/group-4/step-4.4/atomic-golden.v1.txt` | `d49465a4d18426fb5e28bf0c48192a20d0504f813257aabfe20f390bdb1b4015` |

这些值与 `report.md:101-109` 的执行-3 记录一致；`lib.rs`、`tests/atomic.rs` 与金样未变。`git status` 的已跟踪修改只有 `encode.rs`、`lib.rs`、`atomic-plan.md`；`atomic.rs`、`tests/atomic.rs`、`step-4.4/` 仍是未跟踪文件。Cargo 配置、lint/guard 脚本、`rustfmt.toml` 和 `tasks.md` 没有由本轮复核改动；`tasks.md:42` 仍为 `- [ ]`。

### F1：目标已存在时的写入/同步失败

新增的两项测试不是只调用 `write_staging`：

- `atomic_existing_target_survives_staging_partial_write_failure`（`atomic.rs:966-1011`）先创建真实目标 sentinel，再经 `prepare_atomic_with` 调用真实 `open_staging`，用包装了真实 `File` 的 `PartialFileWriter` 写入 3 字节后注入失败。断言 `Io { stage: Write }`、`staging_path() == Some(...)`、target sentinel 不变、staging 只含已写前缀、sync 调用 0 次。
- `atomic_existing_target_survives_staging_sync_failure_after_full_write`（`atomic.rs:1014-1058`）同样使用真实目标和真实 staging 句柄，完整写入后注入 sync 失败。断言 `Io { stage: Sync }`、staging 保留完整 bytes、target 不变、sync 调用恰好 1 次。
- `atomic_zero_write_is_rejected_as_write_zero_without_sync`（`atomic.rs:1061-1084`）补上 `Ok(0)` 分支，断言转换为 `WriteZero` 且不调用 sync。

`prepare_atomic_with`（`atomic.rs:451-501`）不是测试旁路：预算检查、`validate_paths`、取消检查、`open(staging)` 都在其中，且 `open_staging` 仍使用 `write(true).create_new(true)` 和 Unix mode `0600`（`atomic.rs:543-552`）。公开入口 `prepare_atomic`（`atomic.rs:431-445`）只把 `open_staging` 传给该私有函数并包装返回值；`lib.rs` 没有导出 `prepare_atomic_with`。因此，就当前实现而言，这两项测试覆盖的是生产 prepare 的实际逻辑和真实 staging 创建，而不是孤立的 `write_staging` 或仅静态推理；目标存在时的写入/同步失败要求已闭合。

需要保留的证据边界：这两项失败注入测试直接调用私有 `prepare_atomic_with`，没有直接调用公开 `prepare_atomic`；公开入口是薄包装，现有公开集成测试仍覆盖其成功和现有目标路径。该绑定强度是残余观察，不构成代码阻断。测试也断言目标 bytes，而不是目标的 mode、mtime、xattr 或类型；代码检查显示 prepare 阶段没有任何 target 文件操作（target 只进入词法 `validate_paths`），所以当前实现仍满足“prepare 不触碰目标”，但报告不应把测试能发现的变更范围写得比实际更宽。

### F2：报告与方案是否仍自相矛盾

`report.md:3` 的顶部状态明确把此前段落限定为 A/B/C 裁决前的暂停阶段与 execution-2 历史，并说明“4.4 尚未完成/未实现”“未执行 `cargo test`”只描述当时状态；`report.md:5`、`:22`、`:26`、`:76` 因此不再是当前时态结论。`report.md:95-201` 追加了执行-3 的源文件/金样哈希、实际命令、UTC/退出码、17 个测试的覆盖矩阵、真实 FS/假 writer/静态未执行的分界和 F3–F12 限制；`atomic-plan.md:3` 的状态行已更新为“已实现、待独立复核 F1/F2”，`:112` 也改为“已实际执行/未执行未验证”的当前划分。

旧段落中的“未实现”字样仍然存在，但顶部状态已明确其历史范围，因此不再构成当前事实陈述的矛盾。若后续要减少误引风险，可以在旧段落就地加历史标记；这是可读性观察，不是勾选阻断。

### 注入缝、Debug 与安全边界

**注入缝没有改变生产行为。** `prepare_atomic` 仍以同一个 `open_staging` 创建 staging，`prepare_atomic_with` 只是把 writer 类型泛化；前置检查仍在打开 staging 之前执行，闭包只收到 staging 路径，不接收 target。该函数私有且未重导出，外部调用方无法传入自定义 writer；没有新增公开绕过路径。生产 `File` 实现 `StagingWriter`，`sync_all` 仍委托 `File::sync_all`。未发现 prepare 对 target 的打开、删除、截断、rename 或元数据写入。

**Debug 不再泄漏正文。** `PreparedAtomic` 的自定义 Debug（`atomic.rs:230-238`）只输出 target、staging、`bytes_len`；`Written` 的自定义 Debug（`atomic.rs:370-378`）同样只输出路径与长度；`EncodedBytes` 的自定义 Debug（`encode.rs:54-61`）只输出 `bytes_len` 与 `reused_original`。`AtomicFailure` 的派生 Debug 只含失败分类和 staging 路径，`io::Error` 不含文档 bytes。既有的 `SourceBaseline`、`StorageSnapshot` 也都使用不打印源码/bytes 的自定义 Debug；`DecodedSource` 的派生 Debug 通过 `SourceBaseline` 的自定义实现同样不展开正文。未发现本轮新增的正文泄漏。

**没有新的代码/安全阻断。** 未增加依赖、`libc`、`unsafe`、线程或平台后端；`rg` 未发现生产代码使用 rename、truncate、`set_len`、remove 或原地 target 写入。`cargo clippy --workspace --all-targets -- -D warnings`、`cargo fmt --all --check`、`python3 scripts/check_safety.py` 和 `cargo test --workspace` 均本地退出 0；`scripts/check_workspace.py` 未由本次复核执行（按约束它会改写证据），只读取执行-3 的 `summary.json`。

### 证据重跑与覆盖级别

执行-3 的只读证据与协调者声明一致：`targeted-test-execution-3/targeted-test.stdout.log` 显示 10 个单元测试和 7 个集成测试全部通过；`workspace-gates-execution-3/summary.json` 的 19 条记录全部 `exit_code: 0`，其中包括 `cargo test --workspace`、fmt、clippy、dependency、lint、safety、public-api 及其 self-test。报告中“四个 0-test 二进制不是失败”的说明与日志一致。

本次独立重跑结果：

| 命令 | 结果 |
|---|---|
| `cargo test -p aline-source atomic` | 10 + 7 全通过，退出 0 |
| `cargo test --workspace` | 全部测试目标通过，退出 0 |
| `cargo fmt --all --check` | 退出 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | 退出 0 |
| `python3 scripts/check_safety.py` | `failures: []`，退出 0 |

这些仍是当前 macOS 主机、当前工具链和本地工作区的证据，不等于 CI、跨平台、真实磁盘满、断电、ACL/xattr 或 V-08。一次外层 zsh 使用保留变量名 `status` 导致 shell 报错，但定向测试本身已显示全通过；随后用非保留变量重跑并确认退出 0。

### 残余观察（非阻断）

1. **F13（次要证据绑定）：** F1 的故障注入测试调用私有 `prepare_atomic_with`，没有直接调用公开 `prepare_atomic`。公开入口是单行委托，且公开集成测试覆盖成功/现有目标路径；当前没有功能缺口，但这不是公开入口的端到端故障注入证据。
2. **F14（次要覆盖范围）：** 两项目标保全测试只比较 target bytes，没有比较 mode、mtime、xattr 或文件类型；报告“未来在 prepare 中提前探测、清理或改写 target 的重构会被这两个测试发现”的表述偏宽，读操作或仅改 metadata 的回归不一定被这两项测试发现。prepare 当前确实没有任何 target 文件操作，因此不影响当前代码结论，但报告措辞应收窄。
3. **F15（次要文档可读性）：** `report.md` 旧段落仍保留“未实现/未开始”字样；顶部状态已限定历史范围，当前不再矛盾。建议后续就地加历史标记或把旧段落移入明确的历史小节，避免被脱离上下文的引用误读。
4. **F3/F4 仍是重要限制，不是本轮 macOS 最小原语的勾选阻断：** F3 的“身份检查后同 inode 原地改写”仍无法在发布后回读发现；F4 的非 Unix 身份比较仍恒拒绝且错误分类/路径回执不精确。两者都已在报告中明示，且不影响 macOS create-only 新目标路径的当前结论。F5、F7、F8、F9、F11、F12 仍是已记录的限制/未测边界；F6 已由自定义 Debug 修复，F10 已由零写入与 sync 计数测试补强（旧假 writer 测试仍不完全计数 sync）。

### 增量结论

1. **F1 已闭合。** 目标已存在时的部分写入失败和完整写入后 sync 失败，确实经生产 `prepare_atomic_with` 路径、真实 `open_staging` 和真实文件系统执行，断言覆盖失败阶段、staging 路径回执、target sentinel、staging 前缀/完整 bytes 及 sync 调用次数；不再只是 `write_staging` 的 fake-writer 测试。
2. **F2 已闭合。** 报告已提供实现后的哈希、实际命令、UTC/退出码、17 个测试的覆盖矩阵与真实/注入/静态边界；方案状态行同步更新。旧句仍有历史限定，但不构成当前矛盾。
3. **注入缝与 Debug 没有新增阻断。** 私有辅助函数不改变生产 `prepare_atomic` 的行为，不绕过预算/路径/取消/`create_new`；自定义 Debug 不输出正文；没有新增依赖、`unsafe`、lint/guard 变更或新的代码/安全阻断。
4. **阻断计数更新：** 0 个代码级/安全阻断；F1/F2 证据阻断已解除；0 个剩余的勾选前证据阻断；F3/F4 为 2 个重要但当前不阻断 macOS 最小原语接受的限制；F13–F15 为新增次要观察，F5、F7、F8、F9、F11、F12 为既有次要/未测边界。任务所有者可以据此决定是否勾选 4.4；本复核不代写 `tasks.md`，仍不宣称 V-08、M0 出口、现有文件覆盖保存或跨平台能力。
5. 本次只追加本段 `review.md`；未修改代码、金样、`report.md`、`tasks.md`、Cargo 配置或 lint/guard，未 commit/push。
