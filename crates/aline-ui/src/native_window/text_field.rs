//! 短文本控件：拥有输入法组合、选区和焦点，只向宿主发送字段变化及按键意图。
//!
//! 查找面板与链接/图片弹窗共用同一实现；宽度由调用方给定，Enter/Shift+Enter
//! 只表达提交意图，具体语义（查找导航或弹窗确认）由宿主决定。

use std::ops::Range;

use gpui::{
    Bounds, Context, ElementInputHandler, EntityInputHandler, EventEmitter, FocusHandle,
    KeyDownEvent, MouseButton, MouseDownEvent, Pixels, Point, Role, ShapedLine, SharedString,
    TextAlign, TextRun, UTF16Selection, Window, canvas, div, fill, point, prelude::*, px, rgb,
    size,
};

use super::{Copy, Cut, Paste, PastePlain, Redo, SelectAll, Undo, tokens};

/// 单个字段的字节上限；预填与输入共用，避免弹窗塞入超长选区。
pub(super) const MAX_FIELD_BYTES: usize = 4096;
const FIELD_HEIGHT: Pixels = px(23.0);

/// 字段向宿主发送的事件。
#[derive(Clone, Copy)]
pub(crate) enum TextFieldEvent {
    /// 已提交文字发生变化（组合中文字不计）。
    Changed,
    /// 输入法组合开始或结束。
    Composition,
    /// Enter/Shift+Enter。
    Submit {
        /// Enter 时是否按住 Shift。
        shift: bool,
    },
    /// Escape。
    Cancel,
}

struct FieldComposition {
    before: SharedString,
    before_selection: Range<usize>,
    marked: Range<usize>,
}

struct FieldGeometry {
    bounds: Bounds<Pixels>,
    line: ShapedLine,
    origin: Point<Pixels>,
}

pub(crate) struct TextField {
    id: &'static str,
    placeholder: SharedString,
    width: Pixels,
    pub(crate) focus: FocusHandle,
    pub(crate) text: SharedString,
    selection: Range<usize>,
    composition: Option<FieldComposition>,
    geometry: Option<FieldGeometry>,
}

impl EventEmitter<TextFieldEvent> for TextField {}

impl TextField {
    pub(crate) fn new(
        id: &'static str,
        placeholder: SharedString,
        width: Pixels,
        initial: SharedString,
        cx: &mut Context<Self>,
    ) -> Self {
        let selection = 0..initial.len();
        Self {
            id,
            placeholder,
            width,
            focus: cx.focus_handle(),
            text: initial,
            selection,
            composition: None,
            geometry: None,
        }
    }

    /// 已提交文字；输入法组合中返回组合前文字。
    pub(crate) fn committed(&self) -> SharedString {
        self.composition
            .as_ref()
            .map_or_else(|| self.text.clone(), |mark| mark.before.clone())
    }

    pub(crate) fn is_composing(&self) -> bool {
        self.composition.is_some()
    }

    fn replacement_range(&self, requested: Option<Range<usize>>) -> Option<Range<usize>> {
        match requested {
            Some(utf16) => bytes_for_utf16(self.text.as_ref(), utf16),
            None => Some(
                self.composition
                    .as_ref()
                    .map_or_else(|| self.selection.clone(), |mark| mark.marked.clone()),
            ),
        }
    }

    fn replace(&mut self, bytes: Range<usize>, inserted: &str) -> Option<Range<usize>> {
        let removed = self.text.get(bytes.clone())?.len();
        let len = self
            .text
            .len()
            .checked_sub(removed)?
            .checked_add(inserted.len())?;
        if len > MAX_FIELD_BYTES || inserted.contains(['\r', '\n', '\0']) {
            return None;
        }
        let inserted_range = bytes.start..bytes.start.checked_add(inserted.len())?;
        let mut text = self.text.to_string();
        text.replace_range(bytes, inserted);
        self.text = text.into();
        self.geometry = None;
        Some(inserted_range)
    }

    fn publish(&mut self, before: &SharedString, cx: &mut Context<Self>) {
        if before != &self.text {
            cx.emit(TextFieldEvent::Changed);
        }
        cx.notify();
    }

