//! 原生窗口同源协调器的无窗口回归；真实 chooser/菜单/焦点另作 macOS 实验。

use std::error::Error;
use std::io::{self, Cursor};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use aline_buffer::SelectionSpec;

use crate::EditorMode;
use crate::{ReadonlyFileAccess, ReadonlyFileInput};

use super::recovery_panel::{
    RecoveryPanelAction, RecoveryPanelEntryStatus, RecoveryPanelState, RecoveryPanelStep,
};
use super::recovery_runtime::production_recovery_with;
use crate::document::{
    RecoveredDocument, RecoveredPath, RecoveryDocumentId, RecoveryGenerationScan,
    RecoveryGenerationStatus, RecoveryLineageDisposition, RecoveryLineageScan, RecoveryScan,
    RestoredRecovery, WriteKind,
};
#[cfg(target_os = "macos")]
use crate::document::{
    RecoveredOpenError, RecoveryConfig, RecoveryDiscardCompletion, RecoveryGenerationInput,
    RecoveryPublishOutcome, RecoveryResourceInput, RecoveryRestoreCompletion, RecoveryRestoreError,
    RecoveryRoot, RecoveryStore, ResourceError, RetireOutcome, TempRoot, UnavailableResources,
};
#[cfg(target_os = "macos")]
use aline_source::Cancellation;
#[cfg(target_os = "macos")]
use std::sync::Arc;

struct Files<'a>(&'a [u8]);
impl<'a> ReadonlyFileAccess for Files<'a> {
    type Reader = Cursor<&'a [u8]>;
    fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        Ok(ReadonlyFileInput::new(
            Cursor::new(self.0),
            self.0.len() as u64,
            true,
        ))
    }
}

#[test]
fn dialog_cancel_and_failure_restore_focus_without_creating_a_document()
-> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let request = state.request_open()?;
    assert!(state.accept_dialog(&request, Ok(None)).is_none());
    assert!(state.take_focus_return());
    assert!(!state.take_focus_return());
    assert!(state.feedback_key().is_none());
    assert!(state.session().document().is_none());
    let request = state.request_open()?;
    assert!(state.accept_dialog(&request, Err(())).is_none());
    assert_eq!(state.feedback_key(), Some("open_error.dialog"));
    assert!(state.take_focus_return());
    assert!(!state.session().is_loading());
    Ok(())
}

#[test]
fn chooser_extension_rejection_is_localized_and_never_starts_a_reader() -> Result<(), Box<dyn Error>>
{
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let request = state.request_open()?;
    assert!(
        state
            .accept_dialog(&request, Ok(Some(vec![PathBuf::from("a.txt")])))
            .is_none()
    );
    assert_eq!(state.feedback_key(), Some("open_error.extension"));
    assert!(state.take_focus_return());
    assert!(!state.session().is_loading());
    Ok(())
}

