//! M1 不开编辑块的文档导航、能力拒绝与结果采纳回归。

use std::error::Error;

use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, SrcSelection, TextBudget};
use aline_ui::{
    DocCore, DocError, DocLimits, DraftGeneration, EditorMode, InputIdentity, PublicationKind,
    PublicationOutcome, ResourceReference, WorkingVersion,
};

const TEXT: TextBudget = TextBudget::new(1024 * 1024);

fn limits() -> DocLimits {
    DocLimits::new(
        TEXT,
        LineIndexBudget::new(4096),
        HistoryBudget::new(128, 1024 * 1024),
    )
}

fn readonly() -> Result<DocCore, DocError> {
    DocCore::new_readonly(
        "# 中文\r\n\r\nread only",
        SelectionSpec::new(0, 0),
        limits(),
    )
}

#[derive(Debug, PartialEq)]
struct State {
    text: String,
    version: WorkingVersion,
    selection: SelectionSpec,
    mode: EditorMode,
    request: aline_ui::DerivedRequestId,
    config: aline_ui::ParseConfigGeneration,
    history: (usize, usize, usize),
    open: bool,
    draft: Option<String>,
    composition: bool,
    wait: Option<aline_ui::WaitView>,
    next_wait: u64,
    resources: usize,
    alive: bool,
}

fn state(doc: &DocCore) -> Result<State, DocError> {
    Ok(State {
        text: doc.buffer_text()?,
        version: doc.version(),
        selection: doc.current_selection(),
        mode: doc.mode(),
        request: doc.derived_request_id(),
        config: doc.parse_config_generation(),
        history: (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
        open: doc.has_open_block(),
        draft: doc.open_block_draft().map(str::to_owned),
        composition: doc.composition().is_some(),
        wait: doc.wait(),
        next_wait: doc.next_wait_id(),
        resources: doc.resources().len(),
        alive: doc.is_document_alive(),
    })
}

fn reject<T>(
    doc: &mut DocCore,
    action: impl FnOnce(&mut DocCore) -> Result<T, DocError>,
) -> Result<(), DocError> {
    let before = state(doc)?;
    assert!(matches!(action(doc), Err(DocError::ReadOnly)));
    assert_eq!(state(doc)?, before);
    Ok(())
}

#[test]
fn readonly_initialization_and_navigation_never_open_a_block() -> Result<(), Box<dyn Error>> {
    assert!(DocCore::new_readonly("中文", SelectionSpec::new(1, 1), limits()).is_err());
    let mut doc = readonly()?;
    assert!(doc.is_readonly());
    assert!(!doc.has_open_block());
    assert_eq!(
        (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
        (0, 0, 0)
    );
    let initial = doc.current_snapshot(TEXT)?;
    let identity = doc.begin_document_request()?;
    let selection =
        SrcSelection::new(initial.text().src_offset(2)?, initial.text().src_offset(8)?)?;
    let before = doc.version();
    doc.navigate_readonly(&identity, &selection)?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(2, 8));
    assert_eq!(doc.buffer_selection(), SelectionSpec::new(2, 8));
    assert_eq!(doc.current_snapshot(TEXT)?.selection(), &selection);
    assert_eq!(
        doc.current_snapshot(TEXT)?.snapshot_id(),
        initial.snapshot_id()
    );
    doc.switch_mode(EditorMode::Source, 0)?;
    doc.switch_mode(EditorMode::Visual, 1)?;
    assert_eq!(doc.version(), before);
    assert_eq!(doc.buffer_text()?, "# 中文\r\n\r\nread only");
    assert!(!doc.has_open_block());
    assert!(doc.open_block_draft().is_none());
    assert!(doc.composition().is_none());
    assert_eq!(doc.history_node_count(), 0);
    Ok(())
}

#[test]
fn readonly_rejects_every_edit_and_publication_entry_family() -> Result<(), Box<dyn Error>> {
    let mut doc = readonly()?;
    let identity = doc.current_input_identity()?;
    reject(&mut doc, |doc| {
        doc.open_block(0..0, SelectionSpec::new(0, 0))
    })?;
    reject(&mut doc, |doc| doc.set_selection(SelectionSpec::new(0, 0)))?;
    reject(&mut doc, |doc| doc.type_text(0, "x", 0))?;
    reject(&mut doc, |doc| doc.commit_open_block(0))?;
    reject(&mut doc, |doc| doc.leave_block(0))?;
    reject(&mut doc, |doc| doc.escape(0))?;
    reject(&mut doc, |doc| doc.composition_start("x", 0))?;
    reject(&mut doc, |doc| doc.composition_update("x", 0))?;
    reject(&mut doc, |doc| doc.composition_commit(0))?;
    reject(&mut doc, |doc| doc.composition_cancel(0))?;
    reject(&mut doc, DocCore::undo)?;
    reject(&mut doc, DocCore::redo)?;
    reject(&mut doc, |doc| {
        doc.reload("new", SelectionSpec::new(0, 0), 0)
    })?;
    reject(&mut doc, |doc| {
        doc.begin_publication_wait(PublicationKind::Save)
    })?;
    reject(&mut doc, |doc| doc.autosave_snapshot(TEXT))?;
    reject(&mut doc, |doc| doc.recovery_snapshot(TEXT))?;
    let resource = ResourceReference::new(doc.document_id().clone(), "fixture");
    reject(&mut doc, |doc| doc.retain_resource(&identity, resource, 1))?;

    let mut editor = DocCore::new("x", SelectionSpec::new(0, 0), limits())?;
    editor.composition_start("y", 0)?;
    let handle = editor.begin_publication_wait(PublicationKind::Save)?;
    reject(&mut doc, |doc| doc.request_cancel_wait(handle))?;
    reject(&mut doc, |doc| doc.cancel_wait(handle))?;
    reject(&mut doc, |doc| {
        doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 0)
    })?;
    reject(&mut doc, |doc| {
        doc.deliver(handle, PublicationOutcome::PlatformConfirmed, 0)
    })?;
    Ok(())
}