    fn select_all(&mut self, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.composition.is_none() {
            self.selection = 0..self.text.len();
            cx.notify();
        }
    }

    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.composition.is_none()
            && let Some(text) = cx.read_from_clipboard().and_then(|item| item.text())
        {
            self.replace_text_in_range(None, &text, window, cx);
        }
    }

    fn copy_or_cut(&mut self, cut: bool, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.composition.is_none()
            && let Some(text) = self
                .text
                .get(self.selection.clone())
                .filter(|value| !value.is_empty())
        {
            cx.write_to_clipboard(gpui::ClipboardItem::new_string(text.to_owned()));
            if cut {
                self.replace_text_in_range(None, "", window, cx);
            }
        }
    }

    fn key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if !self.focus.is_focused(window) || self.composition.is_some() {
            return;
        }
        let key = &event.keystroke;
        if key.modifiers.platform && key.key == "a" {
            self.select_all(cx);
        } else if key.modifiers.platform && key.key == "v" {
            self.paste(window, cx);
        } else if key.modifiers.platform && matches!(key.key.as_str(), "x" | "c") {
            self.copy_or_cut(key.key == "x", window, cx);
        } else if key.modifiers.platform || key.modifiers.control || key.modifiers.alt {
            return;
        } else {
            match key.key.as_str() {
                "enter" => cx.emit(TextFieldEvent::Submit {
                    shift: key.modifiers.shift,
                }),
                "escape" => cx.emit(TextFieldEvent::Cancel),
                "left" | "right" => self.move_caret(key.key == "left"),
                "backspace" | "delete" => {
                    if self.selection.is_empty() {
                        let caret = self.selection.start;
                        self.selection = if key.key == "backspace" {
                            self.previous(caret)..caret
                        } else {
                            caret..self.next(caret)
                        };
                    }
                    self.replace_text_in_range(None, "", window, cx);
                }
                _ => return,
            }
        }
        cx.stop_propagation();
        cx.notify();
    }

    fn previous(&self, byte: usize) -> usize {
        self.text
            .get(..byte)
            .and_then(|prefix| prefix.char_indices().next_back())
            .map_or(byte, |(start, _)| start)
    }

    fn next(&self, byte: usize) -> usize {
        self.text
            .get(byte..)
            .and_then(|suffix| suffix.chars().next())
            .map_or(byte, |next| byte + next.len_utf8())
    }

    fn move_caret(&mut self, left: bool) {
        let caret = match (left, self.selection.is_empty()) {
            (true, true) => self.previous(self.selection.start),
            (false, true) => self.next(self.selection.end),
            (true, false) => self.selection.start,
            (false, false) => self.selection.end,
        };
        self.selection = caret..caret;
    }

    fn click(&mut self, event: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.composition.is_none()
            && let Some(geometry) = &self.geometry
        {
            let byte = geometry
                .line
                .closest_index_for_x(event.position.x - geometry.origin.x);
            if self.text.is_char_boundary(byte) {
                self.selection = byte..byte;
            }
        }
        self.focus.focus(window, cx);
        cx.notify();
    }
}

impl Render for TextField {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity();
        let focused = self.focus.is_focused(window);
        div()
            .id(self.id)
            .key_context("M2Editor")
            .role(Role::TextInput)
            .aria_label(self.placeholder.clone())
            .aria_value(self.text.clone())
            .track_focus(&self.focus)
            .tab_stop(true)
            .w(self.width)
            .max_w_full()
            .px_2()
            .border_1()
            .rounded_md()
            .border_color(rgb(if focused {
                tokens::ACCENT
            } else {
                tokens::BORDER
            }))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::click))
            .on_key_down(cx.listener(Self::key_down))
            .on_action(cx.listener(|field, _: &SelectAll, _, cx| field.select_all(cx)))
            .on_action(
                cx.listener(|field, _: &Copy, window, cx| field.copy_or_cut(false, window, cx)),
            )
            .on_action(
                cx.listener(|field, _: &Cut, window, cx| field.copy_or_cut(true, window, cx)),
            )
            .on_action(cx.listener(|field, _: &Paste, window, cx| field.paste(window, cx)))
            .on_action(cx.listener(|field, _: &PastePlain, window, cx| field.paste(window, cx)))
            .on_action(cx.listener(|_, _: &Undo, _, cx| cx.stop_propagation()))
            .on_action(cx.listener(|_, _: &Redo, _, cx| cx.stop_propagation()))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, cx| {
                        let field = entity.read(cx);
                        if field.focus.is_focused(window) {
                            window.handle_input(
                                &field.focus,
                                ElementInputHandler::new(bounds, entity.clone()),
                                cx,
                            );
                        }
                        let font = window.text_style().font();
                        let line = window.text_system().shape_line(
                            field.text.clone(),
                            px(14.0),
                            &[TextRun {
                                len: field.text.len(),
                                font: font.clone(),
                                color: rgb(tokens::TEXT).into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );
                        let caret = line.x_for_index(field.selection.end);
                        let shift = (caret - (bounds.size.width - px(2.0))).max(px(0.0));
                        let origin = point(bounds.origin.x - shift, bounds.origin.y);
                        let selection = field
                            .focus
                            .is_focused(window)
                            .then(|| field.selection.clone());
                        let marked = field.composition.as_ref().map(|mark| mark.marked.clone());
                        let placeholder = field.text.is_empty().then(|| field.placeholder.clone());
                        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
                            if let Some(placeholder) = placeholder {
                                let line = window.text_system().shape_line(
                                    placeholder.clone(),
                                    px(14.0),
                                    &[TextRun {
                                        len: placeholder.len(),
                                        font,
                                        color: rgb(tokens::SECONDARY).into(),
                                        background_color: None,
                                        underline: None,
                                        strikethrough: None,
                                    }],
                                    None,
                                );
                                let _ = line.paint(
                                    origin,
                                    FIELD_HEIGHT,
                                    TextAlign::Left,
                                    None,
                                    window,
                                    cx,
                                );
                            }
                            if let Some(selection) = selection {
                                window.paint_quad(fill(
                                    field_bounds(origin, &line, &selection),
                                    if selection.is_empty() {
                                        rgb(tokens::ACCENT)
                                    } else {
                                        gpui::rgba(tokens::ACCENT_SOFT)
                                    },
                                ));
                            }
                            let _ =
                                line.paint(origin, FIELD_HEIGHT, TextAlign::Left, None, window, cx);
                            if let Some(marked) = marked {
                                let marked = field_bounds(origin, &line, &marked);
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(marked.left(), marked.bottom() - px(2.0)),
                                        size(marked.size.width, px(1.0)),
                                    ),
                                    rgb(tokens::ACCENT),
                                ));
                            }
                        });
                        entity.update(cx, |field, _| {
                            field.geometry = Some(FieldGeometry {
                                bounds,
                                line,
                                origin,
                            })
                        });
                    },
                )
                .w_full()
                .h(FIELD_HEIGHT),
            )
    }
}

