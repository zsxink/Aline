//! 无窗口文档核心：唯一 Buffer、History 与 OpenBlock 的纯状态事务。
//!
//! `DocCore` 只持有 `StringBuffer`（内含唯一历史）、一个打开块和发布等待，
//! 不持有窗口、GPUI 实体、任务或文件句柄。普通输入先进入打开块的唯一草稿；
//! 提交、撤销、重做与重载才写入已提交 Buffer。所有拒绝在修改前完成状态与
//! 预算预检，失败调用相对调用前状态不产生可观察变化。
//!
//! 模块所有者：`aline-ui` 文档层。输入：源码文本、源码选区、虚拟毫秒与宿主
//! 发布结果。输出：当前快照、提交结果与明确错误。副作用：仅内存状态变化。
//! 只读会话直接建立空打开槽；文档目标存活与编辑块存活分别复验。

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use aline_buffer::{
    ApplyOutcome, BufferError, DocumentRevision, LineIndexBudget, OperationSource, PatchBudget,
    ReloadOutcome, SelectionSpec, StringBuffer, TYPING_MERGE_WINDOW_MILLIS, TextBudget,
    TextSnapshot,
};

use crate::document::composition::{CompositionState, CompositionView};
use crate::document::identity::{
    DerivedRequestId, DocumentId, DraftGeneration, IdentityError, InputIdentity,
    ParseConfigGeneration, WorkingVersion,
};
use crate::document::open_block::{DraftSlot, OpenBlock, TypingGroup};
use crate::document::publication::{
    PublicationKind, PublicationOutcome, PublicationResult, WaitHandle, WaitState, WaitStatus,
    WaitView,
};
use crate::document::snapshot::{CurrentSnapshot, SnapshotCache};
use crate::document::{EditorMode, ResourceReference};

/// 文档核心的注入预算集合；不声明发布性能阈值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DocLimits {
    text: TextBudget,
    index: LineIndexBudget,
    history: aline_buffer::HistoryBudget,
}

impl DocLimits {
    /// 由宿主注入文本、行索引与历史预算。
    pub const fn new(
        text: TextBudget,
        index: LineIndexBudget,
        history: aline_buffer::HistoryBudget,
    ) -> Self {
        Self {
            text,
            index,
            history,
        }
    }

    /// 返回文本字节预算。
    pub const fn text(self) -> TextBudget {
        self.text
    }

    /// 返回行索引预算。
    pub const fn index(self) -> LineIndexBudget {
        self.index
    }

    /// 返回历史预算。
    pub const fn history(self) -> aline_buffer::HistoryBudget {
        self.history
    }
}

/// 打开块提交的观察结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    /// 没有可提交的草稿；文本、版本与历史不变。
    Unchanged,
    /// 草稿已作为单个单区间补丁写入 Buffer。
    Committed {
        /// 提交后的文档代次。
        revision: DocumentRevision,
    },
}

/// 文档核心的拒绝原因；拒绝不改变调用前状态。
#[derive(Debug)]
pub enum DocError {
    /// 当前会话只允许阅读，拒绝编辑、资源登记和发布意图。
    ReadOnly,
    /// 该入口仅接受只读会话，编辑会话必须沿既有输入协议执行。
    RequiresReadonly,
    /// 没有打开块。
    NoOpenBlock,
    /// 已有打开块，不能重复打开。
    BlockAlreadyOpen,
    /// 打开块基线与当前已提交代次不符；草稿与选区保留。
    DraftBaselineConflict,
    /// 组合候选活动时不允许该操作。
    CompositionActive,
    /// 存在待定普通输入组；Redo 不得越过当前输入。
    PendingInput,
    /// 没有活动的组合候选。
    NoComposition,
    /// 同一文档已有未结束的发布等待。
    PublicationInFlight,
    /// 等待句柄不存在或已被取消/结束。
    UnknownWait,
    /// 目标已关闭，不再接受派生结果。
    ClosedTarget,
    /// 结果属于另一文档。
    WrongDocument,
    /// 结果的工作版本或快照已经过期。
    StaleResult,
    /// 等待句柄编号耗尽。
    WaitIdExhausted,
    /// 资源引用数量超过宿主注入预算。
    ResourceBudgetExceeded,
    /// 底层 Buffer 拒绝。
    Buffer(BufferError),
    /// 编辑计划或发布前复验拒绝；草稿、组合与历史保持调用前状态。
    Edit(aline_edit::EditRefusal),
    /// 工作版本推进被身份层拒绝。
    Identity(IdentityError),
}

impl fmt::Display for DocError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for DocError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Buffer(error) => Some(error),
            Self::Identity(error) => Some(error),
            Self::Edit(error) => Some(error),
            _ => None,
        }
    }
}

const INITIAL_WAIT_ID: u64 = 1;

/// 无窗口文档状态机；一个实例只拥有一个 Buffer、一个 History 与一个打开块。
pub struct DocCore {
    document: DocumentId,
    version: WorkingVersion,
    buffer: StringBuffer,
    open: Option<OpenBlock>,
    wait: Option<WaitState>,
    next_wait_id: u64,
    limits: DocLimits,
    snapshot_cache: Option<SnapshotCache>,
    resources: Arc<[ResourceReference]>,
    parse_config: ParseConfigGeneration,
    derived_request: DerivedRequestId,
    mode: EditorMode,
    access: readonly::DocumentAccess,
    alive: bool,
}

