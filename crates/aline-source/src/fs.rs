//! 恢复存储所需的最小平台文件原语：目录同步、独占目录创建、非阻塞文件锁、
//! 同目录 staging 的无覆盖发布、恢复控制文件替换与受检 rename。
//! 仅使用标准库；macOS 是唯一实现平台，其他平台每个函数都返回
//! [`FsError::UnsupportedPlatform`]，不写盘、不清理、不伪造成功。
//!
//! 本模块只服务应用管理的恢复根；`replace_control_file` 不用于用户文档，
//! 不迁移 ACL/xattr/mode，`rename_no_clobber_checked` 只提供协作实例内的
//! no-clobber 语义，不是跨进程 CAS。

use std::fs::File;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use std::fs::{self, OpenOptions};
#[cfg(target_os = "macos")]
use std::io::{self, Write};
#[cfg(target_os = "macos")]
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

/// 一次受检操作观察到的文件身份；只比较，不持久化。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileIdentity {
    /// 文件系统设备号。
    pub dev: u64,
    /// 文件系统内 inode 号。
    pub ino: u64,
}

/// 失败所属的操作阶段；用于把 I/O 错误带回合法的调用边界。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FsOperation {
    /// 打开目录句柄。
    OpenDirectory,
    /// 同步目录项。
    SyncDirectory,
    /// 独占创建目录。
    CreateDirectory,
    /// 打开既有文件。
    OpenFile,
    /// 创建并写入 staging 文件。
    CreateStaging,
    /// 无覆盖发布 staging。
    Publish,
    /// 替换恢复控制文件。
    ReplaceControl,
    /// 受检 rename。
    Rename,
    /// 读取元数据。
    Metadata,
    /// 删除 staging 或 trash 内容。
    Remove,
}

/// 平台文件原语失败。分类保留“平台不支持/路径/独占/锁占用/身份变化”等可
/// 决策差异；原始 `io::ErrorKind` 只用于非特判的 I/O 失败。
#[derive(Debug)]
pub enum FsError {
    /// 当前平台没有实现；调用方必须失败关闭。
    UnsupportedPlatform,
    /// 路径不是绝对路径，或 staging 与目标不同父目录。
    InvalidPath,
    /// 指定操作的一般 I/O 失败。
    Io {
        /// 失败阶段。
        operation: FsOperation,
        /// 原始错误分类。
        kind: std::io::ErrorKind,
    },
    /// 目标已存在且本原语不覆盖。
    AlreadyExists,
    /// 非阻塞锁被其他句柄或进程占用。
    WouldBlock,
    /// 路径不存在。
    NotFound,
    /// 目标不是普通文件或目录。
    NotOrdinary,
    /// 目标是符号链接。
    Symlink,
    /// 观察到的身份与调用方持有的预期身份不符。
    IdentityChanged,
}

/// 已创建并完整写入、但尚未发布的 staging 文件。
pub struct StagedFile {
    path: PathBuf,
    identity: FileIdentity,
    len_bytes: u64,
}

impl StagedFile {
    /// staging 的绝对路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// staging 创建时观察到的身份。
    pub fn identity(&self) -> FileIdentity {
        self.identity
    }

    /// staging 写入的字节数。
    pub fn len_bytes(&self) -> u64 {
        self.len_bytes
    }
}

#[cfg(target_os = "macos")]
fn absolute(path: &Path) -> Result<(), FsError> {
    if path.is_absolute() {
        Ok(())
    } else {
        Err(FsError::InvalidPath)
    }
}

#[cfg(target_os = "macos")]
fn map_io(operation: FsOperation, error: &io::Error) -> FsError {
    match error.kind() {
        io::ErrorKind::NotFound => FsError::NotFound,
        io::ErrorKind::AlreadyExists => FsError::AlreadyExists,
        io::ErrorKind::WouldBlock => FsError::WouldBlock,
        kind => FsError::Io { operation, kind },
    }
}

#[cfg(target_os = "macos")]
fn identity_of(metadata: &fs::Metadata) -> FileIdentity {
    FileIdentity {
        dev: metadata.dev(),
        ino: metadata.ino(),
    }
}

#[cfg(target_os = "macos")]
fn ordinary_metadata(path: &Path, want_directory: bool) -> Result<fs::Metadata, FsError> {
    absolute(path)?;
    let metadata =
        fs::symlink_metadata(path).map_err(|error| map_io(FsOperation::Metadata, &error))?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return Err(FsError::Symlink);
    }
    let ordinary = if want_directory {
        file_type.is_dir()
    } else {
        file_type.is_file()
    };
    if !ordinary {
        return Err(FsError::NotOrdinary);
    }
    Ok(metadata)
}

