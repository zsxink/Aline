//! 恢复资源读取抽象：按引用键取字节，不解析路径、不推断文件位置、不持有文档。
//!
//! 4.1 生产默认实现为 [`UnavailableResources`]：任何强引用都取不到字节，
//! 协调器因此报告“资源不完整”而不发布恢复副本。真实资源保全归 4.2。

use std::sync::Arc;

use super::store::RecoveryStore;

/// 资源读取失败分类；任何失败都只让本次准备不完整，不删除旧代。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryResourceError {
    /// 引用当前不可解析或不可读。
    Unavailable,
    /// 资源内容损坏或未通过受检解码；仅测试伪资源构造。
    #[allow(dead_code)]
    Corrupt,
    /// 资源超过单次读取预算。
    TooLarge,
}

/// 按引用键读取资源字节；实现不得解析路径、推断文件位置或持有文档。
pub(crate) trait RecoveryResourceSource: Send + Sync {
    /// 读取一条引用对应的字节；`budget` 是本次读取的硬上限。
    fn read(&self, reference: &str, budget: usize) -> Result<Vec<u8>, RecoveryResourceError>;
}

/// 4.1 生产默认：任何强引用都取不到字节。
pub(crate) struct UnavailableResources;

impl RecoveryResourceSource for UnavailableResources {
    fn read(&self, _reference: &str, _budget: usize) -> Result<Vec<u8>, RecoveryResourceError> {
        Err(RecoveryResourceError::Unavailable)
    }
}

/// 恢复会话配置：已打开的存储与资源读取源。
#[derive(Clone)]
pub(crate) struct RecoveryConfig {
    pub(crate) store: RecoveryStore,
    pub(crate) resources: Arc<dyn RecoveryResourceSource>,
}

/// 会话配置被拒绝的原因；配置只能在下一次打开前注入。
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum RecoveryConfigError {
    /// 当前会话已持有文档；配置不得在文档打开期间改变。
    DocumentOpen,
}
