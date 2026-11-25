//! 段落结构转换的纯计划，消费当前解析和源码选区，返回原片段补丁。
//! 源码与历史仍由 DocCore 持有。这里只移除已识别的块标记，不从展示树重建正文。

use std::ops::Range;

use aline_buffer::{SelectionSpec, SrcOffset, SrcSelection};
use aline_markdown::{BlockKind, Parsed, PresentationKind};

use crate::budget::EditBudget;
use crate::command::{EditContext, GroupReason, SourceEdit};
use crate::errors::map_buffer_error;
use crate::plan::{PlanOutcome, StructuralChange, plan_structural};
use crate::refusal::{EditRefusal, RereadFailure};
use crate::reread::{Candidate, RereadContract};

/// 段落转换的目标；标题级别在计划入口校验为 1～6。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParagraphStyle {
    /// 普通正文。
    Paragraph,
    /// ATX 标题，级别为 1～6。
    Heading(u8),
    /// 普通引用。
    Quote,
    /// 有序列表，新增标记从 `1.` 开始。
    OrderedList,
    /// 无序列表，新增标记为 `-`。
    UnorderedList,
    /// 未完成任务列表。
    TaskList,
    /// 围栏代码块。
    CodeBlock,
    /// 独立数学围栏。
    MathBlock,
    /// 在保留正文的前提下插入分隔线。
    ThematicBreak,
}

/// 结构命令；命令只规划，不封口输入组或修改历史。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockCommand {
    /// 将当前明确块转换为目标段落种类。
    Convert(ParagraphStyle),
    /// 列表续行或空项退出一级。
    ListEnter,
    /// 将当前项及子项缩进到前一同级项下。
    IndentList,
    /// 将当前项及子项退回实际父级。
    OutdentList,
    /// 只切换当前任务框的完成字符。
    ToggleTask,
}

