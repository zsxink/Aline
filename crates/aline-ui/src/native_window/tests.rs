//! 原生窗口同源协调器的无窗口回归；真实 chooser/菜单/焦点另作 macOS 实验。

use std::error::Error;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use aline_buffer::SelectionSpec;

use crate::EditorMode;
use crate::{ReadonlyFileAccess, ReadonlyFileInput};

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
        let job = state
            .finish_save_wait(outcome, 3)
            .ok_or("平台自然结束应继续保存")?;
        let completion = job.run();
        assert_eq!(
            state.settle_save(completion).feedback,
            Some("document.save_saved")
        );
        assert_eq!(fs::read(&target)?, expected);
        assert!(
            state.finish_save_wait(outcome, 4).is_none(),
            "等待句柄不得复用"
        );
    }
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
        super::SaveSettlement {
            feedback: None,
            automatic: true,
        },
        "自动保存成功必须静默"
    );
    assert_eq!(fs::read(&target)?, b"xa\n", "自动保存应写入磁盘");

    let job = state.request_save().ok_or("手工保存应生成后台工作")?;
    assert_eq!(
        state.settle_save(job.run()),
        super::SaveSettlement {
            feedback: Some("document.save_saved"),
            automatic: false,
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
