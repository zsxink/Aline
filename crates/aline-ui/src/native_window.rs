//! 原生单窗的打开协调与最小只读表面。
//!
//! 窗口持有一个只读会话、一个实际读取及至多一个最新候选；文件选择、任务与
//! 焦点在本层装配。读取和解析在后台完成，render 只读当前状态，不读盘或解析。

#[path = "native_window/readonly_hit.rs"]
mod readonly_hit;
#[path = "native_window/readonly_layout.rs"]
mod readonly_layout;
#[path = "native_window/readonly_reader.rs"]
mod readonly_reader;

use readonly_hit::{HitError, HitTarget, PreparedLayout};
use readonly_reader::ReaderSurface;

use std::collections::BTreeMap;
use std::path::PathBuf;

use aline_app_core::Translator;
use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use gpui::{
    App, Bounds, Context, FocusHandle, KeyBinding, Menu, MenuItem, PathPromptOptions, Role,
    SharedString, Task, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, size,
};

use crate::{
    DocLimits, EditorMode, OpenDisposition, OpenError, OpenRequestId, ReadonlyOpenCompletion,
    ReadonlyOpenJob, ReadonlyOpenLimits, ReadonlySession,
};

gpui::actions!(
    m1_window,
    [
        OpenDocument,
        CloseDocument,
        FocusNext,
        FocusPrevious,
        VisualView,
        SourceView,
        ReadPageUp,
        ReadPageDown,
        ReadStart,
        ReadEnd,
        CancelNavigation
    ]
);

#[cfg(debug_assertions)]
gpui::actions!(m1_debug, [ReportLayout]);

/// M1 实验预算沿用现有 16 MiB 解析输入上限；不是发布限额。
pub(super) fn open_limits() -> ReadonlyOpenLimits {
    const MAX_BYTES: usize = 16 * 1024 * 1024;
    ReadonlyOpenLimits::new(
        StorageBudget::new(MAX_BYTES),
        DocLimits::new(
            TextBudget::new(MAX_BYTES),
            LineIndexBudget::new(1_000_000),
            HistoryBudget::new(0, 0),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )
}

/// 无 GPUI 的窗口协调规则；会话继续独占文档与请求资格。
pub(super) struct NativeOpenCoordinator {
    session: ReadonlySession,
    reading: bool,
    queued: Option<ReadonlyOpenJob>,
    feedback: Option<&'static str>,
    return_focus: bool,
    closed: bool,
}

impl NativeOpenCoordinator {
    pub(super) fn new(limits: ReadonlyOpenLimits) -> Self {
        Self {
            session: ReadonlySession::new(limits),
            reading: false,
            queued: None,
            feedback: None,
            return_focus: false,
            closed: false,
        }
    }

    pub(super) fn session(&self) -> &ReadonlySession {
        &self.session
    }

    pub(super) fn feedback_key(&self) -> Option<&'static str> {
        self.feedback
    }

    pub(super) fn take_focus_return(&mut self) -> bool {
        std::mem::take(&mut self.return_focus)
    }

    pub(super) fn request_open(&mut self) -> Result<OpenRequestId, OpenError> {
        let request = self.session.request_open()?;
        self.queued = None;
        self.feedback = None;
        self.return_focus = false;
        Ok(request)
    }

    pub(super) fn accept_dialog(
        &mut self,
        request: &OpenRequestId,
        selection: Result<Option<Vec<PathBuf>>, ()>,
    ) -> Option<ReadonlyOpenJob> {
        let selection = match selection {
            Ok(selection) => selection,
            Err(()) => {
                if self.session.cancel_open(request) {
                    self.feedback = Some("open_error.dialog");
                    self.return_focus = !self.closed;
                }
                return None;
            }
        };
        match self.session.accept_selection(request, selection) {
            Ok(job) => {
                self.return_focus = !self.closed;
                self.feedback = None;
                self.queued = job;
                self.take_next_read()
            }
            Err(OpenError::StaleRequest | OpenError::ClosedWindow) => None,
            Err(error) => {
                self.feedback = Some(open_error_key(&error));
                self.return_focus = !self.closed;
                None
            }
        }
    }

    pub(super) fn read_finished(
        &mut self,
        completion: ReadonlyOpenCompletion,
    ) -> Option<ReadonlyOpenJob> {
        self.reading = false;
        match self.session.accept_open(completion) {
            OpenDisposition::Failed(error) => self.feedback = Some(open_error_key(&error)),
            OpenDisposition::Applied => {
                if let Some(document) = self.session.document_mut() {
                    let (mode, reason) =
                        readonly_layout::initial_view(document.parsed().snapshot().len_bytes());
                    if document.switch_mode(mode).is_err() {
                        self.feedback = Some("open_error.internal");
                    } else {
                        self.feedback = reason;
                    }
                }
            }
            OpenDisposition::Cancelled | OpenDisposition::Stale => {}
        }
        self.take_next_read()
    }

    fn take_next_read(&mut self) -> Option<ReadonlyOpenJob> {
        if self.reading || self.closed {
            return None;
        }
        let job = self.queued.take()?;
        if job.is_cancelled() {
            return None;
        }
        self.reading = true;
        Some(job)
    }

    fn close_document(&mut self) {
        self.session.close_document();
        self.queued = None;
        self.feedback = None;
        self.return_focus = false;
    }

    pub(super) fn close_window(&mut self) {
        self.close_document();
        self.session.close_window();
        self.closed = true;
    }
}

