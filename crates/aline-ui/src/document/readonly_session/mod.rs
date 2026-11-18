//! 单窗口文档打开生命周期；唯一持有 Ready 文档与至多一个打开请求。
//!
//! 输入为文件选择结果和后台完成候选，输出为当前只读文档与明确结算结果。
//! 请求资格在候选发布前复验；失败/取消保留原文档。本模块不调度线程、不写盘。
//! 读取适配器见 read 子模块，实际窗口宿主负责有限并发与任务回收。

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aline_markdown::{CancelToken, ParseBudget, ParseConfig, Parsed};
use aline_source::{AtomicFailureKind, IoBudget, SourceBaseline, StorageBudget};

use super::persistence::{
    CloseStatus, ExternalApplyResult, ExternalCheckCommand, ExternalCheckCompletion,
    ExternalCheckIntent, ExternalCheckJob, ExternalCheckRejection, ExternalConflictView,
    ExternalDiskState, ExternalObservation, OperationId, PersistenceCoordinator, SaveCommand,
    SaveError, SaveJob, SaveRejection, SaveRequest, SaveRequestError, SaveResult, SaveStatus,
    WriteCompletion, WriteOutcome, WriteSettlement, WriteStop, clamped_selection, preview,
};
use super::recovery::{
    PreparedRecovery, RecoveryAcceptOutcome, RecoveryAction, RecoveryBegin, RecoveryBeginOutcome,
    RecoveryCloseJob, RecoveryConfig, RecoveryConfigError, RecoveryDocumentId, RecoveryPath,
    RecoverySession, RecoverySettlement, RecoveryStart, RecoveryStore,
};
use super::{
    DocCore, DocError, DocLimits, EditorMode, InputIdentity, ModeSwitchOutcome, PublicationKind,
    PublicationOutcome, WaitHandle, WaitStatus,
};
use aline_buffer::{PatchBudget, ReloadOutcome, SrcSelection, TextSnapshot};
use aline_edit::{
    EditBudget, EditCommand, EditContext, EditMode, PlanOutcome, SourceEdit, plan_change,
};
use aline_markdown::parse_with_cancel;
use aline_source::Cancellation;

#[cfg(test)]
use super::ImageSettingsError;
use super::persistence::WriteKind;
use super::recovery::MAX_RESOURCE_COUNT;
use super::{
    ImageSettings, MigrationCompletion, MigrationError, ResourceCatalog, ResourceCleanupJob,
    ResourceError, ResourceMapping, ResourceMigrationJob, TempRoot, locator_spans,
};
mod autosave;
mod read;
mod recovered;
mod refresh;
mod timing;
pub(crate) use autosave::AutosaveAction;
use autosave::AutosaveScheduler;
pub use read::{ReadonlyFileAccess, ReadonlyFileInput};
#[cfg(test)]
pub(crate) use recovered::RecoveredOpenError;
use recovered::RecoveredOrigin;
pub use refresh::{ParseRefreshCompletion, ParseRefreshDisposition, ParseRefreshJob};
pub(crate) use timing::{StageTimer, StageTiming};

/// 打开输入、文档与解析的显式注入预算；默认只读，可为 M2 选择可编辑访问。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ReadonlyOpenLimits {
    storage: StorageBudget,
    document: DocLimits,
    config: ParseConfig,
    parse: ParseBudget,
    editable: bool,
}

impl ReadonlyOpenLimits {
    /// 构造完整打开预算；均为宿主注入值，不代表已验证的发布限额。
    pub const fn new(
        storage: StorageBudget,
        document: DocLimits,
        config: ParseConfig,
        parse: ParseBudget,
    ) -> Self {
        Self {
            storage,
            document,
            config,
            parse,
            editable: false,
        }
    }

    /// 将此组打开预算用于 M2 可编辑文档；历史预算仍由宿主显式注入。
    pub const fn with_editing(mut self) -> Self {
        self.editable = true;
        self
    }
}

/// 打开请求的实例身份；不能由路径或裸整数构造。
#[derive(Clone)]
pub struct OpenRequestId {
    session: Arc<()>,
    generation: u64,
}
impl PartialEq for OpenRequestId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.session, &other.session) && self.generation == other.generation
    }
}
impl Eq for OpenRequestId {}
impl fmt::Debug for OpenRequestId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("OpenRequestId")
            .field(&self.generation)
            .finish()
    }
}

/// 当前打开请求的实际阶段；Ready 文档与加载状态可以并存。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadonlyOpenPhase {
    /// 等待本地文件选择。
    Selecting,
    /// 受限读取、解码与解析候选；尚未发布。
    Loading,
}

struct PendingOpen {
    id: OpenRequestId,
    cancel: CancelToken,
    phase: ReadonlyOpenPhase,
}

/// 打开失败类别；翻译由原生 UI 边界完成，错误不携带文件正文或敏感路径。
#[derive(Debug)]
pub enum OpenError {
    /// 窗口会话已关闭。
    ClosedWindow,
    /// 请求代次已耗尽，原请求及文档不变。
    RequestExhausted,
    /// 选择结果不属于当前 Selecting 请求。
    StaleRequest,
    /// 必须恰好选择一个路径。
    SelectionCount,
    /// 文件扩展名不是 ASCII 大小写不敏感的 md。
    UnsupportedExtension,
    /// 已打开句柄不是普通文件。
    NotRegularFile,
    /// metadata 或实际读取超出注入的硬字节上限。
    BudgetExceeded,
    /// 读取已取消；不会发布部分候选。
    Cancelled,
    /// 文件打开、metadata 或读取失败。
    Io(std::io::ErrorKind),
    /// 适配器违反 Read 返回长度约束。
    InvalidRead,
    /// 受检存储视图构造失败。
    Storage(aline_source::StorageError),
    /// 不是受支持的有效 UTF-8。
    Decode(aline_source::DecodeError),
    /// 不开块的核心构造或身份复验失败。
    Document(super::DocError),
    /// 解析硬预算、取消或映射失败。
    Parse(aline_markdown::ParseError),
    /// 受检内存分配失败。
    Allocation(std::collections::TryReserveError),
}
impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}
impl std::error::Error for OpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Storage(error) => Some(error),
            Self::Decode(error) => Some(error),
            Self::Document(error) => Some(error),
            Self::Parse(error) => Some(error),
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

/// 保存入口的下一步；首次保存/另存为先要求宿主选择目标。
// 命令枚举只在宿主入口短时存在，按值携带写盘任务避免额外一次装箱。
#[allow(clippy::large_enum_variant)]
#[allow(dead_code)]
pub(crate) enum SaveTargetCommand {
    Save(SaveCommand),
    ChooseTarget { kind: WriteKind },
    RetryAssociation,
}

/// 关联（迁移+写盘）阶段的命令；宿主据此等待、派生任务或反馈。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum AssociationCommand {
    Run(ResourceMigrationJob),
    /// 载荷句柄由宿主等待匹配消费；测试只匹配变体。
    #[allow(dead_code)]
    Wait(WaitHandle),
    /// 载荷保存命令由宿主执行；测试只匹配变体。
    #[allow(dead_code)]
    Save(SaveCommand),
    Cancelled,
    Rejected(SaveRejection),
}

/// 迁移完成后的下一步。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum AssociationStep {
    /// 载荷由宿主派生后台复制；测试只在重试入口生成，不读取内部任务。
    #[allow(dead_code)]
    Run(ResourceMigrationJob),
    Write(SaveJob),
    Settled(AssociationOutcome),
}

/// 关联流程的结算分类；宿主只按类别选反馈键。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum AssociationOutcome {
    NeedsRetry,
    Failed,
    Cancelled,
    Stale,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
enum AssociationPhase {
    Waiting,
    Copying,
    Writing,
}

#[cfg_attr(not(test), allow(dead_code))]
struct AssociationState {
    operation: u64,
    kind: WriteKind,
    target: PathBuf,
    cancellation: Cancellation,
    phase: AssociationPhase,
    wait: Option<WaitHandle>,
    save_operation: Option<OperationId>,
}

#[cfg_attr(not(test), allow(dead_code))]
struct PendingMigration {
    target: PathBuf,
    kind: WriteKind,
    /// 上一次引用映射；重试直接复用已重写文本，不再重新扫描。
    #[allow(dead_code)]
    mappings: Vec<ResourceMapping>,
}

/// 已完整构造且可阅读的单文档；存储基线与 DocCore 共同保留读取事实。
pub struct ReadonlyDocument {
    path: PathBuf,
    baseline: SourceBaseline,
    core: DocCore,
    parsed: Parsed,
    parsed_identity: InputIdentity,
    pending_parse: Option<refresh::PendingParse>,
    editable: bool,
    limits: DocLimits,
    /// 可编辑且目标为绝对路径时持有唯一保存协调器；只读或无绝对目标时为 None。
    persistence: Option<PersistenceCoordinator>,
    /// 保存编码与 I/O 预算，来自打开时注入的存储预算。
    storage_budget: StorageBudget,
    /// 仅呈现的外部冲突视图；Dismiss 只清视图，不清冲突状态与暂停。
    external: Option<ExternalConflictView>,
    /// 外部删除/不可读/未解码或脏冲突期间的粘性自动保存暂停。
    external_pause: bool,
    /// 撤销越过重载边界的粘性暂停。
    reload_pause: bool,
    /// 最近一次 Changed 重载后的 `undo_depth()`。
    reload_mark: Option<usize>,
    /// 1 秒防抖自动保存计时；时间由宿主注入。
    autosave: AutosaveScheduler,
    /// 独立 10 秒恢复会话；只在可编辑文档挂接，与自动保存互不共享状态。
    recovery: Option<RecoverySession>,
    /// 文档私有临时资源目录；首次插图时懒创建，未插图文档不创建目录。
    resources: Option<ResourceCatalog>,
    /// 会话注入的受检临时根；无根时插图返回 `UnsupportedPlatform`。
    temp_root: Option<TempRoot>,
    /// 内部图片资产设置；4.2 不提供 UI、不持久化。
    image_settings: ImageSettings,
    /// 首次保存/另存为的资源迁移与写盘关联。
    association: Option<AssociationState>,
    /// 已完成的引用迁移；写入失败时保留，供同一目标重试。
    pending_migration: Option<PendingMigration>,
    /// 关联操作序号；迟到完成项按此丢弃。
    #[cfg_attr(not(test), allow(dead_code))]
    next_association: u64,
    /// 打开时注入的解析预算；引用重写重新解析时复用。
    #[cfg_attr(not(test), allow(dead_code))]
    parse_budget: ParseBudget,
    /// 恢复专用来源；普通打开为 None。
    recovered_origin: Option<RecoveredOrigin>,
    /// 恢复文档在首次关联成功前不挂接的恢复配置。
    deferred_recovery: Option<RecoveryConfig>,
    /// 首次关联成功后由宿主消费一次的恢复布防标志。
    recovery_pending_arm: bool,
}
impl ReadonlyDocument {
    /// 返回用户选择的文件路径，供标题等显式 UI 使用。
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// 返回当前磁盘基线：打开时为原 bytes/BOM/行尾，成功保存后替换为实际写出快照。
    pub fn baseline(&self) -> &SourceBaseline {
        &self.baseline
    }
    /// 借用唯一文档核心；其访问策略由打开预算决定。
    pub fn core(&self) -> &DocCore {
        &self.core
    }
    /// 仅向原生宿主借用当前可编辑核心，核心仍负责每次命令的受检写入。
    #[cfg(target_os = "macos")]
    pub(crate) fn edit_core(&mut self) -> Result<&mut DocCore, DocError> {
        if !self.editable {
            return Err(DocError::ReadOnly);
        }
        Ok(&mut self.core)
    }

