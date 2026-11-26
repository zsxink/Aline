//! 列表续行、层级和任务标记的源码计划。语法范围由同一解析结果约束，
//! 只在明确的列表项前缀上改写；真实文档、草稿和历史由 DocCore 发布。

use std::ops::Range;

use aline_buffer::{SelectionSpec, SrcOffset, SrcSelection};
use aline_markdown::{BlockKind, Parsed, PresentationKind, PresentationNode};

use crate::block_commands::{BlockCommand, SourceLine, lines, target_block};
use crate::budget::EditBudget;
use crate::command::{EditContext, GroupReason, SourceEdit};
use crate::errors::map_buffer_error;
use crate::plan::{PlanOutcome, StructuralChange, plan_structural};
use crate::refusal::EditRefusal;
use crate::reread::RereadContract;

mod hierarchy;
pub(crate) use hierarchy::{ListMove, verify_move};

pub(crate) struct ListPrefix {
    pub(crate) indent: usize,
    pub(crate) content: usize,
    container: usize,
    child_indent: Option<usize>,
    ordinal: Option<(Range<usize>, u64)>,
    task: Option<usize>,
}

pub(crate) fn prefix(line: &str) -> Result<ListPrefix, EditRefusal> {
    let mut container = 0usize;
    while line
        .get(container..)
        .is_some_and(|rest| rest.starts_with("> "))
    {
        container += 2;
    }
    let rest = line.get(container..).ok_or(EditRefusal::InvalidPlan)?;
    let indent = rest.bytes().take_while(|byte| *byte == b' ').count();
    let marker_start = container
        .checked_add(indent)
        .ok_or(EditRefusal::LengthOverflow)?;
    let marker = line.get(marker_start..).ok_or(EditRefusal::InvalidPlan)?;
    let first = marker
        .as_bytes()
        .first()
        .copied()
        .ok_or(EditRefusal::UnsupportedVisualStructure)?;
    let (width, ordinal) = if matches!(first, b'-' | b'+' | b'*') {
        (1, None)
    } else {
        let digits = marker.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 || digits > 9 || !matches!(marker.as_bytes().get(digits), Some(b'.' | b')'))
        {
            return Err(EditRefusal::UnsupportedVisualStructure);
        }
        let number = marker
            .get(..digits)
            .ok_or(EditRefusal::InvalidPlan)?
            .parse()
            .map_err(|_| EditRefusal::UnsupportedVisualStructure)?;
        (
            digits + 1,
            Some((marker_start..marker_start + digits, number)),
        )
    };
    let marker_end = marker_start
        .checked_add(width)
        .ok_or(EditRefusal::LengthOverflow)?;
    let tail = line.get(marker_end..).ok_or(EditRefusal::InvalidPlan)?;
    if !tail.is_empty() && !tail.starts_with([' ', '\t']) {
        return Err(EditRefusal::UnsupportedVisualStructure);
    }
    let whitespace = tail
        .bytes()
        .take_while(|byte| matches!(byte, b' ' | b'\t'))
        .count();
    let padding = tail.get(..whitespace).ok_or(EditRefusal::InvalidPlan)?;
    let child_indent = if padding.contains('\t') {
        None
    } else if (1..=4).contains(&whitespace) && whitespace < tail.len() {
        Some(marker_end - container + whitespace)
    } else {
        Some(marker_end - container + 1)
    };
    let mut content = marker_end
        .checked_add(whitespace)
        .ok_or(EditRefusal::LengthOverflow)?;
    let remaining = line.get(content..).ok_or(EditRefusal::InvalidPlan)?;
    let task = if remaining.starts_with("[ ]")
        || remaining.starts_with("[x]")
        || remaining.starts_with("[X]")
    {
        let after = remaining.get(3..).ok_or(EditRefusal::InvalidPlan)?;
        if after.is_empty() || after.starts_with([' ', '\t']) {
            let task = Some(content + 1);
            content += 3 + after
                .bytes()
                .take_while(|byte| matches!(byte, b' ' | b'\t'))
                .count();
            task
        } else {
            None
        }
    } else {
        None
    };
    Ok(ListPrefix {
        indent,
        content,
        container,
        child_indent,
        ordinal,
        task,
    })
}

