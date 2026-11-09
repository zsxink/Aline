//! 原生变高列表装配；只有当前模式的一个列表活动。
//!
//! 可视化行按内容计划装配：文本/代码行使用单一 `StyledText`，保持字形级命中与
//! 锚点恢复；表格按单元格各自布局并各自命中；目录、Front matter、分隔线只提供
//! 生成导航或原文占位。这里不重新解析 Markdown、不执行 HTML、不加载图片或网络
//! 资源；行内文字始终来自同一 `Parsed` 的绘制投影窗口。

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ops::Range;
use std::rc::Rc;

use aline_buffer::Affinity;
use aline_markdown::{AlertKind, CodeLine, MappingBudget, Parsed, Projection, TableAlignment};
use gpui::{
    AnyElement, App, Bounds, Div, Font, FontFallbacks, FontStyle, FontWeight, ListAlignment,
    ListOffset, ListSizingBehavior, ListState, MouseButton, Pixels, Point, Role, SharedString,
    StrikethroughStyle, StyledText, TextAlign, TextRun, UnderlineStyle, Window, canvas, div, fill,
    list, point, prelude::*, px, rgb, rgba, size,
};

use super::geometry::{
    BODY_FONT_SIZE, BODY_LINE_HEIGHT, ContentMetrics, SCROLLBAR_WIDTH, content_metrics,
};
use super::readonly_content::{
    Block, ContentLabel, ContentRow, GrayKind, Leaf, LeafKind, ListMarker, TableCell,
    footnote_definition_target, footnote_reference_target,
};
use super::readonly_hit::{HitError, HitTarget, PreparedLayout};
use super::readonly_layout::{
    LayoutCache, LayoutEnvironment, LayoutError, LayoutKey, LayoutToken, ReaderIndex, RestoreState,
    SourceAnchor,
};
use super::tokens;
use crate::fonts::FontEnvironment;
use crate::{DocumentId, EditorMode, ReadonlyDocument};

const MAX_ROWS: usize = 1_000_000;
const MAX_ITEM_BYTES: usize = 64 * 1024;
const HEIGHT_CACHE_CAPACITY: usize = 256;

/// 代码字体；与源码视图使用同一族名。
const CODE_FONT: &str = "Menlo";

/// 代码字号与行高（原型 `pre code` 13px / 1.6）。
const CODE_FONT_SIZE: f32 = 13.0;
const CODE_LINE_HEIGHT: f32 = 20.8;

/// 代码块内边距（原型 `pre` 18px 20px）。
const CODE_PADDING_X: f32 = 20.0;
const CODE_PADDING_Y: f32 = 18.0;

/// 表格字号与行高（原型 `.editor table` 14px / 1.65）。
const TABLE_FONT_SIZE: f32 = 14.0;
const TABLE_LINE_HEIGHT: f32 = 23.1;

/// 表格单元格内边距（原型 `th, td` 9px 13px）。
const CELL_PADDING_X: f32 = 13.0;
const CELL_PADDING_Y: f32 = 9.0;

/// 目录字号与行高（原型 `.editor .toc` 14px）。
const TOC_FONT_SIZE: f32 = 14.0;
const TOC_LINE_HEIGHT: f32 = 22.0;

/// 引用块左边框（原型 `blockquote` 4px）与内边距。
const QUOTE_PADDING: f32 = 16.0;

/// 列表层级缩进（原型 `ul, ol` 的 `padding-left: 2em`）。
const LIST_INDENT: f32 = 32.0;

/// 列表标记槽位宽度。
const MARKER_WIDTH: f32 = 24.0;

/// 目录每级缩进。
const TOC_INDENT: f32 = 14.0;

/// 内容区生成标签；不是源码文字，不参与映射。
pub(super) struct ContentCopy {
    pub(super) front_matter: SharedString,
    pub(super) footnote: SharedString,
    pub(super) toc_empty: SharedString,
    alerts: [SharedString; 5],
}

impl ContentCopy {
    /// 构造内容标签；顺序与 `AlertKind` 的固定顺序一致。
    pub(super) fn new(
        front_matter: SharedString,
        footnote: SharedString,
        toc_empty: SharedString,
        alerts: [SharedString; 5],
    ) -> Self {
        Self {
            front_matter,
            footnote,
            toc_empty,
            alerts,
        }
    }

    fn alert(&self, kind: AlertKind) -> SharedString {
        let index = match kind {
            AlertKind::Note => 0,
            AlertKind::Tip => 1,
            AlertKind::Important => 2,
            AlertKind::Warning => 3,
            AlertKind::Caution => 4,
        };
        self.alerts.get(index).cloned().unwrap_or_default()
    }
}

/// 整行继承的绘制样式；行内样式在此基础上叠加。
#[derive(Clone)]
struct RunBase {
    weight: FontWeight,
    color: u32,
    family: SharedString,
    fallbacks: Option<FontFallbacks>,
}

/// 行内样式叠加后的文本 run；`text` 是该行的完整绘制文字。
fn build_runs(text: &str, base_offset: usize, leaves: &[Leaf], base: &RunBase) -> Vec<TextRun> {
    let mut runs = Vec::new();
    let mut cursor = 0usize;
    for leaf in leaves {
        let start = leaf.drawn.start.saturating_sub(base_offset).min(text.len());
        let end = leaf.drawn.end.saturating_sub(base_offset).min(text.len());
        if start < cursor {
            continue;
        }
        if start > cursor && !push_plain(&mut runs, text, cursor..start, base) {
            return Vec::new();
        }
        if !push_leaf(&mut runs, text, start..end, leaf, base) {
            return Vec::new();
        }
        cursor = cursor.max(end);
    }
    if cursor < text.len() && !push_plain(&mut runs, text, cursor..text.len(), base) {
        return Vec::new();
    }
    runs
}

fn push_plain(runs: &mut Vec<TextRun>, text: &str, range: Range<usize>, base: &RunBase) -> bool {
    let Some(slice) = text.get(range) else {
        return false;
    };
    if slice.is_empty() {
        return true;
    }
    runs.push(TextRun {
        len: slice.len(),
        font: Font {
            family: base.family.clone(),
            fallbacks: base.fallbacks.clone(),
            weight: base.weight,
            ..Font::default()
        },
        color: rgb(base.color).into(),
        background_color: None,
        underline: None,
        strikethrough: None,
    });
    true
}

