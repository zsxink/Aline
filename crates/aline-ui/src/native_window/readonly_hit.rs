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

#[cfg(test)]
mod tests {
    use super::{HitError, HitTarget, PreparedLayout};
    use crate::native_window::readonly_layout::{
        LayoutCache, LayoutEnvironment, LayoutKey, LayoutToken, ReaderIndex,
    };
    use crate::{
        DocLimits, EditorMode, ReadonlyDocument, ReadonlyFileAccess, ReadonlyFileInput,
        ReadonlyOpenLimits, ReadonlySession, WorkingVersion,
    };
    use std::error::Error;
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};

    use aline_buffer::{Affinity, HistoryBudget, LineIndexBudget, TextBudget};
    use aline_markdown::{MappingPrecision, ParseBudget, ParseConfig};
    use aline_source::StorageBudget;

    struct Files(&'static [u8]);
    impl ReadonlyFileAccess for Files {
        type Reader = Cursor<&'static [u8]>;
        fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
            Ok(ReadonlyFileInput::new(
                Cursor::new(self.0),
                self.0.len() as u64,
                true,
            ))
        }
    }
    fn session(bytes: &'static [u8]) -> Result<ReadonlySession, Box<dyn Error>> {
        let max = 1024 * 1024;
        let mut session = ReadonlySession::new(ReadonlyOpenLimits::new(
            StorageBudget::new(max),
            DocLimits::new(
                TextBudget::new(max),
                LineIndexBudget::new(4096),
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
    fn layout(
        document: &ReadonlyDocument,
        mode: EditorMode,
        width: f32,
    ) -> Result<LayoutCache, Box<dyn Error>> {
        let mut cache = LayoutCache::new(8);
        cache.prepare(LayoutKey::new(
            document,
            mode,
            LayoutEnvironment::new(width, 2.0, 1, true, 16.0, 24.0)?,
        ))?;
        Ok(cache)
    }
    fn make_target(
        document: &mut ReadonlyDocument,
        mode: EditorMode,
        row: usize,
        token: LayoutToken,
    ) -> Result<HitTarget, Box<dyn Error>> {
        let index = ReaderIndex::new(document, 4096)?;
        let projection = index.projection(document.parsed(), mode, row, 4096)?;
        Ok(HitTarget::new(document, token, projection)?)
    }
    fn assert_readonly(
        document: &ReadonlyDocument,
        text: &str,
        version: WorkingVersion,
    ) -> Result<(), Box<dyn Error>> {
        assert_eq!(document.core().buffer_text()?, text);
        assert_eq!(document.core().version(), version);
        assert_eq!(document.core().history_node_count(), 0);
        assert_eq!(document.core().undo_depth(), 0);
        assert_eq!(document.core().redo_depth(), 0);
        assert!(!document.core().has_open_block());
        assert!(document.core().open_block_draft().is_none());
        Ok(())
    }

    #[test]
    fn repeated_text_and_entity_sides_map_to_the_window_source_without_search()
    -> Result<(), Box<dyn Error>> {
        let text = "prefix\n\nrepeat &amp; repeat";
        let mut session = session(text.as_bytes())?;
        let document = session.document_mut().ok_or("document")?;
        let version = document.core().version();
        let cache = layout(document, EditorMode::Visual, 600.0)?;
        let token = cache.token().ok_or("token")?;
        let target = make_target(document, EditorMode::Visual, 2, token)?;
        let before = target.navigate(document, Some(token), Ok(7), Affinity::Before)?;
        assert_eq!(before.precision(), MappingPrecision::Aligned);
        assert_eq!(before.source_range().byte_range(), 15..20);
        assert_eq!(document.core().buffer_selection().head(), 15);
        let after = target.navigate(document, Some(token), Ok(7), Affinity::After)?;
        assert_eq!(after.precision(), MappingPrecision::Aligned);
        assert_eq!(document.core().buffer_selection().head(), 20);
        let repeated = target.navigate(document, Some(token), Ok(9), Affinity::After)?;
        assert_eq!(repeated.precision(), MappingPrecision::Exact);
        assert_eq!(document.core().buffer_selection().head(), 21);
        assert_readonly(document, text, version)
    }

    #[test]
    fn hidden_relations_do_not_turn_visible_text_into_hidden_pixel_targets()
    -> Result<(), Box<dyn Error>> {
        let mut session = session(b"**word**")?;
        let document = session.document_mut().ok_or("document")?;
        let index = ReaderIndex::new(document, 4096)?;
        let projection = index.projection(document.parsed(), EditorMode::Visual, 0, 4096)?;
        assert!(
            projection
                .segments()
                .iter()
                .any(|segment| segment.precision() == MappingPrecision::Hidden)
        );
        let token = layout(document, EditorMode::Visual, 600.0)?
            .token()
            .ok_or("token")?;
        let target = HitTarget::new(document, token, projection)?;
        let result = target.navigate(document, Some(token), Ok(0), Affinity::After)?;
        assert_eq!(result.precision(), MappingPrecision::Exact);
        assert_eq!(document.core().buffer_selection().head(), 2);
        Ok(())
    }

    #[test]
    fn synthetic_break_and_multi_scalar_entity_preserve_source_precision()
    -> Result<(), Box<dyn Error>> {
        let mut break_session = session(b"a<br>b")?;
        let document = break_session.document_mut().ok_or("document")?;
        let token = layout(document, EditorMode::Visual, 600.0)?
            .token()
            .ok_or("token")?;
        let target = make_target(document, EditorMode::Visual, 0, token)?;
        let result = target.navigate(document, Some(token), Ok(1), Affinity::After)?;
        assert_eq!(result.precision(), MappingPrecision::Synthetic);
        assert_eq!(result.source_range().byte_range(), 1..1);
        assert_eq!(document.core().buffer_selection().head(), 1);
        let mut entity = session(b"&NotEqualTilde;")?;
        let document = entity.document_mut().ok_or("entity")?;
        let token = layout(document, EditorMode::Visual, 600.0)?
            .token()
            .ok_or("token")?;
        let target = make_target(document, EditorMode::Visual, 0, token)?;
        for candidate in [0, 3] {
            let result = target.navigate(document, Some(token), Ok(candidate), Affinity::After)?;
            assert_eq!(result.precision(), MappingPrecision::Aligned);
            assert_eq!(result.source_range().byte_range(), 0..15);
            assert_eq!(document.core().buffer_selection().head(), 15);
        }
        Ok(())
    }

    #[test]
    fn source_unicode_endpoints_and_edge_candidates_remain_checked() -> Result<(), Box<dyn Error>> {
        let text = "first\r\n\t中文e\u{301} 👨‍👩‍👧‍👦 end\r";
        let mut session = session(text.as_bytes())?;
        let document = session.document_mut().ok_or("document")?;
        document.switch_mode(EditorMode::Source)?;
        let version = document.core().version();
        let token = layout(document, EditorMode::Source, 600.0)?
            .token()
            .ok_or("token")?;
        let target = make_target(document, EditorMode::Source, 1, token)?;
        let result = target.navigate(document, Some(token), Err(7), Affinity::After)?;
        assert!(result.is_edge_candidate());
        assert_eq!(document.core().buffer_selection().head(), 14);
        for candidate in ["\t中文e\u{301}".len(), "\t中文e\u{301} 👨‍👩‍👧‍👦".len()]
        {
            target.navigate(document, Some(token), Ok(candidate), Affinity::After)?;
            assert_eq!(document.core().buffer_selection().head(), 7 + candidate);
        }
        let before = document.core().buffer_selection();
        assert!(matches!(
            target.navigate(document, Some(token), Ok(2), Affinity::After),
            Err(HitError::InvalidOffset)
        ));
        assert!(matches!(
            target.navigate(document, Some(token), Err(usize::MAX), Affinity::After),
            Err(HitError::InvalidOffset)
        ));
        assert_eq!(document.core().buffer_selection(), before);
        assert_readonly(document, text, version)
    }

    #[test]
    fn empty_html_and_gap_source_entries_do_not_guess_visible_offsets() -> Result<(), Box<dyn Error>>
    {
        let mut session = session(b"<!-- hidden -->\n\nword")?;
        let document = session.document_mut().ok_or("document")?;
        let token = layout(document, EditorMode::Visual, 600.0)?
            .token()
            .ok_or("token")?;
        let index = ReaderIndex::new(document, 4096)?;
        let mut has_entry = false;
        for row in 0..index.row_count(EditorMode::Visual) {
            let projection = index.projection(document.parsed(), EditorMode::Visual, row, 4096)?;
            let source = index.source_range(document.parsed(), EditorMode::Visual, row)?;
            if projection.drawn_text().contains("<!--") || projection.drawn_text().trim().is_empty()
            {
                has_entry = true;
                let target = HitTarget::new(document, token, projection)?;
                let result = target.navigate(document, Some(token), Ok(0), Affinity::After)?;
                assert_eq!(result.precision(), MappingPrecision::Exact);
                assert_eq!(
                    document.core().buffer_selection().head(),
                    source.byte_range().start
                );
            }
        }
        assert!(has_entry);
        Ok(())
    }

    #[test]
    fn unprepared_text_layout_is_rejected_before_calling_gpui_panicking_query() {
        let layout = PreparedLayout::new(gpui::TextLayout::default());
        assert!(matches!(
            layout.index_for_position(gpui::point(gpui::px(0.0), gpui::px(0.0))),
            Err(HitError::Unmeasured)
        ));
    }

    #[test]
    fn old_layout_mode_and_replaced_document_hits_are_atomic_rejections()
    -> Result<(), Box<dyn Error>> {
        let text = "word";
        let mut a = session(text.as_bytes())?;
        let document = a.document_mut().ok_or("a")?;
        let mut cache = layout(document, EditorMode::Visual, 600.0)?;
        let token = cache.token().ok_or("token")?;
        let target = make_target(document, EditorMode::Visual, 0, token)?;
        let before = document.core().buffer_selection();
        cache.prepare(LayoutKey::new(
            document,
            EditorMode::Visual,
            LayoutEnvironment::new(300.0, 2.0, 1, true, 16.0, 24.0)?,
        ))?;
        assert!(matches!(
            target.navigate(document, cache.token(), Ok(2), Affinity::After),
            Err(HitError::StaleLayout)
        ));
        assert_eq!(document.core().buffer_selection(), before);
        document.switch_mode(EditorMode::Source)?;
        assert!(matches!(
            target.navigate(document, Some(token), Ok(2), Affinity::After),
            Err(HitError::StaleInput)
        ));
        assert_eq!(document.core().buffer_selection(), before);
        let mut b = session(text.as_bytes())?;
        let b = b.document_mut().ok_or("b")?;
        assert!(matches!(
            target.navigate(b, Some(token), Ok(2), Affinity::After),
            Err(HitError::StaleInput)
        ));
        assert_readonly(b, text, b.core().version())?;
        Ok(())
    }

    #[test]
    fn source_whitespace_projection_is_byte_exact_before_native_wrapping()
    -> Result<(), Box<dyn Error>> {
        for text in ["<foo>a   b</foo>", "a\t b"] {
            let bytes: &'static [u8] = match text {
                "<foo>a   b</foo>" => b"<foo>a   b</foo>",
                _ => b"a\t b",
            };
            let session = session(bytes)?;
            let document = session.document().ok_or("document")?;
            let index = ReaderIndex::new(document, 4096)?;
            let projection = index.projection(document.parsed(), EditorMode::Source, 0, 4096)?;
            assert_eq!(projection.drawn_text().as_bytes(), text.as_bytes());
        }
        Ok(())
    }

    #[test]
    fn native_glyph_candidate_start_and_end_use_opposite_entity_affinities()
    -> Result<(), Box<dyn Error>> {
        for bytes in [b"&amp;".as_slice(), b"&NotEqualTilde;".as_slice()] {
            let mut session = session(bytes)?;
            let document = session.document_mut().ok_or("document")?;
            let token = layout(document, EditorMode::Visual, 600.0)?
                .token()
                .ok_or("token")?;
            let index = ReaderIndex::new(document, 4096)?;
            let projection = index.projection(document.parsed(), EditorMode::Visual, 0, 4096)?;
            let end = projection.drawn_text().len();
            let target = HitTarget::new(document, token, projection)?;
            let start = target.navigate_candidate(document, Some(token), Ok(0))?;
            assert_eq!(start.precision(), MappingPrecision::Aligned);
            assert_eq!(document.core().buffer_selection().head(), 0);
            let end_hit = target.navigate_candidate(document, Some(token), Ok(end))?;
            assert_eq!(end_hit.precision(), MappingPrecision::Aligned);
            assert_eq!(document.core().buffer_selection().head(), bytes.len());
        }
        let mut session = session(b"prefix\n\nrepeat &amp; repeat")?;
        let document = session.document_mut().ok_or("document")?;
        let token = layout(document, EditorMode::Visual, 600.0)?
            .token()
            .ok_or("token")?;
        let target = make_target(document, EditorMode::Visual, 2, token)?;
        target.navigate_candidate(document, Some(token), Ok(7))?;
        assert_eq!(document.core().buffer_selection().head(), 15);
        target.navigate_candidate(document, Some(token), Ok(8))?;
        assert_eq!(document.core().buffer_selection().head(), 20);
        Ok(())
    }
}
