//! 文档所有者消费 edit 计划/校验的唯一普通输入与命令发布边界。
//!
//! 状态仍全部属于 DocCore；本模块只在组/IME提交或显式命令时解析和校验，
//! 不持第二全文、派生缓存或历史。输入携带自身范围和 before/after 选区；
//! 命令携带宿主身份信封和只读 Parsed，全部检查完成后才允许 Buffer 发布。
use super::{
    CommitOutcome, DocCore, DocError, TypingGroup, TypingRedo, WorkingVersion, bind_selection,
    copy_bounded, slice_buffer,
};
use crate::document::open_block::{InputTarget, OpenBlock, VisualInput};
use crate::document::{EditorMode, InputIdentity};
use aline_buffer::{
    Affinity, AnchorTransform, ApplyOutcome, BufferEditBase, BufferError, CheckedBatch,
    OperationSource, PreparedBufferEdit, SelectionSpec, SrcSelection, TextSnapshot,
    transform_anchor,
};
use aline_edit::{
    ChangePlan, EditBudget, EditCommand, EditContext, EditMode, EditRefusal, GroupReason,
    PlanOutcome, RawBlockEdit, RereadFailure, SourceEdit, plan_change, plan_raw_block_input,
    validate_change,
};
use aline_markdown::{Block, ParseBudget, ParseConfig, Parsed, parse};
use std::ops::Range;

