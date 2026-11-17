//! W5 的 macOS 平台探针。窗口只持有一次性测试文本，stderr 只记录回调、偏移和窗口坐标。
//! 此入口不打开用户文件，不消费产品写入口，不判断系统候选的确认或取消。

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    macos::run()?;

    #[cfg(not(target_os = "macos"))]
    {
        let copy = aline_app_core::Translator::embedded(aline_app_core::Locale::ZhCn)?;
        eprintln!("{}", copy.t("app.platform_unavailable")?);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
mod macos {
    use std::{cell::Cell, fmt, ops::Range, rc::Rc, time::Instant};

    use aline_app_core::{Locale, Translator};
    use gpui::{
        Application, Bounds, Context, ElementInputHandler, EntityInputHandler, FocusHandle, Font,
        Hsla, KeyBinding, Menu, MouseButton, MouseDownEvent, Pixels, Point, QuitMode, ShapedLine,
        SharedString, TextAlign, TextRun, UTF16Selection, Window, WindowBounds, WindowOptions,
        actions, canvas, div, fill, point, prelude::*, px, rgb, size,
    };

    actions!(ime_spike, [SelectAll, DeleteBackward]);

    const MAX_BYTES: usize = 1024;
    const FONT_SIZE: Pixels = px(22.);
    const LINE_HEIGHT: Pixels = px(34.);

    pub(super) fn run() -> Result<(), Box<dyn std::error::Error>> {
        let copy = Translator::embedded(Locale::ZhCn)?;
        let title = format!("{} · W5", copy.t("app.name")?);
        let window_failed = copy.t("startup.window_failed")?;
        let app = Application::with_platform(Rc::new(gpui_macos::MacPlatform::new(false)));
        app.with_quit_mode(QuitMode::LastWindowClosed)
            .run(move |cx| {
                cx.bind_keys([
                    KeyBinding::new("cmd-a", SelectAll, Some("ImeSpike")),
                    KeyBinding::new("backspace", DeleteBackward, Some("ImeSpike")),
                ]);
                cx.set_menus([Menu::new(title.clone())]);
                let bounds = Bounds::centered(None, size(px(540.), px(220.)), cx);
                let result = cx.open_window(
                    WindowOptions {
                        window_bounds: Some(WindowBounds::Windowed(bounds)),
                        ..WindowOptions::default()
                    },
                    |window, cx| {
                        window.set_window_title(&title);
                        cx.new(|cx| Probe::new(window, cx))
                    },
                );
                if result.is_err() {
                    eprintln!("{window_failed}");
                    cx.quit();
                } else {
                    cx.activate(true);
                }
            });
        Ok(())
    }

    struct Probe {
        input_focus: FocusHandle,
        other_focus: FocusHandle,
        text: String,
        selection: Range<usize>,
        marked: Option<Range<usize>>,
        geometry: Option<LineGeometry>,
        started: Instant,
        sequence: Cell<u64>,
    }

    impl Probe {
        fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
            let text = String::from("甲😀乙");
            let end = text.len();
            let input_focus = cx.focus_handle();
            let other_focus = cx.focus_handle();
            cx.on_focus(&input_focus, window, |probe, _, cx| {
                probe.record(
                    "input-focus",
                    format_args!("marked={:?}", probe.marked_utf16()),
                );
                cx.notify();
            })
            .detach();
            cx.on_blur(&input_focus, window, |probe, _, cx| {
                probe.record(
                    "input-blur",
                    format_args!("marked={:?}", probe.marked_utf16()),
                );
                cx.notify();
            })
            .detach();
            cx.observe_window_activation(window, |probe, window, _| {
                probe.record(
                    "window-activation",
                    format_args!(
                        "active={} marked={:?}",
                        window.is_window_active(),
                        probe.marked_utf16()
                    ),
                );
            })
            .detach();
            input_focus.focus(window, cx);
            let probe = Self {
                input_focus,
                other_focus,
                text,
                selection: end..end,
                marked: None,
                geometry: None,
                started: Instant::now(),
                sequence: Cell::new(0),
            };
            probe.record(
                "ready",
                format_args!("gpui=0.3.9 scale={}", window.scale_factor()),
            );
            probe
        }

        fn record(&self, event: &str, detail: fmt::Arguments<'_>) {
            let sequence = self.sequence.get().saturating_add(1);
            self.sequence.set(sequence);
            eprintln!(
                "w5 {sequence} {}us {event} {detail}",
                self.started.elapsed().as_micros()
            );
        }

        fn marked_utf16(&self) -> Option<Range<usize>> {
            self.marked
                .as_ref()
                .and_then(|range| to_utf16(&self.text, range))
        }

        fn replacement(&self, requested: Option<Range<usize>>) -> Option<Range<usize>> {
            match requested {
                Some(range) => from_utf16(&self.text, &range),
                None => Some(
                    self.marked
                        .clone()
                        .unwrap_or_else(|| self.selection.clone()),
                ),
            }
        }

        fn replace(&mut self, range: Range<usize>, text: &str) -> Option<Range<usize>> {
            let removed = self.text.get(range.clone())?.len();
            let len = self
                .text
                .len()
                .checked_sub(removed)?
                .checked_add(text.len())?;
            if len > MAX_BYTES || text.contains(['\n', '\r']) {
                return None;
            }
            let inserted = range.start..range.start.checked_add(text.len())?;
            self.text.replace_range(range, text);
            Some(inserted)
        }

        fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
            if self.marked.is_none() {
                self.selection = 0..self.text.len();
                self.record(
                    "probe-select-all",
                    format_args!("range={:?}", to_utf16(&self.text, &self.selection)),
                );
                cx.notify();
            }
        }

        fn delete_backward(&mut self, _: &DeleteBackward, _: &mut Window, cx: &mut Context<Self>) {
            if self.marked.is_some() {
                return;
            }
            let mut range = self.selection.clone();
            if range.is_empty() {
                let Some((previous, _)) = self
                    .text
                    .get(..range.start)
                    .and_then(|prefix| prefix.char_indices().next_back())
                else {
                    return;
                };
                range.start = previous;
            }
            self.record(
                "probe-delete-backward",
                format_args!("range={:?}", to_utf16(&self.text, &range)),
            );
            if let Some(caret) = self.replace(range, "") {
                self.selection = caret;
                cx.notify();
            }
        }

        fn click_input(
            &mut self,
            event: &MouseDownEvent,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) {
            self.input_focus.focus(window, cx);
            if self.marked.is_none()
                && let Some(geometry) = &self.geometry
            {
                let line = geometry.line(&self.text, window);
                let byte_offset =
                    line.closest_index_for_x(event.position.x - geometry.bounds.left());
                if self.text.is_char_boundary(byte_offset) {
                    self.selection = byte_offset..byte_offset;
                }
            }
            cx.notify();
        }
    }

    impl EntityInputHandler for Probe {
        fn text_for_range(
            &mut self,
            range: Range<usize>,
            adjusted_range: &mut Option<Range<usize>>,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<String> {
            self.record("text_for_range", format_args!("range={range:?}"));
            let text = self.text.get(from_utf16(&self.text, &range)?)?;
            *adjusted_range = Some(range);
            Some(text.to_owned())
        }

        fn selected_text_range(
            &mut self,
            _: bool,
            _: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<UTF16Selection> {
            let range = to_utf16(&self.text, &self.selection)?;
            self.record("selected_text_range", format_args!("range={range:?}"));
            Some(UTF16Selection {
                range,
                reversed: false,
            })
        }

        fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
            let range = self.marked_utf16();
            self.record("marked_text_range", format_args!("range={range:?}"));
            range
        }

        fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
            self.record(
                "unmark_text",
                format_args!("before={:?}", self.marked_utf16()),
            );
            self.marked = None;
            cx.notify();
        }

        fn replace_text_in_range(
            &mut self,
            range: Option<Range<usize>>,
            text: &str,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) {
            self.record(
                "replace_text_in_range",
                format_args!(
                    "range={range:?} units={} marked_before={:?}",
                    text.encode_utf16().count(),
                    self.marked_utf16()
                ),
            );
            let Some(inserted) = self
                .replacement(range)
                .and_then(|range| self.replace(range, text))
            else {
                self.record("probe-rejected-range-or-content", format_args!(""));
                return;
            };
            self.marked = None;
            self.selection = inserted.end..inserted.end;
            cx.notify();
        }

        fn replace_and_mark_text_in_range(
            &mut self,
            range: Option<Range<usize>>,
            text: &str,
            selected: Option<Range<usize>>,
            _: &mut Window,
            cx: &mut Context<Self>,
        ) {
            self.record(
                "replace_and_mark_text_in_range",
                format_args!(
                    "range={range:?} selected={selected:?} units={} marked_before={:?}",
                    text.encode_utf16().count(),
                    self.marked_utf16()
                ),
            );
            // AppKit 的新选区在本次传入的 marked 文本内，使用相对 UTF-16 单位。
            let selection = match selected {
                Some(selected) => from_utf16(text, &selected),
                None => Some(text.len()..text.len()),
            };
            let Some((range, selection)) = self.replacement(range).zip(selection) else {
                self.record("probe-rejected-range-or-content", format_args!(""));
                return;
            };
            let Some(inserted) = self.replace(range, text) else {
                self.record("probe-rejected-range-or-content", format_args!(""));
                return;
            };
            self.selection = inserted.start + selection.start..inserted.start + selection.end;
            self.marked = (!inserted.is_empty()).then_some(inserted);
            cx.notify();
        }

        fn bounds_for_range(
            &mut self,
            range: Range<usize>,
            element_bounds: Bounds<Pixels>,
            window: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<Bounds<Pixels>> {
            let bounds = (|| {
                let geometry = self.geometry.as_ref()?;
                if geometry.bounds != element_bounds {
                    return None;
                }
                let bytes = from_utf16(&self.text, &range)?;
                let line = geometry.line(&self.text, window);
                Some(line_bounds(element_bounds, &line, &bytes))
            })();
            self.record(
                "bounds_for_range",
                format_args!(
                    "range={range:?} window_points={bounds:?} scale={}",
                    window.scale_factor()
                ),
            );
            bounds
        }

        fn character_index_for_point(
            &mut self,
            point: Point<Pixels>,
            window: &mut Window,
            _: &mut Context<Self>,
        ) -> Option<usize> {
            let offset = (|| {
                let geometry = self.geometry.as_ref()?;
                let local = geometry.bounds.localize(&point)?;
                let line = geometry.line(&self.text, window);
                let byte_offset = line.closest_index_for_x(local.x);
                Some(self.text.get(..byte_offset)?.encode_utf16().count())
            })();
            self.record(
                "character_index_for_point",
                format_args!("point={point:?} utf16={offset:?}"),
            );
            offset
        }
    }

    impl Render for Probe {
        fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            let entity = cx.entity();
            let input_border = if self.input_focus.is_focused(window) {
                0x4E72A3
            } else {
                0xB9BEC5
            };
            let target_border = if self.other_focus.is_focused(window) {
                0x4E72A3
            } else {
                0xB9BEC5
            };
            div()
                .p(px(24.))
                .bg(rgb(0xF4F5F7))
                .size_full()
                .flex()
                .flex_col()
                .gap(px(16.))
                .child(
                    div()
                        .id("w5-input")
                        .key_context("ImeSpike")
                        .track_focus(&self.input_focus)
                        .on_action(cx.listener(Self::select_all))
                        .on_action(cx.listener(Self::delete_backward))
                        .on_mouse_down(MouseButton::Left, cx.listener(Self::click_input))
                        .p(px(12.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(input_border))
                        .overflow_hidden()
                        .child(
                            canvas(
                                |bounds, window, _| {
                                    let style = window.text_style();
                                    LineGeometry {
                                        bounds,
                                        font: style.font(),
                                        color: style.color,
                                    }
                                },
                                move |bounds, geometry, window, cx| {
                                    let probe = entity.read(cx);
                                    window.handle_input(
                                        &probe.input_focus,
                                        ElementInputHandler::new(bounds, entity.clone()),
                                        cx,
                                    );
                                    let line = geometry.line(&probe.text, window);
                                    if probe.input_focus.is_focused(window) {
                                        let selection =
                                            line_bounds(bounds, &line, &probe.selection);
                                        window.paint_quad(fill(selection, rgb(0xB0D4F3)));
                                    }
                                    let marked = probe
                                        .marked
                                        .as_ref()
                                        .map(|mark| line_bounds(bounds, &line, mark));
                                    if line
                                        .paint(
                                            bounds.origin,
                                            LINE_HEIGHT,
                                            TextAlign::Left,
                                            None,
                                            window,
                                            cx,
                                        )
                                        .is_err()
                                    {
                                        eprintln!("w5 probe-line-paint-failed");
                                    }
                                    if let Some(mark) = marked {
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(mark.left(), mark.bottom() - px(2.)),
                                                size(mark.size.width, px(2.)),
                                            ),
                                            rgb(0x4E72A3),
                                        ));
                                    }
                                    entity.update(cx, |probe, _| probe.geometry = Some(geometry));
                                },
                            )
                            .w_full()
                            .h(LINE_HEIGHT),
                        ),
                )
                // B 仅是探针测试区标记。点击移交焦点，不主动结算组合。
                .child(
                    div()
                        .id("w5-focus-target")
                        .track_focus(&self.other_focus)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|probe, _, window, cx| {
                                probe.record(
                                    "probe-focus-target",
                                    format_args!("marked={:?}", probe.marked_utf16()),
                                );
                                probe.other_focus.focus(window, cx);
                                cx.notify();
                            }),
                        )
                        .p(px(12.))
                        .bg(rgb(0xFFFFFF))
                        .border_1()
                        .border_color(rgb(target_border))
                        .child("B"),
                )
        }
    }

    struct LineGeometry {
        bounds: Bounds<Pixels>,
        font: Font,
        color: Hsla,
    }

    impl LineGeometry {
        fn line(&self, text: &str, window: &mut Window) -> ShapedLine {
            let run = TextRun {
                len: text.len(),
                font: self.font.clone(),
                color: self.color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window.text_system().shape_line(
                SharedString::from(text.to_owned()),
                FONT_SIZE,
                &[run],
                None,
            )
        }
    }

    fn line_bounds(
        bounds: Bounds<Pixels>,
        line: &ShapedLine,
        bytes: &Range<usize>,
    ) -> Bounds<Pixels> {
        let start = line.x_for_index(bytes.start);
        let end = line.x_for_index(bytes.end);
        Bounds::new(
            point(bounds.left() + start.min(end), bounds.top()),
            size((end - start).abs().max(px(1.)), bounds.size.height),
        )
    }

    fn from_utf16(text: &str, range: &Range<usize>) -> Option<Range<usize>> {
        if range.start > range.end {
            return None;
        }
        let byte_offset = |target| {
            let mut units = 0;
            for (byte_offset, character) in text.char_indices() {
                if units == target {
                    return Some(byte_offset);
                }
                units += character.len_utf16();
                if units > target {
                    return None;
                }
            }
            (units == target).then_some(text.len())
        };
        Some(byte_offset(range.start)?..byte_offset(range.end)?)
    }

    fn to_utf16(text: &str, bytes: &Range<usize>) -> Option<Range<usize>> {
        Some(
            text.get(..bytes.start)?.encode_utf16().count()
                ..text.get(..bytes.end)?.encode_utf16().count(),
        )
    }
}