impl DocCore {
    /// 以完整源码、初始选区和注入预算创建文档，并在当前选区上打开唯一块。
    ///
    /// 初始打开范围按已确认契约取选区排序后的范围。构造失败不留下实例。
    pub fn new(
        text: impl Into<String>,
        selection: SelectionSpec,
        limits: DocLimits,
    ) -> Result<Self, DocError> {
        let buffer = StringBuffer::new(
            text.into(),
            selection,
            limits.text,
            limits.index,
            limits.history,
        )
        .map_err(DocError::Buffer)?;
        let range = selection.range();
        let fragment = slice_buffer(&buffer, range.clone(), limits.text)?;
        let block = OpenBlock {
            baseline_revision: buffer.revision(),
            range,
            baseline_fragment: fragment,
            selection,
            draft: DraftSlot::Empty,
            undone_typing: None,
        };
        Ok(Self {
            document: DocumentId::new(),
            version: WorkingVersion::new(buffer.revision(), DraftGeneration::initial()),
            buffer,
            open: Some(block),
            wait: None,
            next_wait_id: INITIAL_WAIT_ID,
            limits,
            snapshot_cache: None,
            resources: Arc::from([]),
            parse_config: ParseConfigGeneration::initial(),
            derived_request: DerivedRequestId::initial(),
            mode: EditorMode::Visual,
            access: readonly::DocumentAccess::Editing,
            alive: true,
        })
    }

    /// 返回会话内文档身份。
    pub fn document_id(&self) -> &DocumentId {
        &self.document
    }

    /// 返回当前工作版本。
    pub const fn version(&self) -> WorkingVersion {
        self.version
    }

    /// 返回当前已提交 Buffer 文本。
    pub fn buffer_text(&self) -> Result<String, DocError> {
        let range = self
            .buffer
            .snapshot()
            .src_range(0..self.buffer.len_bytes())
            .map_err(DocError::Buffer)?;
        Ok(self
            .buffer
            .snapshot()
            .slice(&range, self.limits.text)
            .map_err(DocError::Buffer)?
            .into_owned())
    }

    /// 返回已提交 Buffer 的源码选区（不是草稿逻辑选区）。
    pub fn buffer_selection(&self) -> SelectionSpec {
        selection_of(self.buffer.selection())
    }

    /// 判断是否存在打开块。
    pub const fn has_open_block(&self) -> bool {
        self.open.is_some()
    }

    /// 返回打开块基线代次。
    pub fn open_block_baseline(&self) -> Option<DocumentRevision> {
        self.open.as_ref().map(|block| block.baseline_revision)
    }

    /// 返回打开块的源码范围。
    pub fn open_block_range(&self) -> Option<Range<usize>> {
        self.open.as_ref().map(|block| block.range.clone())
    }

    /// 返回打开块当前逻辑选区（含草稿坐标）。
    pub fn open_block_selection(&self) -> Option<SelectionSpec> {
        self.open.as_ref().map(|block| block.selection)
    }

    /// 返回打开块当前草稿文本；组合活动时返回候选文本。
    pub fn open_block_draft(&self) -> Option<&str> {
        self.open.as_ref().and_then(OpenBlock::current_draft)
    }

    /// 返回打开块基线片段。
    pub fn open_block_baseline_fragment(&self) -> Option<&str> {
        self.open
            .as_ref()
            .map(|block| block.baseline_fragment.as_str())
    }

    /// 返回当前逻辑选区：有打开块时取块选区，否则取 Buffer 选区。
    pub fn current_selection(&self) -> SelectionSpec {
        match &self.open {
            Some(block) => block.selection,
            None => self.buffer_selection(),
        }
    }

