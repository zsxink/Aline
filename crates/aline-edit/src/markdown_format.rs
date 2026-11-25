//! 手动全文格式化的纯计划，读取原片段并复读候选，不保存或持有预览状态。
//! DocCore 消费绑定快照的计划；本模块只整理顶层 ATX 和可安全整理的表格。

use std::ops::Range;

use aline_buffer::{PatchBudget, SelectionSpec, SrcSelection, TextSnapshot};
use aline_markdown::{
    BlockKind, DiagnosticKind, Parsed, SourceTable, TableSourceError, table_source,
};

use crate::budget::EditBudget;
use crate::command::{EditContext, GroupReason, SourceEdit};
use crate::errors::{map_batch_error, map_buffer_error};
use crate::plan::{self, PlanOutcome, StructuralChange};
use crate::refusal::{EditRefusal, RereadFailure};
use crate::reread::{Candidate, RereadContract};

/// 为当前文档规划手动格式化，不修改源码、选区或历史。
///
/// 保留未知语法、HTML、灰区及字面块。计划绑定当前快照、投影和方向性选区，
/// 确认时须经 `validate_change` 重验；无差异返回 `Unchanged`。
///
/// # Errors
///
/// 组合、待定输入、身份不符、预算不足、无法安全整理或候选语义变化时拒绝。
pub fn plan_markdown_format(
    snapshot: &TextSnapshot,
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    plan::precheck(context)?;
    plan::bind_parsed(snapshot, parsed)?;
    snapshot
        .validate_selection(selection)
        .map_err(map_buffer_error)?;
    if parsed.diagnostics().iter().any(|diagnostic| {
        matches!(
            diagnostic.kind(),
            DiagnosticKind::RawFallback | DiagnosticKind::BudgetExceeded
        )
    }) {
        return Err(failed(RereadFailure::Projection));
    }
    let source = snapshot
        .slice(
            &snapshot
                .src_range(0..snapshot.len_bytes())
                .map_err(map_buffer_error)?,
            budget.text(),
        )
        .map_err(map_buffer_error)?;
    let mut edits = FormatEdits {
        patches: Vec::new(),
        inserted: 0,
        budget: budget.patch(),
    };
    for block in parsed.blocks() {
        let range = block.range().byte_range();
        if protected_block(parsed, &range) {
            continue;
        }
        match block.kind() {
            BlockKind::Heading => {
                let raw = source.get(range.clone()).ok_or(EditRefusal::InvalidPlan)?;
                heading_edits(raw, range.start, &mut edits)?;
            }
            BlockKind::Table => {
                let edit = crate::table::tidy_table(parsed, block.range(), budget.text())?;
                edits.push(edit)?;
            }
            _ => {}
        }
    }
    let target = SelectionSpec::new(
        map_endpoint(selection.anchor().byte_offset(), &edits.patches)?,
        map_endpoint(selection.head().byte_offset(), &edits.patches)?,
    );
    plan::plan_structural(
        parsed,
        selection,
        context,
        StructuralChange {
            edits: edits.patches,
            target,
            reason: GroupReason::MarkdownFormat,
            contract: RereadContract::MarkdownFormat,
        },
        budget,
    )
}

fn protected_block(parsed: &Parsed, range: &Range<usize>) -> bool {
    parsed.protected_ranges().iter().any(|entry| {
        let protected = entry.range().byte_range();
        range.start <= protected.end && protected.start <= range.end
    }) || parsed.styled_ranges().iter().any(|entry| {
        let styled = entry.range().byte_range();
        range.start <= styled.end && styled.start <= range.end
    })
}

fn heading_edits(raw: &str, start: usize, edits: &mut FormatEdits) -> Result<(), EditRefusal> {
    if raw.contains(['\n', '\r']) {
        return Ok(());
    }
    let indentation = raw.bytes().take_while(|byte| *byte == b' ').count();
    let after_indent = raw.get(indentation..).ok_or(EditRefusal::InvalidPlan)?;
    let hashes = after_indent
        .bytes()
        .take_while(|byte| *byte == b'#')
        .count();
    if indentation > 3 || !(1..=6).contains(&hashes) {
        return Ok(());
    }
    let marker_end = indentation
        .checked_add(hashes)
        .ok_or(EditRefusal::LengthOverflow)?;
    let after_marker = raw.get(marker_end..).ok_or(EditRefusal::InvalidPlan)?;
    if !after_marker.is_empty() && !after_marker.starts_with([' ', '\t']) {
        return Ok(());
    }
    let content_start = marker_end
        .checked_add(
            after_marker
                .bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count(),
        )
        .ok_or(EditRefusal::LengthOverflow)?;
    let without_trailing_space = raw.trim_end_matches([' ', '\t']);
    let trailing_hashes = without_trailing_space
        .bytes()
        .rev()
        .take_while(|byte| *byte == b'#')
        .count();
    let closing_start = without_trailing_space
        .len()
        .checked_sub(trailing_hashes)
        .ok_or(EditRefusal::InvalidPlan)?;
    let has_closing = trailing_hashes > 0
        && closing_start > marker_end
        && raw
            .as_bytes()
            .get(closing_start - 1)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t'));
    let content_end = if has_closing {
        raw.get(..closing_start)
            .ok_or(EditRefusal::InvalidPlan)?
            .trim_end_matches([' ', '\t'])
            .len()
    } else {
        without_trailing_space.len()
    };
    if content_end <= content_start {
        return edits.push(SourceEdit::new(
            start..checked_add(start, raw.len())?,
            raw,
            "#".repeat(hashes),
        ));
    }
    let prefix = raw.get(..content_start).ok_or(EditRefusal::InvalidPlan)?;
    let canonical_prefix = format!("{} ", "#".repeat(hashes));
    edits.push(SourceEdit::new(
        start..checked_add(start, content_start)?,
        prefix,
        canonical_prefix,
    ))?;
    if content_end < raw.len() {
        edits.push(SourceEdit::new(
            checked_add(start, content_end)?..checked_add(start, raw.len())?,
            raw.get(content_end..).ok_or(EditRefusal::InvalidPlan)?,
            "",
        ))?;
    }
    Ok(())
}

