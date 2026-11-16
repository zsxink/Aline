//! 两步命令的文本、选区、历史与失败边界。

use std::ops::Range;

use aline_buffer::{
    ApplyOutcome, BufferError, CheckedBatch, DocumentRevision, HistoryBudget, HistoryStepHandle,
    LineIndexBudget, OperationSource, PatchBudget, SelectionSpec, SnapshotId, SrcSelection,
    StringBuffer, TextBudget, TextSnapshot,
};

const TEXT: TextBudget = TextBudget::new(128);
const PATCH: PatchBudget = PatchBudget::new(8, 128, 128);

#[test]
fn staged_command_restores_directional_selections_step_by_step() -> Result<(), BufferError> {
    let mut buffer = buffer("a中b", 8, 128, 8)?;
    let before = bind(buffer.snapshot(), SelectionSpec::new(4, 1))?;
    let typing = patch(buffer.snapshot(), 1..4, "xy")?;
    let result = buffer.apply_staged_command(
        &typing,
        (&before, SelectionSpec::new(3, 1)),
        10,
        TEXT,
        |candidate, selected| {
            assert_eq!(selection_spec(selected), SelectionSpec::new(3, 1));
            assert_eq!(candidate.revision().value(), 1);
            let selected = candidate.src_range(1..3)?;
            assert_eq!(candidate.slice(&selected, TEXT)?, "xy");
            command(patch(candidate, 1..3, "😀")?, 5)
        },
    )?;
    assert!(matches!(
        result,
        ApplyOutcome::Changed(applied)
            if applied.revision().value() == 2
                && applied.inverse_patches() == [(1..5, String::from("xy"))]
    ));
    assert_eq!(buffer.undo_depth(), 2);
    assert_editor(&buffer, "a😀b", 5, 5)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "axyb", 3, 1)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "a中b", 4, 1)?;
    buffer.redo(TEXT)?;
    assert_editor(&buffer, "axyb", 3, 1)?;
    buffer.redo(TEXT)?;
    assert_editor(&buffer, "a😀b", 5, 5)?;
    Ok(())
}

#[test]
fn staged_command_keeps_typing_separate_from_an_existing_pending_node() -> Result<(), BufferError> {
    let mut buffer = buffer("a", 8, 128, 8)?;
    apply_insertion(&mut buffer, "x", OperationSource::Typing, 1)?;
    let previous = buffer.history_step(0).ok_or(BufferError::NothingToUndo)?;
    append_typing_and_command(&mut buffer, "y", "Z", 2)?;
    assert_eq!(buffer.history_node_count(), 3);
    let current = buffer.history_step(0).ok_or(BufferError::NothingToUndo)?;
    assert!(std::ptr::eq(previous.get(), current.get()));
    assert_editor(&buffer, "axyZ", 4, 4)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "axy", 3, 3)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "ax", 2, 2)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "a", 1, 1)?;
    Ok(())
}

#[test]
fn staged_command_only_records_the_command_when_typing_has_no_net_change() -> Result<(), BufferError>
{
    let mut buffer = buffer("abc", 1, 128, 8)?;
    let before = bind(buffer.snapshot(), SelectionSpec::new(1, 2))?;
    let first_id = buffer.snapshot().id().clone();
    let typing = patch(buffer.snapshot(), 1..2, "b")?;
    let start_selection = SelectionSpec::new(2, 0);
    let original = State::capture(&buffer)?;
    assert!(matches!(
        buffer.apply_staged_command::<BufferError>(
            &typing,
            (&before, start_selection),
            1,
            TEXT,
            |candidate, selected| {
                assert_eq!(candidate.id(), &first_id);
                assert_eq!(selection_spec(selected), start_selection);
                Ok(None)
            },
        ),
        Ok(ApplyOutcome::Unchanged)
    ));
    original.assert_matches(&buffer)?;
    let result = buffer.apply_staged_command(
        &typing,
        (&before, start_selection),
        1,
        TEXT,
        |candidate, selected| {
            assert_eq!(candidate.id(), &first_id);
            assert_eq!(selection_spec(selected), start_selection);
            command(patch(candidate, 0..2, "")?, 0)
        },
    )?;
    assert!(matches!(
        result,
        ApplyOutcome::Changed(applied) if applied.revision().value() == 1
    ));
    assert_eq!(buffer.undo_depth(), 1);
    assert_editor(&buffer, "c", 0, 0)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "abc", 2, 0)?;
    Ok(())
}

