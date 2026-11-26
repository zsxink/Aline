//! 行内格式的样式复读。只读同一解析器的展示树，核对目标样式切换和范围外样式保全。

use std::ops::Range;

use aline_buffer::{Affinity, SelectionSpec};
use aline_markdown::{Parsed, PresentationKind, PresentationNode};

use crate::command::InlineFormat;
use crate::errors::{map_buffer_error, map_mapping_error};
use crate::inline::InlineAction;
use crate::refusal::{EditRefusal, RereadFailure};

#[derive(Debug, Eq, PartialEq)]
struct Run {
    drawn: Range<usize>,
    style: u8,
}

pub(crate) fn verify(
    before: &Parsed,
    after: &Parsed,
    selected: Range<usize>,
    target: SelectionSpec,
    format: InlineFormat,
    action: InlineAction,
) -> Result<(), EditRefusal> {
    let before_range = drawn_range(before, selected)?;
    let after_range = drawn_range(
        after,
        target.anchor().min(target.head())..target.anchor().max(target.head()),
    )?;
    if before_range != after_range || before_range.is_empty() {
        return Err(failed());
    }
    let mut before_runs = Vec::new();
    let mut after_runs = Vec::new();
    runs(before.presentation(), 0, &mut before_runs)?;
    runs(after.presentation(), 0, &mut after_runs)?;
    let flag = flag(format);
    let was_formatted = all_styled(&before_runs, &before_range, flag);
    if action == InlineAction::Remove {
        if !was_formatted {
            return Err(failed());
        }
        if after_runs
            .iter()
            .any(|run| intersects(&run.drawn, &after_range) && run.style & flag != 0)
        {
            return Err(failed());
        }
    } else if was_formatted || !all_styled(&after_runs, &after_range, flag) {
        return Err(failed());
    }
    if without_target(&before_runs, &before_range, flag)?
        != without_target(&after_runs, &after_range, flag)?
    {
        return Err(failed());
    }
    Ok(())
}

pub(crate) fn drawn_range(
    parsed: &Parsed,
    range: Range<usize>,
) -> Result<Range<usize>, EditRefusal> {
    let start = parsed
        .snapshot()
        .src_offset(range.start)
        .map_err(map_buffer_error)?;
    let end = parsed
        .snapshot()
        .src_offset(range.end)
        .map_err(map_buffer_error)?;
    let start = parsed
        .projection()
        .drawn_at(&start, Affinity::Before)
        .map_err(map_mapping_error)?
        .into_value()
        .byte_offset();
    let end = parsed
        .projection()
        .drawn_at(&end, Affinity::After)
        .map_err(map_mapping_error)?
        .into_value()
        .byte_offset();
    Ok(start..end)
}

fn flag(format: InlineFormat) -> u8 {
    match format {
        InlineFormat::Strong => 1,
        InlineFormat::Emphasis => 2,
        InlineFormat::Strike => 4,
        InlineFormat::Code => 8,
        InlineFormat::Link => 16,
    }
}

fn runs(
    nodes: &[PresentationNode],
    inherited: u8,
    output: &mut Vec<Run>,
) -> Result<(), EditRefusal> {
    for node in nodes {
        let own = match node.kind() {
            PresentationKind::Strong => 1,
            PresentationKind::Emphasis => 2,
            PresentationKind::Strikethrough => 4,
            PresentationKind::Code => 8,
            PresentationKind::Link { .. } => 16,
            _ => 0,
        };
        let style = inherited | own;
        if node.children().is_empty() {
            append(output, node.drawn_range().byte_range(), style)?;
        } else {
            runs(node.children(), style, output)?;
        }
    }
    Ok(())
}

fn intersects(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start < right.end && right.start < left.end
}

fn all_styled(runs: &[Run], selected: &Range<usize>, flag: u8) -> bool {
    let mut covered = selected.start;
    for run in runs {
        if !intersects(&run.drawn, selected) {
            continue;
        }
        let start = run.drawn.start.max(selected.start);
        if start != covered || run.style & flag == 0 {
            return false;
        }
        covered = run.drawn.end.min(selected.end);
    }
    covered == selected.end
}

fn without_target(
    runs: &[Run],
    selected: &Range<usize>,
    flag: u8,
) -> Result<Vec<Run>, EditRefusal> {
    let mut normalized = Vec::new();
    for run in runs {
        if !intersects(&run.drawn, selected) {
            append(&mut normalized, run.drawn.clone(), run.style)?;
            continue;
        }
        let start = run.drawn.start.max(selected.start);
        let end = run.drawn.end.min(selected.end);
        append(&mut normalized, run.drawn.start..start, run.style)?;
        append(&mut normalized, start..end, run.style & !flag)?;
        append(&mut normalized, end..run.drawn.end, run.style)?;
    }
    Ok(normalized)
}

fn append(output: &mut Vec<Run>, drawn: Range<usize>, style: u8) -> Result<(), EditRefusal> {
    if drawn.is_empty() {
        return Ok(());
    }
    if let Some(last) = output.last_mut()
        && last.drawn.end == drawn.start
        && last.style == style
    {
        last.drawn.end = drawn.end;
        return Ok(());
    }
    output.try_reserve(1).map_err(|_| EditRefusal::Allocation)?;
    output.push(Run { drawn, style });
    Ok(())
}

fn failed() -> EditRefusal {
    EditRefusal::RereadFailed {
        reason: RereadFailure::HiddenSourceChanged,
        cause: None,
    }
}
