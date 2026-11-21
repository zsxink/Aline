//! macOS 受检替换平台原语：全仓库唯一允许 `unsafe` 的源码文件。
//!
//! 准入记录见 `docs/decisions.md` §44（2026-11-18 用户明确准入「最小自包含 FFI」）。
//! 边界：
//! - 只声明平台探针 `macos-fs-20261118-f638e1` 及其后续回归实测过的 Darwin ABI，
//!   不新增第三方依赖；
//! - `unsafe` 不越过本文件，`platform` 模块只转出安全函数；
//! - 包级 lint 为 `unsafe_code = "deny"`（见 `crates/aline-source/Cargo.toml`），本文件
//!   用模块级 `#![allow(unsafe_code)]` 显式豁免；其他文件出现 `unsafe` 在普通
//!   `cargo build` 也因 deny 而失败。`scripts/check_safety.py` 的 `unsafe-owned`
//!   规则只放行本文件；宏展开产生的 `unsafe` token 不在脚本扫描范围，门禁仍依赖
//!   CI 执行脚本。
//!
//! 发布顺序与失败边界见 `openspec/changes/qmd-w4-persistence-recovery/design.md` §3：
//! 创建 staging 前若父目录 ACL 含 `file_inherit` 条目则失败关闭——继承发生在
//! `openat` 返回之前，第三方若在清 ACL 前打开该 inode，事后清 ACL 不能撤销已持有
//! 的描述符；创建后再检查一次父目录并清除、复验 staging 自身 ACL，覆盖并发新增
//! 继承条目的窗口。随后在 staging 上逐项复制并复验目标的访问元数据
//! （mode/uid/gid、扩展 ACL、xattr），再用 `renameatx_np(RENAME_SWAP)` 原子交换
//! 目录项；交换后核对 staging 路径仍持有交换前观察到的旧目标 inode，并完整回读
//! 旧目标字节，确认最终窗口内没有同尺寸原地改写，不符则再交换回滚。**这不是
//! 跨进程 CAS**：复验与交换之间的写入由交换后字节对照发现并回滚，对照完成之后的
//! 写入仍不可消除。
//!
//! 明确不支持（失败关闭，不静默降级）：
//! - 文件 flags（`chflags`）非 0 的目标：immutable/append/nounlink 会阻止 rename 或清理；
//! - 任何 `deny delete` ACL：探针证明其使 rename/swap 返回 `EACCES`，且必须先改目标
//!   才能复制该 ACL，违反「失败保留原目标」；
//! - 非 APFS/网络卷及各平台故障注入语义：未取证，不在本模块声称。

#![allow(unsafe_code)]

use std::ffi::CString;
use std::fs::{File, Metadata, Permissions};
use std::io::{self, Read, Seek, SeekFrom};
use std::os::fd::{AsRawFd, FromRawFd};
use std::os::macos::fs::MetadataExt as MacMetadataExt;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt as UnixMetadataExt;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::ptr;

use core::ffi::{c_char, c_int, c_uint, c_void};

use super::super::{AtomicFailureKind, AtomicStage};
use super::StagingOpenFailure;

type SizeT = usize;
type SsizeT = isize;
type UidT = u32;
type GidT = u32;
type AclT = *mut c_void;

/// `sys/acl.h`：扩展 ACL 类型。
const ACL_TYPE_EXTENDED: c_int = 0x100;
/// `sys/fcntl.h`：读写打开；替换路径需要回读 staging 字节。
const O_RDWR: c_int = 0x0002;
/// `sys/fcntl.h`：不存在则创建。
const O_CREAT: c_int = 0x0200;
/// `sys/fcntl.h`：独占创建，已存在则失败。
const O_EXCL: c_int = 0x0800;
/// `sys/fcntl.h`：拒绝末端符号链接。
const O_NOFOLLOW: c_int = 0x0100;
/// `sys/fcntl.h`：exec 时关闭描述符。
const O_CLOEXEC: c_int = 0x1000000;
/// `sys/stdio.h`：`renameatx_np` 交换两个目录项。
const RENAME_SWAP: c_uint = 0x0002;
/// `sys/errno.h`：目标或 staging 路径消失。
const ENOENT: i32 = 2;
/// `sys/errno.h`：缓冲不足，重试读取。
const ERANGE: i32 = 34;
/// `sys/errno.h`：平台不支持该操作。
const ENOTSUP: i32 = 45;
/// 新建 staging 的权限位；目录 ACL 在创建后立即清除并复验。
const STAGING_MODE: c_int = 0o600;