// 仅测试编译的解析调用计数，用于证明发布段不再解析。
// 使用线程局部量，避免并行测试互相干扰；生产构建不包含该状态。
#[cfg(test)]
thread_local! {
    static PARSE_CALLS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// 返回当前线程累计的 `parse_for_edit` 调用次数。
#[cfg(test)]
pub(crate) fn parse_call_count() -> usize {
    PARSE_CALLS.with(std::cell::Cell::get)
}

impl From<BufferError> for DocError {
    fn from(error: BufferError) -> Self {
        Self::Buffer(error)
    }
}

/// 提交时的视图覆盖：`target_mode` 表达目标视图（如跨模式重做），
/// `edit_mode` 强制固定权限上下文（首次保存迁移按 Source 规划）。
#[derive(Clone, Copy, Default)]
struct BoundContext {
    target_mode: Option<EditorMode>,
    edit_mode: Option<EditMode>,
}

impl DocCore {
    pub(super) fn publish_typing_redo_in_current_mode(
        &mut self,
        redo: &TypingRedo,
        next_version: WorkingVersion,
    ) -> Result<(), DocError> {
        let snapshot = self.buffer.snapshot();
        let selection = bind_selection(snapshot, redo.group.before_selection)?;
        let budget = self.edit_budget();
        let parsed = parse_for_edit(snapshot, budget)?;
        // 权限按当前视图判定；重做不得把记录时的 Source 权限用于当前 Visual。
        let edit_mode = match self.mode {
            EditorMode::Visual => EditMode::Visual,
            EditorMode::Source => EditMode::Source,
        };
        let context = EditContext::new(edit_mode);
        let raw_block = match &redo.target {
            InputTarget::Visual(target) => Some(target.extent.clone()),
            InputTarget::Legacy | InputTarget::Source => None,
        };
        let plan = plan_single_patch(
            &parsed,
            &selection,
            &context,
            raw_block.as_ref(),
            redo.group.range.clone(),
            &redo.group.draft,
            budget,
        )?;
        let PlanOutcome::Changed(plan) = plan else {
            // 净无变化的待定 redo 只恢复逻辑选区，不创建空历史。
            let after = bind_selection(snapshot, redo.group.after_selection)?;
            let fragment = if let Some(block) = self.open.as_ref() {
                self.validate_selection(block, redo.group.after_selection)?;
                Some(slice_buffer(
                    &self.buffer,
                    redo.group.after_selection.range(),
                    self.limits.text,
                )?)
            } else {
                None
            };
            self.buffer
                .set_selection(&after)
                .map_err(DocError::Buffer)?;
            if let (Some(block), Some(fragment)) = (self.open.as_mut(), fragment) {
                block.range = redo.group.after_selection.range();
                block.baseline_fragment = fragment;
                block.selection = redo.group.after_selection;
                block.baseline_revision = self.buffer.revision();
            }
            self.typing_redo = None;
            self.version = next_version;
            self.snapshot_cache = None;
            return Ok(());
        };
        let change = validate_change(&plan, snapshot, &parsed, &selection, &context)
            .map_err(DocError::Edit)?;

        // Buffer 在 apply 前读取自身 selection 形成历史 before-selection。临时设置
        // 为输入组起点；若任何提交预算拒绝，立即恢复调用前逻辑选区。
        let previous_selection = self.buffer.selection().clone();
        self.buffer
            .set_selection(&selection)
            .map_err(DocError::Buffer)?;
        let applied = self.buffer.apply(
            change.batch(),
            redo.group.after_selection,
            OperationSource::Typing,
            redo.group.last_input_at,
            self.limits.text,
        );
        let applied = match applied {
            Ok(ApplyOutcome::Changed(applied)) => applied,
            Ok(ApplyOutcome::Unchanged) => {
                self.buffer
                    .set_selection(&previous_selection)
                    .map_err(DocError::Buffer)?;
                self.typing_redo = None;
                self.version = next_version;
                self.snapshot_cache = None;
                return Ok(());
            }
            Err(error) => {
                self.buffer
                    .set_selection(&previous_selection)
                    .map_err(DocError::Buffer)?;
                return Err(DocError::Buffer(error));
            }
        };
        let revision = applied.revision();
        self.buffer.seal_pending();
        self.version = self
            .version
            .after_commit(revision)
            .map_err(DocError::Identity)?;
        self.typing_redo = None;
        self.open = None;
        self.snapshot_cache = None;
        Ok(())
    }

    /// 执行已规划的普通命令。查找计划须经 `apply_find_change` 消费。
    ///
    /// `identity` 必须来自本 DocCore 的当前输入身份；`parsed` 必须绑定该源码
    /// 快照、M0 配置和计划投影。复验文档/工作版本/快照/配置/请求及当前选区后
    /// 才原子发布。拒绝保留草稿、组合和历史；redo 本身不拒绝实际新编辑。
    /// 调用者应先处理 `PlanOutcome::Unchanged`，不制造空计划或历史节点。
    pub fn apply_change(
        &mut self,
        identity: &InputIdentity,
        parsed: &Parsed,
        plan: &ChangePlan,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        self.apply_bound_change(
            identity,
            parsed,
            plan,
            None,
            BoundContext::default(),
            now_millis,
        )
    }

    /// 以固定 Source 权限上下文应用系统迁移命令；不切换用户当前视图。
    ///
    /// 用于首次保存/另存为的引用重写：重写是系统迁移而非用户 Visual 编辑，
    /// 因此按 Source 权限规划与应用，但成功后仍保持用户模式。
    pub(crate) fn apply_migration_change(
        &mut self,
        identity: &InputIdentity,
        parsed: &Parsed,
        plan: &ChangePlan,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        self.apply_bound_change(
            identity,
            parsed,
            plan,
            None,
            BoundContext {
                edit_mode: Some(EditMode::Source),
                ..BoundContext::default()
            },
            now_millis,
        )
    }

    /// 执行已规划的查找替换命令。
    ///
    /// `query_generation` 来自查找所有者的当前查询，`0` 是合法初始代次。普通命令入口不得
    /// 消费查找计划，查找入口也不得消费普通计划，两侧都按 `StaleFindResults`
    /// 拒绝。待定普通输入存在时，输入组与替换命令按两步候选事务原子发布，替换
    /// 命令在封口候选上按同一查询重新查找并重规划。拒绝保留草稿、组合、选区与
    /// 历史；调用者应先处理 `PlanOutcome::Unchanged`，不制造空计划或历史节点。
    pub fn apply_find_change(
        &mut self,
        identity: &InputIdentity,
        parsed: &Parsed,
        plan: &ChangePlan,
        query_generation: u64,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        self.apply_bound_change(
            identity,
            parsed,
            plan,
            Some(query_generation),
            BoundContext::default(),
            now_millis,
        )
    }

    /// 按源码模式原子执行需要回退到源码视图的查找替换。
    ///
    /// 模式、打开块和派生请求只在 Buffer 实际提交后发布；计划拒绝保留调用前
    /// 模式、块、选区与历史。待定输入及组合期间仍拒绝。
    pub(crate) fn apply_find_change_in_source_mode(
        &mut self,
        identity: &InputIdentity,
        parsed: &Parsed,
        plan: &ChangePlan,
        query_generation: u64,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        self.apply_bound_change(
            identity,
            parsed,
            plan,
            Some(query_generation),
            BoundContext {
                target_mode: Some(EditorMode::Source),
                ..BoundContext::default()
            },
            now_millis,
        )
    }

    fn apply_bound_change(
        &mut self,
        identity: &InputIdentity,
        parsed: &Parsed,
        plan: &ChangePlan,
        query_generation: Option<u64>,
        binding: BoundContext,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        let BoundContext {
            target_mode,
            edit_mode: context_mode,
        } = binding;
        self.ensure_writable()?;
        if self.open.is_some() {
            self.ensure_ready()?;
        }
        let current_context = self.command_context();
        let context = match (context_mode, target_mode) {
            (Some(edit_mode), _) => EditContext::new(edit_mode)
                .with_pending_input(current_context.has_pending_input())
                .with_composition_active(current_context.has_composition()),
            (None, Some(mode)) => {
                let edit_mode = match mode {
                    EditorMode::Visual => EditMode::Visual,
                    EditorMode::Source => EditMode::Source,
                };
                EditContext::new(edit_mode)
                    .with_pending_input(current_context.has_pending_input())
                    .with_composition_active(current_context.has_composition())
            }
            (None, None) => current_context,
        };
        if context.has_composition() {
            return Err(DocError::CompositionActive);
        }
        if target_mode.is_some_and(|mode| mode != self.mode) && context.has_pending_input() {
            return Err(DocError::PendingInput);
        }
        if context.has_pending_input()
            && self
                .open
                .as_ref()
                .is_some_and(|block| block.target == InputTarget::Legacy)
            && identity.document_id() == &self.document
            && identity.working_version() != self.version
        {
            // 既有无头入口的组前计划无法用于当前草稿，保留其明确拒绝原因。
            return Err(DocError::PendingInput);
        }
        self.accept_document_result(identity)?;
        if parsed.config() != ParseConfig::m0() || parsed.snapshot().id() != identity.snapshot_id()
        {
            return Err(DocError::StaleResult);
        }
        if plan.group_reason() == GroupReason::RawBlockInput {
            return Err(DocError::Edit(EditRefusal::UnsupportedReplan));
        }
        plan.validate_find_query_generation(query_generation)
            .map_err(DocError::Edit)?;
        let next_request = target_mode
            .filter(|mode| *mode != self.mode)
            .map(|_| self.derived_request.next().map_err(DocError::Identity))
            .transpose()?;
        let selection = bind_selection(self.cached_current_text()?, self.current_selection())?;
        self.version.next_draft().map_err(DocError::Identity)?;
        if context.has_pending_input() {
            let outcome = self.publish_staged_command(
                now_millis,
                query_generation,
                |candidate, candidate_selection, context, _| {
                    plan.replan_on_equivalent_candidate(
                        parsed,
                        &selection,
                        candidate,
                        candidate_selection,
                        context,
                    )
                    .map_err(DocError::Edit)
                },
            )?;
            let committed = self.settle_command_outcome(outcome)?;
            self.publish_target_mode(committed, target_mode, next_request);
            return Ok(committed);
        }
        let context = query_context(context, query_generation);
        let change = validate_change(plan, self.buffer.snapshot(), parsed, &selection, &context)
            .map_err(DocError::Edit)?;
        let outcome = self
            .buffer
            .apply(
                change.batch(),
                change.target_selection(),
                OperationSource::Command,
                now_millis,
                self.limits.text,
            )
            .map_err(DocError::Buffer)?;
        let committed = self.settle_command_outcome(outcome)?;
        self.publish_target_mode(committed, target_mode, next_request);
        Ok(committed)
    }

    fn publish_target_mode(
        &mut self,
        outcome: CommitOutcome,
        target_mode: Option<EditorMode>,
        next_request: Option<crate::document::DerivedRequestId>,
    ) {
        let Some(target_mode) = target_mode
            .filter(|mode| *mode != self.mode)
            .filter(|_| matches!(outcome, CommitOutcome::Committed { .. }))
        else {
            return;
        };
        self.open = None;
        self.wait = None;
        self.snapshot_cache = None;
        if let Some(next_request) = next_request {
            self.derived_request = next_request;
        }
        self.mode = target_mode;
    }

    pub(super) fn settle_command_outcome(
        &mut self,
        outcome: ApplyOutcome,
    ) -> Result<CommitOutcome, DocError> {
        match outcome {
            ApplyOutcome::Unchanged => Ok(CommitOutcome::Unchanged),
            ApplyOutcome::Changed(applied) => {
                let revision = applied.revision();
                self.buffer.seal_pending();
                self.version = self
                    .version
                    .after_commit(revision)
                    .map_err(DocError::Identity)?;
                self.sync_block_after_buffer_change();
                self.snapshot_cache = None;
                Ok(CommitOutcome::Committed { revision })
            }
        }
    }

    pub(super) fn command_context(&self) -> EditContext {
        let mode = match self.mode {
            EditorMode::Visual => EditMode::Visual,
            EditorMode::Source => EditMode::Source,
        };
        EditContext::new(mode)
            .with_pending_input(
                self.open
                    .as_ref()
                    .and_then(|block| block.typing())
                    .is_some(),
            )
            .with_composition_active(self.is_composition_active())
    }

    pub(super) fn edit_budget(&self) -> EditBudget {
        let parse_budget = ParseBudget::m0().with_max_source_bytes(
            ParseBudget::m0()
                .max_source_bytes()
                .min(self.limits.text.max_bytes()),
        );
        EditBudget::new(self.patch_budget(), self.limits.text, parse_budget)
    }

    /// 用候选中的选区规划剪贴命令，一次发布输入组和独立命令。
    pub(super) fn publish_staged_replacement(
        &mut self,
        replacement: &str,
        now_millis: u64,
    ) -> Result<ApplyOutcome, DocError> {
        self.publish_staged_range_replacement(None, replacement, now_millis)
    }

    pub(super) fn publish_staged_range_replacement(
        &mut self,
        range: Option<Range<usize>>,
        replacement: &str,
        now_millis: u64,
    ) -> Result<ApplyOutcome, DocError> {
        self.publish_staged_command(now_millis, None, |parsed, selection, context, budget| {
            plan_single_patch(
                parsed,
                selection,
                context,
                None,
                range.unwrap_or_else(|| selection.range().byte_range()),
                replacement,
                budget,
            )
        })
    }

    /// 以候选快照规划并发布一个独立命令；`find_generation` 供查找替换绑定实时查询代次。
    fn publish_staged_command(
        &mut self,
        now_millis: u64,
        find_generation: Option<u64>,
        plan_command: impl FnOnce(
            &Parsed,
            &SrcSelection,
            &EditContext,
            EditBudget,
        ) -> Result<PlanOutcome, DocError>,
    ) -> Result<ApplyOutcome, DocError> {
        let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
        let group = block.typing().ok_or(DocError::PendingInput)?;
        let prepared = StagedInput {
            group,
            visual_extent: block.visual_extent(),
            selection: block.selection,
            context: query_context(
                EditContext::new(self.command_context().mode()),
                find_generation,
            ),
            now_millis,
            budget: self.edit_budget(),
        }
        .prepare(self.buffer.edit_base(), plan_command)?;
        self.buffer
            .publish_prepared(prepared)
            .map_err(DocError::Buffer)
    }

    pub(super) fn publish_input(
        &mut self,
        range: Range<usize>,
        replacement: String,
        selections: (SelectionSpec, SelectionSpec),
        source: OperationSource,
        now_millis: u64,
        next_target: Option<Range<usize>>,
    ) -> Result<ApplyOutcome, DocError> {
        let prepared = InputPatch {
            range,
            replacement,
            selections,
            source,
            now_millis,
            next_target,
        }
        .prepare(
            self.buffer.edit_base(),
            self.command_context().mode(),
            self.open.as_ref().and_then(OpenBlock::visual_extent),
            self.edit_budget(),
        )?;
        let Some(buffer) = prepared.buffer else {
            return Ok(ApplyOutcome::Unchanged);
        };
        let outcome = self.buffer.publish_prepared(buffer)?;
        self.update_raw_input_target(prepared.target);
        Ok(outcome)
    }

    pub(super) fn update_raw_input_target(&mut self, target: InputTargetUpdate) {
        match target {
            InputTargetUpdate::Preserve => {}
            InputTargetUpdate::Rebind(target) => {
                if let Some(block) = self.open.as_mut() {
                    block.target = InputTarget::Visual(target);
                }
            }
            InputTargetUpdate::Close => self.open = None,
        }
    }
}

/// 单区间输入的已冻结意图。准备可在后台进行，发布仍由文档所有者负责。
pub(super) struct InputPatch {
    pub(super) range: Range<usize>,
    pub(super) replacement: String,
    pub(super) selections: (SelectionSpec, SelectionSpec),
    pub(super) source: OperationSource,
    pub(super) now_millis: u64,
    pub(super) next_target: Option<Range<usize>>,
}

pub(super) struct PreparedInputPatch {
    pub(super) buffer: Option<PreparedBufferEdit>,
    pub(super) target: InputTargetUpdate,
}

pub(super) enum InputTargetUpdate {
    Preserve,
    Rebind(VisualInput),
    Close,
}

impl InputPatch {
    pub(super) fn prepare(
        self,
        base: BufferEditBase,
        mode: EditMode,
        extent: Option<&Range<usize>>,
        budget: EditBudget,
    ) -> Result<PreparedInputPatch, DocError> {
        let snapshot = base.snapshot();
        let selection = bind_selection(snapshot, self.selections.0)?;
        let parsed = parse_for_edit(snapshot, budget)?;
        // 真实基线是输入开始前的已提交快照；草稿仍由 DocCore 保留。
        let context = EditContext::new(mode);
        let raw_block = matches!(
            self.source,
            OperationSource::Typing | OperationSource::ImeCommit
        )
        .then_some(extent)
        .flatten();
        let outcome = plan_single_patch(
            &parsed,
            &selection,
            &context,
            raw_block,
            self.range,
            &self.replacement,
            budget,
        )?;
        let PlanOutcome::Changed(plan) = outcome else {
            return Ok(PreparedInputPatch {
                buffer: None,
                target: InputTargetUpdate::Preserve,
            });
        };
        let change = validate_change(&plan, snapshot, &parsed, &selection, &context)
            .map_err(DocError::Edit)?;
        let target = match raw_block {
            Some(extent) => {
                let transformed = transform_extent(snapshot, change.batch(), extent)?;
                let after_head = self.selections.1.head();
                let target = self
                    .next_target
                    .as_ref()
                    .cloned()
                    .unwrap_or(after_head..after_head);
                let rebound = rebound_extent(change.candidate_blocks(), &transformed, &target);
                if self.next_target.is_some() && rebound.is_none() {
                    return Err(DocError::InvalidVisualBlock);
                }
                if let Some(rebound) = rebound {
                    let visual = VisualInput::from_parse(
                        rebound,
                        change.candidate_protected_ranges(),
                        change.candidate_styled_ranges(),
                    )?;
                    if let Some(next_target) = &self.next_target {
                        visual.check_permissions(next_target, |range| Ok(range.clone()))?;
                    }
                    InputTargetUpdate::Rebind(visual)
                } else {
                    InputTargetUpdate::Close
                }
            }
            None => InputTargetUpdate::Preserve,
        };
        let buffer = base.prepare_apply(
            change.batch(),
            self.selections.1,
            self.source,
            self.now_millis,
            budget.text(),
        )?;
        Ok(PreparedInputPatch {
            buffer: Some(buffer),
            target,
        })
    }
}

pub(super) struct StagedInput<'a> {
    pub(super) group: &'a TypingGroup,
    pub(super) visual_extent: Option<&'a Range<usize>>,
    pub(super) selection: SelectionSpec,
    pub(super) context: EditContext,
    pub(super) now_millis: u64,
    pub(super) budget: EditBudget,
}

