//! 恢复根验证与稳定文档身份。根由宿主显式注入；本模块只做受检打开、
//! 子项所有权/类型复验和 16 位小写十六进制身份名解析，不创建生产根、
//! 不读取恢复内容。
//!
//! macOS 是唯一支持平台：其他平台的根打开与子项验证失败关闭，不写盘。

// 4.1a 只提供根验证与身份类型；生产根接线（4.2）到来前部分访问器仅由
// crate 内测试使用。该豁免只覆盖本模块。
#![cfg_attr(not(test), allow(dead_code))]

use std::fs;
use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

#[cfg(target_os = "macos")]
use std::os::unix::fs::MetadataExt;

use super::store::RecoveryDefect;

/// 应用管理恢复目录的固定子目录名；生产根由 4.2 接线创建。
pub(crate) const RECOVERY_DIR: &str = "recovery";

/// 由 HOME 推导默认恢复根；不访问磁盘。
pub(crate) fn default_recovery_root(home: &Path) -> PathBuf {
    home.join("Library/Application Support/Aline/recovery")
}

/// 已复验的恢复根：绝对路径、普通目录、非符号链接，且 uid 与 HOME 一致。
#[derive(Clone, Debug)]
pub(crate) struct RecoveryRoot {
    path: PathBuf,
    owner_uid: u32,
}

impl RecoveryRoot {
    /// 受检打开既有恢复根。根缺失时拒绝而不创建；生产根由 4.2 创建。
    pub(crate) fn open(path: &Path) -> Result<Self, RootError> {
        #[cfg(target_os = "macos")]
        {
            let home = std::env::var_os("HOME").ok_or(RootError::HomeUnavailable)?;
            let home = PathBuf::from(home);
            if home.as_os_str().is_empty() {
                return Err(RootError::HomeUnavailable);
            }
            let home_metadata = fs::metadata(&home).map_err(|_| RootError::HomeUnavailable)?;
            let home_uid = home_metadata.uid();
            Self::open_with_expected_uid_inner(path, home_uid)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            Err(RootError::UnsupportedPlatform)
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn v1(&self) -> PathBuf {
        self.path.join("v1")
    }

    /// 验证 `path` 是根内的普通文件或目录、非符号链接，且 uid 等于根所有者。
    pub(crate) fn verify_child(&self, path: &Path) -> Result<(), RootError> {
        #[cfg(target_os = "macos")]
        {
            if !path.is_absolute() || !path.starts_with(&self.path) {
                return Err(RootError::InvalidPath);
            }
            let metadata = fs::symlink_metadata(path).map_err(map_io)?;
            let file_type = metadata.file_type();
            if file_type.is_symlink() {
                return Err(RootError::Symlink);
            }
            if !file_type.is_file() && !file_type.is_dir() {
                return Err(RootError::NotDirectory);
            }
            let actual = metadata.uid();
            if actual != self.owner_uid {
                return Err(RootError::ForeignOwner {
                    expected: self.owner_uid,
                    actual,
                });
            }
            Ok(())
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = path;
            Err(RootError::UnsupportedPlatform)
        }
    }

    /// 仅供测试：以显式期望 uid 打开根，验证外部所有者拒绝路径。
    #[cfg(test)]
    pub(crate) fn open_with_expected_uid(path: &Path, uid: u32) -> Result<Self, RootError> {
        #[cfg(target_os = "macos")]
        {
            Self::open_with_expected_uid_inner(path, uid)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (path, uid);
            Err(RootError::UnsupportedPlatform)
        }
    }

    #[cfg(target_os = "macos")]
    fn open_with_expected_uid_inner(path: &Path, expected_uid: u32) -> Result<Self, RootError> {
        if !path.is_absolute() {
            return Err(RootError::InvalidPath);
        }
        let metadata = fs::symlink_metadata(path).map_err(map_io)?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() {
            return Err(RootError::Symlink);
        }
        if !file_type.is_dir() {
            return Err(RootError::NotDirectory);
        }
        let actual = metadata.uid();
        if actual != expected_uid {
            return Err(RootError::ForeignOwner {
                expected: expected_uid,
                actual,
            });
        }
        Ok(Self {
            path: path.to_path_buf(),
            owner_uid: expected_uid,
        })
    }
}

/// 恢复根验证失败；任何变体都使恢复停用且不写盘、不清理。
#[derive(Debug)]
pub(crate) enum RootError {
    UnsupportedPlatform,
    HomeUnavailable,
    InvalidPath,
    Missing,
    NotDirectory,
    Symlink,
    ForeignOwner { expected: u32, actual: u32 },
    Io(io::Error),
}

#[cfg(target_os = "macos")]
fn map_io(error: io::Error) -> RootError {
    if error.kind() == io::ErrorKind::NotFound {
        RootError::Missing
    } else {
        RootError::Io(error)
    }
}

/// 恢复根内稳定文档身份：以 `ids/<id>` 独占创建成立，只增不复用。
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub(crate) struct RecoveryDocumentId(NonZeroU64);

impl RecoveryDocumentId {
    pub(crate) const fn get(self) -> NonZeroU64 {
        self.0
    }

    /// 固定 16 位小写十六进制目录名。
    pub(crate) fn directory_name(self) -> String {
        format!("{:016x}", self.0.get())
    }

    /// 只接受 16 位小写 `[0-9a-f]` 且非零的目录名。
    pub(crate) fn parse_directory_name(name: &str) -> Result<Self, RecoveryDefect> {
        let bytes = name.as_bytes();
        if bytes.len() != 16 || !bytes.iter().all(u8::is_ascii_hexdigit) {
            return Err(RecoveryDefect::BadIdentityName);
        }
        if bytes.iter().any(u8::is_ascii_uppercase) {
            return Err(RecoveryDefect::BadIdentityName);
        }
        let mut value = 0_u64;
        for &byte in bytes {
            let digit = match byte {
                b'0'..=b'9' => u64::from(byte - b'0'),
                b'a'..=b'f' => u64::from(byte - b'a') + 10,
                _ => return Err(RecoveryDefect::BadIdentityName),
            };
            value = value
                .checked_mul(16)
                .and_then(|current| current.checked_add(digit))
                .ok_or(RecoveryDefect::BadIdentityName)?;
        }
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(RecoveryDefect::BadIdentityName)
    }
}