fn push_leaf(
    runs: &mut Vec<TextRun>,
    text: &str,
    range: Range<usize>,
    leaf: &Leaf,
    base: &RunBase,
) -> bool {
    let Some(slice) = text.get(range) else {
        return false;
    };
    if slice.is_empty() {
        return true;
    }
    let style = leaf.style;
    let weight = if style.bold {
        FontWeight::BOLD
    } else {
        base.weight
    };
    let italic = style.italic;
    let family = if style.code || style.math {
        SharedString::from(CODE_FONT)
    } else {
        base.family.clone()
    };
    let color = if style.link
        || matches!(leaf.kind, LeafKind::FootnoteReference)
        || matches!(leaf.kind, LeafKind::TaskMarker { .. })
    {
        tokens::ACCENT
    } else if style.math
        || style.image
        || matches!(
            leaf.kind,
            LeafKind::Math | LeafKind::Image | LeafKind::Unsupported
        )
    {
        tokens::SECONDARY
    } else {
        base.color
    };
    let background = if style.mark {
        Some(rgb(tokens::MARK_SURFACE).into())
    } else if style.code {
        Some(rgb(tokens::CODE_SURFACE).into())
    } else {
        None
    };
    let underline = (style.link || style.underline).then_some(UnderlineStyle {
        thickness: px(1.0),
        color: None,
        wavy: false,
    });
    let strikethrough = style.strike.then_some(StrikethroughStyle {
        thickness: px(1.0),
        color: None,
    });
    runs.push(TextRun {
        len: slice.len(),
        font: Font {
            family,
            fallbacks: base.fallbacks.clone(),
            weight,
            style: if italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            ..Font::default()
        },
        color: rgb(color).into(),
        background_color: background,
        underline,
        strikethrough,
    });
    true
}

fn styled_text(text: SharedString, runs: Vec<TextRun>) -> StyledText {
    let styled = StyledText::new(text);
    if runs.is_empty() {
        styled
    } else {
        styled.with_runs(runs)
    }
}

/// 标题字号与行高（原型 `.editor h1..h6`，行高 1.35）。
fn heading_metrics(level: u8) -> (f32, f32) {
    match level {
        1 => (32.0, 43.2),
        2 => (24.0, 32.4),
        3 => (20.0, 27.0),
        4 => (17.0, 22.95),
        5 => (15.0, 20.25),
        _ => (14.0, 18.9),
    }
}

/// 行块上下留白：由原型块级 margin 折算到行内 padding（相邻行各取一半）。
fn block_spacing(row: &ContentRow) -> (f32, f32) {
    if let Some(level) = row.heading {
        return match level {
            1 => (0.0, 20.0),
            2 => (16.0, 17.0),
            _ => (14.0, 8.0),
        };
    }
    match &row.block {
        Block::Code { .. } => (9.0, 12.0),
        Block::Table { .. } => (10.0, 12.0),
        Block::Rule => (14.0, 14.0),
        Block::Toc { .. } => (8.0, 8.0),
        Block::FrontMatter => (4.0, 4.0),
        Block::Text { leaves } => {
            if row.quote_depth > 0 {
                (10.0, 10.0)
            } else if row.list_depth > 0 {
                (2.0, 2.0)
            } else if leaves.is_empty() && row.drawn.is_none() {
                (0.0, 0.0)
            } else {
                (8.0, 8.0)
            }
        }
    }
}

fn marker_text(marker: ListMarker) -> String {
    match marker {
        ListMarker::Bullet => "•".to_owned(),
        ListMarker::Ordered(number) => format!("{number}."),
    }
}

/// 代码行号栏文字：每个逻辑行一行，行号从解析器给出的编号开始。
fn gutter_text(lines: &[CodeLine]) -> String {
    let mut text = String::new();
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str(&line.number().to_string());
    }
    text
}

/// 行号只在代码投影成功时生成；投影失败时该行显示受限提示，不构造整块行号文本。
fn visible_gutter(projected: bool, lines: &[CodeLine]) -> Option<String> {
    projected.then(|| gutter_text(lines))
}

/// 锚点恢复的项内偏移：有字形时按字形定位，无字形（非文本行或投影失败）回退行首。
fn restore_offset(glyph: Option<Point<Pixels>>, item_top: Pixels, viewport_delta: f32) -> Pixels {
    let base = glyph.map_or(px(0.0), |glyph| glyph.y - item_top);
    base - px(viewport_delta)
}

/// 超限表格/目录的受限提示：不渲染部分结构，完整源码保留在源码视图。
fn bounded_notice(text: SharedString) -> AnyElement {
    div()
        .w_full()
        .bg(rgb(tokens::CODE_SURFACE))
        .border_1()
        .border_color(rgb(tokens::BORDER))
        .rounded_md()
        .px(px(16.0))
        .py(px(12.0))
        .text_size(px(13.0))
        .text_color(rgb(tokens::SECONDARY))
        .child(text)
        .into_any_element()
}

/// 行号栏右侧留白；宽度计算必须把这部分计入，否则数字区会被挤窄。
const GUTTER_GAP: f32 = 12.0;
/// 等宽字体单个数字的估算宽度。
const GUTTER_DIGIT_WIDTH: f32 = 8.0;

/// 行号栏宽度：按末行行号位数估算等宽数字宽度，并含右侧留白与少量余量。
fn gutter_width(text: &str) -> f32 {
    let digits = text
        .rsplit('\n')
        .next()
        .map_or(1, |last| last.chars().count().max(1));
    GUTTER_DIGIT_WIDTH * digits as f32 + GUTTER_GAP + 4.0
}

fn table_align(align: TableAlignment) -> TextAlign {
    match align {
        TableAlignment::None | TableAlignment::Left => TextAlign::Left,
        TableAlignment::Center => TextAlign::Center,
        TableAlignment::Right => TextAlign::Right,
    }
}

fn alert_color(kind: AlertKind) -> u32 {
    match kind {
        AlertKind::Note => tokens::ALERT_NOTE,
        AlertKind::Tip => tokens::ALERT_TIP,
        AlertKind::Important => tokens::ALERT_IMPORTANT,
        AlertKind::Warning => tokens::ALERT_WARNING,
        AlertKind::Caution => tokens::ALERT_CAUTION,
    }
}

/// 尚未取得窗口尺寸时的安全默认：宽窗档位的列宽与正文宽度。
fn default_metrics() -> ContentMetrics {
    content_metrics(1200.0, 1200.0)
}

