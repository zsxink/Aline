//! 纯计划入口：只读取快照、解析结果和选区，返回绑定完整的变更计划。

use std::ops::Range;

use aline_buffer::{
    CheckedBatch, DocumentRevision, SelectionSpec, SnapshotId, SrcSelection, TextSnapshot,
};
use aline_markdown::{ParseConfig, Parsed, ProjectionId};

use crate::budget::EditBudget;
use crate::command::{EditCommand, EditContext, EditMode, GroupReason, InlineFormat, SourceEdit};
use crate::errors::{map_batch_error, map_buffer_error};
use crate::permissions::check_visual_permissions;
use crate::refusal::{EditRefusal, RereadFailure};
use crate::reread::{Candidate, RereadContract, build_candidate, verify};

/// 计划无实际修改的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NoChangeReason {
    /// 替换文本与原文逐字节相同。
    ReplacementIdentical,
    /// 选区两端已经存在目标行内标记。
    AlreadyFormatted,
}

impl NoChangeReason {
    /// 返回稳定名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ReplacementIdentical => "ReplacementIdentical",
            Self::AlreadyFormatted => "AlreadyFormatted",
        }
    }
}

/// 候选文本的只读预览；不发布文本，也不持有可写缓冲。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Preview {
    after_len: usize,
    drawn_text: Box<str>,
}

impl Preview {
    /// 返回候选源码字节长度。
    pub const fn after_len(&self) -> usize {
        self.after_len
    }

    /// 返回候选解析后的绘制文本。
    pub fn drawn_text(&self) -> &str {
        &self.drawn_text
    }
}

/// 计划结果；无变化不产生计划，拒绝以 `Err` 返回。
#[derive(Clone, Debug)]
pub enum PlanOutcome {
    /// 已通过候选重读的变更计划。
    Changed(Box<ChangePlan>),
    /// 无需写入。
    Unchanged(NoChangeReason),
}

/// 一次绑定工作版本、补丁、分组和目标选区的受检变更计划。
#[derive(Clone, Debug)]
pub struct ChangePlan {
    mode: EditMode,
    reason: GroupReason,
    before_revision: DocumentRevision,
    before_snapshot: SnapshotId,
    before_projection: ProjectionId,
    before_selection: SrcSelection,
    pub(crate) expectations: Vec<ExpectedFragment>,
    batch: CheckedBatch,
    target_selection: SelectionSpec,
    preview: Preview,
    pub(crate) contract: RereadContract,
    pub(crate) parse_config: ParseConfig,
    pub(crate) budget: EditBudget,
}

impl ChangePlan {
    /// 返回计划对应的编辑模式。
    pub const fn mode(&self) -> EditMode {
        self.mode
    }

    /// 返回计划唯一的分组原因。
    pub const fn group_reason(&self) -> GroupReason {
        self.reason
    }

    /// 返回计划生成时的已提交代次。
    pub const fn before_revision(&self) -> DocumentRevision {
        self.before_revision
    }

    /// 返回计划绑定的 before 快照身份。
    pub fn before_snapshot_id(&self) -> &SnapshotId {
        &self.before_snapshot
    }

    /// 返回计划生成时读取的投影身份。
    pub fn before_projection_id(&self) -> &ProjectionId {
        &self.before_projection
    }

    /// 返回计划绑定的 before 选区。
    pub fn before_selection(&self) -> &SrcSelection {
        &self.before_selection
    }

    /// 返回受检补丁批次；消费方仍须由文档所有者原子发布。
    pub fn batch(&self) -> &CheckedBatch {
        &self.batch
    }

    /// 返回 after 坐标的目标选区。
    pub const fn target_selection(&self) -> SelectionSpec {
        self.target_selection
    }

    /// 返回候选源码字节长度。
    pub const fn after_len(&self) -> usize {
        self.preview.after_len()
    }

    /// 返回候选文本的只读预览。
    pub fn preview(&self) -> &Preview {
        &self.preview
    }
}

/// 计划期保存的期望片段；校验时会逐条重读核对。
#[derive(Clone, Debug)]
pub(crate) struct ExpectedFragment {
    pub(crate) range: Range<usize>,
    pub(crate) expected: String,
}

/// 生成绑定工作版本的变更计划；成功后仍不修改任何真实文档。
pub fn plan_change(
    snapshot: &TextSnapshot,
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    command: &EditCommand,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    precheck(context)?;
    bind_parsed(snapshot, parsed)?;
    snapshot
        .validate_selection(selection)
        .map_err(map_buffer_error)?;

    match command {
        EditCommand::Replace { edits } => {
            plan_replace(snapshot, parsed, selection, context, edits, budget)
        }
        EditCommand::ToggleInlineFormat { range, format } => {
            plan_format(snapshot, parsed, selection, context, range, *format, budget)
        }
    }
}