impl EntityInputHandler for TextField {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        adjusted: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let bytes = bytes_for_utf16(self.text.as_ref(), range.clone())?;
        let text = self.text.get(bytes)?.to_owned();
        *adjusted = Some(range);
        Some(text)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: utf16_for_bytes(self.text.as_ref(), &self.selection)?,
            reversed: false,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        utf16_for_bytes(self.text.as_ref(), &self.composition.as_ref()?.marked)
    }

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(mark) = self.composition.take() {
            cx.emit(TextFieldEvent::Composition);
            self.publish(&mark.before, cx);
        }
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) {
            return;
        }
        let before = self.committed();
        if let Some(range) = self.replacement_range(range)
            && self
                .composition
                .as_ref()
                .is_none_or(|mark| mark.marked == range)
            && let Some(inserted) = self.replace(range, text)
        {
            self.selection = inserted.end..inserted.end;
            if self.composition.take().is_some() {
                cx.emit(TextFieldEvent::Composition);
            }
            self.publish(&before, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        text: &str,
        selection: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus.is_focused(window) {
            return;
        }
        let Some(range) = self.replacement_range(range) else {
            return;
        };
        if self
            .composition
            .as_ref()
            .is_some_and(|mark| mark.marked != range)
        {
            return;
        }
        let selection = match selection {
            Some(selection) => bytes_for_utf16(text, selection),
            None => Some(text.len()..text.len()),
        };
        let Some(selection) = selection else { return };
        if text.is_empty() {
            if let Some(mark) = self
                .composition
                .as_ref()
                .filter(|mark| mark.marked == range)
            {
                self.text = mark.before.clone();
                self.selection = mark.before_selection.clone();
                self.composition = None;
                self.geometry = None;
                cx.emit(TextFieldEvent::Composition);
                cx.notify();
            }
            return;
        }
        let before = self.committed();
        let before_selection = self.composition.as_ref().map_or_else(
            || self.selection.clone(),
            |mark| mark.before_selection.clone(),
        );
        let starting = self.composition.is_none();
        if let Some(marked) = self.replace(range, text) {
            self.selection = marked.start + selection.start..marked.start + selection.end;
            self.composition = Some(FieldComposition {
                before,
                before_selection,
                marked,
            });
            if starting {
                cx.emit(TextFieldEvent::Composition);
            }
            cx.notify();
        }
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let geometry = self
            .geometry
            .as_ref()
            .filter(|geometry| geometry.bounds == bounds)?;
        let bytes = bytes_for_utf16(self.text.as_ref(), range)?;
        Some(field_bounds(geometry.origin, &geometry.line, &bytes))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let geometry = self.geometry.as_ref()?;
        geometry.bounds.localize(&point)?;
        let byte = geometry
            .line
            .closest_index_for_x(point.x - geometry.origin.x);
        Some(self.text.get(..byte)?.encode_utf16().count())
    }
}

fn field_bounds(origin: Point<Pixels>, line: &ShapedLine, bytes: &Range<usize>) -> Bounds<Pixels> {
    let start = line.x_for_index(bytes.start);
    let end = line.x_for_index(bytes.end);
    Bounds::new(
        point(origin.x + start.min(end), origin.y),
        size((end - start).abs().max(px(1.0)), FIELD_HEIGHT),
    )
}

// 偏移只属于字段私有文本。GPUI 传入的半个代理对不能用于字符串切片。
fn bytes_for_utf16(text: &str, range: Range<usize>) -> Option<Range<usize>> {
    if range.start > range.end {
        return None;
    }
    let boundary = |target| {
        let mut units = 0;
        for (byte, scalar) in text.char_indices() {
            if units == target {
                return Some(byte);
            }
            units += scalar.len_utf16();
            if units > target {
                return None;
            }
        }
        (units == target).then_some(text.len())
    };
    Some(boundary(range.start)?..boundary(range.end)?)
}

fn utf16_for_bytes(text: &str, range: &Range<usize>) -> Option<Range<usize>> {
    Some(
        text.get(..range.start)?.encode_utf16().count()
            ..text.get(..range.end)?.encode_utf16().count(),
    )
}
