//! 组 7：拒绝必须保持完整调用前状态，纯计划不得触碰真实缓冲。

use std::error::Error;
use std::io;
use std::ops::Range;

use aline_buffer::{
    ApplyOutcome, CheckedBatch, DocumentRevision, HistoryBudget, LineIndexBudget, OperationSource,
    PatchBudget, SelectionSpec, SrcSelection, StringBuffer, TextBudget,
};
use aline_edit::{
    EditBudget, EditCommand, EditContext, EditMode, EditRefusal, InlineFormat, NoChangeReason,
    PlanOutcome, SourceEdit, plan_change, validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, Parsed, parse};

const TEXT_LIMIT: usize = 1 << 20;
const PATCH_LIMIT: usize = 64;

type TestResult = Result<(), Box<dyn Error>>;
type Patch = (Range<usize>, String);

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

fn budget() -> EditBudget {
    EditBudget::new(
        PatchBudget::new(PATCH_LIMIT, TEXT_LIMIT, TEXT_LIMIT),
        TextBudget::new(TEXT_LIMIT),
        ParseBudget::m0(),
    )
}

fn make_buffer(source: &str, anchor: usize, head: usize) -> Result<StringBuffer, Box<dyn Error>> {
    Ok(StringBuffer::new(
        source.to_owned(),
        SelectionSpec::new(anchor, head),
        TextBudget::new(TEXT_LIMIT),
        LineIndexBudget::new(4096),
        HistoryBudget::new(128, TEXT_LIMIT),
    )?)
}

fn parse_buffer(buffer: &StringBuffer) -> Result<Parsed, Box<dyn Error>> {
    Ok(parse(
        buffer.snapshot(),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )?)
}

/// 直接解析一段候选文本，用于比较块结构；不构造可写 Buffer。
fn parse_text_for_candidate(text: &str) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = aline_buffer::TextSnapshot::new(text.to_owned(), TextBudget::new(TEXT_LIMIT))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

fn replace(range: Range<usize>, expected: &str, replacement: &str) -> EditCommand {
    EditCommand::replace(vec![SourceEdit::new(
        range,
        expected.to_owned(),
        replacement.to_owned(),
    )])
}

fn apply(
    buffer: &mut StringBuffer,
    patches: Vec<Patch>,
    target: SelectionSpec,
    source: OperationSource,
    now_millis: u64,
) -> Result<(), Box<dyn Error>> {
    let batch = CheckedBatch::new(
        buffer.snapshot(),
        patches,
        PatchBudget::new(PATCH_LIMIT, TEXT_LIMIT, TEXT_LIMIT),
    )?;
    let outcome = buffer.apply(
        &batch,
        target,
        source,
        now_millis,
        TextBudget::new(TEXT_LIMIT),
    )?;
    if !matches!(outcome, ApplyOutcome::Changed(_)) {
        return Err(invalid("expected Changed apply outcome"));
    }
    Ok(())
}

fn text_of(buffer: &StringBuffer) -> Result<String, Box<dyn Error>> {
    Ok(buffer.text(TextBudget::new(TEXT_LIMIT))?.into_owned())
}

#[derive(Debug, PartialEq)]
struct State {
    text: String,
    anchor: usize,
    head: usize,
    revision: DocumentRevision,
    undo_depth: usize,
    redo_depth: usize,
    nodes: usize,
}

fn capture(buffer: &StringBuffer) -> Result<State, Box<dyn Error>> {
    Ok(State {
        text: text_of(buffer)?,
        anchor: buffer.selection().anchor().byte_offset(),
        head: buffer.selection().head().byte_offset(),
        revision: buffer.revision(),
        undo_depth: buffer.undo_depth(),
        redo_depth: buffer.redo_depth(),
        nodes: buffer.history_node_count(),
    })
}

fn assert_refusal<T>(result: Result<T, EditRefusal>, expected: &str) -> TestResult {
    match result {
        Ok(_) => Err(invalid(format!("expected refusal {expected}, got success"))),
        Err(refusal) => {
            assert_eq!(refusal.as_str(), expected);
            Ok(())
        }
    }
}

fn assert_reread_failure<T>(result: Result<T, EditRefusal>, expected: &str) -> TestResult {
    match result {
        Ok(_) => Err(invalid(format!(
            "expected RereadFailed/{expected}, got success"
        ))),
        Err(EditRefusal::RereadFailed { reason, .. }) => {
            assert_eq!(reason.as_str(), expected);
            Ok(())
        }
        Err(other) => Err(invalid(format!(
            "expected RereadFailed/{expected}, got {}",
            other.as_str()
        ))),
    }
}

