//! 单文档保存队列与结算。会话拥有本状态，向后台交付冻结快照及目标基线，
//! 再接收实际写盘回执。后台比较可借用文本片段。本模块主体不读取文件、
//! 不改变历史；实际磁盘阶段在同目录 `execute` 子模块按固定顺序执行。
//! 冻结快照强持有其已有的资源引用，本模块不管理恢复资源。

// 重载/关联替换、取消与关闭停止（`replace_association`、`cancel`、
// `begin_close`）的生产出口分别属于 W4 3.3/4.x；`WriteSettlement` 的
// operation/snapshot/path 字段也仍只有 crate 内测试读取。该豁免只覆盖本模块，
// 对应 UI 接线后逐项复核删除。
#![cfg_attr(not(test), allow(dead_code))]

use std::{
    error::Error,
    path::{Path, PathBuf},
    sync::Arc,
};

use aline_buffer::{BufferError, TextBudget, TextSnapshot};
use aline_source::{
    AtomicFailure, AtomicFailureKind, BaselineError, Cancellation, EncodeError, EncodedBytes,
    ExistingTarget, ReplacementDurability, ReplacementReceipt, SourceBaseline, StorageBudget,
    StorageSnapshot, decode, encode,
};

use super::{CurrentSnapshot, DocumentId, WaitHandle};

mod execute;
mod publication;

pub(crate) use execute::SaveJob;
pub(super) use publication::{SaveRequest, SaveRequestError};

/// 窗口层可命名的保存指令；不暴露协调器内部类型。
// 变体按裁决保持 `Run(SaveJob)` 直传；保存是低频路径，装箱只增加一次分配，
// 因此仅对本枚举豁免大小阈值 lint。
#[allow(clippy::large_enum_variant)]
pub(crate) enum SaveCommand {
    /// 已冻结并排队，交给后台执行。
    Run(SaveJob),
    /// 组合期内等待平台自然结束；句柄用于取消或结算等待。
    Wait(WaitHandle),
    /// 请求被拒绝，按类别映射本地化反馈。
    Rejected(SaveRejection),
}

/// 保存请求被拒绝的宿主可辨类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SaveRejection {
    /// 当前文档只读。
    ReadOnly,
    /// 保存队列正忙（在途、已排队或等待中）。
    Busy,
    /// 窗口/文档已进入关闭流程。
    Closing,
    /// 没有可写的绝对目标路径。
    NoTarget,
    /// 不属于宿主可辨类别的内部失败。
    Internal,
}

/// 一次保存结算后的稳定分类；窗口层只据此选反馈键。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum SaveResult {
    /// 已发布且目录同步确认。
    Saved,
    /// 已写入但同步/回执/基线不能完全确认。
    SavedUncertain,
    /// 目标与保存基线不一致，未发布。
    Conflict,
    /// 准备、编码或写盘失败。
    Failed,
    /// 本次保存被取消。
    Cancelled,
    /// 迟到或外来的完成项；不覆盖当前状态。
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OperationId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PathGeneration(u64);

#[derive(Debug, Eq, PartialEq)]
pub(super) enum SaveError {
    ForeignDocument,
    InvalidPath,
    InconsistentBaseline,
    StaleAssociation,
    Busy,
    Closing,
    NoQueuedWrite,
    Exhausted,
}

