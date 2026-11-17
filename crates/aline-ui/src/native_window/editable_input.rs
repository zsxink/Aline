//! 正文输入适配持有局部只读投影和布局。输入文本、组合、选区和历史仍归 DocCore。

use std::{cell::RefCell, ops::Range, rc::Rc};

use aline_buffer::{BufferError, SrcRange, SrcSelection};
use aline_markdown::{MappingBudget, ProjectionId};
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, FocusHandle, InputHandler, MouseButton,
    Pixels, Point, StyledText, UTF16Selection, WeakEntity, Window, canvas, div, fill, point,
    prelude::*, px, rgb, size,
};

use crate::document::native_input_coordinates::{BoundInputCoordinates, NativeInputCoordinates};
use crate::{CurrentSnapshot, DocError, EditorMode, InputIdentity, ReadonlyDocument};

use super::{NativeDocumentWindow, PreparedLayout, edit_error_key, tokens};

const MAX_INPUT_BYTES: usize = 64 * 1024;
const MAPPING: MappingBudget = MappingBudget::new(MAX_INPUT_BYTES, MAX_INPUT_BYTES + 1);
const LINE_HEIGHT: f32 = 25.0;

/// Source 的虚拟行以该帧的绝对源码范围为入口，只接受范围内的同一局部输入。
#[derive(Clone)]
pub(super) struct InputSite {
    mode: EditorMode,
    source: Range<usize>,
    document_len: usize,
    reader_row: SrcRange,
}

impl InputSite {
    pub(super) fn for_row(
        document: &mut ReadonlyDocument,
        row: SrcRange,
        active: Option<&SourceInputSite>,
    ) -> Option<Self> {
        if !document.is_editable() {
            return None;
        }
        let mode = document.core().mode();
        let current = document.current_snapshot().ok()?;
        let bytes = row.byte_range();
        let source = if mode == EditorMode::Visual {
            let range = document
                .core()
                .visual_block_range(&current)
                .ok()??
                .byte_range();
            (range.start == bytes.start).then_some(range)?
        } else if row.snapshot_id() == current.snapshot_id() {
            let head = document.core().current_selection().head();
            let snapshot = current.text();
            let ends_at_separator = || {
                snapshot
                    .src_range(head..head.checked_add(1)?)
                    .ok()
                    .and_then(|range| snapshot.slice(&range, document.text_budget()).ok())
                    .is_some_and(|separator| separator == "\n" || separator == "\r")
                    .then_some(())
            };
            if !bytes.contains(&head)
                && (head != bytes.end
                    || (head != current.len_bytes() && ends_at_separator().is_none()))
            {
                return None;
            }
            bytes
        } else {
            // 渲染旧列表时只沿用经同一输入回调更新的局部绑定。
            let active = active?;
            let identity = document.input_identity().ok()?;
            active.identity.validate(&identity).ok()?;
            return (active.site.reader_row == row).then(|| active.site.clone());
        };
        Some(Self {
            mode,
            source,
            document_len: current.len_bytes(),
            reader_row: row,
        })
    }

    fn rebound(&self, document: &mut ReadonlyDocument) -> Option<Self> {
        if !document.is_editable() || document.core().mode() != self.mode {
            return None;
        }
        let current = document.current_snapshot().ok()?;
        let source = match self.mode {
            EditorMode::Visual => document
                .core()
                .visual_block_range(&current)
                .ok()??
                .byte_range(),
            EditorMode::Source => {
                resized(self.source.clone(), self.document_len, current.len_bytes())?
            }
        };
        (self.source.start == source.start).then_some(Self {
            mode: self.mode,
            source,
            document_len: current.len_bytes(),
            reader_row: self.reader_row.clone(),
        })
    }
}

pub(super) struct SourceInputSite {
    identity: InputIdentity,
    site: InputSite,
}

fn resized(range: Range<usize>, before: usize, after: usize) -> Option<Range<usize>> {
    let end = if after >= before {
        range.end.checked_add(after.checked_sub(before)?)?
    } else {
        range.end.checked_sub(before.checked_sub(after)?)?
    };
    (range.start <= end).then_some(range.start..end)
}

struct InputView {
    site: InputSite,
    identity: InputIdentity,
    coordinates: NativeInputCoordinates,
}

impl InputView {
    fn new(document: &mut ReadonlyDocument, site: InputSite) -> Option<Self> {
        let (identity, current) = Self::current(document)?;
        Self::from_snapshot(site, identity, &current)
    }