/// 先真实写入一个节点再 undo，留下一个可重做的 redo 分支；
/// undo 会把选区恢复为写入前的初始选区。
fn seed_redo_branch(
    buffer: &mut StringBuffer,
    after_selection: SelectionSpec,
) -> Result<(), Box<dyn Error>> {
    let end = buffer.len_bytes();
    apply(
        buffer,
        vec![(end..end, "Q".to_owned())],
        after_selection,
        OperationSource::Replace,
        0,
    )?;
    buffer.undo(TextBudget::new(TEXT_LIMIT))?;
    Ok(())
}

#[test]
fn refusal_keeps_pending_typing_group_open() -> TestResult {
    let mut buffer = make_buffer("ab\n", 1, 1)?;
    apply(
        &mut buffer,
        vec![(1..1, "x".to_owned())],
        SelectionSpec::new(2, 2),
        OperationSource::Typing,
        1_000,
    )?;
    apply(
        &mut buffer,
        vec![(2..2, "y".to_owned())],
        SelectionSpec::new(3, 3),
        OperationSource::Typing,
        1_100,
    )?;
    assert_eq!(buffer.history_node_count(), 1);
    assert_eq!(text_of(&buffer)?.as_str(), "axyb\n");

    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Source).with_pending_input(true);
    let before = capture(&buffer)?;
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(0..0, "", "q"),
        budget(),
    );
    assert_refusal(result, "PendingInput")?;
    assert_eq!(capture(&buffer)?, before);

    // 拒绝后待定组仍可继续吸收相邻输入；若曾被封口，这里会新增第 2 个节点。
    apply(
        &mut buffer,
        vec![(3..3, "z".to_owned())],
        SelectionSpec::new(4, 4),
        OperationSource::Typing,
        1_200,
    )?;
    assert_eq!(buffer.history_node_count(), 1);
    assert_eq!(text_of(&buffer)?.as_str(), "axyzb\n");
    buffer.undo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(text_of(&buffer)?.as_str(), "ab\n");
    assert_eq!(buffer.undo_depth(), 0);
    Ok(())
}

#[test]
fn refusal_keeps_redo_branch_and_history_intact() -> TestResult {
    let mut buffer = make_buffer("alpha\n", 5, 5)?;
    apply(
        &mut buffer,
        vec![(0..5, "one".to_owned())],
        SelectionSpec::new(3, 3),
        OperationSource::Replace,
        0,
    )?;
    apply(
        &mut buffer,
        vec![(0..3, "two".to_owned())],
        SelectionSpec::new(3, 3),
        OperationSource::Replace,
        0,
    )?;
    buffer.undo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(text_of(&buffer)?.as_str(), "one\n");
    assert_eq!(buffer.redo_depth(), 1);
    assert_eq!(buffer.history_node_count(), 2);

    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Source);
    let before = capture(&buffer)?;
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(0..3, "zzz", "q"),
        budget(),
    );
    assert_refusal(result, "ExpectedFragmentMismatch")?;
    assert_eq!(capture(&buffer)?, before);

    buffer.redo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(text_of(&buffer)?.as_str(), "two\n");
    assert_eq!(buffer.redo_depth(), 0);
    Ok(())
}

#[test]
fn refusal_keeps_composition_state_untouched() -> TestResult {
    let mut buffer = make_buffer("alpha beta\n", 0, 0)?;
    apply(
        &mut buffer,
        vec![(0..5, "one".to_owned())],
        SelectionSpec::new(3, 3),
        OperationSource::Replace,
        0,
    )?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Source).with_composition_active(true);
    let before = capture(&buffer)?;
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(0..3, "one", "two"),
        budget(),
    );
    assert_refusal(result, "CompositionActive")?;
    assert_eq!(capture(&buffer)?, before);
    Ok(())
}

#[test]
fn stale_plan_wrong_version_does_not_touch_buffer() -> TestResult {
    let mut buffer = make_buffer("alpha beta\n", 6, 10)?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Source);
    let plan = match plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(6..10, "beta", "gamma"),
        budget(),
    )? {
        PlanOutcome::Changed(plan) => plan,
        PlanOutcome::Unchanged(reason) => {
            return Err(invalid(format!("expected plan, got {}", reason.as_str())));
        }
    };
    apply(
        &mut buffer,
        vec![(0..0, "X".to_owned())],
        SelectionSpec::new(1, 1),
        OperationSource::Replace,
        0,
    )?;
    let before = capture(&buffer)?;
    let result = validate_change(
        &plan,
        buffer.snapshot(),
        &parsed,
        buffer.selection(),
        &context,
    );
    assert_refusal(result, "WrongVersion")?;
    assert_eq!(capture(&buffer)?, before);
    Ok(())
}