#[test]
fn readonly_navigation_rejects_foreign_snapshot_and_stale_request_atomically()
-> Result<(), Box<dyn Error>> {
    let mut doc = readonly()?;
    let identity = doc.begin_document_request()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let before = state(&doc)?;
    assert!(snapshot.text().src_offset(3).is_err());
    assert_eq!(state(&doc)?, before);
    let mut other = readonly()?;
    let foreign = other.current_snapshot(TEXT)?.selection().clone();
    let before = state(&doc)?;
    assert!(doc.navigate_readonly(&identity, &foreign).is_err());
    assert_eq!(state(&doc)?, before);
    doc.begin_document_request()?;
    let before = state(&doc)?;
    assert!(matches!(
        doc.navigate_readonly(&identity, snapshot.selection()),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, before);
    Ok(())
}

#[test]
fn readonly_rejects_a_valid_edit_plan_before_any_publication() -> Result<(), Box<dyn Error>> {
    use aline_buffer::PatchBudget;
    use aline_edit::{
        EditBudget, EditCommand, EditContext, EditMode, PlanOutcome, SourceEdit, plan_change,
    };
    use aline_markdown::{ParseBudget, ParseConfig, parse};

    let mut doc = readonly()?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let command = EditCommand::replace(vec![SourceEdit::new(0..0, "", "new")]);
    let outcome = plan_change(
        snapshot.text(),
        &parsed,
        snapshot.selection(),
        &EditContext::new(EditMode::Source),
        &command,
        EditBudget::new(PatchBudget::new(1, 1024, 1024), TEXT, ParseBudget::m0()),
    )?;
    let PlanOutcome::Changed(plan) = outcome else {
        return Err("expected a real edit plan".into());
    };
    reject(&mut doc, |doc| {
        doc.apply_change(&identity, &parsed, &plan, 0)
    })?;
    Ok(())
}

#[test]
fn readonly_document_results_do_not_require_an_edit_projection() -> Result<(), Box<dyn Error>> {
    let mut doc = readonly()?;
    let identity = doc.begin_document_request()?;
    let before = state(&doc)?;
    doc.accept_document_result(&identity)?;
    assert_eq!(state(&doc)?, before);
    assert!(matches!(
        doc.accept_derived_result(&identity),
        Err(DocError::ClosedTarget)
    ));
    assert_eq!(state(&doc)?, before);
    Ok(())
}

#[test]
fn document_result_keys_include_working_version_config_request_and_mode_roundtrip()
-> Result<(), Box<dyn Error>> {
    let mut doc = readonly()?;
    let identity = doc.begin_document_request()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let wrong_version = InputIdentity::for_request(
        doc.document_id().clone(),
        WorkingVersion::new(doc.version().revision(), DraftGeneration::from_value(99)),
        snapshot.text(),
        doc.parse_config_generation(),
        doc.derived_request_id(),
    )?;
    assert!(matches!(
        doc.accept_document_result(&wrong_version),
        Err(DocError::StaleResult)
    ));
    doc.advance_parse_config()?;
    assert!(matches!(
        doc.accept_document_result(&identity),
        Err(DocError::StaleResult)
    ));
    let identity = doc.begin_document_request()?;
    doc.begin_document_request()?;
    assert!(matches!(
        doc.accept_document_result(&identity),
        Err(DocError::StaleResult)
    ));
    let identity = doc.begin_document_request()?;
    doc.switch_mode(EditorMode::Source, 0)?;
    doc.switch_mode(EditorMode::Visual, 0)?;
    assert!(matches!(
        doc.accept_document_result(&identity),
        Err(DocError::StaleResult)
    ));
    assert_eq!(
        doc.current_snapshot(TEXT)?.snapshot_id(),
        snapshot.snapshot_id()
    );
    Ok(())
}

#[test]
fn closed_or_replaced_readonly_document_never_accepts_late_results() -> Result<(), Box<dyn Error>> {
    let mut doc = readonly()?;
    let old = doc.begin_document_request()?;
    doc.close_readonly()?;
    let before = state(&doc)?;
    assert!(matches!(
        doc.accept_document_result(&old),
        Err(DocError::ClosedTarget)
    ));
    assert!(matches!(
        doc.current_snapshot(TEXT),
        Err(DocError::ClosedTarget)
    ));
    assert!(matches!(
        doc.begin_document_request(),
        Err(DocError::ClosedTarget)
    ));
    assert_eq!(state(&doc)?, before);
    let mut replacement = readonly()?;
    let before = state(&replacement)?;
    assert!(matches!(
        replacement.accept_document_result(&old),
        Err(DocError::WrongDocument)
    ));
    assert_eq!(state(&replacement)?, before);
    Ok(())
}

#[test]
fn editing_constructor_and_closed_projection_check_remain_compatible() -> Result<(), Box<dyn Error>>
{
    let mut doc = DocCore::new("x", SelectionSpec::new(0, 0), limits())?;
    assert!(doc.has_open_block());
    assert!(!doc.is_readonly());
    let before = state(&doc)?;
    assert!(matches!(
        doc.close_readonly(),
        Err(DocError::RequiresReadonly)
    ));
    assert_eq!(state(&doc)?, before);
    let identity = doc.begin_derived_request()?;
    doc.leave_block(0)?;
    assert!(matches!(
        doc.accept_derived_result(&identity),
        Err(DocError::ClosedTarget)
    ));
    let document_identity = doc.begin_document_request()?;
    doc.accept_document_result(&document_identity)?;
    Ok(())
}

#[test]
fn document_target_rejects_old_working_version_even_when_snapshot_is_reused()
-> Result<(), Box<dyn Error>> {
    let mut doc = DocCore::new("x", SelectionSpec::new(0, 0), limits())?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let identity = doc.begin_document_request()?;
    doc.type_text(0, "a", 0)?;
    doc.undo()?;
    assert_eq!(
        doc.current_snapshot(TEXT)?.snapshot_id(),
        snapshot.snapshot_id()
    );
    let before = state(&doc)?;
    assert!(matches!(
        doc.accept_document_result(&identity),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, before);
    Ok(())
}
