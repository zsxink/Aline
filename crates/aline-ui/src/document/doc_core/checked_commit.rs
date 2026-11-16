//! 文档所有者消费 edit 计划/校验的唯一普通输入与命令发布边界。
//!
//! 状态仍全部属于 DocCore；本模块只在组/IME提交或显式命令时解析和校验，
//! 不持第二全文、派生缓存或历史。输入携带自身范围和 before/after 选区；
//! 命令携带宿主身份信封和只读 Parsed，全部检查完成后才允许 Buffer 发布。
use super::{CommitOutcome, DocCore, DocError, bind_selection, copy_bounded};
use crate::document::open_block::{InputTarget, OpenBlock, VisualInput};
use crate::document::{EditorMode, InputIdentity};
use aline_buffer::{
    Affinity, AnchorTransform, ApplyOutcome, BufferError, CheckedBatch, OperationSource,
    SelectionSpec, SrcSelection, TextSnapshot, transform_anchor,
};
use aline_edit::{
    ChangePlan, EditBudget, EditCommand, EditContext, EditMode, EditRefusal, GroupReason,
    PlanOutcome, RawBlockEdit, RereadFailure, SourceEdit, plan_change, plan_raw_block_input,
    validate_change,
};
use aline_markdown::{Block, ParseBudget, ParseConfig, Parsed, parse};
use std::ops::Range;

impl From<BufferError> for DocError {
    fn from(error: BufferError) -> Self {
        Self::Buffer(error)
    }
}

