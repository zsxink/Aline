//! 原生编辑动作和剪贴板入口。文档核心负责语义与历史，窗口负责平台副作用及派生视图。

use aline_buffer::{BufferError, TextBudget};
use aline_markdown::Parsed;
use gpui::{App, ClipboardEntry, ClipboardItem, Context, Window};

use super::readonly_layout::{LayoutError, ReaderIndex};
use super::{NativeDocumentWindow, NativeOpenCoordinator, ReaderSurface, layout_error_key};
use crate::{
    DocError, EditorMode, ParseRefreshCompletion, ParseRefreshDisposition, ParseRefreshJob,
    ReadonlyDocument,
};

const MAX_READER_ROWS: usize = 1_000_000;

impl NativeDocumentWindow {
    pub(super) fn request_view(
        &mut self,
        mode: EditorMode,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let before = self.state.session.document().map(|doc| doc.core().mode());
        self.switch_view(mode, cx);
        if self.state.session.document().is_some_and(|document| {
            document.is_editable()
                && Some(document.core().mode()) != before
                && !document.is_parse_current()
        }) {
            self.refresh_after_edit(window, cx);
        }
    }

    pub(super) fn edit_document(
        &mut self,
        action: EditAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let now_millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match self
            .state
            .edit(action, |budget| clipboard_text(cx, budget), now_millis)
        {
            Ok(effect) => {
                self.state.feedback = None;
                if let Some(text) = effect.clipboard {
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                if effect.refresh {
                    self.refresh_after_edit(window, cx);
                }
            }
            Err(DocError::ClosedTarget | DocError::ReadOnly) => return,
            Err(error) => self.state.feedback = Some(edit_error_key(error)),
        }
        cx.notify();
    }

    pub(super) fn refresh_after_edit(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.invalidate_statistics();
        match self.state.queue_refresh() {
            Ok(Some(job)) => self.start_refresh(job, window, cx),
            Ok(None) => {}
            Err(_) => self.state.feedback = Some("document.refresh_failed"),
        }
    }

    fn start_refresh(&mut self, job: ParseRefreshJob, window: &Window, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.parse_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                let generation = this.next_reader_generation.checked_add(1);
                let (next, prepared) =
                    this.state.refresh_finished(completion, |document, parsed| {
                        let generation = generation.ok_or(LayoutError::GenerationExhausted)?;
                        let index = ReaderIndex::from_parsed(
                            parsed,
                            MAX_READER_ROWS,
                            document.text_budget(),
                        )?;
                        Ok((
                            ReaderSurface::from_index(
                                index,
                                document.core().document_id().clone(),
                                document.core().mode(),
                                generation,
                                this.evidence.clone(),
                            ),
                            generation,
                        ))
                    });
                this.parse_task = None;
                if let Some((reader, generation)) = prepared {
                    if let Some(document) = this.state.session.document()
                        && let Err(error) = reader
                            .scroll_to_source(document, document.core().current_selection().head())
                    {
                        this.state.feedback = Some(layout_error_key(error));
                    }
                    reader.schedule_height_hint(window, cx);
                    this.reader = Some(reader);
                    this.next_reader_generation = generation;
                    this.refresh_statistics(window, cx);
                }
                if let Some(job) = next {
                    this.start_refresh(job, window, cx);
                }
                cx.notify();
            });
        }));
    }
}

#[derive(Clone, Copy)]
pub(super) enum EditAction {
    Cut,
    Copy,
    Paste,
    SelectAll,
    Undo,
    Redo,
}

struct EditEffect {
    clipboard: Option<String>,
    refresh: bool,
}

impl NativeOpenCoordinator {
    fn edit(
        &mut self,
        action: EditAction,
        read_clipboard: impl FnOnce(TextBudget) -> Result<Option<String>, DocError>,
        now_millis: u64,
    ) -> Result<EditEffect, DocError> {
        let document = self.session.document_mut().ok_or(DocError::ClosedTarget)?;
        let budget = document.text_budget();
        let core = document.edit_core()?;
        if core.composition().is_some() {
            return Err(DocError::CompositionActive);
        }
        let before = core.version();
        let mut clipboard = None;
        match action {
            EditAction::Cut => {
                let identity = core.current_input_identity()?;
                let text = core.cut_selection(&identity, budget, now_millis)?;
                clipboard = (!text.is_empty()).then_some(text);
            }
            EditAction::Copy => {
                let text = core.copy_selection(budget)?;
                clipboard = (!text.is_empty()).then_some(text);
            }
            EditAction::Paste => {
                if let Some(text) = read_clipboard(budget)?.filter(|text| !text.is_empty()) {
                    let identity = core.current_input_identity()?;
                    core.replace_selection(&identity, &text, now_millis)?;
                }
            }
            EditAction::SelectAll => {
                let identity = core.current_input_identity()?;
                core.select_all(&identity)?;
            }
            EditAction::Undo => core.undo()?,
            EditAction::Redo => core.redo()?,
        }
        Ok(EditEffect {
            clipboard,
            refresh: before != core.version(),
        })
    }