    fn from_snapshot(
        site: InputSite,
        identity: InputIdentity,
        current: &CurrentSnapshot,
    ) -> Option<Self> {
        let range = current.text().src_range(site.source.clone()).ok()?;
        let coordinates =
            NativeInputCoordinates::new(identity.clone(), current.text(), &range, MAPPING).ok()?;
        let view = Self {
            site,
            identity,
            coordinates,
        };
        view.bound(&view.identity)?
            .utf16_from_source_selection(current.selection())
            .ok()?;
        Some(view)
    }

    fn current(document: &mut ReadonlyDocument) -> Option<(InputIdentity, CurrentSnapshot)> {
        if !document.is_editable() {
            return None;
        }
        let identity = document.input_identity().ok()?;
        let current = document.current_snapshot().ok()?;
        Some((identity, current))
    }

    fn bound(&self, identity: &InputIdentity) -> Option<BoundInputCoordinates<'_>> {
        self.coordinates
            .bind(identity, self.coordinates.projection_id())
            .ok()
    }

    fn active(
        &self,
        identity: &InputIdentity,
        current: &CurrentSnapshot,
        mode: EditorMode,
    ) -> Option<BoundInputCoordinates<'_>> {
        if current.snapshot_id() != identity.snapshot_id() || mode != self.site.mode {
            return None;
        }
        let bound = self.bound(identity)?;
        // 选区移动不推进工作版本。跨行或跨块后，旧投影仍须停止消费回调。
        bound
            .utf16_from_source_selection(current.selection())
            .ok()?;
        Some(bound)
    }

    fn marked(document: &ReadonlyDocument, current: &CurrentSnapshot) -> Option<SrcSelection> {
        let mark = document.core().composition()?;
        let start = mark.range().start;
        let end = start.checked_add(mark.candidate().len())?;
        SrcSelection::new(
            current.text().src_offset(start).ok()?,
            current.text().src_offset(end).ok()?,
        )
        .ok()
    }

    fn replacement(
        &self,
        current: &CurrentSnapshot,
        document: &ReadonlyDocument,
        range: Option<Range<usize>>,
        replacement_bytes: usize,
    ) -> Result<(), DocError> {
        let bound = self.bound(&self.identity).ok_or(DocError::StaleResult)?;
        let marked = Self::marked(document, current);
        let target = marked.as_ref().unwrap_or(current.selection());
        let target_units = bound
            .utf16_from_source_selection(target)
            .map_err(|_| DocError::OutsideOpenBlock)?;
        if let Some(range) = range
            && bound
                .utf16_range(range)
                .map_err(|_| DocError::OutsideOpenBlock)?
                != target_units
        {
            return Err(DocError::OutsideOpenBlock);
        }
        if marked.is_none() {
            bound
                .editable_selection(&target_units)
                .ok()
                .flatten()
                .ok_or(DocError::OutsideOpenBlock)?;
        }
        let candidate_bytes = bound
            .drawn_text()
            .len()
            .checked_sub(target.range().byte_range().len())
            .and_then(|remaining| remaining.checked_add(replacement_bytes))
            .ok_or(DocError::Buffer(BufferError::LengthOverflow))?;
        if candidate_bytes > MAX_INPUT_BYTES {
            return Err(DocError::Buffer(BufferError::BudgetExceeded {
                needed: candidate_bytes,
                limit: MAX_INPUT_BYTES,
            }));
        }
        Ok(())
    }
}