impl DocCore {
    /// 执行已规划命令。活动组合先拒绝，待定普通输入与重规划命令原子发布。
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
        self.ensure_writable()?;
        if self.open.is_some() {
            self.ensure_ready()?;
        }
        let context = self.command_context();
        if context.has_composition() {
            return Err(DocError::CompositionActive);
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
        if plan.find_query_generation().is_some() {
            // 查找计划只能由查找入口消费；普通命令不得越过查询代次绑定。
            return Err(DocError::Edit(EditRefusal::StaleFindResults));
        }
        let selection = bind_selection(self.cached_current_text()?, self.current_selection())?;
        self.version.next_draft().map_err(DocError::Identity)?;
        if context.has_pending_input() {
            let outcome = self.publish_staged_command(
                now_millis,
                None,
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
            return self.settle_command_outcome(outcome);
        }
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
        self.settle_command_outcome(outcome)
    }

    /// 执行已规划的查找替换命令。
    ///
    /// `query_generation` 必须是查找所有者当前的实时查询代次；普通命令入口不得
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
        self.ensure_writable()?;
        if self.open.is_some() {
            self.ensure_ready()?;
        }
        let context = self.command_context();
        if context.has_composition() {
            return Err(DocError::CompositionActive);
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
        // 查找计划必须绑定本次消费的实时查询代次；普通计划在此入口一律拒绝。
        if plan.find_query_generation() != Some(query_generation) {
            return Err(DocError::Edit(EditRefusal::StaleFindResults));
        }
        let selection = bind_selection(self.cached_current_text()?, self.current_selection())?;
        self.version.next_draft().map_err(DocError::Identity)?;
        if context.has_pending_input() {
            let outcome = self.publish_staged_command(
                now_millis,
                Some(query_generation),
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
            return self.settle_command_outcome(outcome);
        }
        let find_context = context.with_find_query_generation(query_generation);
        let change = validate_change(
            plan,
            self.buffer.snapshot(),
            parsed,
            &selection,
            &find_context,
        )
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
        self.settle_command_outcome(outcome)
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

    fn command_context(&self) -> EditContext {
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

    fn edit_budget(&self) -> EditBudget {
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
        self.publish_staged_command(now_millis, None, |parsed, selection, context, budget| {
            plan_single_patch(
                parsed,
                selection,
                context,
                None,
                selection.range().byte_range(),
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
        let before = self.buffer.snapshot();
        let input_selection = bind_selection(before, group.before_selection)?;
        let command_selection = block.selection;
        let mut context = EditContext::new(self.command_context().mode());
        if let Some(generation) = find_generation {
            context = context.with_find_query_generation(generation);
        }
        let budget = self.edit_budget();
        let parsed = parse_for_edit(before, budget)?;
        let input_plan = plan_single_patch(
            &parsed,
            &input_selection,
            &context,
            block.visual_extent(),
            group.range.clone(),
            &group.draft,
            budget,
        )?;
        let input_batch = match input_plan {
            PlanOutcome::Changed(plan) => {
                validate_change(&plan, before, &parsed, &input_selection, &context)
                    .map_err(DocError::Edit)?
                    .batch()
                    .clone()
            }
            PlanOutcome::Unchanged(_) => {
                // 保留绑定当前基线的第一批；净无变化只记录实际命令。
                let draft = copy_bounded(&group.draft, budget.text())?;
                CheckedBatch::new(before, vec![(group.range.clone(), draft)], budget.patch())?
            }
        };
        self.buffer.apply_staged_command(
            &input_batch,
            (&input_selection, command_selection),
            now_millis,
            budget.text(),
            |candidate, selection| {
                let parsed = parse_for_edit(candidate, budget)?;
                let plan = plan_command(&parsed, selection, &context, budget)?;
                let PlanOutcome::Changed(plan) = plan else {
                    return Ok(None);
                };
                let change = validate_change(&plan, candidate, &parsed, selection, &context)
                    .map_err(DocError::Edit)?;
                Ok(Some((change.batch().clone(), change.target_selection())))
            },
        )
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
        let snapshot = self.buffer.snapshot();
        let selection = bind_selection(snapshot, selections.0)?;
        let budget = self.edit_budget();
        let parsed = parse_for_edit(snapshot, budget)?;
        // 这是唯一输入槽自身的发布，不是待定输入期间执行其它命令。
        // 草稿始终保留至发布成功；edit 读取的是该输入开始前的稳定 Buffer 基线。
        let context = EditContext::new(self.command_context().mode());
        let extent = self.open.as_ref().and_then(OpenBlock::visual_extent);
        let raw_block = matches!(source, OperationSource::Typing | OperationSource::ImeCommit)
            .then_some(extent)
            .flatten();
        let outcome = plan_single_patch(
            &parsed,
            &selection,
            &context,
            raw_block,
            range,
            &replacement,
            budget,
        )?;
        let PlanOutcome::Changed(plan) = outcome else {
            return Ok(ApplyOutcome::Unchanged);
        };
        let change = validate_change(&plan, snapshot, &parsed, &selection, &context)
            .map_err(DocError::Edit)?;
        let updated_target = if let Some(extent) = raw_block {
            let transformed = transform_extent(snapshot, change.batch(), extent)?;
            let after_head = selections.1.head();
            let target = next_target.clone().unwrap_or(after_head..after_head);
            let rebound = rebound_extent(change.candidate_blocks(), &transformed, &target);
            if next_target.is_some() && rebound.is_none() {
                // 滚动到下一输入组时，先确认解析中仍有可接收该输入的块。
                return Err(DocError::InvalidVisualBlock);
            }
            if let Some(rebound) = rebound {
                let visual = VisualInput::from_parse(
                    rebound,
                    change.candidate_protected_ranges(),
                    change.candidate_styled_ranges(),
                )?;
                if let Some(next_target) = next_target.as_ref() {
                    visual.check_permissions(next_target, |range| Ok(range.clone()))?;
                }
                Some(visual)
            } else {
                None
            }
        } else {
            None
        };
        let outcome = self
            .buffer
            .apply(
                change.batch(),
                selections.1,
                source,
                now_millis,
                self.limits.text,
            )
            .map_err(DocError::Buffer)?;
        if raw_block.is_some() && matches!(outcome, ApplyOutcome::Changed(_)) {
            if let Some(target) = updated_target {
                if let Some(block) = self.open.as_mut() {
                    block.target = InputTarget::Visual(target);
                }
            } else {
                self.open = None;
            }
        }
        Ok(outcome)
    }
}

fn parse_for_edit(snapshot: &TextSnapshot, budget: EditBudget) -> Result<Parsed, DocError> {
    parse(snapshot, ParseConfig::m0(), budget.parse()).map_err(|cause| {
        DocError::Edit(EditRefusal::RereadFailed {
            reason: RereadFailure::Parse,
            cause: Some(cause),
        })
    })
}

fn plan_single_patch(
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