#[test]
fn staged_command_trims_only_eligible_older_nodes_and_discards_redo_on_success()
-> Result<(), BufferError> {
    let mut buffer = buffer("a", 3, 128, 8)?;
    for text in ["x", "y", "z"] {
        apply_insertion(&mut buffer, text, OperationSource::Command, 1)?;
    }
    let keep = buffer.history_step(1).ok_or(BufferError::NothingToUndo)?;
    buffer.undo(TEXT)?;
    assert_eq!(buffer.redo_depth(), 1);
    append_typing_and_command(&mut buffer, "T", "C", 2)?;
    assert_eq!(buffer.history_node_count(), 3);
    assert_eq!(buffer.redo_depth(), 0);
    assert_editor(&buffer, "axyTC", 5, 5)?;
    let current = buffer.history_step(0).ok_or(BufferError::NothingToUndo)?;
    assert!(std::ptr::eq(keep.get(), current.get()));
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "axyT", 4, 4)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "axy", 3, 3)?;
    buffer.undo(TEXT)?;
    assert_editor(&buffer, "ax", 2, 2)?;
    Ok(())
}

#[test]
fn staged_command_rejects_history_limits_and_save_point_without_publication()
-> Result<(), BufferError> {
    for (nodes, bytes) in [(1, 128), (8, 1)] {
        let mut limited = buffer("a", nodes, bytes, 8)?;
        let original = State::capture(&limited)?;
        assert!(matches!(
            append_typing_and_command(&mut limited, "T", "C", 2),
            Err(BufferError::HistoryBudgetExceeded { .. })
        ));
        original.assert_matches(&limited)?;
    }

    let mut saved = buffer("a", 3, 128, 8)?;
    apply_insertion(&mut saved, "x", OperationSource::Command, 1)?;
    saved.mark_saved();
    apply_insertion(&mut saved, "y", OperationSource::Command, 2)?;
    let original = State::capture(&saved)?;
    assert!(matches!(
        append_typing_and_command(&mut saved, "T", "C", 3),
        Err(BufferError::HistoryBudgetExceeded { .. })
    ));
    original.assert_matches(&saved)?;
    Ok(())
}

#[derive(Debug)]
enum PlanningError {
    Refused,
    Cancelled,
    Buffer(BufferError),
}

impl From<BufferError> for PlanningError {
    fn from(error: BufferError) -> Self {
        Self::Buffer(error)
    }
}

#[test]
fn staged_command_keeps_redo_for_refusal_cancellation_and_no_change() -> Result<(), BufferError> {
    let mut buffer = buffer("a中", 8, 128, 8)?;
    apply_insertion(&mut buffer, "x", OperationSource::Command, 1)?;
    buffer.mark_saved();
    buffer.undo(TEXT)?;
    let original = State::capture(&buffer)?;
    let before = buffer.selection().clone();
    let typing = patch(buffer.snapshot(), 0..0, "T")?;
    let typing_selection = (&before, SelectionSpec::new(0, 1));
    for error in [PlanningError::Refused, PlanningError::Cancelled] {
        let result =
            buffer.apply_staged_command(&typing, typing_selection, 2, TEXT, |_, _| Err(error));
        assert!(matches!(
            result,
            Err(PlanningError::Refused | PlanningError::Cancelled)
        ));
        original.assert_matches(&buffer)?;
    }
    assert!(matches!(
        buffer.apply_staged_command::<BufferError>(&typing, typing_selection, 2, TEXT, |_, _| {
            Ok(None)
        }),
        Ok(ApplyOutcome::Unchanged)
    ));
    original.assert_matches(&buffer)?;
    assert!(matches!(
        buffer.apply_staged_command(&typing, typing_selection, 2, TEXT, |candidate, _| {
            command(patch(candidate, 0..1, "T")?, 1)
        }),
        Ok(ApplyOutcome::Unchanged)
    ));
    original.assert_matches(&buffer)?;

    let result = buffer.apply_staged_command(&typing, typing_selection, 2, TEXT, |candidate, _| {
        command(patch(candidate, 3..4, "")?, 0).map_err(PlanningError::from)
    });
    assert!(matches!(
        result,
        Err(PlanningError::Buffer(BufferError::NotCharBoundary {
            offset: 3
        }))
    ));
    original.assert_matches(&buffer)?;
    Ok(())
}