pub(crate) fn plan(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    command: BlockCommand,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    let head = selection.head().byte_offset();
    if !list_item_contains_caret(parsed.presentation(), head) {
        return Err(EditRefusal::ListCommandOutsideItem);
    }
    let extent = target_block(parsed, selection, budget)?;
    let top = parsed
        .blocks()
        .iter()
        .find(|block| block.range().byte_range() == extent)
        .ok_or(EditRefusal::UnsupportedVisualStructure)?;
    if !matches!(top.kind(), BlockKind::List | BlockKind::BlockQuote) {
        return Err(EditRefusal::UnsupportedVisualStructure);
    }
    let source = parsed
        .snapshot()
        .slice(top.range(), budget.text())
        .map_err(map_buffer_error)?;
    let caret = selection
        .head()
        .byte_offset()
        .checked_sub(extent.start)
        .ok_or(EditRefusal::InvalidPlan)?;
    let source_lines = lines(&source);
    let line =
        physical_line(&source_lines, caret).ok_or(EditRefusal::UnsupportedVisualStructure)?;
    let line_start = extent
        .start
        .checked_add(line.start)
        .ok_or(EditRefusal::LengthOverflow)?;
    let selection_range = selection.range().byte_range();
    let (item, marker_line, current) = match marker_at(parsed, line_start, line.text) {
        Some((item, marker)) => (item, line, marker),
        None if command == BlockCommand::ListEnter => {
            let item = item_at(parsed.presentation(), line_start, None)
                .filter(|item| item.node.source_range().byte_range().start < line_start)
                .ok_or(EditRefusal::UnsupportedVisualStructure)?;
            let item_range = item.node.source_range().byte_range();
            if !selection_range.is_empty()
                || caret != line.start + line.text.len()
                || !continues_item_tail(&item, &source, &extent, line_start..head)
            {
                // 普通正文回车交回原输入路径，中间换行不生成列表结构命令。
                return Err(EditRefusal::ListCommandOutsideItem);
            }
            let marker_src_off = item_range
                .start
                .checked_sub(extent.start)
                .ok_or(EditRefusal::InvalidPlan)?;
            let marker_line = physical_line(&source_lines, marker_src_off)
                .ok_or(EditRefusal::UnsupportedVisualStructure)?;
            let (_, marker) = marker_at(parsed, extent.start + marker_line.start, marker_line.text)
                .ok_or(EditRefusal::UnsupportedVisualStructure)?;
            (item, marker_line, marker)
        }
        None => return Err(EditRefusal::UnsupportedVisualStructure),
    };
    if selection_range.start < line_start || selection_range.end > line_start + line.text.len() {
        return Err(EditRefusal::UnsupportedVisualStructure);
    }
    let target;
    let mut expected = Some(top.kind());
    let mut count = 1;
    let mut list_move = None;
    let edits = match command {
        BlockCommand::ToggleTask => {
            let marker = current
                .task
                .ok_or(EditRefusal::UnsupportedVisualStructure)?;
            let target = parsed
                .snapshot()
                .src_offset(line_start + marker - 1)
                .map_err(map_buffer_error)?;
            return plan_task_toggle(parsed, selection, context, &target, budget);
        }
        BlockCommand::ListEnter => {
            if !selection_range.is_empty() || caret < marker_line.start + current.content {
                return Err(EditRefusal::UnsupportedVisualStructure);
            }
            if marker_line.start == line.start
                && marker_line
                    .text
                    .get(current.content..)
                    .ok_or(EditRefusal::InvalidPlan)?
                    .trim()
                    .is_empty()
            {
                if item
                    .node
                    .children()
                    .iter()
                    .any(|child| !matches!(child.kind(), PresentationKind::TaskMarker { .. }))
                {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                }
                let indent = match item.parent {
                    Some(parent) => {
                        prefix_at(parsed, parent.source_range().byte_range().start, budget)?.indent
                    }
                    None => 0,
                };
                let (from, to) = if item.parent.is_some() {
                    let remove = current
                        .indent
                        .checked_sub(indent)
                        .ok_or(EditRefusal::InvalidPlan)?;
                    (current.container..current.container + remove, "")
                } else {
                    (current.container..line.text.len(), "")
                };
                let start = line_start
                    .checked_add(from.start)
                    .ok_or(EditRefusal::LengthOverflow)?;
                let end = line_start
                    .checked_add(from.end)
                    .ok_or(EditRefusal::LengthOverflow)?;
                let after = line_start
                    .checked_add(line.text.len() - from.len())
                    .ok_or(EditRefusal::LengthOverflow)?;
                target = SelectionSpec::new(after, after);
                if source.len() == from.len() && to.is_empty() {
                    expected = None;
                    count = 0;
                }
                if item.parent.is_some() {
                    list_move = Some(hierarchy::list_move(
                        parsed,
                        top,
                        item.node,
                        item.grandparent,
                        current.indent,
                        indent,
                    )?);
                }
                vec![SourceEdit::new(
                    start..end,
                    line.text.get(from).ok_or(EditRefusal::InvalidPlan)?,
                    to,
                )]
            } else {
                let mut next = marker_line
                    .text
                    .get(..current.content)
                    .ok_or(EditRefusal::InvalidPlan)?
                    .to_owned();
                if let Some(task) = current.task {
                    next.replace_range(task..task + 1, " ");
                }
                if let Some((range, number)) = current.ordinal {
                    let next_number = number
                        .checked_add(1)
                        .filter(|value| *value <= 999_999_999)
                        .ok_or(EditRefusal::UnsupportedVisualStructure)?;
                    next.replace_range(range, &next_number.to_string());
                }
                if next.ends_with(['.', ')', '-', '+', '*']) {
                    next.push(' ');
                }
                let eol = if !line.eol.is_empty() {
                    line.eol
                } else {
                    source_lines
                        .iter()
                        .find(|line| !line.eol.is_empty())
                        .map_or("\n", |line| line.eol)
                };
                let replacement = format!("{eol}{next}");
                let after = selection
                    .head()
                    .byte_offset()
                    .checked_add(replacement.len())
                    .ok_or(EditRefusal::LengthOverflow)?;
                target = SelectionSpec::new(after, after);
                vec![SourceEdit::new(
                    selection.head().byte_offset()..selection.head().byte_offset(),
                    "",
                    replacement,
                )]
            }
        }
        BlockCommand::IndentList | BlockCommand::OutdentList => {
            let (destination, destination_parent) = if command == BlockCommand::IndentList {
                let previous = item
                    .previous
                    .ok_or(EditRefusal::UnsupportedVisualStructure)?;
                let prefix = prefix_at(parsed, previous.source_range().byte_range().start, budget)?;
                (
                    prefix
                        .child_indent
                        .ok_or(EditRefusal::UnsupportedVisualStructure)?,
                    Some(previous),
                )
            } else {
                let parent = item.parent.ok_or(EditRefusal::UnsupportedVisualStructure)?;
                (
                    prefix_at(parsed, parent.source_range().byte_range().start, budget)?.indent,
                    item.grandparent,
                )
            };
            if destination == current.indent {
                return Err(EditRefusal::UnsupportedVisualStructure);
            }
            let item_extent = item.node.source_range().byte_range();
            let end = item_extent.end.min(extent.end);
            let item_source = source
                .get(line.start..end - extent.start)
                .ok_or(EditRefusal::InvalidPlan)?;
            let mut replacement = String::new();
            replacement
                .try_reserve(
                    item_source
                        .len()
                        .checked_add(item_source.len())
                        .ok_or(EditRefusal::LengthOverflow)?,
                )
                .map_err(|_| EditRefusal::Allocation)?;
            for nested in lines(item_source) {
                if nested.text.is_empty() {
                    replacement.push_str(nested.eol);
                    continue;
                }
                let own_container = nested
                    .text
                    .bytes()
                    .take(current.container)
                    .eq(line.text.bytes().take(current.container));
                if !own_container {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                }
                replacement.push_str(
                    nested
                        .text
                        .get(..current.container)
                        .ok_or(EditRefusal::InvalidPlan)?,
                );
                if destination > current.indent {
                    replacement.push_str(&" ".repeat(destination - current.indent));
                    replacement.push_str(
                        nested
                            .text
                            .get(current.container..)
                            .ok_or(EditRefusal::InvalidPlan)?,
                    );
                } else {
                    let remove = current.indent - destination;
                    let leading = nested
                        .text
                        .get(current.container..)
                        .ok_or(EditRefusal::InvalidPlan)?;
                    if leading.bytes().take_while(|byte| *byte == b' ').count() < remove {
                        return Err(EditRefusal::UnsupportedVisualStructure);
                    }
                    replacement.push_str(
                        nested
                            .text
                            .get(current.container + remove..)
                            .ok_or(EditRefusal::InvalidPlan)?,
                    );
                }
                replacement.push_str(nested.eol);
            }
            let delta = destination.abs_diff(current.indent);
            let prefix_start = line_start
                .checked_add(current.container)
                .ok_or(EditRefusal::LengthOverflow)?;
            let shift = |src_off: usize| {
                if src_off < prefix_start {
                    Some(src_off)
                } else if destination > current.indent {
                    src_off.checked_add(delta)
                } else {
                    // 选区端点落在被移除的缩进里时，保留在本行的容器内。
                    Some(src_off.saturating_sub(delta).max(prefix_start))
                }
            };
            let moved = hierarchy::list_move(
                parsed,
                top,
                item.node,
                destination_parent,
                current.indent,
                destination,
            )?;
            let plan_move = |blank: &str| {
                let endpoint = |src_off| {
                    shift(src_off)
                        .and_then(|shifted| shifted.checked_add(blank.len()))
                        .ok_or(EditRefusal::LengthOverflow)
                };
                let target = SelectionSpec::new(
                    endpoint(selection.anchor().byte_offset())?,
                    endpoint(selection.head().byte_offset())?,
                );
                plan_structural(
                    parsed,
                    selection,
                    context,
                    StructuralChange {
                        edits: vec![SourceEdit::new(
                            line_start..end,
                            item_source,
                            format!("{blank}{replacement}"),
                        )],
                        target,
                        reason: GroupReason::BlockCommand,
                        contract: RereadContract::ListMove(
                            moved.clone().with_leading_blank(blank.len()),
                        ),
                    },
                    budget,
                )
            };
            let result = plan_move("");
            // 非 1 的有序标记不能打断段落。只在直接缩进复读失败时尝试分隔空行。
            if command == BlockCommand::IndentList
                && current.ordinal.is_some_and(|(_, ordinal)| ordinal != 1)
                && matches!(
                    result,
                    Err(EditRefusal::RereadFailed {
                        reason: crate::refusal::RereadFailure::BlockStructureChanged,
                        cause: None,
                    })
                )
                && let Some(previous_line) = source_lines
                    .iter()
                    .rev()
                    .find(|previous| previous.start < line.start && !previous.eol.is_empty())
            {
                let container = line
                    .text
                    .get(..current.container)
                    .ok_or(EditRefusal::InvalidPlan)?;
                return plan_move(&format!("{container}{}", previous_line.eol));
            }
            return result;
        }
        BlockCommand::Convert(_) => return Err(EditRefusal::InvalidPlan),
    };
    plan_structural(
        parsed,
        selection,
        context,
        StructuralChange {
            edits,
            target,
            reason: GroupReason::BlockCommand,
            contract: list_move.map_or(
                RereadContract::Block {
                    start: extent.start,
                    expected,
                    count,
                },
                RereadContract::ListMove,
            ),
        },
        budget,
    )
}

