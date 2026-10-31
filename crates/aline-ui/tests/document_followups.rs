//! 组 8 裁决后的手写金样：资源所有权、快照来源与实际状态入口。
mod common;

use aline_ui::{DocCore, DocError, EditorMode, ModeSwitchOutcome, ResourceReference};
use common::{
    TEXT, assert_history, assert_rejected_without_change, capture, collapsed, current_identity,
    limits, new_doc, selection, snapshot,
};

#[test]
fn document_initial_open_range_is_sorted_selection() -> Result<(), Box<dyn std::error::Error>> {
    let doc = new_doc("abcd\n", 4)?;
    assert_eq!(doc.open_block_range(), Some(4..4));
    let reverse = DocCore::new("abcd\n", selection(4, 1), limits())?;
    assert_eq!(reverse.open_block_range(), Some(1..4));
    assert_eq!(reverse.current_selection(), selection(4, 1));
    Ok(())
}

#[test]
fn document_snapshot_id_is_source_identity_not_content_or_version()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    let mut other = new_doc("x\n", 1)?;
    let initial = snapshot(&mut doc)?;
    assert_ne!(initial.snapshot_id(), snapshot(&mut other)?.snapshot_id());
    let old = current_identity(&mut doc)?;
    doc.type_text(1, "a", 0)?;
    doc.undo()?;
    let restored = snapshot(&mut doc)?;
    assert_eq!(initial.snapshot_id(), restored.snapshot_id());
    assert_ne!(initial.version(), restored.version());
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old),
        |error| matches!(error, DocError::StaleResult),
    )?;
    Ok(())
}

#[test]
fn document_resource_snapshot_freezes_nonempty_owned_references()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("![one](temp:one)\n", 0)?;
    let first = ResourceReference::new(doc.document_id().clone(), "temp:one");
    let second = ResourceReference::new(doc.document_id().clone(), "temp:two");
    let initial = current_identity(&mut doc)?;
    doc.retain_resource(&initial, first.clone(), 2)?;
    let frozen = snapshot(&mut doc)?;
    assert_eq!(frozen.resources(), std::slice::from_ref(&first));
    assert_eq!(frozen.version().draft_generation().value(), 1);
    assert_history(&doc, 0, 0, 0);
    assert_eq!(frozen.full_text(TEXT)?, "![one](temp:one)\n");
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.retain_resource(&initial, second.clone(), 2),
        |error| matches!(error, DocError::StaleResult),
    )?;
    let current = current_identity(&mut doc)?;
    doc.retain_resource(&current, second.clone(), 2)?;
    let latest = snapshot(&mut doc)?;
    assert_eq!(frozen.resources(), std::slice::from_ref(&first));
    assert_eq!(latest.resources(), &[first, second]);
    Ok(())
}

#[test]
fn document_resource_registration_rejects_wrong_document_and_budget()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    let other = new_doc("x\n", 1)?;
    let wrong = ResourceReference::new(other.document_id().clone(), "temp:wrong");
    let own = ResourceReference::new(doc.document_id().clone(), "temp:own");
    let identity = current_identity(&mut doc)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.retain_resource(&identity, wrong, 1),
        |error| matches!(error, DocError::WrongDocument),
    )?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.retain_resource(&identity, own.clone(), 0),
        |error| matches!(error, DocError::ResourceBudgetExceeded),
    )?;
    doc.retain_resource(&identity, own.clone(), 1)?;
    let current = current_identity(&mut doc)?;
    let before = capture(&mut doc)?;
    doc.retain_resource(&current, own, 1)?;
    assert_eq!(before, capture(&mut doc)?);
    Ok(())
}

#[test]
fn document_resource_lives_through_history_and_outlives_document()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    let resource = ResourceReference::new(doc.document_id().clone(), "temp:history");
    let weak = resource.downgrade();
    let identity = current_identity(&mut doc)?;
    doc.retain_resource(&identity, resource, 1)?;
    doc.type_text(1, "a", 0)?;
    doc.commit_open_block(1)?;
    doc.undo()?;
    doc.redo()?;
    let frozen = snapshot(&mut doc)?;
    assert_eq!(frozen.resources().len(), 1);
    doc.leave_block(2)?;
    drop(doc);
    assert!(weak.upgrade().is_some());
    assert_eq!(frozen.resources()[0].locator(), "temp:history");
    drop(frozen);
    assert!(weak.upgrade().is_none());
    Ok(())
}

