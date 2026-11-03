//! 只读命中只接受已布局的局部 UTF-8 候选；不建立编辑能力或字素移动协议。
//!
//! TextLayout 的候选属于实际绘制的局部文本。Projection 校验 UTF-8/身份并转换为
//! 源坐标；不把 index 当全文偏移，不搜索显示文字，也不对块首/行首做手算。

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;

use aline_buffer::{Affinity, SrcRange, SrcSelection};
use aline_markdown::{MappingPrecision, Projection};
use gpui::{Pixels, Point, TextLayout};

use super::readonly_layout::LayoutToken;
use crate::{InputIdentity, ReadonlyDocument};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum HitError {
    Unmeasured,
    InvalidPosition,
    InvalidOffset,
    StaleLayout,
    StaleInput,
    Navigation,
}
impl fmt::Display for HitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}
impl std::error::Error for HitError {}

#[derive(Clone)]
pub(super) struct PreparedLayout {
    layout: TextLayout,
    prepainted: Rc<Cell<bool>>,
}
impl PreparedLayout {
    pub(super) fn new(layout: TextLayout) -> Self {
        Self {
            layout,
            prepainted: Rc::new(Cell::new(false)),
        }
    }
    /// 只能在同一 StyledText 子节点完成 prepaint 后调用；不能由测量阶段置位。
    pub(super) fn mark_prepainted(&self) {
        self.prepainted.set(true);
    }
    /// point 与 TextLayout::prepaint 的 bounds 同属窗口坐标；锁定 API 在内部减行原点。
    /// 调用方不能先减 bounds.origin，否则会重复局部化。
    pub(super) fn index_for_position(
        &self,
        point: Point<Pixels>,
    ) -> Result<Result<usize, usize>, HitError> {
        self.ensure_prepainted()?;
        if !f32::from(point.x).is_finite() || !f32::from(point.y).is_finite() {
            return Err(HitError::InvalidPosition);
        }
        Ok(self.layout.index_for_position(point))
    }
    pub(super) fn position_for_index(
        &self,
        index: usize,
    ) -> Result<Option<Point<Pixels>>, HitError> {
        self.ensure_prepainted()?;
        Ok(self.layout.position_for_index(index))
    }
    #[cfg(debug_assertions)]
    pub(super) fn bounds(&self) -> Result<gpui::Bounds<Pixels>, HitError> {
        self.ensure_prepainted()?;
        Ok(self.layout.bounds())
    }
    #[cfg(debug_assertions)]
    fn text_matches(&self, expected: &str) -> Result<bool, HitError> {
        self.ensure_prepainted()?;
        Ok(self.layout.text() == expected)
    }
    fn ensure_prepainted(&self) -> Result<(), HitError> {
        if self.prepainted.get() {
            Ok(())
        } else {
            Err(HitError::Unmeasured)
        }
    }
}

pub(super) struct HitTarget {
    identity: InputIdentity,
    layout: LayoutToken,
    projection: Projection,
}
impl HitTarget {
    pub(super) fn new(
        document: &mut ReadonlyDocument,
        layout: LayoutToken,
        projection: Projection,
    ) -> Result<Self, HitError> {
        if projection.snapshot_id() != document.parsed().snapshot().id() {
            return Err(HitError::StaleInput);
        }
        let identity = document
            .input_identity()
            .map_err(|_| HitError::StaleInput)?;
        if identity.snapshot_id() != projection.snapshot_id() {
            return Err(HitError::StaleInput);
        }
        Ok(Self {
            identity,
            layout,
            projection,
        })
    }
    pub(super) fn ensure_layout(&self, current: Option<LayoutToken>) -> Result<(), HitError> {
        if current == Some(self.layout) {
            Ok(())
        } else {
            Err(HitError::StaleLayout)
        }
    }
    #[cfg(debug_assertions)]
    pub(super) fn layout_text_matches(&self, layout: &PreparedLayout) -> Result<bool, HitError> {
        layout.text_matches(self.projection.drawn_text())
    }
    pub(super) fn navigate_candidate(
        &self,
        document: &mut ReadonlyDocument,
        current_layout: Option<LayoutToken>,
        candidate: Result<usize, usize>,
    ) -> Result<HitOutcome, HitError> {
        // TextLayout 给出的 glyph 起点偏向来源之前；绘制末端必须偏向之后，
        // 否则仅含一个 Aligned 实体的行在末端会回到实体源码起点。
        let local_drawn = match candidate {
            Ok(local_drawn) | Err(local_drawn) => local_drawn,
        };
        let affinity = if local_drawn == self.projection.drawn_text().len() {
            Affinity::After
        } else {
            Affinity::Before
        };
        self.navigate(document, current_layout, candidate, affinity)
    }
    pub(super) fn navigate(
        &self,
        document: &mut ReadonlyDocument,
        current_layout: Option<LayoutToken>,
        candidate: Result<usize, usize>,
        affinity: Affinity,
    ) -> Result<HitOutcome, HitError> {
        self.ensure_layout(current_layout)?;
        let current = document
            .input_identity()
            .map_err(|_| HitError::StaleInput)?;
        self.identity
            .validate(&current)
            .map_err(|_| HitError::StaleInput)?;
        let (local_drawn, edge) = match candidate {
            Ok(local_drawn) => (local_drawn, false),
            Err(local_drawn) => (local_drawn, true),
        };
        let drawn = self
            .projection
            .drawn_offset(local_drawn)
            .map_err(|_| HitError::InvalidOffset)?;
        let mapped = self
            .projection
            .source_at(&drawn, affinity)
            .map_err(|_| HitError::InvalidOffset)?;
        let selection = SrcSelection::new(mapped.value().clone(), mapped.value().clone())
            .map_err(|_| HitError::InvalidOffset)?;
        document
            .navigate(&self.identity, &selection)
            .map_err(|_| HitError::Navigation)?;
        Ok(HitOutcome {
            source: mapped.source_range().clone(),
            precision: mapped.precision(),
            edge,
        })
    }
}

pub(super) struct HitOutcome {
    source: SrcRange,
    precision: MappingPrecision,
    edge: bool,
}
impl HitOutcome {
    pub(super) fn source_range(&self) -> &SrcRange {
        &self.source
    }
    pub(super) fn precision(&self) -> MappingPrecision {
        self.precision
    }
    pub(super) fn is_edge_candidate(&self) -> bool {
        self.edge
    }
}
