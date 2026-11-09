//! W3 edit：随机命令与选区仅使用现有纯计划/复验 API；发布由测试本地 Buffer 拥有。
//! 不覆盖 DocCore 只读能力或真实 IME，拒绝前后核对 bytes/身份/选区/历史。

mod support;
use support as fuzz;

use aline_buffer::{
    HistoryBudget, LineIndexBudget, OperationSource, PatchBudget, SelectionSpec, SrcSelection,
    StringBuffer, TextBudget, TextSnapshot,
};
use aline_edit::{
    EditBudget, EditCommand, EditContext, EditMode, EditRefusal, InlineFormat, NoChangeReason,
    PlanOutcome, RereadFailure, SourceEdit, plan_change, validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, Parsed, ProtectionKind, parse};
use fuzz::{Config, Run, TestResult, boundaries};

const LIMIT: usize = 1 << 20;

fn budget() -> EditBudget {
    EditBudget::new(
        PatchBudget::new(16, LIMIT, LIMIT),
        TextBudget::new(LIMIT),
        ParseBudget::m0(),
    )
}

fn state(buffer: &StringBuffer) -> TestResult<String> {
    Ok(format!(
        "{:?}:{:?}:{}:{:?}:{}:{}:{}",
        buffer.text(TextBudget::new(LIMIT))?.as_bytes(),
        buffer.snapshot().id(),
        buffer.revision().value(),
        buffer.selection(),
        buffer.undo_depth(),
        buffer.redo_depth(),
        buffer.history_node_count()
    ))
}

/// 随机生成器给出的输入身份/UTF-8/预算均有效，只允许与命令语义对应的拒绝。
fn expected_refusal(
    refusal: &EditRefusal,
    command: &EditCommand,
    mode: EditMode,
    parsed: &Parsed,
) -> bool {
    let (range, format) = match command {
        EditCommand::Replace { edits } => match edits.as_slice() {
            [edit] => (edit.range(), false),
            _ => return false,
        },
        EditCommand::ToggleInlineFormat { range, .. } => (range, true),
    };
    let touches =
        |other: &std::ops::Range<usize>| range.start <= other.end && other.start <= range.end;
    match refusal {
        EditRefusal::WrongMode {
            expected: EditMode::Visual,
            actual: EditMode::Source,
        } => format && mode == EditMode::Source,
        EditRefusal::EmptySelection => format && mode == EditMode::Visual && range.is_empty(),
        EditRefusal::ProtectedRange {
            kind,
            range: protected,
        } => {
            mode == EditMode::Visual
                && touches(protected)
                && parsed
                    .protected_ranges()
                    .iter()
                    .any(|entry| entry.kind() == *kind && entry.range().byte_range() == *protected)
        }
        EditRefusal::HtmlBoundary { range: protected } => {
            mode == EditMode::Visual
                && touches(protected)
                && (parsed.protected_ranges().iter().any(|entry| {
                    entry.kind() == ProtectionKind::Html && entry.range().byte_range() == *protected
                }) || parsed
                    .styled_ranges()
                    .iter()
                    .any(|entry| entry.range().byte_range() == *protected))
        }
        EditRefusal::RereadFailed {
            reason,
            cause: None,
        } if mode == EditMode::Visual => match reason {
            RereadFailure::VisibleTextChanged | RereadFailure::HiddenSourceChanged => true,
            RereadFailure::BlockStructureChanged => format,
            RereadFailure::Projection => !format,
            _ => false,
        },
        // Fresh 输入的身份/坐标/内部计划错误及意外资源失败都必须让测试失败。
        _ => false,
    }
}

fn expected_unchanged(
    reason: NoChangeReason,
    command: &EditCommand,
    text: &str,
    mode: EditMode,
) -> bool {
    match (reason, command) {
        (NoChangeReason::ReplacementIdentical, EditCommand::Replace { edits }) => edits
            .iter()
            .all(|edit| edit.expected() == edit.replacement()),
        (NoChangeReason::AlreadyFormatted, EditCommand::ToggleInlineFormat { range, format }) => {
            mode == EditMode::Visual
                && !range.is_empty()
                && text
                    .get(..range.start)
                    .is_some_and(|prefix| prefix.ends_with(format.marker()))
                && text
                    .get(range.end..)
                    .is_some_and(|suffix| suffix.starts_with(format.marker()))
        }
        _ => false,
    }
}

