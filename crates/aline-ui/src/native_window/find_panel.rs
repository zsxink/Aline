//! 持有查找条件、版本绑定的命中和面板输入焦点。后台只读取冻结源码。
//! 面板交付定位和替换意图，窗口负责当前文档的模式、选区和受检提交。

use aline_app_core::Translator;
use aline_buffer::{SnapshotId, SrcRange, TextBudget};
use aline_edit::{EditRefusal, FindBudget, FindCase, FindResults, FindVersion, find_matches};
use gpui::{
    App, Context, Entity, EventEmitter, FocusHandle, KeyDownEvent, Role, SharedString,
    Subscription, Task, Window, div, prelude::*, rgb, rgba,
};
use std::{rc::Rc, sync::Arc};

mod host;
use super::text_field::{TextField, TextFieldEvent};
#[cfg(test)]
pub(super) use host::{replace_all_for_test, reveal_in_source};

use super::tokens;
use crate::{CurrentSnapshot, DocumentId, WorkingVersion};

const MAX_MATCHES: usize = 65_536;
type SearchResults = FindResults<DocumentId, WorkingVersion>;
type SearchVersion = FindVersion<DocumentId, WorkingVersion>;

#[derive(Clone, Copy, Debug)]
pub(super) enum FindDirection {
    Next,
    Previous,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum FindScope {
    Current,
    All,
}

#[derive(Clone, Copy)]
pub(super) enum FindEvent {
    Refresh,
    Navigate(FindDirection),
    Replace(FindScope),
    Close,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct SearchKey {
    version: SearchVersion,
    snapshot: SnapshotId,
    query: SharedString,
    case: FindCase,
}

impl SearchKey {
    fn new(
        snapshot: &CurrentSnapshot,
        generation: u64,
        query: SharedString,
        case: FindCase,
    ) -> Self {
        Self {
            version: FindVersion::new(
                snapshot.document_id().clone(),
                snapshot.version(),
                generation,
            ),
            snapshot: snapshot.snapshot_id().clone(),
            query,
            case,
        }
    }
}

pub(super) struct SearchJob {
    key: SearchKey,
    snapshot: CurrentSnapshot,
}

impl SearchJob {
    pub(super) fn run(self) -> (SearchKey, Result<SearchResults, EditRefusal>) {
        let budget = FindBudget::new(TextBudget::new(super::MAX_DOCUMENT_BYTES), MAX_MATCHES);
        let result = find_matches(
            self.snapshot.text(),
            self.key.version.clone(),
            self.key.query.as_ref(),
            self.key.case,
            budget,
        );
        (self.key, result)
    }
}

pub(super) struct FindTarget {
    key: SearchKey,
    ordinal: usize,
    range: SrcRange,
}

impl FindTarget {
    pub(super) fn range(&self) -> &SrcRange {
        &self.range
    }
}

#[derive(Default)]
pub(super) struct SearchState {
    generation: u64,
    expected: Option<SearchKey>,
    results: Option<Arc<SearchResults>>,
    current: Option<usize>,
    resume_target: Option<FindTarget>,
    failed: bool,
    exhausted: bool,
}

impl SearchState {
    pub(super) fn submit(
        &mut self,
        snapshot: Option<CurrentSnapshot>,
        query: SharedString,
        case: FindCase,
    ) -> Option<SearchJob> {
        let snapshot = snapshot.filter(|_| !self.exhausted && !query.is_empty());
        let key = snapshot
            .as_ref()
            .map(|snapshot| SearchKey::new(snapshot, self.generation, query, case));
        if self.expected == key {
            return None;
        }
        self.discard();
        self.expected = key.clone();
        let (key, snapshot) = key.zip(snapshot)?;
        Some(SearchJob { key, snapshot })
    }

    fn discard(&mut self) {
        self.expected = None;
        self.results = None;
        self.current = None;
        self.failed = false;
    }

    pub(super) fn query_changed(&mut self, preserve_target: bool) {
        if !preserve_target {
            self.resume_target = None;
        } else if let (Some(ordinal), Some(key), Some(results)) =
            (self.current, &self.expected, &self.results)
            && let Some(hit) = results.matches().get(ordinal)
        {
            self.resume_target = Some(FindTarget {
                key: key.clone(),
                ordinal,
                range: hit.range().clone(),
            });
        }
        self.discard();
        if let Some(next) = self.generation.checked_add(1) {
            self.generation = next;
        } else {
            self.exhausted = true;
        }
    }