    /// 受检撤销：Visual 下若下一次撤销触及 MD-11 保护区间，零副作用拒绝。
    ///
    /// 拒绝不推进版本、选区、草稿、历史或解析；用户可切到 Source 后重试。
    pub(crate) fn checked_undo(&mut self) -> Result<(), DocError> {
        if !self.editable {
            return Err(DocError::ReadOnly);
        }
        if self.core.mode() == EditorMode::Visual
            && let Some(refusal) = self.protected_intersection(self.core.undo_target_ranges())
        {
            return Err(DocError::Edit(refusal));
        }
        self.core.undo()
    }

    /// 受检重做：Visual 下若下一次重做触及 MD-11 保护区间，零副作用拒绝。
    ///
    /// 拒绝不推进版本、选区、草稿、历史或解析；用户可切到 Source 后重试。
    pub(crate) fn checked_redo(&mut self) -> Result<(), DocError> {
        if !self.editable {
            return Err(DocError::ReadOnly);
        }
        if self.core.mode() == EditorMode::Visual
            && let Some(refusal) = self.protected_intersection(self.core.redo_target_ranges())
        {
            return Err(DocError::Edit(refusal));
        }
        self.core.redo()
    }

    /// 返回目标范围与当前解析保护区间的首个交集；闭区间相交与输入检查一致。
    fn protected_intersection(
        &self,
        ranges: Option<Vec<std::ops::Range<usize>>>,
    ) -> Option<aline_edit::EditRefusal> {
        for range in ranges? {
            for protected in self.parsed.protected_ranges() {
                let protected_range = protected.range().byte_range();
                if range.start <= protected_range.end && protected_range.start <= range.end {
                    return Some(aline_edit::EditRefusal::ProtectedRange {
                        kind: protected.kind(),
                        range: protected_range,
                    });
                }
            }
        }
        None
    }
    /// 返回该打开请求是否采用可编辑访问策略。
    pub const fn is_editable(&self) -> bool {
        self.editable
    }
    /// 冻结当前文档快照，使用本次打开时注入的文本预算。
    pub fn current_snapshot(&mut self) -> Result<super::CurrentSnapshot, DocError> {
        self.core.current_snapshot(self.limits.text())
    }
    /// 冻结当前文档输入身份，供布局与派生结果受检消费。
    pub fn input_identity(&mut self) -> Result<InputIdentity, DocError> {
        self.core.current_input_identity()
    }

    /// 请求一次显式保存：组合期进入 ED-01 可取消等待，否则冻结当前快照排队。
    pub(in crate::document) fn request_persistence_save(
        &mut self,
    ) -> Result<SaveRequest, SaveRequestError> {
        if !self.editable {
            return Err(SaveRequestError::Input(DocError::ReadOnly));
        }
        if self.association.is_some() || self.pending_migration.is_some() {
            return Err(SaveRequestError::Queue(SaveError::Busy));
        }
        let coordinator = self
            .persistence
            .as_mut()
            .ok_or(SaveRequestError::Queue(SaveError::InvalidPath))?;
        coordinator.request_save(&mut self.core, self.limits.text())
    }

