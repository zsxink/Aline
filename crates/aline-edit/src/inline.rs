//! 行内命令的纯源码规划。读取当前语义节点识别真实定界符，返回补丁和复读许可区间。
//! 状态与发布由计划器和 DocCore 持有，此处不写 Buffer、不改变输入组或历史。

use std::ops::Range;

use aline_buffer::{SelectionSpec, SrcSelection};
use aline_markdown::{Parsed, PresentationKind, PresentationNode};

use crate::command::InlineFormat;
use crate::permissions::check_visual_permissions;
use crate::refusal::EditRefusal;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InlineAction {
    Pair,
    Add,
    Remove,
}

/// 仅许可本次生成或删除的语法标记改变隐藏语义，其余隐藏源码仍须一致。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InlineContract {
    pub(crate) before_syntax: [Range<usize>; 2],
    pub(crate) after_syntax: [Range<usize>; 2],
    pub(crate) action: InlineAction,
    pub(crate) selected: Range<usize>,
    pub(crate) target: SelectionSpec,
    pub(crate) format: InlineFormat,
}

pub(crate) struct InlineEdit {
    pub(crate) patches: Vec<(Range<usize>, String)>,
    pub(crate) selection: SelectionSpec,
    pub(crate) checked_range: Range<usize>,
    pub(crate) contract: InlineContract,
}

pub(crate) fn plan(
    source: &str,
    parsed: &Parsed,
    selection: &SrcSelection,
    format: InlineFormat,
) -> Result<InlineEdit, EditRefusal> {
    let selected = selection.range().byte_range();
    check_visual_permissions(parsed, &selected)?;
    if !selected.is_empty()
        && let Some(span) = existing(
            parsed.presentation(),
            source,
            &selected,
            &crate::inline_semantics::drawn_range(parsed, selected.clone())?,
            format,
        )?
    {
        check_visual_permissions(parsed, &span.outer)?;
        return remove(span.outer, span.inner, selection, format);
    }
    let content = source
        .get(selected.clone())
        .ok_or(EditRefusal::InvalidPlan)?;
    let (left, right) = delimiters(content, format)?;
    let after_start = selected
        .start
        .checked_add(left.len())
        .ok_or(EditRefusal::LengthOverflow)?;
    let after_end = selected
        .end
        .checked_add(left.len())
        .ok_or(EditRefusal::LengthOverflow)?;
    let after_right_end = after_end
        .checked_add(right.len())
        .ok_or(EditRefusal::LengthOverflow)?;
    let target = SelectionSpec::new(
        selection
            .anchor()
            .byte_offset()
            .checked_add(left.len())
            .ok_or(EditRefusal::LengthOverflow)?,
        selection
            .head()
            .byte_offset()
            .checked_add(left.len())
            .ok_or(EditRefusal::LengthOverflow)?,
    );
    let patches = if selected.is_empty() {
        let mut pair = left;
        pair.try_reserve(right.len())
            .map_err(|_| EditRefusal::Allocation)?;
        pair.push_str(&right);
        vec![(selected.clone(), pair)]
    } else {
        vec![
            (selected.start..selected.start, left),
            (selected.end..selected.end, right),
        ]
    };
    Ok(InlineEdit {
        patches,
        selection: target,
        checked_range: selected.clone(),
        contract: InlineContract {
            before_syntax: [selected.start..selected.start, selected.end..selected.end],
            after_syntax: [selected.start..after_start, after_end..after_right_end],
            action: if selected.is_empty() {
                InlineAction::Pair
            } else {
                InlineAction::Add
            },
            selected,
            target,
            format,
        },
    })
}

