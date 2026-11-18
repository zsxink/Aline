//! W4 恢复：V1 记录编解码、控制记录、受检根、限量发布、退役与启动扫描，
//! 以及 10 秒独立计时、资源读取抽象与会话协调器。
//! 本模块不接生产恢复根、不做 UI 呈现（归 4.2）。

// 4.1a/4.1b 的完整入口与错误分类由 4.2 消费；在这些模块接线前，
// 重导出与分类字段暂时无人使用。该豁免只覆盖恢复模块。
#![allow(unused_imports)]
#![cfg_attr(test, allow(dead_code))]

mod control;
mod coordinator;
mod record;
mod resource;
mod root;
mod scan;
mod schedule;
mod store;
#[cfg(test)]
mod tests;

pub(crate) use coordinator::{
    PreparedReady, PreparedRecovery, PublishedRecovery, RECOVERY_FAILED_KEY,
    RECOVERY_INCOMPLETE_KEY, RecoveryAcceptOutcome, RecoveryBegin, RecoveryBeginOutcome,
    RecoveryCloseJob, RecoveryJobTag, RecoveryPhase, RecoveryPrepareJob, RecoveryPublishJob,
    RecoverySession, RecoverySessionId, RecoverySettlement, RecoverySkip, RecoveryStart,
};
pub(crate) use resource::{
    RecoveryConfig, RecoveryConfigError, RecoveryResourceError, RecoveryResourceSource,
    UnavailableResources,
};
pub(crate) use schedule::{RecoveryAction, RecoveryScheduler};

pub(crate) use control::{ControlError, NextId, Reservation};
pub(crate) use record::{
    MAX_GENERATION_RESOURCE_BYTES, MAX_REFERENCE_BYTES, MAX_RESOURCE_BYTES, MAX_RESOURCE_COUNT,
    RecordDefect, RecordError,
};
pub(crate) use root::{RecoveryRoot, RootError};
pub(crate) use scan::{
    RecoveryGenerationScan, RecoveryGenerationStatus, RecoveryLineageDisposition,
    RecoveryLineageScan, RecoveryScan, RecoveryScanDefect,
};
pub(crate) use store::{
    RecoveredDocument, RecoveredPath, RecoveredResource, RecoveryBudget, RecoveryDefect,
    RecoveryDocumentId, RecoveryGenerationInput, RecoveryLock, RecoveryOperation, RecoveryPath,
    RecoveryPublishCause, RecoveryPublishOutcome, RecoveryReadError, RecoveryResourceInput,
    RecoveryStore, RecoveryStoreError, RetireOutcome,
};