enum InputIntent<'a> {
    LineBreak,
    Delete { forward: bool },
    Move { forward: bool, extend: bool },
    Replace(Option<Range<usize>>, &'a str),
    Mark(Option<Range<usize>>, &'a str, Option<Range<usize>>),
    Select(Range<usize>),
    Paste(&'a str),
}

impl InputIntent<'_> {
    fn apply(
        self,
        view: &InputView,
        current: &CurrentSnapshot,
        document: &mut ReadonlyDocument,
        now_millis: u64,
    ) -> Result<(), DocError> {
        let identity = &view.identity;
        let mode = document.core().mode();
        if view
            .active(&document.input_identity()?, current, mode)
            .is_none()
        {
            return Err(DocError::StaleResult);
        }
        let composing = document.core().composition().is_some();
        if view.site.mode == EditorMode::Source
            && let Some(draft) = document.core().open_block_draft()
        {
            let source_start = document
                .core()
                .open_block_range()
                .ok_or(DocError::NoOpenBlock)?
                .start;
            let source_end = source_start
                .checked_add(draft.len())
                .ok_or(DocError::OutsideOpenBlock)?;
            if source_start < view.site.source.start || source_end > view.site.source.end {
                return Err(DocError::OutsideOpenBlock);
            }
        }
        match self {
            Self::LineBreak => {
                if composing {
                    return Err(DocError::CompositionActive);
                }
                view.replacement(current, document, None, 1)?;
                document
                    .edit_core()?
                    .type_selection(identity, "\n", now_millis)
            }
            Self::Delete { forward } => {
                if composing {
                    return Err(DocError::CompositionActive);
                }
                let selection = current.selection();
                let bound = view.bound(identity).ok_or(DocError::StaleResult)?;
                let units = bound
                    .utf16_from_source_selection(selection)
                    .map_err(|_| DocError::OutsideOpenBlock)?;
                let checked = bound
                    .editable_selection(&units)
                    .ok()
                    .flatten()
                    .ok_or(DocError::OutsideOpenBlock)?;
                let range = if checked.range().is_empty() {
                    let Some(adjacent) = bound.adjacent_grapheme(selection.head(), forward) else {
                        return Ok(());
                    };
                    SrcSelection::new(selection.head().clone(), adjacent)?.range()
                } else {
                    checked.range()
                };
                document
                    .edit_core()?
                    .delete_native_range(identity, &range, now_millis)
                    .map(|_| ())
            }
            Self::Move { forward, extend } => {
                if composing {
                    return Err(DocError::CompositionActive);
                }
                let selection = current.selection();
                let bound = view.bound(identity).ok_or(DocError::StaleResult)?;
                let head = if !extend && !selection.range().is_empty() {
                    if forward == selection.is_reversed() {
                        selection.anchor().clone()
                    } else {
                        selection.head().clone()
                    }
                } else {
                    let Some(head) = bound.adjacent_grapheme(selection.head(), forward) else {
                        return Ok(());
                    };
                    head
                };
                let anchor = if extend { selection.anchor() } else { &head };
                let selected = SrcSelection::new(anchor.clone(), head)?;
                document.edit_core()?.select(identity, &selected)
            }
            Self::Select(range) => {
                let bound = view.bound(identity).ok_or(DocError::StaleResult)?;
                let range = bound
                    .utf16_range(range)
                    .map_err(|_| DocError::OutsideOpenBlock)?;
                let selection = bound
                    .editable_selection(&range)
                    .ok()
                    .flatten()
                    .ok_or(DocError::OutsideOpenBlock)?;
                document.edit_core()?.select(identity, &selection)
            }
            Self::Paste(text) => {
                view.replacement(current, document, None, text.len())?;
                document
                    .edit_core()?
                    .replace_selection(identity, text, now_millis)
                    .map(|_| ())
            }
            Self::Replace(range, text) => {
                view.replacement(current, document, range, text.len())?;
                let core = document.edit_core()?;
                if composing {
                    core.commit_native_candidate(identity, text, now_millis)
                        .map(|_| ())
                } else if text.is_empty() {
                    core.replace_selection(identity, text, now_millis)
                        .map(|_| ())
                } else {
                    core.type_selection(identity, text, now_millis)
                }
            }
            Self::Mark(range, text, selection) => {
                view.replacement(current, document, range, text.len())?;
                if text.is_empty() {
                    if selection.is_some_and(|range| range != (0..0)) {
                        return Err(DocError::OutsideOpenBlock);
                    }
                    if composing {
                        return document.edit_core()?.composition_cancel(now_millis);
                    }
                    return Ok(());
                }
                if text.len() > MAX_INPUT_BYTES {
                    return Err(DocError::Buffer(BufferError::BudgetExceeded {
                        needed: text.len(),
                        limit: MAX_INPUT_BYTES,
                    }));
                }
                let selected = selection
                    .map(|range| {
                        BoundInputCoordinates::candidate_selection(text, range, MAX_INPUT_BYTES)
                            .ok_or(DocError::OutsideOpenBlock)
                    })
                    .transpose()?;
                document
                    .edit_core()?
                    .update_native_candidate(identity, text, selected, now_millis)
            }
        }
    }
}

struct InputGeometry {
    projection: ProjectionId,
    bounds: Bounds<Pixels>,
    layout: PreparedLayout,
    viewport: gpui::Size<Pixels>,
    scale: u32,
}

#[derive(Clone)]
pub(super) struct InputFrame(Rc<RefCell<Option<InputView>>>);

struct DocumentInput {
    host: WeakEntity<NativeDocumentWindow>,
    focus: FocusHandle,
    view: InputFrame,
    geometry: Option<InputGeometry>,
}

impl DocumentInput {
    fn with_current<T>(
        &self,
        window: &Window,
        cx: &mut App,
        query: impl FnOnce(
            &InputView,
            &BoundInputCoordinates<'_>,
            &CurrentSnapshot,
            &ReadonlyDocument,
        ) -> Option<T>,
    ) -> Option<T> {
        if !self.focus.is_focused(window) {
            return None;
        }
        let active = self.view.0.try_borrow().ok()?;
        let view = active.as_ref()?;
        self.host
            .update(cx, |host, _| {
                let document = host.state.session.document_mut()?;
                let (identity, current) = InputView::current(document)?;
                let bound = view.active(&identity, &current, document.core().mode())?;
                query(view, &bound, &current, document)
            })
            .ok()
            .flatten()
    }

    fn apply(&mut self, intent: InputIntent<'_>, window: &Window, cx: &mut App) {
        if self.focus.is_focused(window) {
            let _ = self.host.update(cx, |host, cx| {
                host.apply_native_input(&self.view, intent, window, cx);
            });
        }
    }
}

impl NativeDocumentWindow {
    pub(super) fn document_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if modifiers.alt
            || modifiers.platform
            || modifiers.control
            || modifiers.function
            || !self.focus.is_focused(window)
        {
            return;
        }
        let intent = match event.keystroke.key.as_str() {
            "enter" => InputIntent::LineBreak,
            "backspace" => InputIntent::Delete { forward: false },
            "delete" => InputIntent::Delete { forward: true },
            "left" => InputIntent::Move {
                forward: false,
                extend: modifiers.shift,
            },
            "right" => InputIntent::Move {
                forward: true,
                extend: modifiers.shift,
            },
            _ => return,
        };
        if let Some(frame) = self.input_frame.clone()
            && self.apply_native_input(&frame, intent, window, cx)
        {
            cx.stop_propagation();
        }
    }

    fn apply_native_input(
        &mut self,
        frame: &InputFrame,
        intent: InputIntent<'_>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Ok(mut active) = frame.0.try_borrow_mut() else {
            return false;
        };
        let Some(view) = active.as_ref() else {
            return false;
        };
        let Some(document) = self.state.session.document_mut() else {
            return false;
        };
        let Some((identity, current)) = InputView::current(document) else {
            return false;
        };
        if view
            .active(&identity, &current, document.core().mode())
            .is_none()
        {
            return false;
        }
        let millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match intent.apply(view, &current, document, millis) {
            Ok(()) => {
                let changed = current.version() != document.core().version();
                let rebound = changed.then(|| view.site.rebound(document)).flatten();
                let has_draft = document.core().open_block_draft().is_some();
                self.state.feedback = None;
                if changed {
                    self.refresh_find(cx);
                    // 待定组可跨原行展开。输入仍用原虚拟行的绑定，封口再采纳新列表。
                    if !has_draft {
                        self.refresh_after_edit(window, cx);
                    }
                    *active = self
                        .state
                        .session
                        .document_mut()
                        .and_then(|doc| rebound.and_then(|site| InputView::new(doc, site)));
                    self.source_input_site = active
                        .as_ref()
                        .filter(|view| view.site.mode == EditorMode::Source)
                        .map(|view| SourceInputSite {
                            identity: view.identity.clone(),
                            site: view.site.clone(),
                        });
                }
            }
            Err(error) => self.state.feedback = Some(edit_error_key(error)),
        }
        self.composing_input_frame = self
            .state
            .session
            .document()
            .is_some_and(|doc| doc.core().composition().is_some())
            .then(|| frame.clone());
        cx.notify();
        true
    }

    pub(super) fn escape_document_input(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.focus.is_focused(window) {
            return false;
        }
        let Some(document) = self
            .state
            .session
            .document_mut()
            .filter(|doc| doc.is_editable() && doc.core().has_open_block())
        else {
            return false;
        };
        let millis = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        match document.edit_core().and_then(|core| core.escape(millis)) {
            Ok(()) => {
                let refresh = !document.is_parse_current();
                self.input_frame = None;
                self.source_input_site = None;
                self.composing_input_frame = None;
                self.state.feedback = None;
                if refresh {
                    self.refresh_find(cx);
                    self.refresh_after_edit(window, cx);
                }
            }
            Err(error) => self.state.feedback = Some(edit_error_key(error)),
        }
        cx.notify();
        true
    }

    pub(super) fn move_window_focus(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.focus.is_focused(window)
            && self
                .state
                .session
                .document()
                .is_some_and(|doc| doc.core().composition().is_some())
        {
            self.state.feedback = Some("document.composition_pending");
            cx.notify();
            return;
        }
        if forward {
            window.focus_next(cx);
        } else {
            window.focus_prev(cx);
        }
    }

    pub(super) fn offscreen_input(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let frame = self.composing_input_frame.clone()?;
        let focus = self.focus.clone();
        let host = cx.entity().downgrade();
        Some(
            canvas(
                |_, _, _| (),
                move |_, (), window, cx| {
                    if focus.is_focused(window) {
                        window.handle_input(
                            &focus,
                            DocumentInput {
                                host,
                                focus: focus.clone(),
                                view: frame,
                                geometry: None,
                            },
                            cx,
                        );
                    }
                },
            )
            .absolute()
            .size_0()
            .into_any_element(),
        )
    }
}

