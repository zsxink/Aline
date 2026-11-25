//! M3 独立行内预期，先冻结源码和选区，再接纯计划与真实 Buffer 历史。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{
    HistoryBudget, LineIndexBudget, OperationSource, PatchBudget, SelectionSpec, StringBuffer,
    TextBudget,
};
use aline_edit::{
    EditBudget, EditCommand, EditContext, EditMode, InlineFormat, PlanOutcome, plan_change,
    validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, parse};

const TEXT: TextBudget = TextBudget::new(4096);
const EDIT: EditBudget = EditBudget::new(PatchBudget::new(8, 4096, 4096), TEXT, ParseBudget::m0());
type TestResult = Result<(), Box<dyn Error>>;

fn check(
    source: &str,
    selection: SelectionSpec,
    format: InlineFormat,
    expected: &str,
    target: SelectionSpec,
) -> TestResult {
    let mut buffer = StringBuffer::new(
        source.into(),
        selection,
        TEXT,
        LineIndexBudget::new(100),
        HistoryBudget::new(20, 4096),
    )?;
    let parsed = parse(buffer.snapshot(), ParseConfig::m0(), ParseBudget::m0())?;
    let context = EditContext::new(EditMode::Visual);
    let command =
        EditCommand::toggle_inline_format(buffer.selection().range().byte_range(), format);
    let PlanOutcome::Changed(plan) = plan_change(
        buffer.snapshot(),
        &parsed,
        buffer.selection(),
        &context,
        &command,
        EDIT,
    )
    .map_err(|error| {
        std::io::Error::other(format!("source {source:?}, format {format:?}: {error:?}"))
    })?
    else {
        return Err("expected inline change".into());
    };
    assert_eq!(plan.target_selection(), target, "source {source:?}");
    let validated = validate_change(
        &plan,
        buffer.snapshot(),
        &parsed,
        buffer.selection(),
        &context,
    )?;
    buffer.apply(validated.batch(), target, OperationSource::Command, 0, TEXT)?;
    assert_eq!(buffer.text(TEXT)?.as_ref(), expected, "source {source:?}");
    assert_eq!(buffer.history_node_count(), 1);
    buffer.undo(TEXT)?;
    assert_eq!(buffer.text(TEXT)?.as_ref(), source);
    assert_eq!(
        buffer.selection().anchor().byte_offset(),
        selection.anchor()
    );
    assert_eq!(buffer.selection().head().byte_offset(), selection.head());
    buffer.redo(TEXT)?;
    assert_eq!(buffer.text(TEXT)?.as_ref(), expected);
    assert_eq!(buffer.selection().anchor().byte_offset(), target.anchor());
    assert_eq!(buffer.selection().head().byte_offset(), target.head());
    Ok(())
}

#[test]
fn inline_add_and_remove_preserve_unicode_and_reverse_selection() -> TestResult {
    for (format, marked, offset) in [
        (InlineFormat::Strong, "**甲乙**", 2),
        (InlineFormat::Emphasis, "*甲乙*", 1),
        (InlineFormat::Strike, "~~甲乙~~", 2),
        (InlineFormat::Code, "`甲乙`", 1),
        (InlineFormat::Link, "[甲乙]()", 1),
    ] {
        check(
            "甲乙",
            SelectionSpec::new(6, 0),
            format,
            marked,
            SelectionSpec::new(6 + offset, offset),
        )?;
        check(
            marked,
            SelectionSpec::new(offset, 6 + offset),
            format,
            "甲乙",
            SelectionSpec::new(0, 6),
        )?;
    }
    check(
        "__甲__",
        SelectionSpec::new(2, 5),
        InlineFormat::Strong,
        "甲",
        SelectionSpec::new(0, 3),
    )?;
    check(
        "_甲_",
        SelectionSpec::new(0, 5),
        InlineFormat::Emphasis,
        "甲",
        SelectionSpec::new(0, 3),
    )?;
    check(
        "***甲***",
        SelectionSpec::new(3, 6),
        InlineFormat::Strong,
        "*甲*",
        SelectionSpec::new(1, 4),
    )?;
    check(
        "***甲***",
        SelectionSpec::new(3, 6),
        InlineFormat::Emphasis,
        "**甲**",
        SelectionSpec::new(2, 5),
    )?;
    check(
        "[甲乙](https://a.test \"t\")",
        SelectionSpec::new(1, 7),
        InlineFormat::Link,
        "甲乙",
        SelectionSpec::new(0, 6),
    )
}