fn physical_line<'a, 'source>(
    lines: &'a [SourceLine<'source>],
    src_off: usize,
) -> Option<&'a SourceLine<'source>> {
    lines
        .iter()
        .find(|line| line.start <= src_off && src_off <= line.start + line.text.len())
}

fn marker_at<'parsed>(
    parsed: &'parsed Parsed,
    line_start: usize,
    line: &str,
) -> Option<(ItemAt<'parsed>, ListPrefix)> {
    let marker = prefix(line).ok()?;
    let marker_start = line_start
        .checked_add(marker.container)?
        .checked_add(marker.indent)?;
    let item = item_at(parsed.presentation(), marker_start, None)?;
    (item.node.source_range().byte_range().start == marker_start).then_some((item, marker))
}

fn continues_item_tail(
    item: &ItemAt<'_>,
    source: &str,
    extent: &Range<usize>,
    line: Range<usize>,
) -> bool {
    let item_range = item.node.source_range().byte_range();
    let item_end = item_range.end.min(extent.end);
    if !source
        .get(line.end - extent.start..item_end - extent.start)
        .is_some_and(|tail| {
            tail.bytes()
                .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        })
    {
        return false;
    }
    item.node.children().last().is_some_and(|child| {
        let child_range = child.source_range().byte_range();
        child_range.start <= line.end
            && line.start < child_range.end
            && matches!(
                child.kind(),
                // 紧凑列表的行内节点直接挂于 Item，松散列表用 Paragraph 包裹。
                PresentationKind::Paragraph
                    | PresentationKind::Text
                    | PresentationKind::Emphasis
                    | PresentationKind::Strong
                    | PresentationKind::Strikethrough
                    | PresentationKind::Link { .. }
                    | PresentationKind::Image { .. }
                    | PresentationKind::Code
                    | PresentationKind::InlineMath
                    | PresentationKind::SoftBreak
                    | PresentationKind::HardBreak
            )
    })
}

