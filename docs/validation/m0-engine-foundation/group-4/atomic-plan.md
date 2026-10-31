# M0 最小受检 I/O 原语方案

日期：2026-10-31。状态：任务 4.4 的实施建议。2026-10-31 已按用户裁决的 A 方案在 `aline-source` 实现最小 create-only 原语，并取得 macOS 本机真实文件系统与故障注入记录；执行-3 的源文件哈希、命令、UTC、退出码、测试矩阵与限制见 [`step-4.4/report.md`](step-4.4/report.md)。4.4 仍待独立复核接受 F1/F2 后才由任务所有者决定是否勾选，不等同 V-08、现有文件保存或跨平台能力。本文件不修改规范、tasks 或产品合同；只读源码和文档分析不是实际权限/替换实验。

## 1. 推荐结论

M0 使用具体的 prepare + create-only publish 两阶段：同目录 `create_new` 临时文件，完整写入并 `sync_all` 后，以 `std::fs::hard_link(staging, target)` 发布到**尚不存在**的新目标。若目标已存在，默认返回 `MetadataPreservationUnverified`，不 `rename` 覆盖，不原地截断，不把能力未验证伪称 OS `Unsupported`。

这提供实际完整写入正例、临时创建/写入/发布前故障保全反例，以及新目标竞争出现时的不覆盖拒绝。它不是现有文档保存闭环，不声称已完成现有用户文件 ACL/xattr 保留或三平台原子保存。

## 2. 当前实现与规范依据

已确认约束：