#[test]
fn identity_replace_creates_no_history_step() -> TestResult {
    let buffer = make_buffer("alpha beta\n", 6, 10)?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Source);
    let before = capture(&buffer)?;
    let outcome = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(6..10, "beta", "beta"),
        budget(),
    )?;
    match outcome {
        PlanOutcome::Unchanged(reason) => assert_eq!(reason, NoChangeReason::ReplacementIdentical),
        PlanOutcome::Changed(_) => return Err(invalid("identity replace must not plan")),
    }
    assert_eq!(capture(&buffer)?, before);
    assert_eq!(buffer.history_node_count(), 0);
    assert_eq!(buffer.undo_depth(), 0);
    Ok(())
}

#[test]
fn visual_multi_edit_batch_refused_without_state_change() -> TestResult {
    let buffer = make_buffer("alpha beta\n", 0, 10)?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let command = EditCommand::replace(vec![
        SourceEdit::new(0..5, "alpha".to_owned(), "one".to_owned()),
        SourceEdit::new(6..10, "beta".to_owned(), "two".to_owned()),
    ]);
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &command,
        budget(),
    );
    assert_refusal(result, "UnsupportedVisualBatch")?;
    assert_eq!(capture(&buffer)?, before);
    Ok(())
}

#[test]
fn successful_plan_and_validation_leave_buffer_untouched() -> TestResult {
    let mut buffer = make_buffer("alpha beta\n", 6, 10)?;
    apply(
        &mut buffer,
        vec![(0..5, "one".to_owned())],
        SelectionSpec::new(3, 3),
        OperationSource::Replace,
        0,
    )?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let plan = match plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(4..8, "beta", "BETA"),
        budget(),
    )? {
        PlanOutcome::Changed(plan) => plan,
        PlanOutcome::Unchanged(reason) => {
            return Err(invalid(format!("expected plan, got {}", reason.as_str())));
        }
    };
    assert_eq!(capture(&buffer)?, before);
    let validated = validate_change(&plan, buffer.snapshot(), &parsed, &selection, &context)?;
    assert_eq!(capture(&buffer)?, before);
    assert_eq!(validated.group_reason().as_str(), "VisibleReplace");
    Ok(())
}

#[test]
fn hidden_fragment_internal_whitespace_change_is_refused() -> TestResult {
    // "[x](https://e \"a b\")\n"：链接标题内 16..17 是空格；
    // 隐藏片段原文含内部空白，删除该空格必须被拒绝（F2-2）。
    let buffer = make_buffer("[x](https://e \"a b\")\n", 1, 2)?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(16..17, " ", ""),
        budget(),
    );
    assert_reread_failure(result, "HiddenSourceChanged")?;
    assert_eq!(capture(&buffer)?, before);
    Ok(())
}

#[test]
fn visual_replace_range_is_command_authority() -> TestResult {
    // 选区是光标 0..0，命令给出的编辑范围是 6..10；
    // Visual Replace 以命令范围为准，不要求范围等于选区（F2-1）。
    let buffer = make_buffer("alpha beta\n", 0, 0)?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let plan = match plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(6..10, "beta", "BETA"),
        budget(),
    )? {
        PlanOutcome::Changed(plan) => plan,
        PlanOutcome::Unchanged(reason) => {
            return Err(invalid(format!("expected plan, got {}", reason.as_str())));
        }
    };
    assert_eq!(plan.group_reason().as_str(), "VisibleReplace");
    assert_eq!(plan.batch().patches(), &[(6..10, "BETA".to_owned())]);
    // "beta" 4 字节 -> "BETA" 4 字节；after 终点 = 6 + 4 = 10。
    assert_eq!(plan.target_selection(), SelectionSpec::new(10, 10));
    assert_eq!(plan.preview().drawn_text(), "alpha BETA");
    // 计划绑定的 before_selection 仍是调用方传入的光标 0..0。
    assert_eq!(plan.before_selection().anchor().byte_offset(), 0);
    assert_eq!(plan.before_selection().head().byte_offset(), 0);
    assert_eq!(capture(&buffer)?, before);
    Ok(())
}