#[test]
fn one_running_read_and_one_latest_queued_read_keep_replacements_bounded()
-> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let a = state.request_open()?;
    let running = state
        .accept_dialog(&a, Ok(Some(vec![PathBuf::from("a.md")])))
        .ok_or("missing A")?;
    let b = state.request_open()?;
    assert!(
        state
            .accept_dialog(&b, Ok(Some(vec![PathBuf::from("b.MD")])))
            .is_none()
    );
    let c = state.request_open()?;
    assert!(
        state
            .accept_dialog(&c, Ok(Some(vec![PathBuf::from("c.md")])))
            .is_none()
    );
    let (next, _) = state.read_finished(running.read_with(&Files(b"A")), |_| Ok(()));
    let latest = next.ok_or("missing latest queued read")?;
    let (next, _) = state.read_finished(latest.read_with(&Files(b"C")), |_| Ok(()));
    assert!(next.is_none());
    let document = state.session().document().ok_or("missing C")?;
    assert_eq!(document.core().buffer_text()?, "C");
    assert_eq!(document.path(), Path::new("c.md"));
    assert!(!document.core().has_open_block());
    assert_eq!(document.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn close_and_stale_dialog_never_restore_focus_or_revive_ready_state() -> Result<(), Box<dyn Error>>
{
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let old = state.request_open()?;
    let latest = state.request_open()?;
    assert!(state.accept_dialog(&old, Ok(None)).is_none());
    assert!(!state.take_focus_return());
    let job = state
        .accept_dialog(&latest, Ok(Some(vec![PathBuf::from("a.md")])))
        .ok_or("missing A")?;
    state.take_focus_return();
    state.close_window();
    let (next, _) = state.read_finished(job.read_with(&Files(b"A")), |_| Ok(()));
    assert!(next.is_none());
    assert!(state.session().document().is_none());
    assert!(!state.take_focus_return());
    assert!(state.request_open().is_err());
    Ok(())
}

#[test]
fn production_window_entry_remains_compiled() -> Result<(), Box<dyn Error>> {
    let _ = super::open_initial_window;
    for locale in [aline_app_core::Locale::ZhCn, aline_app_core::Locale::En] {
        let translator = aline_app_core::Translator::embedded(locale)?;
        assert!(super::NativeWindowCopy::new(translator).is_ok());
    }
    Ok(())
}

#[test]
fn newly_opened_large_document_defaults_source_with_specific_reason() -> Result<(), Box<dyn Error>>
{
    let bytes = "中".repeat((4 * 1024 * 1024) / 3 + 1).into_bytes();
    assert!(bytes.len() > 4 * 1024 * 1024);
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let request = state.request_open()?;
    let job = state
        .accept_dialog(&request, Ok(Some(vec![PathBuf::from("large.md")])))
        .ok_or("job")?;
    state.read_finished(job.read_with(&Files(&bytes)), |_| Ok(()));
    let document = state.session().document().ok_or("document")?;
    assert_eq!(document.core().mode(), EditorMode::Source);
    assert_eq!(state.feedback_key(), Some("reading.large_source"));
    assert_eq!(super::parse_fallback_key(document.parsed()), None);
    assert_eq!(document.core().history_node_count(), 0);
    assert!(!document.core().has_open_block());
    Ok(())
}

#[test]
fn layout_failures_distinguish_budget_from_internal_errors() {
    use super::layout_error_key;
    use super::readonly_layout::LayoutError;
    for error in [
        LayoutError::Budget,
        LayoutError::ItemBytes {
            needed: 2,
            limit: 1,
        },
        LayoutError::ItemNodes {
            needed: 2,
            limit: 1,
        },
        LayoutError::TableColumns {
            needed: 2,
            limit: 1,
        },
    ] {
        assert_eq!(layout_error_key(error), "open_error.budget");
    }
    for error in [
        LayoutError::InvalidEnvironment,
        LayoutError::InvalidRange,
        LayoutError::StaleInput,
        LayoutError::GenerationExhausted,
    ] {
        assert_eq!(layout_error_key(error), "open_error.internal");
    }
}

fn limits_with_parse_budget(parse: aline_markdown::ParseBudget) -> crate::ReadonlyOpenLimits {
    crate::ReadonlyOpenLimits::new(
        aline_source::StorageBudget::new(super::MAX_DOCUMENT_BYTES),
        crate::DocLimits::new(
            aline_buffer::TextBudget::new(super::MAX_DOCUMENT_BYTES),
            aline_buffer::LineIndexBudget::new(1_000_000),
            aline_buffer::HistoryBudget::new(0, 0),
        ),
        aline_markdown::ParseConfig::m0(),
        parse,
    )
}

#[test]
fn failed_reader_preparation_keeps_the_published_document_and_mode() -> Result<(), Box<dyn Error>> {
    use super::readonly_layout::LayoutError;

    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let first = state.request_open()?;
    let job = state
        .accept_dialog(&first, Ok(Some(vec![PathBuf::from("a.md")])))
        .ok_or("job A")?;
    let (next, prepared) = state.read_finished(job.read_with(&Files(b"a\n\nb")), |_| Ok(()));
    assert!(next.is_none());
    assert_eq!(prepared, Some(()));
    let stable_id = state
        .session()
        .document()
        .ok_or("A")?
        .core()
        .document_id()
        .clone();
    state
        .session
        .document_mut()
        .ok_or("A")?
        .switch_mode(EditorMode::Source)?;

    let second = state.request_open()?;
    let job = state
        .accept_dialog(&second, Ok(Some(vec![PathBuf::from("b.md")])))
        .ok_or("job B")?;
    let (next, prepared) = state.read_finished(job.read_with(&Files(b"c\nd")), |_| {
        Err::<(), LayoutError>(LayoutError::Budget)
    });
    assert!(next.is_none());
    assert!(prepared.is_none());
    assert_eq!(state.feedback_key(), Some("open_error.budget"));
    assert!(!state.session().is_loading());
    let document = state.session().document().ok_or("A")?;
    assert_eq!(document.core().document_id(), &stable_id);
    assert_eq!(document.path(), Path::new("a.md"));
    assert_eq!(document.core().mode(), EditorMode::Source);
    assert_eq!(document.core().buffer_text()?, "a\n\nb");
    Ok(())
}

#[test]
fn first_open_preparation_failure_reports_without_publishing_a_document()
-> Result<(), Box<dyn Error>> {
    use super::readonly_layout::LayoutError;

    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let request = state.request_open()?;
    let job = state
        .accept_dialog(&request, Ok(Some(vec![PathBuf::from("a.md")])))
        .ok_or("job")?;
    let (next, prepared) = state.read_finished(job.read_with(&Files(b"# t")), |_| {
        Err::<(), LayoutError>(LayoutError::ItemBytes {
            needed: 2,
            limit: 1,
        })
    });
    assert!(next.is_none());
    assert!(prepared.is_none());
    assert!(state.session().document().is_none());
    assert!(!state.session().is_loading());
    assert_eq!(state.feedback_key(), Some("open_error.budget"));
    Ok(())
}

#[test]
fn stale_completion_never_runs_reader_preparation() -> Result<(), Box<dyn Error>> {
    use super::readonly_layout::LayoutError;

    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let first = state.request_open()?;
    let job = state
        .accept_dialog(&first, Ok(Some(vec![PathBuf::from("a.md")])))
        .ok_or("job")?;
    let _ = state.request_open()?;
    let mut prepared = false;
    let (next, result) = state.read_finished(job.read_with(&Files(b"a")), |_| {
        prepared = true;
        Ok::<(), LayoutError>(())
    });
    assert!(!prepared);
    assert!(next.is_none());
    assert!(result.is_none());
    assert!(state.session().document().is_none());
    Ok(())
}

#[test]
fn parse_budget_fallback_opens_source_with_a_localized_reason() -> Result<(), Box<dyn Error>> {
    let limits = limits_with_parse_budget(aline_markdown::ParseBudget::m0().with_max_nodes(1));
    let mut state = super::NativeOpenCoordinator::new(limits);
    let request = state.request_open()?;
    let job = state
        .accept_dialog(&request, Ok(Some(vec![PathBuf::from("fallback.md")])))
        .ok_or("job")?;
    let (next, prepared) =
        state.read_finished(job.read_with(&Files(b"# heading\n\nbody\n")), |_| Ok(()));
    assert!(next.is_none());
    assert_eq!(prepared, Some(()));
    let document = state.session().document().ok_or("document")?;
    assert_eq!(document.core().mode(), EditorMode::Source);
    assert_eq!(
        super::parse_fallback_key(document.parsed()),
        Some("reading.parse_budget_fallback")
    );
    assert_eq!(document.core().buffer_text()?, "# heading\n\nbody\n");
    assert!(!document.core().has_open_block());
    Ok(())
}

#[test]
fn parse_fallback_messages_cover_both_diagnostic_kinds() {
    use aline_markdown::DiagnosticKind;

    assert_eq!(
        super::parse_fallback_message(DiagnosticKind::BudgetExceeded),
        "reading.parse_budget_fallback"
    );
    assert_eq!(
        super::parse_fallback_message(DiagnosticKind::RawFallback),
        "reading.parse_raw_fallback"
    );
}

fn find_fixture(text: &[u8], editable: bool) -> Result<crate::ReadonlySession, Box<dyn Error>> {
    use crate::{DocLimits, ReadonlyOpenLimits, ReadonlySession};
    use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
    use aline_markdown::{ParseBudget, ParseConfig};
    use aline_source::StorageBudget;

    let bytes = 64 * 1024;
    let limits = ReadonlyOpenLimits::new(
        StorageBudget::new(bytes),
        DocLimits::new(
            TextBudget::new(bytes),
            LineIndexBudget::new(1024),
            HistoryBudget::new(32, bytes),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    );
    let mut session = ReadonlySession::new(if editable {
        limits.with_editing()
    } else {
        limits
    });
    let request = session.request_open()?;
    let read = session
        .accept_selection(&request, Some(vec![PathBuf::from("find.md")]))?
        .ok_or("open job")?;
    assert!(matches!(
        session.accept_open(read.read_with(&Files(text))),
        crate::OpenDisposition::Applied
    ));
    Ok(session)
}

#[test]
fn find_query_generation_and_working_version_reject_late_results() -> Result<(), Box<dyn Error>> {
    use super::find_panel::{FindDirection, SearchState};
    use aline_edit::{EditRefusal, FindCase};
    use gpui::SharedString;

    let mut session = find_fixture(b"Demo demo", true)?;
    let doc = session.document_mut().ok_or("document")?;
    let snapshot = doc.current_snapshot()?;
    let mut search = SearchState::default();
    let query = SharedString::from("demo");
    let old = search
        .submit(Some(snapshot.clone()), query.clone(), FindCase::Insensitive)
        .ok_or("old")?;
    search.query_changed(false);
    let (key, results) = search
        .submit(Some(snapshot.clone()), query.clone(), FindCase::Sensitive)
        .ok_or("new")?
        .run();
    assert!(search.complete(&key, results));
    let (old_key, old_results) = old.run();
    assert!(!search.complete(&old_key, old_results));
    let hit = search.target(
        &snapshot,
        query.clone(),
        FindCase::Sensitive,
        FindDirection::Next,
    )?;
    assert_eq!(hit.range().byte_range(), 5..9);
    assert!(search.accept_target(&hit));

    let identity = doc.input_identity()?;
    doc.edit_core()?.replace_selection(&identity, "!", 1)?;
    let changed = doc.current_snapshot()?;
    assert_ne!(changed.version(), snapshot.version());
    assert!(matches!(
        search.target(
            &changed,
            query.clone(),
            FindCase::Sensitive,
            FindDirection::Next
        ),
        Err(EditRefusal::StaleFindResults)
    ));
    let latest = search
        .submit(Some(changed), query, FindCase::Sensitive)
        .ok_or("latest")?;
    assert!(!search.accept_target(&hit));
    let (key, results) = latest.run();
    assert!(search.complete(&key, results));
    Ok(())
}

#[test]
fn find_same_matches_across_modes_and_replacement_field_change() -> Result<(), Box<dyn Error>> {
    use super::find_panel::{FindDirection, SearchState};
    use aline_edit::FindCase;
    use gpui::SharedString;

    let mut session = find_fixture(b"cat cat", false)?;
    let doc = session.document_mut().ok_or("document")?;
    let snapshot = doc.current_snapshot()?;
    let query = SharedString::from("cat");
    let case = FindCase::Sensitive;
    let mut search = SearchState::default();
    let (key, results) = search
        .submit(Some(snapshot.clone()), query.clone(), case)
        .ok_or("search")?
        .run();
    assert!(search.complete(&key, results));
    let last = search.target(&snapshot, query.clone(), case, FindDirection::Previous)?;
    assert_eq!(last.range().byte_range(), 4..7);
    assert!(search.accept_target(&last));
    doc.switch_mode(EditorMode::Source)?;
    let source = doc.current_snapshot()?;
    assert!(
        search
            .submit(Some(source.clone()), query.clone(), case)
            .is_none()
    );

    search.query_changed(true);
    assert!(!search.accept_target(&last));
    let (key, results) = search
        .submit(Some(source.clone()), query.clone(), case)
        .ok_or("replacement refresh")?
        .run();
    assert!(search.complete(&key, results));
    let next = search.target(&source, query, case, FindDirection::Next)?;
    assert_eq!(next.range().byte_range(), 0..3);
    Ok(())
}

#[test]
fn find_hidden_protected_and_visible_targets_choose_correct_reveal_mode()
-> Result<(), Box<dyn Error>> {
    use super::find_panel::reveal_in_source;

    let text = "中 [label](target)\n\n<!-- marker -->\n\nx &amp; y";
    let session = find_fixture(text.as_bytes(), false)?;
    let parsed = session.document().ok_or("document")?.parsed();
    for (query, source_expected) in [
        ("中", false),
        ("label", false),
        ("target", true),
        ("marker", true),
        ("amp", true),
        ("&amp;", false),
    ] {
        let start = text.find(query).ok_or("fixture range")?;
        let range = parsed.snapshot().src_range(start..start + query.len())?;
        assert_eq!(reveal_in_source(parsed, &range), source_expected, "{query}");
    }
    Ok(())
}

#[test]
fn find_replace_all_rejects_stale_query_and_keeps_one_undo_step() -> Result<(), Box<dyn Error>> {
    use super::find_panel::replace_all_for_test;
    use crate::{CommitOutcome, DocError};
    use aline_edit::EditRefusal;

    let mut session = find_fixture(b"cat\n\ncat", true)?;
    let doc = session.document_mut().ok_or("document")?;
    let initial = doc.input_identity()?;
    let before = doc.core().current_selection();
    assert!(matches!(
        replace_all_for_test(doc, "cat".into(), "dog".into(), 1),
        Err(DocError::Edit(EditRefusal::StaleFindResults))
    ));
    assert_eq!(doc.input_identity()?, initial);
    assert_eq!(doc.core().current_selection(), before);
    assert_eq!(doc.core().undo_depth(), 0);
    assert_eq!(doc.core().buffer_text()?, "cat\n\ncat");
    assert!(matches!(
        replace_all_for_test(doc, "cat".into(), "dog".into(), 0)?,
        CommitOutcome::Committed { .. }
    ));
    assert_eq!(doc.core().buffer_text()?, "dog\n\ndog");
    assert_eq!(doc.core().undo_depth(), 1);
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().buffer_text()?, "cat\n\ncat");
    Ok(())
}

#[test]
fn find_panel_catalogs_cover_controls_status_and_count() -> Result<(), Box<dyn Error>> {
    for locale in [aline_app_core::Locale::ZhCn, aline_app_core::Locale::En] {
        let translator = aline_app_core::Translator::embedded(locale)?;
        for key in [
            "action.find",
            "find.query",
            "find.replacement",
            "find.previous",
            "find.next",
            "find.case_sensitive",
            "find.replace_current",
            "find.replace_all",
            "find.close",
            "find.empty",
            "find.no_matches",
            "find.no_document",
            "find.readonly",
            "find.pending",
            "find.failed",
            "find.refused",
        ] {
            assert!(!translator.t(key)?.is_empty());
        }
        let count = translator.t_with("find.count", &[("current", "2"), ("total", "3")])?;
        assert!(count.contains('2') && count.contains('3'));
    }
    Ok(())
}

/// 本文件内的临时保存目录；不跨模块导出，Drop 时递归清理。
#[cfg(target_os = "macos")]
struct WindowSaveDir(PathBuf);

#[cfg(target_os = "macos")]
impl WindowSaveDir {
    /// 在系统临时目录下独占创建本测试使用的目录。
    fn create(prefix: &str) -> Result<Self, Box<dyn Error>> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for attempt in 0..100u32 {
            let candidate = std::env::temp_dir().join(format!(
                "{prefix}-{}-{nonce:x}-{attempt}",
                std::process::id()
            ));
            match std::fs::create_dir(&candidate) {
                Ok(()) => return Ok(Self(candidate)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("无法创建保存测试目录".into())
    }
}

#[cfg(target_os = "macos")]
impl Drop for WindowSaveDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 用 `Files` 适配器按给定字节打开绝对路径上的可编辑文档。
#[cfg(target_os = "macos")]
fn open_window_document(
    state: &mut super::NativeOpenCoordinator,
    path: &Path,
    bytes: &[u8],
) -> Result<(), Box<dyn Error>> {
    let request = state.request_open()?;
    let job = state
        .accept_dialog(&request, Ok(Some(vec![path.to_path_buf()])))
        .ok_or("应生成打开工作")?;
    let (next, prepared) = state.read_finished(job.read_with(&Files(bytes)), |_| Ok(()));
    assert!(next.is_none());
    assert_eq!(prepared, Some(()));
    // 打开成功本身会请求回焦；先消费，保存用例只验证保存处理器不新增。
    assert!(state.take_focus_return(), "打开成功应请求回焦");
    Ok(())
}

/// 在全文范围打开旧式输入块；无窗口可编辑打开不会自动打开块。
#[cfg(target_os = "macos")]
fn open_window_block(
    state: &mut super::NativeOpenCoordinator,
    selection: SelectionSpec,
) -> Result<(), Box<dyn Error>> {
    let document = state.session.document_mut().ok_or("应存在可编辑文档")?;
    let len = document.baseline().source().len();
    document.edit_core()?.open_block(0..len, selection)?;
    Ok(())
}

/// 在打开块的逻辑偏移处输入普通文本。
#[cfg(target_os = "macos")]
fn window_type(
    state: &mut super::NativeOpenCoordinator,
    at: usize,
    text: &str,
) -> Result<(), Box<dyn Error>> {
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .type_text(at, text, 0)?;
    Ok(())
}

/// 保存前后须逐项保持一致的文档可见状态。
#[cfg(target_os = "macos")]
#[derive(Debug, Eq, PartialEq)]
struct WindowSaveView {
    text: String,
    selection: SelectionSpec,
    version: crate::WorkingVersion,
    undo_depth: usize,
    history_nodes: usize,
}

#[cfg(target_os = "macos")]
fn window_save_view(
    state: &mut super::NativeOpenCoordinator,
) -> Result<WindowSaveView, Box<dyn Error>> {
    let document = state.session.document_mut().ok_or("应存在可编辑文档")?;
    let snapshot = document.current_snapshot()?;
    Ok(WindowSaveView {
        text: snapshot.full_text(aline_buffer::TextBudget::new(super::MAX_DOCUMENT_BYTES))?,
        selection: document.core().current_selection(),
        version: snapshot.version(),
        undo_depth: document.core().undo_depth(),
        history_nodes: document.core().history_node_count(),
    })
}

#[cfg(target_os = "macos")]
#[test]
fn window_save_publishes_and_reports_saved() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-save")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;

    let before = window_save_view(&mut state)?;
    let job = state.request_save().ok_or("首次保存应返回后台工作")?;
    assert_eq!(state.feedback_key(), None);
    let completion = job.run();
    assert_eq!(
        state.settle_save(completion).feedback,
        Some("document.save_saved")
    );
    assert_eq!(fs::read(&target)?, b"xa\n");
    let entries = fs::read_dir(&directory.0)?.collect::<Result<Vec<_>, io::Error>>()?;
    assert_eq!(entries.len(), 1, "成功保存后目录不应残留 staging");
    assert_eq!(entries[0].file_name(), "notes.md");
    assert_eq!(
        window_save_view(&mut state)?,
        before,
        "保存不得改变文档可见状态"
    );
    assert!(!state.take_focus_return(), "保存不得设置回焦标记");
    assert_eq!(
        state.session.close_document(),
        crate::CloseDisposition::Closed
    );

    // 成功保存后重新打开同一路径，继续输入并再次保存。
    open_window_document(&mut state, &target, b"xa\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 2, "b")?;
    let job = state.request_save().ok_or("成功保存后队列应可复用")?;
    let completion = job.run();
    assert_eq!(
        state.settle_save(completion).feedback,
        Some("document.save_saved")
    );
    assert_eq!(fs::read(&target)?, b"xab\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_save_while_composing_waits_and_cancel_preserves_input() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-waiting")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 1))?;
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .composition_start("候", 0)?;

    let before = window_save_view(&mut state)?;
    assert!(state.request_save().is_none(), "组合期保存应转入等待");
    assert!(state.save_waiting());
    assert_eq!(state.feedback_key(), Some("document.save_waiting"));

    assert!(state.cancel_save_wait());
    assert!(!state.save_waiting());
    assert_eq!(state.feedback_key(), Some("document.save_cancelled"));

    let document = state.session.document_mut().ok_or("应存在可编辑文档")?;
    assert!(
        document.core().composition().is_some(),
        "取消等待不得结束组合"
    );
    assert_eq!(
        document.core().wait().map(|wait| wait.status()),
        Some(crate::WaitStatus::Cancelled)
    );
    assert_eq!(
        window_save_view(&mut state)?,
        before,
        "取消等待不得改变输入状态"
    );
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .composition_cancel(2)?;
    assert!(!state.take_focus_return(), "等待与取消不得设置回焦标记");
    assert_eq!(fs::read(&target)?, b"x\n", "取消等待不得写盘");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_natural_platform_finish_saves_latest_snapshot() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let cases: [(crate::PublicationOutcome, &[u8]); 2] = [
        (
            crate::PublicationOutcome::PlatformConfirmed,
            "候y\n".as_bytes(),
        ),
        (crate::PublicationOutcome::PlatformCancelled, b"x\n"),
    ];
    for (outcome, expected) in cases {
        let directory = WindowSaveDir::create("aline-window-natural")?;
        let target = directory.0.join("notes.md");
        fs::write(&target, b"x\n")?;
        let mut state = super::NativeOpenCoordinator::new(super::open_limits());
        open_window_document(&mut state, &target, b"x\n")?;
        // 组合插入使用当前选区；选中首个字符后确认才能得到 `候y\n`。
        open_window_block(&mut state, SelectionSpec::new(0, 1))?;
        {
            let core = state
                .session
                .document_mut()
                .ok_or("应存在可编辑文档")?
                .edit_core()?;
            core.composition_start("候", 0)?;
            core.composition_update("候y", 1)?;
        }
        assert!(state.request_save().is_none(), "组合期保存应转入等待");
        assert!(state.save_waiting());
        {
            let core = state
                .session
                .document_mut()
                .ok_or("应存在可编辑文档")?
                .edit_core()?;
            match outcome {
                crate::PublicationOutcome::PlatformConfirmed => {
                    core.composition_commit(2)?;
                }
                crate::PublicationOutcome::PlatformCancelled => {
                    core.composition_cancel(2)?;
                }
            }
        }
        let Some(super::SaveIntent::Save(job)) = state.finish_save_wait_intent(outcome, 3) else {
            return Err("平台自然结束应继续保存".into());
        };
        let completion = job.run();
        assert_eq!(
            state.settle_save(completion).feedback,
            Some("document.save_saved")
        );
        assert_eq!(fs::read(&target)?, expected);
        assert!(
            state.finish_save_wait_intent(outcome, 4).is_none(),
            "等待句柄不得复用"
        );
    }
    Ok(())
}

/// 生产接线 `settle_after_input` 经 `finish_save_wait_intent` 结算等待；
/// 这里固定该入口在组合自然结束后继续保存最新快照。
#[cfg(target_os = "macos")]
#[test]
fn window_composition_end_intent_settles_wait_and_saves_latest_snapshot()
-> Result<(), Box<dyn Error>> {
    use std::fs;

    let cases: [(crate::PublicationOutcome, &[u8]); 2] = [
        (
            crate::PublicationOutcome::PlatformConfirmed,
            "候y\n".as_bytes(),
        ),
        (crate::PublicationOutcome::PlatformCancelled, b"x\n"),
    ];
    for (outcome, expected) in cases {
        let directory = WindowSaveDir::create("aline-window-intent")?;
        let target = directory.0.join("notes.md");
        fs::write(&target, b"x\n")?;
        let mut state = super::NativeOpenCoordinator::new(super::open_limits());
        open_window_document(&mut state, &target, b"x\n")?;
        // 组合插入使用当前选区；选中首个字符后确认才能得到 `候y\n`。
        open_window_block(&mut state, SelectionSpec::new(0, 1))?;
        {
            let core = state
                .session
                .document_mut()
                .ok_or("应存在可编辑文档")?
                .edit_core()?;
            core.composition_start("候", 0)?;
            core.composition_update("候y", 1)?;
        }
        assert!(state.request_save().is_none(), "组合期保存应转入等待");
        assert!(state.save_waiting());
        {
            let core = state
                .session
                .document_mut()
                .ok_or("应存在可编辑文档")?
                .edit_core()?;
            match outcome {
                crate::PublicationOutcome::PlatformConfirmed => {
                    core.composition_commit(2)?;
                }
                crate::PublicationOutcome::PlatformCancelled => {
                    core.composition_cancel(2)?;
                }
            }
        }
        let Some(super::SaveIntent::Save(job)) = state.finish_save_wait_intent(outcome, 3) else {
            return Err("组合自然结束后等待必须继续为保存意图".into());
        };
        assert!(!state.save_waiting(), "结算后等待必须退出");
        assert_eq!(state.feedback_key(), None, "进入写盘应清除等待反馈");
        let completion = job.run();
        assert_eq!(
            state.settle_save(completion).feedback,
            Some("document.save_saved")
        );
        assert_eq!(fs::read(&target)?, expected);
        assert!(
            state.finish_save_wait_intent(outcome, 4).is_none(),
            "等待句柄不得复用"
        );
    }
    Ok(())
}

/// 组合仍活动时生产由 `settle_after_input` 的 `composition_end` 守卫；
/// 直接调用协调器入口必须固定为拒绝路径，不得把等待误当成功或写盘。
#[cfg(target_os = "macos")]
#[test]
fn window_finish_save_wait_intent_while_composing_is_rejected_not_saved()
-> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-composing-reject")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 1))?;
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .composition_start("候", 0)?;
    assert!(state.request_save().is_none(), "组合期保存应转入等待");
    assert!(state.save_waiting());

    assert!(
        state
            .finish_save_wait_intent(crate::PublicationOutcome::PlatformConfirmed, 3)
            .is_none(),
        "组合仍活动时不得返回保存意图"
    );
    assert!(!state.save_waiting(), "拒绝后协调器必须退出等待");
    assert_eq!(state.feedback_key(), Some("document.save_failed"));
    assert_eq!(fs::read(&target)?, b"x\n", "组合活动时不得写盘");
    let document = state.session.document_mut().ok_or("应存在可编辑文档")?;
    assert!(document.core().composition().is_some(), "拒绝不得结束组合");
    Ok(())
}