#[derive(Debug)]
pub(super) enum PrepareError {
    Cancelled,
    Conflict,
    Text(BufferError),
    Encoding(EncodeError),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SaveStatus {
    Idle,
    WaitingForComposition,
    Saving,
    Failed,
    Conflict,
    PublishedUncertain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CloseStatus {
    Waiting,
    NeedsDecision,
    Ready,
}

struct SavedBaseline {
    // 初次打开时，逻辑源码就是读入的精确文本。
    logical: Option<TextSnapshot>,
    storage: SourceBaseline,
}

impl SavedBaseline {
    fn equivalent(&self, current: &TextSnapshot) -> Result<bool, BufferError> {
        let Some(saved) = &self.logical else {
            if current.len_bytes() != self.storage.source().len() {
                return Ok(false);
            }
            return Ok(current
                .iter_chunks()?
                .flat_map(str::bytes)
                .eq(self.storage.source().bytes()));
        };
        if saved.id() == current.id() {
            return Ok(true);
        }
        if saved.len_bytes() != current.len_bytes() {
            return Ok(false);
        }
        Ok(current
            .iter_chunks()?
            .flat_map(str::bytes)
            .eq(saved.iter_chunks()?.flat_map(str::bytes)))
    }
}

struct QueuedWrite {
    operation: OperationId,
    snapshot: CurrentSnapshot,
}

struct ActiveWrite {
    operation: OperationId,
    cancellation: Cancellation,
}

struct Target {
    path: PathBuf,
    generation: PathGeneration,
    saved: SavedBaseline,
}

pub(super) struct FrozenWrite {
    owner: Arc<()>,
    operation: OperationId,
    snapshot: CurrentSnapshot,
    path: PathBuf,
    generation: PathGeneration,
    storage: SourceBaseline,
    logical: Option<TextSnapshot>,
    cancellation: Cancellation,
}

/// 编码由冻结工作创建，随后用其 bytes 核对实际发布回执。
pub(super) struct EncodedWrite {
    owner: Arc<()>,
    operation: OperationId,
    bytes: EncodedBytes,
}

impl EncodedWrite {
    pub(super) fn bytes(&self) -> &EncodedBytes {
        &self.bytes
    }
}

impl FrozenWrite {
    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn baseline(&self) -> &SourceBaseline {
        &self.storage
    }

    pub(super) fn cancellation(&self) -> &Cancellation {
        &self.cancellation
    }

    /// 现有目标观察须采用同路径、同磁盘代次且完整 bytes 相等。
    /// source 仍须在准备及发布阶段验证同一观察的文件和目录身份。
    pub(super) fn check_observation(&self, target: &ExistingTarget) -> Result<(), PrepareError> {
        if self.cancellation.is_cancelled() {
            return Err(PrepareError::Cancelled);
        }
        if target.path() != self.path
            || target.storage().version() != self.storage.storage().version()
            || target.storage().bytes() != self.storage.storage().bytes()
        {
            return Err(PrepareError::Conflict);
        }
        Ok(())
    }

    /// 后台编码。与上次保存的逻辑源码相同时复用实际落盘 bytes。
    pub(super) fn encode(
        &self,
        text_budget: TextBudget,
        storage_budget: StorageBudget,
    ) -> Result<EncodedWrite, PrepareError> {
        if self.cancellation.is_cancelled() {
            return Err(PrepareError::Cancelled);
        }
        let saved = SavedBaseline {
            logical: self.logical.clone(),
            storage: self.storage.clone(),
        };
        let equivalent = saved
            .equivalent(self.snapshot.text())
            .map_err(PrepareError::Text)?;
        let source;
        let text = if equivalent {
            self.storage.source()
        } else {
            source = self
                .snapshot
                .full_text(text_budget)
                .map_err(PrepareError::Text)?;
            &source
        };
        let bytes = encode(text, &self.storage, storage_budget).map_err(PrepareError::Encoding)?;
        Ok(EncodedWrite {
            owner: self.owner.clone(),
            operation: self.operation,
            bytes,
        })
    }

    /// 消费后台的真实结果。回执无法建立基线时仍保留实际发布的事实。
    pub(super) fn finish_replacement(
        self,
        encoded: &EncodedWrite,
        result: Result<ReplacementReceipt, AtomicFailure>,
    ) -> WriteCompletion {
        let outcome = match result {
            Ok(receipt) => {
                let expected = self.storage.storage().version();
                if !Arc::ptr_eq(&self.owner, &encoded.owner)
                    || self.operation != encoded.operation
                    || receipt.target_path() != self.path
                    || receipt.replaced_version() != expected
                    || expected.next().ok() != Some(receipt.version())
                    || receipt.input_baseline_id() != self.storage.id()
                    || receipt.bytes() != encoded.bytes().bytes()
                {
                    WriteOutcome::PublishedReceiptMismatch(receipt)
                } else {
                    let synchronized =
                        matches!(receipt.durability(), ReplacementDurability::Synchronized);
                    let budget = StorageBudget::new(receipt.bytes().len());
                    let storage =
                        StorageSnapshot::new(receipt.bytes().to_vec(), receipt.version(), budget)
                            .map_err(BaselineError::from)
                            .and_then(|storage| {
                                decode(&storage, budget)
                                    .map(|decoded| decoded.into_baseline())
                                    .map_err(BaselineError::from)
                            });
                    match storage {
                        Ok(storage) => WriteOutcome::Published {
                            storage,
                            synchronized,
                        },
                        Err(error) => WriteOutcome::PublishedBaselineFailed { receipt, error },
                    }
                }
            }
            Err(failure) => match failure.kind() {
                AtomicFailureKind::PublishedTargetUnverified { .. } => {
                    WriteOutcome::PublishedTargetUnverified(failure)
                }
                _ => WriteOutcome::NotPublished(WriteStop::File(failure)),
            },
        };
        WriteCompletion { job: self, outcome }
    }

    /// 只有写入前停止时可用。请求取消本身不生成完成项。
    pub(super) fn stop_before_write(self, error: PrepareError) -> WriteCompletion {
        let stop = match error {
            PrepareError::Cancelled => WriteStop::Cancelled,
            PrepareError::Conflict => WriteStop::Conflict,
            error => WriteStop::Preparation(error),
        };
        WriteCompletion {
            job: self,
            outcome: WriteOutcome::NotPublished(stop),
        }
    }
}

#[derive(Debug)]
pub(super) enum WriteStop {
    Cancelled,
    Conflict,
    File(AtomicFailure),
    Preparation(PrepareError),
}

#[derive(Debug)]
pub(super) enum WriteOutcome {
    NotPublished(WriteStop),
    Published {
        storage: SourceBaseline,
        synchronized: bool,
    },
    PublishedTargetUnverified(AtomicFailure),
    PublishedReceiptMismatch(ReplacementReceipt),
    PublishedBaselineFailed {
        receipt: ReplacementReceipt,
        error: BaselineError,
    },
}

pub(crate) struct WriteCompletion {
    job: FrozenWrite,
    outcome: WriteOutcome,
}

/// 每个完成项连同实际冻结输入交给会话完成后续结算。拒绝采用不吞掉已发布事实。
pub(super) struct WriteSettlement {
    pub(super) operation: OperationId,
    pub(super) snapshot: CurrentSnapshot,
    pub(super) path: PathBuf,
    pub(super) adopted: bool,
    pub(super) outcome: WriteOutcome,
}

impl WriteSettlement {
    /// 发布后归属异常时，返回 source 记录的实际目标。
    /// 未发布时没有可报告的发布目标。
    pub(super) fn published_target(&self) -> Option<&Path> {
        match &self.outcome {
            WriteOutcome::Published { .. } => Some(&self.path),
            WriteOutcome::PublishedReceiptMismatch(receipt)
            | WriteOutcome::PublishedBaselineFailed { receipt, .. } => Some(receipt.target_path()),
            WriteOutcome::PublishedTargetUnverified(failure) => match failure.kind() {
                AtomicFailureKind::PublishedTargetUnverified { target, .. } => Some(target),
                _ => None,
            },
            WriteOutcome::NotPublished(_) => None,
        }
    }

    /// 保留底层错误供诊断。状态与本地化反馈由宿主按枚举判断。
    pub(super) fn failure(&self) -> Option<&dyn Error> {
        match &self.outcome {
            WriteOutcome::NotPublished(WriteStop::File(error))
            | WriteOutcome::PublishedTargetUnverified(error) => Some(error),
            WriteOutcome::NotPublished(WriteStop::Preparation(PrepareError::Text(error))) => {
                Some(error)
            }
            WriteOutcome::NotPublished(WriteStop::Preparation(PrepareError::Encoding(error))) => {
                Some(error)
            }
            WriteOutcome::PublishedBaselineFailed { error, .. } => Some(error),
            _ => None,
        }
    }
}

pub(super) struct PersistenceCoordinator {
    owner: Arc<()>,
    document: DocumentId,
    target: Target,
    last_operation: u64,
    waiting: Option<publication::PendingSave>,
    queued: Option<QueuedWrite>,
    active: Option<ActiveWrite>,
    status: SaveStatus,
    closing: bool,
}

impl PersistenceCoordinator {
    pub(super) fn new(
        document: DocumentId,
        path: PathBuf,
        baseline: SourceBaseline,
    ) -> Result<Self, SaveError> {
        if !path.is_absolute() {
            return Err(SaveError::InvalidPath);
        }
        Ok(Self {
            owner: Arc::new(()),
            document,
            target: Target {
                path,
                generation: PathGeneration(0),
                saved: SavedBaseline {
                    logical: None,
                    storage: baseline,
                },
            },
            last_operation: 0,
            waiting: None,
            queued: None,
            active: None,
            status: SaveStatus::Idle,
            closing: false,
        })
    }

    fn next_operation(&self) -> Result<OperationId, SaveError> {
        if self.closing {
            return Err(SaveError::Closing);
        }
        if self.queued.is_some() || self.waiting.is_some() {
            return Err(SaveError::Busy);
        }
        self.last_operation
            .checked_add(1)
            .map(OperationId)
            .ok_or(SaveError::Exhausted)
    }

    fn enqueue(&mut self, operation: OperationId, snapshot: CurrentSnapshot) {
        self.queued = Some(QueuedWrite {
            operation,
            snapshot,
        });
        self.last_operation = operation.0;
    }

    /// 开始下一项时使用最近结算过的磁盘基线。每次工作仍须重新观察磁盘。
    pub(super) fn start(&mut self) -> Result<FrozenWrite, SaveError> {
        if self.closing {
            return Err(SaveError::Closing);
        }
        if self.active.is_some()
            || matches!(
                self.status,
                SaveStatus::Conflict | SaveStatus::PublishedUncertain
            )
        {
            return Err(SaveError::Busy);
        }
        let queued = self.queued.take().ok_or(SaveError::NoQueuedWrite)?;
        let cancellation = Cancellation::new();
        self.active = Some(ActiveWrite {
            operation: queued.operation,
            cancellation: cancellation.clone(),
        });
        Ok(FrozenWrite {
            owner: self.owner.clone(),
            operation: queued.operation,
            snapshot: queued.snapshot,
            path: self.target.path.clone(),
            generation: self.target.generation,
            storage: self.target.saved.storage.clone(),
            logical: self.target.saved.logical.clone(),
            cancellation,
        })
    }

    /// 取消只撤销待执行项或通知在途工作。实际在途项仍需带回回执。
    pub(super) fn cancel(&mut self, operation: OperationId) -> bool {
        if self
            .queued
            .as_ref()
            .is_some_and(|job| job.operation == operation)
        {
            self.queued = None;
            return true;
        }
        if let Some(active) = self
            .active
            .as_ref()
            .filter(|job| job.operation == operation)
        {
            active.cancellation.cancel();
            return true;
        }
        false
    }

    pub(super) fn reconcile(&mut self, completion: WriteCompletion) -> WriteSettlement {
        let WriteCompletion { job, outcome } = completion;
        let matches_active = Arc::ptr_eq(&job.owner, &self.owner)
            && job.snapshot.document_id() == &self.document
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.operation == job.operation);
        let mut adopted = false;
        if matches_active {
            self.active = None;
            if job.generation == self.target.generation && job.path == self.target.path {
                adopted = self.apply_outcome(&job, &outcome);
            }
        }
        WriteSettlement {
            operation: job.operation,
            snapshot: job.snapshot,
            path: job.path,
            adopted,
            outcome,
        }
    }

    fn apply_outcome(&mut self, job: &FrozenWrite, outcome: &WriteOutcome) -> bool {
        self.status = match outcome {
            WriteOutcome::Published {
                storage,
                synchronized,
            } => {
                self.target.saved = SavedBaseline {
                    logical: Some(job.snapshot.text().clone()),
                    storage: storage.clone(),
                };
                if *synchronized {
                    SaveStatus::Idle
                } else {
                    SaveStatus::PublishedUncertain
                }
            }
            WriteOutcome::PublishedTargetUnverified(_)
            | WriteOutcome::PublishedReceiptMismatch(_)
            | WriteOutcome::PublishedBaselineFailed { .. } => SaveStatus::PublishedUncertain,
            WriteOutcome::NotPublished(WriteStop::Cancelled) => return false,
            WriteOutcome::NotPublished(WriteStop::Conflict) => SaveStatus::Conflict,
            WriteOutcome::NotPublished(WriteStop::File(error)) => match error.kind() {
                AtomicFailureKind::Cancelled { .. } => return false,
                AtomicFailureKind::Conflict { .. } => SaveStatus::Conflict,
                _ => SaveStatus::Failed,
            },
            WriteOutcome::NotPublished(WriteStop::Preparation(_)) => SaveStatus::Failed,
        };
        true
    }

    pub(super) fn status(&self) -> SaveStatus {
        if self.active.is_some() {
            SaveStatus::Saving
        } else if self.waiting.is_some() {
            SaveStatus::WaitingForComposition
        } else {
            self.status
        }
    }

    /// 后台观察须记录起点路径代次，采纳时同当前关联复验。
    pub(super) fn path_generation(&self) -> PathGeneration {
        self.target.generation
    }

    /// 返回当前保存目标的绝对路径；宿主据此在目标同目录生成 staging 候选。
    pub(super) fn target_path(&self) -> &Path {
        &self.target.path
    }

    /// 显式建立已完成的路径关联。调用者须先在 DocCore 统一提交重载/引用修改，
    /// 并提供同一读盘结果的文本和存储基线。
    pub(super) fn replace_association(
        &mut self,
        expected: PathGeneration,
        path: PathBuf,
        current_saved: &CurrentSnapshot,
        baseline: SourceBaseline,
    ) -> Result<(), SaveError> {
        if self.active.is_some() || self.queued.is_some() || self.waiting.is_some() {
            return Err(SaveError::Busy);
        }
        if self.closing {
            return Err(SaveError::Closing);
        }
        if !path.is_absolute() {
            return Err(SaveError::InvalidPath);
        }
        if current_saved.document_id() != &self.document {
            return Err(SaveError::ForeignDocument);
        }
        if self.target.generation != expected {
            return Err(SaveError::StaleAssociation);
        }
        let saved = SavedBaseline {
            logical: None,
            storage: baseline,
        };
        if !saved.equivalent(current_saved.text()).unwrap_or(false) {
            return Err(SaveError::InconsistentBaseline);
        }
        let next = self
            .target
            .generation
            .0
            .checked_add(1)
            .ok_or(SaveError::Exhausted)?;
        self.target = Target {
            path,
            generation: PathGeneration(next),
            saved,
        };
        self.status = SaveStatus::Idle;
        Ok(())
    }

    /// 请求关闭会停止待执行项和通知在途项取消。实际回执仍会完整结算。
    fn stop_for_close(&mut self) {
        self.closing = true;
        self.queued = None;
        if let Some(active) = &self.active {
            active.cancellation.cancel();
        }
    }

    pub(super) fn cancel_close(&mut self) {
        self.closing = false;
    }

    /// 比较可能扫描全文，须由宿主在非高频路径调用或移交后台。
    /// 无法证明相等时保守判为有修改。
    pub(super) fn is_dirty(&self, current: &CurrentSnapshot) -> bool {
        current.document_id() != &self.document
            || !self
                .target
                .saved
                .equivalent(current.text())
                .unwrap_or(false)
    }

    /// 关闭使用最新快照并等待在途回执。失败、冲突或不确定发布需用户处理。
    pub(super) fn close_status(&self, current: &CurrentSnapshot) -> CloseStatus {
        if self.active.is_some() || self.queued.is_some() || self.waiting.is_some() {
            CloseStatus::Waiting
        } else if self.status != SaveStatus::Idle || self.is_dirty(current) {
            CloseStatus::NeedsDecision
        } else {
            CloseStatus::Ready
        }
    }
}

#[cfg(test)]
mod tests;