    /// 写入一条临时图片并登记到 `DocCore`，不插入 Markdown、不改选区与历史。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn stage_temp_image(
        &mut self,
        display_name: &str,
        bytes: &[u8],
        max_bytes: u64,
    ) -> Result<super::ResourceReference, ResourceError> {
        if !self.editable {
            return Err(ResourceError::NotEditable);
        }
        let identity = self.input_identity().map_err(ResourceError::Doc)?;
        if self.resources.is_none() {
            let root = self
                .temp_root
                .clone()
                .ok_or(ResourceError::UnsupportedPlatform)?;
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| {
                    u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX)
                });
            self.resources = Some(ResourceCatalog::new(
                self.core.document_id().clone(),
                &root,
                nanos,
            )?);
        }
        let Some(catalog) = self.resources.as_mut() else {
            return Err(ResourceError::Io);
        };
        let entry = catalog.stage(display_name, bytes, max_bytes)?;
        if let Err(error) =
            self.core
                .retain_resource(&identity, entry.reference.clone(), MAX_RESOURCE_COUNT)
        {
            let removed = catalog.remove(entry.id);
            if let Some(removed) = removed {
                let _ = catalog.discard_file(&removed);
            }
            return Err(ResourceError::Doc(error));
        }
        Ok(entry.reference)
    }

    /// 首次保存/另存为入口：迁移意图优先重试；已有目标直接保存；无目标要求选择。
    #[allow(dead_code)]
    pub(crate) fn request_save_or_target(&mut self) -> SaveTargetCommand {
        if self.association.is_some() {
            return SaveTargetCommand::Save(SaveCommand::Rejected(SaveRejection::Busy));
        }
        if !self.editable {
            return SaveTargetCommand::Save(SaveCommand::Rejected(SaveRejection::ReadOnly));
        }
        if self.pending_migration.is_some() {
            return SaveTargetCommand::RetryAssociation;
        }
        if self.persistence.is_some() {
            return SaveTargetCommand::Save(self.request_save_command());
        }
        SaveTargetCommand::ChooseTarget {
            kind: WriteKind::Create,
        }
    }

    /// 另存为总是先请宿主选择目标；选定后交给 `begin_association_save`。
    #[allow(dead_code)]
    pub(crate) fn request_save_as_target(&mut self) -> SaveTargetCommand {
        if self.association.is_some() {
            return SaveTargetCommand::Save(SaveCommand::Rejected(SaveRejection::Busy));
        }
        if !self.editable {
            return SaveTargetCommand::Save(SaveCommand::Rejected(SaveRejection::ReadOnly));
        }
        SaveTargetCommand::ChooseTarget {
            kind: WriteKind::SaveAs,
        }
    }

    /// 用重试时保留的材料重新规划迁移或直接重新入队写盘；不重新选择路径。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn retry_association(&mut self) -> Result<AssociationStep, SaveRejection> {
        if !self.editable {
            return Err(SaveRejection::ReadOnly);
        }
        if self.association.is_some() || self.core.composition().is_some() {
            return Err(SaveRejection::Busy);
        }
        let (target, kind) = match self.pending_migration.as_ref() {
            Some(pending) => (pending.target.clone(), pending.kind),
            None => return Err(SaveRejection::NoTarget),
        };
        let snapshot = self
            .current_snapshot()
            .map_err(|_| SaveRejection::Internal)?;
        let source = snapshot
            .full_text(self.limits.text())
            .map_err(|_| SaveRejection::Internal)?;
        let has_locator = self.resources.as_ref().is_some_and(|catalog| {
            catalog
                .entries()
                .iter()
                .any(|entry| source.contains(&entry.locator))
        });
        let operation = self.next_association;
        self.next_association = self.next_association.wrapping_add(1);
        let cancellation = Cancellation::new();
        if has_locator {
            let job =
                self.freeze_migration(operation, kind, target.clone(), cancellation.clone())?;
            self.association = Some(AssociationState {
                operation,
                kind,
                target,
                cancellation,
                phase: AssociationPhase::Copying,
                wait: None,
                save_operation: None,
            });
            return Ok(AssociationStep::Run(job));
        }
        let (job, save_operation) = self.enqueue_association_write(kind, target.clone())?;
        self.association = Some(AssociationState {
            operation,
            kind,
            target,
            cancellation,
            phase: AssociationPhase::Writing,
            wait: None,
            save_operation: Some(save_operation),
        });
        Ok(AssociationStep::Write(job))
    }

    /// 关联等待是否仍属于当前状态；迟到句柄不得结算新关联。
    #[allow(dead_code)]
    pub(crate) fn association_wait_matches(&self, handle: WaitHandle) -> bool {
        self.association.as_ref().is_some_and(|state| {
            state.phase == AssociationPhase::Waiting && state.wait == Some(handle)
        })
    }

    /// 平台自然结束关联等待：确认后进入复制阶段，取消则清理状态。
    #[allow(dead_code)]
    pub(crate) fn finish_association_wait(
        &mut self,
        handle: WaitHandle,
        outcome: PublicationOutcome,
        now_millis: u64,
    ) -> AssociationCommand {
        let Some(state) = self.association.as_ref() else {
            return AssociationCommand::Rejected(SaveRejection::Internal);
        };
        if state.phase != AssociationPhase::Waiting || state.wait != Some(handle) {
            return AssociationCommand::Rejected(SaveRejection::Internal);
        }
        let operation = state.operation;
        let kind = state.kind;
        let target = state.target.clone();
        let cancellation = state.cancellation.clone();
        if self
            .core
            .finish_publication(handle, outcome, now_millis)
            .is_err()
        {
            return AssociationCommand::Rejected(SaveRejection::Internal);
        }
        if outcome == PublicationOutcome::PlatformCancelled {
            self.association = None;
            return AssociationCommand::Cancelled;
        }
        let job = match self.freeze_migration(operation, kind, target, cancellation) {
            Ok(job) => job,
            Err(rejection) => {
                self.association = None;
                return AssociationCommand::Rejected(rejection);
            }
        };
        if let Some(state) = self.association.as_mut() {
            state.phase = AssociationPhase::Copying;
            state.wait = None;
        }
        AssociationCommand::Run(job)
    }

    /// 开始一次首次保存/另存为关联：校验目标、组合等待或冻结迁移输入。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn begin_association_save(
        &mut self,
        kind: WriteKind,
        target: PathBuf,
        _now_millis: u64,
    ) -> AssociationCommand {
        if !self.editable {
            return AssociationCommand::Rejected(SaveRejection::ReadOnly);
        }
        if self.association.is_some() || self.pending_migration.is_some() {
            return AssociationCommand::Rejected(SaveRejection::Busy);
        }
        if !target.is_absolute() {
            return AssociationCommand::Rejected(SaveRejection::NoTarget);
        }
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) if metadata.is_dir() => {
                return AssociationCommand::Rejected(SaveRejection::NoTarget);
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return AssociationCommand::Rejected(SaveRejection::NoTarget),
        }
        // 无协调器（未命名/恢复文档）时首次保存一律按 Create；展示路径不构成关联。
        let kind = if self.persistence.is_none() {
            WriteKind::Create
        } else {
            kind
        };
        if self.persistence.is_some() && target == self.path {
            return AssociationCommand::Save(self.request_save_command());
        }
        let operation = self.next_association;
        self.next_association = self.next_association.wrapping_add(1);
        let cancellation = Cancellation::new();
        if self.core.composition().is_some() {
            let handle = match self.core.begin_publication_wait(PublicationKind::Save) {
                Ok(handle) => handle,
                Err(_) => return AssociationCommand::Rejected(SaveRejection::Internal),
            };
            self.association = Some(AssociationState {
                operation,
                kind,
                target,
                cancellation,
                phase: AssociationPhase::Waiting,
                wait: Some(handle),
                save_operation: None,
            });
            return AssociationCommand::Wait(handle);
        }
        let job = match self.freeze_migration(operation, kind, target.clone(), cancellation.clone())
        {
            Ok(job) => job,
            Err(rejection) => return AssociationCommand::Rejected(rejection),
        };
        self.association = Some(AssociationState {
            operation,
            kind,
            target,
            cancellation,
            phase: AssociationPhase::Copying,
            wait: None,
            save_operation: None,
        });
        AssociationCommand::Run(job)
    }

    /// 取消等待或复制任务；已排队/在途写盘按协调器取消，不删除临时资源。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn cancel_association(&mut self) {
        let Some(state) = self.association.take() else {
            return;
        };
        // 复制阶段也必须取消同一迁移任务的取消句柄，否则任务会无主继续写盘。
        state.cancellation.cancel();
        if state.phase == AssociationPhase::Waiting
            && let Some(handle) = state.wait
        {
            let _ = self.core.request_cancel_wait(handle);
        }
        if let Some(operation) = state.save_operation
            && let Some(coordinator) = self.persistence.as_mut()
        {
            coordinator.cancel(operation);
        }
    }

    /// 冻结迁移输入；临时根缺失时条目为空，SaveAs 仍可复制既有相对资产。
    #[cfg_attr(not(test), allow(dead_code))]
    fn freeze_migration(
        &mut self,
        operation: u64,
        kind: WriteKind,
        target: PathBuf,
        cancellation: Cancellation,
    ) -> Result<ResourceMigrationJob, SaveRejection> {
        let snapshot = self
            .current_snapshot()
            .map_err(|_| SaveRejection::Internal)?;
        let (entries, temp_directory) = match self.resources.as_ref() {
            Some(catalog) => (
                catalog.entries().to_vec(),
                catalog.directory().to_path_buf(),
            ),
            None => (Vec::new(), PathBuf::new()),
        };
        let current_dir = if self.path.is_absolute() {
            self.path.parent().map(Path::to_path_buf)
        } else {
            None
        };
        Ok(ResourceMigrationJob::new(
            self.core.document_id().clone(),
            operation,
            kind,
            target,
            current_dir,
            snapshot,
            entries,
            self.image_settings.clone(),
            temp_directory,
            self.limits.text(),
            cancellation,
        ))
    }

    /// 迁移完成后按关联目标入队并生成后台写盘工作。
    #[cfg_attr(not(test), allow(dead_code))]
    fn enqueue_association_write(
        &mut self,
        kind: WriteKind,
        target: PathBuf,
    ) -> Result<(SaveJob, OperationId), SaveRejection> {
        if self.persistence.is_none() {
            let coordinator = PersistenceCoordinator::new_untitled(
                self.core.document_id().clone(),
                target.clone(),
                self.baseline.clone(),
            )
            .map_err(|_| SaveRejection::Internal)?;
            self.persistence = Some(coordinator);
        }
        let staging = save_staging_candidate(&target)?;
        let request = {
            let coordinator = self.persistence.as_mut().ok_or(SaveRejection::Internal)?;
            match kind {
                WriteKind::SaveAs => {
                    coordinator.request_save_as(&mut self.core, self.limits.text(), target.clone())
                }
                WriteKind::Create | WriteKind::Existing => {
                    coordinator.request_save(&mut self.core, self.limits.text())
                }
            }
        };
        let operation = match request {
            Ok(SaveRequest::Queued(operation)) => operation,
            Ok(SaveRequest::Waiting(_)) => return Err(SaveRejection::Busy),
            Err(error) => return Err(save_request_rejection(&error)),
        };
        let job = self.start_queued_save(operation, staging)?;
        Ok((job, operation))
    }

    /// 失败后保留同一目标/类型与本次引用映射，供 `retry_association` 直接重试。
    ///
    /// 仅在关联仍存在时记录，避免覆盖此前保留的待重试目标；回滚语义不变：
    /// 只撤回本次新建资产，文档字节、目标与用户模式均不改动。
    fn retain_pending_migration(&mut self, mappings: Vec<ResourceMapping>) {
        if let Some(state) = self.association.as_ref() {
            self.pending_migration = Some(PendingMigration {
                target: state.target.clone(),
                kind: state.kind,
                mappings,
            });
        }
    }

    /// 按固定顺序复核迁移完成项：过期/取消/新引用/编辑拒绝/入队写盘。
    pub(crate) fn association_prepared(
        &mut self,
        completion: MigrationCompletion,
        now_millis: u64,
    ) -> AssociationStep {
        let Some(state) = self.association.as_ref() else {
            return AssociationStep::Settled(AssociationOutcome::Stale);
        };
        if &completion.document != self.core.document_id()
            || completion.operation != state.operation
        {
            return AssociationStep::Settled(AssociationOutcome::Stale);
        }
        if completion.cancelled {
            self.association = None;
            return AssociationStep::Settled(AssociationOutcome::Cancelled);
        }
        let migration = match completion.result {
            Ok(migration) => migration,
            Err(error) => {
                if error == MigrationError::StaleResourceSet {
                    // 资源集已变化：保留同一目标，重试时按当前目录重新迁移。
                    self.retain_pending_migration(Vec::new());
                }
                self.association = None;
                return AssociationStep::Settled(if error == MigrationError::StaleResourceSet {
                    AssociationOutcome::NeedsRetry
                } else {
                    AssociationOutcome::Failed
                });
            }
        };
        let identity = match self.input_identity() {
            Ok(identity) => identity,
            Err(_) => {
                migration.rollback_assets();
                self.retain_pending_migration(migration.mappings.clone());
                self.association = None;
                return AssociationStep::Settled(AssociationOutcome::NeedsRetry);
            }
        };
        let snapshot = match self.current_snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => {
                migration.rollback_assets();
                self.retain_pending_migration(migration.mappings.clone());
                self.association = None;
                return AssociationStep::Settled(AssociationOutcome::NeedsRetry);
            }
        };
        let source = match snapshot.full_text(self.limits.text()) {
            Ok(source) => source,
            Err(_) => {
                migration.rollback_assets();
                self.retain_pending_migration(migration.mappings.clone());
                self.association = None;
                return AssociationStep::Settled(AssociationOutcome::NeedsRetry);
            }
        };
        let mappings = &migration.mappings;
        let new_locator = self.resources.as_ref().is_some_and(|catalog| {
            catalog.entries().iter().any(|entry| {
                source.contains(&entry.locator)
                    && !mappings.iter().any(|mapping| mapping.from == entry.locator)
            })
        });
        if new_locator {
            migration.rollback_assets();
            self.retain_pending_migration(migration.mappings.clone());
            self.association = None;
            return AssociationStep::Settled(AssociationOutcome::NeedsRetry);
        }
        let spans = locator_spans(&source, mappings);
        if !spans.is_empty() {
            let edits = spans
                .iter()
                .map(|(range, index)| {
                    let mapping = &mappings[*index];
                    SourceEdit::new(range.clone(), mapping.from.clone(), mapping.to.clone())
                })
                .collect();
            let parsed_local;
            let parsed =
                if self.is_parse_current() && self.parsed.snapshot().id() == snapshot.text().id() {
                    &self.parsed
                } else {
                    let cancel = CancelToken::new();
                    match parse_with_cancel(
                        snapshot.text(),
                        self.parsed.config(),
                        self.parse_budget,
                        &cancel,
                    ) {
                        Ok(parsed) => {
                            parsed_local = parsed;
                            &parsed_local
                        }
                        Err(_) => {
                            migration.rollback_assets();
                            self.retain_pending_migration(migration.mappings.clone());
                            self.association = None;
                            return AssociationStep::Settled(AssociationOutcome::NeedsRetry);
                        }
                    }
                };
            let inserted: usize = spans
                .iter()
                .map(|(_, index)| mappings[*index].to.len())
                .sum();
            let output = source.len().saturating_add(inserted);
            let budget = EditBudget::new(
                PatchBudget::new(spans.len(), inserted, output),
                self.limits.text(),
                self.parse_budget,
            );
            let command = EditCommand::replace(edits);
            // 引用重写是系统迁移，不是用户 Visual 编辑；固定 Source 上下文，
            // 不改变用户视图，也不把 Visual 权限用于受保护位置。
            let context = EditContext::new(EditMode::Source);
            match plan_change(
                snapshot.text(),
                parsed,
                snapshot.selection(),
                &context,
                &command,
                budget,
            ) {
                Ok(PlanOutcome::Changed(plan)) => {
                    if self
                        .core
                        .apply_migration_change(&identity, parsed, &plan, now_millis)
                        .is_err()
                    {
                        migration.rollback_assets();
                        self.retain_pending_migration(migration.mappings.clone());
                        self.association = None;
                        return AssociationStep::Settled(AssociationOutcome::NeedsRetry);
                    }
                }
                Ok(PlanOutcome::Unchanged(_)) => {}
                Err(_) => {
                    migration.rollback_assets();
                    self.retain_pending_migration(migration.mappings.clone());
                    self.association = None;
                    return AssociationStep::Settled(AssociationOutcome::NeedsRetry);
                }
            }
        }
        let (target, kind) = match self.association.as_ref() {
            Some(state) => (state.target.clone(), state.kind),
            None => return AssociationStep::Settled(AssociationOutcome::Stale),
        };
        match self.enqueue_association_write(kind, target.clone()) {
            Ok((job, operation)) => {
                if let Some(state) = self.association.as_mut() {
                    state.phase = AssociationPhase::Writing;
                    state.save_operation = Some(operation);
                }
                self.pending_migration = Some(PendingMigration {
                    target,
                    kind,
                    mappings: migration.mappings,
                });
                AssociationStep::Write(job)
            }
            Err(_) => {
                self.association = None;
                AssociationStep::Settled(AssociationOutcome::Failed)
            }
        }
    }

    /// 关闭就绪判定：关联等待、组合、待重试迁移与保存协调器统一在此收口。
    pub(crate) fn close_status(&mut self) -> CloseStatus {
        if self.association.is_some() {
            return CloseStatus::Waiting;
        }
        if self.core.composition().is_some()
            || self
                .core
                .wait()
                .is_some_and(|wait| wait.status() == WaitStatus::Pending)
        {
            return CloseStatus::Waiting;
        }
        if self.pending_migration.is_some() {
            return CloseStatus::NeedsDecision;
        }
        if self.persistence.is_none() {
            let Ok(snapshot) = self.current_snapshot() else {
                return CloseStatus::NeedsDecision;
            };
            return if snapshot
                .full_text(self.limits.text())
                .is_ok_and(|text| text == self.baseline.source())
            {
                CloseStatus::Ready
            } else {
                CloseStatus::NeedsDecision
            };
        }
        let Ok(snapshot) = self.current_snapshot() else {
            return CloseStatus::NeedsDecision;
        };
        self.persistence
            .as_ref()
            .map_or(CloseStatus::NeedsDecision, |coordinator| {
                coordinator.close_status(&snapshot)
            })
    }

    /// 捕获清理任务所需的恢复引用上下文；不移动资源目录，供替换在
    /// `prepare` 成功后再退役旧文档资源。
    pub(crate) fn recovery_cleanup_context(&self) -> Option<(RecoveryStore, RecoveryDocumentId)> {
        self.recovery
            .as_ref()
            .and_then(RecoverySession::cleanup_context)
    }

    /// 取走资源目录并生成清理任务；只有文档确定释放后才能交给后台执行。
    pub(crate) fn retire_resources(
        &mut self,
        recovery: Option<(RecoveryStore, RecoveryDocumentId)>,
    ) -> Option<ResourceCleanupJob> {
        let catalog = self.resources.take()?;
        Some(ResourceCleanupJob::new(
            catalog.directory().to_path_buf(),
            catalog.entries().to_vec(),
            recovery,
        ))
    }

    /// 会话打开文档后注入资源设置与受检临时根；普通打开只注入一次。
    fn apply_resource_config(&mut self, settings: ImageSettings, root: Option<TempRoot>) {
        self.image_settings = settings;
        self.temp_root = root;
    }

    /// 取走已排队的保存，生成可离开 UI 线程执行的后台工作；
    /// staging 必须与目标同目录且不存在。
    pub(in crate::document) fn start_save_job(
        &mut self,
        staging: PathBuf,
    ) -> Result<SaveJob, SaveRequestError> {
        if !self.editable {
            return Err(SaveRequestError::Input(DocError::ReadOnly));
        }
        let coordinator = self
            .persistence
            .as_mut()
            .ok_or(SaveRequestError::Queue(SaveError::InvalidPath))?;
        let frozen = coordinator.start()?;
        Ok(SaveJob::new(
            frozen,
            self.limits.text(),
            self.storage_budget,
            IoBudget::new(self.storage_budget.max_bytes()),
            staging,
        ))
    }

    /// 结算一次真实写盘完成项；采纳成功回执时同时替换文档的磁盘基线。
    pub(in crate::document) fn settle_save(
        &mut self,
        completion: WriteCompletion,
    ) -> Result<WriteSettlement, SaveRequestError> {
        // 与另外两个保存入口保持一致：只读文档在排查误接线时先报只读，
        // 而不是落到无协调器的 InvalidPath。
        if !self.editable {
            return Err(SaveRequestError::Input(DocError::ReadOnly));
        }
        let coordinator = self
            .persistence
            .as_mut()
            .ok_or(SaveRequestError::Queue(SaveError::InvalidPath))?;
        let settlement = coordinator.reconcile(completion);
        if settlement.adopted
            && let WriteOutcome::Published { storage, .. } = &settlement.outcome
        {
            // 成功发布即按实际写出字节建立磁盘基线；即使目录同步未确认
            //（synchronized=false）也与协调器保存基线保持一致，关闭 guard
            // 仍按 PublishedUncertain 要求用户处理。PublishedBaselineFailed 不替换。
            self.baseline = storage.clone();
            // Ruling W4-32：显式保存成功后恢复正常自动保存策略。
            self.external = None;
            self.external_pause = false;
            self.reload_pause = false;
            self.reload_mark = None;
        }
        if settlement.adopted {
            if let Some((path, baseline)) = settlement.association.clone() {
                // 首次保存/另存为已发布：展示路径与实际基线一起改为新目标。
                self.path = path;
                self.baseline = baseline;
                self.external = None;
                self.external_pause = false;
                self.reload_pause = false;
                self.reload_mark = None;
                self.association = None;
                self.pending_migration = None;
                self.attach_deferred_recovery();
            } else if self.association.is_some() {
                match &settlement.outcome {
                    WriteOutcome::PublishedTargetUnverified(_)
                    | WriteOutcome::PublishedReceiptMismatch(_)
                    | WriteOutcome::PublishedBaselineFailed { .. }
                    | WriteOutcome::PublishedNewBaselineFailed { .. } => {
                        // 已发布但基线不可确认：路径随实际目标更新，保留关闭 guard 的
                        // PublishedUncertain 判定；没有可采纳的基线可替换。
                        self.path = settlement.path.clone();
                        self.external = None;
                        self.external_pause = false;
                        self.reload_pause = false;
                        self.reload_mark = None;
                        self.association = None;
                        self.pending_migration = None;
                        self.attach_deferred_recovery();
                    }
                    WriteOutcome::NotPublished(_) => {
                        // 写入失败/冲突：保留已完成的迁移映射供同一目标重试。
                        self.association = None;
                    }
                    WriteOutcome::Published { .. } => {}
                }
            }
        }
        Ok(settlement)
    }

    /// 首次关联写盘成功后才授予恢复挂接；普通保存不重置既有恢复会话。
    fn attach_deferred_recovery(&mut self) {
        if self.recovery.is_some() {
            return;
        }
        let Some(config) = self.deferred_recovery.take() else {
            return;
        };
        self.recovery = Some(RecoverySession::new(config));
        self.recovery_pending_arm = true;
    }

    /// 取走首次关联成功的一次性布防标志；宿主只在为 true 时布防恢复计时。
    pub(crate) fn take_recovery_pending_arm(&mut self) -> bool {
        let pending = self.recovery_pending_arm;
        self.recovery_pending_arm = false;
        pending
    }

    /// 关联等待/复制/写盘或待重试迁移是否活动。
    pub(crate) fn association_active(&self) -> bool {
        self.association.is_some() || self.pending_migration.is_some()
    }

    /// 请求一次外改检查；只读/无目标 → NoTarget，组合 → Deferred，在途/不确定按类别拒绝。
    pub(crate) fn request_external_check(
        &self,
        intent: ExternalCheckIntent,
    ) -> ExternalCheckCommand {
        if !self.editable {
            return ExternalCheckCommand::Rejected(ExternalCheckRejection::NoTarget);
        }
        let Some(coordinator) = self.persistence.as_ref() else {
            return ExternalCheckCommand::Rejected(ExternalCheckRejection::NoTarget);
        };
        if self.core.composition().is_some() {
            return ExternalCheckCommand::Rejected(ExternalCheckRejection::Deferred);
        }
        match coordinator.status() {
            SaveStatus::Saving => {
                return ExternalCheckCommand::Rejected(ExternalCheckRejection::Busy);
            }
            SaveStatus::WaitingForComposition => {
                return ExternalCheckCommand::Rejected(ExternalCheckRejection::Deferred);
            }
            SaveStatus::PublishedUncertain => {
                return ExternalCheckCommand::Rejected(ExternalCheckRejection::Uncertain);
            }
            SaveStatus::Idle | SaveStatus::Failed | SaveStatus::Conflict => {}
        }
        ExternalCheckCommand::Run(ExternalCheckJob::new(
            intent,
            coordinator.saved_baseline().clone(),
            coordinator.target_path().to_path_buf(),
            self.storage_budget,
            IoBudget::new(self.storage_budget.max_bytes()),
        ))
    }

    /// 按冻结顺序结算后台观察；任何未通过复验的完成项都零副作用。
    pub(crate) fn apply_external_check(
        &mut self,
        completion: ExternalCheckCompletion,
        now_millis: u64,
    ) -> ExternalApplyResult {
        if !self.editable || self.persistence.is_none() {
            return ExternalApplyResult::Stale;
        }
        if self.core.composition().is_some() {
            return ExternalApplyResult::Deferred;
        }
        let status = self
            .persistence
            .as_ref()
            .map(PersistenceCoordinator::status);
        if matches!(
            status,
            Some(
                SaveStatus::Saving
                    | SaveStatus::WaitingForComposition
                    | SaveStatus::PublishedUncertain
            )
        ) {
            return ExternalApplyResult::Stale;
        }
        let stale_association = self.persistence.as_ref().is_none_or(|coordinator| {
            completion.path() != coordinator.target_path()
                || coordinator.saved_baseline().id() != completion.expected().id()
        });
        if stale_association {
            return ExternalApplyResult::Stale;
        }
        let snapshot = match self.current_snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => return ExternalApplyResult::Failed,
        };
        let dirty = self
            .persistence
            .as_ref()
            .is_some_and(|coordinator| coordinator.is_dirty(&snapshot));
        let local_preview = match snapshot.full_text(self.limits.text()) {
            Ok(text) => preview(&text),
            Err(_) => return ExternalApplyResult::Failed,
        };
        match completion.observation() {
            ExternalObservation::Unchanged => {
                self.external = None;
                self.external_pause = false;
                if let Some(coordinator) = self.persistence.as_mut() {
                    coordinator.clear_external_conflict();
                }
                ExternalApplyResult::Unchanged
            }
            ExternalObservation::Changed { baseline } => {
                if completion.intent() == ExternalCheckIntent::Overwrite {
                    if !self.adopt_disk_baseline(baseline.clone()) {
                        return ExternalApplyResult::Stale;
                    }
                    self.baseline = baseline.clone();
                    self.external = None;
                    self.external_pause = false;
                    return ExternalApplyResult::OverwriteReady;
                }
                // Ruling W4-34：显式「载入磁盘」是用户的确认动作，脏文档也直接走
                // 同一条重载路径并建立可撤销历史边界；只有 Focus/Retry 在脏时重新
                // 呈现冲突，等待用户选择出口。
                if dirty
                    && matches!(
                        completion.intent(),
                        ExternalCheckIntent::Focus | ExternalCheckIntent::Retry
                    )
                {
                    let marked = self
                        .persistence
                        .as_mut()
                        .is_some_and(PersistenceCoordinator::mark_external_conflict);
                    if !marked {
                        return ExternalApplyResult::Stale;
                    }
                    let Some(baseline_preview) = self.saved_baseline_preview() else {
                        return ExternalApplyResult::Stale;
                    };
                    self.external = Some(ExternalConflictView {
                        state: ExternalDiskState::Changed,
                        baseline: baseline_preview,
                        local: local_preview,
                        disk: Some(preview(baseline.source())),
                    });
                    self.external_pause = true;
                    return ExternalApplyResult::Presented(ExternalDiskState::Changed);
                }
                let selection = clamped_selection(self.core.current_selection(), baseline.source());
                match self
                    .core
                    .reload_external(baseline.source().to_owned(), selection, now_millis)
                {
                    Ok(ReloadOutcome::Changed { .. }) => {
                        if !self.adopt_disk_baseline(baseline.clone()) {
                            return ExternalApplyResult::Stale;
                        }
                        self.baseline = baseline.clone();
                        self.reload_mark = Some(self.core.undo_depth());
                        self.reload_pause = false;
                        self.external = None;
                        self.external_pause = false;
                        ExternalApplyResult::Reloaded
                    }
                    Ok(ReloadOutcome::Unchanged) => {
                        if !self.adopt_disk_baseline(baseline.clone()) {
                            return ExternalApplyResult::Stale;
                        }
                        self.baseline = baseline.clone();
                        self.external = None;
                        self.external_pause = false;
                        ExternalApplyResult::Unchanged
                    }
                    Err(DocError::CompositionActive) => ExternalApplyResult::Deferred,
                    Err(_) => ExternalApplyResult::Failed,
                }
            }
            ExternalObservation::Missing => {
                self.present_external(ExternalDiskState::Missing, dirty, local_preview)
            }
            ExternalObservation::Unreadable => {
                self.present_external(ExternalDiskState::Unreadable, dirty, local_preview)
            }
            ExternalObservation::Unstable => {
                self.present_external(ExternalDiskState::Unstable, dirty, local_preview)
            }
            ExternalObservation::Unsupported => {
                self.present_external(ExternalDiskState::Unsupported, dirty, local_preview)
            }
            ExternalObservation::Cancelled => ExternalApplyResult::Stale,
            ExternalObservation::Failed => ExternalApplyResult::Failed,
        }
    }

    /// 返回仅用于呈现的冲突视图。
    pub(crate) fn external_view(&self) -> Option<&ExternalConflictView> {
        self.external.as_ref()
    }

    /// 收起冲突呈现；冲突状态与自动保存暂停保持到磁盘重新等于基线或显式保存。
    pub(crate) fn dismiss_external(&mut self) {
        self.external = None;
    }

    /// 自动保存计时器消费此查询；外部冲突或重载撤销暂停时不自动写盘。
    pub(crate) fn autosave_paused(&self) -> bool {
        self.reload_pause || self.external_pause
    }

    /// 记录一次输入结算或内容变更。组合活动时只清除到期时刻；
    /// 组合结束后的下一次结算会以该时刻重新布防 1 秒。
    pub(crate) fn note_document_activity(&mut self, now_millis: u64) {
        let composing = self.core.composition().is_some();
        self.autosave.note_activity(now_millis, composing);
    }

    /// 返回下一次自动保存唤醒时刻；None 表示没有待办。
    pub(crate) fn autosave_deadline(&self) -> Option<u64> {
        self.autosave.deadline()
    }

    /// 关闭/替换时取消待办计时。
    pub(crate) fn cancel_autosave(&mut self) {
        self.autosave.clear();
    }

    /// 结算到期的自动保存；不执行保存，只返回动作。
    pub(crate) fn autosave_poll(&mut self, now_millis: u64) -> AutosaveAction {
        // 关联/迁移期间不得绕过引用迁移把临时 locator 写入目标文件。
        if self.association.is_some() || self.pending_migration.is_some() {
            return AutosaveAction::NoTarget;
        }
        if !self.editable || self.persistence.is_none() {
            return AutosaveAction::NoTarget;
        }
        if self.core.composition().is_some() {
            return AutosaveAction::DeferredComposition;
        }
        if self.autosave_paused() {
            return AutosaveAction::Paused;
        }
        let status = self
            .persistence
            .as_ref()
            .map(PersistenceCoordinator::status);
        match status {
            // 冲突必须走用户出口；不确定发布必须等用户处理。
            Some(SaveStatus::Conflict) => return AutosaveAction::Paused,
            Some(
                SaveStatus::Saving
                | SaveStatus::WaitingForComposition
                | SaveStatus::PublishedUncertain,
            ) => return AutosaveAction::Busy,
            Some(SaveStatus::Idle | SaveStatus::Failed) | None => {}
        }
        let Some(deadline) = self.autosave.deadline() else {
            return AutosaveAction::Idle;
        };
        if now_millis < deadline {
            return AutosaveAction::NotDue {
                remaining_millis: deadline - now_millis,
            };
        }
        let snapshot = match self.current_snapshot() {
            Ok(snapshot) => snapshot,
            // 快照失败不自动写盘；保留到期时刻，等待下一次结算/唤醒。
            Err(_) => return AutosaveAction::Busy,
        };
        let dirty = self
            .persistence
            .as_ref()
            .is_some_and(|coordinator| coordinator.is_dirty(&snapshot));
        if !dirty {
            self.autosave.clear();
            return AutosaveAction::NotDirty;
        }
        self.autosave.clear(); // Fire 一次性消费到期时刻，失败后不自动重试
        AutosaveAction::Fire
    }

    /// 仅在可编辑文档上挂接恢复会话；只读文档永不挂接。
    pub(crate) fn attach_recovery(&mut self, config: RecoveryConfig) {
        if self.editable {
            self.recovery = Some(RecoverySession::new(config));
        }
    }

    /// 仅当会话存在且空闲时布防；不读时钟。
    pub(crate) fn recovery_arm(&mut self, now: u64) {
        if let Some(recovery) = self.recovery.as_mut() {
            recovery.arm(now);
        }
    }

    /// 返回下一次恢复准备到期时刻；None 表示没有待办。
    pub(crate) fn recovery_deadline(&self) -> Option<u64> {
        self.recovery.as_ref().and_then(RecoverySession::deadline)
    }

    /// 结算恢复计时；无会话返回 None，宿主不布防。
    pub(crate) fn recovery_poll(&mut self, now: u64) -> Option<RecoveryAction> {
        self.recovery.as_mut().map(|recovery| recovery.poll(now))
    }

    /// 开始一次恢复准备；快照失败时就地记录失败反馈。
    pub(crate) fn recovery_start(&mut self, now: u64) -> Option<RecoveryStart> {
        let snapshot = match self.current_snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => {
                let feedback = self
                    .recovery
                    .as_mut()
                    .map(|recovery| recovery.settle_failure(now));
                return Some(RecoveryStart::Settled { feedback });
            }
        };
        // 只有可编辑且目标为绝对路径时才记录宿主路径与路径代次。
        let has_persistence = self.persistence.is_some() && self.path.is_absolute();
        let path = if has_persistence {
            RecoveryPath::from_os_path(&self.path)
        } else {
            RecoveryPath::Unlinked
        };
        let path_generation = if has_persistence {
            self.persistence
                .as_ref()
                .map_or(0, |coordinator| coordinator.path_generation().value())
        } else {
            0
        };
        let captured_composition = self.core.composition().is_some();
        let logical = self.saved_logical_snapshot().cloned();
        let storage = self.baseline.clone();
        let document_id = self.core.document_id().clone();
        let text_budget = self.limits.text();
        let recovery = self.recovery.as_mut()?;
        match recovery.begin(
            RecoveryBegin {
                document_id,
                snapshot,
                captured_composition,
                path,
                path_generation,
                logical,
                storage,
                text_budget,
            },
            now,
        )? {
            RecoveryBeginOutcome::Prepare(job) => Some(RecoveryStart::Prepare(job)),
            RecoveryBeginOutcome::Settled(settlement) => {
                let fallback = settlement.feedback;
                let feedback = recovery.settle(settlement, now).or(fallback);
                // 结算路径可能已在会话内清空尝试；这里保证下次仍会布防。
                recovery.arm(now);
                Some(RecoveryStart::Settled { feedback })
            }
        }
    }

    /// 复验后台准备结果；版本变化时静默丢弃，发布前再交给后台。
    pub(crate) fn recovery_prepared(
        &mut self,
        prepared: PreparedRecovery,
        now: u64,
    ) -> Option<RecoveryStart> {
        if self.recovery.is_none() {
            // 会话已被关闭/替换取走：迟到准备若已分配谱系，交给宿主后台退役。
            return prepared.into_close_job().map(RecoveryStart::Retire);
        }
        let snapshot = match self.current_snapshot() {
            Ok(snapshot) => snapshot,
            Err(_) => {
                let feedback = self
                    .recovery
                    .as_mut()
                    .map(|recovery| recovery.settle_failure(now));
                return Some(RecoveryStart::Settled { feedback });
            }
        };
        let path_generation = self
            .persistence
            .as_ref()
            .map_or(0, |coordinator| coordinator.path_generation().value());
        let recovery = self.recovery.as_mut()?;
        match recovery.accept_prepared(prepared, &snapshot, path_generation, now) {
            RecoveryAcceptOutcome::Stale => None,
            RecoveryAcceptOutcome::Retire(job) => Some(RecoveryStart::Retire(job)),
            RecoveryAcceptOutcome::Publish(job) => Some(RecoveryStart::Publish(job)),
            RecoveryAcceptOutcome::Settled(settlement) => {
                let fallback = settlement.feedback;
                let feedback = recovery.settle(settlement, now).or(fallback);
                recovery.arm(now);
                Some(RecoveryStart::Settled { feedback })
            }
        }
    }

    /// 结算后台发布；外来文档或标签不匹配时零副作用。
    pub(crate) fn recovery_settled(
        &mut self,
        settlement: RecoverySettlement,
        now: u64,
    ) -> Option<&'static str> {
        if &settlement.tag.document_id != self.core.document_id() {
            return None;
        }
        self.recovery.as_mut()?.settle(settlement, now)
    }

    /// 取消在途恢复任务并清除计时；阶段由结算或关闭收尾。
    pub(crate) fn cancel_recovery(&mut self) {
        if let Some(recovery) = self.recovery.as_mut() {
            recovery.cancel();
        }
    }

    /// 取走关闭退役任务；先取消在途任务，无会话或无谱系时返回 None。
    pub(crate) fn take_recovery_close(&mut self) -> Option<RecoveryCloseJob> {
        self.recovery.take()?.into_close_job()
    }

    /// 返回最近一次保存采纳的逻辑视图；恢复准备据此判断保存点等价。
    pub(crate) fn saved_logical_snapshot(&self) -> Option<&TextSnapshot> {
        self.persistence
            .as_ref()
            .and_then(PersistenceCoordinator::saved_logical)
    }

    /// 仅用于无持久化目标测试：移除保存协调器，不改路径与基线。
    #[cfg(test)]
    pub(crate) fn detach_persistence_for_test(&mut self) {
        self.persistence = None;
    }

    /// 在 Undo/Redo 成功（版本发生变化）后由宿主调用。
    /// 返回是否“新进入”重载撤销暂停。
    pub(crate) fn note_history_move(&mut self) -> bool {
        let Some(mark) = self.reload_mark else {
            return false;
        };
        if self.core.undo_depth() < mark {
            let newly = !self.reload_pause;
            self.reload_pause = true;
            return newly;
        }
        if self.reload_pause {
            let clean = self.current_snapshot().ok().is_some_and(|snapshot| {
                self.persistence
                    .as_ref()
                    .is_none_or(|coordinator| !coordinator.is_dirty(&snapshot))
            });
            if clean {
                self.reload_pause = false;
            }
        }
        false
    }

    /// 采纳完整磁盘观察为基线；成功后由调用方替换展示基线并清理暂停标记。
    fn adopt_disk_baseline(&mut self, baseline: SourceBaseline) -> bool {
        let Some(generation) = self
            .persistence
            .as_ref()
            .map(PersistenceCoordinator::path_generation)
        else {
            return false;
        };
        self.persistence.as_mut().is_some_and(|coordinator| {
            coordinator
                .adopt_external_baseline(generation, baseline)
                .is_ok()
        })
    }

    /// 保存基线的截断预览；只在确认协调器存在后调用。
    fn saved_baseline_preview(&self) -> Option<Arc<str>> {
        self.persistence
            .as_ref()
            .map(|coordinator| preview(coordinator.saved_baseline().source()))
    }

    /// 呈现删除/不可读/不支持/不稳定；脏文档同时进入冲突状态。
    fn present_external(
        &mut self,
        state: ExternalDiskState,
        dirty: bool,
        local: Arc<str>,
    ) -> ExternalApplyResult {
        if dirty
            && !self
                .persistence
                .as_mut()
                .is_some_and(PersistenceCoordinator::mark_external_conflict)
        {
            return ExternalApplyResult::Stale;
        }
        let Some(baseline) = self.saved_baseline_preview() else {
            return ExternalApplyResult::Stale;
        };
        self.external = Some(ExternalConflictView {
            state,
            baseline,
            local,
            disk: None,
        });
        self.external_pause = true;
        ExternalApplyResult::Presented(state)
    }

    /// 请求一次显式保存：组合期返回等待句柄，否则冻结并生成后台工作。
    pub(crate) fn request_save_command(&mut self) -> SaveCommand {
        if !self.editable {
            return SaveCommand::Rejected(SaveRejection::ReadOnly);
        }
        // 关联/迁移期间不得绕过引用迁移直接写目标。
        if self.association.is_some() || self.pending_migration.is_some() {
            return SaveCommand::Rejected(SaveRejection::Busy);
        }
        let Some(coordinator) = self.persistence.as_ref() else {
            return SaveCommand::Rejected(SaveRejection::NoTarget);
        };
        // Ruling W4-16：在途保存、冲突与不确定发布都必须在入队前拒绝。
        // 冲突/不确定时 `start()` 会先于取走 queued 返回 Busy，若此处放行就会
        // 留下无人消费的 queued 项，并连带拒绝关闭与替换。
        // `WaitingForComposition` 仍交给协调器 `next_operation` 的既有 Busy 路径。
        match coordinator.status() {
            SaveStatus::Saving => return SaveCommand::Rejected(SaveRejection::Busy),
            SaveStatus::Conflict => return SaveCommand::Rejected(SaveRejection::Conflict),
            SaveStatus::PublishedUncertain => {
                return SaveCommand::Rejected(SaveRejection::Uncertain);
            }
            SaveStatus::Idle | SaveStatus::WaitingForComposition | SaveStatus::Failed => {}
        }
        let staging = match save_staging_candidate(coordinator.target_path()) {
            Ok(staging) => staging,
            Err(rejection) => return SaveCommand::Rejected(rejection),
        };
        match self.request_persistence_save() {
            Ok(SaveRequest::Queued(operation)) => {
                match self.start_queued_save(operation, staging) {
                    Ok(job) => SaveCommand::Run(job),
                    Err(rejection) => SaveCommand::Rejected(rejection),
                }
            }
            Ok(SaveRequest::Waiting(handle)) => SaveCommand::Wait(handle),
            Err(error) => SaveCommand::Rejected(save_request_rejection(&error)),
        }
    }

    /// 从已入队操作生成后台工作；失败时回滚 queued 项并映射拒绝类别。
    ///
    /// Ruling W4-16：只有 `start()` 拒绝后仍留在队列里的项需要撤回，
    /// 否则后续 `next_operation` 会永久返回 Busy，关闭/替换也随 `queued` 被拒。
    fn start_queued_save(
        &mut self,
        operation: OperationId,
        staging: PathBuf,
    ) -> Result<SaveJob, SaveRejection> {
        self.start_save_job(staging).map_err(|error| {
            let rejection = save_request_rejection(&error);
            if let Some(coordinator) = self.persistence.as_mut() {
                coordinator.cancel(operation);
            }
            rejection
        })
    }

    /// 撤销组合期保存等待；只撤发布意图，不改变组合、输入或历史。
    pub(crate) fn cancel_save_wait(&mut self, handle: WaitHandle) -> Result<(), SaveRejection> {
        if !self.editable {
            return Err(SaveRejection::ReadOnly);
        }
        let coordinator = self.persistence.as_mut().ok_or(SaveRejection::NoTarget)?;
        coordinator
            .cancel_save_wait(&mut self.core, handle)
            .map_err(|_| SaveRejection::Internal)
    }

    /// 平台自然结束后按当时快照继续保存；返回可执行工作或拒绝。
    ///
    /// 真实平台结束回调属于 W5；在此之前只有 crate 测试的协议 fake 调用。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn finish_save_wait_command(
        &mut self,
        handle: WaitHandle,
        outcome: PublicationOutcome,
        now_millis: u64,
    ) -> SaveCommand {
        if !self.editable {
            return SaveCommand::Rejected(SaveRejection::ReadOnly);
        }
        let Some(coordinator) = self.persistence.as_ref() else {
            return SaveCommand::Rejected(SaveRejection::NoTarget);
        };
        // Ruling W4-16：先算 staging，再让协调器入队；否则入队后路径失败会
        // 留下无人消费的 queued 项。
        let staging = match save_staging_candidate(coordinator.target_path()) {
            Ok(staging) => staging,
            Err(rejection) => return SaveCommand::Rejected(rejection),
        };
        let operation = {
            let Some(coordinator) = self.persistence.as_mut() else {
                return SaveCommand::Rejected(SaveRejection::NoTarget);
            };
            match coordinator.finish_save_wait(&mut self.core, handle, outcome, now_millis) {
                Ok(operation) => operation,
                Err(error) => return SaveCommand::Rejected(save_request_rejection(&error)),
            }
        };
        match self.start_queued_save(operation, staging) {
            Ok(job) => SaveCommand::Run(job),
            Err(rejection) => SaveCommand::Rejected(rejection),
        }
    }

    /// 结算一次真实写盘完成项，返回窗口层反馈类别。
    pub(crate) fn settle_save_command(&mut self, completion: WriteCompletion) -> SaveResult {
        let settlement = match self.settle_save(completion) {
            Ok(settlement) => settlement,
            Err(_) => return SaveResult::Failed,
        };
        if !settlement.adopted {
            // 迟到或外来的完成项不得覆盖当前状态或反馈。
            return SaveResult::Stale;
        }
        // Minor 1 记录：取消完成项在 `apply_outcome` 中一律返回未采纳，因此会先归
        // `Stale`；`SaveResult::Cancelled` 目前经 adopted 路径不可达。用户主动取消
        // 走宿主 `cancel_save_wait`。W4 3.3 若复用该分类，需拆分取消与迟到的判定。
        settlement_result(&settlement.outcome)
    }

    /// 仅供 crate 内测试读取保存协调器状态；不扩大生产 API。
    #[cfg(test)]
    pub(super) fn persistence_status(&self) -> Option<super::persistence::SaveStatus> {
        self.persistence
            .as_ref()
            .map(PersistenceCoordinator::status)
    }

    /// 仅供 crate 内测试读取首次保存是否已完成关联。
    #[cfg(test)]
    pub(super) fn persistence_is_creating(&self) -> Option<bool> {
        self.persistence
            .as_ref()
            .map(PersistenceCoordinator::is_creating)
    }

    /// 仅供 crate 内测试读取当前是否脏；不扩大生产 API。
    #[cfg(test)]
    pub(super) fn persistence_is_dirty(&self, current: &super::CurrentSnapshot) -> Option<bool> {
        self.persistence
            .as_ref()
            .map(|coordinator| coordinator.is_dirty(current))
    }

    /// 仅供 crate 内测试读取关闭决策；用于验证拒绝保存后没有 queued 残留。
    #[cfg(test)]
    pub(super) fn persistence_close_status(
        &self,
        current: &super::CurrentSnapshot,
    ) -> Option<CloseStatus> {
        self.persistence
            .as_ref()
            .map(|coordinator| coordinator.close_status(current))
    }

    /// 判断已采用解析是否仍属于当前源码和解析配置。
    pub fn is_parse_current(&self) -> bool {
        self.pending_parse.is_none()
            && self.core.is_document_alive()
            && self.parsed_identity.working_version() == self.core.version()
            && self.parsed_identity.parse_config_generation() == self.core.parse_config_generation()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn parsed_identity(&self) -> &InputIdentity {
        &self.parsed_identity
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn text_budget(&self) -> aline_buffer::TextBudget {
        self.limits.text()
    }
    /// 以当前布局/解析身份导航；文档生命周期仍由 ReadonlySession 唯一持有。
    pub fn navigate(
        &mut self,
        identity: &InputIdentity,
        selection: &SrcSelection,
    ) -> Result<(), DocError> {
        if !self.editable {
            return self.core.navigate_readonly(identity, selection);
        }
        let current = self.current_parsed_snapshot(identity)?;
        current
            .text()
            .validate_selection(selection)
            .map_err(DocError::Buffer)?;

        if self.core.mode() == EditorMode::Visual
            && let Some(open) = self.core.visual_block_range(&current)?
        {
            let extent = open.byte_range();
            let selected = selection.range().byte_range();
            if extent.start > selected.start || selected.end > extent.end {
                return self
                    .core
                    .retarget_visual(identity, &self.parsed, None, selection);
            }
        }
        self.core.select(identity, selection)
    }

    /// 将一次普通可视化点击升级为块编辑；目录/脚注导航继续只移动选区。
    pub fn open_visual_at(
        &mut self,
        identity: &InputIdentity,
        selection: &SrcSelection,
    ) -> Result<bool, DocError> {
        if !self.editable || self.core.mode() != EditorMode::Visual {
            return Ok(false);
        }
        let current = self.current_parsed_snapshot(identity)?;
        current
            .text()
            .validate_selection(selection)
            .map_err(DocError::Buffer)?;
        let selected = selection.range().byte_range();
        let mut matching = self.parsed.blocks().iter().filter(|block| {
            let extent = block.range().byte_range();
            extent.start <= selected.start && selected.end <= extent.end
        });
        let first = matching.next();
        let Some(block) = first.filter(|_| matching.next().is_none()) else {
            return Ok(false);
        };
        let protected = self.parsed.protected_ranges().iter().any(|range| {
            let protected = range.range().byte_range();
            selected.start <= protected.end && protected.start <= selected.end
        });
        if protected {
            return Ok(false);
        }

        if let Some(open) = self.core.visual_block_range(&current)?
            && open == *block.range()
        {
            self.core.select(identity, selection)?;
            return Ok(true);
        }
        if self.core.has_open_block() {
            self.core
                .retarget_visual(identity, &self.parsed, Some(block.range()), selection)?;
        } else {
            self.core
                .open_visual_block(identity, &self.parsed, block.range(), selection)?;
        }
        Ok(true)
    }
    /// 在两种视图间切换，不改变文档生命周期。
    pub fn switch_mode(&mut self, mode: EditorMode) -> Result<ModeSwitchOutcome, DocError> {
        let previous = self.core.mode();
        let outcome = self.core.switch_mode(mode, 0)?;
        if self.core.mode() != previous {
            self.cancel_pending_parse();
        }
        Ok(outcome)
    }
    /// 返回上一份已采用的解析。命中或编辑前还须复验当前文档身份。
    pub fn parsed(&self) -> &Parsed {
        &self.parsed
    }

    fn current_parsed_snapshot(
        &mut self,
        identity: &InputIdentity,
    ) -> Result<super::CurrentSnapshot, DocError> {
        if !self.is_parse_current() || self.parsed.snapshot().id() != identity.snapshot_id() {
            return Err(DocError::StaleResult);
        }
        self.core.accept_document_result(identity)?;
        self.current_snapshot()
    }
}

/// 保存目标同目录的 staging 候选：路径唯一且不预创建，最多尝试 8 个。
///
/// 文件名形如 `.<file>.aline-save-<pid>-<nanos>-<attempt>.tmp`；source 在真正
/// 写入时以独占创建复核，本函数只做词法生成与存在性探测。
fn save_staging_candidate(target: &Path) -> Result<PathBuf, SaveRejection> {
    let parent = target.parent().ok_or(SaveRejection::NoTarget)?;
    let name = target.file_name().ok_or(SaveRejection::NoTarget)?;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    for attempt in 0..8u32 {
        let candidate = parent.join(format!(
            ".{}.aline-save-{}-{nanos}-{attempt}.tmp",
            name.to_string_lossy(),
            std::process::id()
        ));
        match candidate.try_exists() {
            Ok(false) => return Ok(candidate),
            Ok(true) => {}
            Err(_) => return Err(SaveRejection::Internal),
        }
    }
    Err(SaveRejection::Internal)
}

/// 保存入口错误的宿主可辨映射；未知组合一律视为内部失败。
fn save_request_rejection(error: &SaveRequestError) -> SaveRejection {
    match error {
        SaveRequestError::Input(DocError::ReadOnly) => SaveRejection::ReadOnly,
        SaveRequestError::Queue(SaveError::Busy) => SaveRejection::Busy,
        SaveRequestError::Queue(SaveError::Closing) => SaveRejection::Closing,
        SaveRequestError::Queue(SaveError::InvalidPath) => SaveRejection::NoTarget,
        _ => SaveRejection::Internal,
    }
}

/// 已采纳完成项的稳定分类；调用方先排除未采纳的迟到结果。
fn settlement_result(outcome: &WriteOutcome) -> SaveResult {
    match outcome {
        WriteOutcome::NotPublished(WriteStop::Cancelled) => SaveResult::Cancelled,
        WriteOutcome::NotPublished(WriteStop::Conflict) => SaveResult::Conflict,
        WriteOutcome::NotPublished(WriteStop::File(failure)) => match failure.kind() {
            AtomicFailureKind::Cancelled { .. } => SaveResult::Cancelled,
            AtomicFailureKind::Conflict { .. } => SaveResult::Conflict,
            _ => SaveResult::Failed,
        },
        WriteOutcome::NotPublished(WriteStop::Preparation(_)) => SaveResult::Failed,
        WriteOutcome::NotPublished(WriteStop::Create) => SaveResult::Failed,
        WriteOutcome::Published {
            synchronized: true, ..
        } => SaveResult::Saved,
        WriteOutcome::Published {
            synchronized: false,
            ..
        }
        | WriteOutcome::PublishedTargetUnverified(_)
        | WriteOutcome::PublishedReceiptMismatch(_)
        | WriteOutcome::PublishedBaselineFailed { .. }
        | WriteOutcome::PublishedNewBaselineFailed { .. } => SaveResult::SavedUncertain,
    }
}

/// 背景读取工作的最小不可变输入；不持有窗口或当前 Ready 文档。
pub struct ReadonlyOpenJob {
    request: OpenRequestId,
    path: PathBuf,
    cancel: CancelToken,
    limits: ReadonlyOpenLimits,
}
impl ReadonlyOpenJob {
    /// 返回是否已取消；宿主在实际执行前也可提前释放已取消工作。
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

/// 后台读取三段的真实起止边界，供验证探针采集分段成本。
///
/// 单调耗时与墙钟边界分开捕获，不参与业务判定、不调度线程、不影响失败语义；
/// 打开失败时各段为零。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct OpenTimings {
    pub(crate) read_decode: StageTiming,
    pub(crate) snapshot: StageTiming,
    pub(crate) parse_mapping: StageTiming,
    pub(crate) dispatch_wait: StageTiming,
    pub(crate) result_wait: StageTiming,
}

/// 后台完成候选；只能由只读 job 产生，必须交回所属会话复验后发布。
pub struct ReadonlyOpenCompletion {
    request: OpenRequestId,
    result: Result<ReadonlyDocument, OpenError>,
    pub(crate) timings: OpenTimings,
}

/// 候选的最终结算；只有 Applied 表示替换成功。
pub enum OpenDisposition {
    /// 当前有效候选已原子替换 Ready 文档。
    Applied,
    /// 当前请求合作取消；旧文档保留且不显示错误。
    Cancelled,
    /// 旧请求、另一会话或关闭后的结果已释放，不能复活目标。
    Stale,
    /// 当前请求失败；旧文档/视图/选区保留。
    Failed(OpenError),
    /// 完成候选有效，但当前可编辑文档有未结算内容，因此拒绝替换。
    RejectedUnresolvedDocument,
}

/// 关闭文档的结果；拒绝时当前文档和在途请求均保留。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CloseDisposition {
    /// 文档已关闭。
    Closed,
    /// 当前可编辑文档仍有未结算内容或发布等待。
    Refused,
}