fn checked_add(left: usize, right: usize) -> Result<usize, EditRefusal> {
    left.checked_add(right).ok_or(EditRefusal::LengthOverflow)
}

struct FormatEdits {
    patches: Vec<SourceEdit>,
    inserted: usize,
    budget: PatchBudget,
}

impl FormatEdits {
    fn push(&mut self, edit: SourceEdit) -> Result<(), EditRefusal> {
        if edit.expected() == edit.replacement() {
            return Ok(());
        }
        let inserted = checked_add(self.inserted, edit.replacement().len())?;
        let count = checked_add(self.patches.len(), 1)?;
        self.budget
            .check_input_totals(count, inserted)
            .map_err(map_batch_error)?;
        self.patches
            .try_reserve(1)
            .map_err(|_| EditRefusal::Allocation)?;
        self.patches.push(edit);
        self.inserted = inserted;
        Ok(())
    }
}

fn map_endpoint(src_off: usize, edits: &[SourceEdit]) -> Result<usize, EditRefusal> {
    let mut removed = 0usize;
    let mut inserted = 0usize;
    for edit in edits {
        let range = edit.range();
        if src_off < range.start {
            break;
        }
        if src_off < range.end {
            let base = range
                .start
                .checked_sub(removed)
                .and_then(|value| value.checked_add(inserted))
                .ok_or(EditRefusal::LengthOverflow)?;
            let mut relative = (src_off - range.start).min(edit.replacement().len());
            while !edit.replacement().is_char_boundary(relative) {
                relative = relative.checked_sub(1).ok_or(EditRefusal::InvalidPlan)?;
            }
            return checked_add(base, relative);
        }
        removed = checked_add(removed, range.len())?;
        inserted = checked_add(inserted, edit.replacement().len())?;
    }
    src_off
        .checked_sub(removed)
        .and_then(|value| value.checked_add(inserted))
        .ok_or(EditRefusal::LengthOverflow)
}

pub(crate) fn verify(
    before: &Parsed,
    candidate: &Candidate,
    budget: EditBudget,
) -> Result<(), EditRefusal> {
    let after = &candidate.parsed;
    if after
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.kind() == DiagnosticKind::RawFallback)
    {
        return Err(failed(RereadFailure::Projection));
    }
    if before.projection().drawn_text() != after.projection().drawn_text() {
        return Err(failed(RereadFailure::VisibleTextChanged));
    }
    if before.blocks().len() != after.blocks().len()
        || before
            .blocks()
            .iter()
            .zip(after.blocks())
            .any(|(left, right)| left.kind() != right.kind())
    {
        return Err(failed(RereadFailure::BlockStructureChanged));
    }
    verify_tables(before, after, budget)?;
    if before.protected_ranges().len() != after.protected_ranges().len() {
        return Err(failed(RereadFailure::HiddenSourceChanged));
    }
    for (left, right) in before
        .protected_ranges()
        .iter()
        .zip(after.protected_ranges())
    {
        if left.kind() != right.kind()
            || before
                .snapshot()
                .slice(left.range(), budget.text())
                .map_err(map_buffer_error)?
                .as_ref()
                != after
                    .snapshot()
                    .slice(right.range(), budget.text())
                    .map_err(map_buffer_error)?
                    .as_ref()
        {
            return Err(failed(RereadFailure::HiddenSourceChanged));
        }
    }
    Ok(())
}

fn failed(reason: RereadFailure) -> EditRefusal {
    EditRefusal::RereadFailed {
        reason,
        cause: None,
    }
}

fn verify_tables(before: &Parsed, after: &Parsed, budget: EditBudget) -> Result<(), EditRefusal> {
    for (left, right) in before.blocks().iter().zip(after.blocks()) {
        if left.kind() != BlockKind::Table {
            continue;
        }
        let left_table = table_source(before, left.range(), budget.text()).map_err(table_error)?;
        let right_table = table_source(after, right.range(), budget.text()).map_err(table_error)?;
        if left_table.alignments() != right_table.alignments()
            || left_table.rows().len() != right_table.rows().len()
        {
            return Err(failed(RereadFailure::BlockStructureChanged));
        }
        verify_cells(before, &left_table, after, &right_table, budget)?;
    }
    Ok(())
}

