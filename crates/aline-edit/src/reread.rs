//! 候选文本构造、重新解析与按命令语义复读；绝不写真实文档。

use std::ops::Range;

use aline_buffer::{Affinity, CheckedBatch, TextBudget, TextSnapshot};
use aline_markdown::{DiagnosticKind, MappingPrecision, ParseConfig, Parsed, parse};

use crate::budget::EditBudget;
use crate::errors::{map_buffer_error, map_candidate_error, map_mapping_error, map_parse_error};
use crate::refusal::{BudgetKind, EditRefusal, RereadFailure};

/// 计划在写前必须复验的语义契约。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RereadContract {
    /// 手动格式化保留绘制语义、块结构和受保护原片段。
    MarkdownFormat,
    /// 段落和列表的目标种类及邻接块范围。
    Block {
        start: usize,
        expected: Option<aline_markdown::BlockKind>,
        count: usize,
    },
    /// 列表层级变化只改变被移动项的父项，其余归属保持。
    ListMove(crate::list_commands::ListMove),
    /// 源码编辑：候选解析成功即可，允许改变隐藏语义。
    Source,
    /// 表格结构复读须保持操作声明的网格与对齐。
    Table(crate::table::TableExpectation),
    /// 所见替换：可见文本按替换拼接，隐藏语义必须保持。
    Visible,
    /// 可视化查找以源码范围为准，仍须保留隐藏语义。
    FindVisual,
    /// 已打开的可视化块编辑其原始源码，允许普通 Markdown 呈现变化。
    RawBlock,
    /// 已核对为顶层 HTML 块的源码输入，仍需复验原块身份和 MD-11。
    RawHtml { block: Range<usize> },
    /// 行内格式：可见文本和块结构保持，仅允许成对标记变化。
    Inline(crate::inline::InlineContract),
}

/// 已构造并重新解析的候选文本；仍只是不可变预览。
pub(crate) struct Candidate {
    pub(crate) snapshot: TextSnapshot,
    pub(crate) parsed: Parsed,
}

/// 在 before 文本上应用受检批次并重新解析候选；不触碰真实缓冲。
pub(crate) fn build_candidate(
    before: &TextSnapshot,
    config: ParseConfig,
    batch: &CheckedBatch,
    budget: EditBudget,
) -> Result<Candidate, EditRefusal> {
    let before_range = before
        .src_range(0..before.len_bytes())
        .map_err(map_buffer_error)?;
    let source = before
        .slice(&before_range, budget.text())
        .map_err(map_buffer_error)?;

    let mut output = String::new();
    output
        .try_reserve_exact(batch.output_len())
        .map_err(|_| EditRefusal::Allocation)?;
    let mut cursor = 0usize;
    for (range, replacement) in batch.patches() {
        let gap = source
            .get(cursor..range.start)
            .ok_or(EditRefusal::InvalidPlan)?;
        output.push_str(gap);
        output.push_str(replacement);
        cursor = range.end;
    }
    let tail = source.get(cursor..).ok_or(EditRefusal::InvalidPlan)?;
    output.push_str(tail);
    if output.len() != batch.output_len() {
        return Err(reread_failure(RereadFailure::CandidateLengthMismatch));
    }

    let snapshot = TextSnapshot::new(output, budget.text()).map_err(map_candidate_error)?;
    let parsed = parse(&snapshot, config, budget.parse()).map_err(map_parse_error)?;
    for diagnostic in parsed.diagnostics() {
        if diagnostic.kind() == DiagnosticKind::BudgetExceeded {
            return Err(EditRefusal::BudgetExceeded {
                kind: BudgetKind::Parse,
                needed: None,
                limit: None,
            });
        }
    }
    Ok(Candidate { snapshot, parsed })
}