fn open_error_key(error: &OpenError) -> &'static str {
    match error {
        OpenError::SelectionCount => "open_error.selection",
        OpenError::UnsupportedExtension => "open_error.extension",
        OpenError::NotRegularFile => "open_error.not_regular",
        OpenError::BudgetExceeded => "open_error.budget",
        OpenError::Io(_) => "open_error.io",
        OpenError::Decode(_) => "open_error.decode",
        OpenError::ClosedWindow
        | OpenError::RequestExhausted
        | OpenError::StaleRequest
        | OpenError::Cancelled
        | OpenError::InvalidRead
        | OpenError::Storage(_)
        | OpenError::Document(_)
        | OpenError::Parse(_)
        | OpenError::Allocation(_) => "open_error.internal",
    }
}

pub(super) struct NativeWindowCopy {
    title: SharedString,
    empty: SharedString,
    open: SharedString,
    close: SharedString,
    file_menu: SharedString,
    selecting: SharedString,
    loading: SharedString,
    readonly: SharedString,
    visual_view: SharedString,
    source_view: SharedString,
    cancel_navigation: SharedString,
    errors: BTreeMap<&'static str, SharedString>,
}

impl NativeWindowCopy {
    pub(super) fn new(translator: &Translator) -> Result<Self, String> {
        let text = |key| {
            translator
                .t(key)
                .map(SharedString::from)
                .map_err(|error| error.to_string())
        };
        let mut errors = BTreeMap::new();
        for key in [
            "open_error.selection",
            "open_error.extension",
            "open_error.not_regular",
            "open_error.budget",
            "open_error.io",
            "open_error.decode",
            "open_error.internal",
            "open_error.dialog",
            "reading.large_source",
            "reading.layout_budget",
        ] {
            errors.insert(key, text(key)?);
        }
        Ok(Self {
            title: text("app.name")?,
            empty: text("window.empty")?,
            open: text("action.open")?,
            close: text("action.close_document")?,
            file_menu: text("menu.file")?,
            selecting: text("window.selecting")?,
            loading: text("window.loading")?,
            readonly: text("window.readonly")?,
            visual_view: text("action.visual_view")?,
            source_view: text("action.source_view")?,
            cancel_navigation: text("action.cancel_navigation")?,
            errors,
        })
    }

    fn install_menu(&self, cx: &App, can_close: bool) {
        cx.set_menus([
            Menu::new(self.title.clone()),
            Menu::new(self.file_menu.clone()).items([
                MenuItem::action(self.open.clone(), OpenDocument),
                MenuItem::action(self.close.clone(), CloseDocument).disabled(!can_close),
            ]),
        ]);
    }
}

pub(crate) fn open_initial_window(
    cx: &mut App,
    copy: NativeWindowCopy,
    locale_fallback: Option<SharedString>,
) -> Result<(), String> {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenDocument, Some("M1Window")),
        KeyBinding::new("cmd-w", CloseDocument, Some("M1Window")),
        KeyBinding::new("tab", FocusNext, Some("M1Window")),
        KeyBinding::new("shift-tab", FocusPrevious, Some("M1Window")),
        KeyBinding::new("pageup", ReadPageUp, Some("M1Window")),
        KeyBinding::new("pagedown", ReadPageDown, Some("M1Window")),
        KeyBinding::new("home", ReadStart, Some("M1Window")),
        KeyBinding::new("end", ReadEnd, Some("M1Window")),
        KeyBinding::new("escape", CancelNavigation, Some("M1Window")),
    ]);
    #[cfg(debug_assertions)]
    cx.bind_keys([KeyBinding::new("cmd-alt-l", ReportLayout, Some("M1Window"))]);
    copy.install_menu(cx, false);
    let bounds = Bounds::centered(None, size(px(960.0), px(640.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..WindowOptions::default()
        },
        move |window, cx| {
            window.set_window_title(copy.title.as_ref());
            let entity = cx.new(|cx| NativeDocumentWindow::new(copy, locale_fallback, window, cx));
            let weak = entity.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                let _ = weak.update(cx, |this, _| this.state.close_window());
                true
            });
            entity
        },
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

