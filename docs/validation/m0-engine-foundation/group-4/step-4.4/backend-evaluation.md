# 4.4 权限复制后端候选初筛

2026-10-31：按用户“你的建议来吧”继续调查安全封装；以下是隔离源码初筛，不是依赖准入、完整安全审计或平台支持结论。未添加依赖、改变 lint 或接入生产代码。

| 候选 | 已核实 | 当前决定 |
|---|---|---|
| [exacl 0.13.0](https://docs.rs/exacl/0.13.0/exacl/) | 原包 SHA-256 `8de9e551ced9d700a31dacb8b168129640240fcd7a279a3fa4d59608b84ebcae`；MIT，MSRV 1.85，默认 features 空；公开 getfacl/setfacl 输入为路径，没有公开 File/AsFd 入口。macOS `SYMLINK_ACL` 的末级链接处理不消除祖先路径和目标身份竞争 | 不作为已持有句柄的安全复制后端；未做构建或依赖闭包审计 |
| [libfs 0.9.4](https://docs.rs/libfs/0.9.4/libfs/) | 原包 SHA-256 `b79c3145dcaad30b6d7b1e30c9d2d23799b882e9a2e9d1fc7bcc0699c5279ff9`；GPL-3.0-only，MSRV 1.88；`copy_permissions(&File, &File)` 的 `common.rs` 明确有 ACL FIXME，macOS 的 xattr 分支不启用，并将 xattr 错误记录后继续 | 不满足当前 macOS 权限要求；许可证兼容性未做独立法律判断，未做依赖闭包审计 |
| rustix 1.1.4 / 1.1.5 | 本地缓存公开安全句柄 xattr API；`fcopyfile` 与 state free 仍为 `unsafe fn`，xattr API 本身不能证明完整 macOS ACL 复制 | 不能仅因已有缓存而当作准入；未做完整依赖和安全审计 |
| windows-permissions 0.2.4 | 缓存源码有安全签名的 Windows 安全描述符接口；内部成功后空指针分支含 `expect` | 仅 Windows，未审计可达性、依赖和许可，不能补足 macOS |

Apple 官方 [copyfile 手册](https://github.com/apple-oss-distributions/copyfile/blob/main/copyfile.3)描述 `fcopyfile` 使用已打开文件描述符，`COPYFILE_SECURITY` 涵盖 POSIX 信息与 ACL；`COPYFILE_SECURITY` 不包含 XATTR（`COPYFILE_METADATA` 才包含）；特殊权限位的默认行为以及是否满足 Aline 权限与目标平台约束仍须逐项验证。标准库文档明确 [`fs::copy`](https://doc.rust-lang.org/stable/std/fs/fn.copy.html) 会覆盖目标，且同一文件可能被截断；本地锁定版本和实际反例见 `report.md`。当前未找到满足全部边界的现成安全句柄 API，不代表此类封装不存在。

## 下一步的具体评估提案（未批准实施）

先独立审查最小 macOS 元数据复制边界：输入为有效且生命周期受借用约束的已打开原文件与独占临时文件；只请求明确的安全元数据复制标志；要求两句柄不指向同一 inode，逐项核查 ACL、owner/group、特殊模式位、错误和复验顺序，不从路径重新打开临时目标。候选可以是经依赖准入和安全审计的现成安全封装；若确需自有 unsafe，需先提交具体例外范围、必要性、安全不变量、有效 lint 配置、保留其他 lint 的方式、拒绝/失败与退出条件，经用户具体裁决和独立安全评审后才能实施。现有 `workspace.lints.rust.unsafe_code = "forbid"` 与成员继承不会因计划自动改变。

独立样本至少覆盖 ACL allow/deny、继承标志、权限复制失败、临时路径替换、目标变化、部分写入与同步失败；失败对比原目标 bytes 和权限事实，替换成功后再发生清理/目录同步错误必须保留已发布回执。macOS 外的平台后端和访问语义分别按 V-01/V-08 验证；不能将 macOS 样本推断为 Windows/Linux 通过，也不能以普遍拒绝含 ACL 的正常文件替代实现。

隔离实验的具体、已授权范围见 `macos-probe-proposal.md`；该实验不构成生产 unsafe 或 libc 直接依赖的例外。