fn precheck(context: &EditContext) -> Result<(), EditRefusal> {
    if context.has_pending_input() {
        return Err(EditRefusal::PendingInput);
    }
    if context.has_composition() {
        return Err(EditRefusal::CompositionActive);
    }
    Ok(())
}

fn bind_parsed(snapshot: &TextSnapshot, parsed: &Parsed) -> Result<(), EditRefusal> {
    if parsed.snapshot().id() != snapshot.id() || parsed.projection().snapshot_id() != snapshot.id()
    {
        return Err(EditRefusal::ProjectionMismatch);
    }
    Ok(())
}

fn plan_replace(
    snapshot: &TextSnapshot,
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    edits: &[SourceEdit],
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    let mode = context.mode();
    let mut effective = Vec::new();
    for edit in edits {
        let range = snapshot
            .src_range(edit.range().clone())
            .map_err(map_buffer_error)?;
        let actual = snapshot
            .slice(&range, budget.text())
            .map_err(map_buffer_error)?;
        if actual.as_ref() != edit.expected() {
            return Err(EditRefusal::ExpectedFragmentMismatch {
                range: range.byte_range(),
            });
        }
        if edit.replacement() != edit.expected() {
            effective.push(EffectiveEdit {
                range: range.byte_range(),
                expected: edit.expected().to_owned(),
                replacement: edit.replacement().to_owned(),
            });
        }
    }
    if effective.is_empty() {
        return Ok(PlanOutcome::Unchanged(NoChangeReason::ReplacementIdentical));
    }
    if mode == EditMode::Visual {
        // F2-1 协议：Visual Replace 的“权威范围”是命令给出的编辑范围。
        // 补丁范围只来自 `edits`，计划既不会把它扩到选区，也不要求它与选区相等；
        // 计划把调用方传入的 `selection` 绑定为 `before_selection`，
        // `validate_change` 只校验当前选区仍等于该绑定（确认选区未被外部改动），
        // 不校验命令范围与选区一致。M1 消费者必须由选区派生命令范围，
        // 不得把光标当选区、把命令当范围；`ToggleInlineFormat` 是例外，
        // 额外要求范围等于选区（见 `plan_toggle_format`）。
        if effective.len() != 1 {
            return Err(EditRefusal::UnsupportedVisualBatch);
        }
        let only = effective.first().ok_or(EditRefusal::InvalidPlan)?;
        check_visual_permissions(parsed, &only.range)?;
    }
    effective.sort_by(|left, right| {
        left.range
            .start
            .cmp(&right.range.start)
            .then(left.range.end.cmp(&right.range.end))
    });

    let patches: Vec<(Range<usize>, String)> = effective
        .iter()
        .map(|edit| (edit.range.clone(), edit.replacement.clone()))
        .collect();
    let batch = CheckedBatch::new(snapshot, patches, budget.patch()).map_err(map_batch_error)?;
    let reason = match (mode, effective.len()) {
        (EditMode::Source, 1) => GroupReason::SourceReplace,
        (EditMode::Source, _) => GroupReason::BulkReplace,
        (EditMode::Visual, _) => GroupReason::VisibleReplace,
    };
    let contract = match mode {
        EditMode::Source => RereadContract::Source,
        EditMode::Visual => RereadContract::Visible,
    };
    let target_selection = after_selection_for_last_patch(batch.patches())?;
    let expectations = effective
        .iter()
        .map(|edit| ExpectedFragment {
            range: edit.range.clone(),
            expected: edit.expected.clone(),
        })
        .collect();

    let candidate = build_candidate(snapshot, parsed.config(), &batch, budget)?;
    verify(contract, parsed, &candidate, batch.patches(), budget)?;
    ensure_target_selection(&candidate.snapshot, target_selection)?;
    let preview = preview_from(&candidate);
    if preview.after_len() != batch.output_len() {
        return Err(EditRefusal::InvalidPlan);
    }

    Ok(PlanOutcome::Changed(Box::new(ChangePlan {
        mode,
        reason,
        before_revision: snapshot.revision(),
        before_snapshot: snapshot.id().clone(),
        before_projection: parsed.projection().id().clone(),
        before_selection: selection.clone(),
        expectations,
        batch,
        target_selection,
        preview,
        contract,
        parse_config: parsed.config(),
        budget,
    })))
}