struct NativeDocumentWindow {
    copy: NativeWindowCopy,
    locale_fallback: Option<SharedString>,
    state: NativeOpenCoordinator,
    focus: FocusHandle,
    dialog_task: Option<Task<()>>,
    dialog_return_focus: Option<FocusHandle>,
    read_task: Option<Task<()>>,
    loaded_name: Option<SharedString>,
    reader: Option<ReaderSurface>,
    next_reader_generation: u64,
}

impl NativeDocumentWindow {
    fn new(
        copy: NativeWindowCopy,
        locale_fallback: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        cx.on_release(|this, _| this.state.close_window()).detach();
        Self {
            copy,
            locale_fallback,
            state: NativeOpenCoordinator::new(open_limits()),
            focus,
            dialog_task: None,
            dialog_return_focus: None,
            read_task: None,
            loaded_name: None,
            reader: None,
            next_reader_generation: 0,
        }
    }

    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_task.is_some() {
            return;
        }
        let request = match self.state.request_open() {
            Ok(request) => request,
            Err(error) => {
                self.state.feedback = Some(open_error_key(&error));
                cx.notify();
                return;
            }
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.copy.open.clone()),
        });
        self.dialog_return_focus = window.focused(cx);
        self.dialog_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let selection = match selection.await {
                Ok(Ok(selection)) => Ok(selection),
                _ => Err(()),
            };
            let _ = weak.update_in(cx, |this, window, cx| {
                this.dialog_task = None;
                let job = this.state.accept_dialog(&request, selection);
                if this.state.take_focus_return() {
                    this.dialog_return_focus
                        .take()
                        .unwrap_or_else(|| this.focus.clone())
                        .focus(window, cx);
                }
                if let Some(job) = job {
                    this.start_read(job, window, cx);
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
        self.update_menu(cx);
        cx.notify();
    }

    fn start_read(&mut self, job: ReadonlyOpenJob, window: &Window, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.read_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.read_task = None;
                let next = this.state.read_finished(completion);
                this.loaded_name = this
                    .state
                    .session()
                    .document()
                    .and_then(|document| document.path().file_name())
                    .map(|name| SharedString::from(name.to_string_lossy().into_owned()));
                this.refresh_reader();
                if let Some(name) = &this.loaded_name {
                    window.set_window_title(name.as_ref());
                }
                if let Some(job) = next {
                    this.start_read(job, window, cx);
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    fn close_document(&mut self, _: &CloseDocument, window: &mut Window, cx: &mut Context<Self>) {
        self.state.close_document();
        self.loaded_name = None;
        self.reader = None;
        window.set_window_title(self.copy.title.as_ref());
        self.focus.focus(window, cx);
        self.update_menu(cx);
        cx.notify();
    }

    fn refresh_reader(&mut self) {
        let Some(document) = self.state.session().document() else {
            self.reader = None;
            return;
        };
        if self
            .reader
            .as_ref()
            .is_none_or(|reader| !reader.matches(document))
        {
            let Some(generation) = self.next_reader_generation.checked_add(1) else {
                self.state.feedback = Some("open_error.internal");
                return;
            };
            match ReaderSurface::new(document, generation) {
                Ok(reader) => {
                    self.reader = Some(reader);
                    self.next_reader_generation = generation;
                }
                Err(_) => {
                    self.reader = None;
                    self.state.feedback = Some("open_error.budget");
                }
            }
        }
    }

    fn switch_view(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        if let Some(document) = self.state.session.document_mut() {
            if document.switch_mode(mode).is_err() {
                self.state.feedback = Some("open_error.internal");
            } else if self.state.feedback == Some("reading.large_source") {
                self.state.feedback = None;
            }
            cx.notify();
        }
    }

    fn render_reading_row(&mut self, row: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(reader) = &self.reader else {
            return div().into_any_element();
        };
        let Some(document) = self.state.session.document_mut() else {
            return div().into_any_element();
        };
        let failure = self
            .copy
            .errors
            .get("reading.layout_budget")
            .cloned()
            .unwrap_or_default();
        let weak = cx.entity().downgrade();
        reader.render_row(
            document,
            row,
            failure,
            move |target, candidate, prepared, _, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.accept_reading_hit(target, candidate, prepared, cx)
                });
            },
        )
    }

    fn accept_reading_hit(
        &mut self,
        target: &HitTarget,
        candidate: Result<Result<usize, usize>, HitError>,
        prepared: &PreparedLayout,
        cx: &mut Context<Self>,
    ) {
        let Some(reader) = &self.reader else {
            return;
        };
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        let expected_edge = candidate
            .as_ref()
            .ok()
            .is_some_and(|candidate| candidate.is_err());
        let outcome = candidate.and_then(|candidate| {
            target.navigate_candidate(document, reader.layout_token(), candidate)
        });
        if let Ok(hit) = &outcome {
            debug_assert_eq!(hit.is_edge_candidate(), expected_edge);
            debug_assert!(match hit.precision() {
                aline_markdown::MappingPrecision::Exact
                | aline_markdown::MappingPrecision::Synthetic => hit.source_range().is_empty(),
                aline_markdown::MappingPrecision::Aligned
                | aline_markdown::MappingPrecision::Hidden => !hit.source_range().is_empty(),
            });
        }
        #[cfg(debug_assertions)]
        match &outcome {
            Ok(outcome) => eprintln!(
                "m1-hit accepted precision={:?} source_start={} source_end={} anchor={} edge={} layout_text_matches_projection={} open={} history={} revision={}",
                outcome.precision(),
                outcome.source_range().byte_range().start,
                outcome.source_range().byte_range().end,
                document.core().buffer_selection().head(),
                outcome.is_edge_candidate(),
                target.layout_text_matches(prepared).unwrap_or(false),
                document.core().has_open_block(),
                document.core().history_node_count(),
                document.core().version().revision().value()
            ),
            Err(error) => eprintln!("m1-hit rejected={error:?}"),
        }
        #[cfg(not(debug_assertions))]
        let _ = (outcome, prepared);
        cx.notify();
    }

    fn update_menu(&self, cx: &App) {
        self.copy.install_menu(
            cx,
            self.state.session().document().is_some() || self.state.session().is_loading(),
        );
    }
}

