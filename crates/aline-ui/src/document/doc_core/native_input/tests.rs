use super::*;
use crate::document::{PublicationKind, WaitStatus};
use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
use std::{error::Error, io, thread};

type TestResult = Result<(), Box<dyn Error>>;
const TEXT: TextBudget = TextBudget::new(1024);

fn document(text: &str, selection: SelectionSpec) -> Result<DocCore, DocError> {
    let mut doc = DocCore::new_editable(
        text,
        selection,
        super::super::DocLimits::new(TEXT, LineIndexBudget::new(32), HistoryBudget::new(8, 1024)),
    )?;
    doc.switch_mode(EditorMode::Source, 0)?;
    Ok(doc)
}

#[test]
fn native_delete_prepares_off_thread_and_restores_direction_on_undo() -> TestResult {
    let initial = SelectionSpec::new(8, 4);
    let mut doc = document("a中🙂x", initial)?;
    let identity = doc.current_input_identity()?;
    let current = doc.current_snapshot(TEXT)?;
    let job = doc
        .prepare_native_delete(&identity, &current.selection().range(), 1)?
        .ok_or("缺少删除准备")?;
    let prepared = thread::spawn(|| job.run())
        .join()
        .map_err(|_| io::Error::other("后台准备线程异常"))??;
    assert_eq!(doc.buffer_text()?, "a中🙂x");
    assert_eq!(doc.current_selection(), initial);
    assert_eq!(doc.undo_depth(), 0);

    assert!(matches!(
        doc.publish_native_input(prepared)?,
        CommitOutcome::Committed { .. }
    ));
    assert_eq!(doc.buffer_text()?, "a中x");
    assert_eq!(doc.current_selection(), SelectionSpec::new(4, 4));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "a中🙂x");
    assert_eq!(doc.current_selection(), initial);
    Ok(())
}

#[test]
fn native_staged_delete_publishes_two_steps_and_keeps_draft_before_completion() -> TestResult {
    let mut doc = document("甲乙", SelectionSpec::new(3, 3))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "🙂", 1)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let range = snapshot.text().src_range(0..3)?;
    let job = doc
        .prepare_native_delete(&identity, &range, 2)?
        .ok_or("缺少两步删除准备")?;
    let prepared = job.run()?;
    assert_eq!(doc.buffer_text()?, "甲乙");
    assert_eq!(doc.open_block_draft(), Some("🙂"));
    assert_eq!(doc.undo_depth(), 0);

    doc.publish_native_input(prepared)?;
    assert_eq!(doc.buffer_text()?, "🙂乙");
    assert_eq!(doc.undo_depth(), 2);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲🙂乙");
    assert_eq!(doc.current_selection(), SelectionSpec::new(7, 7));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲乙");
    assert_eq!(doc.current_selection(), SelectionSpec::new(3, 3));
    Ok(())
}

#[test]
fn prepared_typing_checks_900ms_and_refuses_after_new_draft() -> TestResult {
    let mut doc = document("abc", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 10)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let next = snapshot.selection().range();
    assert!(!doc.native_typing_commit_required(&identity, "2", 910)?);
    assert!(doc.native_typing_commit_required(&identity, "2", 911)?);
    let stale = doc
        .prepare_native_typing_commit(&identity, 911, Some(&next))?
        .ok_or("缺少封口准备")?
        .run()?;
    doc.type_selection(&identity, "2", 910)?;
    let version = doc.version();
    assert!(matches!(
        doc.publish_native_input(stale),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.version(), version);
    assert_eq!(doc.open_block_draft(), Some("12"));
    assert_eq!(doc.current_snapshot(TEXT)?.full_text(TEXT)?, "a12bc");
    assert_eq!(doc.buffer_text()?, "abc");
    assert_eq!(doc.undo_depth(), 0);
    Ok(())
}

#[test]
fn native_confirmation_preserves_cancelled_publication_request_and_commits_input() -> TestResult {
    let mut doc = document("前后", SelectionSpec::new(3, 3))?;
    let identity = doc.current_input_identity()?;
    doc.update_native_candidate(&identity, "未确定", None, 1)?;
    let wait = doc.begin_publication_wait(PublicationKind::Save)?;
    let identity = doc.current_input_identity()?;
    let prepared = doc
        .prepare_native_confirmation(&identity, Some("中"), 2)?
        .run()?;
    doc.cancel_wait(wait)?;
    assert_eq!(doc.composition().ok_or("缺少组合")?.candidate(), "未确定");
    assert_eq!(doc.undo_depth(), 0);

    doc.publish_native_input(prepared)?;
    assert_eq!(doc.buffer_text()?, "前中后");
    assert!(doc.composition().is_none());
    assert_eq!(doc.undo_depth(), 1);
    assert_eq!(
        doc.wait().ok_or("缺少已取消等待")?.status(),
        WaitStatus::Cancelled
    );
    Ok(())
}

#[test]
fn native_preparation_refuses_other_document_or_changed_selection() -> TestResult {
    let start = SelectionSpec::new(0, 1);
    let mut doc = document("ab", start)?;
    let identity = doc.current_input_identity()?;
    let current = doc.current_snapshot(TEXT)?;
    let range = current.selection().range();
    let foreign = doc
        .prepare_native_delete(&identity, &range, 1)?
        .ok_or("缺少准备")?
        .run()?;
    let stale = doc
        .prepare_native_delete(&identity, &range, 1)?
        .ok_or("缺少准备")?
        .run()?;
    let mut other = document("ab", start)?;
    assert!(matches!(
        other.publish_native_input(foreign),
        Err(DocError::WrongDocument)
    ));
    assert_eq!(other.buffer_text()?, "ab");
    assert_eq!(other.undo_depth(), 0);

    let reverse = bind_selection(current.text(), SelectionSpec::new(1, 0))?;
    doc.select(&identity, &reverse)?;
    assert_eq!(doc.version(), current.version());
    assert!(matches!(
        doc.publish_native_input(stale),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.current_selection(), SelectionSpec::new(1, 0));
    assert_eq!(doc.buffer_text()?, "ab");
    assert_eq!(doc.undo_depth(), 0);
    Ok(())
}