impl StagedInput<'_> {
    pub(super) fn prepare(
        self,
        base: BufferEditBase,
        plan_command: impl FnOnce(
            &Parsed,
            &SrcSelection,
            &EditContext,
            EditBudget,
        ) -> Result<PlanOutcome, DocError>,
    ) -> Result<PreparedBufferEdit, DocError> {
        let before = base.snapshot();
        let input_selection = bind_selection(before, self.group.before_selection)?;
        let input_context = EditContext::new(self.context.mode());
        let parsed = parse_for_edit(before, self.budget)?;
        let input_plan = plan_single_patch(
            &parsed,
            &input_selection,
            &input_context,
            self.visual_extent,
            self.group.range.clone(),
            &self.group.draft,
            self.budget,
        )?;
        let input_batch = match input_plan {
            PlanOutcome::Changed(plan) => {
                validate_change(&plan, before, &parsed, &input_selection, &input_context)
                    .map_err(DocError::Edit)?
                    .batch()
                    .clone()
            }
            PlanOutcome::Unchanged(_) => {
                // 保留绑定实际基线的第一批；净无变化只记录实际命令。
                let draft = copy_bounded(&self.group.draft, self.budget.text())?;
                CheckedBatch::new(
                    before,
                    vec![(self.group.range.clone(), draft)],
                    self.budget.patch(),
                )?
            }
        };
        base.prepare_staged_command(
            &input_batch,
            (&input_selection, self.selection),
            self.now_millis,
            self.budget.text(),
            |candidate, selection| {
                let parsed = parse_for_edit(candidate, self.budget)?;
                let plan = plan_command(&parsed, selection, &self.context, self.budget)?;
                let PlanOutcome::Changed(plan) = plan else {
                    return Ok(None);
                };
                let change = validate_change(&plan, candidate, &parsed, selection, &self.context)
                    .map_err(DocError::Edit)?;
                Ok(Some((change.batch().clone(), change.target_selection())))
            },
        )
    }
}

