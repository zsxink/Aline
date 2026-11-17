//! 使用实际窗口输入意图消费文件会话，平台 callback 由锁定 handler 的 trait 方法接线。

use super::*;
use crate::{
    DocLimits, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenLimits, ReadonlySession,
};
use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use std::{error::Error, io::Cursor, path::Path};

const TEXT: TextBudget = TextBudget::new(4096);
type TestResult = Result<(), Box<dyn Error>>;

struct File(&'static str);
impl ReadonlyFileAccess for File {
    type Reader = Cursor<&'static [u8]>;

    fn open(&self, _: &Path) -> std::io::Result<ReadonlyFileInput<Self::Reader>> {
        Ok(ReadonlyFileInput::new(
            Cursor::new(self.0.as_bytes()),
            self.0.len() as u64,
            true,
        ))
    }
}

fn session(
    raw: &'static str,
    history: HistoryBudget,
    mode: EditorMode,
) -> Result<ReadonlySession, Box<dyn Error>> {
    let mut state = ReadonlySession::new(
        ReadonlyOpenLimits::new(
            StorageBudget::new(4096),
            DocLimits::new(TEXT, LineIndexBudget::new(128), history),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
        .with_editing(),
    );
    let request = state.request_open()?;
    let job = state
        .accept_selection(&request, Some(vec!["test.md".into()]))?
        .ok_or("没有工作项")?;
    state.accept_open(job.read_with(&File(raw)));
    let doc = state.document_mut().ok_or("没有文档")?;
    if mode == EditorMode::Visual {
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        assert!(doc.open_visual_at(&identity, current.selection())?);
    } else {
        doc.switch_mode(mode)?;
    }
    Ok(state)
}

fn view(doc: &mut ReadonlyDocument) -> Result<InputView, Box<dyn Error>> {
    let current = doc.current_snapshot()?;
    let len = current.len_bytes();
    let site = InputSite {
        mode: doc.core().mode(),
        source: 0..len,
        document_len: len,
        reader_row: current.text().src_range(0..len)?,
    };
    InputView::new(doc, site).ok_or_else(|| "没有正文输入投影".into())
}

fn apply(
    doc: &mut ReadonlyDocument,
    site: &InputView,
    edit: InputIntent<'_>,
) -> Result<(), DocError> {
    let current = doc.current_snapshot()?;
    edit.apply(site, &current, doc, 1)
}

#[test]
fn native_input_reports_source_entities_and_rejects_half_surrogates_and_graphemes() -> TestResult {
    let mut state = session(
        "👩‍💻 &amp; e\u{301}",
        HistoryBudget::new(8, 4096),
        EditorMode::Visual,
    )?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let site = view(doc)?;
    let bound = site.bound(&site.identity).ok_or("没有绑定")?;
    assert_eq!(bound.drawn_text(), "👩‍💻 &amp; e\u{301}");
    let entity = bound.source_from_utf16(&bound.utf16_range(6..11)?)?;
    assert_eq!(entity.byte_range(), 12..17);
    let before = doc.current_snapshot()?;
    for range in [1..1, 2..2, 5..7] {
        assert!(apply(doc, &site, InputIntent::Replace(Some(range), "x")).is_err());
    }
    let after = doc.current_snapshot()?;
    assert_eq!(before.version(), after.version());
    assert_eq!(before.selection(), after.selection());
    assert_eq!(before.full_text(TEXT)?, after.full_text(TEXT)?);
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn marked_update_replaces_candidate_and_commits_platform_confirmation_once() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(0..1))?;
    apply(doc, &initial, InputIntent::Mark(None, "n", Some(1..1)))?;
    let current = view(doc)?;
    assert!(matches!(
        apply(doc, &initial, InputIntent::Replace(None, "过期")),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.core().composition().ok_or("没有候选")?.candidate(), "n");
    apply(
        doc,
        &current,
        InputIntent::Mark(Some(0..1), "👋你", Some(2..3)),
    )?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(4, 7));
    assert_eq!(doc.core().history_node_count(), 0);
    let updated = view(doc)?;
    apply(doc, &updated, InputIntent::Replace(Some(0..3), "你好"))?;
    assert!(doc.core().composition().is_none());
    assert_eq!(doc.core().buffer_text()?, "你好b");
    assert_eq!(doc.core().history_node_count(), 1);
    assert!(matches!(
        apply(doc, &updated, InputIntent::Replace(None, "你好")),
        Err(DocError::StaleResult)
    ));
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(0, 1));
    Ok(())
}

