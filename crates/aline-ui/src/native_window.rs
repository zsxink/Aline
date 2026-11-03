//! 原生单窗的打开协调与最小只读表面。
//!
//! 窗口持有一个只读会话、一个实际读取及至多一个最新候选；文件选择、任务与
//! 焦点在本层装配。读取和解析在后台完成，render 只读当前状态，不读盘或解析。

mod geometry;
mod readonly_hit;
mod readonly_layout;
mod readonly_reader;
mod shell;

/// 原生窗口同源协调器的无窗口回归；真实 chooser/菜单/焦点另作 macOS 实验。
#[cfg(test)]
mod tests;

use geometry::content_metrics;
use readonly_hit::{HitError, HitTarget, PreparedLayout};
use readonly_reader::ReaderSurface;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use aline_app_core::Translator;
use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use gpui::{
    App, Bounds, Context, FocusHandle, Image, KeyBinding, Menu, MenuItem, PathPromptOptions, Role,
    SharedString, Task, Window, WindowBounds, WindowOptions, div, prelude::*, px, rgb, rgba, size,
};

use crate::document::DocumentStatistics;
use crate::{
    DocLimits, DocumentId, EditorMode, OpenDisposition, OpenError, OpenRequestId, ReadonlyDocument,
    ReadonlyOpenCompletion, ReadonlyOpenJob, ReadonlyOpenLimits, ReadonlySession,
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
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

/// M1 实验预算沿用现有 16 MiB 解析输入上限；不是发布限额。
pub(super) fn open_limits() -> ReadonlyOpenLimits {
    ReadonlyOpenLimits::new(
        StorageBudget::new(MAX_DOCUMENT_BYTES),
        DocLimits::new(
            TextBudget::new(MAX_DOCUMENT_BYTES),
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

/// 取出只读文档冻结的源码文本，作为统计的不可变输入；不含窗口或可变核心。
fn statistics_source(document: &ReadonlyDocument) -> Option<String> {
    let snapshot = document.parsed().snapshot();
    let range = snapshot.src_range(0..snapshot.len_bytes()).ok()?;
    snapshot
        .slice(&range, TextBudget::new(MAX_DOCUMENT_BYTES))
        .ok()
        .map(|text| text.into_owned())
}

pub(super) struct NativeWindowCopy {
    translator: Translator,
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
    statistics_pending: SharedString,
    errors: BTreeMap<&'static str, SharedString>,
}

impl NativeWindowCopy {
    pub(super) fn new(translator: Translator) -> Result<Self, String> {
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
            statistics_pending: text("status.pending")?,
            errors,
            translator,
        })
    }

    /// 用插值模板生成状态栏统计标签；键与占位符在 catalog 构造时已校验。
    fn statistics_labels(
        &self,
        statistics: DocumentStatistics,
    ) -> Result<(SharedString, SharedString), String> {
        let characters = statistics.characters().to_string();
        let lines = statistics.lines().to_string();
        let text = |key, count: &str| {
            self.translator
                .t_with(key, &[("count", count)])
                .map(SharedString::from)
                .map_err(|error| error.to_string())
        };
        Ok((
            text("status.characters", &characters)?,
            text("status.lines", &lines)?,
        ))
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
    brand: Arc<Image>,
    dialog_task: Option<Task<()>>,
    dialog_return_focus: Option<FocusHandle>,
    read_task: Option<Task<()>>,
    loaded_name: Option<SharedString>,
    reader: Option<ReaderSurface>,
    next_reader_generation: u64,
    statistics: Option<(DocumentId, SharedString, SharedString)>,
    statistics_task: Option<(DocumentId, Task<()>)>,
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
            brand: shell::brand_icon(),
            dialog_task: None,
            dialog_return_focus: None,
            read_task: None,
            loaded_name: None,
            reader: None,
            next_reader_generation: 0,
            statistics: None,
            statistics_task: None,
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
        // 新候选开始读取即作废当前统计：旧文档的数值不得伴随候选读取继续显示。
        self.invalidate_statistics();
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
                match next {
                    Some(job) => this.start_read(job, window, cx),
                    None => this.refresh_statistics(window, cx),
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
        self.invalidate_statistics();
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

    /// 立即作废统计值与在途任务；关闭、替换或开始新读取时调用。
    fn invalidate_statistics(&mut self) {
        self.statistics = None;
        self.statistics_task = None;
    }

    /// 为当前文档在后台执行器调度一次统计；同一文档只算一次。
    ///
    /// 计算与文案插值都不在 render 路径：结果连同 `DocumentId` 回到窗口，
    /// 与当前文档身份一致才被采纳，文档已替换或关闭时直接丢弃。
    fn refresh_statistics(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(document) = self.state.session().document() else {
            self.invalidate_statistics();
            return;
        };
        let id = document.core().document_id().clone();
        if self
            .statistics
            .as_ref()
            .is_some_and(|(current, _, _)| current == &id)
            || self
                .statistics_task
                .as_ref()
                .is_some_and(|(current, _)| current == &id)
        {
            return;
        }
        // 先取源码再作废旧值：取文本只需要只读文档，作废会改写窗口状态。
        let source = statistics_source(document);
        // 文档已替换或旧任务属于旧文档：两者都作废，晚到结果还会按身份再拒绝。
        self.invalidate_statistics();
        let Some(source) = source else {
            self.state.feedback = Some("open_error.internal");
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { DocumentStatistics::compute(&source) });
        let expected = id.clone();
        let task = cx.spawn_in(window, async move |weak, cx| {
            let statistics = work.await;
            let _ = weak.update_in(cx, |this, _, cx| {
                let current = this
                    .state
                    .session()
                    .document()
                    .map(|document| document.core().document_id().clone());
                if current.as_ref() != Some(&expected) {
                    return;
                }
                match this.copy.statistics_labels(statistics) {
                    Ok((characters, lines)) => {
                        this.statistics = Some((expected.clone(), characters, lines));
                    }
                    Err(_) => this.state.feedback = Some("open_error.internal"),
                }
                this.statistics_task = None;
                cx.notify();
            });
        });
        self.statistics_task = Some((id, task));
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

    /// 工具栏编辑模式按钮：文字标签、可聚焦、当前模式用强调色弱背景表达。
    fn mode_button(
        &self,
        id: &'static str,
        label: SharedString,
        active: bool,
        mode: EditorMode,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(label.clone())
            .focus(|style| style.border_color(rgb(shell::ACCENT)))
            .cursor_pointer()
            .flex()
            .items_center()
            .h(px(30.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(rgba(0x00000000))
            .text_size(px(13.0))
            .when(active, |button| {
                button
                    .bg(rgba(shell::ACCENT_SOFT))
                    .text_color(rgb(shell::ACCENT))
            })
            .when(!active, |button| {
                button
                    .text_color(rgb(shell::TEXT))
                    .hover(|style| style.bg(rgba(shell::HOVER)))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.switch_view(mode, cx)))
            .on_key_down(cx.listener(move |this, event: &gpui::KeyDownEvent, _, cx| {
                if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                    this.switch_view(mode, cx);
                    cx.stop_propagation();
                }
            }))
            .child(label)
            .into_any_element()
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
        let viewport_width = f32::from(window.viewport_size().width);
        let metrics = content_metrics(viewport_width, viewport_width);
        if let (Some(reader), Some(document)) = (&mut self.reader, self.state.session().document())
            && reader.prepare(document, window, metrics).is_err()
        {
            self.state.feedback = Some("open_error.budget");
        }
        let document_open = self.state.session().document().is_some();
        let mode = self
            .state
            .session()
            .document()
            .map(|document| document.core().mode());
        let entity = cx.entity().downgrade();
        let reader_element = self.reader.as_ref().map(|reader| {
            reader.render(move |row, _, cx| {
                entity
                    .update(cx, |this, cx| this.render_reading_row(row, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
        });
        let mut notices: Vec<SharedString> = Vec::new();
        if let Some(phase) = phase {
            notices.push(phase);
        }
        if let Some(message) = self
            .state
            .feedback_key()
            .and_then(|key| self.copy.errors.get(key))
        {
            notices.push(message.clone());
        }
        if let Some(fallback) = &self.locale_fallback {
            notices.push(fallback.clone());
        }
        let cancel_pending = self
            .reader
            .as_ref()
            .is_some_and(|reader| reader.has_pending_restore());
        let notice = (!notices.is_empty() || cancel_pending).then(|| {
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_start()
                .gap_1()
                .px_3()
                .py_2()
                .bg(rgb(shell::NOTICE_SURFACE))
                .border_b_1()
                .border_color(rgb(shell::BORDER))
                .text_xs()
                .text_color(rgb(shell::SECONDARY))
                .children(notices)
                .when(cancel_pending, |banner| {
                    banner.child(
                        div()
                            .id("cancel-navigation")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label(self.copy.cancel_navigation.clone())
                            .focus(|style| style.border_color(rgb(shell::ACCENT)))
                            .cursor_pointer()
                            .px_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(shell::BORDER))
                            .hover(|style| style.bg(rgba(shell::HOVER)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(reader) = &this.reader {
                                    reader.cancel_restore();
                                    cx.notify();
                                }
                            }))
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
                            .child(self.copy.cancel_navigation.clone()),
                    )
                })
        });
        let open_button = div()
            .id("open-document")
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(self.copy.open.clone())
            .focus(|style| style.border_color(rgb(shell::ACCENT)))
            .cursor_pointer()
            .px_4()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(shell::BORDER))
            .text_size(px(14.0))
            .hover(|style| style.bg(rgba(shell::HOVER)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                    this.open_document(&OpenDocument, window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_click(
                cx.listener(|this, _, window, cx| this.open_document(&OpenDocument, window, cx)),
            )
            .child(self.copy.open.clone());
        let content_body = match reader_element {
            Some(reader) => div().flex_1().min_h_0().w_full().child(reader),
            None => div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .gap_3()
                .when(!document_open, |body| {
                    body.child(
                        div()
                            .text_xl()
                            .text_color(rgb(shell::TEXT))
                            .child(self.copy.empty.clone()),
                    )
                    .child(open_button)
                }),
        };
        let toolbar = shell::toolbar_row()
            .child(shell::brand(self.brand.clone(), self.copy.title.clone()))
            .children(mode.map(|mode| {
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.mode_button(
                        "mode-visual",
                        self.copy.visual_view.clone(),
                        mode == EditorMode::Visual,
                        EditorMode::Visual,
                        cx,
                    ))
                    .child(self.mode_button(
                        "mode-source",
                        self.copy.source_view.clone(),
                        mode == EditorMode::Source,
                        EditorMode::Source,
                        cx,
                    ))
            }))
            .when(document_open, |row| {
                row.child(
                    div()
                        .pl_2()
                        .text_size(px(12.0))
                        .text_color(rgb(shell::SECONDARY))
                        .child(self.copy.readonly.clone()),
                )
            });
        let current_id = self
            .state
            .session()
            .document()
            .map(|document| document.core().document_id());
        let adopted = self
            .statistics
            .as_ref()
            .filter(|(id, _, _)| current_id.is_some_and(|current| current == id));
        let status = shell::status_row()
            .when(document_open, |row| match adopted {
                Some((_, characters, lines)) => row
                    .child(div().child(characters.clone()))
                    .child(div().child(lines.clone())),
                None => row.child(div().child(self.copy.statistics_pending.clone())),
            })
            .child(div().flex_1())
            .children(mode.map(|mode| {
                div().child(match mode {
                    EditorMode::Visual => self.copy.visual_view.clone(),
                    EditorMode::Source => self.copy.source_view.clone(),
                })
            }));
        let content = shell::content_row().child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .children(notice)
                .child(content_body),
        );
        let root = div()
            .size_full()
            .bg(rgb(shell::SURFACE))
            .text_color(rgb(shell::TEXT))
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
            .child(toolbar)
            .child(content)
            .child(status);
        #[cfg(debug_assertions)]
        let root = root.on_action(cx.listener(|this, _: &ReportLayout, _, _| {
            if let Some(reader) = &this.reader {
                reader.report();
            }
        }));
        root
    }
}