/// 在当前快照上规划段落、列表或任务标记命令。
///
/// 跨块、无法可靠识别的容器、保护区和非法层级返回枚举拒绝。
/// 生成计划后仍须通过 `validate_change` 执行写前复验。
pub fn plan_block_command(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    command: BlockCommand,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    crate::plan::precheck(context)?;
    parsed
        .snapshot()
        .validate_selection(selection)
        .map_err(map_buffer_error)?;
    let BlockCommand::Convert(style) = command else {
        return crate::list_commands::plan(parsed, selection, context, command, budget);
    };
    let target = target_block(parsed, selection, budget)?;
    let source = parsed
        .snapshot()
        .slice(
            &parsed
                .snapshot()
                .src_range(target.clone())
                .map_err(map_buffer_error)?,
            budget.text(),
        )
        .map_err(map_buffer_error)?;
    let kind = parsed
        .blocks()
        .iter()
        .find(|block| block.range().byte_range().end == target.end)
        .map_or(BlockKind::Paragraph, |block| block.kind());
    if style == ParagraphStyle::CodeBlock
        && kind == BlockKind::CodeBlock
        && parsed.presentation().iter().any(|node| {
            node.source_range().byte_range().start == target.start
                && matches!(
                    node.kind(),
                    PresentationKind::CodeBlock { fenced: true, .. }
                )
        })
    {
        return Ok(PlanOutcome::Unchanged(
            crate::plan::NoChangeReason::ReplacementIdentical,
        ));
    }
    let body = body_lines(&source, kind)?;
    let eol = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut output = String::new();
    let capacity = source
        .len()
        .checked_mul(4)
        .and_then(|len| len.checked_add(32))
        .ok_or(EditRefusal::LengthOverflow)?;
    output
        .try_reserve(capacity.min(budget.text().max_bytes()))
        .map_err(|_| EditRefusal::Allocation)?;
    let mut retained = Vec::new();
    let expected = style_kind(style)?;
    match style {
        ParagraphStyle::Heading(level) => {
            let [line] = body.as_slice() else {
                return Err(EditRefusal::UnsupportedVisualStructure);
            };
            let prefix = format!("{} ", "#".repeat(usize::from(level)));
            append_line(&mut output, line, &prefix, &mut retained)?;
            if heading_closing_hashes(line.text).is_some() {
                // 追加明确的闭合标记，原有行尾井号便继续作为标题正文。
                output.insert_str(output.len() - line.eol.len(), " #");
            }
        }
        ParagraphStyle::CodeBlock => {
            let fence = if body.iter().any(|line| line.text.contains("```")) {
                "~~~~"
            } else {
                "```"
            };
            if body
                .iter()
                .any(|line| line.text.trim_start().starts_with(fence))
            {
                return Err(EditRefusal::UnsupportedVisualStructure);
            }
            output.push_str(fence);
            output.push_str(eol);
            append_body(&mut output, &body, "", &mut retained)?;
            if !output.ends_with('\n') {
                output.push_str(eol);
            }
            output.push_str(fence);
        }
        ParagraphStyle::MathBlock => {
            if body.iter().any(|line| line.text.trim() == "$$") {
                return Err(EditRefusal::UnsupportedVisualStructure);
            }
            output.push_str("$$");
            output.push_str(eol);
            append_body(&mut output, &body, "", &mut retained)?;
            if !output.ends_with('\n') {
                output.push_str(eol);
            }
            output.push_str("$$");
        }
        ParagraphStyle::ThematicBreak => {
            if kind == BlockKind::ThematicBreak {
                return Ok(PlanOutcome::Unchanged(
                    crate::plan::NoChangeReason::ReplacementIdentical,
                ));
            }
            output.push_str("---");
            if body.iter().any(|line| !line.text.is_empty()) {
                output.push_str(eol);
                output.push_str(eol);
                append_body(&mut output, &body, "", &mut retained)?;
            }
        }
        ParagraphStyle::OrderedList => {
            for (number, line) in body.iter().enumerate() {
                let ordinal = number.checked_add(1).ok_or(EditRefusal::LengthOverflow)?;
                let prefix = format!("{ordinal}. ");
                append_line(&mut output, line, &prefix, &mut retained)?;
            }
        }
        _ => {
            let prefix = match style {
                ParagraphStyle::Paragraph => String::new(),
                ParagraphStyle::Quote => "> ".into(),
                ParagraphStyle::UnorderedList => "- ".into(),
                ParagraphStyle::TaskList => "- [ ] ".into(),
                _ => return Err(EditRefusal::InvalidPlan),
            };
            append_body(&mut output, &body, &prefix, &mut retained)?;
        }
    }
    let anchor = map_endpoint(
        selection.anchor().byte_offset(),
        target.start,
        &retained,
        output.len(),
    )?;
    let head = map_endpoint(
        selection.head().byte_offset(),
        target.start,
        &retained,
        output.len(),
    )?;
    let count = if style == ParagraphStyle::ThematicBreak
        && retained.iter().any(|entry| !entry.source.is_empty())
    {
        2
    } else if output.is_empty() {
        0
    } else {
        1
    };
    plan_structural(
        parsed,
        selection,
        context,
        StructuralChange {
            edits: vec![SourceEdit::new(target.clone(), source.as_ref(), output)],
            target: SelectionSpec::new(anchor, head),
            reason: GroupReason::BlockCommand,
            contract: RereadContract::Block {
                start: target.start,
                expected: if count == 0 { None } else { Some(expected) },
                count,
            },
        },
        budget,
    )
}

/// 规划点击目标任务框的单字符切换，历史绑定仍使用调用方当前选区。
///
/// 目标必须属于当前快照中的实际任务标记，过期身份和非任务文本拒绝。
pub fn plan_task_toggle(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    target: &SrcOffset,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    crate::list_commands::plan_task_toggle(parsed, selection, context, target, budget)
}

