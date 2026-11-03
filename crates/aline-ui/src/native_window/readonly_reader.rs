//! 原生变高列表装配；只有当前模式的一个列表活动。

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;

use gpui::{
    AnyElement, App, Bounds, ListAlignment, ListOffset, ListSizingBehavior, ListState, MouseButton,
    Pixels, Point, SharedString, StyledText, Window, canvas, div, fill, list, point, prelude::*,
    px, rgb, size,
};

use super::readonly_hit::{HitError, HitTarget, PreparedLayout};
use super::readonly_layout::{
    LayoutCache, LayoutEnvironment, LayoutError, LayoutKey, LayoutToken, ReaderIndex, RestoreState,
    SourceAnchor,
};
use crate::{DocumentId, EditorMode, ReadonlyDocument};

const MAX_ROWS: usize = 1_000_000;
const MAX_ITEM_BYTES: usize = 64 * 1024;
const HEIGHT_CACHE_CAPACITY: usize = 256;
const COLUMN_WIDTH: f32 = 880.0;
const SCROLLBAR_WIDTH: f32 = 12.0;
const FONT_SIZE: f32 = 16.0;
const LINE_HEIGHT: f32 = 24.0;

struct MeasuredRow {
    row: usize,
    token: LayoutToken,
    projection: aline_markdown::Projection,
    prepared: PreparedLayout,
    bounds: Bounds<Pixels>,
}

pub(super) struct ReaderSurface {
    document: DocumentId,
    generation: u64,
    index: ReaderIndex,
    list: ListState,
    mode: EditorMode,
    heights: Rc<RefCell<LayoutCache>>,
    rendered: Rc<Cell<usize>>,
    rail_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    measured: Rc<RefCell<VecDeque<MeasuredRow>>>,
    restores: Rc<RefCell<RestoreState>>,
    alive: Rc<Cell<bool>>,
}
impl ReaderSurface {
    pub(super) fn new(document: &ReadonlyDocument, generation: u64) -> Result<Self, LayoutError> {
        let index = ReaderIndex::new(document, MAX_ROWS)?;
        let mode = document.core().mode();
        let list = ListState::new(index.row_count(mode), ListAlignment::Top, px(120.0));
        let restores = Rc::new(RefCell::new(RestoreState::new(
            document.core().document_id().clone(),
            document.parsed().snapshot().id().clone(),
        )));
        let scroll_restores = restores.clone();
        list.set_scroll_handler(move |_, _, _| {
            scroll_restores.borrow_mut().cancel_current();
        });
        Ok(Self {
            document: document.core().document_id().clone(),
            generation,
            index,
            list,
            mode,
            heights: Rc::new(RefCell::new(LayoutCache::new(HEIGHT_CACHE_CAPACITY))),
            rendered: Rc::new(Cell::new(0)),
            rail_bounds: Rc::new(Cell::new(None)),
            measured: Rc::new(RefCell::new(VecDeque::new())),
            restores,
            alive: Rc::new(Cell::new(true)),
        })
    }
    pub(super) fn matches(&self, document: &ReadonlyDocument) -> bool {
        &self.document == document.core().document_id()
    }
    pub(super) fn prepare(
        &mut self,
        document: &ReadonlyDocument,
        window: &Window,
    ) -> Result<(), LayoutError> {
        let width = (f32::from(window.viewport_size().width) - SCROLLBAR_WIDTH)
            .clamp(64.0, COLUMN_WIDTH)
            - 32.0;
        let mode = document.core().mode();
        let environment = LayoutEnvironment::new(
            width,
            window.scale_factor(),
            1,
            true,
            FONT_SIZE,
            LINE_HEIGHT,
        )?;
        let key = LayoutKey::new(document, mode, environment);
        if !self.heights.borrow().matches_key(&key) {
            let anchor = if self.heights.borrow().token().is_some() {
                self.restores
                    .borrow()
                    .pending()
                    .map(|target| target.anchor)
                    .or_else(|| self.capture_viewport_anchor(document).ok())
            } else {
                None
            };
            self.heights.borrow_mut().prepare(key)?;
            self.measured.borrow_mut().clear();
            self.list.reset(self.index.row_count(mode));
            if let Some(anchor) = anchor {
                let target = self
                    .index
                    .row_for_anchor(document.parsed(), mode, &anchor)?;
                let token = self
                    .heights
                    .borrow()
                    .token()
                    .ok_or(LayoutError::StaleInput)?;
                self.restores.borrow_mut().begin(anchor, target, token)?;
                self.list.scroll_to(ListOffset {
                    item_ix: target,
                    offset_in_item: px(0.0),
                });
            }
            self.mode = mode;
        }
        self.rendered.set(0);
        Ok(())
    }
    fn capture_viewport_anchor(
        &self,
        document: &ReadonlyDocument,
    ) -> Result<SourceAnchor, LayoutError> {
        let top = self.list.logical_scroll_top();
        let row = top
            .item_ix
            .min(self.index.row_count(self.mode).saturating_sub(1));
        let token = self.heights.borrow().token();
        if let Some(measured) = self
            .measured
            .borrow()
            .iter()
            .rev()
            .find(|measured| measured.row == row && Some(measured.token) == token)
        {
            let viewport_top = self
                .rail_bounds
                .get()
                .map_or(measured.bounds.top(), |bounds| bounds.top());
            let point = point(
                measured.bounds.left(),
                viewport_top.max(measured.bounds.top()),
            );
            if let Ok(candidate) = measured.prepared.index_for_position(point) {
                let local_drawn = match candidate {
                    Ok(local_drawn) | Err(local_drawn) => local_drawn,
                };
                if let Ok(Some(glyph)) = measured.prepared.position_for_index(local_drawn) {
                    return self.index.anchor_from_local(
                        document.parsed(),
                        &measured.projection,
                        local_drawn,
                        aline_buffer::Affinity::Before,
                        f32::from(glyph.y - viewport_top),
                    );
                }
            }
        }
        self.index.anchor(
            document.parsed(),
            self.mode,
            row,
            -f32::from(top.offset_in_item),
        )
    }
    pub(super) fn has_pending_restore(&self) -> bool {
        self.restores.borrow().pending().is_some()
    }
    pub(super) fn cancel_restore(&self) -> bool {
        let request = self
            .restores
            .borrow()
            .pending()
            .map(|pending| pending.request);
        request.is_some_and(|request| self.restores.borrow_mut().cancel(&request))
    }