    pub(super) fn complete(
        &mut self,
        key: &SearchKey,
        result: Result<SearchResults, EditRefusal>,
    ) -> bool {
        if self.expected.as_ref() != Some(key) {
            return false;
        }
        match result {
            Ok(results) => {
                if let Some(target) = self.resume_target.take()
                    && target.key.snapshot == key.snapshot
                    && target.key.query == key.query
                    && target.key.case == key.case
                    && results
                        .matches()
                        .get(target.ordinal)
                        .is_some_and(|hit| hit.range() == &target.range)
                {
                    self.current = Some(target.ordinal);
                }
                self.results = Some(Arc::new(results));
            }
            Err(_) => self.failed = true,
        }
        true
    }

    fn validated<'a>(
        &'a self,
        snapshot: &CurrentSnapshot,
        query: SharedString,
        case: FindCase,
    ) -> Result<(&'a Arc<SearchResults>, SearchKey), EditRefusal> {
        let key = SearchKey::new(snapshot, self.generation, query, case);
        if self.expected.as_ref() != Some(&key) {
            return Err(EditRefusal::StaleFindResults);
        }
        let results = self.results.as_ref().ok_or(EditRefusal::StaleFindResults)?;
        results.validate(snapshot.text(), &key.version)?;
        Ok((results, key))
    }

    pub(super) fn target(
        &self,
        snapshot: &CurrentSnapshot,
        query: SharedString,
        case: FindCase,
        direction: FindDirection,
    ) -> Result<FindTarget, EditRefusal> {
        let (results, key) = self.validated(snapshot, query, case)?;
        let last = results
            .matches()
            .len()
            .checked_sub(1)
            .ok_or(EditRefusal::FindMatchUnavailable)?;
        let ordinal = match (direction, self.current) {
            (FindDirection::Next, Some(current)) if current < last => current + 1,
            (FindDirection::Next, _) => 0,
            (FindDirection::Previous, Some(current)) if current > 0 => current - 1,
            (FindDirection::Previous, _) => last,
        };
        let hit = results
            .matches()
            .get(ordinal)
            .ok_or(EditRefusal::FindMatchUnavailable)?;
        Ok(FindTarget {
            key,
            ordinal,
            range: hit.range().clone(),
        })
    }

    pub(super) fn accept_target(&mut self, target: &FindTarget) -> bool {
        if self.expected.as_ref() != Some(&target.key)
            || self
                .results
                .as_ref()
                .and_then(|results| results.matches().get(target.ordinal))
                .is_none_or(|hit| hit.range() != &target.range)
        {
            return false;
        }
        self.current = Some(target.ordinal);
        true
    }
}

struct FindCopy {
    query: SharedString,
    replacement: SharedString,
    previous: SharedString,
    next: SharedString,
    case_sensitive: SharedString,
    replace_current: SharedString,
    replace_all: SharedString,
    close: SharedString,
    empty: SharedString,
    no_matches: SharedString,
    no_document: SharedString,
    readonly: SharedString,
    pending: SharedString,
    failed: SharedString,
}

impl FindCopy {
    fn new(translator: &Translator) -> Result<Self, String> {
        translator
            .t_with("find.count", &[("current", "0"), ("total", "0")])
            .map_err(|error| error.to_string())?;
        let text = |key| {
            translator
                .t(key)
                .map(SharedString::from)
                .map_err(|error| error.to_string())
        };
        Ok(Self {
            query: text("find.query")?,
            replacement: text("find.replacement")?,
            previous: text("find.previous")?,
            next: text("find.next")?,
            case_sensitive: text("find.case_sensitive")?,
            replace_current: text("find.replace_current")?,
            replace_all: text("find.replace_all")?,
            close: text("find.close")?,
            empty: text("find.empty")?,
            no_matches: text("find.no_matches")?,
            no_document: text("find.no_document")?,
            readonly: text("find.readonly")?,
            pending: text("find.pending")?,
            failed: text("find.failed")?,
        })
    }
}

pub(super) struct FindPanel {
    translator: Rc<Translator>,
    copy: FindCopy,
    query: Entity<TextField>,
    replacement: Entity<TextField>,
    focus: FocusHandle,
    case: FindCase,
    search: SearchState,
    has_document: bool,
    editable: bool,
    running: Option<Task<()>>,
    queued: Option<SearchJob>,
    _subscriptions: [Subscription; 2],
}

impl EventEmitter<FindEvent> for FindPanel {}

impl FindPanel {
    fn new(translator: Rc<Translator>, copy: FindCopy, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| {
            TextField::new(
                "find-query",
                copy.query.clone(),
                gpui::px(228.0),
                SharedString::default(),
                cx,
            )
        });
        let replacement = cx.new(|cx| {
            TextField::new(
                "find-replacement",
                copy.replacement.clone(),
                gpui::px(228.0),
                SharedString::default(),
                cx,
            )
        });
        let query_events = cx.subscribe(&query, |panel, _, &event, cx| match event {
            TextFieldEvent::Changed => panel.query_changed(false, cx),
            TextFieldEvent::Composition => cx.notify(),
            TextFieldEvent::Submit { shift } => cx.emit(FindEvent::Navigate(if shift {
                FindDirection::Previous
            } else {
                FindDirection::Next
            })),
            TextFieldEvent::Cancel => cx.emit(FindEvent::Close),
        });
        let replacement_events = cx.subscribe(&replacement, |panel, _, &event, cx| match event {
            TextFieldEvent::Changed => panel.query_changed(true, cx),
            TextFieldEvent::Composition => cx.notify(),
            TextFieldEvent::Submit { shift } => cx.emit(FindEvent::Navigate(if shift {
                FindDirection::Previous
            } else {
                FindDirection::Next
            })),
            TextFieldEvent::Cancel => cx.emit(FindEvent::Close),
        });
        Self {
            translator,
            copy,
            query,
            replacement,
            focus: cx.focus_handle(),
            case: FindCase::Insensitive,
            search: SearchState::default(),
            has_document: false,
            editable: false,
            running: None,
            queued: None,
            _subscriptions: [query_events, replacement_events],
        }
    }