    /// 返回当前组合候选视图。
    pub fn composition(&self) -> Option<CompositionView<'_>> {
        let block = self.open.as_ref()?;
        let state = block.composition()?;
        Some(CompositionView {
            range: state.range.clone(),
            candidate: &state.candidate,
        })
    }

    /// 返回当前发布等待视图。
    pub fn wait(&self) -> Option<WaitView> {
        self.wait
            .as_ref()
            .map(|wait| WaitView::new(wait.handle, wait.kind, wait.status))
    }

    /// 返回可撤销历史节点数；草稿撤销不计数。
    pub fn undo_depth(&self) -> usize {
        self.buffer.undo_depth()
    }

    /// 返回可重做历史节点数；草稿重做不计数。
    pub fn redo_depth(&self) -> usize {
        self.buffer.redo_depth()
    }

    /// 返回历史节点总数，包含 redo 分支。
    pub fn history_node_count(&self) -> usize {
        self.buffer.history_node_count()
    }

    /// 在指定源码范围上打开块；块已打开时拒绝。
    ///
    /// 打开只记录基线与选区，不改变版本或历史。
    pub fn open_block(
        &mut self,
        range: Range<usize>,
        selection: SelectionSpec,
    ) -> Result<(), DocError> {
        self.ensure_writable()?;
        if self.open.is_some() {
            return Err(DocError::BlockAlreadyOpen);
        }
        let base = self.buffer.snapshot();
        base.src_range(range.clone()).map_err(DocError::Buffer)?;
        base.src_offset(selection.anchor())
            .map_err(DocError::Buffer)?;
        base.src_offset(selection.head())
            .map_err(DocError::Buffer)?;
        let fragment = slice_buffer(&self.buffer, range.clone(), self.limits.text)?;
        self.open = Some(OpenBlock {
            baseline_revision: self.buffer.revision(),
            range,
            baseline_fragment: fragment,
            selection,
            draft: DraftSlot::Empty,
            undone_typing: None,
        });
        self.snapshot_cache = None;
        Ok(())
    }

    /// 移动打开块的逻辑选区；只改变选区，不改变版本、历史或草稿。
    ///
    /// 组合活动时拒绝。该入口是测试与宿主调整插入点的纯状态驱动入口。
    pub fn set_selection(&mut self, selection: SelectionSpec) -> Result<(), DocError> {
        self.ensure_ready()?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
        self.validate_selection(block, selection)?;
        let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
        block.selection = selection;
        if matches!(block.draft, DraftSlot::Empty) {
            block.range = selection.range();
        }
        self.snapshot_cache = None;
        Ok(())
    }

    /// 在逻辑偏移处输入普通文本；连续相邻输入并入同一草稿组。
    ///
    /// 到达 900ms 边界时先提交旧组并封口，再以本次输入开启新组；无打开块、
    /// 基线冲突、组合活动或非法字符边界都拒绝且不改变状态。
    pub fn type_text(&mut self, at: usize, text: &str, now_millis: u64) -> Result<(), DocError> {
        self.ensure_ready()?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        if text.is_empty() {
            return Ok(());
        }
        let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
        self.validate_position(block, at)?;
        let continuous = match block.typing() {
            Some(group) => {
                let end = group
                    .range
                    .start
                    .checked_add(group.draft.len())
                    .ok_or_else(overflow)?;
                let cursor = block.selection;
                cursor.anchor() == at
                    && cursor.head() == at
                    && at == end
                    && now_millis >= group.last_input_at
                    && now_millis
                        .checked_sub(group.last_input_at)
                        .is_some_and(|delta| delta <= TYPING_MERGE_WINDOW_MILLIS)
            }
            None => false,
        };
        if continuous {
            self.append_to_group(at, text, now_millis)
        } else {
            self.start_group(at, text, now_millis)
        }
    }

    /// 提交当前普通输入草稿组并封口，但保持块打开；无草稿时为无变化。
    pub fn commit_open_block(&mut self, now_millis: u64) -> Result<CommitOutcome, DocError> {
        self.ensure_ready()?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        self.commit_typing_group(now_millis)
    }

    /// 提交剩余普通输入组、封口并关闭块；无剩余组时直接关闭。
    pub fn leave_block(&mut self, now_millis: u64) -> Result<(), DocError> {
        self.close_block(now_millis)
    }

    /// 非组合态的 Escape 等价于离开块；组合活动时不伪造组合结束。
    pub fn escape(&mut self, now_millis: u64) -> Result<(), DocError> {
        self.close_block(now_millis)
    }

    /// 取得当前一致的只读全文快照；不提交、不封口、不清 redo。
    ///
    /// 同一工作版本重复读取返回同一 `SnapshotId`。基线冲突或超预算时拒绝，
    /// 且不产生快照。
    pub fn current_snapshot(
        &mut self,
        text_budget: TextBudget,
    ) -> Result<CurrentSnapshot, DocError> {
        self.ensure_document_alive()?;
        if self.open.is_some() {
            self.ensure_ready()?;
        }
        let has_draft = self
            .open
            .as_ref()
            .is_some_and(|block| block.current_draft().is_some());
        if has_draft
            && let Some(cache) = self.snapshot_cache.as_ref()
            && cache.version == self.version
        {
            check_len(cache.snapshot.len_bytes(), text_budget)?;
            return Ok(cache.snapshot.clone());
        }
        let snapshot = self.build_current_snapshot(text_budget)?;
        if has_draft {
            self.snapshot_cache = Some(SnapshotCache {
                version: self.version,
                snapshot: snapshot.clone(),
            });
        }
        Ok(snapshot)
    }

    /// 以当前逻辑选区开启组合；已有组合时拒绝。
    ///
    /// 若已有普通输入组，本实现先提交并封口该组再进入组合（T-8-05 的实现决定），
    /// 保证二者不误并；规范未裁决该状态。
    pub fn composition_start(&mut self, candidate: &str, now_millis: u64) -> Result<(), DocError> {
        self.ensure_ready()?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        self.start_composition(candidate, now_millis)
    }

    /// 更新组合候选；首次更新隐式以当前选区开启组合，之后是候选替换。
    ///
    /// 每次更新推进草稿代次，即使候选文本相同（T-8-06 的实现决定）。
    pub fn composition_update(&mut self, candidate: &str, now_millis: u64) -> Result<(), DocError> {
        self.ensure_ready()?;
        if self.is_composition_active() {
            self.replace_candidate(candidate)
        } else {
            self.start_composition(candidate, now_millis)
        }
    }

    /// 确认组合：候选作为单个 `ImeCommit` 补丁写入 Buffer，块保持打开。
    ///
    /// 无文本变化时走 `Unchanged`：不记步、不前进 revision，但组合结束并推进 dg。
    pub fn composition_commit(&mut self, now_millis: u64) -> Result<CommitOutcome, DocError> {
        self.ensure_ready()?;
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let (range, candidate, before_selection, after_selection) = {
            let state = self
                .open
                .as_ref()
                .and_then(OpenBlock::composition)
                .ok_or(DocError::NoComposition)?;
            (
                state.range.clone(),
                state.candidate.clone(),
                state.before_selection,
                state.selection,
            )
        };
        let outcome = self.publish_input(
            range,
            candidate,
            (before_selection, after_selection),
            OperationSource::ImeCommit,
            now_millis,
        )?;
        match outcome {
            ApplyOutcome::Changed(applied) => {
                let revision = applied.revision();
                self.buffer.seal_pending();
                self.version = self
                    .version
                    .after_commit(revision)
                    .map_err(DocError::Identity)?;
                self.finish_composition(Some(revision), after_selection);
                Ok(CommitOutcome::Committed { revision })
            }
            ApplyOutcome::Unchanged => {
                self.version = next_version;
                self.finish_composition(None, after_selection);
                Ok(CommitOutcome::Unchanged)
            }
        }
    }

    /// 取消组合：丢弃候选并恢复组合前选区，不产生历史步或 revision 步。
    pub fn composition_cancel(&mut self, _now_millis: u64) -> Result<(), DocError> {
        self.ensure_ready()?;
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let (range, before_selection) = {
            let state = self
                .open
                .as_ref()
                .and_then(OpenBlock::composition)
                .ok_or(DocError::NoComposition)?;
            (state.range.clone(), state.before_selection)
        };
        let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
        block.draft = DraftSlot::Empty;
        block.range = range;
        block.selection = before_selection;
        block.undone_typing = None;
        self.version = next_version;
        self.snapshot_cache = None;
        Ok(())
    }

    /// 在组合活动时建立显式发布等待；不取快照、不封口、不改输入或历史。
    pub fn begin_publication_wait(
        &mut self,
        kind: PublicationKind,
    ) -> Result<WaitHandle, DocError> {
        self.ensure_ready()?;
        if !self.is_composition_active() {
            return Err(DocError::NoComposition);
        }
        if self
            .wait
            .as_ref()
            .is_some_and(|wait| wait.status == WaitStatus::Pending)
        {
            return Err(DocError::PublicationInFlight);
        }
        let next = self
            .next_wait_id
            .checked_add(1)
            .ok_or(DocError::WaitIdExhausted)?;
        let handle = WaitHandle::new(self.next_wait_id);
        self.wait = Some(WaitState {
            handle,
            kind,
            status: WaitStatus::Pending,
        });
        self.next_wait_id = next;
        Ok(handle)
    }

    /// 请求取消发布等待；只改等待状态，结束组合、输入与历史零变化。
    pub fn request_cancel_wait(&mut self, handle: WaitHandle) -> Result<(), DocError> {
        self.ensure_writable()?;
        let wait = self.wait.as_mut().ok_or(DocError::UnknownWait)?;
        if wait.handle != handle || wait.status != WaitStatus::Pending {
            return Err(DocError::UnknownWait);
        }
        wait.status = WaitStatus::Cancelled;
        Ok(())
    }

    /// 请求取消发布等待的别名。
    pub fn cancel_wait(&mut self, handle: WaitHandle) -> Result<(), DocError> {
        self.request_cancel_wait(handle)
    }

    /// 平台自然确认/取消后在组合已结束时取当前快照并移除等待。
    ///
    /// 组合仍活动时返回 `CompositionActive` 且等待保持；取消过的等待返回
    /// `UnknownWait`。成功不新增历史步、不封口、不清 redo。
    pub fn finish_publication(
        &mut self,
        handle: WaitHandle,
        outcome: PublicationOutcome,
        _now_millis: u64,
    ) -> Result<PublicationResult, DocError> {
        self.ensure_writable()?;
        let kind = {
            let wait = self.wait.as_ref().ok_or(DocError::UnknownWait)?;
            if wait.handle != handle || wait.status != WaitStatus::Pending {
                return Err(DocError::UnknownWait);
            }
            if self.is_composition_active() {
                return Err(DocError::CompositionActive);
            }
            wait.kind
        };
        let snapshot = self.current_snapshot(self.limits.text)?;
        self.wait = None;
        Ok(PublicationResult::new(kind, outcome, snapshot))
    }

    /// 平台自然结束后交付结果的别名。
    pub fn deliver(
        &mut self,
        handle: WaitHandle,
        outcome: PublicationOutcome,
        now_millis: u64,
    ) -> Result<PublicationResult, DocError> {
        self.finish_publication(handle, outcome, now_millis)
    }

    /// 按 `InputIdentity` 校验编辑投影结果；成功不改变任何状态。
    ///
    /// 校验顺序：先确认目标仍打开，再取当前快照并逐项比较文档、工作版本与
    /// 快照身份。失败是零副作用；成功返回值仅为 `()`（T-8-14 的实现决定）。
    pub fn accept_derived_result(&mut self, identity: &InputIdentity) -> Result<(), DocError> {
        self.ensure_document_alive()?;
        if self.open.is_none() {
            return Err(DocError::ClosedTarget);
        }
        self.validate_result_identity(identity)
    }

    fn validate_result_identity(&mut self, identity: &InputIdentity) -> Result<(), DocError> {
        let snapshot = self.current_snapshot(self.limits.text)?;
        let current = InputIdentity::for_request(
            self.document.clone(),
            self.version,
            snapshot.text(),
            self.parse_config,
            self.derived_request,
        )
        .map_err(DocError::Identity)?;
        match identity.validate(&current) {
            Ok(()) => Ok(()),
            Err(IdentityError::WrongDocument) => Err(DocError::WrongDocument),
            Err(
                IdentityError::StaleWorkingVersion
                | IdentityError::StaleSnapshot
                | IdentityError::StaleParseConfig
                | IdentityError::StaleRequest,
            ) => Err(DocError::StaleResult),
            Err(error) => Err(DocError::Identity(error)),
        }
    }

    /// 撤销：优先撤销待定草稿组，否则撤销最近已提交历史节点。
    ///
    /// 组合活动时拒绝。撤销不产生空步；撤销后的草稿代次不复用。
    pub fn undo(&mut self) -> Result<(), DocError> {
        self.ensure_writable()?;
        if self.open.is_some() {
            self.ensure_ready()?;
        }
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let pending = self.open.as_ref().and_then(OpenBlock::typing).is_some();
        if pending {
            // 撤销时刻的块选区就是重做应恢复的 after-selection；先捕获再覆盖。
            let (group, range, before_selection) = {
                let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
                let group = block.typing().ok_or(DocError::NoOpenBlock)?;
                let mut captured = group.clone();
                captured.after_selection = block.selection;
                (captured, group.range.clone(), group.before_selection)
            };
            let fragment = slice_buffer(&self.buffer, range.clone(), self.limits.text)?;
            let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
            block.undone_typing = Some(group);
            block.draft = DraftSlot::Empty;
            block.range = range;
            block.baseline_fragment = fragment;
            block.selection = before_selection;
            self.version = next_version;
            self.snapshot_cache = None;
            return Ok(());
        }
        let revision = self
            .buffer
            .undo(self.limits.text)
            .map_err(DocError::Buffer)?;
        self.version = self
            .version
            .after_commit(revision)
            .map_err(DocError::Identity)?;
        self.sync_block_after_buffer_change();
        self.snapshot_cache = None;
        Ok(())
    }

    /// 重做：优先重做被撤销的草稿组，否则重做历史节点。
    ///
    /// 当前存在待定输入组时拒绝：Redo 不得越过尚未结算的输入去重做旧节点，
    /// 否则待定草稿会被 `sync_block_after_buffer_change` 清空。拒绝不改变任何状态。
    pub fn redo(&mut self) -> Result<(), DocError> {
        self.ensure_writable()?;
        if self.open.is_some() {
            self.ensure_ready()?;
        }
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        if self.open.as_ref().and_then(OpenBlock::typing).is_some() {
            return Err(DocError::PendingInput);
        }
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let undone = self
            .open
            .as_ref()
            .and_then(|block| block.undone_typing.clone());
        if let Some(group) = undone {
            let selection = group.after_selection;
            let range = group.range.clone();
            // 受检恢复：先在探针块上校验选区两端，通过后才改动真实状态。
            let probe = {
                let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
                let mut probe = block.clone();
                probe.draft = DraftSlot::Typing(group.clone());
                probe.range = range.clone();
                probe.baseline_fragment.clear();
                probe
            };
            self.validate_selection(&probe, selection)?;
            let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
            block.draft = DraftSlot::Typing(group);
            block.range = range;
            block.baseline_fragment.clear();
            block.selection = selection;
            block.undone_typing = None;
            self.version = next_version;
            self.snapshot_cache = None;
            return Ok(());
        }
        let revision = self
            .buffer
            .redo(self.limits.text)
            .map_err(DocError::Buffer)?;
        self.version = self
            .version
            .after_commit(revision)
            .map_err(DocError::Identity)?;
        self.sync_block_after_buffer_change();
        self.snapshot_cache = None;
        Ok(())
    }

    /// 模拟宿主重载：整段新文本作为独立历史边界，打开块基线不自动前移。
    ///
    /// 这样草稿存在时后续读写会得到 `DraftBaselineConflict` 并保留草稿与选区。
    /// 相同文本返回 `Unchanged` 且不推进版本、不截断 redo。组合活动时拒绝。
    pub fn reload(
        &mut self,
        new_text: impl Into<String>,
        after_selection: SelectionSpec,
        now_millis: u64,
    ) -> Result<ReloadOutcome, DocError> {
        self.ensure_writable()?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        let outcome = self
            .buffer
            .reload(
                new_text.into(),
                after_selection,
                now_millis,
                self.limits.text,
            )
            .map_err(DocError::Buffer)?;
        match &outcome {
            ReloadOutcome::Unchanged => Ok(outcome),
            ReloadOutcome::Changed { revision } => {
                self.version = self
                    .version
                    .after_commit(*revision)
                    .map_err(DocError::Identity)?;
                if let Some(block) = self.open.as_mut() {
                    block.undone_typing = None;
                }
                self.snapshot_cache = None;
                Ok(outcome)
            }
        }
    }

    fn ensure_ready(&self) -> Result<(), DocError> {
        self.ensure_writable()?;
        let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
        if block.baseline_revision != self.buffer.revision() {
            return Err(DocError::DraftBaselineConflict);
        }
        Ok(())
    }

    fn is_composition_active(&self) -> bool {
        self.open
            .as_ref()
            .is_some_and(|block| block.composition().is_some())
    }

    fn validate_position(&self, block: &OpenBlock, offset: usize) -> Result<(), DocError> {
        let base = self.buffer.snapshot();
        match &block.draft {
            DraftSlot::Empty => {
                base.src_offset(offset).map_err(DocError::Buffer)?;
            }
            DraftSlot::Typing(group) => {
                let removed = group.range.len();
                let inserted = group.draft.len();
                let total = self
                    .buffer
                    .len_bytes()
                    .checked_sub(removed)
                    .and_then(|value| value.checked_add(inserted))
                    .ok_or_else(overflow)?;
                if offset > total {
                    return Err(DocError::Buffer(BufferError::OutOfBounds {
                        offset,
                        len: total,
                    }));
                }
                let inserted_end = group
                    .range
                    .start
                    .checked_add(inserted)
                    .ok_or_else(overflow)?;
                if offset <= group.range.start {
                    base.src_offset(offset).map_err(DocError::Buffer)?;
                } else if offset <= inserted_end {
                    let local = offset - group.range.start;
                    if !group.draft.is_char_boundary(local) {
                        return Err(DocError::Buffer(BufferError::NotCharBoundary { offset }));
                    }
                } else {
                    let shifted = offset
                        .checked_sub(inserted)
                        .and_then(|value| value.checked_add(removed))
                        .ok_or_else(overflow)?;
                    base.src_offset(shifted).map_err(DocError::Buffer)?;
                }
            }
            DraftSlot::Composition(_) => {
                return Err(DocError::Buffer(BufferError::SnapshotMismatch));
            }
        }
        Ok(())
    }

    fn validate_selection(
        &self,
        block: &OpenBlock,
        selection: SelectionSpec,
    ) -> Result<(), DocError> {
        self.validate_position(block, selection.anchor())?;
        self.validate_position(block, selection.head())
    }

    fn append_to_group(&mut self, at: usize, text: &str, now_millis: u64) -> Result<(), DocError> {
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let (start, draft_len, old_draft) = {
            let group = self
                .open
                .as_ref()
                .and_then(OpenBlock::typing)
                .ok_or(DocError::NoOpenBlock)?;
            (group.range.start, group.draft.len(), group.draft.clone())
        };
        let new_draft_len = draft_len.checked_add(text.len()).ok_or_else(overflow)?;
        let total = self
            .buffer
            .len_bytes()
            .checked_add(new_draft_len)
            .ok_or_else(overflow)?;
        check_len(total, self.limits.text)?;
        let local = at.checked_sub(start).ok_or_else(overflow)?;
        if local > draft_len {
            return Err(DocError::Buffer(BufferError::OutOfBounds {
                offset: local,
                len: draft_len,
            }));
        }
        let end = at.checked_add(text.len()).ok_or_else(overflow)?;
        let selection = SelectionSpec::new(end, end);
        let mut draft = try_string(new_draft_len)?;
        draft.push_str(&old_draft);
        draft.insert_str(local, text);
        let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
        let group = block.typing_mut().ok_or(DocError::NoOpenBlock)?;
        group.draft = draft;
        group.last_input_at = now_millis;
        group.after_selection = selection;
        block.selection = selection;
        block.undone_typing = None;
        self.version = next_version;
        self.snapshot_cache = None;
        Ok(())
    }

    fn start_group(&mut self, at: usize, text: &str, now_millis: u64) -> Result<(), DocError> {
        let has_group = self.open.as_ref().and_then(OpenBlock::typing).is_some();
        let committed_len = {
            let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
            match block.typing() {
                Some(group) => self
                    .buffer
                    .len_bytes()
                    .checked_sub(group.range.len())
                    .and_then(|value| value.checked_add(group.draft.len()))
                    .ok_or_else(overflow)?,
                None => self.buffer.len_bytes(),
            }
        };
        let total = committed_len.checked_add(text.len()).ok_or_else(overflow)?;
        check_len(total, self.limits.text)?;
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        if has_group {
            next_version.next_draft().map_err(DocError::Identity)?;
        }
        let draft = copy_bounded(text, self.limits.text)?;
        let end = at.checked_add(text.len()).ok_or_else(overflow)?;
        let selection = SelectionSpec::new(end, end);
        if has_group {
            self.commit_typing_group(now_millis)?;
            self.version = self.version.next_draft().map_err(DocError::Identity)?;
            let revision = self.buffer.revision();
            let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
            block.baseline_revision = revision;
            block.range = at..at;
            block.baseline_fragment.clear();
            block.selection = selection;
            block.draft = DraftSlot::Typing(TypingGroup {
                range: at..at,
                draft,
                before_selection: SelectionSpec::new(at, at),
                after_selection: selection,
                last_input_at: now_millis,
            });
            block.undone_typing = None;
        } else {
            // 有效的非空输入一旦建立新草稿，即分叉当前编辑历史。
            // 前面的校验和草稿分配均已成功，之后的状态发布不再失败。
            let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
            self.buffer.discard_redo_branch();
            self.version = next_version;
            block.range = at..at;
            block.baseline_fragment.clear();
            block.selection = selection;
            block.draft = DraftSlot::Typing(TypingGroup {
                range: at..at,
                draft,
                before_selection: SelectionSpec::new(at, at),
                after_selection: selection,
                last_input_at: now_millis,
            });
            block.undone_typing = None;
        }
        self.snapshot_cache = None;
        Ok(())
    }

    fn commit_typing_group(&mut self, now_millis: u64) -> Result<CommitOutcome, DocError> {
        let (range, draft, before_selection, after_selection) = {
            let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
            let Some(group) = block.typing() else {
                return Ok(CommitOutcome::Unchanged);
            };
            (
                group.range.clone(),
                group.draft.clone(),
                group.before_selection,
                block.selection,
            )
        };
        if draft.is_empty() {
            return Ok(CommitOutcome::Unchanged);
        }
        self.version.next_draft().map_err(DocError::Identity)?;
        let outcome = self.publish_input(
            range,
            draft,
            (before_selection, after_selection),
            OperationSource::Typing,
            now_millis,
        )?;
        let ApplyOutcome::Changed(applied) = outcome else {
            return Err(DocError::Buffer(BufferError::InvalidBacking));
        };
        let revision = applied.revision();
        self.buffer.seal_pending();
        self.version = self
            .version
            .after_commit(revision)
            .map_err(DocError::Identity)?;
        let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
        let head = after_selection.head();
        block.baseline_revision = revision;
        block.range = head..head;
        block.baseline_fragment.clear();
        block.selection = after_selection;
        block.draft = DraftSlot::Empty;
        block.undone_typing = None;
        self.snapshot_cache = None;
        Ok(CommitOutcome::Committed { revision })
    }

    fn close_block(&mut self, now_millis: u64) -> Result<(), DocError> {
        self.ensure_ready()?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        // 关闭是结果采纳资格的边界；预检必须在任何待定组提交之前完成。
        let next_request = self.derived_request.next().map_err(DocError::Identity)?;
        self.commit_typing_group(now_millis)?;
        self.derived_request = next_request;
        self.open = None;
        self.wait = None;
        self.snapshot_cache = None;
        Ok(())
    }

    fn start_composition(&mut self, candidate: &str, now_millis: u64) -> Result<(), DocError> {
        let has_group = self.open.as_ref().and_then(OpenBlock::typing).is_some();
        let (logical_base, selection) = {
            let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
            let base = match block.typing() {
                Some(group) => self
                    .buffer
                    .len_bytes()
                    .checked_sub(group.range.len())
                    .and_then(|value| value.checked_add(group.draft.len()))
                    .ok_or_else(overflow)?,
                None => self.buffer.len_bytes(),
            };
            (base, block.selection)
        };
        let range = selection.range();
        let range_len = range.end.checked_sub(range.start).ok_or_else(overflow)?;
        let without_range = logical_base.checked_sub(range_len).ok_or_else(overflow)?;
        let total = without_range
            .checked_add(candidate.len())
            .ok_or_else(overflow)?;
        check_len(total, self.limits.text)?;
        let candidate = copy_bounded(candidate, self.limits.text)?;
        if has_group {
            self.version
                .next_draft()
                .map_err(DocError::Identity)?
                .next_draft()
                .map_err(DocError::Identity)?;
            self.commit_typing_group(now_millis)?;
        } else {
            self.version.next_draft().map_err(DocError::Identity)?;
        }
        let src_range = self
            .buffer
            .snapshot()
            .src_range(range.clone())
            .map_err(DocError::Buffer)?;
        let fragment = self
            .buffer
            .snapshot()
            .slice(&src_range, self.limits.text)
            .map_err(DocError::Buffer)?
            .into_owned();
        let after = range
            .start
            .checked_add(candidate.len())
            .ok_or_else(overflow)?;
        let after_selection = SelectionSpec::new(after, after);
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
        block.baseline_revision = self.buffer.revision();
        block.range = range.clone();
        block.baseline_fragment = fragment;
        block.selection = after_selection;
        block.draft = DraftSlot::Composition(CompositionState {
            range,
            candidate,
            before_selection: selection,
            selection: after_selection,
        });
        block.undone_typing = None;
        self.version = next_version;
        self.clear_cancelled_wait();
        self.snapshot_cache = None;
        Ok(())
    }

    fn replace_candidate(&mut self, candidate: &str) -> Result<(), DocError> {
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let range = {
            let state = self
                .open
                .as_ref()
                .and_then(OpenBlock::composition)
                .ok_or(DocError::NoComposition)?;
            state.range.clone()
        };
        let without_range = self
            .buffer
            .len_bytes()
            .checked_sub(range.len())
            .ok_or_else(overflow)?;
        let total = without_range
            .checked_add(candidate.len())
            .ok_or_else(overflow)?;
        check_len(total, self.limits.text)?;
        let after = range
            .start
            .checked_add(candidate.len())
            .ok_or_else(overflow)?;
        let candidate = copy_bounded(candidate, self.limits.text)?;
        let after_selection = SelectionSpec::new(after, after);
        let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
        let state = block.composition_mut().ok_or(DocError::NoComposition)?;
        state.candidate = candidate;
        state.selection = after_selection;
        block.selection = after_selection;
        self.version = next_version;
        self.clear_cancelled_wait();
        self.snapshot_cache = None;
        Ok(())
    }

    fn finish_composition(
        &mut self,
        revision: Option<DocumentRevision>,
        after_selection: SelectionSpec,
    ) {
        if let Some(block) = self.open.as_mut() {
            if let Some(revision) = revision {
                block.baseline_revision = revision;
            }
            let head = after_selection.head();
            block.range = head..head;
            block.baseline_fragment.clear();
            block.selection = after_selection;
            block.draft = DraftSlot::Empty;
            block.undone_typing = None;
        }
        self.snapshot_cache = None;
    }

    fn clear_cancelled_wait(&mut self) {
        if self
            .wait
            .as_ref()
            .is_some_and(|wait| wait.status == WaitStatus::Cancelled)
        {
            self.wait = None;
        }
    }

    fn sync_block_after_buffer_change(&mut self) {
        let selection = self.buffer_selection();
        if let Some(block) = self.open.as_mut() {
            let head = selection.head();
            block.selection = selection;
            block.range = head..head;
            block.baseline_fragment.clear();
            block.baseline_revision = self.buffer.revision();
            block.draft = DraftSlot::Empty;
            block.undone_typing = None;
        }
    }

    fn build_current_snapshot(&self, text_budget: TextBudget) -> Result<CurrentSnapshot, DocError> {
        let base = self.buffer.snapshot();
        let Some(block) = self.open.as_ref() else {
            check_len(base.len_bytes(), text_budget)?;
            return Ok(CurrentSnapshot::new(
                self.document.clone(),
                self.version,
                base.clone(),
                self.buffer.selection().clone(),
                self.resources.clone(),
            ));
        };
        let Some(draft) = block.current_draft() else {
            check_len(base.len_bytes(), text_budget)?;
            let selection = bind_selection(base, block.selection)?;
            return Ok(CurrentSnapshot::new(
                self.document.clone(),
                self.version,
                base.clone(),
                selection,
                self.resources.clone(),
            ));
        };
        let replaced = base
            .src_range(block.range.clone())
            .map_err(DocError::Buffer)?;
        let text =
            TextSnapshot::with_overlay(base, replaced, Arc::new(draft.to_owned()), text_budget)
                .map_err(DocError::Buffer)?;
        let selection = bind_selection(&text, block.selection)?;
        Ok(CurrentSnapshot::new(
            self.document.clone(),
            self.version,
            text,
            selection,
            self.resources.clone(),
        ))
    }

    fn patch_budget(&self) -> PatchBudget {
        PatchBudget::new(
            1,
            self.limits.text.max_bytes(),
            self.limits.text.max_bytes(),
        )
    }
}