fn query_context(context: EditContext, generation: Option<u64>) -> EditContext {
    generation.map_or(context, |generation| {
        context.with_find_query_generation(generation)
    })
}

fn parse_for_edit(snapshot: &TextSnapshot, budget: EditBudget) -> Result<Parsed, DocError> {
    #[cfg(test)]
    PARSE_CALLS.with(|calls| calls.set(calls.get().saturating_add(1)));
    parse(snapshot, ParseConfig::m0(), budget.parse()).map_err(|cause| {
        DocError::Edit(EditRefusal::RereadFailed {
            reason: RereadFailure::Parse,
            cause: Some(cause),
        })
    })
}

pub(super) fn plan_single_patch(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    raw_block: Option<&Range<usize>>,
    range: Range<usize>,
    replacement: &str,
    budget: EditBudget,
) -> Result<PlanOutcome, DocError> {
    let snapshot = parsed.snapshot();
    let src_range = snapshot.src_range(range.clone())?;
    let expected = snapshot.slice(&src_range, budget.text())?;
    if let Some(extent) = raw_block {
        let block = snapshot.src_range(extent.clone())?;
        return plan_raw_block_input(
            parsed,
            selection,
            context,
            &block,
            RawBlockEdit::new(&src_range, expected.as_ref(), replacement),
            budget,
        )
        .map_err(DocError::Edit);
    }
    let expected = copy_bounded(expected.as_ref(), budget.text())?;
    let replacement = copy_bounded(replacement, budget.text())?;
    let command = EditCommand::replace(vec![SourceEdit::new(range, expected, replacement)]);
    plan_change(snapshot, parsed, selection, context, &command, budget).map_err(DocError::Edit)
}

fn rebound_extent(
    blocks: &[Block],
    transformed: &Range<usize>,
    target: &Range<usize>,
) -> Option<Range<usize>> {
    let mut matching = blocks
        .iter()
        .map(|block| block.range().byte_range())
        .filter(|extent| {
            transformed.start <= extent.start
                && extent.end <= transformed.end
                && extent.start <= target.start
                && target.end <= extent.end
        });
    let extent = matching.next()?;
    matching.next().is_none().then_some(extent)
}

pub(super) fn transform_extent(
    snapshot: &TextSnapshot,
    batch: &CheckedBatch,
    extent: &Range<usize>,
) -> Result<Range<usize>, DocError> {
    let adjust = |src_off, affinity| {
        let anchor = snapshot.src_offset(src_off).map_err(DocError::Buffer)?;
        match transform_anchor(snapshot, &anchor, batch, affinity).map_err(DocError::Buffer)? {
            AnchorTransform::Valid { byte } => Ok(byte),
            AnchorTransform::Invalidated => Err(DocError::OutsideOpenBlock),
        }
    };
    Ok(adjust(extent.start, Affinity::Before)?..adjust(extent.end, Affinity::After)?)
}