struct MeasuredRow {
    row: usize,
    token: LayoutToken,
    projection: Projection,
    prepared: PreparedLayout,
    bounds: Bounds<Pixels>,
}

/// 行级命中回调；参数与 `ReaderSurface::render_row` 的调用方约定一致。
type HitCallback = dyn Fn(
    &HitTarget,
    Result<Result<usize, usize>, HitError>,
    &PreparedLayout,
    &mut Window,
    &mut App,
);

/// 生成导航回调；参数是目标源码字节偏移。
type NavigateCallback = dyn Fn(usize, &mut Window, &mut App);

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
    metrics: Cell<ContentMetrics>,
    font: Font,
    alive: Rc<Cell<bool>>,
    /// 上次补高度提示时的视口宽度；列表宽度变化时 GPUI 会清空高度提示。
    hint_width: Rc<Cell<Option<f32>>>,
}
impl ReaderSurface {
    pub(super) fn new(
        document: &ReadonlyDocument,
        generation: u64,
        evidence: Option<super::readonly_evidence::SharedEvidence>,
    ) -> Result<Self, LayoutError> {
        let index = ReaderIndex::new(document, MAX_ROWS)?;
        let mode = document.core().mode();
        let list = ListState::new(index.row_count(mode), ListAlignment::Top, px(120.0));
        let restores = Rc::new(RefCell::new(RestoreState::new(
            document.core().document_id().clone(),
            document.parsed().snapshot().id().clone(),
        )));
        let scroll_restores = restores.clone();
        list.set_scroll_handler(move |_, _, _| {
            if let Some(evidence) = &evidence {
                evidence.borrow_mut().event("scroll-wheel");
            }
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
            metrics: Cell::new(default_metrics()),
            font: Font::default(),
            alive: Rc::new(Cell::new(true)),
            hint_width: Rc::new(Cell::new(None)),
        })
    }
    pub(super) fn matches(&self, document: &ReadonlyDocument) -> bool {
        &self.document == document.core().document_id()
    }
    /// 准备当前布局键；返回是否重建了列表（调用方需在帧末补未测量行高度提示）。
    pub(super) fn prepare(
        &mut self,
        document: &ReadonlyDocument,
        window: &Window,
        metrics: ContentMetrics,
        font: &FontEnvironment,
    ) -> Result<bool, LayoutError> {
        self.metrics.set(metrics);
        self.font = font.font();
        let mode = document.core().mode();
        let environment = LayoutEnvironment::new(
            metrics.text_width,
            window.scale_factor(),
            font.generation(),
            true,
            BODY_FONT_SIZE,
            BODY_LINE_HEIGHT,
        )?;
        let key = LayoutKey::new(document, mode, environment);
        let mut reset = false;
        if !self.heights.borrow().matches_key(&key) {
            reset = true;
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
            // 列表按像素滚动时，未测量行若按零高度计入总高，End 后 PageUp
            // 或滚动条拖动会跨过整段未测量区跳到首个已测量块。这里先重置，
            // 由调用方在帧末调用 `schedule_height_hint` 给未测量行补正文行高
            // 提示；该提示会被实际测量高度替换，且只影响像素跳转与滚动条比例。
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
        Ok(reset)
    }
    /// 在帧末给未测量行补统一高度提示。
    ///
    /// GPUI `list` 在首次布局或宽度变化时会重建未测量行并清空高度提示，
    /// 因此在渲染阶段设置提示会被随后的 prepaint 抹掉；必须延后到帧末执行。
    pub(super) fn schedule_height_hint(&self, window: &mut Window, cx: &mut App) {
        let list = self.list.clone();
        let alive = self.alive.clone();
        window.defer(cx, move |window, _| {
            if alive.get() {
                list.with_uniform_item_height(px(BODY_LINE_HEIGHT));
                window.refresh();
            }
        });
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
                        Affinity::Before,
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
    /// 把包含目标源码偏移的行滚动到顶部；用于目录与脚注跳转。
    pub(super) fn scroll_to_source(
        &self,
        document: &ReadonlyDocument,
        offset: usize,
    ) -> Result<(), LayoutError> {
        let source = document
            .parsed()
            .snapshot()
            .src_offset(offset)
            .map_err(|_| LayoutError::InvalidRange)?;
        let anchor = SourceAnchor::new(document.parsed(), source, Affinity::After, 0.0)?;
        let row = self
            .index
            .row_for_anchor(document.parsed(), self.mode, &anchor)?;
        self.cancel_restore();
        self.list.scroll_to(ListOffset {
            item_ix: row,
            offset_in_item: px(0.0),
        });
        #[cfg(debug_assertions)]
        eprintln!("m1-navigate source={offset} row={row} mode={:?}", self.mode);
        Ok(())
    }

    pub(super) fn render_row(
        &self,
        document: &mut ReadonlyDocument,
        row: usize,
        failure: SharedString,
        copy: &ContentCopy,
        on_hit: impl Fn(
            &HitTarget,
            Result<Result<usize, usize>, HitError>,
            &PreparedLayout,
            &mut Window,
            &mut App,
        ) + 'static,
        on_navigate: impl Fn(usize, &mut Window, &mut App) + 'static,
    ) -> AnyElement {
        self.rendered.set(self.rendered.get().saturating_add(1));
        if self.mode == EditorMode::Source {
            return self.render_source_row(document, row, failure, on_hit);
        }
        self.render_visual_row(
            document,
            row,
            failure,
            copy,
            Rc::new(on_hit),
            Rc::new(on_navigate),
        )
    }

    fn render_source_row(
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
        let metrics = self.metrics.get();
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
        let mark_prepared = prepared.clone();
        let cache = self.heights.clone();
        let token = cache.borrow().token();
        let caret = document
            .parsed()
            .snapshot()
            .src_offset(document.core().buffer_selection().head())
            .ok()
            .and_then(|source| projection.as_ref()?.drawn_at(&source, Affinity::After).ok())
            .map(|mapped| mapped.value().byte_offset());
        let geometry_projection = projection.clone();
        let hit = token
            .and_then(|token| {
                projection
                    .clone()
                    .map(|projection| HitTarget::new(document, token, projection))
            })
            .transpose()
            .ok()
            .flatten();
        let click_cache = cache.clone();
        let restore_document = document.core().document_id().clone();
        let text_bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
        let bounds_cell = text_bounds.clone();
        let source_start = self
            .index
            .source_range(document.parsed(), self.mode, row)
            .map(|range| range.byte_range().start)
            .unwrap_or_default();
        let last_row = self.index.row_count(self.mode).saturating_sub(1);
        let element_id =
            SharedString::from(format!("reader-row-{}-{source_start}", self.generation));
        let content = div()
            .relative()
            .w_full()
            .max_w(px(metrics.column_width))
            .px(px(metrics.padding_x))
            .pt(px(if row == 0 { metrics.padding_top } else { 0.0 }))
            .pb(px(if row == last_row {
                metrics.padding_bottom
            } else {
                0.0
            }))
            .min_h(px(BODY_LINE_HEIGHT))
            .text_size(px(BODY_FONT_SIZE))
            .line_height(px(BODY_LINE_HEIGHT))
            .whitespace_normal()
            .font_family(CODE_FONT)
            .child(styled)
            .on_children_prepainted(move |bounds, _, _| {
                mark_prepared.mark_prepainted();
                bounds_cell.set(bounds.first().copied());
            })
            .children(caret.map(|caret| {
                canvas(
                    move |_, _, _| (),
                    move |_, (), window, _| {
                        if let Ok(Some(point)) = caret_prepared.position_for_index(caret) {
                            window.paint_quad(fill(
                                Bounds::new(point, size(px(1.5), px(BODY_LINE_HEIGHT))),
                                rgb(tokens::LINK),
                            ));
                        }
                    },
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full()
            }));
        let outer = div().w_full().flex().justify_center().child(content);
        let outer = self.measured_row(
            outer,
            row,
            token,
            geometry_projection.map(|projection| (projection, prepared.clone())),
            text_bounds,
            restore_document,
        );
        outer
            .id(element_id)
            .when_some(hit, |outer, target| {
                outer.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    #[cfg(debug_assertions)]
                    if let Ok(bounds) = click_prepared.bounds() {
                        eprintln!("m1-hit-layout point_x={} point_y={} bounds_x={} bounds_y={} bounds_w={} bounds_h={}", f32::from(event.position.x), f32::from(event.position.y), f32::from(bounds.origin.x), f32::from(bounds.origin.y), f32::from(bounds.size.width), f32::from(bounds.size.height));
                    }
                    let candidate = target
                        .ensure_layout(click_cache.borrow().token())
                        .and_then(|()| click_prepared.index_for_position(event.position));
                    on_hit(&target, candidate, &click_prepared, window, cx);
                })
            })
            .into_any_element()
    }

    fn render_visual_row(
        &self,
        document: &mut ReadonlyDocument,
        row: usize,
        failure: SharedString,
        copy: &ContentCopy,
        on_hit: Rc<HitCallback>,
        on_navigate: Rc<NavigateCallback>,
    ) -> AnyElement {
        let metrics = self.metrics.get();
        let Some(content) = self.index.content_row(row) else {
            return div().into_any_element();
        };
        let source_start = content.source.start;
        let token = self.heights.borrow().token();
        let document_id = document.core().document_id().clone();
        let projection = self
            .index
            .projection(document.parsed(), EditorMode::Visual, row, MAX_ITEM_BYTES)
            .ok();
        let (top, bottom) = block_spacing(content);
        let quote = content.quote_depth > 0;
        let alert = content.alert;
        let heading = content.heading;
        let gray = content.gray.is_some();
        let list_depth = content.list_depth;
        let marker = content.marker;
        let label = content.label;
        let alert_badge = content.alert_badge.then_some(alert).flatten();
        let fallback = match content.block {
            Block::Rule | Block::FrontMatter => Some(content.source.start),
            _ => None,
        };
        let footnote_target = match &content.block {
            Block::Text { leaves }
                if leaves
                    .iter()
                    .any(|leaf| leaf.kind == LeafKind::FootnoteReference) =>
            {
                footnote_reference_target(document.parsed(), &content.source)
            }
            Block::Text { .. } if content.gray == Some(GrayKind::Footnote) => {
                footnote_definition_target(document.parsed(), &content.source)
            }
            _ => None,
        };
        let base_offset = content.drawn.as_ref().map_or(0, |drawn| drawn.start);
        let (font_size, line_height) =
            heading.map_or((BODY_FONT_SIZE, BODY_LINE_HEIGHT), heading_metrics);
        let mut base_color = if gray || (quote && alert.is_none()) {
            tokens::SECONDARY
        } else {
            tokens::TEXT
        };
        if heading == Some(6) {
            base_color = tokens::SECONDARY;
        }
        let base = RunBase {
            weight: if heading.is_some() {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            },
            color: base_color,
            family: self.font.family.clone(),
            fallbacks: self.font.fallbacks.clone(),
        };
        let text_bounds = Rc::new(Cell::new(None::<Bounds<Pixels>>));
        let mut geometry: Option<(Projection, PreparedLayout)> = None;
        let mut hit: Option<HitTarget> = None;
        let block: AnyElement = match &content.block {
            Block::Text { leaves } => {
                // 缺口行没有绘制范围，显示为空行；只有存在绘制范围却取窗口失败时才提示。
                let text = match &projection {
                    Some(projection) => SharedString::from(projection.drawn_text().to_owned()),
                    None if content.drawn.is_none() => SharedString::from(""),
                    None => failure.clone(),
                };
                let runs = if projection.is_some() {
                    build_runs(&text, base_offset, leaves, &base)
                } else {
                    Vec::new()
                };
                let styled = styled_text(text, runs);
                let prepared = PreparedLayout::new(styled.layout().clone());
                let mark_prepared = prepared.clone();
                let bounds_cell = text_bounds.clone();
                let text_wrap = div()
                    .flex_1()
                    .min_w_0()
                    .child(styled)
                    .on_children_prepainted(move |bounds, _, _| {
                        mark_prepared.mark_prepainted();
                        bounds_cell.set(bounds.first().copied());
                    });
                if let Some(projection) = projection.clone() {
                    geometry = Some((projection, prepared));
                }
                hit = token
                    .zip(projection.clone())
                    .and_then(|(token, projection)| {
                        HitTarget::new(document, token, projection).ok()
                    });
                div()
                    .flex()
                    .items_start()
                    .when_some(label, |line, label| line.child(footnote_label(label, copy)))
                    .when_some(marker, |line, marker| line.child(marker_chip(marker)))
                    .child(text_wrap)
                    .into_any_element()
            }
            Block::Code {
                info,
                lines,
                leaves,
            } => {
                let text = projection
                    .as_ref()
                    .map(|projection| SharedString::from(projection.drawn_text().to_owned()))
                    .unwrap_or_else(|| failure.clone());
                let runs = if projection.is_some() {
                    build_runs(&text, base_offset, leaves, &base)
                } else {
                    Vec::new()
                };
                let styled = styled_text(text, runs);
                let prepared = PreparedLayout::new(styled.layout().clone());
                let mark_prepared = prepared.clone();
                let bounds_cell = text_bounds.clone();
                let gutter = visible_gutter(projection.is_some(), lines).map(|numbers| {
                    div()
                        .flex_none()
                        .w(px(gutter_width(&numbers)))
                        .pr(px(12.0))
                        .text_align(TextAlign::Right)
                        .font_family(SharedString::from(CODE_FONT))
                        .text_color(rgb(tokens::SECONDARY))
                        .child(StyledText::new(SharedString::from(numbers)))
                });
                let scroller = div()
                    .flex_1()
                    .min_w_0()
                    .whitespace_nowrap()
                    .child(styled)
                    .on_children_prepainted(move |bounds, _, _| {
                        mark_prepared.mark_prepainted();
                        bounds_cell.set(bounds.first().copied());
                    })
                    .id(SharedString::from(format!(
                        "code-scroll-{}-{source_start}",
                        self.generation
                    )))
                    .overflow_x_scroll()
                    .restrict_scroll_to_axis();
                if let Some(projection) = projection.clone() {
                    geometry = Some((projection, prepared));
                }
                hit = token
                    .zip(projection.clone())
                    .and_then(|(token, projection)| {
                        HitTarget::new(document, token, projection).ok()
                    });
                div()
                    .w_full()
                    .bg(rgb(tokens::CODE_SURFACE))
                    .border_1()
                    .border_color(rgb(tokens::BORDER))
                    .rounded_md()
                    .px(px(CODE_PADDING_X))
                    .py(px(CODE_PADDING_Y))
                    .font_family(SharedString::from(CODE_FONT))
                    .text_size(px(CODE_FONT_SIZE))
                    .line_height(px(CODE_LINE_HEIGHT))
                    .when(!info.is_empty(), |code| {
                        code.child(
                            div()
                                .mb(px(6.0))
                                .text_size(px(12.0))
                                .text_color(rgb(tokens::SECONDARY))
                                .child(SharedString::from(info.clone())),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .w_full()
                            .when_some(gutter, |row, gutter| row.child(gutter))
                            .child(scroller),
                    )
                    .into_any_element()
            }
            Block::Table {
                rows, truncated, ..
            } => {
                // 超过结构上限或投影失败的表格不逐行渲染：显示受限提示，
                // 完整源码保留在源码视图中。
                if *truncated || projection.is_none() {
                    bounded_notice(failure.clone())
                } else {
                    let last_row = rows.len().saturating_sub(1);
                    let mut table = div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .rounded_md()
                        .border_1()
                        .border_color(rgb(tokens::BORDER))
                        .overflow_hidden();
                    for (index, table_row) in rows.iter().enumerate() {
                        let last_cell = table_row.cells.len().saturating_sub(1);
                        let mut line = div().flex().w_full();
                        for (cell_index, cell) in table_row.cells.iter().enumerate() {
                            line = line.child(self.table_cell(
                                document,
                                token,
                                cell,
                                table_row.head,
                                index > 0 && index % 2 == 0,
                                cell_index == last_cell,
                                index == last_row,
                                &failure,
                                on_hit.clone(),
                            ));
                        }
                        table = table.child(line);
                    }
                    table.into_any_element()
                }
            }
            Block::Toc { entries, truncated } => {
                if *truncated || projection.is_none() {
                    bounded_notice(failure.clone())
                } else {
                    let mut list = div().w_full().flex().flex_col();
                    if entries.is_empty() {
                        list = list.child(
                            div()
                                .text_color(rgb(tokens::SECONDARY))
                                .child(copy.toc_empty.clone()),
                        );
                    }
                    for entry in entries.iter() {
                        let target = entry.source.start;
                        let navigate_click = on_navigate.clone();
                        let navigate_key = on_navigate.clone();
                        let id = SharedString::from(format!("toc-{}-{target}", self.generation));
                        list = list.child(
                            div()
                                .id(id)
                                .focusable()
                                .tab_stop(true)
                                .role(Role::Link)
                                .cursor_pointer()
                                .pl(px(TOC_INDENT * f32::from(entry.level.saturating_sub(1))))
                                .py(px(2.0))
                                .hover(|style| style.text_color(rgb(tokens::ACCENT)))
                                .focus(|style| style.bg(rgba(tokens::ACCENT_SOFT)))
                                .on_click(move |_, window, cx| navigate_click(target, window, cx))
                                .on_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
                                    if event.keystroke.key == "enter"
                                        || event.keystroke.key == "space"
                                    {
                                        navigate_key(target, window, cx);
                                        cx.stop_propagation();
                                    }
                                })
                                .child(SharedString::from(entry.title.clone())),
                        );
                    }
                    div()
                        .w_full()
                        .bg(rgb(tokens::CODE_SURFACE))
                        .border_1()
                        .border_color(rgb(tokens::BORDER))
                        .rounded_md()
                        .px(px(20.0))
                        .py(px(16.0))
                        .text_size(px(TOC_FONT_SIZE))
                        .line_height(px(TOC_LINE_HEIGHT))
                        .child(list)
                        .into_any_element()
                }
            }
            Block::Rule => div()
                .w_full()
                .child(div().h(px(3.0)).w_full().bg(rgb(tokens::BORDER)))
                .into_any_element(),
            Block::FrontMatter => div()
                .w_full()
                .bg(rgb(tokens::CODE_SURFACE))
                .border_1()
                .border_color(rgb(tokens::BORDER))
                .rounded_md()
                .px(px(16.0))
                .py(px(12.0))
                .text_size(px(13.0))
                .text_color(rgb(tokens::SECONDARY))
                .child(copy.front_matter.clone())
                .into_any_element(),
        };
        let last_row = row + 1 == self.index.row_count(EditorMode::Visual);
        let left_padding = metrics.padding_x
            + if quote { QUOTE_PADDING } else { 0.0 }
            + LIST_INDENT * f32::from(list_depth);
        let content = div()
            .relative()
            .w_full()
            .max_w(px(metrics.column_width))
            .pr(px(metrics.padding_x))
            .pl(px(left_padding))
            .pt(px(if row == 0 { metrics.padding_top } else { top }))
            .pb(px(if last_row {
                metrics.padding_bottom
            } else {
                bottom
            }))
            .min_h(px(BODY_LINE_HEIGHT))
            .text_size(px(font_size))
            .line_height(px(line_height))
            .whitespace_normal()
            .when(quote, |content| {
                content
                    .border_l_4()
                    .border_color(rgb(alert.map_or(tokens::BORDER, alert_color)))
            })
            .when(heading.is_some_and(|level| level <= 2), |content| {
                content.border_b_1().border_color(rgb(tokens::BORDER))
            })
            .when_some(alert_badge, |content, kind| {
                content.child(
                    div()
                        .mb(px(8.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(alert_color(kind)))
                        .child(copy.alert(kind)),
                )
            })
            .child(block);
        let element_id =
            SharedString::from(format!("reader-row-{}-{source_start}", self.generation));
        let click_prepared = geometry.as_ref().map(|(_, prepared)| prepared.clone());
        let outer = div().w_full().flex().justify_center().child(content);
        let outer = self.measured_row(outer, row, token, geometry, text_bounds, document_id);
        let outer = outer.id(element_id);
        let outer = if let Some(target) = footnote_target {
            let navigate = on_navigate.clone();
            outer
                .focusable()
                .tab_stop(true)
                .role(Role::Link)
                .focus(|style| style.bg(rgba(tokens::ACCENT_SOFT)))
                .on_key_down(move |event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                        navigate(target, window, cx);
                        cx.stop_propagation();
                    }
                })
        } else {
            outer
        };
        let outer = if let (Some(target), Some(prepared)) = (hit, click_prepared) {
            let cache = self.heights.clone();
            let callback = on_hit.clone();
            outer.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                #[cfg(debug_assertions)]
                if let Ok(bounds) = prepared.bounds() {
                    eprintln!("m1-hit-layout point_x={} point_y={} bounds_x={} bounds_y={} bounds_w={} bounds_h={}", f32::from(event.position.x), f32::from(event.position.y), f32::from(bounds.origin.x), f32::from(bounds.origin.y), f32::from(bounds.size.width), f32::from(bounds.size.height));
                }
                let candidate = target
                    .ensure_layout(cache.borrow().token())
                    .and_then(|()| prepared.index_for_position(event.position));
                callback(&target, candidate, &prepared, window, cx);
            })
        } else {
            outer
        };
        if let Some(target) = fallback {
            let navigate = on_navigate.clone();
            outer
                .on_click(move |_, window, cx| navigate(target, window, cx))
                .into_any_element()
        } else {
            outer.into_any_element()
        }
    }

    /// 表格单元格：独立绘制、独立命中；投影窗口只覆盖该单元格的绘制范围。
    #[allow(clippy::too_many_arguments)]
    fn table_cell(
        &self,
        document: &mut ReadonlyDocument,
        token: Option<LayoutToken>,
        cell: &TableCell,
        head: bool,
        shaded: bool,
        last_cell: bool,
        last_row: bool,
        failure: &SharedString,
        on_hit: Rc<HitCallback>,
    ) -> AnyElement {
        let projection = cell_projection(document.parsed(), cell);
        let text = table_cell_text(projection.as_ref(), &cell.drawn, failure);
        let base = RunBase {
            weight: if head {
                FontWeight::SEMIBOLD
            } else {
                FontWeight::NORMAL
            },
            color: tokens::TEXT,
            family: self.font.family.clone(),
            fallbacks: self.font.fallbacks.clone(),
        };
        let runs = build_runs(&text, cell.drawn.start, &cell.leaves, &base);
        let styled = styled_text(text, runs);
        let prepared = PreparedLayout::new(styled.layout().clone());
        let mark_prepared = prepared.clone();
        let click_prepared = prepared.clone();
        let cache = self.heights.clone();
        let hit = token.and_then(|token| {
            projection
                .clone()
                .and_then(|projection| HitTarget::new(document, token, projection).ok())
        });
        let mut element = div()
            .flex_1()
            .min_w_0()
            .px(px(CELL_PADDING_X))
            .py(px(CELL_PADDING_Y))
            .text_align(table_align(cell.align))
            .text_size(px(TABLE_FONT_SIZE))
            .line_height(px(TABLE_LINE_HEIGHT))
            .border_color(rgb(tokens::BORDER))
            .when(!last_cell, |cell| cell.border_r_1())
            .when(!last_row, |cell| cell.border_b_1())
            .when(head || shaded, |cell| cell.bg(rgb(tokens::CODE_SURFACE)))
            .child(styled)
            .on_children_prepainted(move |_, _, _| mark_prepared.mark_prepainted());
        if let Some(target) = hit {
            element = element.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                let candidate = target
                    .ensure_layout(cache.borrow().token())
                    .and_then(|()| click_prepared.index_for_position(event.position));
                on_hit(&target, candidate, &click_prepared, window, cx);
            });
        }
        element.into_any_element()
    }

    /// 行级测量与延后锚点恢复；只有存在主文本布局的行参与字形级恢复。
    fn measured_row(
        &self,
        outer: Div,
        row: usize,
        token: Option<LayoutToken>,
        geometry: Option<(Projection, PreparedLayout)>,
        text_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
        document_id: DocumentId,
    ) -> Div {
        let cache = self.heights.clone();
        let measured = self.measured.clone();
        let restore_state = self.restores.clone();
        let restore_list = self.list.clone();
        let restore_alive = self.alive.clone();
        let restore_cache = self.heights.clone();
        let viewport_bounds = self.rail_bounds.clone();
        outer.on_children_prepainted(move |bounds, window, cx| {
            let Some(item_bounds) = bounds.first().copied() else {
                return;
            };
            let Some(token) = token else {
                return;
            };
            cache
                .borrow_mut()
                .record(token, row, f32::from(item_bounds.size.height));
            if let (Some((projection, prepared)), Some(text_bounds)) =
                (geometry.as_ref(), text_bounds.get())
            {
                let mut rows = measured.borrow_mut();
                if let Some(old) = rows.iter().position(|measured| measured.row == row) {
                    rows.remove(old);
                }
                if rows.len() == HEIGHT_CACHE_CAPACITY {
                    rows.pop_front();
                }
                rows.push_back(MeasuredRow {
                    row,
                    token,
                    projection: projection.clone(),
                    prepared: prepared.clone(),
                    bounds: text_bounds,
                });
            }
            let pending = restore_state
                .borrow()
                .pending()
                .filter(|pending| pending.row == row && pending.layout == token);
            let Some(pending) = pending else {
                return;
            };
            // 非文本行没有字形布局，按行首恢复；文本行仍优先字形级定位。
            let glyph = geometry
                .as_ref()
                .zip(text_bounds.get())
                .and_then(|((projection, prepared), _)| {
                    projection
                        .drawn_at(pending.anchor.source(), pending.anchor.affinity())
                        .ok()
                        .and_then(|local| {
                            prepared
                                .position_for_index(local.value().byte_offset())
                                .ok()
                                .flatten()
                        })
                });
            let offset = restore_offset(
                glyph,
                item_bounds.top(),
                pending.anchor.viewport_delta(),
            );
            if f32::from(offset).is_finite()
                && restore_state.borrow_mut().schedule_for_row(
                    &pending.request,
                    &document_id,
                    token,
                    row,
                )
            {
                let restore_state = restore_state.clone();
                let restore_list = restore_list.clone();
                let restore_document = document_id.clone();
                let restore_alive = restore_alive.clone();
                let restore_cache = restore_cache.clone();
                let viewport_bounds = viewport_bounds.clone();
                window.defer(cx, move |window, _| {
                    if !restore_alive.get() || restore_cache.borrow().token() != Some(token) {
                        return;
                    }
                    if restore_state
                        .borrow_mut()
                        .take_for_row(&pending.request, &restore_document, token, row)
                        .is_some()
                    {
                        restore_list.scroll_to(ListOffset {
                            item_ix: row,
                            offset_in_item: offset.max(px(0.0)),
                        });
                        #[cfg(debug_assertions)]
                        eprintln!("m1-anchor restored source={} row={} viewport_delta={} glyph_y={} viewport_y={}", pending.anchor.source().byte_offset(), row, pending.anchor.viewport_delta(), glyph.map_or(0.0, |glyph| f32::from(glyph.y)), viewport_bounds.get().map_or(0.0, |bounds| f32::from(bounds.top())));
                        #[cfg(not(debug_assertions))]
                        let _ = viewport_bounds;
                        window.refresh();
                    }
                });
            }
        })
    }

    pub(super) fn render(
        &self,
        render_row: impl FnMut(usize, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> AnyElement {
        let bounds = self.rail_bounds.clone();
        let rail_list = self.list.clone();
        let hint_width = self.hint_width.clone();
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
                                // 窗口宽度变化时 gpui list 会把所有行重建为无高度提示的
                                // 未测量行，而布局键可能不变（例如内容列已达 880 上限）。
                                // 这里按实际视口宽度补一次统一高度；绘制阶段排在同帧列表
                                // 之后，本轮滚动条与后续翻页直接使用恢复后的总高度。
                                let width = f32::from(window.viewport_size().width);
                                if hint_width.get() != Some(width) {
                                    hint_width.set(Some(width));
                                    rail_list
                                        .clone()
                                        .with_uniform_item_height(px(BODY_LINE_HEIGHT));
                                }
                                window.paint_quad(fill(rail, rgb(tokens::SCROLLBAR_RAIL)));
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
                                window.paint_quad(fill(thumb, rgb(tokens::SCROLLBAR_THUMB)));
                            },
                        )
                        .size_full(),
                    ),
            )
            .into_any_element()
    }
    pub(super) fn layout_token(&self) -> Option<LayoutToken> {
        self.heights.borrow().token()
    }
    /// 返回本帧已渲染的虚拟项数量；验证探针用它记录虚拟化规模。
    pub(super) fn rendered_item_count(&self) -> usize {
        self.rendered.get()
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
        eprintln!(
            "m1-scrollbar max_offset={} px_offset={} item_count={}",
            self.list.max_offset_for_scrollbar().y,
            self.list.scroll_px_offset_for_scrollbar().y,
            self.list.item_count()
        );
    }
}