#[test]
fn visual_replace_validates_bound_selection_only() -> TestResult {
    // 同一计划：validate 只比较“当前选区 == 计划绑定的 before_selection”，
    // 不比较命令范围与选区是否一致（F2-1）。
    let buffer = make_buffer("alpha beta\n", 0, 0)?;
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let plan = match plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(6..10, "beta", "BETA"),
        budget(),
    )? {
        PlanOutcome::Changed(plan) => plan,
        PlanOutcome::Unchanged(reason) => {
            return Err(invalid(format!("expected plan, got {}", reason.as_str())));
        }
    };

    let validated = validate_change(&plan, buffer.snapshot(), &parsed, &selection, &context)?;
    assert_eq!(validated.target_selection(), SelectionSpec::new(10, 10));
    assert_eq!(capture(&buffer)?, before);

    let other = SrcSelection::new(
        buffer.snapshot().src_offset(0)?,
        buffer.snapshot().src_offset(5)?,
    )?;
    let result = validate_change(&plan, buffer.snapshot(), &parsed, &other, &context);
    assert_refusal(result, "SelectionMismatch")?;
    assert_eq!(capture(&buffer)?, before);
    Ok(())
}

#[test]
fn budget_exceeded_refusal_keeps_buffer_state() -> TestResult {
    let mut buffer = make_buffer("alpha beta\n", 6, 10)?;
    seed_redo_branch(&mut buffer, SelectionSpec::new(6, 10))?;
    assert_eq!(buffer.redo_depth(), 1);
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Source);
    let before = capture(&buffer)?;
    let tiny = EditBudget::new(
        PatchBudget::new(0, TEXT_LIMIT, TEXT_LIMIT),
        TextBudget::new(TEXT_LIMIT),
        ParseBudget::m0(),
    );
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(6..10, "beta", "gamma"),
        tiny,
    );
    match result {
        Err(EditRefusal::BudgetExceeded { kind, .. }) => {
            assert_eq!(kind.as_str(), "PatchCount");
        }
        Ok(_) => return Err(invalid("expected BudgetExceeded, got success")),
        Err(other) => {
            return Err(invalid(format!(
                "expected BudgetExceeded, got {}",
                other.as_str()
            )));
        }
    }
    assert_eq!(capture(&buffer)?, before);
    buffer.redo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(text_of(&buffer)?.as_str(), "alpha beta\nQ");
    assert_eq!(buffer.redo_depth(), 0);
    Ok(())
}

#[test]
fn reread_failure_refusal_keeps_buffer_state() -> TestResult {
    let mut buffer = make_buffer("[x](https://e)\n", 1, 2)?;
    seed_redo_branch(&mut buffer, SelectionSpec::new(1, 2))?;
    assert_eq!(buffer.redo_depth(), 1);
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(0..14, "[x](https://e)", "[x]"),
        budget(),
    );
    assert_reread_failure(result, "HiddenSourceChanged")?;
    assert_eq!(capture(&buffer)?, before);
    buffer.redo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(text_of(&buffer)?.as_str(), "[x](https://e)\nQ");
    assert_eq!(buffer.redo_depth(), 0);
    Ok(())
}

#[test]
fn protected_range_refusal_keeps_buffer_state() -> TestResult {
    let mut buffer = make_buffer("before [^1] after\n\n[^1]: note\n", 0, 6)?;
    seed_redo_branch(&mut buffer, SelectionSpec::new(0, 6))?;
    assert_eq!(buffer.redo_depth(), 1);
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &replace(0..11, "before [^1]", "BEFORE"),
        budget(),
    );
    match result {
        Err(EditRefusal::ProtectedRange { kind, range }) => {
            assert_eq!(kind.as_str(), "FootnoteReference");
            assert_eq!(range, 7..11);
        }
        Ok(_) => return Err(invalid("expected ProtectedRange, got success")),
        Err(other) => {
            return Err(invalid(format!(
                "expected ProtectedRange, got {}",
                other.as_str()
            )));
        }
    }
    assert_eq!(capture(&buffer)?, before);
    buffer.redo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(
        text_of(&buffer)?.as_str(),
        "before [^1] after\n\n[^1]: note\nQ"
    );
    assert_eq!(buffer.redo_depth(), 0);
    Ok(())
}

#[test]
fn html_boundary_refusal_keeps_buffer_state() -> TestResult {
    let mut buffer = make_buffer("a <b>x</b> c\n", 5, 6)?;
    seed_redo_branch(&mut buffer, SelectionSpec::new(5, 6))?;
    assert_eq!(buffer.redo_depth(), 1);
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let result = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &EditCommand::toggle_inline_format(5..6, InlineFormat::Strong),
        budget(),
    );
    match result {
        Err(EditRefusal::HtmlBoundary { range }) => assert_eq!(range, 2..5),
        Ok(_) => return Err(invalid("expected HtmlBoundary, got success")),
        Err(other) => {
            return Err(invalid(format!(
                "expected HtmlBoundary, got {}",
                other.as_str()
            )));
        }
    }
    assert_eq!(capture(&buffer)?, before);
    buffer.redo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(text_of(&buffer)?.as_str(), "a <b>x</b> c\nQ");
    assert_eq!(buffer.redo_depth(), 0);
    Ok(())
}