impl Render for NativeDocumentWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let phase = self
            .state
            .session()
            .pending_phase()
            .map(|phase| match phase {
                crate::ReadonlyOpenPhase::Selecting => self.copy.selecting.clone(),
                crate::ReadonlyOpenPhase::Loading => self.copy.loading.clone(),
            });
        if let (Some(reader), Some(document)) = (&mut self.reader, self.state.session().document())
            && reader.prepare(document, window).is_err()
        {
            self.state.feedback = Some("open_error.budget");
        }
        let reading = self.reader.is_some();
        let entity = cx.entity().downgrade();
        let reader_element = self.reader.as_ref().map(|reader| {
            reader.render(move |row, _, cx| {
                entity
                    .update(cx, |this, cx| this.render_reading_row(row, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
        });
        let root = div()
            .size_full()
            .bg(rgb(0xFFFFFF))
            .text_color(rgb(0x1F2328))
            .key_context("M1Window")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::close_document))
            .on_action(cx.listener(|this, _: &CancelNavigation, _, cx| {
                if let Some(reader) = &this.reader
                    && reader.cancel_restore()
                {
                    cx.notify();
                }
            }))
            .on_action(
                cx.listener(|this, _: &VisualView, _, cx| this.switch_view(EditorMode::Visual, cx)),
            )
            .on_action(
                cx.listener(|this, _: &SourceView, _, cx| this.switch_view(EditorMode::Source, cx)),
            )
            .on_action(cx.listener(|this, _: &ReadPageUp, window, cx| {
                if let Some(reader) = &this.reader {
                    reader.page(-1.0, window.viewport_size().height);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ReadPageDown, window, cx| {
                if let Some(reader) = &this.reader {
                    reader.page(1.0, window.viewport_size().height);
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ReadStart, _, cx| {
                if let Some(reader) = &this.reader {
                    reader.start();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ReadEnd, _, cx| {
                if let Some(reader) = &this.reader {
                    reader.end();
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|_, _: &FocusNext, window, cx| window.focus_next(cx)))
            .on_action(cx.listener(|_, _: &FocusPrevious, window, cx| window.focus_prev(cx)))
            .flex()
            .flex_col()
            .when(!reading, |root| {
                root.justify_center().items_center().gap_3()
            })
            .when(reading, |root| {
                root.child(
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap_3()
                        .px_4()
                        .py_2()
                        .border_b_1()
                        .border_color(rgb(0xD1D9E0))
                        .child(
                            div()
                                .flex_1()
                                .child(self.loaded_name.clone().unwrap_or_default()),
                        )
                        .child(
                            div()
                                .id("visual-view")
                                .focusable()
                                .tab_stop(true)
                                .role(Role::Button)
                                .aria_label(self.copy.visual_view.clone())
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.switch_view(EditorMode::Visual, cx)
                                }))
                                .on_key_down(cx.listener(
                                    |this, event: &gpui::KeyDownEvent, _, cx| {
                                        if event.keystroke.key == "enter"
                                            || event.keystroke.key == "space"
                                        {
                                            this.switch_view(EditorMode::Visual, cx);
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .child(self.copy.visual_view.clone()),
                        )
                        .child(
                            div()
                                .id("source-view")
                                .focusable()
                                .tab_stop(true)
                                .role(Role::Button)
                                .aria_label(self.copy.source_view.clone())
                                .cursor_pointer()
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.switch_view(EditorMode::Source, cx)
                                }))
                                .on_key_down(cx.listener(
                                    |this, event: &gpui::KeyDownEvent, _, cx| {
                                        if event.keystroke.key == "enter"
                                            || event.keystroke.key == "space"
                                        {
                                            this.switch_view(EditorMode::Source, cx);
                                            cx.stop_propagation();
                                        }
                                    },
                                ))
                                .child(self.copy.source_view.clone()),
                        ),
                )
            })
            .children(reader_element.map(|reader| div().flex_1().min_h_0().w_full().child(reader)))
            .when(!reading, |root| {
                root.child(
                    div().text_xl().child(
                        self.loaded_name
                            .clone()
                            .unwrap_or_else(|| self.copy.title.clone()),
                    ),
                )
                .child(div().child(if self.loaded_name.is_some() {
                    self.copy.readonly.clone()
                } else {
                    self.copy.empty.clone()
                }))
            })
            .child(
                div()
                    .id("open-document")
                    .focusable()
                    .tab_stop(true)
                    .role(Role::Button)
                    .aria_label(self.copy.open.clone())
                    .focus(|style| style.border_color(rgb(0x0969DA)))
                    .cursor_pointer()
                    .px_4()
                    .py_2()
                    .border_1()
                    .border_color(rgb(0xD1D9E0))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            this.open_document(&OpenDocument, window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_document(&OpenDocument, window, cx)
                    }))
                    .child(self.copy.open.clone()),
            )
            .children(self.loaded_name.as_ref().map(|_| {
                div()
                    .id("close-document")
                    .focusable()
                    .tab_stop(true)
                    .role(Role::Button)
                    .aria_label(self.copy.close.clone())
                    .focus(|style| style.border_color(rgb(0x0969DA)))
                    .cursor_pointer()
                    .px_4()
                    .py_2()
                    .border_1()
                    .border_color(rgb(0xD1D9E0))
                    .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                        if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                            this.close_document(&CloseDocument, window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.close_document(&CloseDocument, window, cx)
                    }))
                    .child(self.copy.close.clone())
            }))
            .children(
                self.reader
                    .as_ref()
                    .filter(|reader| reader.has_pending_restore())
                    .map(|_| {
                        div()
                            .id("cancel-navigation")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label(self.copy.cancel_navigation.clone())
                            .cursor_pointer()
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "enter" || event.keystroke.key == "space"
                                {
                                    if let Some(reader) = &this.reader {
                                        reader.cancel_restore();
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                }
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(reader) = &this.reader {
                                    reader.cancel_restore();
                                    cx.notify();
                                }
                            }))
                            .child(self.copy.cancel_navigation.clone())
                    }),
            )
            .children(phase.map(|message| div().child(message)))
            .children(
                self.state
                    .feedback_key()
                    .and_then(|key| self.copy.errors.get(key))
                    .cloned()
                    .map(|message| div().child(message)),
            )
            .children(
                self.locale_fallback
                    .clone()
                    .map(|message| div().child(message)),
            );
        #[cfg(debug_assertions)]
        let root = root.on_action(cx.listener(|this, _: &ReportLayout, _, _| {
            if let Some(reader) = &this.reader {
                reader.report();
            }
        }));
        root
    }
}
