//! 虚拟阅读的纯范围索引、布局身份、有界布局预算与延后锚点恢复。

use std::collections::VecDeque;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;

use super::readonly_content::{Block, ContentError, ContentIndex, ContentRow};
use crate::{DocumentId, EditorMode, ReadonlyDocument, WorkingVersion};
use aline_buffer::{Affinity, LineIndex, LineIndexBudget, SnapshotId, SrcOffset, SrcRange};
use aline_markdown::{MappingBudget, Parsed, Projection, ProjectionId};

const SOURCE_FRAGMENT_BYTES: usize = 4096;
const DEFAULT_SOURCE_THRESHOLD: usize = 4 * 1024 * 1024;

pub(super) fn initial_view(source_bytes: usize) -> (EditorMode, Option<&'static str>) {
    if source_bytes > DEFAULT_SOURCE_THRESHOLD {
        (EditorMode::Source, Some("reading.large_source"))
    } else {
        (EditorMode::Visual, None)
    }
}

/// Visual 行的有效绘制范围；代码正文的终止换行不构成额外可见行。
///
/// 隐藏结构、缺口行和空代码块没有可见文字，返回 `None`，由空投影保留
/// “点击落到行首”的只读命中语义，但不得把源码回退成正文。
fn visual_drawn(parsed: &Parsed, row: &ContentRow) -> Option<Range<usize>> {
    let drawn = row.drawn.as_ref()?;
    if drawn.is_empty() {
        return None;
    }
    let mut end = drawn.end;
    if matches!(&row.block, Block::Code { .. }) {
        let text = parsed.projection().drawn_text();
        if text.as_bytes().get(end - 1) == Some(&b'\n') {
            end -= 1;
            if end > drawn.start && text.as_bytes().get(end - 1) == Some(&b'\r') {
                end -= 1;
            }
        }
    }
    (end > drawn.start).then_some(drawn.start..end)
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
pub(super) struct ReaderIndex {
    snapshot: SnapshotId,
    source: Vec<SrcRange>,
    visual: ContentIndex,
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
        let visual = ContentIndex::new(parsed, max_rows).map_err(content_error)?;
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
    /// 返回可视化行计划；行号来自同一 `ReaderIndex` 的虚拟顺序。
    pub(super) fn content_row(&self, row: usize) -> Option<&ContentRow> {
        self.visual.row(row)
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
                .row(row)
                .map(|row| snapshot_range(parsed, row.source.clone()))
                .transpose()?
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
            self.visual.row(row)
        } else {
            None
        };
        let bytes = if mode == EditorMode::Visual {
            visual
                .and_then(|row| visual_drawn(parsed, row))
                .map_or(0, |drawn| drawn.len())
        } else {
            source.len_bytes()
        };
        if bytes > budget.bytes {
            return Err(LayoutError::ItemBytes {
                needed: bytes,
                limit: budget.bytes,
            });
        }
        // 列预算先于节点预算：超宽表格的具体原因应报 TableColumns，
        // 而不是被截断行的节点哨兵归为 ItemNodes。
        let columns = visual.map_or(0, |row| row.columns);
        if columns > budget.columns {
            return Err(LayoutError::TableColumns {
                needed: columns,
                limit: budget.columns,
            });
        }
        let nodes = visual.map_or(1, |row| row.nodes);
        if nodes > budget.nodes {
            return Err(LayoutError::ItemNodes {
                needed: nodes,
                limit: budget.nodes,
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
        if mode == EditorMode::Visual {
            if let Some(drawn) = self
                .visual
                .row(row)
                .and_then(|row| visual_drawn(parsed, row))
            {
                return parsed
                    .projection()
                    .drawn_range(drawn)
                    .map_err(|_| LayoutError::InvalidRange)
                    .and_then(|drawn| {
                        parsed
                            .projection()
                            .window(&drawn, budget)
                            .map_err(|_| LayoutError::Budget)
                    });
            }
            // 无可见文字的行用空绘制投影：正文为空，命中仍经同一映射落到行首。
            return Projection::from_segments(
                parsed.snapshot().clone(),
                source,
                String::new(),
                Vec::new(),
                budget,
            )
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
        let count = self.row_count(mode);
        let after = match mode {
            EditorMode::Visual => self
                .visual
                .rows()
                .partition_point(|row| row.source.start <= src_off),
            EditorMode::Source => self
                .source
                .partition_point(|range| range.byte_range().start <= src_off),
        };
        let range_at = |row: usize| -> Option<Range<usize>> {
            match mode {
                EditorMode::Visual => self.visual.row(row).map(|row| row.source.clone()),
                EditorMode::Source => self.source.get(row).map(|range| range.byte_range()),
            }
        };
        // 这里只比较同一源码坐标；目标局部绘制位置仍由 Projection::drawn_at 计算。
        let mut row = after.saturating_sub(1);
        if anchor.affinity == Affinity::Before
            && row > 0
            && range_at(row).is_some_and(|range| range.start == src_off)
            && range_at(row - 1).is_some_and(|range| range.end == src_off)
        {
            row -= 1;
        }
        // 锚点可以落在块间空白或隐藏标记上；前进到下一行，末行保持可恢复。
        if range_at(row).is_some_and(|range| src_off > range.end) && row + 1 < count {
            row += 1;
        }
        if range_at(row).is_none() {
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
fn content_error(error: ContentError) -> LayoutError {
    match error {
        ContentError::Budget => LayoutError::Budget,
        ContentError::InvalidRange => LayoutError::InvalidRange,
    }
}

fn snapshot_range(parsed: &Parsed, range: Range<usize>) -> Result<SrcRange, LayoutError> {
    parsed
        .snapshot()
        .src_range(range)
        .map_err(|_| LayoutError::InvalidRange)
}

#[derive(Clone)]
pub(super) struct SourceAnchor {
    source: SrcOffset,
    affinity: Affinity,
    viewport_delta: f32,
}
impl SourceAnchor {
    pub(super) fn new(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DocLimits, ReadonlyDocument, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenLimits,
        ReadonlySession,
    };
    use std::error::Error;
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};

    use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
    use aline_markdown::{ParseBudget, ParseConfig};
    use aline_source::StorageBudget;

    struct Files<'a>(&'a [u8]);
    impl<'a> ReadonlyFileAccess for Files<'a> {
        type Reader = Cursor<&'a [u8]>;
        fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(
                Cursor::new(self.0),
                self.0.len() as u64,
                true,
            ))
        }
    }
    fn session(bytes: &[u8]) -> Result<ReadonlySession, Box<dyn Error>> {
        let max = 1024 * 1024;
        let mut session = ReadonlySession::new(ReadonlyOpenLimits::new(
            StorageBudget::new(max),
            DocLimits::new(
                TextBudget::new(max),
                LineIndexBudget::new(100_000),
                HistoryBudget::new(0, 0),
            ),
            ParseConfig::m0(),
            ParseBudget::m0(),
        ));
        let request = session.request_open()?;
        let job = session
            .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
            .ok_or("job")?;
        session.accept_open(job.read_with(&Files(bytes)));
        Ok(session)
    }
    fn environment(width: f32) -> Result<LayoutEnvironment, Box<dyn Error>> {
        Ok(LayoutEnvironment::new(width, 2.0, 1, true, 16.0, 24.0)?)
    }

    #[test]
    fn source_rows_keep_crlf_isolated_cr_and_terminal_empty_line() -> Result<(), Box<dyn Error>> {
        let session = session("中\r\n\tlast\rpart\n".as_bytes())?;
        let document = session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100)?;
        assert_eq!(index.row_count(EditorMode::Source), 3);
        let expected = [(0..3, "中"), (5..15, "\tlast\rpart"), (16..16, "")];
        for (row, (source, text)) in expected.into_iter().enumerate() {
            assert_eq!(
                index
                    .source_range(document.parsed(), EditorMode::Source, row)?
                    .byte_range(),
                source
            );
            assert_eq!(
                index
                    .projection(document.parsed(), EditorMode::Source, row, 4096)?
                    .drawn_text(),
                text
            );
        }
        Ok(())
    }

    #[test]
    fn visual_rows_without_visible_text_project_empty_text_and_keep_source_start()
    -> Result<(), Box<dyn Error>> {
        let session = session(b"<!-- hidden -->\n\nword\n\n```\n```\n")?;
        let document = session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100)?;
        let mut hidden = false;
        let mut gap = false;
        let mut empty_code = false;
        for row in 0..index.row_count(EditorMode::Visual) {
            let content = index.content_row(row).ok_or("row")?;
            let projection =
                index.projection(document.parsed(), EditorMode::Visual, row, 65_536)?;
            match &content.block {
                Block::Code { lines, .. }
                    if content.drawn.as_ref().is_some_and(|drawn| drawn.is_empty()) =>
                {
                    empty_code = true;
                    assert_eq!(lines.len(), 1);
                    assert_eq!(projection.drawn_text(), "");
                }
                Block::Text { .. } if content.drawn.is_none() => {
                    if content.source.len() > 1 {
                        hidden = true;
                    } else {
                        gap = true;
                    }
                    assert_eq!(projection.drawn_text(), "");
                }
                _ => {}
            }
            if projection.drawn_text().is_empty() {
                let mapped = projection
                    .source_at(&projection.drawn_offset(0)?, aline_buffer::Affinity::After)?;
                assert_eq!(mapped.value().byte_offset(), content.source.start);
            }
        }
        assert!(hidden && gap && empty_code);
        Ok(())
    }

    #[test]
    fn code_rows_trim_terminal_line_break_from_visual_projection() -> Result<(), Box<dyn Error>> {
        let first = session(b"```rust\none\n\nthree\n```\n")?;
        let document = first.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100)?;
        let content = index.content_row(0).ok_or("row")?;
        let Block::Code { lines, .. } = &content.block else {
            return Err("expected code row".into());
        };
        assert_eq!(lines.len(), 3);
        assert_eq!(
            index
                .projection(document.parsed(), EditorMode::Visual, 0, 65_536)?
                .drawn_text(),
            "one\n\nthree"
        );
        let blank = session(b"```\n\n```\n")?;
        let document = blank.document().ok_or("blank document")?;
        let index = ReaderIndex::new(document, 100)?;
        let content = index.content_row(0).ok_or("blank row")?;
        let Block::Code { lines, .. } = &content.block else {
            return Err("expected blank code row".into());
        };
        assert_eq!(lines.len(), 1);
        assert_eq!(
            index
                .projection(document.parsed(), EditorMode::Visual, 0, 65_536)?
                .drawn_text(),
            ""
        );
        Ok(())
    }

    #[test]
    fn visual_rows_partition_source_and_reveal_source_anchor_after_reflow()
    -> Result<(), Box<dyn Error>> {
        let session = session(b"# Head\n\nfirst\n\nlast")?;
        let document = session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100)?;
        let count = index.row_count(EditorMode::Visual);
        assert!(count >= 3);
        let mut source_end = 0;
        for row in 0..count {
            let range = index
                .source_range(document.parsed(), EditorMode::Visual, row)?
                .byte_range();
            assert_eq!(range.start, source_end);
            source_end = range.end;
        }
        assert_eq!(source_end, 19);
        let anchor = index.anchor(document.parsed(), EditorMode::Source, 4, 7.0)?;
        let visual_row = index.row_for_anchor(document.parsed(), EditorMode::Visual, &anchor)?;
        assert_eq!(
            index
                .projection(document.parsed(), EditorMode::Visual, visual_row, 4096)?
                .drawn_text(),
            "last"
        );
        assert_eq!(anchor.viewport_delta(), 7.0);
        Ok(())
    }

    #[test]
    fn width_scale_font_wrap_and_projection_identity_invalidate_heights()
    -> Result<(), Box<dyn Error>> {
        let a = session(b"same")?;
        let b = session(b"same")?;
        let a = a.document().ok_or("a")?;
        let b = b.document().ok_or("b")?;
        let mut cache = LayoutCache::new(2);
        let key = LayoutKey::new(a, EditorMode::Visual, environment(600.0)?);
        assert!(cache.prepare(key.clone())?);
        let token = cache.token().ok_or("token")?;
        assert!(cache.record(token, 0, 48.0));
        assert!(cache.matches_key(&key));
        assert!(!cache.prepare(key)?);
        assert_eq!(cache.height(0), Some(48.0));
        for key in [
            LayoutKey::new(a, EditorMode::Visual, environment(400.0)?),
            LayoutKey::new(
                a,
                EditorMode::Visual,
                LayoutEnvironment::new(400.0, 1.0, 1, true, 16.0, 24.0)?,
            ),
            LayoutKey::new(
                a,
                EditorMode::Visual,
                LayoutEnvironment::new(400.0, 1.0, 2, true, 16.0, 24.0)?,
            ),
            LayoutKey::new(
                a,
                EditorMode::Visual,
                LayoutEnvironment::new(400.0, 1.0, 2, false, 16.0, 24.0)?,
            ),
            LayoutKey::new(a, EditorMode::Source, environment(400.0)?),
            LayoutKey::new(b, EditorMode::Visual, environment(400.0)?),
        ] {
            assert!(cache.prepare(key)?);
            assert_eq!(cache.height(0), None);
            assert!(!cache.record(token, 0, 999.0));
        }
        Ok(())
    }

    #[test]
    fn cache_is_bounded_and_stable_measurement_does_not_request_new_layout()
    -> Result<(), Box<dyn Error>> {
        let session = session(b"line")?;
        let document = session.document().ok_or("document")?;
        let mut cache = LayoutCache::new(2);
        cache.prepare(LayoutKey::new(
            document,
            EditorMode::Source,
            environment(600.0)?,
        ))?;
        let token = cache.token().ok_or("token")?;
        assert!(cache.record(token, 0, 24.0));
        assert!(!cache.record(token, 0, 24.0));
        assert!(cache.record(token, 1, 96.0));
        assert_eq!(cache.height(0), Some(24.0));
        assert_eq!(cache.height(1), Some(96.0));
        for row in 2..1000 {
            cache.record(token, row, 24.0);
            assert!(cache.len() <= 2);
        }
        assert_eq!(cache.height(0), None);
        assert!(!cache.record(token, 1000, f32::NAN));
        assert!(!cache.record(token, 1000, -1.0));
        assert_eq!(cache.len(), 2);
        Ok(())
    }

    #[test]
    fn stale_anchors_and_layout_budget_are_rejected() -> Result<(), Box<dyn Error>> {
        let a = session(b"a\nb")?;
        let b = session(b"a\nb")?;
        let a = a.document().ok_or("a")?;
        let b = b.document().ok_or("b")?;
        assert!(ReaderIndex::new(a, 1).is_err());
        let index = ReaderIndex::new(a, 100)?;
        let anchor = index.anchor(a.parsed(), EditorMode::Source, 1, 0.0)?;
        assert!(
            index
                .row_for_anchor(b.parsed(), EditorMode::Source, &anchor)
                .is_err()
        );
        assert!(
            index
                .projection(a.parsed(), EditorMode::Source, 1, 0)
                .is_err()
        );
        assert!(LayoutEnvironment::new(0.0, 2.0, 1, true, 16.0, 24.0).is_err());
        Ok(())
    }

    #[test]
    fn initial_source_policy_uses_strict_four_mib_source_byte_threshold() {
        let threshold = 4 * 1024 * 1024;
        for bytes in [0, threshold - 1, threshold] {
            assert_eq!(initial_view(bytes), (EditorMode::Visual, None));
        }
        assert_eq!(
            initial_view(threshold + 1),
            (EditorMode::Source, Some("reading.large_source"))
        );
    }

    #[test]
    fn long_source_line_is_readable_through_checked_unicode_safe_fragments()
    -> Result<(), Box<dyn Error>> {
        let text = format!("{}中{}", "a".repeat(4095), "tail".repeat(20_000));
        let session = session(text.as_bytes())?;
        let document = session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100_000)?;
        assert!(index.row_count(EditorMode::Source) > 2);
        let mut restored = String::new();
        let mut source_end = 0;
        for row in 0..index.row_count(EditorMode::Source) {
            let source = index.source_range(document.parsed(), EditorMode::Source, row)?;
            assert_eq!(source.byte_range().start, source_end);
            source_end = source.byte_range().end;
            assert!(source.len_bytes() <= 4096);
            let projection = index.projection(document.parsed(), EditorMode::Source, row, 4096)?;
            restored.push_str(projection.drawn_text());
        }
        assert_eq!(source_end, text.len());
        assert_eq!(restored, text);
        assert_eq!(
            index
                .source_range(document.parsed(), EditorMode::Source, 0)?
                .byte_range(),
            0..4095
        );
        assert_eq!(
            index
                .source_range(document.parsed(), EditorMode::Source, 1)?
                .byte_range()
                .start,
            4095
        );
        Ok(())
    }

    #[test]
    fn giant_visual_item_is_rejected_before_projection_and_source_remains_accessible()
    -> Result<(), Box<dyn Error>> {
        let text = "a".repeat(65_537);
        let session = session(text.as_bytes())?;
        let document = session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100_000)?;
        assert!(matches!(
            index.preflight(
                document.parsed(),
                EditorMode::Visual,
                0,
                ItemLayoutBudget::new(65_536, 4096, 256)
            ),
            Err(LayoutError::ItemBytes {
                needed: 65_537,
                limit: 65_536
            })
        ));
        assert!(matches!(
            index.projection(document.parsed(), EditorMode::Visual, 0, 65_536),
            Err(LayoutError::ItemBytes { .. })
        ));
        assert!(
            index
                .projection(document.parsed(), EditorMode::Source, 0, 4096)
                .is_ok()
        );
        // 节点预算按该行要 shape 的文本片段计数：单段文本为 1，零预算必然拒绝。
        assert!(matches!(
            index.preflight(
                document.parsed(),
                EditorMode::Visual,
                0,
                ItemLayoutBudget::new(100_000, 0, 256)
            ),
            Err(LayoutError::ItemNodes {
                needed: 1,
                limit: 0
            })
        ));
        Ok(())
    }

    #[test]
    fn wide_table_column_budget_is_independent_from_small_text_bytes() -> Result<(), Box<dyn Error>>
    {
        let session = session(b"|a|b|c|\n|-|-|-|\n|x|y|z|\n")?;
        let document = session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100)?;
        assert!(matches!(
            index.preflight(
                document.parsed(),
                EditorMode::Visual,
                0,
                ItemLayoutBudget::new(65_536, 4096, 2)
            ),
            Err(LayoutError::TableColumns {
                needed: 3,
                limit: 2
            })
        ));
        assert!(
            index
                .preflight(
                    document.parsed(),
                    EditorMode::Visual,
                    0,
                    ItemLayoutBudget::new(65_536, 4096, 3)
                )
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn internal_source_anchor_survives_mode_width_and_font_layout_keys()
    -> Result<(), Box<dyn Error>> {
        let text = format!("prefix {} tail", "word ".repeat(3000));
        let session = session(text.as_bytes())?;
        let document = session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100_000)?;
        let visual = index.projection(document.parsed(), EditorMode::Visual, 0, 65_536)?;
        let anchor = index.anchor_from_local(
            document.parsed(),
            &visual,
            10_007,
            aline_buffer::Affinity::Before,
            -5.0,
        )?;
        assert_eq!(anchor.source().byte_offset(), 10_007);
        assert_eq!(anchor.affinity(), aline_buffer::Affinity::Before);
        assert_ne!(
            anchor.source().byte_offset(),
            index
                .source_range(document.parsed(), EditorMode::Visual, 0)?
                .byte_range()
                .start
        );
        let mut cache = LayoutCache::new(8);
        cache.prepare(LayoutKey::new(
            document,
            EditorMode::Visual,
            environment(600.0)?,
        ))?;
        for (mode, environment) in [
            (EditorMode::Source, environment(300.0)?),
            (
                EditorMode::Visual,
                LayoutEnvironment::new(800.0, 2.0, 2, true, 20.0, 30.0)?,
            ),
        ] {
            cache.prepare(LayoutKey::new(document, mode, environment))?;
            let row = index.row_for_anchor(document.parsed(), mode, &anchor)?;
            let projection = index.projection(document.parsed(), mode, row, 65_536)?;
            let local = projection.drawn_at(anchor.source(), aline_buffer::Affinity::Before)?;
            let rebound = projection.source_at(local.value(), aline_buffer::Affinity::Before)?;
            assert_eq!(rebound.value(), anchor.source());
            assert_eq!(anchor.viewport_delta(), -5.0);
        }
        Ok(())
    }

    #[test]
    fn deferred_anchor_restore_rejects_old_tokens_documents_and_cancellation()
    -> Result<(), Box<dyn Error>> {
        let a = session(b"first\nsecond")?;
        let b = session(b"first\nsecond")?;
        let a = a.document().ok_or("a")?;
        let b = b.document().ok_or("b")?;
        let index = ReaderIndex::new(a, 100)?;
        let anchor = index.anchor(a.parsed(), EditorMode::Source, 1, 3.0)?;
        let mut cache = layout_cache_for_restore(a)?;
        let old_layout = cache.token().ok_or("old layout")?;
        let mut restores = RestoreState::new(
            a.core().document_id().clone(),
            a.parsed().snapshot().id().clone(),
        );
        let old_request = restores.begin(anchor.clone(), 1, old_layout)?;
        cache.prepare(LayoutKey::new(a, EditorMode::Source, environment(300.0)?))?;
        let layout = cache.token().ok_or("layout")?;
        let request = restores.begin(anchor, 1, layout)?;
        assert!(!restores.cancel(&old_request));
        assert!(
            restores
                .take_for_row(&old_request, a.core().document_id(), layout, 1)
                .is_none()
        );
        assert!(
            restores
                .take_for_row(&request, b.core().document_id(), layout, 1)
                .is_none()
        );
        assert!(
            restores
                .take_for_row(&request, a.core().document_id(), old_layout, 1)
                .is_none()
        );
        assert!(
            restores
                .take_for_row(&request, a.core().document_id(), layout, 0)
                .is_none()
        );
        assert!(restores.pending().is_some());
        assert!(restores.schedule_for_row(&request, a.core().document_id(), layout, 1));
        assert!(!restores.schedule_for_row(&request, a.core().document_id(), layout, 1));
        assert!(restores.cancel(&request));
        assert!(
            restores
                .take_for_row(&request, a.core().document_id(), layout, 1)
                .is_none()
        );
        let request = restores.begin(
            index.anchor(a.parsed(), EditorMode::Source, 1, 3.0)?,
            1,
            layout,
        )?;
        let restored = restores
            .take_for_row(&request, a.core().document_id(), layout, 1)
            .ok_or("restore")?;
        assert_eq!(restored.source().byte_offset(), 6);
        assert!(!restores.cancel_current());
        assert!(
            restores
                .take_for_row(&request, a.core().document_id(), layout, 1)
                .is_none()
        );
        Ok(())
    }
    fn layout_cache_for_restore(
        document: &ReadonlyDocument,
    ) -> Result<LayoutCache, Box<dyn Error>> {
        let mut cache = LayoutCache::new(8);
        cache.prepare(LayoutKey::new(
            document,
            EditorMode::Source,
            environment(600.0)?,
        ))?;
        Ok(cache)
    }

    #[test]
    fn truncated_giant_table_is_rejected_by_item_nodes_and_source_stays_readable()
    -> Result<(), Box<dyn Error>> {
        let mut text = String::from("| a | b |\n| --- | --- |\n");
        for _ in 0..4096 {
            text.push_str("| x | y |\n");
        }
        let table_session = session(text.as_bytes())?;
        let document = table_session.document().ok_or("table")?;
        let index = ReaderIndex::new(document, 100_000)?;
        assert!(matches!(
            index.projection(document.parsed(), EditorMode::Visual, 0, 65_536),
            Err(LayoutError::ItemNodes { .. })
        ));
        // 源码视图保留完整内容，可继续逐行阅读。
        assert!(
            index
                .projection(document.parsed(), EditorMode::Source, 0, 65_536)
                .is_ok()
        );
        Ok(())
    }

    #[test]
    fn exact_visual_byte_limit_and_default_table_column_limit_are_enforced()
    -> Result<(), Box<dyn Error>> {
        let text = "a".repeat(65_536);
        let bytes_session = session(text.as_bytes())?;
        let document = bytes_session.document().ok_or("document")?;
        let index = ReaderIndex::new(document, 100_000)?;
        assert_eq!(
            index
                .projection(document.parsed(), EditorMode::Visual, 0, 65_536)?
                .drawn_text()
                .len(),
            65_536
        );
        for columns in [256, 257] {
            let row = format!("|{}|\n", vec!["a"; columns].join("|"));
            let delimiter = format!("|{}|\n", vec!["---"; columns].join("|"));
            let text = format!("{row}{delimiter}{row}");
            let table_session = session(text.as_bytes())?;
            let document = table_session.document().ok_or("table")?;
            let index = ReaderIndex::new(document, 100_000)?;
            let projected = index.projection(document.parsed(), EditorMode::Visual, 0, 65_536);
            if columns == 256 {
                assert!(projected.is_ok());
            } else {
                assert!(matches!(
                    projected,
                    Err(LayoutError::TableColumns {
                        needed: 257,
                        limit: 256
                    })
                ));
            }
            assert!(
                index
                    .projection(document.parsed(), EditorMode::Source, 0, 4096)
                    .is_ok()
            );
        }
        Ok(())
    }
}