#[test]
fn marked_cancel_restores_selection_and_failed_confirmation_keeps_mark() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(0, 0), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let site = view(doc)?;
    apply(doc, &site, InputIntent::Select(0..1))?;
    assert!(apply(doc, &site, InputIntent::Mark(None, "👋", Some(1..2))).is_err());
    assert!(doc.core().composition().is_none());
    assert!(!doc.core().has_open_block());
    apply(doc, &site, InputIntent::Mark(None, "ni", Some(2..2)))?;
    let updated = view(doc)?;
    let before = doc.current_snapshot()?;
    assert!(matches!(
        apply(doc, &updated, InputIntent::Replace(None, "你")),
        Err(DocError::Buffer(BufferError::HistoryBudgetExceeded { .. }))
    ));
    let after = doc.current_snapshot()?;
    assert_eq!(before.version(), after.version());
    assert_eq!(before.selection(), after.selection());
    assert_eq!(before.full_text(TEXT)?, after.full_text(TEXT)?);
    assert_eq!(
        doc.core().composition().ok_or("没有候选")?.candidate(),
        "ni"
    );
    apply(doc, &updated, InputIntent::Mark(Some(0..2), "", Some(0..0)))?;
    assert!(doc.core().composition().is_none());
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(0, 1));
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn same_version_source_retarget_invalidates_queries_and_writes_for_old_row() -> TestResult {
    let mut state = session("ab\ncd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(0..2)?;
    let site = InputSite::for_row(doc, row, None).ok_or("没有当前输入行")?;
    let old = InputView::new(doc, site).ok_or("没有正文输入投影")?;
    let caret = current.text().src_offset(4)?;
    let selection = SrcSelection::new(caret.clone(), caret)?;
    doc.edit_core()?.select(&old.identity, &selection)?;
    let newer = doc.current_snapshot()?;
    let identity = doc.input_identity()?;
    assert_eq!(old.identity, identity);
    assert!(old.active(&identity, &newer, EditorMode::Source).is_none());
    assert!(matches!(
        apply(doc, &old, InputIntent::Select(1..1)),
        Err(DocError::StaleResult)
    ));
    assert!(matches!(
        apply(doc, &old, InputIntent::Replace(None, "旧输入")),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(4, 4));
    assert_eq!(doc.core().buffer_text()?, "ab\ncd");
    Ok(())
}

#[test]
fn backspace_and_delete_remove_a_grapheme_and_restore_directional_selection_on_undo() -> TestResult
{
    let mut state = session(
        "a👩‍💻e\u{301}b",
        HistoryBudget::new(8, 4096),
        EditorMode::Source,
    )?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(
        doc,
        &initial,
        InputIntent::Move {
            forward: true,
            extend: false,
        },
    )?;
    apply(
        doc,
        &initial,
        InputIntent::Move {
            forward: true,
            extend: true,
        },
    )?;
    apply(doc, &initial, InputIntent::Delete { forward: false })?;
    assert_eq!(doc.core().buffer_text()?, "ae\u{301}b");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 12));
    let restored = view(doc)?;
    apply(
        doc,
        &restored,
        InputIntent::Move {
            forward: true,
            extend: false,
        },
    )?;
    apply(doc, &restored, InputIntent::Delete { forward: true })?;
    assert_eq!(doc.core().buffer_text()?, "a👩‍💻b");
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(12, 12));
    Ok(())
}