/// 组合期首次另存为建立关联等待；组合自然结束后必须转入迁移运行态，
/// 复制阶段取消仍按既有 `association_active` 语义生效且等待不得二次结算。
#[cfg(target_os = "macos")]
#[test]
fn window_composition_end_migrates_association_and_cancel_stays_available()
-> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;
    {
        let document = state.session.document_mut().ok_or("应存在可编辑文档")?;
        let len = document.baseline().source().len();
        document
            .edit_core()?
            .open_block(0..len, SelectionSpec::new(0, 1))?;
        document.edit_core()?.composition_start("候", 0)?;
    }
    let target =
        std::env::temp_dir().join(format!("aline-association-wait-{}.md", std::process::id()));
    assert!(
        state
            .accept_save_target(WriteKind::SaveAs, Ok(Some(target)), 1)
            .is_none(),
        "组合期首次另存为应转入关联等待"
    );
    assert!(state.save_waiting());
    assert_eq!(state.feedback_key(), Some("document.save_waiting"));
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .composition_commit(2)?;

    let Some(super::SaveIntent::Migrate(job)) =
        state.finish_save_wait_intent(crate::PublicationOutcome::PlatformConfirmed, 3)
    else {
        return Err("组合自然结束后关联等待应继续为迁移意图".into());
    };
    assert!(!state.save_waiting(), "迁移复制阶段不得仍处于等待");
    assert!(
        matches!(
            state.save_host,
            super::SaveHost::Running { automatic: false }
        ),
        "迁移复制阶段必须进入运行态"
    );
    assert_eq!(state.feedback_key(), None, "进入迁移应清除等待反馈");
    assert!(state.save_cancel_enabled(), "迁移复制阶段取消菜单应可用");
    assert!(state.save_busy());

    // 已结算的等待不得被第二次组合结束重复结算。
    assert!(
        state
            .finish_save_wait_intent(crate::PublicationOutcome::PlatformConfirmed, 4)
            .is_none(),
        "已结算的关联等待不得再次返回意图"
    );
    assert!(
        matches!(
            state.save_host,
            super::SaveHost::Running { automatic: false }
        ),
        "重复结算不得改变运行态"
    );
    assert_eq!(state.feedback_key(), None);

    // 迁移复制阶段取消仍走既有 association_active 分支。
    assert!(state.cancel_save_wait());
    assert_eq!(state.feedback_key(), Some("document.save_cancelled"));
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    assert!(!state.save_cancel_enabled());
    assert!(!state.save_busy());
    {
        let document = state.session.document().ok_or("应存在恢复文档")?;
        assert!(!document.association_active(), "取消后关联必须结束");
        assert_eq!(document.path(), Path::new("recovered.md"));
    }

    // 迟到的迁移完成不得覆盖取消反馈或改路径。
    let late = job.run();
    assert!(state.association_finished(late, 5).is_none());
    assert_eq!(state.feedback_key(), Some("document.save_cancelled"));
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_save_request_while_running_is_rejected_and_queue_recovers() -> Result<(), Box<dyn Error>>
{
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-busy")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;

    let job1 = state.request_save().ok_or("首次保存应返回后台工作")?;
    assert!(state.request_save().is_none(), "保存进行中应拒绝再次请求");
    assert_eq!(state.feedback_key(), Some("document.save_busy"));
    let completion = job1.run();
    assert_eq!(
        state.settle_save(completion).feedback,
        Some("document.save_saved")
    );
    assert_eq!(fs::read(&target)?, b"xa\n");

    let job2 = state.request_save().ok_or("结算后队列应可复用")?;
    let completion = job2.run();
    assert_eq!(
        state.settle_save(completion).feedback,
        Some("document.save_saved")
    );
    assert_eq!(fs::read(&target)?, b"xa\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_autosave_success_is_silent_but_manual_save_reports() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-autosave")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;

    let job = state.request_autosave().ok_or("自动保存应生成后台工作")?;
    assert_eq!(state.feedback_key(), None, "自动保存进入执行时不得提示");
    let completion = job.run();
    assert_eq!(
        state.settle_save(completion),
        super::SaveSettlement { feedback: None },
        "自动保存成功必须静默"
    );
    assert_eq!(fs::read(&target)?, b"xa\n", "自动保存应写入磁盘");

    let job = state.request_save().ok_or("手工保存应生成后台工作")?;
    assert_eq!(
        state.settle_save(job.run()),
        super::SaveSettlement {
            feedback: Some("document.save_saved")
        },
        "手工保存成功仍须提示"
    );
    Ok(())
}

