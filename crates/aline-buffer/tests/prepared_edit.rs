//! 后台候选只持冻结源码，真实缓冲按当前身份与历史约束结算。

use std::ops::Range;
use std::sync::Arc;

use aline_buffer::{
    ApplyOutcome, BufferError, CheckedBatch, HistoryBudget, LineIndexBudget, OperationSource,
    PatchBudget, SelectionSpec, SrcSelection, StringBuffer, TextBudget, TextSnapshot,
};

const TEXT: TextBudget = TextBudget::new(64);
const PATCH: PatchBudget = PatchBudget::new(4, 64, 64);

#[test]
fn prepared_edit_leaves_real_buffer_untouched_until_publish() -> Result<(), BufferError> {
    let mut buffer = buffer("甲x", SelectionSpec::new(4, 0), 8, 8)?;
    let base = buffer.edit_base();
    let patch = patch(base.snapshot(), 3..4, "乙\n尾")?;
    let id = base.snapshot().id().clone();
    let prepared = base.prepare_apply(
        &patch,
        SelectionSpec::new(0, 3),
        OperationSource::Replace,
        1,
        TEXT,
    )?;
    let candidate = prepared.result_snapshot();
    let candidate_range = candidate.src_range(0..candidate.len_bytes())?;
    assert_eq!(candidate.slice(&candidate_range, TEXT)?, "甲乙\n尾");
    assert_eq!(prepared.result_selection().anchor().byte_offset(), 0);
    assert_eq!(prepared.result_selection().head().byte_offset(), 3);
    let candidate_id = candidate.id().clone();

    assert_eq!(buffer.snapshot().id(), &id);
    assert_editor(&buffer, "甲x", SelectionSpec::new(4, 0), 0, 0)?;
    let ApplyOutcome::Changed(applied) = buffer.publish_prepared(prepared)? else {
        return Err(BufferError::HistoryContextMismatch);
    };
    assert_eq!(
        buffer.snapshot().id(),
        &candidate_id,
        "真正发布须消费准确候选文本"
    );
    assert_eq!(applied.revision().value(), 1);
    assert_eq!(applied.line_count(), 2);
    assert_eq!(applied.inverse_patches(), &[(3..10, String::from("x"))]);
    assert_eq!(buffer.line_of_offset(7), Some(1));
    assert_editor(&buffer, "甲乙\n尾", SelectionSpec::new(0, 3), 1, 0)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "甲x", SelectionSpec::new(4, 0), 0, 1)?;
    buffer.redo(TEXT)?;
    assert_editor(&buffer, "甲乙\n尾", SelectionSpec::new(0, 3), 1, 0)?;
    Ok(())
}

#[test]
fn prepared_edit_rejects_foreign_snapshot_and_changed_direction() -> Result<(), BufferError> {
    let before = SelectionSpec::new(4, 1);
    let after = SelectionSpec::new(1, 4);
    let mut source = buffer("a甲", before, 8, 8)?;
    let mut other = buffer("a甲", before, 8, 8)?;
    let base = source.edit_base();
    let replacement = patch(base.snapshot(), 1..4, "x")?;
    let end = SelectionSpec::new(2, 2);
    let prepared = base.prepare_apply(&replacement, end, OperationSource::Replace, 0, TEXT)?;
    assert!(matches!(
        other.publish_prepared(prepared),
        Err(BufferError::SnapshotMismatch)
    ));
    assert_editor(&other, "a甲", before, 0, 0)?;

    let prepared =
        source
            .edit_base()
            .prepare_apply(&replacement, end, OperationSource::Replace, 1, TEXT)?;
    let id = source.snapshot().id().clone();
    let selection = SrcSelection::new(
        source.snapshot().src_offset(1)?,
        source.snapshot().src_offset(4)?,
    )?;
    source.set_selection(&selection)?;
    assert_eq!(source.snapshot().id(), &id);
    assert!(matches!(
        source.publish_prepared(prepared),
        Err(BufferError::HistoryContextMismatch)
    ));
    assert_editor(&source, "a甲", after, 0, 0)?;
    Ok(())
}

#[test]
fn prepared_no_change_rejects_old_revision_and_preserves_redo() -> Result<(), BufferError> {
    let mut buffer = buffer("a", SelectionSpec::new(1, 1), 8, 8)?;
    let same = patch(buffer.snapshot(), 0..1, "a")?;
    let selection = SelectionSpec::new(1, 1);
    let prepared =
        buffer
            .edit_base()
            .prepare_apply(&same, selection, OperationSource::Replace, 0, TEXT)?;
    let replacement = patch(buffer.snapshot(), 0..1, "b")?;
    buffer.apply(&replacement, selection, OperationSource::Replace, 1, TEXT)?;
    buffer.undo(TEXT)?;
    assert!(matches!(
        buffer.publish_prepared(prepared),
        Err(BufferError::RevisionMismatch)
    ));
    assert_editor(&buffer, "a", selection, 0, 1)?;
    let same = patch(buffer.snapshot(), 0..1, "a")?;
    let prepared =
        buffer
            .edit_base()
            .prepare_apply(&same, selection, OperationSource::Replace, 2, TEXT)?;
    assert!(matches!(
        buffer.publish_prepared(prepared)?,
        ApplyOutcome::Unchanged
    ));
    assert_editor(&buffer, "a", selection, 0, 1)?;
    Ok(())
}