fn prefix_at(
    parsed: &Parsed,
    src_off: usize,
    budget: EditBudget,
) -> Result<ListPrefix, EditRefusal> {
    let source = parsed
        .snapshot()
        .slice(
            &parsed
                .snapshot()
                .src_range(0..parsed.snapshot().len_bytes())
                .map_err(map_buffer_error)?,
            budget.text(),
        )
        .map_err(map_buffer_error)?;
    let line_start = source
        .get(..src_off)
        .ok_or(EditRefusal::InvalidPlan)?
        .rfind('\n')
        .map_or(0, |offset| offset + 1);
    let first = source
        .get(line_start..)
        .ok_or(EditRefusal::InvalidPlan)?
        .split('\n')
        .next()
        .ok_or(EditRefusal::UnsupportedVisualStructure)?;
    prefix(first.strip_suffix('\r').unwrap_or(first))
}

struct ItemAt<'a> {
    node: &'a PresentationNode,
    previous: Option<&'a PresentationNode>,
    parent: Option<&'a PresentationNode>,
    grandparent: Option<&'a PresentationNode>,
}

fn list_item_contains_caret(nodes: &[PresentationNode], head: usize) -> bool {
    nodes.iter().any(|node| {
        let range = node.source_range().byte_range();
        range.start <= head
            && head <= range.end
            && (matches!(node.kind(), PresentationKind::Item)
                || list_item_contains_caret(node.children(), head))
    })
}