unsafe extern "C" {
    // int openat(int, const char *, int, ...); 变参 mode 按 C 默认提升传 c_int。
    fn openat(dirfd: c_int, path: *const c_char, flags: c_int, ...) -> c_int;
    // int fchown(int, uid_t, gid_t);
    fn fchown(fd: c_int, owner: UidT, group: GidT) -> c_int;
    // ssize_t flistxattr(int, char *, size_t, int);
    fn flistxattr(fd: c_int, list: *mut c_char, size: SizeT, options: c_int) -> SsizeT;
    // ssize_t fgetxattr(int, const char *, void *, size_t, u_int32_t, int);
    fn fgetxattr(
        fd: c_int,
        name: *const c_char,
        value: *mut c_void,
        size: SizeT,
        position: u32,
        options: c_int,
    ) -> SsizeT;
    // int fsetxattr(int, const char *, const void *, size_t, u_int32_t, int);
    fn fsetxattr(
        fd: c_int,
        name: *const c_char,
        value: *const c_void,
        size: SizeT,
        position: u32,
        options: c_int,
    ) -> c_int;
    // acl_t acl_get_fd_np(int, acl_type_t); 探针确认 2 参数 ABI。
    fn acl_get_fd_np(fd: c_int, acl_type: c_int) -> AclT;
    // int acl_set_fd_np(int, acl_t, acl_type_t); 探针确认 3 参数 ABI。
    fn acl_set_fd_np(fd: c_int, acl: AclT, acl_type: c_int) -> c_int;
    // acl_t acl_init(int);
    fn acl_init(count: c_int) -> AclT;
    // char *acl_to_text(acl_t, ssize_t *);
    fn acl_to_text(acl: AclT, len_p: *mut SsizeT) -> *mut c_char;
    // int acl_free(void *);
    fn acl_free(object: *mut c_void) -> c_int;
    // int renameatx_np(int, const char *, int, const char *, unsigned int);
    fn renameatx_np(
        from_fd: c_int,
        from: *const c_char,
        to_fd: c_int,
        to: *const c_char,
        flags: c_uint,
    ) -> c_int;
    // int unlinkat(int, const char *, int);
    fn unlinkat(dirfd: c_int, path: *const c_char, flag: c_int) -> c_int;
}

/// ACL 句柄的 RAII 所有权；非空句柄由 `acl_free` 释放。
struct AclHandle(AclT);

impl Drop for AclHandle {
    fn drop(&mut self) {
        // SAFETY: 构造时保证非空且独占地由本值拥有；acl_free 接受 acl_t 与
        // acl_to_text 返回值（均经 acl_free 释放），重复释放不会发生。
        let _ = unsafe { acl_free(self.0) };
    }
}

/// `acl_to_text` 文本缓冲区的 RAII 所有权；同样由 `acl_free` 释放。
struct AclText(*mut c_char);

impl Drop for AclText {
    fn drop(&mut self) {
        // SAFETY: 构造时保证非空且独占地由本值拥有；缓冲区来自 acl_to_text，
        // 按 man 约定用 acl_free 释放。
        let _ = unsafe { acl_free(self.0.cast()) };
    }
}

/// 目标访问元数据的完整快照；只覆盖 M2 声明支持的字段。
struct AccessState {
    mode: u32,
    uid: u32,
    gid: u32,
    flags: u32,
    acl: Option<Vec<u8>>,
    xattrs: Vec<(CString, Vec<u8>)>,
}