#[test]
fn staged_preparation_requires_first_actual_candidate_and_publishes_two_steps()
-> Result<(), BufferError> {
    let before = SelectionSpec::new(2, 1);
    let mut buffer = buffer("ab", before, 8, 8)?;
    let typing = patch(buffer.snapshot(), 1..2, "中")?;
    let typing_selection = SelectionSpec::new(4, 1);
    let command_selection = SelectionSpec::new(2, 2);
    let start = buffer.selection().clone();
    let alien_result = buffer.edit_base().prepare_staged_command::<BufferError>(
        &typing,
        (&start, typing_selection),
        1,
        TEXT,
        |candidate, _| {
            let separate = TextSnapshot::from_shared(
                Arc::new(String::from("a中")),
                candidate.revision(),
                TEXT,
            )?;
            Ok(Some((patch(&separate, 1..4, "Z")?, command_selection)))
        },
    );
    assert!(matches!(alien_result, Err(BufferError::SnapshotMismatch)));
    let prepared = buffer.edit_base().prepare_staged_command::<BufferError>(
        &typing,
        (&start, typing_selection),
        1,
        TEXT,
        |candidate, selection| {
            assert!(selection.is_reversed());
            let content = candidate.src_range(0..4)?;
            assert_eq!(candidate.slice(&content, TEXT)?, "a中");
            Ok(Some((patch(candidate, 1..4, "Z")?, command_selection)))
        },
    )?;
    assert_editor(&buffer, "ab", before, 0, 0)?;
    assert!(matches!(
        buffer.publish_prepared(prepared)?,
        ApplyOutcome::Changed(_)
    ));
    assert_editor(&buffer, "aZ", command_selection, 2, 0)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "a中", typing_selection, 1, 1)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "ab", before, 0, 2)?;
    Ok(())
}

#[test]
fn prepared_edit_observes_live_history_and_retains_saved_node() -> Result<(), BufferError> {
    let mut buffer = buffer("", SelectionSpec::new(0, 0), 2, 8)?;
    for (text, now_millis) in [("x", 0), ("y", 1)] {
        let end = buffer.len_bytes();
        let patch = patch(buffer.snapshot(), end..end, text)?;
        buffer.apply(
            &patch,
            SelectionSpec::new(end + 1, end + 1),
            OperationSource::Command,
            now_millis,
            TEXT,
        )?;
    }
    let base = buffer.edit_base();
    let insert = patch(base.snapshot(), 2..2, "z")?;
    let prepared = base.prepare_apply(
        &insert,
        SelectionSpec::new(3, 3),
        OperationSource::Command,
        2,
        TEXT,
    )?;
    buffer.mark_saved();
    let save = buffer.saved_revision();
    assert!(matches!(
        buffer.publish_prepared(prepared)?,
        ApplyOutcome::Changed(_)
    ));
    assert_eq!(buffer.saved_revision(), save);
    assert!(!buffer.is_at_save_point());
    assert_editor(&buffer, "xyz", SelectionSpec::new(3, 3), 2, 0)?;
    assert_eq!(buffer.trim_history(1).removed(), 0);
    Ok(())
}

#[test]
fn prepared_edit_history_budget_failure_preserves_state_after_group_is_sealed()
-> Result<(), BufferError> {
    let mut buffer = buffer("", SelectionSpec::new(0, 0), 1, 8)?;
    let insert = patch(buffer.snapshot(), 0..0, "x")?;
    buffer.apply(
        &insert,
        SelectionSpec::new(1, 1),
        OperationSource::Typing,
        0,
        TEXT,
    )?;
    let insert = patch(buffer.snapshot(), 1..1, "y")?;
    let prepared = buffer.edit_base().prepare_apply(
        &insert,
        SelectionSpec::new(2, 2),
        OperationSource::Typing,
        900,
        TEXT,
    )?;
    buffer.seal_pending();
    buffer.mark_saved();
    let id = buffer.snapshot().id().clone();
    assert!(matches!(
        buffer.publish_prepared(prepared),
        Err(BufferError::HistoryBudgetExceeded { .. })
    ));
    assert!(buffer.is_at_save_point());
    assert_eq!(buffer.snapshot().id(), &id);
    assert_editor(&buffer, "x", SelectionSpec::new(1, 1), 1, 0)?;
    Ok(())
}

fn buffer(
    text: &str,
    selection: SelectionSpec,
    max_nodes: usize,
    max_lines: usize,
) -> Result<StringBuffer, BufferError> {
    StringBuffer::new(
        text.to_owned(),
        selection,
        TEXT,
        LineIndexBudget::new(max_lines),
        HistoryBudget::new(max_nodes, 64),
    )
}

fn patch(
    snapshot: &TextSnapshot,
    range: Range<usize>,
    text: &str,
) -> Result<CheckedBatch, BufferError> {
    CheckedBatch::new(snapshot, vec![(range, text.to_owned())], PATCH)
}

fn assert_editor(
    buffer: &StringBuffer,
    text: &str,
    selection: SelectionSpec,
    undo: usize,
    redo: usize,
) -> Result<(), BufferError> {
    assert_eq!(buffer.text(TEXT)?, text);
    assert_eq!(
        buffer.selection().anchor().byte_offset(),
        selection.anchor()
    );
    assert_eq!(buffer.selection().head().byte_offset(), selection.head());
    assert_eq!(buffer.undo_depth(), undo);
    assert_eq!(buffer.redo_depth(), redo);
    Ok(())
}
