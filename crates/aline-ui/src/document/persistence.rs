//! 单文档保存队列与结算。会话拥有本状态，向后台交付冻结快照及目标基线，
//! 再接收实际写盘回执。后台比较可借用文本片段。本模块主体不读取文件、
//! 不改变历史；实际磁盘阶段在同目录 `execute` 子模块按固定顺序执行。
//! 冻结快照强持有其已有的资源引用，本模块不管理恢复资源。

#[cfg(test)]
use std::error::Error;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use aline_buffer::{BufferError, TextBudget, TextSnapshot};
use aline_source::{
    AtomicFailure, AtomicFailureKind, BaselineError, Cancellation, EncodeError, EncodedBytes,
    ExistingTarget, ReplacementDurability, ReplacementReceipt, SourceBaseline, StorageBudget,
    StorageSnapshot, decode, encode,
};

use super::{CurrentSnapshot, DocError, DocumentId, WaitHandle};

mod execute;
mod external;
mod publication;

pub(crate) use execute::SaveJob;
pub(crate) use external::{
    ExternalApplyResult, ExternalCheckCommand, ExternalCheckCompletion, ExternalCheckIntent,
    ExternalCheckJob, ExternalCheckRejection, ExternalConflictView, ExternalDiskState,
    ExternalObservation, clamped_selection, preview,
};
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
    /// 保存基线冲突待用户处理。
    Conflict,
    /// 不确定发布待用户处理。
    Uncertain,
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
pub(crate) enum WriteKind {
    Existing,
    Create,
    SaveAs,
}

#[derive(Clone, Debug)]
pub(super) enum WriteMode {
    Existing,
    Associate { kind: WriteKind, overwrite: bool },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct OperationId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PathGeneration(u64);

impl PathGeneration {
    /// 返回路径代次数值，供恢复准备记录起点。
    pub(super) const fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Eq, PartialEq)]