/// 表格单元格投影：只覆盖该单元格的绘制范围，映射仍回到同一源码快照。
fn cell_projection(parsed: &Parsed, cell: &TableCell) -> Option<Projection> {
    if cell.drawn.is_empty() {
        return None;
    }
    let budget = MappingBudget::new(MAX_ITEM_BYTES, MAX_ITEM_BYTES.saturating_add(1));
    let drawn = parsed.projection().drawn_range(cell.drawn.clone()).ok()?;
    parsed.projection().window(&drawn, budget).ok()
}

/// 单元格文字：空单元格保持空白，有绘制范围却取窗口失败时显示预算反馈。
fn table_cell_text(
    projection: Option<&Projection>,
    drawn: &std::ops::Range<usize>,
    failure: &SharedString,
) -> SharedString {
    match projection {
        Some(projection) => SharedString::from(projection.drawn_text().to_owned()),
        None if drawn.is_empty() => SharedString::default(),
        None => failure.clone(),
    }
}

fn footnote_label(label: ContentLabel, copy: &ContentCopy) -> AnyElement {
    let text = match label {
        ContentLabel::Footnote => copy.footnote.clone(),
    };
    div()
        .flex_none()
        .pr(px(8.0))
        .text_color(rgb(tokens::SECONDARY))
        .child(text)
        .into_any_element()
}