- tasks 4.4 明列“完整写入”正例、“临时创建/写入/替换前失败保全目标”反例；4.5 明列权限/替换竞争未测边界。没有要求 M0 宣称现有文件完整 metadata 替换能力。
- source delta 的受检文件原语要求区分 I/O、取消、冲突，失败不发布成功基线；成功返回实际冻结 bytes 的写出事实，由宿主结算。
- [引擎 §5.3](../../../engine-design.md#53-最小文件层接口)将同步受检 I/O 放 source、保存编排放 UI，并明确 mode 不等于 ACL/xattr。
- [架构 §7.3](../../../v1-architecture.md#73-保存外部变化和关闭)第 569～576 行：不原地截断/先删原文件，不伪称跨进程 CAS；平台权限、目录同步、竞争窗口留 V-08。WC-04 有条件继续只适用于已证明的 Unsupported/ENOTSUP 和安全访问语义。
- [R-15-07](../../../architecture-guide.md#151-所有权保存状态及外部变化)与 [WC-04](../../../wordcraft-study.md#wc-04--不支持权限复制时是否允许继续保存)要求其他权限失败保全原文件，不允许一概忽略。
- source 当前只准入 std，自有 unsafe forbid；本方案不增加平台库、shell 服务、线程、全局状态或保存协调器。

因此，对现有目标明确拒绝未证明的 metadata 替换能力，正是受检原语的失败边界；不能把拒绝称为现有文件成功保存。后续 M2/V-08 再接完整平台替换能力，M0 不预装无人消费的后端。

## 3. 推荐具体 API 与步骤

下述是建议签名，不代表最终公开 API；实现已按 A 方案落地，类型名与签名以 `crates/aline-source/src/atomic.rs` 及 `step-4.4/report.md` 为准。

```text
prepare_atomic(encoded: &Encoded, target: &Path, staging: &Path,
               budget: IoBudget, cancellation: Cancellation)
    -> Result<PreparedAtomic, AtomicFailure>
prepared.staging_path() -> &Path
prepared.publish_new(cancellation: Cancellation)
    -> Result<Written, AtomicFailure>
written.into_baseline() -> Result<Baseline, SourceError>
```

`Encoded` 来自已绑定基线/冻结文本的受检编码结果，不接受无来源的成功 bool。`PreparedAtomic`、`Written` 字段私有；调用方不能伪造“实际成功写出”。source 不把 UI 文档/Buffer 类型引入接口。

prepare：

1. 先检查字节预算、路径不相同、同目录/同文件系统前提及取消。staging 名由显式调用上下文提供，使用 `create_new`，不得对已有临时路径 `create(true)` 或 `truncate`。命名冲突返回错误，不覆盖、删除或复用已有文件。
2. 创建新的 staging，保留创建得到的 File 句柄及该操作的所有权记录。Unix 可通过安全 std 扩展请求初始 mode 0600；这只说明 mode，不能声称已检查全部 ACL 或继承规则。
3. 以受限分块完整写入，正确处理短写、Interrupted、WriteZero、取消和 I/O 错误；在循环中检查取消/预算。所有内容写完后调用 `sync_all`，成功才返回 PreparedAtomic。
4. prepare 不改变目标文件。即使目标已有字节，也能在此阶段注入创建/部分写入/同步故障，验证原目标始终不变。

publish_new：

1. 检查取消和 staging 仍对应本操作拥有的准备结果。检查目标时使用能识别链接自身的元数据入口；普通既有文件返回 `MetadataPreservationUnverified`，链接/目录等返回明确不适用类别。不得据 `Permissions::readonly` 或 mode 推断完整权限保证。
2. 调用 hard_link 完成无覆盖发布；不能使用“先判不存在、再 rename”作为不覆盖实现。目标在检查后竞争出现时，hard_link 的 AlreadyExists 分类为 Conflict，保留竞争目标。
3. hard_link 成功意味着新目标已经指向完整准备内容；立即形成 Written，绑定实际目标、冻结 bytes/源码、输入基线身份及能力说明。宿主根据该事实建立新基线，不能把较新输入标已保存。

取消只在发布前保证无新目标；不能把实际发布之后到达的取消转换成“未写入”。File 同步或 OS 系统调用内部不能中断，须如实登记这个盲区。

M0 的 staging 由明确所有者保留：成功与失败结果都可暴露本操作拥有的 staging 路径用于取证/后续清理。没有 Drop 中静默 remove_file，也不删除用户原有文件或其他操作的路径。生产清理由后续明确所有者处理，不在此任务暗中增加清理协调器。

## 4. 错误、成功事实与关键取舍

建议错误枚举至少区分：

| 分类 | 行为 |
|---|---|
| `Io { stage, kind, ... }` | 保存原 io 错误分类，指出 create/write/sync/publish 阶段；不翻译成用户文案 |
| `Cancelled { stage }` | 发布前停止，不生成 Written；保留拥有的 staging 信息 |
| `Conflict { target_exists }` | hard_link 不覆盖既有/竞争目标，原字节保留 |
| `MetadataPreservationUnverified` | 现有文件完整 metadata 能力未实现/未证明；不是 OS Unsupported，也不允许有条件继续分支 |
| `InvalidInput` / `Budget` | 创建/写入之前或分块循环内受检拒绝 |

Written 至少说明“文件内容已完整写入且文件 sync 成功、目标已发布、目录持久性未验证、staging 保留”。不把 hard_link 或 sync_all 泛化为断电保证。

若以后在发布后增加目录同步或清理，必须返回“已发布事实 + durability/cleanup 告警”，不能将它们折叠为普通发布前失败、丢弃成功回执或假装原目标未变。本 M0 方案不需要提前实现这些后置动作。

hard_link 保证的是新路径不覆盖已有目标，不能据此声称同目录 staging 路径、父目录替换、链接别名或恶意并发都已安全。真实测试使用受控私有夹具目录；ACL 继承、非本地文件系统及路径竞争留独立平台验证。

## 5. 风险与回滚

没有实施本方案时无需代码回滚。实施后如 hard_link 在某文件系统不支持，返回真实 I/O/Unsupported 分类，不回退为直接写目标，也不扩大权限豁免。既有目标能力尚未建立时继续拒绝，保留内存、原目标与准备材料。

范围登记要精确：M0 证明 create-only 发布和前置故障保全；现有文件替换、ACL/xattr、外改协调、覆盖确认、串行保存队列、目录同步/断电、Windows/Linux 均未验证。若未来要声称这些能力，需要 V-08 的实际实现和证据，不能由本最小原语结果外推。

## 6. 验证计划与只读取证

真实 FS 测试，实施后运行 `cargo test -p aline-source atomic`：

- 新目标完整发布：手写冻结 bytes，prepare 后 staging 与独立预期逐字节一致，publish 后 target 相同；Written 只在成功发布后存在。
- 目标原有字节时：临时名称冲突/创建失败、写若干字节后失败、pre-publish 取消或注入失败，均比较原目标完整 bytes 不变；失败不生成新基线。
- prepare 后竞争创建目标，再 publish，返回 Conflict，竞争目标完整保留。
- 普通既有目标走 MetadataPreservationUnverified；不调用 rename、set_permissions 或截断写目标。链接/目录单列拒绝。
- 不把“真实 FS 使用可控 Writer 的部分写入错误”写成真实磁盘满实验；明确注入位置及实际步骤。

最小 fake：优先复用 std::io::Write 的可控短写/失败 Writer，或仅为 create/write/sync/pre-publish 检查点提供模块内小替身。必须与生产流水线共用实际顺序；不构建公开通用 FileSystem trait/多平台后端。fake 证明错误次序和目标零修改，不证明真实权限、磁盘满或崩溃语义。

本次已执行的只读命令均退出 0：

```sh
rustc --print sysroot
rg -n 'fn copy|COPYFILE_|fcopyfile|fclonefileat' /Users/xian/.rustup/toolchains/1.98.1-aarch64-apple-darwin/lib/rustlib/src/rust/library/std/src/sys/fs/unix.rs
sed -n '2300,2383p' /Users/xian/.rustup/toolchains/1.98.1-aarch64-apple-darwin/lib/rustlib/src/rust/library/std/src/sys/fs/unix.rs
```

实际输出：sysroot 是 `/Users/xian/.rustup/toolchains/1.98.1-aarch64-apple-darwin`；Apple 分支第 2317 行开始，2335 行 `fclonefileat(..., 0)` 成功即返回；EEXIST/ENOTSUP/EXDEV 后进入 fcopyfile 分支，2362 行对普通目标使用 COPYFILE_ALL。此为锁定本机源码事实，不是文件系统复制试验。

外部 primary 资料核对：

- Rust [hard_link 文档](https://doc.rust-lang.org/std/fs/fn.hard_link.html)明确目标已存在会报错；[create_new 文档](https://doc.rust-lang.org/std/fs/struct.OpenOptions.html#method.create_new)说明不覆盖现有文件或悬空链接。
- Rust [Permissions 文档](https://doc.rust-lang.org/std/fs/struct.Permissions.html)明确 readonly/mode 不读取 ACL；[copy 文档](https://doc.rust-lang.org/std/fs/fn.copy.html)的可移植承诺是内容和 permission bits，不能泛化成完整 metadata。
- Apple 官方 [clonefile.2](https://github.com/apple-oss-distributions/xnu/blob/main/bsd/man/man2/clonefile.2)说明 flags=0 继承目标目录 ACL，CLONE_ACL 才额外复制源 ACL；因此不能把本机 `std::fs::copy` 的所有成功分支当完整 ACL 保留证明。
- Apple 官方 [copyfile.3](https://github.com/apple-oss-distributions/copyfile/blob/main/copyfile.3)分别定义 ACL/STAT/XATTR 与 COPYFILE_ALL。即使预建 temp 使 Rust copy 进入 fcopyfile(COPYFILE_ALL)，也仍需实际写后验证和路径/竞争取证，当前不据它放行现有用户文件覆盖。

在线 Rust 页面本次显示 1.99.1，仅用作稳定 API 文档参照；版本化 1.98.1 页面读取失败，已改用上列本机 1.98.1 源码核对具体分支。不把在线最新版实现当锁定工具链事实。

当前已实际执行（2026-10-31，macOS 本机）：create-only 真实 FS 写入与发布、部分写入/同步失败注入、`create_new` 冲突、prepare/publish 取消、staging 替换检测、已存在普通文件/符号链接/目录拒绝、检查后目标竞争、70,000 字符跨 64 KiB 块写入。未执行/未验证：ACL/xattr、权限复制、目录 fsync/断电、跨进程 CAS、Windows/Linux、网络/外接卷、真实应用保存闭环。V-01～V-09 不据本分析或该最小原语判通过。

## 7. 不推荐方案

- `set_permissions`/mode 成功便声称完整 ACL/xattr 保留。
- 把“实现尚无 metadata 能力”包装为 OS Unsupported，再默认继续覆盖。
- 在权限错误、hard_link 不支持或 rename 失败后回退为原地截断。
- 先检查目标不存在，再用允许覆盖的 rename 声称原子 create-only。
- 用 shell/cp 作为生产文件服务，或提前新增平台依赖/unsafe 例外。
- 为成功清理临时文件而删除无法证明属于本操作的已有路径。
- 将新目标成功发布称为已有文档保存闭环、完整平台事务或断电保证。