#[test]
fn already_formatted_creates_no_history_step() -> TestResult {
    // alpha 0..5，空格 5，** 6..8，beta 8..12，** 12..14，空格 14，gamma 15..20，\n 20。
    let mut buffer = make_buffer("alpha **beta** gamma\n", 8, 12)?;
    seed_redo_branch(&mut buffer, SelectionSpec::new(8, 12))?;
    assert_eq!(buffer.history_node_count(), 1);
    assert_eq!(buffer.undo_depth(), 0);
    assert_eq!(buffer.redo_depth(), 1);
    let parsed = parse_buffer(&buffer)?;
    let selection = buffer.selection().clone();
    let context = EditContext::new(EditMode::Visual);
    let before = capture(&buffer)?;
    let outcome = plan_change(
        buffer.snapshot(),
        &parsed,
        &selection,
        &context,
        &EditCommand::toggle_inline_format(8..12, InlineFormat::Strong),
        budget(),
    )?;
    match outcome {
        PlanOutcome::Unchanged(reason) => assert_eq!(reason, NoChangeReason::AlreadyFormatted),
        PlanOutcome::Changed(_) => return Err(invalid("already formatted must not plan")),
    }
    assert_eq!(capture(&buffer)?, before);
    assert_eq!(buffer.history_node_count(), 1);
    assert_eq!(buffer.undo_depth(), 0);
    assert_eq!(buffer.redo_depth(), 1);
    buffer.redo(TextBudget::new(TEXT_LIMIT))?;
    assert_eq!(text_of(&buffer)?.as_str(), "alpha **beta** gamma\nQ");
    assert_eq!(buffer.redo_depth(), 0);
    Ok(())
}

#[test]
fn pure_whitespace_hidden_separator_is_accepted_limitation() -> TestResult {
    // 复核 N-F2-1：把“纯空白隐藏片段与片段边界不比较”这一 V-07 已接受限制
    // 钉成测试，并记录其块结构后果。绘制文本不变，但块种类会合并：
    //   "a\n\nb\n"   删 1..3（隐藏 "\n\n"）→ "ab\n"，[Paragraph, Paragraph] 变 [Paragraph]
    //   "- a\n\nb\n" 删 3..5（隐藏 "\n\n"）→ "- ab\n"，[List, Paragraph] 变 [List]
    // 这不是“可见文字被静默改写”的反例（前后绘制文本都是 "ab"），而是
    // reread.rs 的 Visible 契约不比较块种类所导致的已接受限制；
    // 若日后收紧为比较块结构，本测试应随之改为拒绝并更新 V-07 记录。
    for (source, range, expected_hidden, candidate) in [
        ("a\n\nb\n", 1..3, "\n\n", "ab\n"),
        ("- a\n\nb\n", 3..5, "\n\n", "- ab\n"),
    ] {
        let buffer = make_buffer(source, 0, 0)?;
        let parsed = parse_buffer(&buffer)?;
        let selection = buffer.selection().clone();
        let context = EditContext::new(EditMode::Visual);
        let before = capture(&buffer)?;
        let plan = match plan_change(
            buffer.snapshot(),
            &parsed,
            &selection,
            &context,
            &replace(range.clone(), expected_hidden, ""),
            budget(),
        )? {
            PlanOutcome::Changed(plan) => plan,
            PlanOutcome::Unchanged(reason) => {
                return Err(invalid(format!(
                    "expected accepted limitation to plan, got {}",
                    reason.as_str()
                )));
            }
        };
        // 前后绘制文本相同，说明改变只发生在隐藏空白。
        let before_drawn = parsed.projection().drawn_text().to_owned();
        assert_eq!(before_drawn.as_str(), "ab");
        assert_eq!(plan.preview().drawn_text(), "ab");
        assert_eq!(plan.preview().after_len(), candidate.len());
        // 计划只读，真实缓冲仍保持调用前状态。
        assert_eq!(capture(&buffer)?, before);
        // 块种类确实合并，这正是本限制未写进 Visible 契约的后果。
        let candidate_parsed = parse_text_for_candidate(candidate)?;
        assert_eq!(parsed.blocks().len(), 2);
        assert_eq!(candidate_parsed.blocks().len(), 1);
    }
    Ok(())
}