#[test]
fn staged_command_rejects_stale_batches_and_invalid_selections() -> Result<(), BufferError> {
    let mut buffer = buffer("中", 8, 128, 8)?;
    let obsolete = patch(buffer.snapshot(), 0..0, "X")?;
    apply_insertion(&mut buffer, "y", OperationSource::Command, 1)?;
    let original = State::capture(&buffer)?;
    let before = buffer.selection().clone();
    let typing = patch(buffer.snapshot(), 0..0, "T")?;
    let old_base_command = patch(buffer.snapshot(), 0..0, "X")?;
    let typing_selection = (&before, SelectionSpec::new(1, 1));
    assert!(matches!(
        buffer.apply_staged_command::<BufferError>(&obsolete, typing_selection, 2, TEXT, |_, _| {
            Ok(None)
        }),
        Err(BufferError::RevisionMismatch)
    ));
    original.assert_matches(&buffer)?;
    assert!(matches!(
        buffer.apply_staged_command(&typing, typing_selection, 2, TEXT, |_, _| {
            command(old_base_command, 1)
        }),
        Err(BufferError::RevisionMismatch)
    ));
    original.assert_matches(&buffer)?;
    assert!(matches!(
        buffer.apply_staged_command(&typing, typing_selection, 2, TEXT, |candidate, _| {
            command(patch(candidate, 0..1, "X")?, 2)
        }),
        Err(BufferError::NotCharBoundary { offset: 2 })
    ));
    original.assert_matches(&buffer)?;
    let foreign = TextSnapshot::new("中y".to_owned(), TEXT)?;
    let foreign_selection = bind(&foreign, SelectionSpec::new(0, 0))?;
    assert!(matches!(
        buffer.apply_staged_command::<BufferError>(
            &typing,
            (&foreign_selection, SelectionSpec::new(1, 1)),
            2,
            TEXT,
            |_, _| Ok(None)
        ),
        Err(BufferError::SnapshotMismatch)
    ));
    original.assert_matches(&buffer)?;
    Ok(())
}

#[test]
fn staged_command_rejects_second_batch_line_and_text_budgets_atomically() -> Result<(), BufferError>
{
    let mut buffer = buffer("a", 8, 128, 1)?;
    let before = buffer.selection().clone();
    let typing = patch(buffer.snapshot(), 0..0, "T")?;
    let original = State::capture(&buffer)?;
    let typing_selection = (&before, SelectionSpec::new(1, 1));
    assert!(matches!(
        buffer.apply_staged_command(&typing, typing_selection, 2, TEXT, |candidate, _| {
            command(patch(candidate, 0..0, "\n")?, 1)
        }),
        Err(BufferError::LineCountExceeded { count: 2, limit: 1 })
    ));
    original.assert_matches(&buffer)?;
    assert!(matches!(
        buffer.apply_staged_command(
            &typing,
            typing_selection,
            2,
            TextBudget::new(2),
            |candidate, _| command(patch(candidate, 0..0, "Z")?, 1)
        ),
        Err(BufferError::BudgetExceeded {
            needed: 3,
            limit: 2
        })
    ));
    original.assert_matches(&buffer)?;
    Ok(())
}

fn buffer(
    text: &str,
    max_nodes: usize,
    history_bytes: usize,
    max_lines: usize,
) -> Result<StringBuffer, BufferError> {
    StringBuffer::new(
        text.to_owned(),
        SelectionSpec::new(text.len(), text.len()),
        TEXT,
        LineIndexBudget::new(max_lines),
        HistoryBudget::new(max_nodes, history_bytes),
    )
}

fn patch(
    snapshot: &TextSnapshot,
    range: Range<usize>,
    text: &str,
) -> Result<CheckedBatch, BufferError> {
    CheckedBatch::new(snapshot, vec![(range, text.to_owned())], PATCH)
}

