//! 平台受检替换原语入口。
//!
//! 只有 macOS 提供真实实现（`macos` 子模块，全仓库唯一允许 `unsafe` 的源码文件，
//! 准入记录见 `docs/decisions.md` §44）。其他平台保持失败关闭：`replace` 模块的
//! 默认适配函数直接拒绝，不退回路径 `rename` 或删除后重建。
//!
//! 本模块只转出安全 Rust 函数；`unsafe` 不越过 `macos` 子模块边界。

use super::AtomicFailureKind;

#[cfg(target_os = "macos")]
mod macos;

#[cfg(target_os = "macos")]
pub(crate) use macos::{cleanup_staging, open_staging, publish_existing, transfer_access_state};

/// 独占创建 staging 的失败；区分 staging 是否已由本操作创建。
///
/// `created == true` 表示失败发生在创建之后；调用方必须把调用方提供的 staging
/// 路径交给宿主清理，即使本层已尝试删除——该路径不保证仍然存在。
pub(crate) struct StagingOpenFailure {
    /// 原始受检失败分类。
    pub(crate) kind: AtomicFailureKind,
    /// staging 是否已由本操作创建。
    pub(crate) created: bool,
}

impl StagingOpenFailure {
    /// 创建前失败：磁盘上没有本操作的 staging。
    pub(crate) fn not_created(kind: AtomicFailureKind) -> Self {
        Self {
            kind,
            created: false,
        }
    }

    /// 创建后失败：调用方需要携带 staging 路径。
    pub(crate) fn created(kind: AtomicFailureKind) -> Self {
        Self {
            kind,
            created: true,
        }
    }
}
