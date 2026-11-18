//! 恢复条目的后台读取与丢弃任务：恢复只读快照并写回内嵌资源到临时根，
//! 丢弃只走 `RecoveryStore::discard`；两者都不触碰源文件或原路径授权。

use std::num::NonZeroU64;

use super::store::{
    RecoveredDocument, RecoveryDocumentId, RecoveryReadError, RecoveryStore, RecoveryStoreError,
    RetireOutcome,
};
use crate::document::resources::{ResourceError, TempDocumentId, TempRoot, materialize_recovered};

/// 一条恢复记录的后台读取任务输入；`operation` 由面板分配并回传比对。
#[derive(Clone, Debug)]
pub(crate) struct RecoveryRestoreJob {
    store: RecoveryStore,
    document: RecoveryDocumentId,
    generation: NonZeroU64,
    operation: u64,
    temp_root: Option<TempRoot>,
}

impl RecoveryRestoreJob {
    pub(crate) fn new(
        store: RecoveryStore,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
        operation: u64,
        temp_root: Option<TempRoot>,
    ) -> Self {
        Self {
            store,
            document,
            generation,
            operation,
            temp_root,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn operation(&self) -> u64 {
        self.operation
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn document(&self) -> RecoveryDocumentId {
        self.document
    }

    /// 后台执行；只读记录并把内嵌资源写回临时根，不写恢复记录。
    pub(crate) fn run(self) -> RecoveryRestoreCompletion {
        let result = self.restore();
        RecoveryRestoreCompletion {
            document: self.document,
            operation: self.operation,
            result,
        }
    }

    fn restore(&self) -> Result<RestoredRecovery, RecoveryRestoreError> {
        let recovered = self
            .store
            .recover(self.document, self.generation)
            .map_err(RecoveryRestoreError::Read)?;
        let temp_document = self.materialize(&recovered)?;
        Ok(RestoredRecovery {
            recovered,
            temp_document,
        })
    }

    fn materialize(
        &self,
        recovered: &RecoveredDocument,
    ) -> Result<Option<TempDocumentId>, RecoveryRestoreError> {
        if recovered.resources.is_empty() {
            return Ok(None);
        }
        let Some(root) = self.temp_root.as_ref() else {
            return Err(RecoveryRestoreError::Resources(
                ResourceError::UnsupportedPlatform,
            ));
        };
        let temp_document = materialize_recovered(root, &recovered.resources)
            .map_err(RecoveryRestoreError::Resources)?;
        Ok(Some(temp_document))
    }
}

/// 后台读取任务完成信封；迟到结果由面板按 `operation` 比对丢弃。
#[derive(Debug)]
pub(crate) struct RecoveryRestoreCompletion {
    pub(crate) document: RecoveryDocumentId,
    pub(crate) operation: u64,
    pub(crate) result: Result<RestoredRecovery, RecoveryRestoreError>,
}

/// 成功恢复出的内存文档与已写回的临时资源身份；资源为空时无临时身份。
#[derive(Debug)]
pub(crate) struct RestoredRecovery {
    pub(crate) recovered: RecoveredDocument,
    pub(crate) temp_document: Option<TempDocumentId>,
}

/// 恢复失败分类：记录读取失败或资源写回失败。
/// 分类载荷只供窗口选择反馈与测试断言，生产路径不读取内部错误。
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) enum RecoveryRestoreError {
    Read(RecoveryReadError),
    Resources(ResourceError),
}

/// 一条恢复记录的显式丢弃任务；只由二次确认后构造。
#[derive(Clone, Debug)]
pub(crate) struct RecoveryDiscardJob {
    store: RecoveryStore,
    document: RecoveryDocumentId,
    operation: u64,
}

impl RecoveryDiscardJob {
    pub(crate) fn new(store: RecoveryStore, document: RecoveryDocumentId, operation: u64) -> Self {
        Self {
            store,
            document,
            operation,
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn operation(&self) -> u64 {
        self.operation
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) const fn document(&self) -> RecoveryDocumentId {
        self.document
    }

    /// 后台执行逐文档退役；不删除临时根或原文件。
    pub(crate) fn run(self) -> RecoveryDiscardCompletion {
        let result = self.store.discard(self.document);
        RecoveryDiscardCompletion {
            document: self.document,
            operation: self.operation,
            result,
        }
    }
}

/// 后台丢弃任务完成信封；只有成功退役才允许从面板移除条目。
#[derive(Debug)]
pub(crate) struct RecoveryDiscardCompletion {
    pub(crate) document: RecoveryDocumentId,
    pub(crate) operation: u64,
    pub(crate) result: Result<RetireOutcome, RecoveryStoreError>,
}