fn remove(
    outer: Range<usize>,
    inner: Range<usize>,
    selection: &SrcSelection,
    format: InlineFormat,
) -> Result<InlineEdit, EditRefusal> {
    let target_end = outer
        .start
        .checked_add(inner.len())
        .ok_or(EditRefusal::LengthOverflow)?;
    let selected = selection.range().byte_range();
    let target = if selected == outer {
        if selection.anchor().byte_offset() <= selection.head().byte_offset() {
            SelectionSpec::new(outer.start, target_end)
        } else {
            SelectionSpec::new(target_end, outer.start)
        }
    } else {
        let removed = inner
            .start
            .checked_sub(outer.start)
            .ok_or(EditRefusal::InvalidPlan)?;
        SelectionSpec::new(
            selection
                .anchor()
                .byte_offset()
                .checked_sub(removed)
                .ok_or(EditRefusal::InvalidPlan)?,
            selection
                .head()
                .byte_offset()
                .checked_sub(removed)
                .ok_or(EditRefusal::InvalidPlan)?,
        )
    };
    let left = outer.start..inner.start;
    let right = inner.end..outer.end;
    Ok(InlineEdit {
        patches: vec![
            (left.clone(), String::new()),
            (right.clone(), String::new()),
        ],
        selection: target,
        checked_range: outer,
        contract: InlineContract {
            before_syntax: [left, right],
            after_syntax: [inner.start..inner.start, inner.end..inner.end],
            action: InlineAction::Remove,
            selected,
            target,
            format,
        },
    })
}

fn matches_kind(kind: &PresentationKind, format: InlineFormat) -> bool {
    matches!(
        (kind, format),
        (PresentationKind::Strong, InlineFormat::Strong)
            | (PresentationKind::Emphasis, InlineFormat::Emphasis)
            | (PresentationKind::Strikethrough, InlineFormat::Strike)
            | (PresentationKind::Code, InlineFormat::Code)
            | (PresentationKind::Link { .. }, InlineFormat::Link)
    )
}

struct InlineSpan {
    outer: Range<usize>,
    inner: Range<usize>,
}

fn existing(
    nodes: &[PresentationNode],
    source: &str,
    selected: &Range<usize>,
    drawn: &Range<usize>,
    format: InlineFormat,
) -> Result<Option<InlineSpan>, EditRefusal> {
    for node in nodes {
        let outer = node.source_range().byte_range();
        if outer.start > selected.start || outer.end < selected.end {
            continue;
        }
        if matches_kind(node.kind(), format)
            && let Some(inner) = content_range(source, &outer, format)?
            && (*selected == outer
                || *selected == inner
                || (inner.start <= selected.start
                    && selected.end <= inner.end
                    && node.drawn_range().byte_range() == *drawn))
        {
            return Ok(Some(InlineSpan { outer, inner }));
        }
        if let Some(found) = existing(node.children(), source, selected, drawn, format)? {
            return Ok(Some(found));
        }
    }
    Ok(None)
}

fn content_range(
    source: &str,
    outer: &Range<usize>,
    format: InlineFormat,
) -> Result<Option<Range<usize>>, EditRefusal> {
    let raw = source.get(outer.clone()).ok_or(EditRefusal::InvalidPlan)?;
    let (left, right) = match format {
        InlineFormat::Strong => (2, 2),
        InlineFormat::Emphasis => (1, 1),
        InlineFormat::Strike => {
            let count = raw.bytes().take_while(|byte| *byte == b'~').count();
            (count, count)
        }
        InlineFormat::Code => {
            let ticks = raw.bytes().take_while(|byte| *byte == b'`').count();
            let Some(body) = raw.get(ticks..raw.len().saturating_sub(ticks)) else {
                return Ok(None);
            };
            let padding = usize::from(
                body.starts_with(' ')
                    && body.ends_with(' ')
                    && !body.chars().all(|character| character == ' '),
            );
            (ticks + padding, ticks + padding)
        }
        InlineFormat::Link => {
            if raw.starts_with('<') && raw.ends_with('>') {
                (1, 1)
            } else {
                let Some(close) = label_end(raw) else {
                    return Ok(None);
                };
                (1, raw.len() - close)
            }
        }
    };
    let start = outer
        .start
        .checked_add(left)
        .ok_or(EditRefusal::LengthOverflow)?;
    let end = outer
        .end
        .checked_sub(right)
        .ok_or(EditRefusal::InvalidPlan)?;
    if start > end || left == 0 || right == 0 {
        return Ok(None);
    }
    Ok(Some(start..end))
}

