//! 真实开块输入规划。文档核心拥有打开块和输入组，传入当前顶层块与补丁。
//! 此处校验块、源码与权限，再以受检计划返回一次单区间修改。

use aline_buffer::{SrcRange, SrcSelection};
use aline_markdown::{BlockKind, Parsed};

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
/// 输入目标为当前快照中的顶层块或空行插入点。补丁端点须处于该源码范围内。普通 Markdown
/// 标记及实体可以改变呈现。含在引用和列表内的表格也须保持逐格边界。真实 HTML
/// 块允许原文输入；MD-11 与行内 HTML 边界仍拒绝。
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
    let block_kind = parsed
        .blocks()
        .iter()
        .find(|candidate| candidate.range() == block)
        .map(|candidate| candidate.kind());
    if block_kind.is_none() && !is_blank_input_anchor(parsed, block, budget.text()) {
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
    if let Some(table) = crate::table::table_touched_by_input(parsed.presentation(), edit.range) {
        return crate::table::plan_table_raw_input(
            parsed, selection, context, table, patch, budget,
        );
    }
    plan_replace(
        snapshot,
        parsed,
        selection,
        context,
        &[patch],
        budget,
        if block_kind == Some(BlockKind::HtmlBlock) {
            ReplaceKind::RawHtml { block: extent }
        } else {
            ReplaceKind::RawBlock
        },
    )
}

/// 检查空选区是否锚定在真实块外的一条空白行首；空文档的起点也是合法入口。
/// 此检查不授予写入权限，调用方仍须检查保护区并复读输入补丁。
pub fn is_blank_input_anchor(
    parsed: &Parsed,
    anchor: &SrcRange,
    budget: aline_buffer::TextBudget,
) -> bool {
    let snapshot = parsed.snapshot();
    let Ok(bytes) = snapshot.validate_range(anchor) else {
        return false;
    };
    if !bytes.is_empty() {
        return false;
    }
    if snapshot.len_bytes() == 0 {
        return true;
    }
    let start = bytes.start;
    let Some(gap) = parsed.gaps().iter().find(|gap| {
        let bytes = gap.range().byte_range();
        bytes.start <= start
            && (start < bytes.end || start == bytes.end && start == snapshot.len_bytes())
    }) else {
        return false;
    };
    let fragment = |range| {
        let range = snapshot.src_range(range).ok()?;
        snapshot.slice(&range, budget).ok()
    };
    if start != 0 && fragment(start - 1..start).as_deref() != Some("\n") {
        return false;
    }
    fragment(start..gap.range().byte_range().end).is_some_and(|tail| {
        tail.split('\n').next().is_some_and(|line| {
            line.bytes()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'\r'))
        })
    })
}

fn copy_fragment(fragment: &str) -> Result<String, EditRefusal> {
    let mut copy = String::new();
    copy.try_reserve_exact(fragment.len())
        .map_err(|_| EditRefusal::Allocation)?;
    copy.push_str(fragment);
    Ok(copy)
}