#[test]
fn backspace_inside_pending_typing_stays_in_the_same_group() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "👩‍💻"))?;
    let inserted = view(doc)?;
    apply(doc, &inserted, InputIntent::Delete { forward: false })?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "ab");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn composing_projection_can_process_natural_end_without_current_reader_row() -> TestResult {
    let mut state = session(
        "ab\ncd\nef",
        HistoryBudget::new(8, 4096),
        EditorMode::Source,
    )?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(0..2)?;
    let hidden_row = current.text().src_range(6..8)?;
    let site = InputSite::for_row(doc, row.clone(), None).ok_or("没有当前输入行")?;
    let view = InputView::new(doc, site).ok_or("没有正文输入投影")?;
    apply(doc, &view, InputIntent::Mark(None, "ni", Some(2..2)))?;
    let site = view.site.rebound(doc).ok_or("组合投影失效")?;
    let view = InputView::new(doc, site.clone()).ok_or("组合投影为空")?;
    let continuation = SourceInputSite {
        identity: view.identity.clone(),
        site,
    };
    assert!(InputSite::for_row(doc, hidden_row, Some(&continuation)).is_none());
    assert!(InputSite::for_row(doc, row.clone(), None).is_none());
    assert!(InputSite::for_row(doc, row, Some(&continuation)).is_some());
    apply(doc, &view, InputIntent::Mark(Some(0..2), "", Some(0..0)))?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "ab\ncd\nef");
    assert_eq!(doc.core().history_node_count(), 0);
    assert!(doc.core().composition().is_none());
    Ok(())
}

#[test]
fn escape_commits_pending_text_and_preserves_a_live_composition() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Replace(None, "x"))?;
    assert_eq!(doc.core().history_node_count(), 0);
    doc.edit_core()?.escape(1)?;
    assert!(!doc.core().has_open_block());
    assert_eq!(doc.core().buffer_text()?, "xab");
    assert_eq!(doc.core().history_node_count(), 1);

    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Mark(None, "ni", Some(2..2)))?;
    assert!(matches!(
        doc.edit_core()?.escape(2),
        Err(DocError::CompositionActive)
    ));
    assert_eq!(
        doc.core().composition().ok_or("没有候选")?.candidate(),
        "ni"
    );
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn identical_ime_confirmation_keeps_final_selection_after_mode_switch() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(0..1))?;
    apply(doc, &initial, InputIntent::Mark(None, "n", Some(1..1)))?;
    let marked = view(doc)?;
    apply(doc, &marked, InputIntent::Replace(None, "a"))?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.core().buffer_selection(), SelectionSpec::new(1, 1));
    doc.switch_mode(EditorMode::Visual)?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn line_break_and_following_input_share_the_source_site_until_commit() -> TestResult {
    let mut state = session("ab\ncd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(0..2)?;
    let site = InputSite::for_row(doc, row.clone(), None).ok_or("没有当前行")?;
    let initial = InputView::new(doc, site).ok_or("没有输入投影")?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::LineBreak)?;
    let site = initial.site.rebound(doc).ok_or("输入行失效")?;
    let continued = InputView::new(doc, site.clone()).ok_or("没有续接投影")?;
    let continuation = SourceInputSite {
        identity: continued.identity.clone(),
        site,
    };
    assert!(InputSite::for_row(doc, row, Some(&continuation)).is_some());
    apply(doc, &continued, InputIntent::Replace(None, "中"))?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "a\n中b\ncd");
    assert_eq!(doc.core().history_node_count(), 0);
    doc.edit_core()?.escape(2)?;
    assert_eq!(doc.core().buffer_text()?, "a\n中b\ncd");
    assert_eq!(doc.core().history_node_count(), 1);
    Ok(())
}

#[test]
fn deleting_all_of_a_replacement_draft_keeps_its_removal_at_commit() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(0..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "x"))?;
    let after = view(doc)?;
    apply(doc, &after, InputIntent::Delete { forward: false })?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "b");
    doc.edit_core()?.escape(2)?;
    assert_eq!(doc.core().buffer_text()?, "b");
    assert_eq!(doc.core().history_node_count(), 1);
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(0, 1));
    Ok(())
}