/// 无窗口单文档宿主；当前 Ready 与一个 Selecting/Loading 请求分别持有。
pub struct ReadonlySession {
    identity: Arc<()>,
    next_generation: u64,
    limits: ReadonlyOpenLimits,
    pending: Option<PendingOpen>,
    document: Option<ReadonlyDocument>,
    alive: bool,
    /// 下一次打开可编辑文档时注入的恢复配置；打开期间不可更改。
    recovery: Option<RecoveryConfig>,
    /// 关闭/替换后待后台退役的恢复谱系；由宿主取走执行。
    retired_recoveries: Vec<RecoveryCloseJob>,
    /// 下一次打开文档时注入的资源目录设置；打开期间不可更改。
    image_settings: ImageSettings,
    /// 下一次打开文档时注入的受检临时根；无根时插图失败关闭。
    temp_root: Option<TempRoot>,
    /// 关闭/替换后待后台执行的临时资源清理任务。
    retired_cleanups: Vec<ResourceCleanupJob>,
}
impl ReadonlySession {
    /// 建立无文档状态，不创建编辑块、任务或文件句柄。
    pub fn new(limits: ReadonlyOpenLimits) -> Self {
        Self {
            identity: Arc::new(()),
            next_generation: 0,
            limits,
            pending: None,
            document: None,
            alive: true,
            recovery: None,
            retired_recoveries: Vec::new(),
            image_settings: ImageSettings::default(),
            temp_root: None,
            retired_cleanups: Vec::new(),
        }
    }