pub(super) enum SaveError {
    ForeignDocument,
    InvalidPath,
    /// 仅关联测试覆盖的基线一致性拒绝。
    #[cfg(test)]
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
    /// 底层缓冲错误只由测试读取诊断。
    Text(#[allow(dead_code)] BufferError),
    /// 编码错误只由测试读取诊断。
    Encoding(#[allow(dead_code)] EncodeError),
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
pub(crate) enum CloseStatus {
    Waiting,
    NeedsDecision,
    Ready,
}

/// 关闭被拒的原因；宿主据此告诉用户下一步能做什么，而不是笼统提示「有未结算编辑」。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloseBlocker {
    /// 有未保存的编辑或在途/等待项。
    PendingChanges,
    /// 最近一次保存失败；重试保存或另存为后可关闭。
    SaveFailed,
    /// 外部冲突未处理。
    SaveConflict,
    /// 上次写入结果未确认；只能另存为新文件后再关闭。
    PublishedUncertain,
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

/// 以给定逻辑保存视图与存储基线判断当前文本是否仍等价；复用唯一比较实现。
pub(super) fn saved_equivalent(
    logical: Option<&TextSnapshot>,
    storage: &SourceBaseline,
    current: &TextSnapshot,
) -> Result<bool, BufferError> {
    SavedBaseline {
        logical: logical.cloned(),
        storage: storage.clone(),
    }
    .equivalent(current)
}

struct QueuedWrite {
    operation: OperationId,
    snapshot: CurrentSnapshot,
    kind: WriteKind,
    path: PathBuf,
}

struct ActiveWrite {
    operation: OperationId,
    cancellation: Cancellation,
}

struct Target {
    path: PathBuf,
    generation: PathGeneration,
    saved: SavedBaseline,
    creating: bool,
}

pub(super) struct FrozenWrite {
    owner: Arc<()>,
    operation: OperationId,
    snapshot: CurrentSnapshot,
    path: PathBuf,
    generation: PathGeneration,
    storage: SourceBaseline,
    logical: Option<TextSnapshot>,
    mode: WriteMode,
    cancellation: Cancellation,
    /// 已发布但未关联的侧目标；重试覆盖前必须逐字节复验该事实。
    association_expected: Option<SourceBaseline>,
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

    pub(super) fn mode(&self) -> &WriteMode {
        &self.mode
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

    /// Create/SaveAs 覆盖既有目标时的回执复验：目标路径、被替换版本、bytes 与
    /// 新版本都必须来自本次观察，而不是旧目标基线。
    pub(super) fn finish_associate(
        self,
        encoded: &EncodedWrite,
        expected_version: aline_source::DiskVersion,
        result: Result<ReplacementReceipt, AtomicFailure>,
    ) -> WriteCompletion {
        let outcome = match result {
            Ok(receipt) => {
                if !Arc::ptr_eq(&self.owner, &encoded.owner)
                    || self.operation != encoded.operation
                    || receipt.target_path() != self.path
                    || receipt.replaced_version() != expected_version
                    || expected_version.next().ok() != Some(receipt.version())
                    || receipt.bytes() != encoded.bytes().bytes()
                {
                    WriteOutcome::PublishedReceiptMismatch(receipt)
                } else {
                    let synchronized =
                        matches!(receipt.durability(), ReplacementDurability::Synchronized);
                    let budget = StorageBudget::new(receipt.bytes().len());
                    match StorageSnapshot::new(receipt.bytes().to_vec(), receipt.version(), budget)
                        .map_err(BaselineError::from)
                        .and_then(|storage| {
                            decode(&storage, budget)
                                .map(|decoded| decoded.into_baseline())
                                .map_err(BaselineError::from)
                        }) {
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
    /// 底层失败载荷只由测试读取诊断；生产只按枚举分类。
    File(#[allow(dead_code)] AtomicFailure),
    /// 新建目标失败；4.2a 只按分类转为 Failed，底层原因不参与结算。
    Create,
    /// 失败分类只由测试读取诊断。
    Preparation(#[allow(dead_code)] PrepareError),
}

#[derive(Debug)]
pub(super) enum WriteOutcome {
    NotPublished(WriteStop),
    Published {
        storage: SourceBaseline,
        synchronized: bool,
    },
    /// 载荷只由测试读取目标路径；生产仅按分类结算。
    PublishedTargetUnverified(#[allow(dead_code)] AtomicFailure),
    /// 载荷只由测试读取目标路径；生产仅按分类结算。
    PublishedReceiptMismatch(#[allow(dead_code)] ReplacementReceipt),
    PublishedBaselineFailed {
        /// 只由测试读取发布回执。
        #[allow(dead_code)]
        receipt: ReplacementReceipt,
        /// 只由测试读取底层错误。
        #[allow(dead_code)]
        error: BaselineError,
    },
    /// 新建目标已写出，但实际 bytes 无法建立保存基线。
    PublishedNewBaselineFailed {
        /// 只由测试读取实际目标路径。
        #[allow(dead_code)]
        path: PathBuf,
        /// 只由测试读取底层错误。
        #[allow(dead_code)]
        error: BaselineError,
    },
}

pub(crate) struct WriteCompletion {
    job: FrozenWrite,
    outcome: WriteOutcome,
}

impl WriteCompletion {
    /// 文件是否已越过不可逆发布点（含基线不可确认的部分发布）。
    /// 会话据此决定引用候选是否可被本回执消费；未发布结算必须保留候选。
    pub(crate) fn is_published(&self) -> bool {
        !matches!(self.outcome, WriteOutcome::NotPublished(_))
    }
}

/// 每个完成项连同实际冻结输入交给会话完成后续结算。拒绝采用不吞掉已发布事实。
pub(super) struct WriteSettlement {
    /// 只由测试校验结算归属。
    #[allow(dead_code)]
    pub(super) operation: OperationId,
    /// 只由测试校验冻结快照。
    #[allow(dead_code)]
    pub(super) snapshot: CurrentSnapshot,
    pub(super) path: PathBuf,
    pub(super) adopted: bool,
    /// 文件已经发布，但当前核心拒绝引用/资源结算，仍保持旧目标关联。
    pub(super) association_deferred: bool,
    /// 关联成功时的目标路径与基线；会话据此更新展示路径。
    pub(super) association: Option<(PathBuf, SourceBaseline)>,
    pub(super) outcome: WriteOutcome,
}

impl WriteSettlement {
    /// 发布后归属异常时，返回 source 记录的实际目标。
    /// 未发布时没有可报告的发布目标。仅 crate 测试读取。
    #[cfg(test)]
    pub(super) fn published_target(&self) -> Option<&Path> {
        match &self.outcome {
            WriteOutcome::Published { .. } => Some(&self.path),
            WriteOutcome::PublishedReceiptMismatch(receipt)
            | WriteOutcome::PublishedBaselineFailed { receipt, .. } => Some(receipt.target_path()),
            WriteOutcome::PublishedTargetUnverified(failure) => match failure.kind() {
                AtomicFailureKind::PublishedTargetUnverified { target, .. } => Some(target),
                _ => None,
            },
            WriteOutcome::PublishedNewBaselineFailed { path, .. } => Some(path),
            WriteOutcome::NotPublished(_) => None,
        }
    }

    /// 保留底层错误供诊断。状态与本地化反馈由宿主按枚举判断。仅 crate 测试读取。
    #[cfg(test)]
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
            WriteOutcome::PublishedBaselineFailed { error, .. }
            | WriteOutcome::PublishedNewBaselineFailed { error, .. } => Some(error),
            WriteOutcome::NotPublished(WriteStop::Create)
            | WriteOutcome::NotPublished(WriteStop::Cancelled)
            | WriteOutcome::NotPublished(WriteStop::Conflict) => None,
            WriteOutcome::NotPublished(WriteStop::Preparation(PrepareError::Cancelled))
            | WriteOutcome::NotPublished(WriteStop::Preparation(PrepareError::Conflict)) => None,
            WriteOutcome::Published { .. } | WriteOutcome::PublishedReceiptMismatch(_) => None,
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
    /// 最近的侧目标发布事实；不作为当前文档的保存基线或目标关联。
    partial_association: Option<(PathBuf, SourceBaseline)>,
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
                creating: false,
            },
            last_operation: 0,
            waiting: None,
            queued: None,
            active: None,
            status: SaveStatus::Idle,
            closing: false,
            partial_association: None,
        })
    }

    /// 未命名/无目标文档的首个协调器；`path` 是用户刚选定的绝对目标，
    /// `encoding_baseline` 只用于 BOM/行尾策略，不代表目标已存在。
    pub(super) fn new_untitled(
        document: DocumentId,
        path: PathBuf,
        encoding_baseline: SourceBaseline,
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
                    storage: encoding_baseline,
                },
                creating: true,
            },
            last_operation: 0,
            waiting: None,
            queued: None,
            active: None,
            status: SaveStatus::Idle,
            closing: false,
            partial_association: None,
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

    fn enqueue(
        &mut self,
        operation: OperationId,
        snapshot: CurrentSnapshot,
        kind: WriteKind,
        path: PathBuf,
    ) {
        self.queued = Some(QueuedWrite {
            operation,
            snapshot,
            kind,
            path,
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
        let mode = match queued.kind {
            WriteKind::Existing => WriteMode::Existing,
            kind => WriteMode::Associate {
                kind,
                overwrite: true,
            },
        };
        let association_expected = self
            .partial_association
            .as_ref()
            .filter(|(path, _)| path == &queued.path)
            .map(|(_, baseline)| baseline.clone());
        Ok(FrozenWrite {
            owner: self.owner.clone(),
            operation: queued.operation,
            snapshot: queued.snapshot,
            path: queued.path,
            generation: self.target.generation,
            storage: self.target.saved.storage.clone(),
            logical: self.target.saved.logical.clone(),
            mode,
            cancellation,
            association_expected,
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
            let same_target = match &job.mode {
                WriteMode::Existing => {
                    job.generation == self.target.generation && job.path == self.target.path
                }
                WriteMode::Associate { .. } => job.generation == self.target.generation,
            };
            if same_target {
                adopted = self.apply_outcome(&job, &outcome);
            }
        }
        let association = if adopted {
            match (&job.mode, &outcome) {
                (WriteMode::Associate { .. }, WriteOutcome::Published { storage, .. }) => {
                    Some((job.path.clone(), storage.clone()))
                }
                _ => None,
            }
        } else {
            None
        };
        WriteSettlement {
            operation: job.operation,
            snapshot: job.snapshot,
            path: job.path,
            adopted,
            association_deferred: false,
            association,
            outcome,
        }
    }

    /// 关联写盘只能在文档候选发布成功后挂接目标；拒绝时仍登记真实侧目标发布。
    pub(super) fn reconcile_association(
        &mut self,
        completion: WriteCompletion,
        publish_core: impl FnOnce() -> Result<(), DocError>,
    ) -> WriteSettlement {
        let valid = self.is_current_association(&completion);
        let job = &completion.job;
        let next_generation_valid = match job.mode {
            WriteMode::Associate {
                kind: WriteKind::SaveAs,
                ..
            } => self.target.generation.0.checked_add(1).is_some(),
            _ => true,
        };
        if valid
            && let WriteOutcome::Published { storage, .. } = &completion.outcome
            && (!next_generation_valid || publish_core().is_err())
        {
            self.active = None;
            self.status = SaveStatus::PublishedUncertain;
            self.partial_association = Some((job.path.clone(), storage.clone()));
            return WriteSettlement {
                operation: job.operation,
                snapshot: job.snapshot.clone(),
                path: job.path.clone(),
                adopted: true,
                association_deferred: true,
                association: None,
                outcome: completion.outcome,
            };
        }
        let settlement = self.reconcile(completion);
        // 防御不变量：任何「已发布但未采纳」的当前关联回执都必须登记侧目标
        // 写盘事实，重试覆盖前逐字节复验；正常代次耗尽已走上面的 deferred 分支。
        if valid
            && !settlement.adopted
            && let WriteOutcome::Published { storage, .. } = &settlement.outcome
        {
            self.status = SaveStatus::PublishedUncertain;
            self.partial_association = Some((settlement.path.clone(), storage.clone()));
        }
        settlement
    }

    /// 候选只可由当前关联写盘回执消费；旧回执不得取走在途的新候选。
    pub(super) fn is_current_association(&self, completion: &WriteCompletion) -> bool {
        let job = &completion.job;
        Arc::ptr_eq(&job.owner, &self.owner)
            && job.snapshot.document_id() == &self.document
            && self
                .active
                .as_ref()
                .is_some_and(|active| active.operation == job.operation)
            && job.generation == self.target.generation
            && job.path.is_absolute()
            && matches!(job.mode, WriteMode::Associate { .. })
    }

    /// 文档所有者提供真实受检候选；组合等待已在关联流程先行处理。
    pub(super) fn queue_association_snapshot(
        &mut self,
        snapshot: CurrentSnapshot,
        kind: WriteKind,
        path: PathBuf,
    ) -> Result<OperationId, SaveError> {
        if snapshot.document_id() != &self.document || !path.is_absolute() {
            return Err(SaveError::ForeignDocument);
        }
        let operation = self.next_operation()?;
        if self
            .partial_association
            .as_ref()
            .is_some_and(|(partial, _)| partial == &path)
        {
            // 此处只授予重试资格；实际覆盖还要核对侧目标的完整存储 bytes。
            self.status = SaveStatus::Idle;
        }
        self.enqueue(operation, snapshot, kind, path);
        Ok(operation)
    }

    fn apply_outcome(&mut self, job: &FrozenWrite, outcome: &WriteOutcome) -> bool {
        self.status = match outcome {
            WriteOutcome::Published {
                storage,
                synchronized,
            } => {
                match &job.mode {
                    WriteMode::Existing => {
                        self.target.saved = SavedBaseline {
                            logical: Some(job.snapshot.text().clone()),
                            storage: storage.clone(),
                        };
                    }
                    WriteMode::Associate { kind, .. } => {
                        if self
                            .associate_new_target_inner(
                                *kind,
                                job.path.clone(),
                                storage.clone(),
                                job.snapshot.text().clone(),
                            )
                            .is_err()
                        {
                            return false;
                        }
                    }
                }
                if *synchronized {
                    SaveStatus::Idle
                } else {
                    SaveStatus::PublishedUncertain
                }
            }
            WriteOutcome::PublishedTargetUnverified(_)
            | WriteOutcome::PublishedReceiptMismatch(_)
            | WriteOutcome::PublishedBaselineFailed { .. }
            | WriteOutcome::PublishedNewBaselineFailed { .. } => SaveStatus::PublishedUncertain,
            WriteOutcome::NotPublished(WriteStop::Cancelled) => return false,
            WriteOutcome::NotPublished(WriteStop::Conflict) => SaveStatus::Conflict,
            WriteOutcome::NotPublished(WriteStop::File(error)) => match error.kind() {
                AtomicFailureKind::Cancelled { .. } => return false,
                AtomicFailureKind::Conflict { .. } => SaveStatus::Conflict,
                _ => SaveStatus::Failed,
            },
            WriteOutcome::NotPublished(WriteStop::Create)
            | WriteOutcome::NotPublished(WriteStop::Preparation(_)) => SaveStatus::Failed,
        };
        true
    }

    fn associate_new_target_inner(
        &mut self,
        kind: WriteKind,
        path: PathBuf,
        baseline: SourceBaseline,
        logical: TextSnapshot,
    ) -> Result<(), SaveError> {
        if !path.is_absolute() {
            return Err(SaveError::InvalidPath);
        }
        let generation = match kind {
            WriteKind::SaveAs => PathGeneration(
                self.target
                    .generation
                    .0
                    .checked_add(1)
                    .ok_or(SaveError::Exhausted)?,
            ),
            WriteKind::Create | WriteKind::Existing => self.target.generation,
        };
        self.target = Target {
            path,
            generation,
            saved: SavedBaseline {
                logical: Some(logical),
                storage: baseline,
            },
            creating: false,
        };
        self.status = SaveStatus::Idle;
        self.partial_association = None;
        Ok(())
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

    /// 仅供 crate 内测试读取当前目标是否仍处于首次创建状态。
    #[cfg(test)]
    pub(super) fn is_creating(&self) -> bool {
        self.target.creating
    }

    /// 后台观察须记录起点路径代次，采纳时同当前关联复验。
    pub(super) fn path_generation(&self) -> PathGeneration {
        self.target.generation
    }

    /// 返回当前保存目标的绝对路径；宿主据此在目标同目录生成 staging 候选。
    pub(super) fn target_path(&self) -> &Path {
        &self.target.path
    }

    /// 返回当前保存基线；外改检查以它为比较对象。
    pub(super) fn saved_baseline(&self) -> &SourceBaseline {
        &self.target.saved.storage
    }

    /// 返回最近一次保存采纳的逻辑视图；恢复准备据此判断是否与保存点等价。
    pub(super) fn saved_logical(&self) -> Option<&TextSnapshot> {
        self.target.saved.logical.as_ref()
    }

    /// 仅在完全空闲且非不确定发布时把状态置为 Conflict；用于“已发现有本地修改的
    /// 外部变化”，让后续保存/关闭按既有冲突路径处理。返回是否生效。
    pub(super) fn mark_external_conflict(&mut self) -> bool {
        if self.active.is_some() || self.queued.is_some() || self.waiting.is_some() || self.closing
        {
            return false;
        }
        if self.status == SaveStatus::PublishedUncertain {
            return false;
        }
        self.status = SaveStatus::Conflict;
        true
    }

    /// 磁盘重新等于保存基线时解除外部冲突；不改其它状态。
    pub(super) fn clear_external_conflict(&mut self) {
        if self.status == SaveStatus::Conflict {
            self.status = SaveStatus::Idle;
        }
    }

    /// 采纳用户刚确认的完整磁盘观察为保存基线（确认覆盖 / 载入磁盘）。
    /// 守卫与 `replace_association` 相同，但绝不改路径、不 bump 代次、不做内容等价检查，
    /// 也不得替换出更晚的磁盘事实（调用方已按 `BaselineId` 复验）。成功后状态回 Idle。
    pub(super) fn adopt_external_baseline(
        &mut self,
        expected: PathGeneration,
        baseline: SourceBaseline,
    ) -> Result<(), SaveError> {
        if self.active.is_some() || self.queued.is_some() || self.waiting.is_some() {
            return Err(SaveError::Busy);
        }
        if self.closing {
            return Err(SaveError::Closing);
        }
        if self.target.generation != expected {
            return Err(SaveError::StaleAssociation);
        }
        self.target.saved = SavedBaseline {
            logical: None,
            storage: baseline,
        };
        self.status = SaveStatus::Idle;
        Ok(())
    }

    /// 显式建立已完成的路径关联。调用者须先在 DocCore 统一提交重载/引用修改，
    /// 并提供同一读盘结果的文本和存储基线。生产接线前仅 crate 测试调用。
    #[cfg(test)]
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
            creating: false,
        };
        self.status = SaveStatus::Idle;
        Ok(())
    }

    /// 请求关闭会停止待执行项和通知在途项取消。实际回执仍会完整结算。
    /// 只由测试专用的 `begin_close` 调用。
    #[cfg(test)]
    fn stop_for_close(&mut self) {
        self.closing = true;
        self.queued = None;
        if let Some(active) = &self.active {
            active.cancellation.cancel();
        }
    }

    #[cfg(test)]
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

    /// 关闭被拒的原因；只在 `close_status` 非 `Ready` 时有意义，且不改变任何状态。
    pub(super) fn close_blocker(&self) -> CloseBlocker {
        match self.status {
            SaveStatus::Failed => CloseBlocker::SaveFailed,
            SaveStatus::Conflict => CloseBlocker::SaveConflict,
            SaveStatus::PublishedUncertain => CloseBlocker::PublishedUncertain,
            SaveStatus::Idle | SaveStatus::WaitingForComposition | SaveStatus::Saving => {
                CloseBlocker::PendingChanges
            }
        }
    }
}

#[cfg(test)]
mod tests;