#[test]
fn inline_empty_pairs_place_caret_inside_each_marker() -> TestResult {
    for (format, pair, offset) in [
        (InlineFormat::Strong, "****", 2),
        (InlineFormat::Emphasis, "**", 1),
        (InlineFormat::Strike, "~~~~", 2),
        (InlineFormat::Code, "``", 1),
        (InlineFormat::Link, "[]()", 1),
    ] {
        check(
            "甲",
            SelectionSpec::new(3, 3),
            format,
            &format!("甲{pair}"),
            SelectionSpec::new(3 + offset, 3 + offset),
        )?;
        check(
            "",
            SelectionSpec::new(0, 0),
            format,
            pair,
            SelectionSpec::new(offset, offset),
        )?;
    }
    Ok(())
}

#[test]
fn inline_code_chooses_noncolliding_delimiters_and_padding() -> TestResult {
    check(
        "a`b",
        SelectionSpec::new(0, 3),
        InlineFormat::Code,
        "``a`b``",
        SelectionSpec::new(2, 5),
    )?;
    check(
        "`a",
        SelectionSpec::new(0, 2),
        InlineFormat::Code,
        "`` `a ``",
        SelectionSpec::new(3, 5),
    )?;
    check(
        "x a y",
        SelectionSpec::new(1, 4),
        InlineFormat::Code,
        "x`  a  `y",
        SelectionSpec::new(3, 6),
    )?;
    check(
        "x`  a  `y",
        SelectionSpec::new(3, 6),
        InlineFormat::Code,
        "x a y",
        SelectionSpec::new(1, 4),
    )?;
    check(
        "x y",
        SelectionSpec::new(1, 2),
        InlineFormat::Code,
        "x` `y",
        SelectionSpec::new(2, 3),
    )?;
    check(
        "`` `a ``",
        SelectionSpec::new(3, 5),
        InlineFormat::Code,
        "`a",
        SelectionSpec::new(0, 2),
    )
}

#[test]
fn inline_entities_escapes_and_nested_markers_keep_original_bytes() -> TestResult {
    check(
        "甲 &amp; \\*乙",
        SelectionSpec::new(0, 15),
        InlineFormat::Strong,
        "**甲 &amp; \\*乙**",
        SelectionSpec::new(2, 17),
    )?;
    check(
        "**甲 *乙***",
        SelectionSpec::new(2, 11),
        InlineFormat::Strong,
        "甲 *乙*",
        SelectionSpec::new(0, 9),
    )
}

#[test]
fn inline_refusals_keep_text_selection_and_history() -> TestResult {
    let cases: &[(&str, Range<usize>, InlineFormat)] = &[
        ("a <em>b</em>", 0..11, InlineFormat::Strong),
        ("甲[^a]乙\n\n[^a]: 注", 0..10, InlineFormat::Link),
        ("a\n\nb", 0..4, InlineFormat::Strong),
        ("&amp;", 0..5, InlineFormat::Code),
        ("a\\*b", 0..4, InlineFormat::Code),
    ];
    for (source, range, format) in cases {
        let buffer = StringBuffer::new(
            (*source).into(),
            SelectionSpec::new(range.start, range.end),
            TEXT,
            LineIndexBudget::new(100),
            HistoryBudget::new(20, 4096),
        )?;
        let parsed = parse(buffer.snapshot(), ParseConfig::m0(), ParseBudget::m0())?;
        let context = EditContext::new(EditMode::Visual);
        let command = EditCommand::toggle_inline_format(range.clone(), *format);
        assert!(
            plan_change(
                buffer.snapshot(),
                &parsed,
                buffer.selection(),
                &context,
                &command,
                EDIT
            )
            .is_err(),
            "source {source:?}"
        );
        assert_eq!(buffer.text(TEXT)?.as_ref(), *source);
        assert_eq!(buffer.history_node_count(), 0);
    }
    Ok(())
}