pub(crate) fn target_block(
    parsed: &Parsed,
    selection: &SrcSelection,
    budget: EditBudget,
) -> Result<Range<usize>, EditRefusal> {
    let range = selection.range().byte_range();
    for block in parsed.blocks() {
        let mut extent = block.range().byte_range();
        if extent.end < range.end {
            continue;
        }
        if extent.start > range.start
            && (block.kind() != BlockKind::CodeBlock || extent.start - range.start > 4)
        {
            break;
        }
        if block.kind() == BlockKind::CodeBlock {
            let prefix = parsed
                .snapshot()
                .slice(
                    &parsed
                        .snapshot()
                        .src_range(0..extent.start)
                        .map_err(map_buffer_error)?,
                    budget.text(),
                )
                .map_err(map_buffer_error)?;
            let line_start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
            let indentation = prefix.get(line_start..).ok_or(EditRefusal::InvalidPlan)?;
            if indentation == "    " || indentation == "\t" {
                extent.start = line_start;
            }
        }
        if extent.start <= range.start && range.end <= extent.end {
            return Ok(extent);
        }
    }
    if parsed.snapshot().len_bytes() == 0 && range.is_empty() {
        return Ok(0..0);
    }
    Err(EditRefusal::UnsupportedVisualStructure)
}

pub(crate) struct SourceLine<'a> {
    pub(crate) text: &'a str,
    pub(crate) eol: &'a str,
    pub(crate) start: usize,
}

pub(crate) fn lines(source: &str) -> Vec<SourceLine<'_>> {
    let mut result = Vec::new();
    let mut start = 0;
    for part in source.split_inclusive('\n') {
        let without_lf = part.strip_suffix('\n');
        let (text, eol) = match without_lf {
            Some(line) => match line.strip_suffix('\r') {
                Some(text) => (text, "\r\n"),
                None => (line, "\n"),
            },
            None => (part, ""),
        };
        result.push(SourceLine { text, eol, start });
        start += part.len();
    }
    if result.is_empty() {
        result.push(SourceLine {
            text: "",
            eol: "",
            start: 0,
        });
    }
    result
}