    fn queue_refresh(&mut self) -> Result<Option<ParseRefreshJob>, DocError> {
        self.queued_refresh = Some(self.session.begin_parse_refresh()?);
        Ok(self.take_next_refresh())
    }

    fn take_next_refresh(&mut self) -> Option<ParseRefreshJob> {
        if self.refreshing || self.closed {
            return None;
        }
        let job = self.queued_refresh.take()?;
        self.refreshing = true;
        Some(job)
    }

    fn refresh_finished<T>(
        &mut self,
        completion: ParseRefreshCompletion,
        prepare: impl FnOnce(&ReadonlyDocument, &Parsed) -> Result<T, LayoutError>,
    ) -> (Option<ParseRefreshJob>, Option<T>) {
        self.refreshing = false;
        let prepared = match self.session.accept_parse_refresh_with(completion, prepare) {
            Ok((ParseRefreshDisposition::Applied, prepared)) => prepared,
            Ok((ParseRefreshDisposition::Failed(_), _)) => {
                self.feedback = Some("document.refresh_failed");
                None
            }
            Ok(_) => None,
            Err(error) => {
                self.feedback = Some(layout_error_key(error));
                None
            }
        };
        (self.take_next_refresh(), prepared)
    }
}

fn clipboard_text(cx: &App, budget: TextBudget) -> Result<Option<String>, DocError> {
    let Some(item) = cx.read_from_clipboard() else {
        return Ok(None);
    };
    let mut text = String::new();
    for entry in item.entries() {
        let ClipboardEntry::String(entry) = entry else {
            continue;
        };
        let fragment = entry.text();
        let needed = text
            .len()
            .checked_add(fragment.len())
            .ok_or(DocError::Buffer(BufferError::LengthOverflow))?;
        if needed > budget.max_bytes() {
            return Err(DocError::Buffer(BufferError::BudgetExceeded {
                needed,
                limit: budget.max_bytes(),
            }));
        }
        text.try_reserve(fragment.len())
            .map_err(|error| DocError::Buffer(BufferError::Allocation(error)))?;
        text.push_str(fragment);
    }
    Ok((!text.is_empty()).then_some(text))
}

pub(super) fn edit_error_key(error: DocError) -> &'static str {
    match error {
        DocError::CompositionActive => "document.composition_pending",
        _ => "document.edit_refused",
    }
}

#[cfg(test)]
mod tests {
    use std::{error::Error, io::Cursor, path::Path};

    use std::path::PathBuf;