/// 创建同目录 staging，并清除目录 ACL 可能施加的继承访问。
///
/// 独占创建（`O_CREAT|O_EXCL`）只绑定已核验的父目录 fd。父目录 ACL 含
/// `file_inherit` 条目时在创建前失败关闭：继承在 `openat` 返回前已生效，第三方可能
/// 在清除前打开该 inode 并一直持有描述符，事后清 ACL 不是安全边界。创建后再检查
/// 一次父目录并清空、复验 staging 自身 ACL，覆盖并发新增继承条目的窗口。创建后的
/// 任何失败都尝试按父目录 fd 删除本操作的 inode；删除失败或身份变化时保留现场，
/// 由调用方携带的 staging 路径交给宿主清理。
pub(crate) fn open_staging(parent: &File, path: &Path) -> Result<File, StagingOpenFailure> {
    let name =
        path_component(path, AtomicStage::Create).map_err(StagingOpenFailure::not_created)?;
    if parent_acl_inherits(parent).map_err(StagingOpenFailure::not_created)? {
        return Err(StagingOpenFailure::not_created(
            AtomicFailureKind::ReplacementMetadataUnverified,
        ));
    }
    // SAFETY: parent 在调用期间有效；name 是以 NUL 结尾的单一路径组件；openat 只
    // 读取该目录项并返回新描述符。变参 mode_t 在 Darwin 为 uint16_t，按 C 默认
    // 提升传 c_int 是正确 ABI。
    let fd = unsafe {
        openat(
            parent.as_raw_fd(),
            name.as_ptr(),
            O_RDWR | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC,
            STAGING_MODE,
        )
    };
    if fd < 0 {
        return Err(StagingOpenFailure::not_created(publish_io(
            AtomicStage::Create,
            io::Error::last_os_error(),
        )));
    }
    // SAFETY: fd 是 openat 刚返回的有效、未共享描述符；所有权此后只属于 file。
    let file = unsafe { File::from_raw_fd(fd) };
    let verify = || {
        if parent_acl_inherits(parent)? {
            return Err(AtomicFailureKind::ReplacementMetadataUnverified);
        }
        // 继承条目可能在 openat 前短暂存在又消失：两次父目录检查都会通过，但
        // staging 已继承 ACL。发现 staging 自身有 ACL 即失败关闭，不先清除后继续；
        // 第三方可能已在这段窗口内打开该 inode。
        if read_acl(&file)?.is_some() {
            return Err(AtomicFailureKind::ReplacementMetadataUnverified);
        }
        clear_acl(&file)?;
        if read_acl(&file)?.is_some() {
            return Err(AtomicFailureKind::ReplacementMetadataUnverified);
        }
        Ok(())
    };
    match verify() {
        Ok(()) => Ok(file),
        Err(kind) => {
            remove_created_staging(parent, path, &name, &file);
            drop(file);
            Err(StagingOpenFailure::created(kind))
        }
    }
}

/// 父目录 ACL 是否含会继承到新建文件的条目。
///
/// 只拒绝 `file_inherit`：仅 `directory_inherit` 的条目作用于子目录，不影响
/// staging 文件本身。这里不评估条目主体是否为本用户；任何继承条目都失败关闭，
/// 避免把「清除后即可继续」当作访问控制边界。
fn parent_acl_inherits(parent: &File) -> Result<bool, AtomicFailureKind> {
    let Some(text) = read_acl(parent)? else {
        return Ok(false);
    };
    Ok(contains_byte_run(&text, b"file_inherit"))
}

/// 子串匹配；空针返回 false，避免 `windows(0)` 触发 panic。
fn contains_byte_run(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty()
        && haystack
            .windows(needle.len())
            .any(|window| window == needle)
}

/// 删除本操作刚创建的 staging；身份不符或删除失败时保留现场。
fn remove_created_staging(parent: &File, path: &Path, name: &CString, file: &File) {
    let owned = file.metadata().map(|metadata| identity(&metadata));
    let current = std::fs::symlink_metadata(path);
    let (Ok(owned), Ok(current)) = (owned, current) else {
        return;
    };
    if !current.is_file() || identity(&current) != owned {
        return;
    }
    // SAFETY: parent 在调用期间有效；name 是单一 NUL 结尾组件；调用前已核对路径
    // 身份为本操作的 inode，unlinkat 不跟随末端符号链接。删除失败只表示残余仍需
    // 宿主按失败记录携带的路径清理。
    let _ = unsafe { unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) };
}

/// 把目标的访问元数据复制到 staging，并立即逐项复验。
///
/// 复制顺序：owner/group → mode → 扩展 ACL → xattr。先改属主再改 mode，避免
/// `fchown` 清除 setuid/setgid 位。目标 flags 非 0 时拒绝：M2 不支持可能阻止
/// rename/清理的 chflags 语义，也不宣称已取证。
pub(crate) fn transfer_access_state(
    target: &File,
    staging: &File,
) -> Result<(), AtomicFailureKind> {
    let wanted = read_access_state(target)?;
    let current = read_access_state(staging)?;
    if wanted.flags != 0 || current.flags != 0 {
        return Err(AtomicFailureKind::ReplacementMetadataUnverified);
    }
    if current.uid != wanted.uid || current.gid != wanted.gid {
        // SAFETY: 两个 fd 在调用期间有效；fchown 只修改 staging 的属主。
        if unsafe { fchown(staging.as_raw_fd(), wanted.uid, wanted.gid) } != 0 {
            return Err(publish_io(
                AtomicStage::Permissions,
                io::Error::last_os_error(),
            ));
        }
    }
    staging
        .set_permissions(Permissions::from_mode(wanted.mode))
        .map_err(|source| publish_io(AtomicStage::Permissions, source))?;
    copy_acl(target, staging)?;
    for (name, value) in &wanted.xattrs {
        write_xattr(staging, name, value)?;
    }
    verify_access_state(target, staging)
}