fn marker_chip(marker: ListMarker) -> AnyElement {
    div()
        .flex_none()
        .w(px(MARKER_WIDTH))
        .pr(px(8.0))
        .text_align(TextAlign::Right)
        .text_color(rgb(tokens::SECONDARY))
        .child(SharedString::from(marker_text(marker)))
        .into_any_element()
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

#[cfg(test)]
mod tests {
    use std::ops::Range;

    use aline_markdown::AlertKind;
    use gpui::{FontWeight, SharedString};

    use super::super::readonly_content::InlineStyle;
    use super::*;

    fn leaf(drawn: Range<usize>, style: InlineStyle, kind: LeafKind) -> Leaf {
        Leaf { drawn, style, kind }
    }

    fn base() -> RunBase {
        RunBase {
            weight: FontWeight::NORMAL,
            color: tokens::TEXT,
            family: SharedString::from("TestFamily"),
            fallbacks: None,
        }
    }

    fn row(
        block: Block,
        heading: Option<u8>,
        quote: u16,
        list: u16,
        drawn: Option<Range<usize>>,
    ) -> ContentRow {
        ContentRow {
            source: 0..0,
            drawn,
            block,
            heading,
            quote_depth: quote,
            list_depth: list,
            marker: None,
            alert: None,
            alert_badge: false,
            gray: None,
            label: None,
            nodes: 1,
            columns: 0,
        }
    }

    #[test]
    fn runs_cover_every_byte_and_apply_leaf_styles() {
        let text = "ab code";
        let leaves = [
            leaf(
                0..2,
                InlineStyle {
                    bold: true,
                    ..InlineStyle::default()
                },
                LeafKind::Text,
            ),
            leaf(
                3..7,
                InlineStyle {
                    code: true,
                    ..InlineStyle::default()
                },
                LeafKind::Code,
            ),
        ];
        let runs = build_runs(text, 0, &leaves, &base());
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), text.len());
        assert_eq!(runs.len(), 3);
        assert_eq!(runs[0].font.weight, FontWeight::BOLD);
        assert_eq!(runs[1].font.weight, FontWeight::NORMAL);
        assert_eq!(runs[2].font.family, SharedString::from(CODE_FONT));
        assert!(runs[2].background_color.is_some());
        assert_eq!(runs[2].color, rgb(tokens::TEXT).into());
    }

    #[test]
    fn table_cell_without_projection_keeps_empty_cells_empty_and_reports_budget_failure() {
        let failure = SharedString::from("layout budget");
        assert_eq!(table_cell_text(None, &(0..65_537), &failure), failure);
        assert_eq!(
            table_cell_text(None, &(5..5), &failure),
            SharedString::default()
        );
    }

    #[test]
    fn runs_always_cover_text_even_with_malformed_leaves() {
        let leaves = [
            leaf(4..99, InlineStyle::default(), LeafKind::Text),
            leaf(1..2, InlineStyle::default(), LeafKind::Text),
        ];
        let runs = build_runs("abc", 0, &leaves, &base());
        assert_eq!(runs.iter().map(|run| run.len).sum::<usize>(), 3);
        let empty = build_runs("", 0, &leaves, &base());
        assert!(empty.is_empty());
    }

    #[test]
    fn link_and_mark_use_accent_and_mark_surface() {
        let leaves = [leaf(
            0..4,
            InlineStyle {
                link: true,
                mark: true,
                ..InlineStyle::default()
            },
            LeafKind::Text,
        )];
        let runs = build_runs("link", 0, &leaves, &base());
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].color, rgb(tokens::ACCENT).into());
        assert_eq!(
            runs[0].background_color,
            Some(rgb(tokens::MARK_SURFACE).into())
        );
        assert!(runs[0].underline.is_some());
    }

    #[test]
    fn heading_metrics_follow_prototype_scale() {
        assert_eq!(heading_metrics(1), (32.0, 43.2));
        assert_eq!(heading_metrics(2), (24.0, 32.4));
        assert_eq!(heading_metrics(6), (14.0, 18.9));
        assert!(heading_metrics(2).0 > heading_metrics(3).0);
    }

    #[test]
    fn block_spacing_separates_gap_list_quote_and_heading_rows() {
        let gap = row(Block::Text { leaves: Vec::new() }, None, 0, 0, None);
        assert_eq!(block_spacing(&gap), (0.0, 0.0));
        let item = row(Block::Text { leaves: Vec::new() }, None, 0, 1, Some(0..3));
        assert_eq!(block_spacing(&item), (2.0, 2.0));
        let quote = row(Block::Text { leaves: Vec::new() }, None, 1, 0, Some(0..3));
        assert_eq!(block_spacing(&quote), (10.0, 10.0));
        let heading = row(
            Block::Text { leaves: Vec::new() },
            Some(2),
            0,
            0,
            Some(0..3),
        );
        assert_eq!(block_spacing(&heading), (16.0, 17.0));
        assert_eq!(
            block_spacing(&row(Block::Rule, None, 0, 0, None)),
            (14.0, 14.0)
        );
    }

    #[test]
    fn gutter_is_only_built_for_projected_code_rows() {
        let lines = Vec::new();
        assert_eq!(visible_gutter(false, &lines), None);
        assert_eq!(visible_gutter(true, &lines), Some(String::new()));
    }

    #[test]
    fn restore_offset_falls_back_to_row_start_without_glyph() {
        // 非文本行没有字形：偏移取行首减视口差值，滚动时再夹到 0。
        assert_eq!(restore_offset(None, px(100.0), 30.0), px(-30.0));
        assert_eq!(
            restore_offset(Some(point(px(0.0), px(140.0))), px(100.0), 30.0),
            px(10.0)
        );
    }

    #[test]
    fn gutter_width_keeps_digits_outside_the_gap() {
        // 行号栏宽度含 12 px 右留白；数字区必须容得下最宽行号，
        // 否则两位数（如 10）会逐字折行成两行。
        assert_eq!(gutter_width("1"), 12.0 + 8.0 + 4.0);
        assert_eq!(gutter_width("9\n10"), 12.0 + 16.0 + 4.0);
        assert_eq!(gutter_width(""), 12.0 + 8.0 + 4.0);
        for text in ["1", "9\n10", "", "1\n123"] {
            let digits = text
                .rsplit('\n')
                .next()
                .map_or(1, |last| last.chars().count().max(1));
            assert!(
                gutter_width(text) - 12.0 >= 8.0 * digits as f32,
                "digits must fit after the 12 px gap: {text:?}"
            );
        }
    }

    #[test]
    fn marker_text_preserves_ordered_numbers() {
        assert_eq!(marker_text(ListMarker::Bullet), "•");
        assert_eq!(marker_text(ListMarker::Ordered(12)), "12.");
    }

    #[test]
    fn alert_colors_are_distinct() {
        let colors = [
            alert_color(AlertKind::Note),
            alert_color(AlertKind::Tip),
            alert_color(AlertKind::Important),
            alert_color(AlertKind::Warning),
            alert_color(AlertKind::Caution),
        ];
        for (index, color) in colors.iter().enumerate() {
            for other in colors.iter().skip(index + 1) {
                assert_ne!(color, other);
            }
        }
    }
}