#[test]
fn edit_fuzz_keeps_planning_pure_and_rejects_stale_targets() -> TestResult {
    let config = Config::read()?;
    for seed_index in 0..config.seeds {
        let mut run = Run::new(config.seed + u64::try_from(seed_index)?, config.trace);
        let result = (|| -> TestResult {
            let mut buffer = StringBuffer::new(
                "---\ntitle: 中文\n---\n\n# 标题\n\n中文 **bold** 🙂\n\n[toc]\n\n[^N]: 脚注\n"
                    .into(),
                SelectionSpec::new(0, 0),
                TextBudget::new(LIMIT),
                LineIndexBudget::new(LIMIT),
                HistoryBudget::new(200_001, LIMIT * 32),
            )?;
            let mut applied = 0;
            for step in 0..config.steps {
                let text = buffer.text(TextBudget::new(LIMIT))?.into_owned();
                let points = boundaries(&text);
                let a = run.below(points.len())?;
                let b = run.below(points.len())?;
                let range = points[a.min(b)]..points[a.max(b)];
                let target = SrcSelection::new(
                    buffer.snapshot().src_offset(points[a])?,
                    buffer.snapshot().src_offset(points[b])?,
                )?;
                buffer.set_selection(&target)?;
                let parsed = parse(buffer.snapshot(), ParseConfig::m0(), ParseBudget::m0())?;
                let mode = if step % 3 == 0 {
                    EditMode::Visual
                } else {
                    EditMode::Source
                };
                let context = EditContext::new(mode);
                let replacements = ["x", "中文", "\r\n", "🙂", "", "**a**"];
                let replacement = replacements[run.below(replacements.len())?];
                let command = if step % 4 == 0 {
                    EditCommand::toggle_inline_format(range.clone(), InlineFormat::Strong)
                } else {
                    EditCommand::replace(vec![SourceEdit::new(
                        range.clone(),
                        text.get(range.clone()).ok_or("选区无效")?.to_owned(),
                        replacement.to_owned(),
                    )])
                };
                run.record(
                    step,
                    format!(
                        "source={text:?} mode={mode:?} selection=({}, {}) command={command:?}",
                        points[a], points[b]
                    ),
                );
                let before = state(&buffer)?;
                // 每个保护区都实际走 Visual 写入口，Source 是否放行由普通随机命令覆盖。
                for protected in parsed.protected_ranges() {
                    let protected_range = protected.range().byte_range();
                    let source = text.get(protected_range.clone()).ok_or("保护区越界")?;
                    let blocked = EditCommand::replace(vec![SourceEdit::new(
                        protected_range,
                        source.to_owned(),
                        "X",
                    )]);
                    let outcome = plan_change(
                        buffer.snapshot(),
                        &parsed,
                        buffer.selection(),
                        &EditContext::new(EditMode::Visual),
                        &blocked,
                        budget(),
                    );
                    run.check(
                        matches!(outcome, Err(EditRefusal::ProtectedRange { .. })),
                        "Visual 保护区拒绝",
                        text.as_bytes(),
                    )?;
                }
                let planned = plan_change(
                    buffer.snapshot(),
                    &parsed,
                    buffer.selection(),
                    &context,
                    &command,
                    budget(),
                );
                run.equal(&state(&buffer)?, &before, "计划不得写入", text.as_bytes())?;
                let plan = match planned {
                    Ok(PlanOutcome::Changed(plan)) => Some(plan),
                    Ok(PlanOutcome::Unchanged(reason)) => {
                        run.check(
                            expected_unchanged(reason, &command, &text, mode),
                            &format!("非预期无变化: {reason:?}"),
                            text.as_bytes(),
                        )?;
                        None
                    }
                    Err(refusal) => {
                        run.check(
                            expected_refusal(&refusal, &command, mode, &parsed),
                            &format!("非预期拒绝: {refusal:?}"),
                            text.as_bytes(),
                        )?;
                        None
                    }
                };
                if let Some(plan) = plan {
                    let repeated = parse(buffer.snapshot(), ParseConfig::m0(), ParseBudget::m0())?;
                    run.check(
                        matches!(
                            validate_change(
                                &plan,
                                buffer.snapshot(),
                                &repeated,
                                buffer.selection(),
                                &context
                            ),
                            Err(EditRefusal::ProjectionMismatch)
                        ),
                        "同快照新投影拒绝",
                        text.as_bytes(),
                    )?;
                    let foreign = TextSnapshot::new(text.clone(), TextBudget::new(LIMIT))?;
                    run.check(
                        validate_change(&plan, &foreign, &parsed, buffer.selection(), &context)
                            .is_err(),
                        "外来快照拒绝",
                        text.as_bytes(),
                    )?;
                    let other_off = if points[a] == 0 { text.len() } else { 0 };
                    let different = SrcSelection::new(
                        buffer.snapshot().src_offset(other_off)?,
                        buffer.snapshot().src_offset(other_off)?,
                    )?;
                    if &different != buffer.selection() {
                        run.check(
                            matches!(
                                validate_change(
                                    &plan,
                                    buffer.snapshot(),
                                    &parsed,
                                    &different,
                                    &context
                                ),
                                Err(EditRefusal::SelectionMismatch)
                            ),
                            "选区漂移拒绝",
                            text.as_bytes(),
                        )?;
                    }
                    let validated = validate_change(
                        &plan,
                        buffer.snapshot(),
                        &parsed,
                        buffer.selection(),
                        &context,
                    )?;
                    run.equal(&state(&buffer)?, &before, "复验不得写入", text.as_bytes())?;
                    let mut oracle = text.clone();
                    for (src_range, replacement) in validated.batch().patches().iter().rev() {
                        oracle.replace_range(src_range.clone(), replacement);
                    }
                    let after_selection = validated.target_selection();
                    buffer.apply(
                        validated.batch(),
                        after_selection,
                        OperationSource::Command,
                        u64::try_from(step)?,
                        TextBudget::new(LIMIT),
                    )?;
                    applied += 1;
                    let actual = buffer.text(TextBudget::new(LIMIT))?;
                    let mut observed = actual.as_bytes().to_vec();
                    if config.inject {
                        observed.pop();
                    }
                    if let Err(error) = run.equal(
                        &observed,
                        &oracle.as_bytes().to_vec(),
                        "发布精确补丁",
                        text.as_bytes(),
                    ) {
                        // 注入只损坏实际发布后的观测值；同一字节损失性质用于缩减。
                        if config.inject {
                            let minimum = fuzz::minimize(oracle.as_bytes().to_vec(), |bytes| {
                                std::str::from_utf8(bytes).is_ok_and(|source| {
                                    let mut observed = source.as_bytes().to_vec();
                                    observed.pop();
                                    observed != bytes
                                })
                            });
                            return Err(fuzz::invalid(format!(
                                "{error}\n最小注入观测片段（计划重放仍需上述操作序列）={minimum:?}"
                            )));
                        }
                        return Err(error);
                    }
                    run.check(
                        matches!(
                            validate_change(
                                &plan,
                                buffer.snapshot(),
                                &parsed,
                                buffer.selection(),
                                &context
                            ),
                            Err(EditRefusal::WrongVersion { .. })
                        ),
                        "发布后旧计划拒绝",
                        text.as_bytes(),
                    )?;
                }
            }
            if config.steps >= 20 {
                run.check(applied > 0, "随机命令不得全部拒绝空跑", b"")?;
            }
            Ok(())
        })();
        result
            .map_err(|error| run.failure(&error.to_string(), b"input is recorded in operations"))?;
    }
    Ok(())
}

#[test]
fn edit_fuzz_refusal_oracle_rejects_fresh_identity_errors_and_false_noops() -> TestResult {
    let snapshot = TextSnapshot::new("abc".into(), TextBudget::new(LIMIT))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let command = EditCommand::replace(vec![SourceEdit::new(0..1, "a", "b")]);
    for refusal in [
        EditRefusal::ProjectionMismatch,
        EditRefusal::InvalidPlan,
        EditRefusal::WrongSnapshot,
    ] {
        assert!(!expected_refusal(
            &refusal,
            &command,
            EditMode::Source,
            &parsed
        ));
        assert!(!expected_refusal(
            &refusal,
            &command,
            EditMode::Visual,
            &parsed
        ));
    }
    assert!(!expected_unchanged(
        NoChangeReason::ReplacementIdentical,
        &command,
        "abc",
        EditMode::Source
    ));
    let formatted = EditCommand::toggle_inline_format(2..3, InlineFormat::Strong);
    assert!(!expected_unchanged(
        NoChangeReason::AlreadyFormatted,
        &formatted,
        "**a**",
        EditMode::Source
    ));
    Ok(())
}