/// 打开目录句柄并对其调用 `sync_all`，把目录项确立为耐久事实。
pub fn sync_directory(path: &Path) -> Result<(), FsError> {
    #[cfg(target_os = "macos")]
    {
        ordinary_metadata(path, true)?;
        let directory =
            File::open(path).map_err(|error| map_io(FsOperation::OpenDirectory, &error))?;
        directory
            .sync_all()
            .map_err(|error| map_io(FsOperation::SyncDirectory, &error))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(FsError::UnsupportedPlatform)
    }
}

/// 以 `create_dir` 独占创建目录并设置 mode 0700；已存在返回 `AlreadyExists`。
pub fn create_directory_exclusive(path: &Path) -> Result<FileIdentity, FsError> {
    #[cfg(target_os = "macos")]
    {
        absolute(path)?;
        fs::create_dir(path).map_err(|error| map_io(FsOperation::CreateDirectory, &error))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|error| map_io(FsOperation::CreateDirectory, &error))?;
        verify_ordinary_directory(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(FsError::UnsupportedPlatform)
    }
}

/// 打开（必要时以 mode 0600 独占创建）锁文件；返回未加锁的读写句柄。
/// 已存在的目标必须是普通文件，符号链接与非普通项拒绝。
pub fn open_lock_file(path: &Path) -> Result<File, FsError> {
    #[cfg(target_os = "macos")]
    {
        absolute(path)?;
        match fs::symlink_metadata(path) {
            Ok(_) => {
                ordinary_metadata(path, false)?;
                OpenOptions::new()
                    .read(true)
                    .write(true)
                    .open(path)
                    .map_err(|error| map_io(FsOperation::OpenFile, &error))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)
                {
                    Ok(file) => {
                        drop(file);
                        OpenOptions::new()
                            .read(true)
                            .write(true)
                            .open(path)
                            .map_err(|error| map_io(FsOperation::OpenFile, &error))
                    }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        ordinary_metadata(path, false)?;
                        OpenOptions::new()
                            .read(true)
                            .write(true)
                            .open(path)
                            .map_err(|error| map_io(FsOperation::OpenFile, &error))
                    }
                    Err(error) => Err(map_io(FsOperation::OpenFile, &error)),
                }
            }
            Err(error) => Err(map_io(FsOperation::Metadata, &error)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(FsError::UnsupportedPlatform)
    }
}

