//! 生产临时资源读取源：把 `aline-temp:` 引用严格解析为临时根下的普通文件。
//! 只读，不创建、不修复、不删除；任何越界或身份异常都返回 `Unavailable`。

use std::fs;
use std::path::Path;

use aline_source::fs as platform_fs;

use super::catalog::parse_locator;
use super::temp_root::TempRoot;
use crate::document::recovery::{
    MAX_RESOURCE_BYTES, RecoveryResourceError, RecoveryResourceSource,
};

/// 以受检临时根为唯一根目录的资源读取源。
#[derive(Clone, Debug)]
pub(crate) struct TempResourceSource {
    root: TempRoot,
}

#[cfg_attr(not(test), allow(dead_code))]
impl TempResourceSource {
    pub(crate) fn new(root: TempRoot) -> Self {
        Self { root }
    }
}

impl RecoveryResourceSource for TempResourceSource {
    fn read(&self, reference: &str, budget: usize) -> Result<Vec<u8>, RecoveryResourceError> {
        let part = parse_locator(reference).ok_or(RecoveryResourceError::Unavailable)?;
        let directory = self.root.path().join(part.document_name());
        verify_directory(&directory)?;
        let path = directory.join(part.file_name());
        platform_fs::verify_ordinary_file(&path).map_err(|_| RecoveryResourceError::Unavailable)?;
        let metadata = fs::metadata(&path).map_err(|_| RecoveryResourceError::Unavailable)?;
        let length =
            usize::try_from(metadata.len()).map_err(|_| RecoveryResourceError::TooLarge)?;
        if length > budget || length > MAX_RESOURCE_BYTES {
            return Err(RecoveryResourceError::TooLarge);
        }
        fs::read(&path).map_err(|_| RecoveryResourceError::Unavailable)
    }
}

fn verify_directory(path: &Path) -> Result<(), RecoveryResourceError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| RecoveryResourceError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(RecoveryResourceError::Unavailable);
    }
    Ok(())
}