fn body_lines(source: &str, kind: BlockKind) -> Result<Vec<SourceLine<'_>>, EditRefusal> {
    let mut body = lines(source);
    match kind {
        BlockKind::Paragraph => {}
        BlockKind::Heading => {
            if body.len() == 2 && body[1].text.trim().chars().all(|ch| ch == '=' || ch == '-') {
                body.pop();
                if let Some(first) = body.first_mut() {
                    first.eol = "";
                }
            } else {
                let first = body
                    .first_mut()
                    .ok_or(EditRefusal::UnsupportedVisualStructure)?;
                let trimmed = first.text.trim_start_matches(' ');
                let leading = first.text.len() - trimmed.len();
                let marks = trimmed.bytes().take_while(|byte| *byte == b'#').count();
                if marks == 0 || marks > 6 {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                }
                let rest = trimmed.get(marks..).ok_or(EditRefusal::InvalidPlan)?;
                let content = rest.trim_start_matches([' ', '\t']);
                first.start += leading + marks + rest.len() - content.len();
                first.text = content;
                if let Some(prefix) = heading_closing_hashes(content) {
                    first.text = prefix;
                }
            }
        }
        BlockKind::BlockQuote => {
            for line in &mut body {
                let prefix = if line.text.starts_with("> ") {
                    2
                } else if line.text.starts_with('>') {
                    1
                } else {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                };
                line.text = line.text.get(prefix..).ok_or(EditRefusal::InvalidPlan)?;
                line.start += prefix;
                if line.text.starts_with('>') {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                }
            }
        }
        BlockKind::List => {
            for line in &mut body {
                let prefix = crate::list_commands::prefix(line.text)?;
                if prefix.indent != 0 {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                }
                line.text = line
                    .text
                    .get(prefix.content..)
                    .ok_or(EditRefusal::InvalidPlan)?;
                line.start += prefix.content;
            }
        }
        BlockKind::CodeBlock => {
            let first = body
                .first()
                .ok_or(EditRefusal::UnsupportedVisualStructure)?
                .text;
            let fence_byte = first.as_bytes().first().copied();
            if matches!(fence_byte, Some(b'`' | b'~')) {
                let byte = fence_byte.ok_or(EditRefusal::InvalidPlan)?;
                let width = first.bytes().take_while(|item| *item == byte).count();
                if width < 3 || body.len() < 2 {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                }
                let last = body.last().ok_or(EditRefusal::InvalidPlan)?.text.trim();
                if last.len() < width || !last.bytes().all(|item| item == byte) {
                    return Err(EditRefusal::UnsupportedVisualStructure);
                }
                body.remove(0);
                body.pop();
                if let Some(last) = body.last_mut() {
                    last.eol = "";
                }
            } else {
                for line in &mut body {
                    let prefix = if line.text.starts_with("    ") {
                        4
                    } else if line.text.starts_with('\t') {
                        1
                    } else if line.text.is_empty() {
                        0
                    } else {
                        return Err(EditRefusal::UnsupportedVisualStructure);
                    };
                    line.text = line.text.get(prefix..).ok_or(EditRefusal::InvalidPlan)?;
                    line.start += prefix;
                }
            }
        }
        BlockKind::Math => {
            if body.len() < 2
                || body.first().map(|line| line.text.trim()) != Some("$$")
                || body.last().map(|line| line.text.trim()) != Some("$$")
            {
                return Err(EditRefusal::UnsupportedVisualStructure);
            }
            body.remove(0);
            body.pop();
            if let Some(last) = body.last_mut() {
                last.eol = "";
            }
        }
        BlockKind::ThematicBreak => body.clear(),
        _ => return Err(EditRefusal::UnsupportedVisualStructure),
    }
    Ok(body)
}

fn heading_closing_hashes(text: &str) -> Option<&str> {
    let trimmed = text.trim_end_matches([' ', '\t']);
    let prefix = trimmed.trim_end_matches('#');
    (prefix.len() != trimmed.len() && (prefix.ends_with([' ', '\t']) || prefix.is_empty()))
        .then(|| prefix.trim_end_matches([' ', '\t']))
}

fn style_kind(style: ParagraphStyle) -> Result<BlockKind, EditRefusal> {
    Ok(match style {
        ParagraphStyle::Paragraph => BlockKind::Paragraph,
        ParagraphStyle::Heading(1..=6) => BlockKind::Heading,
        ParagraphStyle::Heading(_) => return Err(EditRefusal::UnsupportedVisualStructure),
        ParagraphStyle::Quote => BlockKind::BlockQuote,
        ParagraphStyle::OrderedList | ParagraphStyle::UnorderedList | ParagraphStyle::TaskList => {
            BlockKind::List
        }
        ParagraphStyle::CodeBlock => BlockKind::CodeBlock,
        ParagraphStyle::MathBlock => BlockKind::Math,
        ParagraphStyle::ThematicBreak => BlockKind::ThematicBreak,
    })
}

struct Retained {
    source: Range<usize>,
    output_start: usize,
}

fn append_line(
    output: &mut String,
    line: &SourceLine<'_>,
    prefix: &str,
    retained: &mut Vec<Retained>,
) -> Result<(), EditRefusal> {
    output.push_str(prefix);
    retained.push(Retained {
        source: line.start
            ..line
                .start
                .checked_add(line.text.len())
                .ok_or(EditRefusal::LengthOverflow)?,
        output_start: output.len(),
    });
    output.push_str(line.text);
    output.push_str(line.eol);
    Ok(())
}