/// 交换前复验：目标当前访问元数据仍与 staging 逐项一致。
///
/// 不一致时再次读取目标，以区分两类事实：目标在两次读取之间继续变化说明是外部
/// 改写，按 [`AtomicFailureKind::Conflict`] 结算；两次读取稳定而 staging 仍不一致
/// 说明是本操作复制失败，按 `ReplacementMetadataUnverified` 失败关闭，不误报为
/// 外部冲突。读取失败按平台权限/元数据能力失败处理。只比较目标拥有的 xattr，
/// 不要求 staging 没有额外项（staging 由本操作创建，正常路径不会引入）。
fn verify_access_state(target: &File, staging: &File) -> Result<(), AtomicFailureKind> {
    let wanted = read_access_state(target)?;
    let actual = read_access_state(staging)?;
    if same_access_state(&wanted, &actual) {
        return Ok(());
    }
    let again = read_access_state(target)?;
    if same_access_state(&wanted, &again) {
        Err(AtomicFailureKind::ReplacementMetadataUnverified)
    } else {
        Err(AtomicFailureKind::Conflict {
            target_exists: true,
        })
    }
}

/// M2 支持字段上的访问元数据一致；只要求 `wanted` 的 xattr 都存在于 `actual`。
fn same_access_state(wanted: &AccessState, actual: &AccessState) -> bool {
    wanted.mode == actual.mode
        && wanted.uid == actual.uid
        && wanted.gid == actual.gid
        && wanted.flags == actual.flags
        && wanted.acl == actual.acl
        && wanted.xattrs.iter().all(|(name, value)| {
            actual
                .xattrs
                .iter()
                .any(|(candidate, other)| candidate == name && other == value)
        })
}

/// 用 `renameatx_np(RENAME_SWAP)` 交换 staging 与目标，并核对旧目标 inode 与字节。
///
/// 调用前复验访问元数据，交换后要求 staging 路径持有调用方观察到的旧目标 inode，
/// 并完整回读旧目标字节与 `old_bytes` 对照，确认复验与交换之间没有同尺寸原地改写。
/// 任一项不符时再交换一次回滚，按外部改写返回 `Conflict`；回滚也失败时返回
/// `PublishedTargetUnverified`，表示实际副作用无法归属。
pub(crate) fn publish_existing(
    parent: &File,
    current: &File,
    staging_file: &File,
    staging: &Path,
    target: &Path,
    old_bytes: &[u8],
) -> Result<(), AtomicFailureKind> {
    let staging_name = path_component(staging, AtomicStage::Publish)?;
    let target_name = path_component(target, AtomicStage::Publish)?;
    verify_access_state(current, staging_file)?;
    let expected = identity(
        &current
            .metadata()
            .map_err(|source| publish_io(AtomicStage::Publish, source))?,
    );
    // 交换会把 staging 路径当前持有的 inode 放到目标路径；回滚复验以交换前
    // 目标路径实际持有的 inode 为基准（可能已不同于调用方观察）。
    let pre_swap_target = identity(&std::fs::symlink_metadata(target).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            AtomicFailureKind::Conflict {
                target_exists: false,
            }
        } else {
            publish_io(AtomicStage::Publish, source)
        }
    })?);
    // SAFETY: 两个路径组件都以 NUL 结尾且不越出已核验父目录 fd；RENAME_SWAP 不
    // 跟随末端符号链接，只交换同一目录中的两个目录项。
    if unsafe {
        renameatx_np(
            parent.as_raw_fd(),
            staging_name.as_ptr(),
            parent.as_raw_fd(),
            target_name.as_ptr(),
            RENAME_SWAP,
        )
    } != 0
    {
        return Err(classify_publish_error(io::Error::last_os_error(), target));
    }
    match verify_identity_at(staging, expected) {
        Ok(()) => match matches_file_bytes(current, old_bytes, AtomicStage::VerifyPublished) {
            Ok(true) => Ok(()),
            Ok(false) => rollback_swap(
                parent,
                &staging_name,
                &target_name,
                target,
                pre_swap_target,
                AtomicFailureKind::Conflict {
                    target_exists: true,
                },
            ),
            Err(kind) => rollback_swap(
                parent,
                &staging_name,
                &target_name,
                target,
                pre_swap_target,
                kind,
            ),
        },
        Err(_check) => rollback_swap(
            parent,
            &staging_name,
            &target_name,
            target,
            pre_swap_target,
            AtomicFailureKind::Conflict {
                target_exists: true,
            },
        ),
    }
}

