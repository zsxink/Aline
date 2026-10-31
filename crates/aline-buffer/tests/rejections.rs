//! 组 5 反例：所有拒绝在发布前返回，文本/行索引/选区/代次/历史零变化。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{
    ApplyOutcome, BufferError, CheckedBatch, DocumentRevision, HistoryBudget, LineIndexBudget,
    OperationSource, PatchBudget, SelectionSpec, SnapshotId, StringBuffer, TextBudget,
};

const TEXT_BUDGET: usize = 4096;

#[derive(Debug)]
struct State {
    text: String,
    line_count: usize,
    line_ranges: Vec<Option<Range<usize>>>,
    selection: SelectionSpec,
    revision: DocumentRevision,
    undo: usize,
    redo: usize,
    nodes: usize,
    snapshot_id: SnapshotId,
}

#[test]
fn rejection_cases_leave_text_index_selection_revision_and_history_unchanged()
-> Result<(), Box<dyn Error>> {
    let buffer = make_buffer("abcXYZ", 6, 2)?;
    let state = capture(&buffer)?;
    let budget = PatchBudget::new(8, 64, 64);
    let invalid = [
        ("末项越界", vec![(6..7, String::from("!"))]),
        (
            "重叠",
            vec![(0..3, String::new()), (2..4, String::from("!"))],
        ),
        ("逆序", vec![(4..5, String::new()), (0..1, String::new())]),
        (
            "同点歧义插入",
            vec![(3..3, String::from("a")), (3..3, String::from("b"))],
        ),
    ];
    for (label, patches) in invalid {
        let rejected = CheckedBatch::new(buffer.snapshot(), patches, budget);
        assert!(rejected.is_err(), "{label}: 必须拒绝");
        assert_state(&buffer, &state)?;
    }
    let emoji = make_buffer("甲😀Z", 8, 8)?;
    let emoji_state = capture(&emoji)?;
    assert!(matches!(
        CheckedBatch::new(
            &emoji.snapshot().clone(),
            vec![(1..3, String::new())],
            budget
        ),
        Err(BufferError::NotCharBoundary { .. })
    ));
    assert_state(&emoji, &emoji_state)?;
    Ok(())
}