fn plan_format(
    snapshot: &TextSnapshot,
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    range: &Range<usize>,
    format: InlineFormat,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    if context.mode() != EditMode::Visual {
        return Err(EditRefusal::WrongMode {
            expected: EditMode::Visual,
            actual: context.mode(),
        });
    }
    if range.is_empty() {
        return Err(EditRefusal::EmptySelection);
    }
    let selected_range = snapshot
        .src_range(range.clone())
        .map_err(map_buffer_error)?;
    if selected_range.byte_range() != selection.range().byte_range() {
        return Err(EditRefusal::SelectionMismatch);
    }
    let document_range = snapshot
        .src_range(0..snapshot.len_bytes())
        .map_err(map_buffer_error)?;
    let source = snapshot
        .slice(&document_range, budget.text())
        .map_err(map_buffer_error)?;
    let marker = format.marker();
    if surrounded_by_marker(&source, range, marker) {
        return Ok(PlanOutcome::Unchanged(NoChangeReason::AlreadyFormatted));
    }
    check_visual_permissions(parsed, range)?;

    let expected = source
        .get(range.clone())
        .ok_or(EditRefusal::InvalidPlan)?
        .to_owned();
    let patches = vec![
        (range.start..range.start, marker.to_owned()),
        (range.end..range.end, marker.to_owned()),
    ];
    let batch = CheckedBatch::new(snapshot, patches, budget.patch()).map_err(map_batch_error)?;
    let target_selection = SelectionSpec::new(
        selection
            .anchor()
            .byte_offset()
            .checked_add(marker.len())
            .ok_or(EditRefusal::LengthOverflow)?,
        selection
            .head()
            .byte_offset()
            .checked_add(marker.len())
            .ok_or(EditRefusal::LengthOverflow)?,
    );
    let expectations = vec![ExpectedFragment {
        range: range.clone(),
        expected,
    }];
    let contract = RereadContract::Format { marker };
    let candidate = build_candidate(snapshot, parsed.config(), &batch, budget)?;
    verify(contract, parsed, &candidate, batch.patches(), budget)?;
    ensure_target_selection(&candidate.snapshot, target_selection)?;
    let preview = preview_from(&candidate);
    if preview.after_len() != batch.output_len() {
        return Err(EditRefusal::InvalidPlan);
    }

    Ok(PlanOutcome::Changed(Box::new(ChangePlan {
        mode: EditMode::Visual,
        reason: GroupReason::InlineFormat(format),
        before_revision: snapshot.revision(),
        before_snapshot: snapshot.id().clone(),
        before_projection: parsed.projection().id().clone(),
        before_selection: selection.clone(),
        expectations,
        batch,
        target_selection,
        preview,
        contract,
        parse_config: parsed.config(),
        budget,
    })))
}

fn surrounded_by_marker(source: &str, range: &Range<usize>, marker: &str) -> bool {
    let left = range
        .start
        .checked_sub(marker.len())
        .and_then(|start| source.get(start..range.start));
    let right = range
        .end
        .checked_add(marker.len())
        .and_then(|end| source.get(range.end..end));
    left.is_some_and(|slice| slice == marker) && right.is_some_and(|slice| slice == marker)
}

fn after_selection_for_last_patch(
    patches: &[(Range<usize>, String)],
) -> Result<SelectionSpec, EditRefusal> {
    let (last_range, last_replacement) = patches.last().ok_or(EditRefusal::InvalidPlan)?;
    let prior_count = patches
        .len()
        .checked_sub(1)
        .ok_or(EditRefusal::InvalidPlan)?;
    let prior = patches.get(..prior_count).ok_or(EditRefusal::InvalidPlan)?;
    let mut added = 0usize;
    let mut removed = 0usize;
    for (range, replacement) in prior {
        added = added
            .checked_add(replacement.len())
            .ok_or(EditRefusal::LengthOverflow)?;
        removed = removed
            .checked_add(range.len())
            .ok_or(EditRefusal::LengthOverflow)?;
    }
    let after_start = last_range
        .start
        .checked_add(added)
        .ok_or(EditRefusal::LengthOverflow)?
        .checked_sub(removed)
        .ok_or(EditRefusal::LengthOverflow)?;
    let after_end = after_start
        .checked_add(last_replacement.len())
        .ok_or(EditRefusal::LengthOverflow)?;
    Ok(SelectionSpec::new(after_end, after_end))
}

pub(crate) fn ensure_target_selection(
    candidate: &TextSnapshot,
    selection: SelectionSpec,
) -> Result<(), EditRefusal> {
    candidate
        .src_offset(selection.anchor())
        .map_err(|_| EditRefusal::RereadFailed {
            reason: RereadFailure::InvalidAfterSelection,
            cause: None,
        })?;
    candidate
        .src_offset(selection.head())
        .map_err(|_| EditRefusal::RereadFailed {
            reason: RereadFailure::InvalidAfterSelection,
            cause: None,
        })?;
    Ok(())
}

fn preview_from(candidate: &Candidate) -> Preview {
    Preview {
        after_len: candidate.snapshot.len_bytes(),
        drawn_text: candidate.parsed.projection().drawn_text().into(),
    }
}

struct EffectiveEdit {
    range: Range<usize>,
    expected: String,
    replacement: String,
}