#[test]
fn window_save_without_absolute_target_reports_unavailable() -> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let request = state.request_open()?;
    let job = state
        .accept_dialog(&request, Ok(Some(vec![PathBuf::from("fixture.md")])))
        .ok_or("应生成打开工作")?;
    state.read_finished(job.read_with(&Files(b"x\n")), |_| Ok(()));
    assert!(
        state
            .session
            .document()
            .is_some_and(crate::ReadonlyDocument::is_editable),
        "相对路径仍需以可编辑文档打开"
    );
    assert!(state.request_save().is_none());
    assert_eq!(state.feedback_key(), Some("document.save_unavailable"));
    assert!(!state.save_waiting());
    Ok(())
}

#[test]
fn save_job_and_completion_are_send() {
    fn assert_send<T: Send>() {}
    assert_send::<super::SaveJob>();
    assert_send::<super::WriteCompletion>();
}

#[test]
fn window_without_recovery_configuration_never_arms_recovery() -> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let request = state.request_open()?;
    let job = state
        .accept_dialog(&request, Ok(Some(vec![PathBuf::from("fixture.md")])))
        .ok_or("应生成打开工作")?;
    let (next, prepared) = state.read_finished(job.read_with(&Files(b"x\n")), |_| Ok(()));
    assert!(next.is_none());
    assert_eq!(prepared, Some(()));
    // 未配置恢复：无会话、无计时、无法开始，且无退役任务。
    assert!(state.arm_recovery(0).is_none());
    assert!(state.recovery_poll(0).is_none());
    assert!(state.recovery_start(0).is_none());
    // 取消在无会话文档时同样零副作用。
    state.cancel_recovery();
    assert!(state.take_retired_recoveries().is_empty());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_close_cancels_recovery_and_queues_retire() -> Result<(), Box<dyn Error>> {
    use std::fs;
    use std::sync::Arc;

    use crate::document::{RecoveryRoot, RecoveryStore, UnavailableResources};

    let directory = WindowSaveDir::create("aline-window-recovery")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let root_path = directory.0.join("recovery");
    fs::create_dir(&root_path)?;
    let root = RecoveryRoot::open(&root_path).map_err(|error| format!("{error:?}"))?;
    let store = RecoveryStore::open(root).map_err(|error| format!("{error:?}"))?;

    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    state
        .configure_recovery(Some(crate::document::RecoveryConfig {
            store: store.clone(),
            resources: Arc::new(UnavailableResources),
        }))
        .map_err(|error| format!("{error:?}"))?;
    open_window_document(&mut state, &target, b"x\n")?;
    // 打开成功即布防；下一次到期为注入时刻 + 10 秒。
    assert_eq!(state.arm_recovery(0), Some(10_000));
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;

    let job = match state.recovery_start(0) {
        Some(super::RecoveryStart::Prepare(job)) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    let prepared = job.run();
    let job = match state.recovery_prepared(prepared, 0) {
        Some(super::RecoveryStart::Publish(job)) => job,
        other => return Err(format!("期望发布阶段，实际 {other:?}").into()),
    };
    let settlement = job.run();
    assert_eq!(state.recovery_settled(settlement, 0), None);

    // 撤销后文档重新与保存基线等价，关闭才会获准。
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .undo()?;
    assert!(state.close_window(), "关闭应获准");
    let retired = state.take_retired_recoveries();
    assert!(!retired.is_empty(), "关闭应排队退役");
    for job in retired {
        job.run();
    }
    // 退役成功后活动谱系应被隐藏；失败会保留谱系供下次扫描（W4-53）。
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty(), "关闭退役应隐藏活动谱系");
    assert!(state.arm_recovery(0).is_none());
    assert!(state.session().document().is_none());
    Ok(())
}

#[test]
fn window_recovery_feedback_is_silent_on_success_and_visible_on_failure() {
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    // 成功与正常跳过传 None：保持既有反馈不变。
    state.apply_recovery_feedback(None);
    assert!(state.feedback_key().is_none());
    state.apply_recovery_feedback(Some("document.recovery_failed"));
    assert_eq!(state.feedback_key(), Some("document.recovery_failed"));
    // 后续静默结算不得清除已显示失败。
    state.apply_recovery_feedback(None);
    assert_eq!(state.feedback_key(), Some("document.recovery_failed"));
}

/// 以 "y\n" 替换目标并保留原 mtime；本文件私有，不能复用 persistence 测试实现。
#[cfg(target_os = "macos")]
fn rewrite_window_target_same_mtime(target: &Path) -> Result<(), Box<dyn Error>> {
    use std::fs::{self, FileTimes, OpenOptions};

    let before = fs::metadata(target)?.modified()?;
    fs::write(target, b"y\n")?;
    OpenOptions::new()
        .write(true)
        .open(target)?
        .set_times(FileTimes::new().set_modified(before))?;
    assert_eq!(fs::metadata(target)?.modified()?, before);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_save_conflict_requests_and_presents_external_check() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-external-conflict")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;
    rewrite_window_target_same_mtime(&target)?;

    let job = state.request_save().ok_or("首次保存应返回后台工作")?;
    let completion = job.run();
    assert_eq!(
        state.settle_save(completion).feedback,
        Some("document.save_conflict")
    );
    assert!(
        state.take_external_refresh(),
        "保存冲突后应排队一次静默外改检查"
    );

    let job = state
        .request_external_check(super::ExternalCheckIntent::Focus)
        .ok_or("冲突后的静默检查应生成后台工作")?;
    let completion = job.run();
    let outcome = state.external_check_finished(completion, 10);
    assert_eq!(outcome.feedback, None, "Focus 检查必须静默");
    assert!(!outcome.refresh);
    assert!(!outcome.start_save);
    let view = state
        .session()
        .document()
        .ok_or("应存在可编辑文档")?
        .external_view()
        .ok_or("应呈现外改冲突")?;
    assert_eq!(view.state, super::ExternalDiskState::Changed);

    assert!(state.request_save().is_none(), "冲突未处理时保存应被拒绝");
    assert_eq!(state.feedback_key(), Some("document.save_conflict"));
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_focus_auto_reload_still_reports_source() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-focus-reload")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    rewrite_window_target_same_mtime(&target)?;

    let job = state
        .request_external_check(super::ExternalCheckIntent::Focus)
        .ok_or("干净文档的 Focus 检查应生成后台工作")?;
    let completion = job.run();
    let outcome = state.external_check_finished(completion, 10);
    assert_eq!(
        outcome.feedback,
        Some("document.external_reloaded"),
        "Ruling W4-33：Focus 自动重载仍须提示来源"
    );
    assert!(outcome.refresh);
    assert!(!outcome.start_save);
    let document = state.session().document().ok_or("应存在可编辑文档")?;
    assert_eq!(document.core().buffer_text()?, "y\n");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_external_check_is_silent_without_a_document() -> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    assert!(
        state
            .request_external_check(super::ExternalCheckIntent::Focus)
            .is_none()
    );
    assert_eq!(state.feedback_key(), None, "Focus 在无文档时必须静默");
    assert!(
        state
            .request_external_check(super::ExternalCheckIntent::Retry)
            .is_none()
    );
    assert_eq!(
        state.feedback_key(),
        Some("document.save_unavailable"),
        "显式入口在无文档时应反馈不可保存"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn window_overwrite_ready_starts_save_and_publishes() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-external-overwrite")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;
    rewrite_window_target_same_mtime(&target)?;

    let job = state.request_save().ok_or("首次保存应返回后台工作")?;
    assert_eq!(
        state.settle_save(job.run()).feedback,
        Some("document.save_conflict")
    );
    state.take_external_refresh();
    let job = state
        .request_external_check(super::ExternalCheckIntent::Focus)
        .ok_or("静默检查应生成后台工作")?;
    state.external_check_finished(job.run(), 10);

    let job = state
        .request_external_check(super::ExternalCheckIntent::Overwrite)
        .ok_or("覆盖检查应生成后台工作")?;
    let outcome = state.external_check_finished(job.run(), 11);
    assert!(outcome.start_save, "OverwriteReady 应触发既有保存");
    assert_eq!(outcome.feedback, None);
    let job = state.request_save().ok_or("覆盖后应允许保存")?;
    assert_eq!(
        state.settle_save(job.run()).feedback,
        Some("document.save_saved")
    );
    assert_eq!(fs::read(&target)?, b"xa\n", "覆盖确认后应发布本地文本");
    Ok(())
}

#[test]
fn conflict_exits_follow_view_state() {
    use std::sync::Arc;

    use super::{ConflictExits, ExternalConflictView, ExternalDiskState, conflict_exits};

    assert_eq!(conflict_exits(false, None), ConflictExits::default());
    assert_eq!(conflict_exits(true, None), ConflictExits::default());
    let changed = ExternalConflictView {
        state: ExternalDiskState::Changed,
        baseline: Arc::from("x\n"),
        local: Arc::from("xa\n"),
        disk: Some(Arc::from("y\n")),
    };
    let exits = conflict_exits(true, Some(&changed));
    assert!(exits.load_disk);
    assert!(exits.overwrite);
    assert!(exits.retry);
    assert!(exits.dismiss);
    let missing = ExternalConflictView {
        state: ExternalDiskState::Missing,
        baseline: Arc::from("x\n"),
        local: Arc::from("xa\n"),
        disk: None,
    };
    let exits = conflict_exits(true, Some(&missing));
    assert!(!exits.load_disk);
    assert!(!exits.overwrite);
    assert!(exits.retry);
    assert!(exits.dismiss);
}

#[test]
fn recovery_slot_reset_rearms_new_timer_and_rejects_old_completion() -> Result<(), Box<dyn Error>> {
    let mut slot = super::recovery_slot::RecoverySlot::new();
    assert!(slot.can_arm());
    let old = slot.begin_timer().ok_or("旧计时槽应可占用")?;
    assert!(!slot.can_arm(), "旧计时占用期间不得重新布防");
    // 替换文档：旧任务槽整体作废，新文档重新布防。
    slot.reset();
    assert!(slot.can_arm());
    let fresh = slot.begin_timer().ok_or("替换后应可重新布防")?;
    assert!(!slot.finish_timer(old), "旧计时完成不得清除新任务");
    assert!(!slot.can_arm(), "新计时仍在，槽不得被旧完成释放");
    assert!(slot.finish_timer(fresh));
    assert!(slot.can_arm());
    Ok(())
}

#[test]
fn recovery_slot_old_work_completion_does_not_clear_new_timer() -> Result<(), Box<dyn Error>> {
    let mut slot = super::recovery_slot::RecoverySlot::new();
    let old_work = slot.begin_work().ok_or("旧工作槽应可占用")?;
    // 替换文档：旧工作代次失效，新文档开始计时。
    slot.reset();
    let fresh = slot.begin_timer().ok_or("替换后应可重新布防")?;
    assert!(!slot.finish_work(old_work), "旧工作完成不得清除新计时槽");
    assert!(!slot.can_arm(), "新计时仍在，槽不得被旧完成释放");
    assert!(slot.finish_timer(fresh));
    assert!(slot.can_arm());
    Ok(())
}

#[test]
fn recovery_slot_close_rejects_late_completions_and_stays_empty() -> Result<(), Box<dyn Error>> {
    let mut slot = super::recovery_slot::RecoverySlot::new();
    let timer = slot.begin_timer().ok_or("关闭前应有计时任务")?;
    let work = slot.begin_work().ok_or("关闭前应有工作任务")?;
    // 关闭窗口：槽整体作废，之后不再布防；任何迟到完成都被拒绝且不占用槽。
    slot.reset();
    assert!(!slot.finish_timer(timer), "关闭后迟到计时完成应被拒绝");
    assert!(!slot.finish_work(work), "关闭后迟到工作完成应被拒绝");
    assert!(slot.can_arm(), "关闭后槽应保持空闲");
    Ok(())
}

// ---- 4.2b 另存为/恢复接线（W4-56/57/58） ----

/// 把只实现 `Debug` 的内部错误折算为测试动态错误。
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, Box<dyn Error>> {
    result.map_err(|error| format!("{error:?}").into())
}

fn non_zero(value: u64) -> Result<NonZeroU64, Box<dyn Error>> {
    NonZeroU64::new(value).ok_or_else(|| "代次/身份必须非零".into())
}

fn recovered_identity(value: u64) -> Result<RecoveryDocumentId, Box<dyn Error>> {
    Ok(RecoveryDocumentId::from_raw_for_test(non_zero(value)?))
}

/// 纯合成恢复记录；不触碰存储，用于另存为与恢复面板状态用例。
fn synthetic_recovered(
    source: &str,
    generation: u64,
    path: RecoveredPath,
) -> Result<RecoveredDocument, Box<dyn Error>> {
    Ok(RecoveredDocument {
        document: recovered_identity(1)?,
        generation: non_zero(generation)?,
        path_generation: 7,
        captured_composition: false,
        path,
        anchor_byte: 0,
        head_byte: 0,
        source: source.to_owned(),
        logical_saved: None,
        baseline: None,
        resources: Vec::new(),
    })
}

fn synthetic_restored(
    source: &str,
    generation: u64,
    path: RecoveredPath,
) -> Result<RestoredRecovery, Box<dyn Error>> {
    Ok(RestoredRecovery {
        recovered: synthetic_recovered(source, generation, path)?,
        temp_resources: None,
    })
}

/// 在协调器会话中原子发布一条合成恢复文档。
fn open_synthetic_recovered(
    state: &mut super::NativeOpenCoordinator,
    source: &str,
) -> Result<(), Box<dyn Error>> {
    let restored = synthetic_restored(source, 1, RecoveredPath::Unlinked)?;
    state
        .session_mut()
        .open_recovered(restored)
        .map_err(|error| format!("{error:?}"))?;
    Ok(())
}

/// 便于对只读/可编辑两种窗口预算复用同一打开路径。
fn coordinator_limits(editable: bool) -> crate::ReadonlyOpenLimits {
    let bytes = 64 * 1024;
    let limits = crate::ReadonlyOpenLimits::new(
        aline_source::StorageBudget::new(bytes),
        crate::DocLimits::new(
            aline_buffer::TextBudget::new(bytes),
            aline_buffer::LineIndexBudget::new(1024),
            aline_buffer::HistoryBudget::new(32, bytes),
        ),
        aline_markdown::ParseConfig::m0(),
        aline_markdown::ParseBudget::m0(),
    );
    if editable {
        limits.with_editing()
    } else {
        limits
    }
}

/// 用内存读取器打开任意路径；不依赖真实文件，可跨平台使用。
fn open_memory_document(
    state: &mut super::NativeOpenCoordinator,
    path: &Path,
    bytes: &[u8],
) -> Result<(), Box<dyn Error>> {
    let request = state.request_open()?;
    let job = state
        .accept_dialog(&request, Ok(Some(vec![path.to_path_buf()])))
        .ok_or("应生成打开工作")?;
    let (next, prepared) = state.read_finished(job.read_with(&Files(bytes)), |_| Ok(()));
    assert!(next.is_none());
    assert_eq!(prepared, Some(()));
    assert!(state.take_focus_return());
    Ok(())
}

fn scan_generation(
    value: u64,
    status: RecoveryGenerationStatus,
) -> Result<RecoveryGenerationScan, Box<dyn Error>> {
    Ok(RecoveryGenerationScan {
        generation: non_zero(value)?,
        status,
        payload_bytes: value,
    })
}

fn scan_lineage(
    document: u64,
    disposition: RecoveryLineageDisposition,
    generations: Vec<RecoveryGenerationScan>,
) -> Result<RecoveryLineageScan, Box<dyn Error>> {
    Ok(RecoveryLineageScan {
        document: recovered_identity(document)?,
        disposition,
        retired_through: None,
        generations,
        reservations: Vec::new(),
        defects: Vec::new(),
    })
}

fn synthetic_scan(lineages: Vec<RecoveryLineageScan>) -> RecoveryScan {
    RecoveryScan {
        root_payload_bytes: 0,
        lineages,
        defects: Vec::new(),
    }
}

#[test]
fn save_as_menu_enabled_only_for_editable_idle_document() -> Result<(), Box<dyn Error>> {
    let mut readonly = super::NativeOpenCoordinator::new(coordinator_limits(false));
    assert!(!readonly.save_as_enabled(), "无文档时另存为不可用");
    open_memory_document(&mut readonly, Path::new("fixture.md"), b"x\n")?;
    assert!(!readonly.save_as_enabled(), "只读文档不得另存为");

    let mut editable = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut editable, "x\n")?;
    assert!(editable.save_as_enabled(), "空闲可编辑恢复文档应可另存为");
    editable.save_host = super::SaveHost::Running { automatic: false };
    assert!(!editable.save_as_enabled(), "保存执行中不得另存为");
    editable.save_host = super::SaveHost::Idle;
    assert!(editable.save_as_enabled(), "回到空闲后恢复可用性");
    Ok(())
}