    /// 测试专用：注入下一次打开使用的图片目录设置；文档打开期间拒绝更改。
    #[cfg(test)]
    pub(crate) fn configure_image_settings(
        &mut self,
        settings: ImageSettings,
    ) -> Result<(), ImageSettingsError> {
        if self.document.is_some() {
            return Err(ImageSettingsError::DocumentOpen);
        }
        settings
            .validate()
            .map_err(|_| ImageSettingsError::Invalid)?;
        self.image_settings = settings;
        Ok(())
    }

    /// 注入下一次打开使用的受检临时根；文档打开期间忽略。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn configure_temp_root(&mut self, root: Option<TempRoot>) {
        if self.document.is_none() {
            self.temp_root = root;
        }
    }

    /// 取走关闭/替换后待执行的临时资源清理任务；宿主在文档释放后运行。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn take_retired_cleanups(&mut self) -> Vec<ResourceCleanupJob> {
        std::mem::take(&mut self.retired_cleanups)
    }
    /// 返回当前完整 Ready 文档；加载候选不会覆盖它。
    pub fn document(&self) -> Option<&ReadonlyDocument> {
        self.document.as_ref()
    }
    /// 借用当前 Ready 的受检导航/模式能力。
    pub fn document_mut(&mut self) -> Option<&mut ReadonlyDocument> {
        self.document.as_mut()
    }
    /// 是否存在尚未结算的文件选择或加载候选。
    pub const fn is_loading(&self) -> bool {
        self.pending.is_some()
    }
    /// 返回当前候选的实际阶段，供加载反馈使用。
    pub fn pending_phase(&self) -> Option<ReadonlyOpenPhase> {
        self.pending.as_ref().map(|pending| pending.phase)
    }

    /// 发起新选择意图；身份预检成功后取消旧请求资格，保留 Ready 文档。
    pub fn request_open(&mut self) -> Result<OpenRequestId, OpenError> {
        if !self.alive {
            return Err(OpenError::ClosedWindow);
        }
        let generation = self
            .next_generation
            .checked_add(1)
            .ok_or(OpenError::RequestExhausted)?;
        self.cancel_pending();
        let id = OpenRequestId {
            session: self.identity.clone(),
            generation,
        };
        self.pending = Some(PendingOpen {
            id: id.clone(),
            cancel: CancelToken::new(),
            phase: ReadonlyOpenPhase::Selecting,
        });
        self.next_generation = generation;
        Ok(id)
    }

    /// 复验单路径选择并生成只读工作；None 为用户取消，其他失败不会替换 Ready。
    pub fn accept_selection(
        &mut self,
        request: &OpenRequestId,
        selection: Option<Vec<PathBuf>>,
    ) -> Result<Option<ReadonlyOpenJob>, OpenError> {
        if !self.alive {
            return Err(OpenError::ClosedWindow);
        }
        let pending = self.pending.as_ref().ok_or(OpenError::StaleRequest)?;
        if &pending.id != request || pending.phase != ReadonlyOpenPhase::Selecting {
            return Err(OpenError::StaleRequest);
        }
        let Some(mut paths) = selection else {
            self.cancel_pending();
            return Ok(None);
        };
        if paths.len() != 1 {
            self.cancel_pending();
            return Err(OpenError::SelectionCount);
        }
        let path = paths.pop().ok_or(OpenError::SelectionCount)?;
        if !path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
        {
            self.cancel_pending();
            return Err(OpenError::UnsupportedExtension);
        }
        let pending = self.pending.as_mut().ok_or(OpenError::StaleRequest)?;
        pending.phase = ReadonlyOpenPhase::Loading;
        Ok(Some(ReadonlyOpenJob {
            request: pending.id.clone(),
            path,
            cancel: pending.cancel.clone(),
            limits: self.limits,
        }))
    }

    /// 取消仍属当前的意图；旧取消不能影响更新的请求。
    pub fn cancel_open(&mut self, request: &OpenRequestId) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| &pending.id == request)
        {
            self.cancel_pending();
            return true;
        }
        false
    }

    /// 仅发布仍存活会话的最新 Loading 候选；失败/取消/旧结果都保留原文档。
    pub fn accept_open(&mut self, completion: ReadonlyOpenCompletion) -> OpenDisposition {
        match self.accept_open_with(completion, |_| Ok::<(), std::convert::Infallible>(())) {
            Ok((disposition, _)) => disposition,
            Err(never) => match never {},
        }
    }

    /// 配置下一次打开使用的恢复会话；文档打开期间拒绝更改。
    /// 生产由窗口启动装配调用；打开期间拒绝，测试覆盖拒绝分支。
    pub(crate) fn configure_recovery(
        &mut self,
        config: Option<RecoveryConfig>,
    ) -> Result<(), RecoveryConfigError> {
        if self.document.is_some() {
            return Err(RecoveryConfigError::DocumentOpen);
        }
        self.recovery = config;
        Ok(())
    }

    /// 取走关闭/替换后待退役的恢复谱系；宿主在后台执行。
    pub(crate) fn take_retired_recoveries(&mut self) -> Vec<RecoveryCloseJob> {
        std::mem::take(&mut self.retired_recoveries)
    }

    /// 复验请求后准备宿主所需的派生状态。准备失败时结算请求并保留当前文档。
    pub(crate) fn accept_open_with<T, E>(
        &mut self,
        completion: ReadonlyOpenCompletion,
        prepare: impl FnOnce(&mut ReadonlyDocument) -> Result<T, E>,
    ) -> Result<(OpenDisposition, Option<T>), E> {
        if !self.alive
            || !self.pending.as_ref().is_some_and(|pending| {
                pending.id == completion.request && pending.phase == ReadonlyOpenPhase::Loading
            })
        {
            return Ok((OpenDisposition::Stale, None));
        }
        self.pending = None;
        match completion.result {
            Ok(mut document) => {
                if self
                    .document
                    .as_mut()
                    .is_some_and(|current| current.close_status() != CloseStatus::Ready)
                {
                    return Ok((OpenDisposition::RejectedUnresolvedDocument, None));
                }
                // 替换前先捕获旧文档的恢复引用上下文；取消与退役必须等新文档
                // 准备成功后再执行，否则 prepare 失败会误伤仍在使用的旧会话。
                let recovery_context = self
                    .document
                    .as_ref()
                    .and_then(ReadonlyDocument::recovery_cleanup_context);
                if let Some(config) = self.recovery.clone() {
                    document.attach_recovery(config);
                }
                document.apply_resource_config(self.image_settings.clone(), self.temp_root.clone());
                let prepared = prepare(&mut document)?;
                // 只有新文档准备成功才取消旧恢复并退役旧文档资源；`prepare` 失败时旧文档
                // 仍是当前文档，必须继续持有目录、条目与恢复会话。
                if let Some(old) = self.document.as_mut() {
                    old.cancel_autosave();
                    if let Some(job) = old.take_recovery_close() {
                        self.retired_recoveries.push(job);
                    }
                    if let Some(job) = old.retire_resources(recovery_context) {
                        self.retired_cleanups.push(job);
                    }
                }
                self.document = Some(document);
                Ok((OpenDisposition::Applied, Some(prepared)))
            }
            Err(OpenError::Cancelled) => Ok((OpenDisposition::Cancelled, None)),
            Err(error) => Ok((OpenDisposition::Failed(error), None)),
        }
    }

    /// 关闭当前文档并取消在途候选；回到可再次打开的无文档状态。
    /// 关闭获准后、释放文档前执行一次由宿主提供的本地结算回调。
    pub(crate) fn close_document_with<T>(
        &mut self,
        before_close: impl FnOnce(&ReadonlyDocument) -> T,
    ) -> Result<Option<T>, CloseDisposition> {
        let close_value = if let Some(document) = self.document.as_mut() {
            if document.close_status() != CloseStatus::Ready {
                return Err(CloseDisposition::Refused);
            }
            // 释放准备（W4-53）：先取消计时与关联、捕获恢复上下文并退役谱系、
            // 构造资源清理任务，最后才让证据回调看到已收口的文档。
            document.cancel_autosave();
            document.cancel_association();
            let recovery_context = document.recovery_cleanup_context();
            if let Some(job) = document.take_recovery_close() {
                self.retired_recoveries.push(job);
            }
            if let Some(job) = document.retire_resources(recovery_context) {
                self.retired_cleanups.push(job);
            }
            let value = before_close(document);
            Some(value)
        } else {
            None
        };
        self.cancel_pending();
        self.document = None;
        Ok(close_value)
    }

    /// 关闭当前文档并取消在途候选；未结算编辑时保留文档。
    pub fn close_document(&mut self) -> CloseDisposition {
        match self.close_document_with(|_| ()) {
            Ok(_) => CloseDisposition::Closed,
            Err(disposition) => disposition,
        }
    }
    /// 系统关闭窗口时失效所有候选、释放文档，之后不再接受新意图。
    pub fn close_window(&mut self) -> CloseDisposition {
        if self.close_document() == CloseDisposition::Refused {
            return CloseDisposition::Refused;
        }
        self.alive = false;
        CloseDisposition::Closed
    }
    fn cancel_pending(&mut self) {
        if let Some(pending) = self.pending.take() {
            pending.cancel.cancel();
        }
    }
}
impl Drop for ReadonlySession {
    fn drop(&mut self) {
        self.cancel_pending();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::error::Error;
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::document::WorkingVersion;
    #[cfg(target_os = "macos")]
    use crate::document::resources::CleanupOutcome;
    use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
    #[cfg(target_os = "macos")]
    use std::time::{SystemTime, UNIX_EPOCH};

    struct Files(&'static [u8]);

    impl ReadonlyFileAccess for Files {
        type Reader = Cursor<&'static [u8]>;

        fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(
                Cursor::new(self.0),
                self.0.len() as u64,
                true,
            ))
        }
    }