/// 对已打开句柄尝试标准库非阻塞独占锁；占用返回 `WouldBlock`。
pub fn try_lock_exclusive(file: &File) -> Result<(), FsError> {
    #[cfg(target_os = "macos")]
    {
        match file.try_lock() {
            Ok(()) => Ok(()),
            Err(fs::TryLockError::WouldBlock) => Err(FsError::WouldBlock),
            Err(fs::TryLockError::Error(error)) => Err(map_io(FsOperation::OpenFile, &error)),
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = file;
        Err(FsError::UnsupportedPlatform)
    }
}

/// 在 `directory` 内独占创建 staging 并完整写入 `bytes`，随后 `sync_all`。
/// 最多尝试 64 个候选名；仍冲突返回 `AlreadyExists`。文件 mode 0600。
pub fn create_staging(directory: &Path, prefix: &str, bytes: &[u8]) -> Result<StagedFile, FsError> {
    #[cfg(target_os = "macos")]
    {
        ordinary_metadata(directory, true)?;
        if prefix.is_empty() || prefix.contains('/') {
            return Err(FsError::InvalidPath);
        }
        for attempt in 0..64_u32 {
            let candidate = directory.join(format!("{prefix}.{attempt:02}.staging"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&candidate)
            {
                Ok(mut file) => {
                    file.write_all(bytes)
                        .map_err(|error| map_io(FsOperation::CreateStaging, &error))?;
                    file.sync_all()
                        .map_err(|error| map_io(FsOperation::CreateStaging, &error))?;
                    let metadata = file
                        .metadata()
                        .map_err(|error| map_io(FsOperation::Metadata, &error))?;
                    return Ok(StagedFile {
                        path: candidate,
                        identity: identity_of(&metadata),
                        len_bytes: bytes.len() as u64,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(map_io(FsOperation::CreateStaging, &error)),
            }
        }
        Err(FsError::AlreadyExists)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (directory, prefix, bytes);
        Err(FsError::UnsupportedPlatform)
    }
}

/// 以硬链接把 staging 发布到同父目录的不存在目标；目标身份必须等于 staging
/// 身份，成功后同步父目录。失败不删除 staging。
pub fn publish_staged_new(staged: &StagedFile, target: &Path) -> Result<FileIdentity, FsError> {
    #[cfg(target_os = "macos")]
    {
        absolute(target)?;
        let parent = target.parent().ok_or(FsError::InvalidPath)?;
        if staged.path.parent() != Some(parent) {
            return Err(FsError::InvalidPath);
        }
        match fs::symlink_metadata(target) {
            Ok(_) => return Err(FsError::AlreadyExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(map_io(FsOperation::Metadata, &error)),
        }
        fs::hard_link(&staged.path, target)
            .map_err(|error| map_io(FsOperation::Publish, &error))?;
        let identity = file_identity(target)?;
        if identity != staged.identity {
            return Err(FsError::IdentityChanged);
        }
        sync_directory(parent)?;
        Ok(identity)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (staged, target);
        Err(FsError::UnsupportedPlatform)
    }
}

/// 删除 staging；只接受由 [`create_staging`] 返回的句柄。
pub fn discard_staging(staged: StagedFile) -> Result<(), FsError> {
    #[cfg(target_os = "macos")]
    {
        fs::remove_file(&staged.path).map_err(|error| map_io(FsOperation::Remove, &error))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = staged;
        Err(FsError::UnsupportedPlatform)
    }
}

/// 替换恢复控制文件（例如 `next-id`），不用于用户文档；不迁移 ACL/xattr/mode。
///
/// 调用方须传入刚读取并复验的既有目标身份；目标身份不符即返回
/// `IdentityChanged`，防止在读取与替换之间被其他实例改写。目标必须存在且为
/// 普通文件；同目录 staging 完成 `sync_all` 后以 `rename` 覆盖，再同步父目录
/// 并核验新身份等于 staging 身份。
pub fn replace_control_file(
    target: &Path,
    expected: FileIdentity,
    bytes: &[u8],
) -> Result<FileIdentity, FsError> {
    #[cfg(target_os = "macos")]
    {
        absolute(target)?;
        let current = verify_ordinary_file(target)?;
        if current != expected {
            return Err(FsError::IdentityChanged);
        }
        let parent = target.parent().ok_or(FsError::InvalidPath)?;
        let name = target
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(FsError::InvalidPath)?;
        let staged = create_staging(parent, &format!(".{name}.control"), bytes)?;
        fs::rename(&staged.path, target)
            .map_err(|error| map_io(FsOperation::ReplaceControl, &error))?;
        sync_directory(parent)?;
        let identity = file_identity(target)?;
        if identity != staged.identity {
            return Err(FsError::IdentityChanged);
        }
        Ok(identity)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (target, expected, bytes);
        Err(FsError::UnsupportedPlatform)
    }
}

/// 协作实例内的 no-clobber 移动：先确认目标不存在，再 `rename`，最后核验
/// `from` 消失且 `to` 的 dev+ino 等于 `expected`，并同步两个父目录。
///
/// 依据 W4-52，预检与 `rename` 之间存在非协作进程的竞争窗口，post-verify 也
/// 不能检测旧目标被覆盖；本函数不是跨进程 CAS，只保证遵守本协议的 Aline
/// 实例之间的语义。
pub fn rename_no_clobber_checked(
    from: &Path,
    to: &Path,
    expected: FileIdentity,
) -> Result<(), FsError> {
    #[cfg(target_os = "macos")]
    {
        absolute(from)?;
        absolute(to)?;
        match fs::symlink_metadata(to) {
            Ok(_) => return Err(FsError::AlreadyExists),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(map_io(FsOperation::Metadata, &error)),
        }
        fs::rename(from, to).map_err(|error| map_io(FsOperation::Rename, &error))?;
        match fs::symlink_metadata(from) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Ok(_) => return Err(FsError::IdentityChanged),
            Err(error) => return Err(map_io(FsOperation::Metadata, &error)),
        }
        if file_identity(to)? != expected {
            return Err(FsError::IdentityChanged);
        }
        let from_parent = from.parent().ok_or(FsError::InvalidPath)?;
        let to_parent = to.parent().ok_or(FsError::InvalidPath)?;
        sync_directory(from_parent)?;
        sync_directory(to_parent)?;
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (from, to, expected);
        Err(FsError::UnsupportedPlatform)
    }
}

/// 读取路径的 `symlink_metadata` 身份；符号链接返回其自身身份。
pub fn file_identity(path: &Path) -> Result<FileIdentity, FsError> {
    #[cfg(target_os = "macos")]
    {
        absolute(path)?;
        let metadata =
            fs::symlink_metadata(path).map_err(|error| map_io(FsOperation::Metadata, &error))?;
        Ok(identity_of(&metadata))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(FsError::UnsupportedPlatform)
    }
}

/// 要求路径是普通文件（非符号链接），返回其身份。
pub fn verify_ordinary_file(path: &Path) -> Result<FileIdentity, FsError> {
    #[cfg(target_os = "macos")]
    {
        ordinary_metadata(path, false).map(|metadata| identity_of(&metadata))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(FsError::UnsupportedPlatform)
    }
}

/// 要求路径是普通目录（非符号链接），返回其身份。
pub fn verify_ordinary_directory(path: &Path) -> Result<FileIdentity, FsError> {
    #[cfg(target_os = "macos")]
    {
        ordinary_metadata(path, true).map(|metadata| identity_of(&metadata))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(FsError::UnsupportedPlatform)
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::error::Error;
    use std::io;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn ok<T>(result: Result<T, FsError>) -> Result<T, Box<dyn Error>> {
        result.map_err(|error| format!("{error:?}").into())
    }

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn create() -> Result<Self, Box<dyn Error>> {
            // 引擎测试禁止可变静态计数器，改用纳秒时间戳避让；冲突时重试。
            for _ in 0..64 {
                let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
                let path = std::env::temp_dir()
                    .join(format!("aline-fs-test-{}-{nanos}", std::process::id()));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(Box::new(error)),
                }
            }
            Err("临时目录名连续冲突".into())
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn fs_sync_directory_rejects_file_and_missing() -> Result<(), Box<dyn Error>> {
        let directory = TempDir::create()?;
        ok(sync_directory(directory.path()))?;
        let file = directory.path().join("file");
        fs::write(&file, b"x")?;
        assert!(matches!(sync_directory(&file), Err(FsError::NotOrdinary)));
        assert!(matches!(
            sync_directory(&directory.path().join("missing")),
            Err(FsError::NotFound)
        ));
        assert!(matches!(
            sync_directory(Path::new("relative")),
            Err(FsError::InvalidPath)
        ));
        Ok(())
    }

    #[test]
    fn fs_create_directory_exclusive_is_exclusive() -> Result<(), Box<dyn Error>> {
        let temporary = TempDir::create()?;
        let path = temporary.path().join("owned");
        let identity = ok(create_directory_exclusive(&path))?;
        assert_eq!(identity, ok(verify_ordinary_directory(&path))?);
        assert!(matches!(
            create_directory_exclusive(&path),
            Err(FsError::AlreadyExists)
        ));
        let mode = fs::metadata(&path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o700);
        Ok(())
    }

    #[test]
    fn fs_try_lock_is_nonblocking_and_exclusive() -> Result<(), Box<dyn Error>> {
        let temporary = TempDir::create()?;
        let path = temporary.path().join("owner.lock");
        let first = ok(open_lock_file(&path))?;
        ok(try_lock_exclusive(&first))?;
        let second = ok(open_lock_file(&path))?;
        assert!(matches!(
            try_lock_exclusive(&second),
            Err(FsError::WouldBlock)
        ));
        drop(first);
        ok(try_lock_exclusive(&second))?;
        let mode = fs::metadata(&path)?.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        Ok(())
    }

    #[test]
    fn fs_publish_staged_new_rejects_existing_target_and_keeps_same_identity()
    -> Result<(), Box<dyn Error>> {
        let temporary = TempDir::create()?;
        let staged = ok(create_staging(temporary.path(), "payload", b"hello"))?;
        let original = staged.identity();
        assert_eq!(staged.len_bytes(), 5);
        let target = temporary.path().join("manifest");
        let published = ok(publish_staged_new(&staged, &target))?;
        assert_eq!(published, original);
        assert_eq!(fs::read(&target)?, b"hello");
        let other = ok(create_staging(temporary.path(), "payload", b"again"))?;
        assert!(matches!(
            publish_staged_new(&other, &target),
            Err(FsError::AlreadyExists)
        ));
        assert!(other.path().exists());
        let other_path = other.path().to_path_buf();
        ok(discard_staging(other))?;
        assert!(!other_path.exists());
        Ok(())
    }

    #[test]
    fn fs_replace_control_file_replaces_regular_file_and_rejects_symlink()
    -> Result<(), Box<dyn Error>> {
        let temporary = TempDir::create()?;
        let target = temporary.path().join("next-id");
        fs::write(&target, b"old")?;
        let expected = ok(verify_ordinary_file(&target))?;
        let replaced = ok(replace_control_file(&target, expected, b"new"))?;
        assert_eq!(fs::read(&target)?, b"new");
        assert_ne!(replaced, expected);
        assert_eq!(replaced, ok(verify_ordinary_file(&target))?);
        assert!(matches!(
            replace_control_file(&target, expected, b"stale"),
            Err(FsError::IdentityChanged)
        ));
        assert!(matches!(
            replace_control_file(&temporary.path().join("missing"), expected, b"x"),
            Err(FsError::NotFound)
        ));
        let link = temporary.path().join("link");
        std::os::unix::fs::symlink(&target, &link)?;
        assert!(matches!(
            replace_control_file(&link, expected, b"x"),
            Err(FsError::Symlink)
        ));
        Ok(())
    }

    #[test]
    fn fs_replace_control_file_rejects_changed_expected_identity() -> Result<(), Box<dyn Error>> {
        let temporary = TempDir::create()?;
        let target = temporary.path().join("next-id");
        fs::write(&target, b"old")?;
        let expected = ok(verify_ordinary_file(&target))?;
        let changed = FileIdentity {
            dev: expected.dev,
            ino: expected.ino.wrapping_add(1),
        };
        assert!(matches!(
            replace_control_file(&target, changed, b"new"),
            Err(FsError::IdentityChanged)
        ));
        assert_eq!(fs::read(&target)?, b"old");
        Ok(())
    }

    #[test]
    fn fs_rename_no_clobber_checked_rejects_existing_and_preserves_identity()
    -> Result<(), Box<dyn Error>> {
        let temporary = TempDir::create()?;
        let from = temporary.path().join("from");
        fs::write(&from, b"bytes")?;
        let expected = ok(verify_ordinary_file(&from))?;
        let to = temporary.path().join("to");
        ok(rename_no_clobber_checked(&from, &to, expected))?;
        assert!(!from.exists());
        assert_eq!(ok(verify_ordinary_file(&to))?, expected);
        assert_eq!(fs::read(&to)?, b"bytes");
        assert!(matches!(
            rename_no_clobber_checked(&from, &temporary.path().join("missing-target"), expected),
            Err(FsError::NotFound)
        ));
        let existing_from = temporary.path().join("existing-from");
        fs::write(&existing_from, b"x")?;
        assert!(matches!(
            rename_no_clobber_checked(&existing_from, &to, expected),
            Err(FsError::AlreadyExists)
        ));
        let moved = temporary.path().join("moved");
        let wrong = FileIdentity {
            dev: expected.dev,
            ino: expected.ino.wrapping_add(1),
        };
        assert!(matches!(
            rename_no_clobber_checked(&existing_from, &moved, wrong),
            Err(FsError::IdentityChanged)
        ));
        Ok(())
    }
}

#[cfg(all(test, not(target_os = "macos")))]
mod non_macos_tests {
    use std::error::Error;
    use std::fs::File;
    use std::path::Path;

    use super::*;

    #[test]
    fn fs_non_macos_fail_closed() -> Result<(), Box<dyn Error>> {
        let path = Path::new("/aline-fs-non-macos");
        assert!(matches!(
            sync_directory(path),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            create_directory_exclusive(path),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            open_lock_file(path),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            create_staging(path, "prefix", b"bytes"),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            replace_control_file(path, FileIdentity { dev: 0, ino: 0 }, b"bytes"),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            rename_no_clobber_checked(path, path, FileIdentity { dev: 0, ino: 0 }),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            file_identity(path),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            verify_ordinary_file(path),
            Err(FsError::UnsupportedPlatform)
        ));
        assert!(matches!(
            verify_ordinary_directory(path),
            Err(FsError::UnsupportedPlatform)
        ));
        let file = File::open("/")?;
        assert!(matches!(
            try_lock_exclusive(&file),
            Err(FsError::UnsupportedPlatform)
        ));
        Ok(())
    }
}
