//! W4 恢复存储：V1 记录编解码、控制记录、受检根、限量发布、退役与启动扫描。
//! 本模块不接生产恢复根、不做 10 秒调度或 UI 呈现（归 4.1b/4.2）。

// 4.1a 的完整入口与错误分类由 4.1b/4.2 消费；在这些模块接线前，
// 重导出与分类字段暂时无人使用。该豁免只覆盖恢复模块。
#![allow(unused_imports)]
#![cfg_attr(test, allow(dead_code))]

mod control;
mod record;
mod root;
mod scan;
mod store;
#[cfg(test)]
mod tests;

pub(crate) use control::{ControlError, NextId, Reservation};
pub(crate) use record::{RecordDefect, RecordError};
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