    pub(super) fn render_row(
        &self,
        document: &mut ReadonlyDocument,
        row: usize,
        failure: SharedString,
        on_hit: impl Fn(
            &HitTarget,
            Result<Result<usize, usize>, HitError>,
            &PreparedLayout,
            &mut Window,
            &mut App,
        ) + 'static,
    ) -> AnyElement {
        self.rendered.set(self.rendered.get().saturating_add(1));
        let projection = self
            .index
            .projection(document.parsed(), self.mode, row, MAX_ITEM_BYTES);
        let projection = projection.ok();
        let text = projection
            .as_ref()
            .map(|projection| SharedString::from(projection.drawn_text().to_owned()))
            .unwrap_or(failure);
        let styled = StyledText::new(text);
        let prepared = PreparedLayout::new(styled.layout().clone());
        let click_prepared = prepared.clone();
        let caret_prepared = prepared.clone();
        let cache = self.heights.clone();
        let token = cache.borrow().token();
        let caret = if self.mode == EditorMode::Source {
            document
                .parsed()
                .snapshot()
                .src_offset(document.core().buffer_selection().head())
                .ok()
                .and_then(|source| {
                    projection
                        .as_ref()?
                        .drawn_at(&source, aline_buffer::Affinity::After)
                        .ok()
                })
                .map(|mapped| mapped.value().byte_offset())
        } else {
            None
        };
        let geometry_projection = projection.clone();
        let hit = token
            .and_then(|token| {
                projection.map(|projection| HitTarget::new(document, token, projection))
            })
            .transpose()
            .ok()
            .flatten();
        let click_cache = cache.clone();
        let geometry_prepared = prepared.clone();
        let geometry_rows = self.measured.clone();
        let restore_state = self.restores.clone();
        let restore_list = self.list.clone();
        let restore_document = document.core().document_id().clone();
        let viewport_bounds = self.rail_bounds.clone();
        let text_bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
        let measured_bounds = text_bounds.clone();
        let measure_ready = prepared.clone();
        let restore_alive = self.alive.clone();
        let restore_cache = self.heights.clone();
        let source_start = self
            .index
            .source_range(document.parsed(), self.mode, row)
            .map(|range| range.byte_range().start)
            .unwrap_or_default();
        let element_id =
            SharedString::from(format!("reader-row-{}-{source_start}", self.generation));
        let content = div()
            .relative()
            .w_full()
            .max_w(px(COLUMN_WIDTH))
            .px_4()
            .when(self.mode == EditorMode::Visual, |div| div.py_2())
            .min_h(px(LINE_HEIGHT))
            .text_size(px(FONT_SIZE))
            .line_height(px(LINE_HEIGHT))
            .whitespace_normal()
            .when(self.mode == EditorMode::Source, |div| {
                div.font_family("Menlo")
            })
            .child(styled)
            .on_children_prepainted(move |bounds, _, _| {
                measure_ready.mark_prepainted();
                text_bounds.set(bounds.first().copied());
            })
            .children(caret.map(|caret| {
                canvas(
                    move |_, _, _| (),
                    move |_, (), window, _| {
                        if let Ok(Some(point)) = caret_prepared.position_for_index(caret) {
                            window.paint_quad(fill(
                                Bounds::new(point, size(px(1.5), px(LINE_HEIGHT))),
                                rgb(0x0969DA),
                            ));
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            }));
        div()
            .w_full()
            .flex()
            .justify_center()
            .child(content)
            .on_children_prepainted(move |bounds, window, cx| {
                prepared.mark_prepainted();
                if let (Some(token), Some(item_bounds)) = (token, bounds.first()) {
                    cache.borrow_mut().record(token, row, f32::from(item_bounds.size.height));
                    if let (Some(text_bounds), Some(projection)) = (measured_bounds.get(), geometry_projection.as_ref()) {
                        let mut rows = geometry_rows.borrow_mut();
                        if let Some(old) = rows.iter().position(|measured| measured.row == row) { rows.remove(old); }
                        if rows.len() == HEIGHT_CACHE_CAPACITY { rows.pop_front(); }
                        rows.push_back(MeasuredRow { row, token, projection: projection.clone(), prepared: geometry_prepared.clone(), bounds: text_bounds });
                        drop(rows);
                        let pending = restore_state.borrow().pending();
                        if let Some(pending) = pending.filter(|pending| pending.row == row && pending.layout == token)
                            && let Ok(local) = projection.drawn_at(pending.anchor.source(), pending.anchor.affinity())
                            && let Ok(Some(glyph)) = geometry_prepared.position_for_index(local.value().byte_offset())
                        {
                            let offset = glyph.y - item_bounds.top() - px(pending.anchor.viewport_delta());
                            if f32::from(offset).is_finite()
                                && restore_state.borrow_mut().schedule_for_row(&pending.request, &restore_document, token, row)
                            {
                                let restore_state = restore_state.clone();
                                let restore_list = restore_list.clone();
                                let restore_document = restore_document.clone();
                                let restore_alive = restore_alive.clone();
                                let restore_cache = restore_cache.clone();
                                let viewport_bounds = viewport_bounds.clone();
                                window.defer(cx, move |window, _| {
                                    if !restore_alive.get() || restore_cache.borrow().token() != Some(token) { return; }
                                    if restore_state.borrow_mut().take_for_row(&pending.request, &restore_document, token, row).is_some() {
                                        restore_list.scroll_to(ListOffset { item_ix: row, offset_in_item: offset.max(px(0.0)) });
                                        #[cfg(debug_assertions)]
                                        eprintln!("m1-anchor restored source={} row={} viewport_delta={} glyph_y={} viewport_y={}", pending.anchor.source().byte_offset(), row, pending.anchor.viewport_delta(), f32::from(glyph.y), viewport_bounds.get().map_or(0.0, |bounds| f32::from(bounds.top())));
                                        #[cfg(not(debug_assertions))]
                                        let _ = viewport_bounds;
                                        window.refresh();
                                    }
                                });
                            }
                        }
                    }
                }
            })
            .id(element_id)
            .on_mouse_down(MouseButton::Left, move |event, window, cx| {
                if let Some(target) = &hit {
                    #[cfg(debug_assertions)]
                    if let Ok(bounds) = click_prepared.bounds() {
                        eprintln!("m1-hit-layout point_x={} point_y={} bounds_x={} bounds_y={} bounds_w={} bounds_h={}", f32::from(event.position.x), f32::from(event.position.y), f32::from(bounds.origin.x), f32::from(bounds.origin.y), f32::from(bounds.size.width), f32::from(bounds.size.height));
                    }
                    let candidate = target
                        .ensure_layout(click_cache.borrow().token())
                        .and_then(|()| click_prepared.index_for_position(event.position));
                    on_hit(target, candidate, &click_prepared, window, cx);
                }
            })
            .into_any_element()
    }
    pub(super) fn render(
        &self,
        render_row: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> AnyElement {
        let bounds = self.rail_bounds.clone();
        let rail_list = self.list.clone();
        let click_list = self.list.clone();
        let click_bounds = self.rail_bounds.clone();
        let move_list = self.list.clone();
        let move_bounds = self.rail_bounds.clone();
        div()
            .flex()
            .size_full()
            .min_h_0()
            .child(
                list(self.list.clone(), render_row)
                    .with_sizing_behavior(ListSizingBehavior::Auto)
                    .flex_1()
                    .h_full(),
            )
            .child(
                div()
                    .id("reader-scrollbar")
                    .w(px(SCROLLBAR_WIDTH))
                    .h_full()
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, move |event, window, _| {
                        if scroll_from_rail(&click_list, click_bounds.get(), event.position) {
                            window.refresh();
                        }
                    })
                    .on_mouse_move(move |event, window, _| {
                        if event.pressed_button == Some(MouseButton::Left)
                            && scroll_from_rail(&move_list, move_bounds.get(), event.position)
                        {
                            window.refresh();
                        }
                    })
                    .child(
                        canvas(
                            move |rail, _, _| {
                                bounds.set(Some(rail));
                            },
                            move |rail, (), window, _| {
                                window.paint_quad(fill(rail, rgb(0xF6F8FA)));
                                let height = f32::from(rail.size.height);
                                let maximum =
                                    f32::from(rail_list.max_offset_for_scrollbar().y).max(0.0);
                                if height <= 0.0 || maximum <= 0.0 {
                                    return;
                                }
                                let thumb_height = (height * height / (height + maximum))
                                    .clamp(20.0_f32.min(height), height);
                                let fraction =
                                    (-f32::from(rail_list.scroll_px_offset_for_scrollbar().y)
                                        / maximum)
                                        .clamp(0.0, 1.0);
                                let top = (height - thumb_height) * fraction;
                                let thumb = Bounds::new(
                                    point(rail.left() + px(3.0), rail.top() + px(top)),
                                    size(px(6.0), px(thumb_height)),
                                );
                                window.paint_quad(fill(thumb, rgb(0x8C959F)));
                            },
                        )
                        .size_full(),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn layout_token(&self) -> Option<super::readonly_layout::LayoutToken> {
        self.heights.borrow().token()
    }
    pub(super) fn page(&self, direction: f32, viewport_height: Pixels) {
        self.cancel_restore();
        self.list.scroll_by(viewport_height * (direction * 0.8));
    }
    pub(super) fn start(&self) {
        self.cancel_restore();
        self.list.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: px(0.0),
        });
    }
    pub(super) fn end(&self) {
        self.cancel_restore();
        self.list.scroll_to_end();
    }
    #[cfg(debug_assertions)]
    pub(super) fn report(&self) {
        let heights = self.heights.borrow();
        let top = self.list.logical_scroll_top();
        eprintln!(
            "m1-layout mode={:?} rows={} rendered={} height_cache={}/{} top_row={} top_height_cached={} measured_rows={}",
            self.mode,
            self.list.item_count(),
            self.rendered.get(),
            heights.len(),
            HEIGHT_CACHE_CAPACITY,
            top.item_ix,
            heights.height(top.item_ix).is_some(),
            self.measured.borrow().len()
        );
    }
}

fn scroll_from_rail(
    list: &ListState,
    bounds: Option<Bounds<Pixels>>,
    point: Point<Pixels>,
) -> bool {
    let Some(bounds) = bounds else {
        return false;
    };
    let height = f32::from(bounds.size.height);
    if height <= 0.0 {
        return false;
    }
    let fraction = (f32::from(point.y - bounds.top()) / height).clamp(0.0, 1.0);
    let maximum = list.max_offset_for_scrollbar().y;
    list.set_offset_from_scrollbar(gpui::point(px(0.0), -maximum * fraction));
    true
}

impl Drop for ReaderSurface {
    fn drop(&mut self) {
        self.alive.set(false);
        self.restores.borrow_mut().cancel_current();
        self.measured.borrow_mut().clear();
    }
}
