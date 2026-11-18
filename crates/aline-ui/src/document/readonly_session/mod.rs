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
use super::{
    DocCore, DocError, DocLimits, EditorMode, InputIdentity, ModeSwitchOutcome, PublicationOutcome,
    WaitHandle, WaitStatus,
};
use aline_buffer::{ReloadOutcome, SrcSelection};

mod read;
mod refresh;
mod timing;
pub use read::{ReadonlyFileAccess, ReadonlyFileInput};
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
        let coordinator = self
            .persistence
            .as_mut()
            .ok_or(SaveRequestError::Queue(SaveError::InvalidPath))?;
        coordinator.request_save(&mut self.core, self.limits.text())
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
        Ok(settlement)
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

    /// 3.4 的自动保存计时器消费此查询；3.3 生产代码不调用。
    #[cfg_attr(not(test), allow(dead_code))] // Ruling W4-21：3.4 消费
    pub(crate) fn autosave_paused(&self) -> bool {
        self.reload_pause || self.external_pause
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

    fn can_discard_for_replacement(&mut self) -> bool {
        if !self.editable {
            return true;
        }
        if self.core.composition().is_some()
            || self
                .core
                .wait()
                .is_some_and(|wait| wait.status() == WaitStatus::Pending)
        {
            return false;
        }
        let Ok(snapshot) = self.current_snapshot() else {
            return false;
        };
        match &self.persistence {
            // 可编辑文档的关闭/替换一律以协调器的最新状态为准：在途保存、
            // 未结算失败/冲突/不确定发布或相对保存基线有修改都须先由用户处理。
            Some(coordinator) => coordinator.close_status(&snapshot) == CloseStatus::Ready,
            // 没有绝对目标路径的可编辑文档只可能来自注入读取器或测试，
            // 此时没有可写目标关联，退回原有的打开基线全文比较。
            None => snapshot
                .full_text(self.limits.text())
                .is_ok_and(|text| text == self.baseline.source()),
        }
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
        WriteOutcome::Published {
            synchronized: true, ..
        } => SaveResult::Saved,
        WriteOutcome::Published {
            synchronized: false,
            ..
        }
        | WriteOutcome::PublishedTargetUnverified(_)
        | WriteOutcome::PublishedReceiptMismatch(_)
        | WriteOutcome::PublishedBaselineFailed { .. } => SaveResult::SavedUncertain,
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
        }
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
                    .is_some_and(|current| !current.can_discard_for_replacement())
                {
                    return Ok((OpenDisposition::RejectedUnresolvedDocument, None));
                }
                let prepared = prepare(&mut document)?;
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
            if !document.can_discard_for_replacement() {
                return Err(CloseDisposition::Refused);
            }
            Some(before_close(document))
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
    use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};

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
}