#[test]
fn save_as_action_accepts_picked_path_and_rejects_cancel() -> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;

    let cancelled = state.accept_save_target(WriteKind::SaveAs, Ok(None), 0);
    assert!(cancelled.is_none());
    assert_eq!(state.feedback_key(), Some("document.save_cancelled"));
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    assert!(state.session().document().is_some_and(|document| {
        !document.association_active() && document.path() == Path::new("recovered.md")
    }));

    let failed = state.accept_save_target(WriteKind::SaveAs, Err(()), 0);
    assert!(failed.is_none());
    assert_eq!(state.feedback_key(), Some("document.save_target_failed"));
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    assert!(
        state
            .session()
            .document()
            .is_some_and(|document| !document.association_active())
    );

    let target =
        std::env::temp_dir().join(format!("aline-save-as-recovered-{}.md", std::process::id()));
    let accepted = state.accept_save_target(WriteKind::SaveAs, Ok(Some(target.clone())), 0);
    assert!(matches!(accepted, Some(super::SaveIntent::Migrate(_))));
    assert!(matches!(
        state.save_host,
        super::SaveHost::Running { automatic: false }
    ));
    assert!(state.feedback_key().is_none());
    assert!(
        state
            .session()
            .document()
            .is_some_and(|document| document.association_active()),
        "选择目标后关联应处于活动态"
    );
    Ok(())
}

#[test]
fn cancel_association_copy_returns_to_idle_and_ignores_late_completion()
-> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;
    let target = std::env::temp_dir().join(format!(
        "aline-cancel-association-{}.md",
        std::process::id()
    ));

    let job = match state.accept_save_target(WriteKind::SaveAs, Ok(Some(target.clone())), 0) {
        Some(super::SaveIntent::Migrate(job)) => job,
        _ => return Err("选择目标后应进入迁移复制".into()),
    };
    assert!(matches!(
        state.save_host,
        super::SaveHost::Running { automatic: false }
    ));
    assert!(
        state
            .session()
            .document()
            .is_some_and(|document| document.association_active()),
        "复制阶段关联应活动"
    );
    assert!(state.save_cancel_enabled(), "复制阶段取消菜单应可用");
    assert!(state.save_busy());

    assert!(state.cancel_save_wait());
    assert_eq!(state.feedback_key(), Some("document.save_cancelled"));
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    assert!(!state.save_cancel_enabled());
    assert!(!state.save_busy());
    assert!(state.save_as_enabled(), "取消后应可再次保存");
    {
        let document = state.session().document().ok_or("应存在恢复文档")?;
        assert!(!document.association_active());
        assert_eq!(document.path(), Path::new("recovered.md"));
        assert_eq!(document.core().buffer_text()?, "x\n");
    }

    // 迟到的迁移完成不得改路径、清文档或覆盖取消反馈。
    let late = job.run();
    assert!(state.association_finished(late, 1).is_none());
    assert_eq!(state.feedback_key(), Some("document.save_cancelled"));
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    {
        let document = state.session().document().ok_or("应存在恢复文档")?;
        assert!(!document.association_active());
        assert_eq!(document.path(), Path::new("recovered.md"));
        assert_eq!(document.core().buffer_text()?, "x\n");
    }

    // 取消后同一目标可再次发起保存，并再次取消回到空闲。
    assert!(matches!(
        state.accept_save_target(WriteKind::SaveAs, Ok(Some(target)), 2),
        Some(super::SaveIntent::Migrate(_))
    ));
    assert!(matches!(
        state.save_host,
        super::SaveHost::Running { automatic: false }
    ));
    assert!(state.cancel_save_wait());
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    assert!(
        state
            .session()
            .document()
            .is_some_and(|document| !document.association_active())
    );
    Ok(())
}

#[test]
fn save_as_target_without_home_reports_unavailable() -> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;
    assert!(state.save_target_plan(None).is_none());
    assert_eq!(state.feedback_key(), Some("document.save_target_failed"));
    let document = state.session().document().ok_or("应存在恢复文档")?;
    assert_eq!(document.path(), Path::new("recovered.md"));
    assert!(document.recovered_original_path().is_none());
    assert!(!document.association_active());
    Ok(())
}

#[test]
fn untitled_save_action_requests_target_instead_of_no_target() -> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;
    assert!(matches!(
        state.request_save_intent(),
        Some(super::SaveIntent::ChooseTarget(WriteKind::Create))
    ));
    assert!(state.feedback_key().is_none());
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    Ok(())
}