    fn editable_session(bytes: &'static [u8]) -> Result<ReadonlySession, Box<dyn Error>> {
        let max = 4096;
        let limits = ReadonlyOpenLimits::new(
            StorageBudget::new(max),
            DocLimits::new(
                TextBudget::new(max),
                LineIndexBudget::new(128),
                HistoryBudget::new(16, max),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
        .with_editing();
        let mut session = ReadonlySession::new(limits);
        let request = session.request_open()?;
        let job = session
            .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
            .ok_or("missing open job")?;
        assert!(matches!(
            session.accept_open(job.read_with(&Files(bytes))),
            OpenDisposition::Applied
        ));
        Ok(session)
    }

    /// 受检历史操作前后用于零副作用断言的完整可观测状态。
    type CheckedState = (
        String,
        WorkingVersion,
        SelectionSpec,
        usize,
        usize,
        EditorMode,
        bool,
    );

    fn checked_state(document: &ReadonlyDocument) -> Result<CheckedState, DocError> {
        Ok((
            document.core().buffer_text()?,
            document.core().version(),
            document.core().current_selection(),
            document.core().undo_depth(),
            document.core().redo_depth(),
            document.core().mode(),
            document.core().has_open_block(),
        ))
    }

    #[test]
    fn visual_undo_of_source_protected_edit_is_refused_without_side_effects()
    -> Result<(), Box<dyn Error>> {
        let mut session = editable_session(b"---\nt: x\n---\n\nbody")?;
        let document = session.document_mut().ok_or("document")?;
        assert_eq!(
            document.switch_mode(EditorMode::Source)?,
            ModeSwitchOutcome::Ready
        );
        document.core.open_block(0..18, SelectionSpec::new(7, 7))?;
        document.core.type_text(7, "Y", 1)?;
        document.core.commit_open_block(2)?;
        assert_eq!(document.core.buffer_text()?, "---\nt: Yx\n---\n\nbody");
        assert_eq!(
            document.switch_mode(EditorMode::Visual)?,
            ModeSwitchOutcome::Ready
        );

        let before = checked_state(document)?;
        let result = document.checked_undo();
        assert!(matches!(
            result,
            Err(DocError::Edit(aline_edit::EditRefusal::ProtectedRange {
                kind: aline_markdown::ProtectionKind::FrontMatter,
                ..
            }))
        ));
        assert_eq!(checked_state(document)?, before);

        // 切回 Source 后同一撤销可以完成。
        document.switch_mode(EditorMode::Source)?;
        document.checked_undo()?;
        assert_eq!(document.core().buffer_text()?, "---\nt: x\n---\n\nbody");
        Ok(())
    }

    #[test]
    fn visual_redo_of_source_protected_edit_is_refused_without_side_effects()
    -> Result<(), Box<dyn Error>> {
        let mut session = editable_session(b"---\nt: x\n---\n\nbody")?;
        let document = session.document_mut().ok_or("document")?;
        document.switch_mode(EditorMode::Source)?;
        document.core.open_block(0..18, SelectionSpec::new(7, 7))?;
        document.core.type_text(7, "Y", 1)?;
        document.core.commit_open_block(2)?;
        document.checked_undo()?;
        assert_eq!(document.core().buffer_text()?, "---\nt: x\n---\n\nbody");
        document.switch_mode(EditorMode::Visual)?;

        let before = checked_state(document)?;
        let result = document.checked_redo();
        assert!(matches!(
            result,
            Err(DocError::Edit(aline_edit::EditRefusal::ProtectedRange {
                kind: aline_markdown::ProtectionKind::FrontMatter,
                ..
            }))
        ));
        assert_eq!(checked_state(document)?, before);

        document.switch_mode(EditorMode::Source)?;
        document.checked_redo()?;
        assert_eq!(document.core().buffer_text()?, "---\nt: Yx\n---\n\nbody");
        Ok(())
    }

    #[test]
    fn visual_undo_and_redo_of_body_edit_still_succeed() -> Result<(), Box<dyn Error>> {
        let mut session = editable_session(b"hello body")?;
        let document = session.document_mut().ok_or("document")?;
        document.core.open_block(0..10, SelectionSpec::new(0, 0))?;
        document.core.type_text(0, "X", 1)?;
        document.core.commit_open_block(2)?;
        assert_eq!(document.core().buffer_text()?, "Xhello body");

        document.checked_undo()?;
        assert_eq!(document.core().buffer_text()?, "hello body");
        document.checked_redo()?;
        assert_eq!(document.core().buffer_text()?, "Xhello body");
        Ok(())
    }

    #[test]
    fn request_id_exhaustion_keeps_the_existing_pending_request() -> Result<(), OpenError> {
        let limits = ReadonlyOpenLimits::new(
            StorageBudget::new(1024),
            DocLimits::new(
                TextBudget::new(1024),
                LineIndexBudget::new(32),
                HistoryBudget::new(32, 1024),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        );
        let mut session = ReadonlySession::new(limits);
        let current = session.request_open()?;
        session.next_generation = u64::MAX;
        assert!(matches!(
            session.request_open(),
            Err(OpenError::RequestExhausted)
        ));
        assert_eq!(session.pending_phase(), Some(ReadonlyOpenPhase::Selecting));
        assert!(session.cancel_open(&current));
        Ok(())
    }

    #[test]
    fn dirty_editable_document_refuses_close_and_replacement_until_restored()
    -> Result<(), Box<dyn Error>> {
        let mut session = editable_session(b"base")?;
        let document = session.document_mut().ok_or("document")?;
        let identity = document.input_identity()?;
        let source = document.parsed().snapshot().src_offset(0)?;
        let selection = SrcSelection::new(source.clone(), source)?;
        assert!(document.open_visual_at(&identity, &selection)?);
        document.core.type_text(0, "x", 1)?;
        let original_id = document.core.document_id().clone();

        let request = session.request_open()?;
        let replacement = session
            .accept_selection(&request, Some(vec![PathBuf::from("replacement.md")]))?
            .ok_or("replacement job")?;
        assert!(matches!(
            session.accept_open(replacement.read_with(&Files(b"replacement"))),
            OpenDisposition::RejectedUnresolvedDocument
        ));
        let document = session.document_mut().ok_or("preserved document")?;
        assert_eq!(document.core.document_id(), &original_id);
        assert_eq!(document.path(), Path::new("fixture.md"));
        let snapshot = document.current_snapshot()?;
        assert_eq!(snapshot.full_text(document.limits.text())?, "xbase");
        assert_eq!(session.close_document(), CloseDisposition::Refused);
        assert!(session.document().is_some());
        let side_effect = Cell::new(false);
        assert_eq!(
            session.close_document_with(|_| side_effect.set(true)),
            Err(CloseDisposition::Refused)
        );
        assert!(!side_effect.get());

        session
            .document_mut()
            .ok_or("preserved document")?
            .core
            .undo()?;
        let closed = Cell::new(false);
        assert_eq!(
            session.close_document_with(|_| closed.set(true)),
            Ok(Some(()))
        );
        assert!(closed.get());
        assert!(session.document().is_none());
        Ok(())
    }

    #[test]
    fn cancelled_publication_wait_does_not_block_clean_close() -> Result<(), Box<dyn Error>> {
        let mut session = editable_session(b"base")?;
        let document = session.document_mut().ok_or("document")?;
        let identity = document.input_identity()?;
        let source = document.parsed().snapshot().src_offset(0)?;
        let selection = SrcSelection::new(source.clone(), source)?;
        assert!(document.open_visual_at(&identity, &selection)?);
        document.core.composition_start("候选", 1)?;
        let wait = document
            .core
            .begin_publication_wait(super::super::PublicationKind::Save)?;
        document.core.request_cancel_wait(wait)?;
        document.core.composition_cancel(2)?;
        assert!(
            document
                .core
                .wait()
                .is_some_and(|wait| wait.status() == WaitStatus::Cancelled)
        );

        assert_eq!(session.close_document(), CloseDisposition::Closed);
        assert!(session.document().is_none());
        Ok(())
    }

    /// 系统临时目录下的测试 HOME；Drop 时递归清理，绝不使用真实 HOME。
    #[cfg(target_os = "macos")]
    struct TempHome(PathBuf);

    #[cfg(target_os = "macos")]
    impl TempHome {
        fn create(prefix: &str) -> Result<Self, Box<dyn Error>> {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            for attempt in 0..100u32 {
                let path = std::env::temp_dir().join(format!(
                    "aline-{prefix}-{}-{nanos:x}-{attempt}",
                    std::process::id()
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => {
                        std::fs::create_dir_all(path.join("Library/Application Support"))?;
                        return Ok(Self(path));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Err("无法创建测试 HOME".into())
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    #[cfg(target_os = "macos")]
    impl Drop for TempHome {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// 打开一个挂接受检临时根的可编辑会话；使用相对路径，不创建保存协调器。
    #[cfg(target_os = "macos")]
    fn editable_resource_session(home: &Path) -> Result<ReadonlySession, Box<dyn Error>> {
        let max = 4096;
        let limits = ReadonlyOpenLimits::new(
            StorageBudget::new(max),
            DocLimits::new(
                TextBudget::new(max),
                LineIndexBudget::new(128),
                HistoryBudget::new(16, max),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
        .with_editing();
        let mut session = ReadonlySession::new(limits);
        let root = TempRoot::open_or_create(home).map_err(|error| format!("{error:?}"))?;
        session.configure_temp_root(Some(root));
        let request = session.request_open()?;
        let job = session
            .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
            .ok_or("missing open job")?;
        assert!(matches!(
            session.accept_open(job.read_with(&Files(b"base"))),
            OpenDisposition::Applied
        ));
        Ok(session)
    }

    /// 替换准备失败时旧文档仍持有资源目录，不得产生可删除其临时图片的清理任务；
    /// 准备成功后才按序退役旧资源。
    #[cfg(target_os = "macos")]
    #[test]
    fn replacement_retires_old_resources_only_after_prepare_succeeds() -> Result<(), Box<dyn Error>>
    {
        let home = TempHome::create("replace-resources")?;
        let mut session = editable_resource_session(home.path())?;
        {
            let document = session.document_mut().ok_or("document")?;
            let len = document
                .current_snapshot()?
                .full_text(TextBudget::new(4096))?
                .len();
            document
                .edit_core()?
                .open_block(0..len, SelectionSpec::new(0, 0))?;
        }
        session
            .document_mut()
            .ok_or("document")?
            .stage_temp_image("pic.png", b"png", 4096)
            .map_err(|error| format!("{error:?}"))?;
        session
            .document_mut()
            .ok_or("document")?
            .edit_core()?
            .leave_block(1)?;
        let old_id = session
            .document()
            .ok_or("document")?
            .core
            .document_id()
            .clone();
        let temp_file = {
            let document = session.document().ok_or("document")?;
            let catalog = document.resources.as_ref().ok_or("resources")?;
            let entry = catalog.entries().first().ok_or("resource entry")?;
            catalog.directory().join(&entry.file_name)
        };
        assert!(temp_file.is_file());

        let request = session.request_open()?;
        let replacement = session
            .accept_selection(&request, Some(vec![PathBuf::from("replacement.md")]))?
            .ok_or("replacement job")?;
        let failed = session
            .accept_open_with(replacement.read_with(&Files(b"replacement")), |_| {
                Err::<(), _>("prepare failed")
            });
        assert_eq!(failed.err(), Some("prepare failed"));
        assert!(
            session.take_retired_cleanups().is_empty(),
            "prepare 失败时不得生成会删除旧文档资源的清理任务"
        );
        {
            let document = session.document().ok_or("preserved document")?;
            assert_eq!(document.core.document_id(), &old_id);
            assert_eq!(document.path(), Path::new("fixture.md"));
            let catalog = document.resources.as_ref().ok_or("resources preserved")?;
            assert_eq!(catalog.entries().len(), 1);
        }
        assert!(temp_file.is_file(), "旧文档资源文件必须仍在");

        let request = session.request_open()?;
        let replacement = session
            .accept_selection(&request, Some(vec![PathBuf::from("replacement.md")]))?
            .ok_or("replacement job")?;
        let applied = session
            .accept_open_with(replacement.read_with(&Files(b"replacement")), |_| {
                Ok::<(), &'static str>(())
            });
        assert!(matches!(applied, Ok((OpenDisposition::Applied, Some(())))));
        let cleanup = session
            .take_retired_cleanups()
            .into_iter()
            .next()
            .ok_or("cleanup job")?;
        assert!(temp_file.exists(), "清理任务执行前旧文件仍在");
        assert_eq!(cleanup.run(), CleanupOutcome::Cleaned);
        assert!(!temp_file.exists(), "替换成功后旧资源应被清理");
        assert!(
            session
                .document()
                .ok_or("new document")?
                .resources
                .is_none()
        );
        Ok(())
    }
}