fn overflow() -> DocError {
    DocError::Buffer(BufferError::LengthOverflow)
}

fn check_len(needed: usize, budget: TextBudget) -> Result<(), DocError> {
    if needed > budget.max_bytes() {
        return Err(DocError::Buffer(BufferError::BudgetExceeded {
            needed,
            limit: budget.max_bytes(),
        }));
    }
    Ok(())
}

fn copy_bounded(text: &str, budget: TextBudget) -> Result<String, DocError> {
    check_len(text.len(), budget)?;
    let mut copy = try_string(text.len())?;
    copy.push_str(text);
    Ok(copy)
}

fn try_string(capacity: usize) -> Result<String, DocError> {
    let mut text = String::new();
    text.try_reserve_exact(capacity)
        .map_err(|error| DocError::Buffer(BufferError::Allocation(error)))?;
    Ok(text)
}

fn slice_buffer(
    buffer: &StringBuffer,
    range: Range<usize>,
    budget: TextBudget,
) -> Result<String, DocError> {
    let snapshot = buffer.snapshot();
    let src_range = snapshot.src_range(range).map_err(DocError::Buffer)?;
    Ok(snapshot
        .slice(&src_range, budget)
        .map_err(DocError::Buffer)?
        .into_owned())
}

fn bind_selection(
    snapshot: &TextSnapshot,
    selection: SelectionSpec,
) -> Result<aline_buffer::SrcSelection, DocError> {
    aline_buffer::SrcSelection::new(
        snapshot
            .src_offset(selection.anchor())
            .map_err(DocError::Buffer)?,
        snapshot
            .src_offset(selection.head())
            .map_err(DocError::Buffer)?,
    )
    .map_err(DocError::Buffer)
}

fn selection_of(selection: &aline_buffer::SrcSelection) -> SelectionSpec {
    SelectionSpec::new(
        selection.anchor().byte_offset(),
        selection.head().byte_offset(),
    )
}

mod checked_commit;
mod protocols;
mod readonly;

#[cfg(test)]
mod checked_commit_tests;