fn item_at<'a>(
    nodes: &'a [PresentationNode],
    line_start: usize,
    ancestors: Option<(&'a PresentationNode, Option<&'a PresentationNode>)>,
) -> Option<ItemAt<'a>> {
    for node in nodes {
        if matches!(node.kind(), PresentationKind::List { .. }) {
            let mut previous = None;
            for item in node.children() {
                let range = item.source_range().byte_range();
                if range.start <= line_start && line_start < range.end {
                    let (parent, grandparent) = ancestors
                        .map_or((None, None), |(parent, grandparent)| {
                            (Some(parent), grandparent)
                        });
                    if let Some(nested) = item_at(item.children(), line_start, Some((item, parent)))
                    {
                        return Some(nested);
                    }
                    return Some(ItemAt {
                        node: item,
                        previous,
                        parent,
                        grandparent,
                    });
                }
                previous = Some(item);
            }
        } else if let Some(found) = item_at(node.children(), line_start, ancestors) {
            return Some(found);
        }
    }
    None
}

pub(crate) fn plan_task_toggle(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    target: &SrcOffset,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    crate::plan::precheck(context)?;
    let snapshot = parsed.snapshot();
    snapshot
        .validate_selection(selection)
        .map_err(map_buffer_error)?;
    let target = snapshot.validate_offset(target).map_err(map_buffer_error)?;
    let marker =
        task_at(parsed.presentation(), target).ok_or(EditRefusal::UnsupportedVisualStructure)?;
    let range = marker.source_range().byte_range();
    let source = snapshot
        .slice(marker.source_range(), budget.text())
        .map_err(map_buffer_error)?;
    if !matches!(source.as_ref(), "[ ]" | "[x]" | "[X]") {
        return Err(EditRefusal::UnsupportedVisualStructure);
    }
    let start = range
        .start
        .checked_add(1)
        .ok_or(EditRefusal::LengthOverflow)?;
    let original = source.get(1..2).ok_or(EditRefusal::InvalidPlan)?;
    let replacement = if original == " " { "x" } else { " " };
    let block = parsed
        .blocks()
        .iter()
        .find(|block| {
            let extent = block.range().byte_range();
            extent.start <= range.start && range.end <= extent.end
        })
        .ok_or(EditRefusal::UnsupportedVisualStructure)?;
    plan_structural(
        parsed,
        selection,
        context,
        StructuralChange {
            edits: vec![SourceEdit::new(start..start + 1, original, replacement)],
            target: SelectionSpec::new(
                selection.anchor().byte_offset(),
                selection.head().byte_offset(),
            ),
            reason: GroupReason::BlockCommand,
            contract: RereadContract::Block {
                start: block.range().byte_range().start,
                expected: Some(block.kind()),
                count: 1,
            },
        },
        budget,
    )
}

fn task_at(nodes: &[PresentationNode], target: usize) -> Option<&PresentationNode> {
    for node in nodes {
        let range = node.source_range().byte_range();
        if matches!(node.kind(), PresentationKind::TaskMarker { .. }) && range.contains(&target) {
            return Some(node);
        }
        if let Some(marker) = task_at(node.children(), target) {
            return Some(marker);
        }
    }
    None
}
