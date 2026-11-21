//! 真实开块输入规划。文档核心拥有打开块和输入组，传入当前顶层块与补丁。
//! 此处校验块、源码与权限，再以受检计划返回一次单区间修改。

use aline_buffer::{SrcRange, SrcSelection};
use aline_markdown::Parsed;

use crate::budget::EditBudget;
use crate::command::{EditContext, EditMode, SourceEdit};
use crate::errors::map_buffer_error;
use crate::plan::{NoChangeReason, PlanOutcome, ReplaceKind, bind_parsed, plan_replace, precheck};
use crate::refusal::{BudgetKind, EditRefusal};

/// 打开编辑块的一次源码输入，范围须属于当前已提交快照。
pub struct RawBlockEdit<'a> {
    range: &'a SrcRange,
    expected: &'a str,
    replacement: &'a str,
}

impl<'a> RawBlockEdit<'a> {
    /// 借用原补丁范围、基线片段与输入；计划生成时统一复验。
    pub const fn new(range: &'a SrcRange, expected: &'a str, replacement: &'a str) -> Self {
        Self {
            range,
            expected,
            replacement,
        }
    }
}

/// 为单个已打开的可视化顶层块准备一次源码补丁。
///
/// 块和补丁必须绑定当前解析与快照，补丁端点须处于块源码内。普通 Markdown
/// 标记及实体可以改变呈现，MD-11 和 HTML 边界仍经同一套保护规则拒绝。
/// 文档核心使用常规 `validate_change` 复验后才可发布。
pub fn plan_raw_block_input(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    block: &SrcRange,
    edit: RawBlockEdit<'_>,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    precheck(context)?;
    if context.mode() != EditMode::Visual {
        return Err(EditRefusal::WrongMode {
            expected: EditMode::Visual,
            actual: context.mode(),
        });
    }
    let snapshot = parsed.snapshot();
    bind_parsed(snapshot, parsed)?;
    snapshot
        .validate_selection(selection)
        .map_err(map_buffer_error)?;
    let extent = snapshot.validate_range(block).map_err(map_buffer_error)?;
    let range = snapshot
        .validate_range(edit.range)
        .map_err(map_buffer_error)?;
    if !parsed
        .blocks()
        .iter()
        .any(|candidate| candidate.range() == block)
    {
        return Err(EditRefusal::InvalidRawBlock);
    }
    if range.start < extent.start || range.end > extent.end {
        return Err(EditRefusal::RawInputOutsideBlock);
    }
    let actual = snapshot
        .slice(edit.range, budget.text())
        .map_err(map_buffer_error)?;
    if actual != edit.expected {
        return Err(EditRefusal::ExpectedFragmentMismatch { range });
    }
    let limit = budget.text().max_bytes();
    if edit.replacement.len() > limit {
        return Err(EditRefusal::BudgetExceeded {
            kind: BudgetKind::CandidateBytes,
            needed: Some(edit.replacement.len()),
            limit: Some(limit),
        });
    }
    if edit.expected == edit.replacement {
        return Ok(PlanOutcome::Unchanged(NoChangeReason::ReplacementIdentical));
    }
    let patch = SourceEdit::new(
        range,
        copy_fragment(edit.expected)?,
        copy_fragment(edit.replacement)?,
    );
    plan_replace(
        snapshot,
        parsed,
        selection,
        context,
        &[patch],
        budget,
        ReplaceKind::RawBlock,
    )
}

fn copy_fragment(fragment: &str) -> Result<String, EditRefusal> {
    let mut copy = String::new();
    copy.try_reserve_exact(fragment.len())
        .map_err(|_| EditRefusal::Allocation)?;
    copy.push_str(fragment);
    Ok(copy)
}