    use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec};
    use aline_markdown::{ParseBudget, ParseConfig};
    use aline_source::StorageBudget;

    use super::*;
    use crate::native_window::open_limits;
    use crate::{
        DocLimits, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenJob, ReadonlyOpenLimits,
    };

    struct Files(&'static [u8]);

    impl ReadonlyFileAccess for Files {
        type Reader = Cursor<&'static [u8]>;

        fn open(&self, _: &Path) -> std::io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(
                Cursor::new(self.0),
                self.0.len() as u64,
                true,
            ))
        }
    }

    fn open_job(state: &mut NativeOpenCoordinator) -> Result<ReadonlyOpenJob, Box<dyn Error>> {
        let request = state.request_open()?;
        state
            .accept_dialog(&request, Ok(Some(vec![PathBuf::from("fixture.md")])))
            .ok_or_else(|| "missing read job".into())
    }

    /// 构造只读会话预算；生产 `open_limits()` 自 M2 起可编辑，只读路径需显式固定。
    fn read_only_limits() -> ReadonlyOpenLimits {
        ReadonlyOpenLimits::new(
            StorageBudget::new(4096),
            DocLimits::new(
                TextBudget::new(4096),
                LineIndexBudget::new(128),
                HistoryBudget::new(0, 0),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
    }

    fn open(bytes: &'static [u8], editable: bool) -> Result<NativeOpenCoordinator, Box<dyn Error>> {
        let limits = if editable {
            ReadonlyOpenLimits::new(
                StorageBudget::new(4096),
                DocLimits::new(
                    TextBudget::new(4096),
                    LineIndexBudget::new(128),
                    HistoryBudget::new(16, 4096),
                ),
                ParseConfig::m0(),
                ParseBudget::m0(),
            )
            .with_editing()
        } else {
            read_only_limits()
        };
        let mut state = NativeOpenCoordinator::new(limits);
        let job = open_job(&mut state)?;
        let (_, applied) = state.read_finished(job.read_with(&Files(bytes)), |_| Ok(()));
        assert_eq!(applied, Some(()));
        state
            .session
            .document_mut()
            .ok_or("missing document")?
            .switch_mode(EditorMode::Source)?;
        Ok(state)
    }

    #[test]
    fn clipboard_never_reads_without_an_editable_document() -> Result<(), Box<dyn Error>> {
        let mut empty = NativeOpenCoordinator::new(open_limits());
        let mut readonly = open(b"readonly", false)?;
        let mut reads = 0;
        for (state, expected) in [
            (&mut empty, DocError::ClosedTarget),
            (&mut readonly, DocError::ReadOnly),
        ] {
            let error = state
                .edit(
                    EditAction::Paste,
                    |_| {
                        reads += 1;
                        Ok(Some(String::from("new")))
                    },
                    0,
                )
                .err()
                .ok_or("unexpected paste success")?;
            assert_eq!(
                std::mem::discriminant(&error),
                std::mem::discriminant(&expected)
            );
        }
        assert_eq!(reads, 0);
        let doc = readonly.session.document().ok_or("missing document")?;
        assert_eq!(doc.core().buffer_text()?, "readonly");
        assert_eq!(doc.core().history_node_count(), 0);
        Ok(())
    }

    #[test]
    fn clipboard_cut_only_returns_frozen_text_after_a_commit() -> Result<(), Box<dyn Error>> {
        let mut state = open("甲🙂乙".as_bytes(), true)?;
        let collapsed = state.edit(EditAction::Cut, |_| Ok(None), 0)?;
        assert!(collapsed.clipboard.is_none());
        assert!(!collapsed.refresh);
        let core = state
            .session
            .document_mut()
            .ok_or("missing doc")?
            .edit_core()?;
        let snapshot = core.current_snapshot(TextBudget::new(4096))?;
        let text = snapshot.text();
        let selection = aline_buffer::SrcSelection::new(text.src_offset(7)?, text.src_offset(3)?)?;
        let identity = core.current_input_identity()?;
        core.select(&identity, &selection)?;
        let copied = state.edit(EditAction::Copy, |_| Ok(None), 1)?;
        assert_eq!(copied.clipboard.as_deref(), Some("🙂"));
        assert!(!copied.refresh);
        let unchanged = state.edit(EditAction::Paste, |_| Ok(Some(String::from("🙂"))), 2)?;
        assert!(unchanged.clipboard.is_none());
        assert!(!unchanged.refresh);
        let cut = state.edit(EditAction::Cut, |_| Ok(None), 3)?;
        assert_eq!(cut.clipboard.as_deref(), Some("🙂"));
        assert!(cut.refresh);
        let doc = state.session.document().ok_or("missing doc")?;
        assert_eq!(doc.core().buffer_text()?, "甲乙");
        assert_eq!(doc.core().undo_depth(), 1);
        assert!(!state.close_document());
        let replacement = open_job(&mut state)?;
        let (_, applied) =
            state.read_finished(replacement.read_with(&Files(b"replacement")), |_| Ok(()));
        assert_eq!(applied, None);
        assert!(state.edit(EditAction::Undo, |_| Ok(None), 4)?.refresh);
        let doc = state.session.document().ok_or("missing doc")?;
        assert_eq!(doc.core().buffer_text()?, "甲🙂乙");
        assert_eq!(doc.core().current_selection(), SelectionSpec::new(7, 3));
        assert!(state.edit(EditAction::Redo, |_| Ok(None), 5)?.refresh);
        let doc = state.session.document().ok_or("missing doc")?;
        assert_eq!(doc.core().buffer_text()?, "甲乙");
        assert_eq!(doc.core().current_selection(), SelectionSpec::new(3, 3));
        Ok(())
    }

    #[test]
    fn clipboard_cut_stages_pending_input_before_its_own_undo_step() -> Result<(), Box<dyn Error>> {
        let mut state = open(b"base", true)?;
        let core = state
            .session
            .document_mut()
            .ok_or("missing doc")?
            .edit_core()?;
        let identity = core.current_input_identity()?;
        core.type_selection(&identity, "甲", 1)?;
        let selected = state.edit(EditAction::SelectAll, |_| Ok(None), 2)?;
        assert!(selected.clipboard.is_none());
        assert!(!selected.refresh);
        let cut = state.edit(EditAction::Cut, |_| Ok(None), 3)?;
        assert_eq!(cut.clipboard.as_deref(), Some("甲base"));
        assert!(cut.refresh);
        let core = state.session.document().ok_or("missing doc")?.core();
        assert_eq!(core.buffer_text()?, "");
        assert_eq!(core.undo_depth(), 2);
        state.edit(EditAction::Undo, |_| Ok(None), 4)?;
        let core = state.session.document().ok_or("missing doc")?.core();
        assert_eq!(core.buffer_text()?, "甲base");
        assert_eq!(core.current_selection(), SelectionSpec::new(0, 7));
        state.edit(EditAction::Undo, |_| Ok(None), 5)?;
        let core = state.session.document().ok_or("missing doc")?.core();
        assert_eq!(core.buffer_text()?, "base");
        assert_eq!(core.current_selection(), SelectionSpec::new(0, 0));
        assert!(state.close_document());
        Ok(())
    }

    #[test]
    fn edit_actions_during_composition_preserve_input_and_never_read_clipboard()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"before", true)?;
        let document = state.session.document_mut().ok_or("missing document")?;
        let core = document.edit_core()?;
        let identity = core.current_input_identity()?;
        core.type_selection(&identity, "x", 1)?;
        core.composition_start("候选", 2)?;
        let version = core.version();
        let selection = core.current_selection();
        let depth = core.history_node_count();
        let mut reads = 0;
        for action in [
            EditAction::Cut,
            EditAction::Copy,
            EditAction::Paste,
            EditAction::SelectAll,
            EditAction::Undo,
            EditAction::Redo,
        ] {
            assert!(matches!(
                state.edit(
                    action,
                    |_| {
                        reads += 1;
                        Ok(Some(String::from("clipboard")))
                    },
                    3
                ),
                Err(DocError::CompositionActive)
            ));
        }
        assert_eq!(reads, 0);
        let core = state.session.document().ok_or("missing doc")?.core();
        assert!(core.composition().is_some());
        assert_eq!(core.version(), version);
        assert_eq!(core.current_selection(), selection);
        assert_eq!(core.history_node_count(), depth);
        Ok(())
    }

    #[test]
    fn parse_refresh_bounds_concurrency_and_preserves_view_on_preparation_failure()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"base", true)?;
        let original = state
            .session
            .document()
            .ok_or("missing document")?
            .parsed()
            .snapshot()
            .id()
            .clone();
        let loading = state.queue_refresh()?.ok_or("missing first refresh")?;
        state.edit(EditAction::Paste, |_| Ok(Some(String::from("甲"))), 1)?;
        assert!(state.queue_refresh()?.is_none());
        state.edit(EditAction::Paste, |_| Ok(Some(String::from("🙂"))), 2)?;
        assert!(state.queue_refresh()?.is_none());
        let (next, applied) = state.refresh_finished(loading.run(), |_, _| Ok(()));
        assert_eq!(applied, None);
        let latest = next.ok_or("missing latest refresh")?;
        let (_, applied) =
            state.refresh_finished(latest.run(), |_, _| Err::<(), _>(LayoutError::Budget));
        assert_eq!(applied, None);
        let doc = state.session.document().ok_or("missing document")?;
        assert_eq!(doc.parsed().snapshot().id(), &original);
        assert!(!doc.is_parse_current());
        assert_eq!(doc.core().buffer_text()?, "甲🙂base");
        let retry = state.queue_refresh()?.ok_or("missing retry")?;
        let (_, applied) = state.refresh_finished(retry.run(), |_, _| Ok(()));
        assert_eq!(applied, Some(()));
        let doc = state.session.document().ok_or("missing document")?;
        assert!(doc.is_parse_current());
        assert_ne!(doc.parsed().snapshot().id(), &original);
        Ok(())
    }

    #[test]
    fn closed_document_discards_queued_and_late_refresh_without_preparation()
    -> Result<(), Box<dyn Error>> {
        let mut state = open(b"base", true)?;
        let completion = state.queue_refresh()?.ok_or("missing first refresh")?.run();
        assert!(state.queue_refresh()?.is_none());
        assert!(state.close_document());
        let mut prepared = false;
        let (next, applied) = state.refresh_finished(completion, |_, _| {
            prepared = true;
            Ok(())
        });
        assert!(next.is_none());
        assert!(applied.is_none());
        assert!(!prepared);
        assert!(state.session.document().is_none());
        Ok(())
    }
}