fn verify_cells(
    before: &Parsed,
    left: &SourceTable,
    after: &Parsed,
    right: &SourceTable,
    budget: EditBudget,
) -> Result<(), EditRefusal> {
    for (left_row, right_row) in left.rows().iter().zip(right.rows()) {
        if left_row.cells().len() != right_row.cells().len()
            || left_row.line_ending() != right_row.line_ending()
        {
            return Err(failed(RereadFailure::BlockStructureChanged));
        }
        for (left_cell, right_cell) in left_row.cells().iter().zip(right_row.cells()) {
            let left_text = before
                .snapshot()
                .slice(left_cell.source_range(), budget.text())
                .map_err(map_buffer_error)?;
            let right_text = after
                .snapshot()
                .slice(right_cell.source_range(), budget.text())
                .map_err(map_buffer_error)?;
            if left_text.trim_matches([' ', '\t']) != right_text.trim_matches([' ', '\t']) {
                return Err(failed(RereadFailure::HiddenSourceChanged));
            }
        }
    }
    if left.separator().line_ending() != right.separator().line_ending() {
        return Err(failed(RereadFailure::HiddenSourceChanged));
    }
    Ok(())
}

fn table_error(error: TableSourceError) -> EditRefusal {
    match error {
        TableSourceError::Source(error) => map_buffer_error(error),
        TableSourceError::InvalidTable => failed(RereadFailure::BlockStructureChanged),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aline_buffer::{PatchBudget, TextBudget};
    use aline_markdown::{ParseBudget, ParseConfig, parse};

    const TEXT: TextBudget = TextBudget::new(4096);
    const EDIT: EditBudget =
        EditBudget::new(PatchBudget::new(32, 4096, 4096), TEXT, ParseBudget::m0());

    fn candidate(source: &str) -> Result<Candidate, Box<dyn std::error::Error>> {
        let snapshot = TextSnapshot::new(source.to_owned(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        Ok(Candidate { snapshot, parsed })
    }

    #[test]
    fn markdown_format_table_patch_preserves_adjacent_blocks()
    -> Result<(), Box<dyn std::error::Error>> {
        let before = candidate("前\n\na|b\r\n:---|---:\n x\\|y | `a\\|b`\r\n| 短 |\n\n后")?;
        let block = before
            .parsed
            .blocks()
            .iter()
            .find(|block| block.kind() == BlockKind::Table)
            .ok_or("missing table")?;
        let edit = crate::table::tidy_table(&before.parsed, block.range(), TEXT)?;
        let batch = aline_buffer::CheckedBatch::new(
            &before.snapshot,
            vec![(edit.range().clone(), edit.replacement().to_owned())],
            EDIT.patch(),
        )?;
        let after =
            crate::reread::build_candidate(&before.snapshot, before.parsed.config(), &batch, EDIT)?;
        assert_eq!(
            before.parsed.projection().drawn_text(),
            after.parsed.projection().drawn_text(),
            "patch={:?}",
            batch.patches()
        );
        Ok(())
    }

    #[test]
    fn markdown_format_table_projection_keeps_cell_text() -> Result<(), Box<dyn std::error::Error>>
    {
        let before = candidate("a|b\r\n:---|---:\n x\\|y | `a\\|b`\r\n| 短 |")?;
        let after = candidate("| a | b |\r\n| :--- | ---: |\n| x\\|y | `a\\|b` |\r\n| 短 |  |")?;
        assert_eq!(
            before.parsed.projection().drawn_text(),
            after.parsed.projection().drawn_text()
        );
        Ok(())
    }

    #[test]
    fn markdown_format_reread_rejects_table_link_destination_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let before = candidate("| a |\n| --- |\n| [字](first) |")?;
        let after = candidate("| a |\n| --- |\n| [字](other) |")?;
        assert_eq!(
            before.parsed.projection().drawn_text(),
            after.parsed.projection().drawn_text()
        );
        assert!(matches!(
            verify(&before.parsed, &after, EDIT),
            Err(EditRefusal::RereadFailed {
                reason: RereadFailure::HiddenSourceChanged,
                ..
            })
        ));
        Ok(())
    }

    #[test]
    fn markdown_format_reread_rejects_html_comment_changes()
    -> Result<(), Box<dyn std::error::Error>> {
        let before = candidate("<!-- first -->\n\n# 字")?;
        let after = candidate("<!-- other -->\n\n# 字")?;
        assert_eq!(
            before.parsed.projection().drawn_text(),
            after.parsed.projection().drawn_text()
        );
        assert!(matches!(
            verify(&before.parsed, &after, EDIT),
            Err(EditRefusal::RereadFailed {
                reason: RereadFailure::HiddenSourceChanged,
                ..
            })
        ));
        Ok(())
    }
}