    pub(super) fn focus(&self, window: &mut Window, cx: &mut App) {
        let focus = self.query.read(cx).focus.clone();
        focus.focus(window, cx);
    }

    fn query_changed(&mut self, preserve_target: bool, cx: &mut Context<Self>) {
        self.search.query_changed(preserve_target);
        self.queued = None;
        cx.emit(FindEvent::Refresh);
        cx.notify();
    }

    pub(super) fn refresh(
        &mut self,
        snapshot: Option<CurrentSnapshot>,
        editable: bool,
        cx: &mut Context<Self>,
    ) {
        self.has_document = snapshot.is_some();
        self.editable = editable && self.has_document;
        let query = self.query.read(cx).committed();
        let previous = self.search.expected.clone();
        let job = self.search.submit(snapshot, query, self.case);
        if self.search.expected != previous {
            self.queued = job;
        }
        self.start_next(cx);
        cx.notify();
    }

    fn start_next(&mut self, cx: &mut Context<Self>) {
        if self.running.is_some() {
            return;
        }
        let Some(job) = self.queued.take() else {
            return;
        };
        let background = cx.background_executor().spawn(async move { job.run() });
        self.running = Some(cx.spawn(async move |panel, cx| {
            let (key, result) = background.await;
            let _ = panel.update(cx, |panel, cx| {
                panel.running = None;
                if panel.search.complete(&key, result) {
                    cx.notify();
                }
                panel.start_next(cx);
            });
        }));
    }

    pub(super) fn query_generation(&self) -> u64 {
        self.search.generation
    }

    pub(super) fn target(
        &self,
        snapshot: &CurrentSnapshot,
        direction: FindDirection,
        cx: &App,
    ) -> Result<FindTarget, EditRefusal> {
        if self.is_composing(cx) {
            return Err(EditRefusal::CompositionActive);
        }
        self.search.target(
            snapshot,
            self.query.read(cx).committed(),
            self.case,
            direction,
        )
    }

    pub(super) fn accept_target(&mut self, target: &FindTarget, cx: &mut Context<Self>) {
        if self.search.accept_target(target) {
            cx.notify();
        }
    }

    fn is_composing(&self, cx: &App) -> bool {
        self.query.read(cx).is_composing() || self.replacement.read(cx).is_composing()
    }