fn label_end(raw: &str) -> Option<usize> {
    if !raw.starts_with('[') {
        return None;
    }
    let mut depth = 0usize;
    let mut escaped = false;
    for (offset, character) in raw.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' => escaped = true,
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn delimiters(content: &str, format: InlineFormat) -> Result<(String, String), EditRefusal> {
    if format == InlineFormat::Link {
        return Ok((owned("[")?, owned("]()")?));
    }
    if format != InlineFormat::Code {
        return Ok((owned(format.marker())?, owned(format.marker())?));
    }
    let mut longest = 0usize;
    let mut current = 0usize;
    for byte in content.bytes() {
        if byte == b'`' {
            current += 1;
            longest = longest.max(current);
        } else {
            current = 0;
        }
    }
    let count = longest.checked_add(1).ok_or(EditRefusal::LengthOverflow)?;
    let padding = !content.is_empty()
        && (content.starts_with('`')
            || content.ends_with('`')
            || (content.starts_with(' ')
                && content.ends_with(' ')
                && !content.chars().all(|character| character == ' ')));
    let capacity = count
        .checked_add(usize::from(padding))
        .ok_or(EditRefusal::LengthOverflow)?;
    let mut left = String::new();
    left.try_reserve_exact(capacity)
        .map_err(|_| EditRefusal::Allocation)?;
    for _ in 0..count {
        left.push('`');
    }
    let mut right = owned(&left)?;
    if padding {
        left.push(' ');
        right.insert(0, ' ');
    }
    Ok((left, right))
}

fn owned(value: &str) -> Result<String, EditRefusal> {
    let mut result = String::new();
    result
        .try_reserve_exact(value.len())
        .map_err(|_| EditRefusal::Allocation)?;
    result.push_str(value);
    Ok(result)
}

pub(crate) fn verify(
    contract: &InlineContract,
    before: &Parsed,
    after: &Parsed,
    budget: aline_buffer::TextBudget,
) -> Result<(), EditRefusal> {
    if contract.action == InlineAction::Pair {
        // 成对标记在尚无正文时是输入源码；其显示语义在后续输入后才确定。
        // 计划只插入一次已冻结的标记对，原片段和保护许可仍由共同协议复验。
        return Ok(());
    }
    if before.projection().drawn_text() != after.projection().drawn_text() {
        return Err(failed(crate::refusal::RereadFailure::VisibleTextChanged));
    }
    if before.blocks().len() != after.blocks().len()
        || !before
            .blocks()
            .iter()
            .zip(after.blocks())
            .all(|(left, right)| left.kind() == right.kind())
    {
        return Err(failed(crate::refusal::RereadFailure::BlockStructureChanged));
    }
    crate::inline_semantics::verify(
        before,
        after,
        contract.selected.clone(),
        contract.target,
        contract.format,
        contract.action,
    )?;
    if hidden_except(before, &contract.before_syntax, budget)?
        != hidden_except(after, &contract.after_syntax, budget)?
    {
        return Err(failed(crate::refusal::RereadFailure::HiddenSourceChanged));
    }
    Ok(())
}

fn failed(reason: crate::refusal::RereadFailure) -> EditRefusal {
    EditRefusal::RereadFailed {
        reason,
        cause: None,
    }
}

fn hidden_except(
    parsed: &Parsed,
    excluded: &[Range<usize>; 2],
    budget: aline_buffer::TextBudget,
) -> Result<String, EditRefusal> {
    let mut output = String::new();
    for segment in parsed.projection().segments() {
        if segment.precision() != aline_markdown::MappingPrecision::Hidden {
            continue;
        }
        let range = segment.source_range().byte_range();
        let raw = parsed
            .snapshot()
            .slice(segment.source_range(), budget)
            .map_err(crate::errors::map_buffer_error)?;
        if raw.chars().all(char::is_whitespace) {
            continue;
        }
        let mut kept = String::new();
        kept.try_reserve_exact(raw.len())
            .map_err(|_| EditRefusal::Allocation)?;
        for (offset, character) in raw.char_indices() {
            let src_off = range
                .start
                .checked_add(offset)
                .ok_or(EditRefusal::LengthOverflow)?;
            if !excluded.iter().any(|syntax| syntax.contains(&src_off)) {
                kept.push(character);
            }
        }
        if !kept.chars().all(char::is_whitespace) {
            output
                .try_reserve(kept.len())
                .map_err(|_| EditRefusal::Allocation)?;
            output.push_str(&kept);
        }
    }
    Ok(output)
}
