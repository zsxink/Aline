//! W5 输入协议的独立无头回归：候选只更新组合草稿，平台结束回调只结算有效等待。

mod common;

use aline_buffer::SelectionSpec;
use aline_ui::{DocCore, DocError, PublicationKind, PublicationOutcome, WaitStatus};
use common::{TEXT, capture, limits};

fn document(text: &str, anchor: usize, head: usize) -> Result<DocCore, DocError> {
    DocCore::new(text, SelectionSpec::new(anchor, head), limits())
}

#[test]
fn native_input_candidate_table_replaces_the_same_source_range()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("abc\n", 3, 1)?;
    let candidates = [("候", "a候\n"), ("候选", "a候选\n"), ("選", "a選\n")];

    for (candidate, expected) in candidates {
        doc.composition_update(candidate, 10)?;
        let current = doc.current_snapshot(TEXT)?;
        assert_eq!(current.full_text(TEXT)?, expected);
        assert_eq!(doc.buffer_text()?, "abc\n");
        let composition = doc.composition().ok_or("composition missing")?;
        assert_eq!(composition.range(), 1..3);
        assert_eq!(composition.candidate(), candidate);
        assert_eq!(doc.undo_depth(), 0);
        assert_eq!(doc.redo_depth(), 0);
    }

    Ok(())
}

#[test]
fn native_input_duplicate_composition_end_callbacks_are_rejected_without_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("ab\n", 1, 1)?;
    doc.composition_update("中", 0)?;
    doc.composition_commit(1)?;

    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.composition_commit(2),
        Err(DocError::NoComposition)
    ));
    assert_eq!(before, capture(&mut doc)?);
    assert!(matches!(
        doc.composition_cancel(2),
        Err(DocError::NoComposition)
    ));
    assert_eq!(before, capture(&mut doc)?);

    Ok(())
}

#[test]
fn native_input_publication_callbacks_are_bound_to_one_wait_and_current_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("x\n", 1, 1)?;
    doc.composition_update("中", 0)?;
    let handle = doc.begin_publication_wait(PublicationKind::Save)?;

    let pending = capture(&mut doc)?;
    assert!(matches!(
        doc.begin_publication_wait(PublicationKind::SaveAs),
        Err(DocError::PublicationInFlight)
    ));
    assert_eq!(pending, capture(&mut doc)?);

    assert!(matches!(
        doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 1),
        Err(DocError::CompositionActive)
    ));
    assert_eq!(pending, capture(&mut doc)?);

    doc.composition_update("中国", 2)?;
    doc.composition_commit(3)?;
    let result = doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 4)?;
    assert_eq!(result.kind(), PublicationKind::Save);
    assert_eq!(result.outcome(), PublicationOutcome::PlatformConfirmed);
    assert_eq!(result.snapshot().full_text(TEXT)?, "x中国\n");
    assert_eq!(doc.wait(), None);

    let settled = capture(&mut doc)?;
    assert!(matches!(
        doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 5),
        Err(DocError::UnknownWait)
    ));
    assert_eq!(settled, capture(&mut doc)?);

    Ok(())
}

#[test]
fn native_input_cancelled_wait_rejects_late_platform_callback()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("x\n", 1, 1)?;
    doc.composition_update("中", 0)?;
    let handle = doc.begin_publication_wait(PublicationKind::SaveAs)?;
    doc.request_cancel_wait(handle)?;

    let cancelled = capture(&mut doc)?;
    assert_eq!(
        doc.wait().map(|wait| wait.status()),
        Some(WaitStatus::Cancelled)
    );
    assert!(matches!(
        doc.finish_publication(handle, PublicationOutcome::PlatformCancelled, 1),
        Err(DocError::UnknownWait)
    ));
    assert_eq!(cancelled, capture(&mut doc)?);
    assert_eq!(doc.composition().map(|view| view.candidate()), Some("中"));

    Ok(())
}

#[test]
fn native_input_natural_platform_cancel_returns_post_composition_state()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("xy\n", 2, 1)?;
    doc.composition_update("候", 0)?;
    let handle = doc.begin_publication_wait(PublicationKind::Export)?;
    doc.composition_cancel(1)?;

    let result = doc.finish_publication(handle, PublicationOutcome::PlatformCancelled, 2)?;
    assert_eq!(result.kind(), PublicationKind::Export);
    assert_eq!(result.outcome(), PublicationOutcome::PlatformCancelled);
    assert_eq!(result.snapshot().full_text(TEXT)?, "xy\n");
    assert_eq!(doc.buffer_text()?, "xy\n");
    assert_eq!(doc.undo_depth(), 0);
    assert_eq!(doc.wait(), None);

    Ok(())
}