#[test]
fn non_composing_save_entries_keep_their_target_selection() -> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;
    assert!(matches!(
        state.request_save_intent(),
        Some(super::SaveIntent::ChooseTarget(WriteKind::Create))
    ));
    assert!(state.feedback_key().is_none());
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    assert!(matches!(
        state.request_save_as_intent(),
        Some(super::SaveIntent::ChooseTarget(WriteKind::SaveAs))
    ));
    assert!(state.feedback_key().is_none());
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn composing_untitled_save_intent_rejects_target_selection_without_side_effects()
-> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;
    {
        let document = state.session.document_mut().ok_or("应存在可编辑文档")?;
        let len = document.baseline().source().len();
        let core = document.edit_core()?;
        core.open_block(0..len, SelectionSpec::new(0, 0))?;
        core.composition_start("候", 0)?;
    }

    assert!(state.save_as_enabled(), "组合期菜单保持可用，点击后给反馈");
    assert!(
        state.request_save_intent().is_none(),
        "无目标首次保存在组合期不得打开选择器"
    );
    assert_eq!(state.feedback_key(), Some("document.composition_pending"));
    assert!(
        matches!(state.save_host, super::SaveHost::Idle),
        "拒绝不得建立等待句柄"
    );
    assert!(!state.save_waiting());
    let document = state.session.document().ok_or("应存在文档")?;
    assert!(document.core().composition().is_some(), "拒绝不得结束组合");
    assert!(
        !document.association_active(),
        "拒绝不得产生关联或迁移副作用"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn composing_save_as_intent_rejects_target_selection_without_side_effects()
-> Result<(), Box<dyn Error>> {
    let mut state = super::NativeOpenCoordinator::new(coordinator_limits(true));
    open_synthetic_recovered(&mut state, "x\n")?;
    {
        let document = state.session.document_mut().ok_or("应存在可编辑文档")?;
        let len = document.baseline().source().len();
        let core = document.edit_core()?;
        core.open_block(0..len, SelectionSpec::new(0, 0))?;
        core.composition_start("候", 0)?;
    }

    assert!(state.save_as_enabled(), "组合期另存为菜单仍可用");
    assert!(
        state.request_save_as_intent().is_none(),
        "组合期另存为不得打开原生选择器"
    );
    assert_eq!(state.feedback_key(), Some("document.composition_pending"));
    assert!(matches!(state.save_host, super::SaveHost::Idle));
    assert!(!state.save_waiting());
    let document = state.session.document().ok_or("应存在文档")?;
    assert!(document.core().composition().is_some());
    assert!(!document.association_active());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn composing_save_intent_with_target_still_waits_for_natural_end() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-composing-save-target")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 1))?;
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .composition_start("候", 0)?;

    assert!(
        state.request_save_intent().is_none(),
        "已有目标的保存在组合期仍转入等待"
    );
    assert!(matches!(state.save_host, super::SaveHost::Waiting(_)));
    assert!(state.save_waiting());
    assert_eq!(state.feedback_key(), Some("document.save_waiting"));
    assert_eq!(fs::read(&target)?, b"x\n", "等待不得提前写盘");
    Ok(())
}

#[cfg(target_os = "macos")]
struct RecoveryFixture {
    dir: WindowSaveDir,
    store: RecoveryStore,
}

#[cfg(target_os = "macos")]
impl RecoveryFixture {
    fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
        use std::fs;

        let dir = WindowSaveDir::create(prefix)?;
        let root_path = dir.0.join("recovery");
        fs::create_dir(&root_path)?;
        let root = ok(RecoveryRoot::open(&root_path))?;
        let store = ok(RecoveryStore::open(root))?;
        Ok(Self { dir, store })
    }

    fn path(&self) -> &Path {
        &self.dir.0
    }

    fn lineage(&self) -> Result<RecoveryDocumentId, Box<dyn Error>> {
        ok(self.store.open_lineage(100))
    }

    fn publish(
        &self,
        document: RecoveryDocumentId,
        source: &str,
        resources: Vec<RecoveryResourceInput>,
    ) -> Result<NonZeroU64, Box<dyn Error>> {
        publish_record(&self.store, document, source, resources)
    }

    fn temp_root(&self) -> Result<TempRoot, Box<dyn Error>> {
        use std::fs;

        let home = self.dir.0.join("home");
        fs::create_dir_all(home.join("Library/Application Support"))?;
        ok(TempRoot::open_or_create(&home))
    }
}

#[cfg(target_os = "macos")]
fn publish_record(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    source: &str,
    resources: Vec<RecoveryResourceInput>,
) -> Result<NonZeroU64, Box<dyn Error>> {
    let input = RecoveryGenerationInput {
        path_generation: 7,
        captured_composition: false,
        path: crate::document::RecoveryPath::Unlinked,
        anchor_byte: 0,
        head_byte: 0,
        source: source.to_owned(),
        logical_saved: None,
        baseline: None,
        resources,
    };
    let cancellation = Cancellation::new();
    match ok(store.publish(document, input, &cancellation))? {
        RecoveryPublishOutcome::Published { generation, .. } => Ok(generation),
        other => Err(format!("期望 Published，实际 {other:?}").into()),
    }
}