fn bind(snapshot: &TextSnapshot, selected: SelectionSpec) -> Result<SrcSelection, BufferError> {
    SrcSelection::new(
        snapshot.src_offset(selected.anchor())?,
        snapshot.src_offset(selected.head())?,
    )
}

fn selection_spec(selection: &SrcSelection) -> SelectionSpec {
    SelectionSpec::new(
        selection.anchor().byte_offset(),
        selection.head().byte_offset(),
    )
}

fn assert_editor(
    buffer: &StringBuffer,
    text: &str,
    anchor: usize,
    head: usize,
) -> Result<(), BufferError> {
    assert_eq!(buffer.text(TEXT)?, text);
    assert_eq!(
        selection_spec(buffer.selection()),
        SelectionSpec::new(anchor, head)
    );
    Ok(())
}

fn command(
    batch: CheckedBatch,
    caret: usize,
) -> Result<Option<(CheckedBatch, SelectionSpec)>, BufferError> {
    Ok(Some((batch, SelectionSpec::new(caret, caret))))
}

fn apply_insertion(
    buffer: &mut StringBuffer,
    text: &str,
    source: OperationSource,
    time: u64,
) -> Result<(), BufferError> {
    let end = buffer.len_bytes();
    let batch = patch(buffer.snapshot(), end..end, text)?;
    let caret = end + text.len();
    let result = buffer.apply(&batch, SelectionSpec::new(caret, caret), source, time, TEXT)?;
    assert!(matches!(result, ApplyOutcome::Changed(_)));
    Ok(())
}

fn append_typing_and_command(
    buffer: &mut StringBuffer,
    typed: &str,
    inserted: &str,
    time: u64,
) -> Result<ApplyOutcome, BufferError> {
    let end = buffer.len_bytes();
    let before = buffer.selection().clone();
    let typing = patch(buffer.snapshot(), end..end, typed)?;
    let caret = end + typed.len();
    buffer.apply_staged_command(
        &typing,
        (&before, SelectionSpec::new(caret, caret)),
        time,
        TEXT,
        |candidate, _| {
            command(
                patch(candidate, caret..caret, inserted)?,
                caret + inserted.len(),
            )
        },
    )
}

struct State {
    text: String,
    selection: SrcSelection,
    revision: DocumentRevision,
    snapshot: SnapshotId,
    saved_revision: Option<DocumentRevision>,
    saved: bool,
    undo: usize,
    redo: usize,
    lines: Vec<Option<Range<usize>>>,
    history: Vec<Option<HistoryStepHandle>>,
}

impl State {
    fn capture(buffer: &StringBuffer) -> Result<Self, BufferError> {
        Ok(Self {
            text: buffer.text(TEXT)?.into_owned(),
            selection: buffer.selection().clone(),
            revision: buffer.revision(),
            snapshot: buffer.snapshot().id().clone(),
            saved_revision: buffer.saved_revision(),
            saved: buffer.is_at_save_point(),
            undo: buffer.undo_depth(),
            redo: buffer.redo_depth(),
            lines: (0..buffer.line_count())
                .map(|line| buffer.line_index().line_range(line))
                .collect(),
            history: (0..buffer.history_node_count())
                .map(|node| buffer.history_step(node))
                .collect(),
        })
    }

    fn assert_matches(&self, buffer: &StringBuffer) -> Result<(), BufferError> {
        let actual = Self::capture(buffer)?;
        assert_eq!(self.text, actual.text);
        assert_eq!(self.selection, actual.selection);
        assert_eq!(self.revision, actual.revision);
        assert_eq!(self.snapshot, actual.snapshot);
        assert_eq!(self.saved_revision, actual.saved_revision);
        assert_eq!(self.saved, actual.saved);
        assert_eq!(self.undo, actual.undo);
        assert_eq!(self.redo, actual.redo);
        assert_eq!(self.lines, actual.lines);
        assert_eq!(self.history.len(), actual.history.len());
        for (before, after) in self.history.iter().zip(actual.history.iter()) {
            assert!(
                matches!((before, after), (Some(before), Some(after)) if std::ptr::eq(before.get(), after.get()))
            );
        }
        Ok(())
    }
}