#[test]
fn input_group_mode_switch_commits_only_pending_group() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    doc.type_text(1, "a", 0)?;
    assert_eq!(
        doc.switch_mode(EditorMode::Source, 100)?,
        ModeSwitchOutcome::Ready
    );
    assert_history(&doc, 1, 0, 1);
    assert_eq!(doc.buffer_text()?, "xa\n");
    let before = capture(&mut doc)?;
    doc.switch_mode(EditorMode::Visual, 200)?;
    let mut after = capture(&mut doc)?;
    assert_eq!(after.request.value(), before.request.value() + 1);
    after.request = before.request;
    after.mode = before.mode;
    assert_eq!(before, after);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "x\n");
    Ok(())
}

#[test]
fn input_group_mode_switch_after_cancel_and_rejection_preserves_state()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    doc.composition_update("中", 0)?;
    assert_eq!(
        doc.switch_mode(EditorMode::Source, 1)?,
        ModeSwitchOutcome::Deferred
    );
    doc.composition_cancel(2)?;
    let before = capture(&mut doc)?;
    doc.switch_mode(EditorMode::Source, 3)?;
    let mut after = capture(&mut doc)?;
    assert_eq!(after.request.value(), before.request.value() + 1);
    after.request = before.request;
    after.mode = before.mode;
    assert_eq!(before, after);
    doc.reload("z\n", collapsed(1), 4)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.switch_mode(EditorMode::Visual, 5).map(|_| ()),
        |error| matches!(error, DocError::DraftBaselineConflict),
    )?;
    Ok(())
}

#[test]
fn publication_wait_autosave_and_recovery_are_independent_reads()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    doc.composition_update("中", 0)?;
    let before = capture(&mut doc)?;
    assert!(doc.autosave_snapshot(TEXT)?.is_none());
    assert_eq!(doc.recovery_snapshot(TEXT)?.full_text(TEXT)?, "x中\n");
    assert_eq!(before, capture(&mut doc)?);
    doc.composition_cancel(1)?;
    let before = capture(&mut doc)?;
    let auto = doc.autosave_snapshot(TEXT)?;
    assert!(auto.is_some());
    if let Some(auto) = auto {
        assert_eq!(auto.full_text(TEXT)?, "x\n");
    }
    assert_eq!(before, capture(&mut doc)?);
    Ok(())
}

#[cfg(unix)]
#[test]
fn document_written_receipt_survives_stale_result_and_closed_target()
-> Result<(), Box<dyn std::error::Error>> {
    use aline_source::{
        AtomicFailureKind, Cancellation, DiskVersion, IoBudget, StorageBudget, StorageSnapshot,
        decode, encode, prepare_atomic,
    };
    let mut doc = new_doc("x\n", 1)?;
    let frozen = snapshot(&mut doc)?;
    let old = current_identity(&mut doc)?;
    let storage_budget = StorageBudget::new(1024);
    let storage = StorageSnapshot::new(b"x\n".to_vec(), DiskVersion::initial(), storage_budget)?;
    let baseline = decode(&storage, storage_budget)?.into_baseline();
    let encoded = encode(&frozen.full_text(TEXT)?, &baseline, storage_budget)?;
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let directory =
        std::env::temp_dir().join(format!("aline-ui-receipt-{}-{unique}", std::process::id()));
    std::fs::create_dir(&directory)?;
    let target = directory.join("new.md");
    let staging = directory.join("new.stage");
    let cancel = Cancellation::new();
    let prepared = prepare_atomic(&encoded, &target, &staging, IoBudget::new(1024), &cancel)?;
    doc.type_text(1, "a", 0)?;
    let written = prepared.publish_new(&cancel)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old),
        |error| matches!(error, DocError::StaleResult),
    )?;
    doc.leave_block(1)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old),
        |error| matches!(error, DocError::ClosedTarget),
    )?;
    assert_eq!(written.bytes(), b"x\n");
    assert_eq!(std::fs::read(&target)?, b"x\n");
    assert_eq!(doc.buffer_text()?, "xa\n");
    let before = capture(&mut doc)?;
    let failed = prepare_atomic(
        &encoded,
        &target,
        &directory.join("retry.stage"),
        IoBudget::new(1024),
        &cancel,
    )
    .and_then(|prepared| prepared.publish_new(&cancel));
    match failed {
        Err(error) => assert!(matches!(
            error.kind(),
            AtomicFailureKind::MetadataPreservationUnverified
        )),
        Ok(_) => return Err("existing target unexpectedly admitted".into()),
    }
    let saved = written.into_baseline(DiskVersion::initial().next()?)?;
    assert_eq!(saved.source(), "x\n");
    assert_eq!(before, capture(&mut doc)?);
    std::fs::remove_dir_all(directory)?;
    Ok(())
}