#[cfg(target_os = "macos")]
fn publish_active_recovery(state: &mut super::NativeOpenCoordinator) -> Result<(), Box<dyn Error>> {
    let job = match state.recovery_start(0) {
        Some(super::RecoveryStart::Prepare(job)) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    let prepared = job.run();
    let job = match state.recovery_prepared(prepared, 0) {
        Some(super::RecoveryStart::Publish(job)) => job,
        other => return Err(format!("期望发布阶段，实际 {other:?}").into()),
    };
    let settlement = job.run();
    state.recovery_settled(settlement, 0);
    Ok(())
}

#[cfg(target_os = "macos")]
fn stage_window_image(
    state: &mut super::NativeOpenCoordinator,
    name: &str,
    bytes: &[u8],
) -> Result<String, Box<dyn Error>> {
    open_window_block(state, SelectionSpec::new(0, 0))?;
    let reference = state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .stage_temp_image(name, bytes, 4096)
        .map_err(|error| format!("{error:?}"))?;
    let locator = reference.locator().to_owned();
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .leave_block(1)?;
    Ok(locator)
}

#[cfg(target_os = "macos")]
fn locator_path(root: &TempRoot, locator: &str) -> Option<PathBuf> {
    let rest = locator.strip_prefix("aline-temp:")?;
    let (document, file) = rest.split_once('/')?;
    Some(root.path().join(document).join(file))
}

#[cfg(target_os = "macos")]
fn regular_file_count(path: &Path) -> Result<usize, Box<dyn Error>> {
    let mut count = 0;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            count += regular_file_count(&entry.path())?;
        } else if metadata.is_file() {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(target_os = "macos")]
fn configure_window_recovery(
    state: &mut super::NativeOpenCoordinator,
    store: &RecoveryStore,
    root: TempRoot,
) -> Result<(), Box<dyn Error>> {
    state
        .configure_recovery(Some(RecoveryConfig {
            store: store.clone(),
            resources: Arc::new(UnavailableResources),
        }))
        .map_err(|error| format!("{error:?}"))?;
    state.session_mut().configure_temp_root(Some(root));
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn close_cancels_autosave_and_recovery_before_release() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let fixture = RecoveryFixture::new("aline-window-close-cancel")?;
    let target = fixture.path().join("notes.md");
    fs::write(&target, b"x\n")?;
    let root = fixture.temp_root()?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    configure_window_recovery(&mut state, &fixture.store, root)?;
    open_window_document(&mut state, &target, b"x\n")?;
    assert_eq!(state.arm_recovery(0), Some(10_000));
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;
    publish_active_recovery(&mut state)?;
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .undo()?;
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .note_document_activity(0);
    {
        let document = state.session.document().ok_or("应存在可编辑文档")?;
        assert_eq!(document.autosave_deadline(), Some(1_000));
        assert_eq!(document.recovery_deadline(), Some(10_000));
    }

    let observed = state
        .close_document_with(|document| {
            (document.autosave_deadline(), document.association_active())
        })
        .map_err(|()| "关闭应获准")?;
    assert_eq!(observed, Some((None, false)));
    let (recoveries, cleanups) = state.take_retired();
    assert!(!recoveries.is_empty(), "关闭应先排队恢复退役");
    assert!(cleanups.is_empty(), "未暂存资源时无清理任务");
    assert!(state.session().document().is_none());
    assert!(state.arm_recovery(0).is_none());
    for job in recoveries {
        job.run();
    }
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn close_refusal_does_not_cancel_recovery_or_cleanup() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let fixture = RecoveryFixture::new("aline-window-close-refuse")?;
    let target = fixture.path().join("notes.md");
    fs::write(&target, b"x\n")?;
    let root = fixture.temp_root()?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    configure_window_recovery(&mut state, &fixture.store, root.clone())?;
    open_window_document(&mut state, &target, b"x\n")?;
    assert_eq!(state.arm_recovery(0), Some(10_000));
    let locator = stage_window_image(&mut state, "pic.png", b"png-bytes")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .note_document_activity(0);

    assert!(state.close_document_with(|_| ()).is_err());
    assert_eq!(state.feedback_key(), Some("document.close_pending_changes"));
    assert!(state.session().document().is_some(), "拒绝关闭必须保留文档");
    let (recoveries, cleanups) = state.take_retired();
    assert!(recoveries.is_empty());
    assert!(cleanups.is_empty());
    let document = state.session.document().ok_or("应存在可编辑文档")?;
    assert_eq!(document.autosave_deadline(), Some(1_000));
    assert_eq!(document.recovery_deadline(), Some(10_000));
    let resource_path = locator_path(&root, &locator).ok_or("locator 应可解析")?;
    assert!(resource_path.is_file(), "拒绝关闭不得删除临时资源");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn close_success_spawns_cleanup_only_after_retirement() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let fixture = RecoveryFixture::new("aline-window-close-cleanup")?;
    let target = fixture.path().join("notes.md");
    fs::write(&target, b"x\n")?;
    let root = fixture.temp_root()?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    configure_window_recovery(&mut state, &fixture.store, root.clone())?;
    open_window_document(&mut state, &target, b"x\n")?;
    assert_eq!(state.arm_recovery(0), Some(10_000));
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;
    publish_active_recovery(&mut state)?;
    state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .edit_core()?
        .undo()?;
    // 直接暂存资源而不新开输入块：发布时资源尚未写入记录，关闭后的清理
    // 才能验证“退役谱系后删除临时文件”。
    let reference = state
        .session
        .document_mut()
        .ok_or("应存在可编辑文档")?
        .stage_temp_image("pic.png", b"png-bytes", 4096)
        .map_err(|error| format!("{error:?}"))?;
    let locator = reference.locator().to_owned();
    let resource_path = locator_path(&root, &locator).ok_or("locator 应可解析")?;
    assert!(resource_path.is_file());

    assert!(state.close_document_with(|_| ()).is_ok(), "关闭应获准");
    let (recoveries, cleanups) = state.take_retired();
    assert!(!recoveries.is_empty());
    assert!(!cleanups.is_empty(), "暂存资源应生成清理任务");
    // 先退役恢复谱系，再清理临时资源；清理任务在退役后执行才能删除。
    for job in recoveries {
        job.run();
    }
    let scan = ok(fixture.store.scan())?;
    assert!(
        scan.recoverable_lineages()
            .iter()
            .all(|lineage| lineage.document.directory_name() != locator_document(&locator)),
        "退役后扫描不得再列出该谱系"
    );
    let outcome = cleanups
        .into_iter()
        .map(super::ResourceCleanupJob::run)
        .collect::<Vec<_>>();
    assert_eq!(
        outcome,
        vec![crate::document::CleanupOutcome::Cleaned],
        "退役后清理应成功"
    );
    assert_eq!(
        regular_file_count(root.path())?,
        0,
        "临时根不得残留普通文件"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
fn locator_document(locator: &str) -> String {
    locator
        .strip_prefix("aline-temp:")
        .and_then(|rest| rest.split_once('/'))
        .map_or_else(String::new, |(document, _)| document.to_owned())
}

#[cfg(target_os = "macos")]
#[test]
fn startup_creates_recovery_dirs_and_scans_once() -> Result<(), Box<dyn Error>> {
    use std::fs;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    let directory = WindowSaveDir::create("aline-window-startup")?;
    let home = directory.0.join("home");
    fs::create_dir_all(home.join("Library/Application Support"))?;
    let runtime = production_recovery_with(Some(&home), true);
    assert!(runtime.config.is_some(), "macOS 注入 HOME 时应启用恢复");
    assert!(runtime.feedback.is_none());
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    let store = state
        .install_recovery_runtime(runtime)
        .ok_or("启用后应安装存储")?;

    let recovery_dir = home.join("Library/Application Support/Aline/recovery");
    let temporary_dir = home.join("Library/Application Support/Aline/temporary");
    for path in [&recovery_dir, &temporary_dir] {
        let metadata = fs::metadata(path)?;
        assert!(metadata.is_dir());
        assert_eq!(
            metadata.permissions().mode() & 0o777,
            0o700,
            "生产目录必须为 0700"
        );
        assert_eq!(metadata.uid(), fs::metadata(&home)?.uid());
    }

    assert_eq!(state.begin_startup_scan(), Some(1));
    assert!(state.begin_startup_scan().is_none(), "扫描只能启动一次");
    let document = ok(store.open_lineage(100))?;
    publish_record(&store, document, "recovered\n", Vec::new())?;
    let scan = ok(store.scan())?;
    assert!(state.startup_scan_finished(1, Ok(scan.clone())));
    assert_eq!(state.panel.entries().len(), 1);
    assert!(state.panel.is_visible());
    assert!(
        !state.startup_scan_finished(1, Ok(scan)),
        "重复代次不得再次安装"
    );
    assert_eq!(state.panel.entries().len(), 1);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn startup_scan_failure_allows_retry_without_auto_rescan() -> Result<(), Box<dyn Error>> {
    use std::fs;

    let directory = WindowSaveDir::create("aline-window-scan-retry")?;
    let home = directory.0.join("home");
    fs::create_dir_all(home.join("Library/Application Support"))?;
    let runtime = production_recovery_with(Some(&home), true);
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    state
        .install_recovery_runtime(runtime)
        .ok_or("启用后应安装存储")?;

    assert_eq!(state.begin_startup_scan(), Some(1));
    assert!(
        !state.startup_scan_finished(1, Err(crate::document::RecoveryStoreError::InvalidInput))
    );
    assert_eq!(state.feedback_key(), Some("document.recovery_scan_failed"));
    assert_eq!(state.begin_startup_scan(), Some(1), "失败后应允许再次开始");
    assert!(
        state.begin_startup_scan().is_none(),
        "同一轮仍只能启动一次，不自动循环重扫"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn startup_without_home_disables_recovery_and_keeps_editing_available() -> Result<(), Box<dyn Error>>
{
    use std::fs;

    let runtime = production_recovery_with(None, true);
    assert!(runtime.config.is_none());
    assert!(runtime.temp_root.is_none());
    assert_eq!(runtime.feedback, Some("document.recovery_disabled"));
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    assert!(state.install_recovery_runtime(runtime).is_none());
    assert_eq!(state.feedback_key(), Some("document.recovery_disabled"));
    assert!(state.begin_startup_scan().is_none());
    assert!(!state.panel.is_visible());

    let directory = WindowSaveDir::create("aline-window-no-home")?;
    let target = directory.0.join("notes.md");
    fs::write(&target, b"x\n")?;
    open_window_document(&mut state, &target, b"x\n")?;
    open_window_block(&mut state, SelectionSpec::new(0, 0))?;
    window_type(&mut state, 1, "a")?;
    let job = state.request_save().ok_or("应返回保存工作")?;
    assert_eq!(
        state.settle_save(job.run()).feedback,
        Some("document.save_saved")
    );
    assert_eq!(fs::read(&target)?, b"xa\n");
    Ok(())
}

#[test]
fn non_macos_runtime_is_disabled_without_side_effects() -> Result<(), Box<dyn Error>> {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let home = std::env::temp_dir().join(format!(
        "aline-non-macos-runtime-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir(&home)?;
    let runtime = production_recovery_with(Some(&home), false);
    let result = {
        assert!(runtime.config.is_none());
        assert!(runtime.temp_root.is_none());
        assert_eq!(runtime.feedback, Some("document.recovery_disabled"));
        assert!(!home.join("Library").exists(), "禁用平台不得创建目录");
        Ok(())
    };
    let _ = fs::remove_dir_all(&home);
    result
}

#[test]
fn startup_scan_lists_only_recoverable_records() -> Result<(), Box<dyn Error>> {
    let scan = synthetic_scan(vec![
        scan_lineage(
            1,
            RecoveryLineageDisposition::Active,
            vec![scan_generation(1, RecoveryGenerationStatus::Complete)?],
        )?,
        scan_lineage(
            2,
            RecoveryLineageDisposition::Active,
            vec![scan_generation(1, RecoveryGenerationStatus::Incomplete)?],
        )?,
        scan_lineage(
            3,
            RecoveryLineageDisposition::Blocked,
            vec![scan_generation(1, RecoveryGenerationStatus::Complete)?],
        )?,
        scan_lineage(
            4,
            RecoveryLineageDisposition::Active,
            vec![scan_generation(1, RecoveryGenerationStatus::UnknownFormat)?],
        )?,
    ]);
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));
    assert_eq!(panel.entries().len(), 1);
    assert_eq!(panel.entries()[0].document, recovered_identity(1)?);
    assert_eq!(panel.unrecoverable(), 3);
    assert!(panel.is_visible());
    Ok(())
}

#[test]
fn startup_scan_reports_unrecoverable_records_without_actions() -> Result<(), Box<dyn Error>> {
    let scan = synthetic_scan(vec![
        scan_lineage(
            2,
            RecoveryLineageDisposition::Active,
            vec![scan_generation(1, RecoveryGenerationStatus::Incomplete)?],
        )?,
        scan_lineage(
            3,
            RecoveryLineageDisposition::Blocked,
            vec![scan_generation(1, RecoveryGenerationStatus::Complete)?],
        )?,
        scan_lineage(
            4,
            RecoveryLineageDisposition::Active,
            vec![scan_generation(1, RecoveryGenerationStatus::UnknownFormat)?],
        )?,
    ]);
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));
    assert!(panel.entries().is_empty(), "不可恢复项不得生成动作行");
    assert_eq!(panel.unrecoverable(), 3);
    assert!(panel.is_visible(), "仅报告也需要可见");
    assert!(panel.confirming().is_none());
    assert!(!panel.is_busy());
    Ok(())
}

#[test]
fn corrupt_latest_generation_lists_earlier_complete_and_reports_defect()
-> Result<(), Box<dyn Error>> {
    let scan = synthetic_scan(vec![scan_lineage(
        1,
        RecoveryLineageDisposition::Active,
        vec![
            scan_generation(1, RecoveryGenerationStatus::Complete)?,
            scan_generation(2, RecoveryGenerationStatus::Corrupt)?,
        ],
    )?]);
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));
    assert_eq!(panel.entries().len(), 1);
    assert_eq!(panel.entries()[0].generation, non_zero(1)?);
    assert_eq!(panel.unrecoverable(), 1, "损坏代只报告不列出");
    let corrupted = non_zero(2)?;
    assert!(
        panel
            .entries()
            .iter()
            .all(|entry| entry.generation != corrupted),
        "不得列出损坏代"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_panel_restore_reads_latest_complete_generation() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-restore")?;
    let document = fixture.lineage()?;
    let first = fixture.publish(document, "one\n", Vec::new())?;
    let second = fixture.publish(document, "two\n", Vec::new())?;
    assert!(second > first);
    let scan = ok(fixture.store.scan())?;
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));
    assert_eq!(panel.entries().len(), 1);
    assert_eq!(panel.entries()[0].generation, second);

    let job = match panel.request(
        RecoveryPanelAction::Restore { document },
        &fixture.store,
        None,
    ) {
        RecoveryPanelStep::Read(job) => job,
        _ => return Err("恢复请求应生成读取任务".into()),
    };
    assert_eq!(job.document(), document);
    let stale = RecoveryRestoreCompletion {
        document,
        operation: job.operation().wrapping_add(1),
        result: Ok(synthetic_restored("stale", 1, RecoveredPath::Unlinked)?),
    };
    assert!(matches!(
        panel.restore_finished(stale),
        RecoveryPanelStep::Ignore
    ));
    assert!(panel.is_busy(), "迟到完成不得清除在途状态");

    let completion = job.run();
    let restored = match panel.restore_finished(completion) {
        RecoveryPanelStep::Recovered(restored) => restored,
        _ => return Err("恢复完成应返回已恢复文档".into()),
    };
    assert_eq!(restored.recovered.source, "two\n");
    assert_eq!(
        panel.entries()[0].status,
        RecoveryPanelEntryStatus::Restored
    );
    assert_eq!(panel.feedback_key(), Some("document.recovery_restored"));
    assert!(!panel.is_busy());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn starting_open_invalidates_in_flight_restore_and_cleans_materialized_resources()
-> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-invalidate-open")?;
    let document = fixture.lineage()?;
    let root = fixture.temp_root()?;
    let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
    fixture.publish(
        document,
        "img\n",
        vec![RecoveryResourceInput {
            reference: locator,
            bytes: b"img".to_vec(),
        }],
    )?;
    let scan = ok(fixture.store.scan())?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    assert!(state.panel.install_scan(1, &scan));
    let job = match state.panel.request(
        RecoveryPanelAction::Restore { document },
        &fixture.store,
        Some(root.clone()),
    ) {
        RecoveryPanelStep::Read(job) => job,
        _ => return Err("恢复请求应生成读取任务".into()),
    };

    // 用户开始打开（随后关闭）：在飞恢复作废，迟到完成不得接管文档。
    let _request = state.request_open()?;
    assert!(!state.panel.is_busy(), "开始打开必须作废在途恢复");
    assert!(state.close_document_with(|_| ()).is_ok());

    let completion = job.run();
    assert!(completion.result.is_ok(), "后台任务仍会完成并物化材料");
    assert_eq!(
        regular_file_count(root.path())?,
        1,
        "完成信封持有材料时文件应存在"
    );
    assert!(matches!(
        state.panel.restore_finished(completion),
        RecoveryPanelStep::Ignore
    ));
    assert!(state.session().document().is_none(), "迟到完成不得重开文档");
    assert_eq!(state.panel.entries().len(), 1);
    assert_eq!(
        state.panel.entries()[0].status,
        RecoveryPanelEntryStatus::Available
    );
    assert_eq!(regular_file_count(root.path())?, 0, "未采纳材料必须清理");
    assert!(
        std::fs::read_dir(root.path())?.next().is_none(),
        "未采纳材料必须删除按文档创建的子目录"
    );
    let scan = ok(fixture.store.scan())?;
    assert_eq!(
        scan.recoverable_lineages()
            .iter()
            .filter(|lineage| lineage.document == document)
            .count(),
        1,
        "失效不得删除记录"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn in_flight_read_makes_recovered_open_busy_and_cleans_materialized_resources()
-> Result<(), Box<dyn Error>> {
    use std::fs;

    let fixture = RecoveryFixture::new("aline-panel-busy")?;
    let document = fixture.lineage()?;
    let root = fixture.temp_root()?;
    let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
    fixture.publish(
        document,
        "img\n",
        vec![RecoveryResourceInput {
            reference: locator,
            bytes: b"img".to_vec(),
        }],
    )?;
    let scan = ok(fixture.store.scan())?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    assert!(state.panel.install_scan(1, &scan));
    let job = match state.panel.request(
        RecoveryPanelAction::Restore { document },
        &fixture.store,
        Some(root.clone()),
    ) {
        RecoveryPanelStep::Read(job) => job,
        _ => return Err("恢复请求应生成读取任务".into()),
    };

    // 读取在飞：选择已受理但尚未完成。
    let target = fixture.path().join("busy.md");
    fs::write(&target, b"x\n")?;
    let request = state.request_open()?;
    assert!(
        state
            .accept_dialog(&request, Ok(Some(vec![target])))
            .is_some()
    );
    assert!(state.session().is_loading());

    let completion = job.run();
    let restored = match completion.result {
        Ok(restored) => restored,
        Err(error) => return Err(format!("恢复应成功物化：{error:?}").into()),
    };
    assert_eq!(regular_file_count(root.path())?, 1);
    let result = state
        .session_mut()
        .open_recovered_with(restored, |_| Ok::<(), ()>(()));
    assert!(matches!(result, Err(RecoveredOpenError::Busy)));
    assert!(state.session().document().is_none());
    assert_eq!(
        regular_file_count(root.path())?,
        0,
        "Busy 拒绝必须清理未采纳材料"
    );
    assert!(
        fs::read_dir(root.path())?.next().is_none(),
        "Busy 拒绝必须删除按文档创建的子目录"
    );
    let scan = ok(fixture.store.scan())?;
    assert_eq!(
        scan.recoverable_lineages()
            .iter()
            .filter(|lineage| lineage.document == document)
            .count(),
        1,
        "失败不得删除记录"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn adopted_recovered_resources_survive_restore() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-adopt")?;
    let document = fixture.lineage()?;
    let root = fixture.temp_root()?;
    let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
    fixture.publish(
        document,
        "img\n",
        vec![RecoveryResourceInput {
            reference: locator,
            bytes: b"img".to_vec(),
        }],
    )?;
    let scan = ok(fixture.store.scan())?;
    let mut state = super::NativeOpenCoordinator::new(super::open_limits());
    assert!(state.panel.install_scan(1, &scan));
    let job = match state.panel.request(
        RecoveryPanelAction::Restore { document },
        &fixture.store,
        Some(root.clone()),
    ) {
        RecoveryPanelStep::Read(job) => job,
        _ => return Err("恢复请求应生成读取任务".into()),
    };

    let restored = match state.panel.restore_finished(job.run()) {
        RecoveryPanelStep::Recovered(restored) => restored,
        _ => return Err("恢复完成应返回已恢复文档".into()),
    };
    state.session_mut().configure_temp_root(Some(root.clone()));
    let (open, _) = state
        .session_mut()
        .open_recovered_with(restored, |_| Ok::<(), ()>(()))
        .map_err(|error| format!("恢复文档应采纳：{error:?}"))?;
    assert_eq!(open.resources, 1);
    assert!(state.session().document().is_some());
    assert_eq!(regular_file_count(root.path())?, 1, "采纳后材料必须保留");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_panel_restore_failure_keeps_record_and_reports() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-restore-fail")?;
    let document = fixture.lineage()?;
    let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
    publish_record(
        &fixture.store,
        document,
        "with-image\n",
        vec![RecoveryResourceInput {
            reference: locator,
            bytes: b"img".to_vec(),
        }],
    )?;
    let scan = ok(fixture.store.scan())?;
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));
    let job = match panel.request(
        RecoveryPanelAction::Restore { document },
        &fixture.store,
        None,
    ) {
        RecoveryPanelStep::Read(job) => job,
        _ => return Err("恢复请求应生成读取任务".into()),
    };
    let completion = job.run();
    assert!(matches!(
        completion.result,
        Err(RecoveryRestoreError::Resources(
            ResourceError::UnsupportedPlatform
        ))
    ));
    assert!(matches!(
        panel.restore_finished(completion),
        RecoveryPanelStep::Ignore
    ));
    assert_eq!(panel.entries().len(), 1);
    assert_eq!(
        panel.entries()[0].status,
        RecoveryPanelEntryStatus::Available
    );
    assert_eq!(
        panel.feedback_key(),
        Some("document.recovery_restore_failed")
    );
    assert!(!panel.is_busy());
    let scan = ok(fixture.store.scan())?;
    assert_eq!(
        scan.recoverable_lineages()
            .iter()
            .filter(|lineage| lineage.document == document)
            .count(),
        1,
        "失败必须保留记录"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_panel_discard_requires_confirmation() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-discard-confirm")?;
    let document = fixture.lineage()?;
    fixture.publish(document, "keep\n", Vec::new())?;
    let scan = ok(fixture.store.scan())?;
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));

    assert!(matches!(
        panel.request(
            RecoveryPanelAction::RequestDiscard { document },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    assert_eq!(panel.confirming(), Some(document));
    assert_eq!(panel.entries().len(), 1);
    assert_eq!(ok(fixture.store.scan())?, scan, "确认前存储不得变化");

    assert!(matches!(
        panel.request(RecoveryPanelAction::CancelDiscard, &fixture.store, None),
        RecoveryPanelStep::Ignore
    ));
    assert!(panel.confirming().is_none());
    assert_eq!(panel.entries().len(), 1);
    assert_eq!(ok(fixture.store.scan())?, scan);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_panel_discard_success_hides_after_store_retirement() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-discard-ok")?;
    let first = fixture.lineage()?;
    fixture.publish(first, "first\n", Vec::new())?;
    let scan = ok(fixture.store.scan())?;
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));

    assert!(matches!(
        panel.request(
            RecoveryPanelAction::RequestDiscard { document: first },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    let job = match panel.request(RecoveryPanelAction::ConfirmDiscard, &fixture.store, None) {
        RecoveryPanelStep::Discard(job) => job,
        _ => return Err("确认后应生成丢弃任务".into()),
    };
    let completion = job.run();
    panel.discard_finished(completion);
    assert!(panel.entries().is_empty());
    assert!(panel.feedback_key().is_none());
    let scan = ok(fixture.store.scan())?;
    assert!(
        scan.recoverable_lineages()
            .iter()
            .all(|lineage| lineage.document != first),
        "退役后扫描不得再列出该谱系"
    );

    // `AlreadyRetired` 同样是成功退役；合成完成项验证该分支也移除条目。
    let second = fixture.lineage()?;
    fixture.publish(second, "second\n", Vec::new())?;
    let scan = ok(fixture.store.scan())?;
    assert!(panel.install_scan(2, &scan));
    assert!(matches!(
        panel.request(
            RecoveryPanelAction::RequestDiscard { document: second },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    let job = match panel.request(RecoveryPanelAction::ConfirmDiscard, &fixture.store, None) {
        RecoveryPanelStep::Discard(job) => job,
        _ => return Err("确认后应生成丢弃任务".into()),
    };
    let completion = RecoveryDiscardCompletion {
        document: second,
        operation: job.operation(),
        result: Ok(RetireOutcome::AlreadyRetired { through: 0 }),
    };
    panel.discard_finished(completion);
    assert!(panel.entries().is_empty());
    assert!(panel.feedback_key().is_none());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_panel_discard_failure_keeps_record_and_reports() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-discard-fail")?;
    let document = fixture.lineage()?;
    fixture.publish(document, "keep\n", Vec::new())?;
    let scan = ok(fixture.store.scan())?;
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));

    assert!(matches!(
        panel.request(
            RecoveryPanelAction::RequestDiscard { document },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    let job = match panel.request(RecoveryPanelAction::ConfirmDiscard, &fixture.store, None) {
        RecoveryPanelStep::Discard(job) => job,
        _ => return Err("确认后应生成丢弃任务".into()),
    };
    let lock = ok(fixture.store.hold_owner_lock_for_test(document))?;
    let completion = job.run();
    drop(lock);
    assert!(completion.result.is_err(), "锁占用应使丢弃失败");
    panel.discard_finished(completion);
    assert_eq!(panel.entries().len(), 1, "失败必须保留条目");
    assert_eq!(
        panel.feedback_key(),
        Some("document.recovery_discard_failed")
    );
    assert!(!panel.is_busy());
    let active = fixture
        .path()
        .join("recovery/v1/active")
        .join(document.directory_name());
    assert!(active.is_dir(), "失败不得删除谱系目录");
    assert_eq!(
        ok(fixture.store.scan())?
            .recoverable_lineages()
            .iter()
            .filter(|lineage| lineage.document == document)
            .count(),
        1
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_panel_discard_is_per_document_without_bulk_entry() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-per-document")?;
    let first = fixture.lineage()?;
    let second = fixture.lineage()?;
    fixture.publish(first, "first\n", Vec::new())?;
    fixture.publish(second, "second\n", Vec::new())?;
    let scan = ok(fixture.store.scan())?;
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));
    assert_eq!(panel.entries().len(), 2);

    // 穷举面板动作：每个变体至多携带一个文档，不存在批量入口。
    let actions = [
        RecoveryPanelAction::Restore { document: first },
        RecoveryPanelAction::Restore { document: second },
        RecoveryPanelAction::RequestDiscard { document: first },
        RecoveryPanelAction::RequestDiscard { document: second },
        RecoveryPanelAction::ConfirmDiscard,
        RecoveryPanelAction::CancelDiscard,
    ];
    for action in actions {
        match action {
            RecoveryPanelAction::Restore { document }
            | RecoveryPanelAction::RequestDiscard { document } => {
                assert!(document == first || document == second);
            }
            RecoveryPanelAction::ConfirmDiscard | RecoveryPanelAction::CancelDiscard => {}
        }
    }

    assert!(matches!(
        panel.request(
            RecoveryPanelAction::RequestDiscard { document: first },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    let job = match panel.request(RecoveryPanelAction::ConfirmDiscard, &fixture.store, None) {
        RecoveryPanelStep::Discard(job) => job,
        _ => return Err("确认后应生成丢弃任务".into()),
    };
    panel.discard_finished(job.run());
    assert_eq!(panel.entries().len(), 1);
    assert_eq!(panel.entries()[0].document, second);
    let scan = ok(fixture.store.scan())?;
    assert!(
        scan.recoverable_lineages()
            .iter()
            .all(|lineage| lineage.document != first)
    );
    assert!(
        scan.recoverable_lineages()
            .iter()
            .any(|lineage| lineage.document == second)
    );

    // 另一条必须重新确认，不会沿用上一条的确认态。
    assert!(matches!(
        panel.request(
            RecoveryPanelAction::RequestDiscard { document: second },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    assert_eq!(panel.confirming(), Some(second));
    assert_eq!(panel.entries().len(), 1);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovery_panel_marks_restored_record_for_later_discard() -> Result<(), Box<dyn Error>> {
    let fixture = RecoveryFixture::new("aline-panel-restored")?;
    let document = fixture.lineage()?;
    fixture.publish(document, "restored\n", Vec::new())?;
    let scan = ok(fixture.store.scan())?;
    let mut panel = RecoveryPanelState::default();
    assert!(panel.install_scan(1, &scan));
    let job = match panel.request(
        RecoveryPanelAction::Restore { document },
        &fixture.store,
        None,
    ) {
        RecoveryPanelStep::Read(job) => job,
        _ => return Err("恢复请求应生成读取任务".into()),
    };
    assert!(matches!(
        panel.restore_finished(job.run()),
        RecoveryPanelStep::Recovered(_)
    ));
    panel.mark_restored(document);
    assert_eq!(
        panel.entries()[0].status,
        RecoveryPanelEntryStatus::Restored
    );
    assert!(
        !super::recovery_panel::recovery_panel_visible(true, panel.is_visible()),
        "文档打开期间面板隐藏"
    );
    assert!(
        super::recovery_panel::recovery_panel_visible(false, panel.is_visible()),
        "文档关闭后恢复面板可见"
    );

    assert!(matches!(
        panel.request(
            RecoveryPanelAction::Restore { document },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    assert!(matches!(
        panel.request(
            RecoveryPanelAction::RequestDiscard { document },
            &fixture.store,
            None,
        ),
        RecoveryPanelStep::Ignore
    ));
    assert_eq!(panel.confirming(), Some(document), "已恢复条目只允许丢弃");
    Ok(())
}