    pub(super) fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        self.focus.contains_focused(window, cx)
    }

    fn status(&self, cx: &App) -> SharedString {
        if !self.has_document {
            return self.copy.no_document.clone();
        }
        if self.search.failed || self.search.exhausted {
            return self.copy.failed.clone();
        }
        if self.query.read(cx).committed().is_empty() {
            return self.copy.empty.clone();
        }
        let Some(results) = self.search.results.as_ref() else {
            return self.copy.pending.clone();
        };
        if results.matches().is_empty() {
            return self.copy.no_matches.clone();
        }
        let current = self
            .search
            .current
            .map_or(0, |ordinal| ordinal + 1)
            .to_string();
        let total = results.matches().len().to_string();
        self.translator
            .t_with("find.count", &[("current", &current), ("total", &total)])
            .map(SharedString::from)
            .unwrap_or_else(|_| self.copy.failed.clone())
    }
}

impl Render for FindPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let has_results = !self.is_composing(cx)
            && self
                .search
                .results
                .as_ref()
                .is_some_and(|results| !results.matches().is_empty());
        let can_replace = has_results && self.editable;
        let sensitive = self.case == FindCase::Sensitive;
        div()
            .id("find-panel")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .gap_1()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(rgb(tokens::BORDER))
            .bg(rgb(tokens::SURFACE))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.query.clone())
                    .child(
                        div()
                            .text_sm()
                            .text_color(rgb(tokens::SECONDARY))
                            .child(self.status(cx)),
                    )
                    .child(button(
                        "find-previous",
                        self.copy.previous.clone(),
                        has_results,
                        false,
                        |_, cx| cx.emit(FindEvent::Navigate(FindDirection::Previous)),
                        cx,
                    ))
                    .child(button(
                        "find-next",
                        self.copy.next.clone(),
                        has_results,
                        false,
                        |_, cx| {
                            cx.emit(FindEvent::Navigate(FindDirection::Next));
                        },
                        cx,
                    ))
                    .child(button(
                        "find-case",
                        self.copy.case_sensitive.clone(),
                        true,
                        sensitive,
                        |panel, cx| {
                            panel.case = if panel.case == FindCase::Sensitive {
                                FindCase::Insensitive
                            } else {
                                FindCase::Sensitive
                            };
                            panel.query_changed(false, cx);
                        },
                        cx,
                    ))
                    .child(button(
                        "find-close",
                        self.copy.close.clone(),
                        true,
                        false,
                        |_, cx| {
                            cx.emit(FindEvent::Close);
                        },
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(self.replacement.clone())
                    .child(button(
                        "find-replace-current",
                        self.copy.replace_current.clone(),
                        can_replace && self.search.current.is_some(),
                        false,
                        |_, cx| {
                            cx.emit(FindEvent::Replace(FindScope::Current));
                        },
                        cx,
                    ))
                    .child(button(
                        "find-replace-all",
                        self.copy.replace_all.clone(),
                        can_replace,
                        false,
                        |_, cx| {
                            cx.emit(FindEvent::Replace(FindScope::All));
                        },
                        cx,
                    ))
                    .when(self.has_document && !self.editable, |row| {
                        row.child(
                            div()
                                .text_sm()
                                .text_color(rgb(tokens::SECONDARY))
                                .child(self.copy.readonly.clone()),
                        )
                    }),
            )
    }
}

fn button(
    id: &'static str,
    label: SharedString,
    enabled: bool,
    selected: bool,
    callback: impl Fn(&mut FindPanel, &mut Context<FindPanel>) + Clone + 'static,
    cx: &Context<FindPanel>,
) -> gpui::Stateful<gpui::Div> {
    let keyboard = callback.clone();
    div()
        .id(id)
        .role(Role::Button)
        .rounded_md()
        .px_2()
        .py_1()
        .text_sm()
        .child(label)
        .when(selected, |button| button.bg(rgba(tokens::ACCENT_SOFT)))
        .when(!enabled, |button| button.text_color(rgb(tokens::SECONDARY)))
        .when(enabled, |button| {
            button
                .focusable()
                .tab_stop(true)
                .cursor_pointer()
                .hover(|style| style.bg(rgba(tokens::HOVER)))
                .on_click(cx.listener(move |panel, _, _, cx| callback(panel, cx)))
                .on_key_down(cx.listener(move |panel, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        cx.stop_propagation();
                        keyboard(panel, cx);
                    }
                }))
        })
}