/// 按契约比较 before 与候选解析结果；失败只返回拒绝。
pub(crate) fn verify(
    contract: &RereadContract,
    before: &Parsed,
    candidate: &Candidate,
    patches: &[(Range<usize>, String)],
    budget: EditBudget,
) -> Result<(), EditRefusal> {
    match contract {
        RereadContract::Block {
            start,
            expected,
            count,
        } => crate::block_commands::verify(before, candidate, patches, *start, *expected, *count),
        RereadContract::ListMove(expected) => {
            crate::list_commands::verify_move(expected, before, candidate, patches, budget)
        }
        RereadContract::MarkdownFormat => crate::markdown_format::verify(before, candidate, budget),
        RereadContract::Source => Ok(()),
        RereadContract::Table(expected) => crate::table::verify_table(expected, candidate, budget),
        RereadContract::RawBlock => reject_raw_fallback(candidate),
        RereadContract::RawHtml { block } => {
            for (range, _) in patches {
                crate::permissions::check_raw_html_permissions(before, block, range)?;
            }
            reject_raw_fallback(candidate)
        }
        RereadContract::FindVisual => {
            reject_raw_fallback(candidate)?;
            if semantic_hidden(before, budget.text())?
                != semantic_hidden(&candidate.parsed, budget.text())?
            {
                return Err(reread_failure(RereadFailure::HiddenSourceChanged));
            }
            Ok(())
        }
        RereadContract::Visible => {
            let before_hidden = semantic_hidden(before, budget.text())?;
            let after_hidden = semantic_hidden(&candidate.parsed, budget.text())?;
            if before_hidden != after_hidden {
                return Err(reread_failure(RereadFailure::HiddenSourceChanged));
            }
            let (range, replacement) = match patches {
                [(range, replacement)] => (range, replacement),
                _ => return Err(EditRefusal::InvalidPlan),
            };
            let expected = expected_visible(before, range, replacement)?;
            if candidate.parsed.projection().drawn_text() != expected {
                return Err(reread_failure(RereadFailure::VisibleTextChanged));
            }
            Ok(())
        }
        RereadContract::Inline(contract) => {
            reject_raw_fallback(candidate)?;
            crate::inline::verify(contract, before, &candidate.parsed, budget.text())
        }
    }
}

fn reject_raw_fallback(candidate: &Candidate) -> Result<(), EditRefusal> {
    if candidate
        .parsed
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.kind() == DiagnosticKind::RawFallback)
    {
        return Err(reread_failure(RereadFailure::Projection));
    }
    Ok(())
}

fn reread_failure(reason: RereadFailure) -> EditRefusal {
    EditRefusal::RereadFailed {
        reason,
        cause: None,
    }
}

/// 提取隐藏片段用于比较的原文：保留片段内部空白，只跳过纯空白片段。
///
/// 比较的是“非纯空白隐藏片段的原文（含内部空白）”。隐藏片段内部任何字符
/// （包括空格、制表符等）的变化都会使比较失败，例如链接标题 `"a b"` 中的
/// 空格不能被静默删除。
///
/// 以下两点**未**参与比较，作为 V-07 的已接受限制如实记录，不得据此声称
/// “隐藏片段已完整保留”：
/// 1. 纯空白隐藏片段不参与比较——可见编辑可能合法地产生或消费尾随空白并
///    使其落入隐藏片段，因此这类片段被整体跳过；
/// 2. 片段边界与位置不参与比较——只比较隐藏片段的有序文本序列，片段如何
///    切分或位于何处不影响比较结果。
fn semantic_hidden(parsed: &Parsed, budget: TextBudget) -> Result<Vec<String>, EditRefusal> {
    let mut fragments = Vec::new();
    for segment in parsed.projection().segments() {
        if segment.precision() != MappingPrecision::Hidden {
            continue;
        }
        let text = parsed
            .snapshot()
            .slice(segment.source_range(), budget)
            .map_err(map_buffer_error)?;
        if text.chars().all(char::is_whitespace) {
            continue;
        }
        let mut fragment = String::new();
        fragment
            .try_reserve_exact(text.len())
            .map_err(|_| EditRefusal::Allocation)?;
        fragment.push_str(text.as_ref());
        fragments
            .try_reserve(1)
            .map_err(|_| EditRefusal::Allocation)?;
        fragments.push(fragment);
    }
    Ok(fragments)
}

/// 按“before 绘制文本中被替换范围 + 字面替换文本”计算命令预期可见文本。
fn expected_visible(
    before: &Parsed,
    range: &Range<usize>,
    replacement: &str,
) -> Result<String, EditRefusal> {
    let start = before
        .snapshot()
        .src_offset(range.start)
        .map_err(map_buffer_error)?;
    let end = before
        .snapshot()
        .src_offset(range.end)
        .map_err(map_buffer_error)?;
    let start_drawn = before
        .projection()
        .drawn_at(&start, Affinity::Before)
        .map_err(map_mapping_error)?
        .into_value()
        .byte_offset();
    let end_drawn = before
        .projection()
        .drawn_at(&end, Affinity::After)
        .map_err(map_mapping_error)?
        .into_value()
        .byte_offset();
    if start_drawn > end_drawn {
        return Err(reread_failure(RereadFailure::Projection));
    }
    let drawn = before.projection().drawn_text();
    let prefix = drawn.get(..start_drawn).ok_or(EditRefusal::InvalidPlan)?;
    let suffix = drawn.get(end_drawn..).ok_or(EditRefusal::InvalidPlan)?;
    let capacity = prefix
        .len()
        .checked_add(replacement.len())
        .and_then(|value| value.checked_add(suffix.len()))
        .ok_or(EditRefusal::LengthOverflow)?;
    let mut expected = String::new();
    expected
        .try_reserve_exact(capacity)
        .map_err(|_| EditRefusal::Allocation)?;
    expected.push_str(prefix);
    expected.push_str(replacement);
    expected.push_str(suffix);
    Ok(expected)
}
