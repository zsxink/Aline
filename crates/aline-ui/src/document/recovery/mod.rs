//! W4 恢复：V1 记录编解码、控制记录、受检根、限量发布、退役与启动扫描，
//! 以及 10 秒独立计时、资源读取抽象与会话协调器。
//! 本模块不接生产恢复根、不做 UI 呈现（归 4.2）。

mod control;
mod coordinator;
mod record;
mod resource;
mod restore;
mod root;
mod scan;
mod schedule;
mod store;
#[cfg(test)]
mod tests;

pub(crate) use coordinator::{
    PreparedRecovery, RecoveryAcceptOutcome, RecoveryBegin, RecoveryBeginOutcome, RecoveryCloseJob,
    RecoveryPrepareJob, RecoveryPublishJob, RecoverySession, RecoverySettlement, RecoveryStart,
};
pub(crate) use resource::{
    RecoveryConfig, RecoveryConfigError, RecoveryResourceError, RecoveryResourceSource,
    UnavailableResources,
};
pub(crate) use restore::{
    RecoveryDiscardCompletion, RecoveryDiscardJob, RecoveryRestoreCompletion, RecoveryRestoreJob,
    RestoredRecovery,
};
pub(crate) use schedule::RecoveryAction;

pub(crate) use record::{
    MAX_GENERATION_RESOURCE_BYTES, MAX_REFERENCE_BYTES, MAX_RESOURCE_BYTES, MAX_RESOURCE_COUNT,
};
pub(crate) use root::create_default_root;
pub(crate) use scan::{RecoveryGenerationStatus, RecoveryLineageDisposition, RecoveryScan};
pub(crate) use store::{
    RecoveredPath, RecoveredResource, RecoveryDocumentId, RecoveryPath, RecoveryStore,
    RecoveryStoreError, RetireOutcome,
};

// 测试通过 `crate::document` 访问的恢复类型；生产路径使用上面的收窄入口。
#[cfg(test)]
pub(crate) use restore::RecoveryRestoreError;
#[cfg(test)]
pub(crate) use root::RecoveryRoot;
#[cfg(test)]
pub(crate) use scan::{RecoveryGenerationScan, RecoveryLineageScan};
#[cfg(test)]
pub(crate) use store::{
    RecoveredDocument, RecoveryGenerationInput, RecoveryPublishOutcome, RecoveryResourceInput,
};