impl InputHandler for DocumentInput {
    fn selected_text_range(
        &mut self,
        _: bool,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.with_current(window, cx, |_, bound, current, _| {
            Some(UTF16Selection {
                range: bound
                    .utf16_from_source_selection(current.selection())
                    .ok()?
                    .code_unit_range(),
                reversed: current.selection().is_reversed(),
            })
        })
    }

    fn marked_text_range(&mut self, window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.with_current(window, cx, |_, bound, current, doc| {
            let selected = InputView::marked(doc, current)?;
            Some(
                bound
                    .utf16_from_source_selection(&selected)
                    .ok()?
                    .code_unit_range(),
            )
        })
    }

    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        self.with_current(window, cx, |_, bound, current, doc| {
            let checked = bound.utf16_range(range.clone()).ok()?;
            let source = bound.source_from_utf16(&checked).ok()?;
            let text = current
                .text()
                .slice(&source, doc.text_budget())
                .ok()?
                .into_owned();
            *adjusted = Some(range);
            Some(text)
        })
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.apply(InputIntent::Replace(range, text), window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.apply(InputIntent::Mark(range, text, selection), window, cx);
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut App) {
        // 当前 macOS 探针未观察到独立 unmark 的自然结果。保持组合等待平台明确替换。
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut App) {
        if let Some(text) = item.text() {
            self.apply(InputIntent::Paste(&text), window, cx);
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        self.with_current(window, cx, |view, bound, _, _| {
            let geometry = self.geometry.as_ref()?;
            if view.coordinates.projection_id() != &geometry.projection
                || window.viewport_size() != geometry.viewport
                || window.scale_factor().to_bits() != geometry.scale
            {
                return None;
            }
            let drawn = bound
                .drawn_from_utf16(&bound.utf16_range(range).ok()?)
                .ok()?;
            let bytes = drawn.byte_range();
            let start = geometry.layout.position_for_index(bytes.start).ok()??;
            let end = geometry.layout.position_for_index(bytes.end).ok()??;
            let width = if start.y == end.y {
                (end.x - start.x).abs().max(px(1.0))
            } else {
                px(1.0)
            };
            let x = if start.y == end.y {
                start.x.min(end.x)
            } else {
                start.x
            };
            Some(Bounds::new(point(x, start.y), size(width, px(LINE_HEIGHT))))
        })
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.with_current(window, cx, |view, bound, _, _| {
            let geometry = self.geometry.as_ref()?;
            if view.coordinates.projection_id() != &geometry.projection
                || window.viewport_size() != geometry.viewport
                || window.scale_factor().to_bits() != geometry.scale
            {
                return None;
            }
            geometry.bounds.localize(&point)?;
            let byte = geometry.layout.index_for_position(point).ok()?.ok()?;
            Some(
                bound
                    .utf16_from_drawn(&bound.drawn_offset(byte).ok()?)
                    .ok()?
                    .code_units(),
            )
        })
    }

    fn set_selected_text_range(&mut self, range: Range<usize>, window: &mut Window, cx: &mut App) {
        self.apply(InputIntent::Select(range), window, cx);
    }

    fn text_length_utf16(&mut self, window: &mut Window, cx: &mut App) -> Option<usize> {
        self.with_current(window, cx, |_, bound, _, _| {
            Some(
                bound
                    .utf16_from_drawn(&bound.drawn_offset(bound.drawn_text().len()).ok()?)
                    .ok()?
                    .code_units(),
            )
        })
    }

    fn accepts_text_input(&mut self, window: &mut Window, cx: &mut App) -> bool {
        self.with_current(window, cx, |_, _, _, _| Some(()))
            .is_some()
    }
}

pub(super) fn render(
    document: &mut ReadonlyDocument,
    site: InputSite,
    focus: FocusHandle,
    cx: &Context<NativeDocumentWindow>,
) -> Option<(AnyElement, InputFrame)> {
    let (_, current) = InputView::current(document)?;
    let view = InputView::new(document, site)?;
    let bound = view.bound(&view.identity)?;
    let styled = StyledText::new(gpui::SharedString::from(bound.drawn_text().to_owned()));
    let selected = bound
        .utf16_from_source_selection(current.selection())
        .ok()?;
    let selected = bound.drawn_from_utf16(&selected).ok()?.byte_range();
    let marked = InputView::marked(document, &current).and_then(|selected| {
        let selected = bound.utf16_from_source_selection(&selected).ok()?;
        Some(bound.drawn_from_utf16(&selected).ok()?.byte_range())
    });
    let caret = if current.selection().is_reversed() {
        selected.start
    } else {
        selected.end
    };
    let layout = PreparedLayout::new(styled.layout().clone());
    let prepainted = layout.clone();
    let host = cx.entity().downgrade();
    let click_host = host.clone();
    let click_layout = layout.clone();
    let click_focus = focus.clone();
    let click_identity = view.identity.clone();
    let click_coordinates = view.coordinates.clone();
    let projection = view.coordinates.projection_id().clone();
    let frame = InputFrame(Rc::new(RefCell::new(Some(view))));
    let handler_frame = frame.clone();
    Some((
        div()
            .relative()
            .w_full()
            .min_h(px(LINE_HEIGHT))
            .line_height(px(LINE_HEIGHT))
            .child(styled)
            .on_children_prepainted(move |_, _, _| prepainted.mark_prepainted())
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let Some(byte) = click_layout
                    .index_for_position(event.position)
                    .ok()
                    .and_then(Result::ok)
                else {
                    return;
                };
                let applied = click_host.update(cx, |host, cx| {
                    let document = host.state.session.document_mut()?;
                    let (now, current) = InputView::current(document)?;
                    click_identity.validate(&now).ok()?;
                    let bound = click_coordinates
                        .bind(&now, click_coordinates.projection_id())
                        .ok()?;
                    bound
                        .utf16_from_source_selection(current.selection())
                        .ok()?;
                    let utf16 = bound
                        .utf16_from_drawn(&bound.drawn_offset(byte).ok()?)
                        .ok()?
                        .code_units();
                    let range = bound.utf16_range(utf16..utf16).ok()?;
                    let selection = bound.editable_selection(&range).ok()??;
                    document.edit_core().ok()?.select(&now, &selection).ok()?;
                    cx.notify();
                    Some(())
                });
                if let Ok(Some(())) = applied {
                    click_focus.focus(window, cx);
                }
            })
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, (), window, cx| {
                        if !focus.is_focused(window) {
                            return;
                        }
                        for (range, color) in marked
                            .into_iter()
                            .map(|range| (range, tokens::ACCENT))
                            .chain(
                                (selected.start != selected.end)
                                    .then_some((selected, tokens::BORDER)),
                            )
                        {
                            let start = layout.position_for_index(range.start).ok().flatten();
                            let end = layout.position_for_index(range.end).ok().flatten();
                            if let (Some(start), Some(end)) = (start, end)
                                && start.y == end.y
                            {
                                let width = (end.x - start.x).abs();
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(start.x.min(end.x), start.y + px(LINE_HEIGHT - 2.0)),
                                        size(width, px(2.0)),
                                    ),
                                    rgb(color),
                                ));
                            }
                        }
                        if let Ok(Some(caret)) = layout.position_for_index(caret) {
                            window.paint_quad(fill(
                                Bounds::new(caret, size(px(1.5), px(LINE_HEIGHT))),
                                rgb(tokens::ACCENT),
                            ));
                        }
                        let handler = DocumentInput {
                            host,
                            focus: focus.clone(),
                            view: handler_frame,
                            geometry: Some(InputGeometry {
                                projection,
                                bounds,
                                layout,
                                viewport: window.viewport_size(),
                                scale: window.scale_factor().to_bits(),
                            }),
                        };
                        window.handle_input(&focus, handler, cx);
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full(),
            )
            .into_any_element(),
        frame,
    ))
}

#[cfg(test)]
mod tests;