#[test]
fn document_snapshot_budget_rejection_without_draft_preserves_state()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("abcd\n", 4)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| {
            doc.current_snapshot(aline_buffer::TextBudget::new(4))
                .map(|_| ())
        },
        |error| {
            matches!(
                error,
                DocError::Buffer(aline_buffer::BufferError::BudgetExceeded { .. })
            )
        },
    )?;
    Ok(())
}

#[test]
fn document_close_reopen_rejects_old_result_even_when_text_and_version_match()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    let old = doc.begin_derived_request()?;
    doc.leave_block(0)?;
    doc.open_block(1..1, collapsed(1))?;
    let reopened = current_identity(&mut doc)?;
    assert_eq!(old.working_version(), reopened.working_version());
    assert_eq!(old.snapshot_id(), reopened.snapshot_id());
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old),
        |error| matches!(error, DocError::StaleResult),
    )?;
    doc.accept_derived_result(&reopened)?;
    Ok(())
}

#[test]
fn document_mode_roundtrip_rejects_old_result_without_text_or_history_change()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    let old = doc.begin_derived_request()?;
    let version = doc.version();
    doc.switch_mode(EditorMode::Source, 0)?;
    doc.switch_mode(EditorMode::Visual, 1)?;
    assert_eq!(doc.version(), version);
    assert_eq!(doc.buffer_text()?, "x\n");
    assert_history(&doc, 0, 0, 0);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old),
        |error| matches!(error, DocError::StaleResult),
    )?;
    Ok(())
}

#[test]
fn document_lifecycle_noop_deferred_and_rejections_keep_identity()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = new_doc("x\n", 1)?;
    let current = doc.begin_derived_request()?;
    let before = capture(&mut doc)?;
    assert_eq!(
        doc.switch_mode(EditorMode::Visual, 0)?,
        ModeSwitchOutcome::Ready
    );
    assert_eq!(before, capture(&mut doc)?);
    doc.accept_derived_result(&current)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.open_block(1..1, collapsed(1)),
        |error| matches!(error, DocError::BlockAlreadyOpen),
    )?;
    doc.composition_update("中", 1)?;
    let before = capture(&mut doc)?;
    assert_eq!(
        doc.switch_mode(EditorMode::Source, 2)?,
        ModeSwitchOutcome::Deferred
    );
    assert_eq!(before, capture(&mut doc)?);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.leave_block(3),
        |error| matches!(error, DocError::CompositionActive),
    )?;
    doc.composition_cancel(4)?;
    doc.leave_block(5)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.open_block(9..9, collapsed(9)),
        |error| matches!(error, DocError::Buffer(_)),
    )?;
    Ok(())
}

#[test]
fn input_group_lifecycle_history_budget_rejection_does_not_invalidate_requests()
-> Result<(), Box<dyn std::error::Error>> {
    let tight =
        aline_ui::DocLimits::new(TEXT, common::INDEX, aline_buffer::HistoryBudget::new(0, 0));
    let mut doc = DocCore::new("x\n", collapsed(1), tight)?;
    doc.type_text(1, "a", 0)?;
    let current = doc.begin_derived_request()?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.switch_mode(EditorMode::Source, 1).map(|_| ()),
        |error| matches!(error, DocError::Buffer(_)),
    )?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.leave_block(2),
        |error| matches!(error, DocError::Buffer(_)),
    )?;
    doc.accept_derived_result(&current)?;
    Ok(())
}
