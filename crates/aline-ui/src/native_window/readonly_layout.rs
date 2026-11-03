//! 虚拟阅读的纯范围索引、布局身份、有界布局预算与延后锚点恢复。

use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;

use crate::{DocumentId, EditorMode, ReadonlyDocument, WorkingVersion};
use aline_buffer::{Affinity, LineIndex, LineIndexBudget, SnapshotId, SrcOffset, SrcRange};
use aline_markdown::{
    DrawnRange, MappingBudget, Parsed, PresentationKind, PresentationNode, Projection, ProjectionId,
};

const SOURCE_FRAGMENT_BYTES: usize = 4096;
const DEFAULT_SOURCE_THRESHOLD: usize = 4 * 1024 * 1024;

pub(super) fn initial_view(source_bytes: usize) -> (EditorMode, Option<&'static str>) {
    if source_bytes > DEFAULT_SOURCE_THRESHOLD {
        (EditorMode::Source, Some("reading.large_source"))
    } else {
        (EditorMode::Visual, None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LayoutError {
    Budget,
    ItemBytes { needed: usize, limit: usize },
    ItemNodes { needed: usize, limit: usize },
    TableColumns { needed: usize, limit: usize },
    InvalidEnvironment,
    InvalidRange,
    StaleInput,
    GenerationExhausted,
}
impl fmt::Display for LayoutError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}
impl std::error::Error for LayoutError {}

#[derive(Clone, Copy)]
pub(super) struct ItemLayoutBudget {
    bytes: usize,
    nodes: usize,
    columns: usize,
}
impl ItemLayoutBudget {
    pub(super) const fn new(bytes: usize, nodes: usize, columns: usize) -> Self {
        Self {
            bytes,
            nodes,
            columns,
        }
    }
}
struct VisualRow {
    source: SrcRange,
    drawn: Option<DrawnRange>,
    nodes: usize,
    columns: usize,
}

pub(super) struct ReaderIndex {
    snapshot: SnapshotId,
    source: Vec<SrcRange>,
    visual: Vec<VisualRow>,
}
impl ReaderIndex {
    pub(super) fn new(document: &ReadonlyDocument, max_rows: usize) -> Result<Self, LayoutError> {
        let parsed = document.parsed();
        let snapshot = parsed.snapshot();
        let text = document.baseline().source();
        let lines = LineIndex::build(text, LineIndexBudget::new(max_rows))
            .map_err(|_| LayoutError::Budget)?;
        let mut source = Vec::new();
        for line in 0..lines.line_count() {
            let range = lines.line_range(line).ok_or(LayoutError::InvalidRange)?;
            let mut src_start = range.start;
            loop {
                let mut src_end = src_start
                    .saturating_add(SOURCE_FRAGMENT_BYTES)
                    .min(range.end);
                while !text.is_char_boundary(src_end) {
                    src_end = src_end.saturating_sub(1);
                }
                if src_end < src_start || (src_end == src_start && src_start != range.end) {
                    return Err(LayoutError::InvalidRange);
                }
                if source.len() >= max_rows {
                    return Err(LayoutError::Budget);
                }
                source.try_reserve(1).map_err(|_| LayoutError::Budget)?;
                source.push(
                    snapshot
                        .src_range(src_start..src_end)
                        .map_err(|_| LayoutError::InvalidRange)?,
                );
                if src_end == range.end {
                    break;
                }
                src_start = src_end;
            }
        }
        let mut visual = Vec::new();
        let mut source_end = 0;
        for node in parsed.presentation() {
            let range = node.source_range().byte_range();
            if range.start < source_end {
                return Err(LayoutError::InvalidRange);
            }
            if range.start > source_end {
                push_row(
                    &mut visual,
                    max_rows,
                    VisualRow {
                        source: snapshot
                            .src_range(source_end..range.start)
                            .map_err(|_| LayoutError::InvalidRange)?,
                        drawn: None,
                        nodes: 1,
                        columns: 0,
                    },
                )?;
            }
            let (nodes, columns) = node_cost(node)?;
            push_row(
                &mut visual,
                max_rows,
                VisualRow {
                    source: node.source_range().clone(),
                    drawn: Some(node.drawn_range().clone()),
                    nodes,
                    columns,
                },
            )?;
            source_end = range.end;
        }
        if source_end < snapshot.len_bytes() || visual.is_empty() {
            push_row(
                &mut visual,
                max_rows,
                VisualRow {
                    source: snapshot
                        .src_range(source_end..snapshot.len_bytes())
                        .map_err(|_| LayoutError::InvalidRange)?,
                    drawn: None,
                    nodes: 1,
                    columns: 0,
                },
            )?;
        }
        Ok(Self {
            snapshot: snapshot.id().clone(),
            source,
            visual,
        })
    }
    pub(super) fn row_count(&self, mode: EditorMode) -> usize {
        match mode {
            EditorMode::Visual => self.visual.len(),
            EditorMode::Source => self.source.len(),
        }
    }
    pub(super) fn source_range(
        &self,
        parsed: &Parsed,
        mode: EditorMode,
        row: usize,
    ) -> Result<SrcRange, LayoutError> {
        self.check(parsed)?;
        match mode {
            EditorMode::Visual => self
                .visual
                .get(row)
                .map(|row| row.source.clone())
                .ok_or(LayoutError::InvalidRange),
            EditorMode::Source => self
                .source
                .get(row)
                .cloned()
                .ok_or(LayoutError::InvalidRange),
        }
    }
    /// 只读范围/计数预检；在局部 Projection 分配和 GPUI shape 之前调用。
    pub(super) fn preflight(
        &self,
        parsed: &Parsed,
        mode: EditorMode,
        row: usize,
        budget: ItemLayoutBudget,
    ) -> Result<(), LayoutError> {
        let source = self.source_range(parsed, mode, row)?;
        let visual = if mode == EditorMode::Visual {
            self.visual.get(row)
        } else {
            None
        };
        let bytes = visual
            .and_then(|row| row.drawn.as_ref())
            .filter(|drawn| !drawn.byte_range().is_empty())
            .map_or(source.len_bytes(), |drawn| drawn.byte_range().len());
        if bytes > budget.bytes {
            return Err(LayoutError::ItemBytes {
                needed: bytes,
                limit: budget.bytes,
            });
        }
        let nodes = visual.map_or(1, |row| row.nodes);
        if nodes > budget.nodes {
            return Err(LayoutError::ItemNodes {
                needed: nodes,
                limit: budget.nodes,
            });
        }
        let columns = visual.map_or(0, |row| row.columns);
        if columns > budget.columns {
            return Err(LayoutError::TableColumns {
                needed: columns,
                limit: budget.columns,
            });
        }
        Ok(())
    }
    pub(super) fn projection(
        &self,
        parsed: &Parsed,
        mode: EditorMode,
        row: usize,
        max_bytes: usize,
    ) -> Result<Projection, LayoutError> {
        self.preflight(
            parsed,
            mode,
            row,
            ItemLayoutBudget::new(max_bytes, 4096, 256),
        )?;
        let source = self.source_range(parsed, mode, row)?;
        let budget = MappingBudget::new(max_bytes, max_bytes.saturating_add(1));
        if mode == EditorMode::Visual
            && let Some(drawn) = self
                .visual
                .get(row)
                .and_then(|row| row.drawn.as_ref())
                .filter(|drawn| !drawn.byte_range().is_empty())
        {
            return parsed
                .projection()
                .window(drawn, budget)
                .map_err(|_| LayoutError::Budget);
        }
        Projection::identity(parsed.snapshot().clone(), source, budget)
            .map_err(|_| LayoutError::Budget)
    }
    pub(super) fn anchor(
        &self,
        parsed: &Parsed,
        mode: EditorMode,
        row: usize,
        viewport_delta: f32,
    ) -> Result<SourceAnchor, LayoutError> {
        let source = self.source_range(parsed, mode, row)?;
        SourceAnchor::new(
            parsed,
            parsed
                .snapshot()
                .src_offset(source.byte_range().start)
                .map_err(|_| LayoutError::InvalidRange)?,
            Affinity::After,
            viewport_delta,
        )
    }
    pub(super) fn anchor_from_local(
        &self,
        parsed: &Parsed,
        projection: &Projection,
        local_drawn: usize,
        affinity: Affinity,
        viewport_delta: f32,
    ) -> Result<SourceAnchor, LayoutError> {
        self.check(parsed)?;
        if projection.snapshot_id() != parsed.snapshot().id() {
            return Err(LayoutError::StaleInput);
        }
        let drawn = projection
            .drawn_offset(local_drawn)
            .map_err(|_| LayoutError::InvalidRange)?;
        let mapped = projection
            .source_at(&drawn, affinity)
            .map_err(|_| LayoutError::InvalidRange)?;
        SourceAnchor::new(parsed, mapped.into_value(), affinity, viewport_delta)
    }
    pub(super) fn row_for_anchor(
        &self,
        parsed: &Parsed,
        mode: EditorMode,
        anchor: &SourceAnchor,
    ) -> Result<usize, LayoutError> {
        self.check(parsed)?;
        parsed
            .snapshot()
            .validate_offset(&anchor.source)
            .map_err(|_| LayoutError::StaleInput)?;
        let src_off = anchor.source.byte_offset();
        let after = match mode {
            EditorMode::Visual => self
                .visual
                .partition_point(|row| row.source.byte_range().start <= src_off),
            EditorMode::Source => self
                .source
                .partition_point(|range| range.byte_range().start <= src_off),
        };
        let range_at = |row: usize| match mode {
            EditorMode::Visual => self.visual.get(row).map(|row| &row.source),
            EditorMode::Source => self.source.get(row),
        };
        // 这里只比较同一源码坐标；目标局部绘制位置仍由 Projection::drawn_at 计算。
        let mut row = after.saturating_sub(1);
        if anchor.affinity == Affinity::Before
            && row > 0
            && range_at(row).is_some_and(|range| range.byte_range().start == src_off)
            && range_at(row - 1).is_some_and(|range| range.byte_range().end == src_off)
        {
            row -= 1;
        }
        let range = range_at(row).ok_or(LayoutError::InvalidRange)?.byte_range();
        if src_off < range.start || src_off > range.end {
            return Err(LayoutError::InvalidRange);
        }
        Ok(row)
    }
    fn check(&self, parsed: &Parsed) -> Result<(), LayoutError> {
        if &self.snapshot == parsed.snapshot().id() {
            Ok(())
        } else {
            Err(LayoutError::StaleInput)
        }
    }
}
fn push_row(rows: &mut Vec<VisualRow>, max_rows: usize, row: VisualRow) -> Result<(), LayoutError> {
    if rows.len() >= max_rows {
        return Err(LayoutError::Budget);
    }
    rows.try_reserve(1).map_err(|_| LayoutError::Budget)?;
    rows.push(row);
    Ok(())
}
fn node_cost(node: &PresentationNode) -> Result<(usize, usize), LayoutError> {
    let mut nodes = 1usize;
    let mut columns = match node.kind() {
        PresentationKind::Table { alignments } => alignments.len(),
        _ => 0,
    };
    for child in node.children() {
        let (child_nodes, child_columns) = node_cost(child)?;
        nodes = nodes.checked_add(child_nodes).ok_or(LayoutError::Budget)?;
        columns = columns.max(child_columns);
    }
    Ok((nodes, columns))
}

#[derive(Clone)]
pub(super) struct SourceAnchor {
    source: SrcOffset,
    affinity: Affinity,
    viewport_delta: f32,
}
impl SourceAnchor {
    fn new(
        parsed: &Parsed,
        source: SrcOffset,
        affinity: Affinity,
        viewport_delta: f32,
    ) -> Result<Self, LayoutError> {
        parsed
            .snapshot()
            .validate_offset(&source)
            .map_err(|_| LayoutError::StaleInput)?;
        if !viewport_delta.is_finite() {
            return Err(LayoutError::InvalidEnvironment);
        }
        Ok(Self {
            source,
            affinity,
            viewport_delta,
        })
    }
    pub(super) fn source(&self) -> &SrcOffset {
        &self.source
    }
    pub(super) fn affinity(&self) -> Affinity {
        self.affinity
    }
    pub(super) fn viewport_delta(&self) -> f32 {
        self.viewport_delta
    }
}

#[derive(Clone)]
pub(super) struct RestoreRequest {
    owner: Rc<()>,
    generation: u64,
}
impl PartialEq for RestoreRequest {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.owner, &other.owner) && self.generation == other.generation
    }
}
impl Eq for RestoreRequest {}
#[derive(Clone)]
pub(super) struct RestoreTarget {
    pub(super) request: RestoreRequest,
    pub(super) anchor: SourceAnchor,
    pub(super) row: usize,
    pub(super) layout: LayoutToken,
    scheduled: bool,
}
pub(super) struct RestoreState {
    owner: Rc<()>,
    document: DocumentId,
    snapshot: SnapshotId,
    generation: u64,
    pending: Option<RestoreTarget>,
}
impl RestoreState {
    pub(super) fn new(document: DocumentId, snapshot: SnapshotId) -> Self {
        Self {
            owner: Rc::new(()),
            document,
            snapshot,
            generation: 0,
            pending: None,
        }
    }
    pub(super) fn begin(
        &mut self,
        anchor: SourceAnchor,
        row: usize,
        layout: LayoutToken,
    ) -> Result<RestoreRequest, LayoutError> {
        if anchor.source.snapshot_id() != &self.snapshot {
            return Err(LayoutError::StaleInput);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(LayoutError::GenerationExhausted)?;
        let request = RestoreRequest {
            owner: self.owner.clone(),
            generation,
        };
        self.generation = generation;
        self.pending = Some(RestoreTarget {
            request: request.clone(),
            anchor,
            row,
            layout,
            scheduled: false,
        });
        Ok(request)
    }
    pub(super) fn pending(&self) -> Option<RestoreTarget> {
        self.pending.clone()
    }
    pub(super) fn cancel(&mut self, request: &RestoreRequest) -> bool {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| &pending.request == request)
        {
            self.pending = None;
            true
        } else {
            false
        }
    }
    pub(super) fn cancel_current(&mut self) -> bool {
        self.pending.take().is_some()
    }
    pub(super) fn schedule_for_row(
        &mut self,
        request: &RestoreRequest,
        document: &DocumentId,
        layout: LayoutToken,
        row: usize,
    ) -> bool {
        let Some(pending) = self.pending.as_mut() else {
            return false;
        };
        if document != &self.document
            || &pending.request != request
            || pending.layout != layout
            || pending.row != row
            || pending.scheduled
        {
            return false;
        }
        pending.scheduled = true;
        true
    }
    pub(super) fn take_for_row(
        &mut self,
        request: &RestoreRequest,
        document: &DocumentId,
        layout: LayoutToken,
        row: usize,
    ) -> Option<SourceAnchor> {
        let pending = self.pending.as_ref()?;
        if document != &self.document
            || &pending.request != request
            || pending.layout != layout
            || pending.row != row
        {
            return None;
        }
        self.pending.take().map(|pending| pending.anchor)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LayoutEnvironment {
    content_width: u32,
    scale: u32,
    font_generation: u64,
    wrap: bool,
    font_size: u32,
    line_height: u32,
}
impl LayoutEnvironment {
    pub(super) fn new(
        content_width: f32,
        scale: f32,
        font_generation: u64,
        wrap: bool,
        font_size: f32,
        line_height: f32,
    ) -> Result<Self, LayoutError> {
        if [content_width, scale, font_size, line_height]
            .into_iter()
            .any(|value| !value.is_finite() || value <= 0.0)
        {
            return Err(LayoutError::InvalidEnvironment);
        }
        Ok(Self {
            content_width: content_width.to_bits(),
            scale: scale.to_bits(),
            font_generation,
            wrap,
            font_size: font_size.to_bits(),
            line_height: line_height.to_bits(),
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct LayoutKey {
    document: DocumentId,
    version: WorkingVersion,
    snapshot: SnapshotId,
    projection: ProjectionId,
    config: aline_markdown::ParseConfig,
    mode: EditorMode,
    environment: LayoutEnvironment,
}
impl LayoutKey {
    pub(super) fn new(
        document: &ReadonlyDocument,
        mode: EditorMode,
        environment: LayoutEnvironment,
    ) -> Self {
        Self {
            document: document.core().document_id().clone(),
            version: document.core().version(),
            snapshot: document.parsed().snapshot().id().clone(),
            projection: document.parsed().projection().id().clone(),
            config: document.parsed().config(),
            mode,
            environment,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct LayoutToken(u64);

pub(super) struct LayoutCache {
    key: Option<LayoutKey>,
    generation: u64,
    capacity: usize,
    heights: VecDeque<(usize, f32)>,
}
impl LayoutCache {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            key: None,
            generation: 0,
            capacity,
            heights: VecDeque::new(),
        }
    }
    pub(super) fn matches_key(&self, key: &LayoutKey) -> bool {
        self.key.as_ref() == Some(key)
    }
    pub(super) fn prepare(&mut self, key: LayoutKey) -> Result<bool, LayoutError> {
        if self.key.as_ref() == Some(&key) {
            return Ok(false);
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or(LayoutError::GenerationExhausted)?;
        self.key = Some(key);
        self.generation = generation;
        self.heights.clear();
        Ok(true)
    }
    pub(super) fn token(&self) -> Option<LayoutToken> {
        self.key.as_ref().map(|_| LayoutToken(self.generation))
    }
    pub(super) fn record(&mut self, token: LayoutToken, row: usize, height: f32) -> bool {
        if self.token() != Some(token) || self.capacity == 0 || !height.is_finite() || height <= 0.0
        {
            return false;
        }
        if let Some(entry) = self
            .heights
            .iter_mut()
            .find(|(cached_row, _)| *cached_row == row)
        {
            let changed = entry.1 != height;
            entry.1 = height;
            return changed;
        }
        if self.heights.len() == self.capacity {
            self.heights.pop_front();
        }
        self.heights.push_back((row, height));
        true
    }
    #[cfg(any(test, debug_assertions))]
    pub(super) fn height(&self, row: usize) -> Option<f32> {
        self.heights
            .iter()
            .find(|(cached_row, _)| *cached_row == row)
            .map(|entry| entry.1)
    }
    #[cfg(any(test, debug_assertions))]
    pub(super) fn len(&self) -> usize {
        self.heights.len()
    }
}