/// 交换后核对失败时的回滚：再交换一次恢复原目录项，并复验目标路径。
///
/// `pre_swap_target` 是交换前目标路径的 inode；回滚成功后它应回到目标路径，
/// 此时按 `on_success` 结算原始失败。
fn rollback_swap(
    parent: &File,
    staging_name: &CString,
    target_name: &CString,
    target: &Path,
    pre_swap_target: (u64, u64),
    on_success: AtomicFailureKind,
) -> Result<(), AtomicFailureKind> {
    // SAFETY: 同 publish_existing；第二次交换恢复交换前的目录项布局。
    if unsafe {
        renameatx_np(
            parent.as_raw_fd(),
            staging_name.as_ptr(),
            parent.as_raw_fd(),
            target_name.as_ptr(),
            RENAME_SWAP,
        )
    } != 0
    {
        return Err(AtomicFailureKind::PublishedTargetUnverified {
            target: target.to_path_buf(),
            cause: Box::new(publish_io(AtomicStage::Publish, io::Error::last_os_error())),
        });
    }
    match verify_identity_at(target, pre_swap_target) {
        Ok(()) => {
            // 目录项已恢复：staging 路径仍归本操作用于清理。
            Err(on_success)
        }
        Err(source) => Err(AtomicFailureKind::PublishedTargetUnverified {
            target: target.to_path_buf(),
            cause: Box::new(publish_io(AtomicStage::VerifyPublished, source)),
        }),
    }
}

/// 读取目标访问元数据；任何读取失败都视为本阶段 I/O 失败并保留原目标。
fn read_access_state(file: &File) -> Result<AccessState, AtomicFailureKind> {
    let metadata = file
        .metadata()
        .map_err(|source| publish_io(AtomicStage::Permissions, source))?;
    Ok(AccessState {
        mode: metadata.mode() & 0o7777,
        uid: metadata.uid(),
        gid: metadata.gid(),
        flags: metadata.st_flags(),
        acl: read_acl(file)?,
        xattrs: read_xattrs(file)?,
    })
}