fn append_body(
    output: &mut String,
    body: &[SourceLine<'_>],
    prefix: &str,
    retained: &mut Vec<Retained>,
) -> Result<(), EditRefusal> {
    for line in body {
        append_line(output, line, prefix, retained)?;
    }
    Ok(())
}

fn map_endpoint(
    src_off: usize,
    before_start: usize,
    retained: &[Retained],
    output_len: usize,
) -> Result<usize, EditRefusal> {
    let local = src_off
        .checked_sub(before_start)
        .ok_or(EditRefusal::InvalidPlan)?;
    let mapped = retained
        .iter()
        .find(|entry| local <= entry.source.end)
        .map_or(output_len, |entry| {
            entry.output_start + local.saturating_sub(entry.source.start)
        });
    before_start
        .checked_add(mapped)
        .ok_or(EditRefusal::LengthOverflow)
}

pub(crate) fn verify(
    before: &Parsed,
    candidate: &Candidate,
    patches: &[(Range<usize>, String)],
    start: usize,
    expected: Option<BlockKind>,
    count: usize,
) -> Result<(), EditRefusal> {
    let fail = || EditRefusal::RereadFailed {
        reason: RereadFailure::BlockStructureChanged,
        cause: None,
    };
    let [(range, replacement)] = patches else {
        return Err(EditRefusal::InvalidPlan);
    };
    let before_target = before.blocks().iter().find(|block| {
        let original = block.range().byte_range();
        original.start == start || range.start <= original.start && original.end <= range.end
    });
    let mut unchanged = 0usize;
    let mut after_blocks = candidate.parsed.blocks().iter().peekable();
    for block in before
        .blocks()
        .iter()
        .filter(|block| Some(*block) != before_target)
    {
        let original = block.range().byte_range();
        let shifted = if original.end <= range.start {
            original.clone()
        } else if original.start >= range.end {
            let new_start = original
                .start
                .checked_sub(range.len())
                .and_then(|value| value.checked_add(replacement.len()))
                .ok_or(EditRefusal::LengthOverflow)?;
            let new_end = new_start
                .checked_add(original.len())
                .ok_or(EditRefusal::LengthOverflow)?;
            new_start..new_end
        } else {
            return Err(fail());
        };
        while after_blocks
            .peek()
            .is_some_and(|other| other.range().byte_range().start < shifted.start)
        {
            after_blocks.next();
        }
        if !after_blocks.next().is_some_and(|other| {
            other.kind() == block.kind() && other.range().byte_range() == shifted
        }) {
            return Err(fail());
        }
        unchanged += 1;
    }
    if candidate.parsed.blocks().len()
        != unchanged
            .checked_add(count)
            .ok_or(EditRefusal::LengthOverflow)?
    {
        return Err(fail());
    }
    if let Some(kind) = expected
        && !candidate
            .parsed
            .blocks()
            .iter()
            .any(|block| block.kind() == kind && block.range().byte_range().start == start)
    {
        return Err(fail());
    }
    if expected == Some(BlockKind::Heading)
        && let Some(original) = before_target
        && matches!(original.kind(), BlockKind::Paragraph | BlockKind::Heading)
        && heading_text(before, original.range().byte_range().start)?
            != heading_text(&candidate.parsed, start)?
    {
        return Err(EditRefusal::RereadFailed {
            reason: RereadFailure::VisibleTextChanged,
            cause: None,
        });
    }
    Ok(())
}

fn heading_text(parsed: &Parsed, start: usize) -> Result<&str, EditRefusal> {
    let range = parsed
        .presentation()
        .iter()
        .find(|node| {
            node.source_range().byte_range().start == start
                && matches!(
                    node.kind(),
                    PresentationKind::Paragraph | PresentationKind::Heading { .. }
                )
        })
        .ok_or(EditRefusal::InvalidPlan)?
        .drawn_range();
    parsed
        .projection()
        .drawn_text()
        .get(range.byte_range())
        .ok_or(EditRefusal::InvalidPlan)
}