#[test]
fn apply_rejections_are_atomic_for_baseline_selection_and_capacities() -> Result<(), Box<dyn Error>>
{
    let source = make_buffer("abc", 3, 3)?;
    let batch = CheckedBatch::new(
        source.snapshot(),
        vec![(0..0, String::from("X"))],
        PatchBudget::new(8, 64, 64),
    )?;

    let mut other = make_buffer("abc", 3, 3)?;
    let other_state = capture(&other)?;
    assert!(matches!(
        other.apply(
            &batch,
            SelectionSpec::new(4, 4),
            OperationSource::Replace,
            1000,
            TextBudget::new(TEXT_BUDGET)
        ),
        Err(BufferError::SnapshotMismatch)
    ));
    assert_state(&other, &other_state)?;

    let mut wide = make_buffer("甲a", 4, 4)?;
    let wide_batch = CheckedBatch::new(
        wide.snapshot(),
        vec![(0..0, String::from("x"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let wide_state = capture(&wide)?;
    assert!(matches!(
        wide.apply(
            &wide_batch,
            SelectionSpec::new(2, 2),
            OperationSource::Replace,
            1000,
            TextBudget::new(TEXT_BUDGET)
        ),
        Err(BufferError::NotCharBoundary { offset: 2 })
    ));
    assert_state(&wide, &wide_state)?;

    let mut long = make_buffer("abc", 3, 3)?;
    let long_batch = CheckedBatch::new(
        long.snapshot(),
        vec![(0..0, String::from("X")), (3..3, String::from("Y"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let long_state = capture(&long)?;
    assert!(matches!(
        long.apply(
            &long_batch,
            SelectionSpec::new(5, 5),
            OperationSource::Replace,
            1000,
            TextBudget::new(4)
        ),
        Err(BufferError::BudgetExceeded { .. })
    ));
    assert_state(&long, &long_state)?;

    let mut lines = StringBuffer::new(
        String::from("a"),
        SelectionSpec::new(1, 1),
        TextBudget::new(TEXT_BUDGET),
        LineIndexBudget::new(1),
        HistoryBudget::new(16, 1 << 20),
    )?;
    let line_batch = CheckedBatch::new(
        lines.snapshot(),
        vec![(1..1, String::from("\nb"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let line_state = capture(&lines)?;
    assert!(matches!(
        lines.apply(
            &line_batch,
            SelectionSpec::new(3, 3),
            OperationSource::Replace,
            1000,
            TextBudget::new(TEXT_BUDGET)
        ),
        Err(BufferError::LineCountExceeded { count: 2, limit: 1 })
    ));
    assert_state(&lines, &line_state)?;

    let mut history_limited = StringBuffer::new(
        String::from("abc"),
        SelectionSpec::new(3, 3),
        TextBudget::new(TEXT_BUDGET),
        LineIndexBudget::new(16),
        HistoryBudget::new(0, 1024),
    )?;
    let history_batch = CheckedBatch::new(
        history_limited.snapshot(),
        vec![(0..0, String::from("X"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let history_state = capture(&history_limited)?;
    assert!(matches!(
        history_limited.apply(
            &history_batch,
            SelectionSpec::new(4, 4),
            OperationSource::Typing,
            1000,
            TextBudget::new(TEXT_BUDGET)
        ),
        Err(BufferError::HistoryBudgetExceeded { .. })
    ));
    assert_state(&history_limited, &history_state)?;

    let mut byte_limited = StringBuffer::new(
        String::from("abc"),
        SelectionSpec::new(3, 3),
        TextBudget::new(TEXT_BUDGET),
        LineIndexBudget::new(16),
        HistoryBudget::new(8, 1),
    )?;
    let byte_batch = CheckedBatch::new(
        byte_limited.snapshot(),
        vec![(0..0, String::from("XY"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let byte_state = capture(&byte_limited)?;
    assert!(matches!(
        byte_limited.apply(
            &byte_batch,
            SelectionSpec::new(5, 5),
            OperationSource::Typing,
            1000,
            TextBudget::new(TEXT_BUDGET)
        ),
        Err(BufferError::HistoryBudgetExceeded { .. })
    ));
    assert_state(&byte_limited, &byte_state)?;
    Ok(())
}

#[test]
fn legal_empty_insertion_and_no_change_replacement_are_distinct() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("abc", 3, 3)?;
    let insertion = CheckedBatch::new(
        buffer.snapshot(),
        vec![(1..1, String::from("X"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let outcome = buffer.apply(
        &insertion,
        SelectionSpec::new(4, 4),
        OperationSource::Typing,
        1000,
        TextBudget::new(TEXT_BUDGET),
    )?;
    let ApplyOutcome::Changed(applied) = outcome else {
        return Err("合法空区间插入必须产生变化".to_string().into());
    };
    assert_eq!(text(&buffer)?, "aXbc");
    assert_eq!(applied.inverse_patches(), &[(1..2, String::new())]);
    assert_eq!(buffer.history_node_count(), 1);

    let mut unchanged = make_buffer("abc", 2, 1)?;
    let unchanged_state = capture(&unchanged)?;
    let no_op = CheckedBatch::new(
        unchanged.snapshot(),
        vec![(1..2, String::from("b"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let outcome = unchanged.apply(
        &no_op,
        SelectionSpec::new(0, 3),
        OperationSource::Replace,
        1001,
        TextBudget::new(TEXT_BUDGET),
    )?;
    assert!(matches!(outcome, ApplyOutcome::Unchanged));
    assert_state(&unchanged, &unchanged_state)?;

    let mut mixed = make_buffer("abc", 3, 2)?;
    let mixed_batch = CheckedBatch::new(
        mixed.snapshot(),
        vec![(0..0, String::new()), (1..2, String::from("B"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let outcome = mixed.apply(
        &mixed_batch,
        SelectionSpec::new(3, 2),
        OperationSource::Replace,
        1002,
        TextBudget::new(TEXT_BUDGET),
    )?;
    let ApplyOutcome::Changed(applied) = outcome else {
        return Err("混合批次必须只记录实际变化".to_string().into());
    };
    assert_eq!(text(&mixed)?, "aBc");
    assert_eq!(applied.inverse_patches(), &[(1..2, String::from("b"))]);
    let step = mixed
        .history_step(0)
        .ok_or_else(|| "缺少混合批次历史".to_string())?;
    assert_eq!(step.get().forward_patches(), &[(1..2, String::from("B"))]);
    Ok(())
}

#[test]
fn no_change_after_undo_does_not_clear_redo_branch() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("abc", 3, 3)?;
    let insert = CheckedBatch::new(
        buffer.snapshot(),
        vec![(3..3, String::from("X"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let outcome = buffer.apply(
        &insert,
        SelectionSpec::new(4, 4),
        OperationSource::Typing,
        1000,
        TextBudget::new(TEXT_BUDGET),
    )?;
    assert!(matches!(outcome, ApplyOutcome::Changed(_)));
    assert_eq!(text(&buffer)?, "abcX");

    let undone_revision = buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(undone_revision.value(), 2);
    assert_eq!(text(&buffer)?, "abc");
    assert_eq!(buffer.redo_depth(), 1);
    let after_undo = capture(&buffer)?;

    let no_op = CheckedBatch::new(
        buffer.snapshot(),
        vec![(1..2, String::from("b"))],
        PatchBudget::new(8, 64, 64),
    )?;
    let outcome = buffer.apply(
        &no_op,
        SelectionSpec::new(0, 2),
        OperationSource::Replace,
        1001,
        TextBudget::new(TEXT_BUDGET),
    )?;
    assert!(matches!(outcome, ApplyOutcome::Unchanged));
    assert_eq!(buffer.redo_depth(), 1);
    assert_state(&buffer, &after_undo)?;

    let redone_revision = buffer.redo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(redone_revision.value(), 3);
    assert_eq!(text(&buffer)?, "abcX");
    assert_eq!(
        SelectionSpec::new(
            buffer.selection().anchor().byte_offset(),
            buffer.selection().head().byte_offset()
        ),
        SelectionSpec::new(4, 4)
    );
    Ok(())
}

fn make_buffer(text: &str, anchor: usize, head: usize) -> Result<StringBuffer, Box<dyn Error>> {
    StringBuffer::new(
        text.to_string(),
        SelectionSpec::new(anchor, head),
        TextBudget::new(TEXT_BUDGET),
        LineIndexBudget::new(16),
        HistoryBudget::new(16, 1 << 20),
    )
    .map_err(Into::into)
}

fn text(buffer: &StringBuffer) -> Result<String, Box<dyn Error>> {
    Ok(buffer.text(TextBudget::new(TEXT_BUDGET))?.into_owned())
}

fn capture(buffer: &StringBuffer) -> Result<State, Box<dyn Error>> {
    let line_count = buffer.line_count();
    let line_ranges = (0..line_count)
        .map(|line| buffer.line_index().line_range(line))
        .collect();
    Ok(State {
        text: text(buffer)?,
        line_count,
        line_ranges,
        selection: SelectionSpec::new(
            buffer.selection().anchor().byte_offset(),
            buffer.selection().head().byte_offset(),
        ),
        revision: buffer.revision(),
        undo: buffer.undo_depth(),
        redo: buffer.redo_depth(),
        nodes: buffer.history_node_count(),
        snapshot_id: buffer.snapshot().id().clone(),
    })
}

fn assert_state(buffer: &StringBuffer, expected: &State) -> Result<(), Box<dyn Error>> {
    let actual = capture(buffer)?;
    assert_eq!(actual.text, expected.text);
    assert_eq!(actual.line_count, expected.line_count);
    assert_eq!(actual.line_ranges, expected.line_ranges);
    assert_eq!(actual.selection, expected.selection);
    assert_eq!(actual.revision, expected.revision);
    assert_eq!(actual.undo, expected.undo);
    assert_eq!(actual.redo, expected.redo);
    assert_eq!(actual.nodes, expected.nodes);
    assert_eq!(actual.snapshot_id, expected.snapshot_id);
    Ok(())
}
