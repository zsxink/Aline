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
    let latest = state
        .read_finished(running.read_with(&Files(b"A")))
        .ok_or("missing latest queued read")?;
    assert!(
        state
            .read_finished(latest.read_with(&Files(b"C")))
            .is_none()
    );
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
    assert!(state.read_finished(job.read_with(&Files(b"A"))).is_none());
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
    state.read_finished(job.read_with(&Files(&bytes)));
    let document = state.session().document().ok_or("document")?;
    assert_eq!(document.core().mode(), EditorMode::Source);
    assert_eq!(state.feedback_key(), Some("reading.large_source"));
    assert_eq!(document.core().history_node_count(), 0);
    assert!(!document.core().has_open_block());
    Ok(())
}
