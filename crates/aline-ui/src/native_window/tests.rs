//! 原生窗口同源协调器的无窗口回归；真实 chooser/菜单/焦点另作 macOS 实验。

use std::error::Error;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

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