/// 读取扩展 ACL 文本；`ENOENT` 表示没有 ACL（探针实测的 Darwin 约定）。
fn read_acl(file: &File) -> Result<Option<Vec<u8>>, AtomicFailureKind> {
    // SAFETY: fd 由 file 持有；acl_get_fd_np 返回新的 ACL 句柄（NULL 表示无 ACL
    // 或错误），句柄由 AclHandle 独占并释放。
    let acl = unsafe { acl_get_fd_np(file.as_raw_fd(), ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        let source = io::Error::last_os_error();
        return if source.raw_os_error() == Some(ENOENT) {
            Ok(None)
        } else {
            Err(publish_io(AtomicStage::Permissions, source))
        };
    }
    let handle = AclHandle(acl);
    let mut len: SsizeT = 0;
    // SAFETY: 句柄非空且由 AclHandle 持有；len 是有效的输出参数；返回缓冲区
    // 由 AclText 独占并在复制后释放。
    let text = unsafe { acl_to_text(handle.0, &mut len) };
    if text.is_null() {
        return Err(publish_io(
            AtomicStage::Permissions,
            io::Error::last_os_error(),
        ));
    }
    let text = AclText(text);
    let len = usize::try_from(len).map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?;
    // SAFETY: acl_to_text 返回的缓冲区按 len 字节有效（不含结尾 NUL），文本指针
    // 非空；只读复制，不保留借用。
    let bytes = unsafe { std::slice::from_raw_parts(text.0.cast::<u8>(), len) }.to_vec();
    Ok(Some(bytes))
}

/// 清除扩展 ACL；空 ACL 在 Darwin 上等于没有 ACL（探针实测）。
fn clear_acl(file: &File) -> Result<(), AtomicFailureKind> {
    // SAFETY: acl_init(0) 返回新的空 ACL 句柄，非空时由 AclHandle 释放。
    let empty = unsafe { acl_init(0) };
    if empty.is_null() {
        return Err(publish_io(
            AtomicStage::Permissions,
            io::Error::last_os_error(),
        ));
    }
    let handle = AclHandle(empty);
    // SAFETY: 两个参数都有效；acl_set_fd_np 把句柄内容复制到 file 的 fd。
    if unsafe { acl_set_fd_np(file.as_raw_fd(), handle.0, ACL_TYPE_EXTENDED) } != 0 {
        return Err(publish_io(
            AtomicStage::Permissions,
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

/// 把目标的扩展 ACL 句柄复制到 staging；目标没有 ACL 时清空 staging。
fn copy_acl(target: &File, staging: &File) -> Result<(), AtomicFailureKind> {
    // SAFETY: target 的 fd 有效；返回新的 ACL 句柄，由 AclHandle 独占释放。
    let acl = unsafe { acl_get_fd_np(target.as_raw_fd(), ACL_TYPE_EXTENDED) };
    if acl.is_null() {
        let source = io::Error::last_os_error();
        return if source.raw_os_error() == Some(ENOENT) {
            clear_acl(staging)
        } else {
            Err(publish_io(AtomicStage::Permissions, source))
        };
    }
    let handle = AclHandle(acl);
    // SAFETY: 句柄来自 target 且仍有效；acl_set_fd_np 把内容复制进 staging 的 fd。
    if unsafe { acl_set_fd_np(staging.as_raw_fd(), handle.0, ACL_TYPE_EXTENDED) } != 0 {
        return Err(publish_io(
            AtomicStage::Permissions,
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

/// 读取目标全部扩展属性的名称和值；缓冲不足（ERANGE）时有界重试。
fn read_xattrs(file: &File) -> Result<Vec<(CString, Vec<u8>)>, AtomicFailureKind> {
    let mut result = Vec::new();
    for name in list_xattr_names(file)? {
        let value = read_xattr(file, &name)?;
        result.push((name, value));
    }
    Ok(result)
}

/// 列出扩展属性名称；列表在两次调用间变化时重新读取，超过重试上限失败关闭。
fn list_xattr_names(file: &File) -> Result<Vec<CString>, AtomicFailureKind> {
    let fd = file.as_raw_fd();
    for _ in 0..4 {
        // SAFETY: 空缓冲区探测所需长度；fd 有效，列表仅写入内核返回大小。
        let size = unsafe { flistxattr(fd, ptr::null_mut(), 0, 0) };
        if size < 0 {
            return Err(publish_io(
                AtomicStage::Permissions,
                io::Error::last_os_error(),
            ));
        }
        let size =
            usize::try_from(size).map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?;
        let mut buffer = Vec::new();
        buffer
            .try_reserve_exact(size)
            .map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?;
        buffer.resize(size, 0);
        if size == 0 {
            return Ok(Vec::new());
        }
        // SAFETY: buffer 至少有 size 字节可写；fd 有效。
        let read = unsafe { flistxattr(fd, buffer.as_mut_ptr().cast(), buffer.len(), 0) };
        if read < 0 {
            let source = io::Error::last_os_error();
            if source.raw_os_error() == Some(ERANGE) {
                continue;
            }
            return Err(publish_io(AtomicStage::Permissions, source));
        }
        let read =
            usize::try_from(read).map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?;
        if read > buffer.len() {
            return Err(AtomicFailureKind::ReplacementMetadataUnverified);
        }
        buffer.truncate(read);
        return parse_xattr_names(&buffer);
    }
    Err(AtomicFailureKind::ReplacementMetadataUnverified)
}

/// 解析 NUL 分隔的扩展属性名列表。
fn parse_xattr_names(buffer: &[u8]) -> Result<Vec<CString>, AtomicFailureKind> {
    let mut names = Vec::new();
    for part in buffer.split(|byte| *byte == 0) {
        if part.is_empty() {
            continue;
        }
        names.push(
            CString::new(part).map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?,
        );
    }
    Ok(names)
}

/// 读取单个扩展属性值；长度在两次调用间变化（ERANGE）时有界重试。
fn read_xattr(file: &File, name: &CString) -> Result<Vec<u8>, AtomicFailureKind> {
    let fd = file.as_raw_fd();
    for _ in 0..4 {
        // SAFETY: name 以 NUL 结尾；空缓冲区探测所需长度；fd 有效。
        let size = unsafe { fgetxattr(fd, name.as_ptr(), ptr::null_mut(), 0, 0, 0) };
        if size < 0 {
            return Err(publish_io(
                AtomicStage::Permissions,
                io::Error::last_os_error(),
            ));
        }
        let size =
            usize::try_from(size).map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?;
        let mut value = Vec::new();
        value
            .try_reserve_exact(size)
            .map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?;
        value.resize(size, 0);
        if size == 0 {
            return Ok(value);
        }
        // SAFETY: value 至少有 size 字节可写；name 以 NUL 结尾；fd 有效。
        let read = unsafe {
            fgetxattr(
                fd,
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if read < 0 {
            let source = io::Error::last_os_error();
            if source.raw_os_error() == Some(ERANGE) {
                continue;
            }
            return Err(publish_io(AtomicStage::Permissions, source));
        }
        let read =
            usize::try_from(read).map_err(|_| AtomicFailureKind::ReplacementMetadataUnverified)?;
        if read > value.len() {
            return Err(AtomicFailureKind::ReplacementMetadataUnverified);
        }
        value.truncate(read);
        return Ok(value);
    }
    Err(AtomicFailureKind::ReplacementMetadataUnverified)
}

/// 把单个扩展属性写到 staging。
fn write_xattr(file: &File, name: &CString, value: &[u8]) -> Result<(), AtomicFailureKind> {
    // SAFETY: name 以 NUL 结尾；value 指针与长度一致（空值用悬垂但长度 0 的
    // 有效指针）；fd 有效。fsetxattr 只读取缓冲区。
    if unsafe {
        fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    } != 0
    {
        return Err(publish_io(
            AtomicStage::Permissions,
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

/// 取路径末端组件为 NUL 结尾的 C 字符串；路径已由 `replace` 的父目录前提约束。
fn path_component(path: &Path, stage: AtomicStage) -> Result<CString, AtomicFailureKind> {
    let name = path.file_name().ok_or_else(|| {
        publish_io(
            stage,
            io::Error::new(io::ErrorKind::InvalidInput, "路径缺少末端组件"),
        )
    })?;
    CString::new(name.as_bytes()).map_err(|_| {
        publish_io(
            stage,
            io::Error::new(io::ErrorKind::InvalidInput, "路径组件包含 NUL"),
        )
    })
}

/// 路径当前指向的文件身份；用于交换后确认旧目标 inode 的位置。
fn verify_identity_at(path: &Path, expected: (u64, u64)) -> Result<(), io::Error> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file() || identity(&metadata) != expected {
        return Err(io::Error::other("目录项不再指向观察到的旧目标 inode"));
    }
    Ok(())
}

/// 交换后回读旧目标 inode 的字节，确认最终窗口内没有同尺寸原地改写。
///
/// `RENAME_SWAP` 会推进两个 inode 的 ctime（本机回归实测），交换前后的时间戳
/// 无法区分「交换本身」与外部写入，因此以完整字节对照为准。读取上限是期望
/// 长度加 1 字节；`file` 的偏移被复制描述符共享，调用方此后不再使用其偏移。
fn matches_file_bytes(
    file: &File,
    expected: &[u8],
    stage: AtomicStage,
) -> Result<bool, AtomicFailureKind> {
    let mut clone = file
        .try_clone()
        .map_err(|source| publish_io(stage, source))?;
    clone
        .seek(SeekFrom::Start(0))
        .map_err(|source| publish_io(stage, source))?;
    let mut scratch = [0; 4096];
    for chunk in expected.chunks(scratch.len()) {
        let buffer = &mut scratch[..chunk.len()];
        match clone.read_exact(buffer) {
            Ok(()) => {}
            Err(source) if source.kind() == io::ErrorKind::UnexpectedEof => return Ok(false),
            Err(source) => return Err(publish_io(stage, source)),
        }
        if chunk != buffer {
            return Ok(false);
        }
    }
    let mut extra = [0; 1];
    match clone.read(&mut extra) {
        Ok(0) => Ok(true),
        Ok(_) => Ok(false),
        Err(source) => Err(publish_io(stage, source)),
    }
}

/// 交换成功后删除 staging 路径上的旧目标 inode；路径身份不符时保留现场。
///
/// 删除经父目录 fd 的 `unlinkat` 进行，避免路径中间组件被重定向到其他目录。
/// 删除前核对路径仍指向交换前记录的旧目标 inode；不符时返回错误，由回执按
/// `LeftInPlace` 暴露，不删除不属于本操作的文件。
pub(crate) fn cleanup_staging(
    parent: &File,
    staging: &Path,
    expected: (u64, u64),
) -> io::Result<()> {
    let name = path_component(staging, AtomicStage::Publish)
        .map_err(|_| io::Error::other("staging 路径缺少可删除组件"))?;
    match std::fs::symlink_metadata(staging) {
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(source) => return Err(source),
        Ok(metadata) if !metadata.is_file() || identity(&metadata) != expected => {
            return Err(io::Error::other("staging 路径不再指向旧目标 inode，未删除"));
        }
        Ok(_) => {}
    }
    // SAFETY: parent 在调用期间有效；name 是单一 NUL 结尾组件；调用前已核对路径
    // 身份为交换前的旧目标 inode；unlinkat 不跟随末端符号链接。
    if unsafe { unlinkat(parent.as_raw_fd(), name.as_ptr(), 0) } != 0 {
        let source = io::Error::last_os_error();
        if source.kind() == io::ErrorKind::NotFound {
            return Ok(());
        }
        return Err(source);
    }
    Ok(())
}

/// Unix (dev, ino) 身份。
fn identity(metadata: &Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

/// 把交换系统调用错误分类为受检失败；`EXDEV` 等其余错误保留原始 I/O 分类。
///
/// `ENOENT` 只有在目标确实不存在时才结算为 `Conflict{target_exists:false}`；
/// 目标仍在而 staging 路径消失时保留原始 I/O 事实，不误导调用方丢弃内存基线。
fn classify_publish_error(source: io::Error, target: &Path) -> AtomicFailureKind {
    match source.raw_os_error() {
        Some(ENOENT) => match std::fs::symlink_metadata(target) {
            Ok(_) => publish_io(AtomicStage::Publish, source),
            Err(error) if error.kind() == io::ErrorKind::NotFound => AtomicFailureKind::Conflict {
                target_exists: false,
            },
            Err(_) => publish_io(AtomicStage::Publish, source),
        },
        Some(ENOTSUP) => AtomicFailureKind::ReplacementIdentityUnverified,
        _ => publish_io(AtomicStage::Publish, source),
    }
}

/// 访问元数据阶段的 I/O 失败；保留原始错误分类，不翻译为用户文案。
fn publish_io(stage: AtomicStage, source: io::Error) -> AtomicFailureKind {
    AtomicFailureKind::Io { stage, source }
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::OpenOptionsExt as _;
    use std::path::PathBuf;

    use super::*;

    type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

    /// 独占创建的临时文件；直接测试本模块的 xattr/ACL 原语。
    struct TempFile(PathBuf);

    impl TempFile {
        fn create(label: &str) -> TestResult<(Self, File)> {
            let base = std::env::temp_dir();
            let nanos = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos();
            let path = base.join(format!(
                "aline-platform-{}-{nanos}-{label}",
                std::process::id()
            ));
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)?;
            Ok((Self(path), file))
        }
    }

    impl Drop for TempFile {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    fn kind_error(kind: AtomicFailureKind) -> io::Error {
        io::Error::other(format!("{kind:?}"))
    }

    #[test]
    fn parse_xattr_names_handles_empty_and_trailing_parts() -> TestResult {
        let names = parse_xattr_names(b"").map_err(kind_error)?;
        assert!(names.is_empty());
        let names = parse_xattr_names(b"alpha\0\0beta\0").map_err(kind_error)?;
        assert_eq!(names.len(), 2);
        assert_eq!(names[0].to_str()?, "alpha");
        assert_eq!(names[1].to_str()?, "beta");
        let split = parse_xattr_names(b"al\0pha").map_err(kind_error)?;
        assert_eq!(split.len(), 2, "NUL 分隔的名字各段独立解析");
        assert_eq!(split[1].to_str()?, "pha");
        Ok(())
    }

    #[test]
    fn xattr_roundtrip_preserves_empty_and_binary_values() -> TestResult {
        let (_temp, file) = TempFile::create("xattr")?;
        let empty = CString::new("com.aline.empty")?;
        write_xattr(&file, &empty, &[]).map_err(kind_error)?;
        assert!(read_xattr(&file, &empty).map_err(kind_error)?.is_empty());
        let binary = CString::new("com.aline.binary")?;
        write_xattr(&file, &binary, b"\x00\xff\x01").map_err(kind_error)?;
        assert_eq!(
            read_xattr(&file, &binary).map_err(kind_error)?,
            b"\x00\xff\x01"
        );
        let names = list_xattr_names(&file).map_err(kind_error)?;
        assert!(
            names
                .iter()
                .any(|name| name.as_bytes() == b"com.aline.empty")
        );
        assert!(
            names
                .iter()
                .any(|name| name.as_bytes() == b"com.aline.binary")
        );
        Ok(())
    }

    #[test]
    fn acl_clear_leaves_no_acl_on_a_fresh_file() -> TestResult {
        let (_temp, file) = TempFile::create("acl")?;
        assert!(read_acl(&file).map_err(kind_error)?.is_none());
        clear_acl(&file).map_err(kind_error)?;
        assert!(read_acl(&file).map_err(kind_error)?.is_none());
        Ok(())
    }

    #[test]
    fn contains_byte_run_rejects_empty_needle_without_panicking() {
        assert!(!contains_byte_run(b"file_inherit", b""));
        assert!(contains_byte_run(
            b"allow read,file_inherit",
            b"file_inherit"
        ));
        assert!(!contains_byte_run(
            b"allow read,directory_inherit",
            b"file_inherit"
        ));
    }
}
