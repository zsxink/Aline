//! 临时资源根：按用户注入的 HOME 推导固定目录，受检创建两级目录，并为每个
//! 文档独占分配一个 16 位十六进制目录。仅 macOS 实现；其他平台失败关闭，
//! 不创建、不清理任何目录。本模块不读时钟，候选纳秒由调用方注入。

use std::path::{Path, PathBuf};

use aline_source::fs::{self as platform_fs, FsError};

/// 应用临时目录在 `Aline` 下的固定名称。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) const TEMPORARY_DIR: &str = "temporary";
/// 文档目录分配的最大连续尝试次数。
#[cfg_attr(not(test), allow(dead_code))]
const MAX_ALLOCATION_ATTEMPTS: u32 = 8;

/// 由 HOME 推导默认临时根；不访问磁盘。
#[allow(dead_code)]
pub(crate) fn default_temp_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Aline/temporary")
}

/// 临时文档目录身份；只由分配结果或恢复材料解析得到。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct TempDocumentId(u64);

#[cfg_attr(not(test), allow(dead_code))]
impl TempDocumentId {
    /// 返回原始数值，供 locator 编码或诊断。
    pub(crate) const fn value(self) -> u64 {
        self.0
    }

    /// 从既有 locator/恢复材料解析出的原始值重建身份。
    pub(crate) const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    /// 固定 16 位小写十六进制目录名。
    pub(crate) fn directory_name(self) -> String {
        format!("{:016x}", self.0)
    }
}

/// 已受检的临时根：绝对路径、普通目录、非符号链接，且 uid 与 HOME 一致。
#[derive(Clone, Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct TempRoot {
    path: PathBuf,
    owner_uid: u32,
}

#[cfg_attr(not(test), allow(dead_code))]
impl TempRoot {
    /// 返回临时根绝对路径。
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// 返回期望的属主 uid；恢复材料写回时复验既有文档目录。
    pub(super) const fn owner_uid(&self) -> u32 {
        self.owner_uid
    }

    /// 受检打开或创建临时根。`home` 必须绝对、既有且为普通目录；只创建
    /// `Library/Application Support` 之下已有的 `Aline` 与 `temporary` 两级，
    /// 不创建更上层目录。
    pub(crate) fn open_or_create(home: &Path) -> Result<Self, TempRootError> {
        #[cfg(target_os = "macos")]
        {
            use std::fs;
            use std::os::unix::fs::MetadataExt;

            if !home.is_absolute() {
                return Err(TempRootError::InvalidPath);
            }
            let home_metadata = fs::symlink_metadata(home).map_err(map_io)?;
            if home_metadata.file_type().is_symlink() {
                return Err(TempRootError::Symlink);
            }
            if !home_metadata.is_dir() {
                return Err(TempRootError::NotDirectory);
            }
            let owner_uid = home_metadata.uid();
            let base = home.join("Library/Application Support");
            let base_metadata = fs::symlink_metadata(&base).map_err(map_io)?;
            if base_metadata.file_type().is_symlink() {
                return Err(TempRootError::Symlink);
            }
            if !base_metadata.is_dir() {
                return Err(TempRootError::NotDirectory);
            }
            if base_metadata.uid() != owner_uid {
                return Err(TempRootError::ForeignOwner);
            }
            let app = base.join("Aline");
            create_or_verify(&app, owner_uid)?;
            let temporary = app.join(TEMPORARY_DIR);
            create_or_verify(&temporary, owner_uid)?;
            Ok(Self {
                path: temporary,
                owner_uid,
            })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = home;
            Err(TempRootError::UnsupportedPlatform)
        }
    }

    /// 从 `candidate_nanos` 起最多尝试 8 个连续目录名；独占创建成功即返回。
    /// 已存在的目录一律不删除、不覆盖，继续尝试下一个。
    pub(crate) fn allocate_document(
        &self,
        candidate_nanos: u64,
    ) -> Result<(TempDocumentId, PathBuf), TempRootError> {
        #[cfg(target_os = "macos")]
        {
            let mut value = candidate_nanos;
            for _ in 0..MAX_ALLOCATION_ATTEMPTS {
                let id = TempDocumentId(value);
                let path = self.path.join(id.directory_name());
                match platform_fs::create_directory_exclusive(&path) {
                    Ok(_) => {
                        if platform_fs::sync_directory(&self.path).is_err() {
                            // 同步失败时尽力撤回刚创建的目录，避免留下无法归属的材料。
                            let _ = std::fs::remove_dir(&path);
                            return Err(TempRootError::Io);
                        }
                        return Ok((id, path));
                    }
                    Err(FsError::AlreadyExists) => {
                        value = value.wrapping_add(1);
                    }
                    Err(error) => return Err(map_fs(error)),
                }
            }
            Err(TempRootError::AlreadyExists)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (self, candidate_nanos);
            Err(TempRootError::UnsupportedPlatform)
        }
    }
}

#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(dead_code))]
fn create_or_verify(path: &Path, owner_uid: u32) -> Result<(), TempRootError> {
    match platform_fs::create_directory_exclusive(path) {
        Ok(_) => {
            let parent = path.parent().ok_or(TempRootError::InvalidPath)?;
            platform_fs::sync_directory(parent).map_err(map_fs)?;
            Ok(())
        }
        Err(FsError::AlreadyExists) => verify_directory(path, owner_uid),
        Err(error) => Err(map_fs(error)),
    }
}

#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(dead_code))]
fn verify_directory(path: &Path, owner_uid: u32) -> Result<(), TempRootError> {
    use std::fs;
    use std::os::unix::fs::MetadataExt;

    let metadata = fs::symlink_metadata(path).map_err(map_io)?;
    if metadata.file_type().is_symlink() {
        return Err(TempRootError::Symlink);
    }
    if !metadata.is_dir() {
        return Err(TempRootError::NotDirectory);
    }
    if metadata.uid() != owner_uid {
        return Err(TempRootError::ForeignOwner);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(dead_code))]
fn map_io(error: std::io::Error) -> TempRootError {
    if error.kind() == std::io::ErrorKind::NotFound {
        TempRootError::InvalidPath
    } else {
        TempRootError::Io
    }
}

#[cfg(target_os = "macos")]
#[cfg_attr(not(test), allow(dead_code))]
fn map_fs(error: FsError) -> TempRootError {
    match error {
        FsError::UnsupportedPlatform => TempRootError::UnsupportedPlatform,
        FsError::InvalidPath | FsError::NotFound => TempRootError::InvalidPath,
        FsError::NotOrdinary => TempRootError::NotDirectory,
        FsError::Symlink => TempRootError::Symlink,
        FsError::AlreadyExists => TempRootError::AlreadyExists,
        FsError::WouldBlock | FsError::IdentityChanged | FsError::Io { .. } => TempRootError::Io,
    }
}

/// 临时根创建/分配失败；所有变体都不留下受管材料之外的副作用。
#[derive(Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum TempRootError {
    UnsupportedPlatform,
    InvalidPath,
    NotDirectory,
    Symlink,
    ForeignOwner,
    AlreadyExists,
    Io,
}
