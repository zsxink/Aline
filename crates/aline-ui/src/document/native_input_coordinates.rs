//! 原生输入坐标适配。文档核心提供当前身份及可编辑源码范围，适配层只持有局部
//! 只读投影。平台回调携带捕获的投影身份，复验后才消费 UTF-16 或布局字节。

use aline_buffer::{Affinity, SrcOffset, SrcRange, SrcSelection, TextSnapshot};
use aline_markdown::{
    DrawnOffset, DrawnRange, MappingBudget, MappingError, Projection, ProjectionId, Utf16Offset,
    Utf16Range,
};

use crate::{IdentityError, InputIdentity};

/// 适配层拒绝。宿主在进入文档写路径前处理。
#[derive(Debug)]
pub(crate) enum InputCoordinateError {
    Identity(IdentityError),
    Mapping(MappingError),
}

impl std::fmt::Display for InputCoordinateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Identity(error) => error.fmt(formatter),
            Self::Mapping(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for InputCoordinateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Identity(error) => Some(error),
            Self::Mapping(error) => Some(error),
        }
    }
}

#[derive(Clone)]
pub(crate) struct NativeInputCoordinates {
    identity: InputIdentity,
    projection: Projection,
}

impl NativeInputCoordinates {
    /// 从宿主选定的当前源码输入范围构造恒等投影。打开块中的标记仍是原文。
    pub(crate) fn new(
        identity: InputIdentity,
        snapshot: &TextSnapshot,
        source: &SrcRange,
        budget: MappingBudget,
    ) -> Result<Self, InputCoordinateError> {
        if identity.snapshot_id() != snapshot.id() {
            return Err(InputCoordinateError::Identity(IdentityError::StaleSnapshot));
        }
        let projection = Projection::identity(snapshot.clone(), source.clone(), budget)
            .map_err(InputCoordinateError::Mapping)?;
        Ok(Self {
            identity,
            projection,
        })
    }

    /// 返回输入投影实例身份，供布局或平台桥接捕获。
    pub(crate) fn projection_id(&self) -> &ProjectionId {
        self.projection.id()
    }

    /// 用文档所有者当前的身份及回调捕获的投影身份获得一次只读映射。
    pub(crate) fn bind(
        &self,
        current: &InputIdentity,
        callback_projection: &ProjectionId,
    ) -> Result<BoundInputCoordinates<'_>, InputCoordinateError> {
        self.identity
            .validate(current)
            .map_err(InputCoordinateError::Identity)?;
        if callback_projection != self.projection_id() {
            return Err(InputCoordinateError::Mapping(
                MappingError::ProjectionMismatch,
            ));
        }
        Ok(BoundInputCoordinates(&self.projection))
    }
}

pub(crate) struct BoundInputCoordinates<'a>(&'a Projection);

impl BoundInputCoordinates<'_> {
    /// 绑定平台输入单位。代理对中间、反向和越界范围均拒绝。
    pub(crate) fn utf16_range(
        &self,
        units: std::ops::Range<usize>,
    ) -> Result<Utf16Range, MappingError> {
        self.0.utf16_range(units)
    }

    /// 绑定同一输入投影中布局返回的 UTF-8 字节。
    pub(crate) fn drawn_offset(&self, drawn_byte: usize) -> Result<DrawnOffset, MappingError> {
        self.0.drawn_offset(drawn_byte)
    }

    /// 布局使用的输入原文。
    pub(crate) fn drawn_text(&self) -> &str {
        self.0.drawn_text()
    }

    pub(crate) fn source_from_utf16(&self, range: &Utf16Range) -> Result<SrcRange, MappingError> {
        Ok(self.source_selection(range)?.range())
    }

    pub(crate) fn source_from_drawn(
        &self,
        offset: &DrawnOffset,
    ) -> Result<SrcOffset, MappingError> {
        Ok(self.0.source_at(offset, Affinity::Before)?.into_value())
    }

    pub(crate) fn drawn_from_utf16(&self, range: &Utf16Range) -> Result<DrawnRange, MappingError> {
        let selection = self.source_selection(range)?;
        let start = self
            .0
            .drawn_at(selection.anchor(), Affinity::Before)?
            .into_value();
        let end = self
            .0
            .drawn_at(selection.head(), Affinity::After)?
            .into_value();
        DrawnRange::new(start, end)
    }

    pub(crate) fn utf16_from_drawn(
        &self,
        offset: &DrawnOffset,
    ) -> Result<Utf16Offset, MappingError> {
        let source = self.source_from_drawn(offset)?;
        Ok(self
            .0
            .utf16_from_source(&source, Affinity::Before)?
            .into_value())
    }

    /// 返回平台排序范围。方向由原源码选区持有，跨出当前输入范围时拒绝。
    pub(crate) fn utf16_from_source_selection(
        &self,
        selection: &SrcSelection,
    ) -> Result<Utf16Range, MappingError> {
        let (start, end) = if selection.is_reversed() {
            (selection.head(), selection.anchor())
        } else {
            (selection.anchor(), selection.head())
        };
        let start = self
            .0
            .utf16_from_source(start, Affinity::Before)?
            .into_value();
        let end = self.0.utf16_from_source(end, Affinity::After)?.into_value();
        Utf16Range::new(start, end)
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn editable_selection(
        &self,
        range: &Utf16Range,
    ) -> Result<Option<SrcSelection>, MappingError> {
        let selection = self.source_selection(range)?;
        let is_boundary = |endpoint| {
            self.0
                .is_source_grapheme_boundary(endpoint)
                .unwrap_or(false)
        };
        Ok((is_boundary(selection.anchor()) && is_boundary(selection.head())).then_some(selection))
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn adjacent_grapheme(&self, source: &SrcOffset, forward: bool) -> Option<SrcOffset> {
        self.0
            .adjacent_source_grapheme_boundary(source, forward)
            .ok()?
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn candidate_selection(
        text: &str,
        range: std::ops::Range<usize>,
        max_bytes: usize,
    ) -> Option<aline_buffer::SelectionSpec> {
        if text.len() > max_bytes {
            return None;
        }
        let mut candidate = String::new();
        candidate.try_reserve_exact(text.len()).ok()?;
        candidate.push_str(text);
        let text = TextSnapshot::new(candidate, aline_buffer::TextBudget::new(text.len())).ok()?;
        let source = text.src_range(0..text.len_bytes()).ok()?;
        let projection = Projection::identity(
            text,
            source,
            MappingBudget::new(max_bytes, max_bytes.saturating_add(1)),
        )
        .ok()?;
        let bound = BoundInputCoordinates(&projection);
        let range = bound.utf16_range(range).ok()?;
        let selected = bound.editable_selection(&range).ok()??;
        Some(aline_buffer::SelectionSpec::new(
            selected.anchor().byte_offset(),
            selected.head().byte_offset(),
        ))
    }

    fn source_selection(&self, range: &Utf16Range) -> Result<SrcSelection, MappingError> {
        if range.projection_id() != self.0.id() {
            return Err(MappingError::ProjectionMismatch);
        }
        let units = range.code_unit_range();
        let start = self
            .0
            .source_from_utf16(&self.0.utf16_offset(units.start)?, Affinity::Before)?;
        let end = self
            .0
            .source_from_utf16(&self.0.utf16_offset(units.end)?, Affinity::After)?;
        SrcSelection::new(start.into_value(), end.into_value()).map_err(MappingError::Source)
    }
}

#[cfg(test)]
mod tests {
    //! W5 的独立无头坐标金样。

    use std::error::Error;

    use aline_buffer::{
        BufferError, HistoryBudget, LineIndexBudget, SelectionSpec, SrcSelection, TextBudget,
    };
    use aline_markdown::{
        MappingBudget, MappingError, MappingPrecision, ParseBudget, ParseConfig, parse,
    };

    use super::{InputCoordinateError, NativeInputCoordinates};
    use crate::{DocCore, DocLimits, EditorMode, IdentityError};

    const TEXT: TextBudget = TextBudget::new(256);
    const MAPPING: MappingBudget = MappingBudget::new(256, 128);

    fn document(text: &str) -> Result<DocCore, Box<dyn Error>> {
        Ok(DocCore::new_editable(
            text.to_owned(),
            SelectionSpec::new(0, 0),
            DocLimits::new(TEXT, LineIndexBudget::new(32), HistoryBudget::new(16, 1024)),
        )?)
    }

    #[test]
    fn local_unicode_goldens_map_utf16_layout_bytes_and_document_source()
    -> Result<(), Box<dyn Error>> {
        let mut document = document("前👋\n甲，e\u{301}👩‍💻\r\n尾")?;
        let identity = document.current_input_identity()?;
        let current = document.current_snapshot(TEXT)?;
        let source = current.text();
        let coordinates = NativeInputCoordinates::new(
            identity.clone(),
            source,
            &source.src_range(8..30)?,
            MAPPING,
        )?;
        let bound = coordinates.bind(&identity, coordinates.projection_id())?;
        assert_eq!(bound.drawn_text(), "甲，e\u{301}👩‍💻\r\n");

        // 源码字节、局部布局字节、UTF-16 各列独立列出。组合符、ZWJ 及 CRLF
        // 内部的标量边界可用于平台查询，不意味着普通移动/删除可拆字素。
        for (src_off, drawn_byte, utf16_off) in [
            (8, 0, 0),
            (11, 3, 1),
            (14, 6, 2),
            (15, 7, 3),
            (17, 9, 4),
            (21, 13, 6),
            (24, 16, 7),
            (28, 20, 9),
            (29, 21, 10),
            (30, 22, 11),
        ] {
            let layout = bound.drawn_offset(drawn_byte)?;
            let platform = bound.utf16_range(utf16_off..utf16_off)?;
            assert_eq!(bound.utf16_from_drawn(&layout)?.code_units(), utf16_off);
            assert_eq!(bound.source_from_drawn(&layout)?.byte_offset(), src_off);
            assert_eq!(
                bound.source_from_utf16(&platform)?.byte_range(),
                src_off..src_off
            );
            assert_eq!(
                bound.drawn_from_utf16(&platform)?.byte_range(),
                drawn_byte..drawn_byte
            );
        }
        let selection = SrcSelection::new(source.src_offset(30)?, source.src_offset(14)?)?;
        let platform = bound.utf16_from_source_selection(&selection)?;
        assert!(selection.is_reversed());
        assert_eq!(platform.code_unit_range(), 2..11);
        assert_eq!(bound.drawn_from_utf16(&platform)?.byte_range(), 6..22);
        assert_eq!(bound.source_from_utf16(&platform)?.byte_range(), 14..30);
        Ok(())
    }

    #[test]
    fn native_ranges_refuse_half_units_inverted_bounds_and_cross_block_selection()
    -> Result<(), Box<dyn Error>> {
        let mut document = document("上\n甲😀\r\n下")?;
        let identity = document.current_input_identity()?;
        let current = document.current_snapshot(TEXT)?;
        let source = current.text();
        let coordinates = NativeInputCoordinates::new(
            identity.clone(),
            source,
            &source.src_range(4..11)?,
            MAPPING,
        )?;
        let bound = coordinates.bind(&identity, coordinates.projection_id())?;
        assert!(matches!(
            bound.utf16_range(1..2),
            Err(MappingError::MiddleSurrogate)
        ));
        assert!(matches!(
            bound.drawn_offset(5),
            Err(MappingError::NotCharBoundary)
        ));
        assert!(matches!(
            bound.utf16_range(std::ops::Range { start: 3, end: 1 }),
            Err(MappingError::ReversedRange)
        ));
        assert!(matches!(
            bound.utf16_range(0..usize::MAX),
            Err(MappingError::OutOfBounds { len: 3, .. })
        ));
        let crossing = SrcSelection::new(source.src_offset(4)?, source.src_offset(13)?)?;
        assert!(matches!(
            bound.utf16_from_source_selection(&crossing),
            Err(MappingError::SourceOutsideProjection)
        ));
        let empty = NativeInputCoordinates::new(
            identity.clone(),
            source,
            &source.src_range(11..11)?,
            MAPPING,
        )?;
        let empty = empty.bind(&identity, empty.projection_id())?;
        assert_eq!(
            empty
                .source_from_utf16(&empty.utf16_range(0..0)?)?
                .byte_range(),
            11..11
        );
        Ok(())
    }

    #[test]
    fn reading_hidden_aligned_and_synthetic_projection_cannot_be_used_for_input()
    -> Result<(), Box<dyn Error>> {
        let mut document = document("**甲** &amp;<br>乙")?;
        let identity = document.current_input_identity()?;
        let current = document.current_snapshot(TEXT)?;
        let source = current.text();
        let parsed = parse(source, ParseConfig::m0(), ParseBudget::m0())?;
        let reading = parsed.projection();
        for precision in [
            MappingPrecision::Hidden,
            MappingPrecision::Aligned,
            MappingPrecision::Synthetic,
        ] {
            assert!(
                reading
                    .segments()
                    .iter()
                    .any(|segment| segment.precision() == precision)
            );
        }
        let coordinates = NativeInputCoordinates::new(
            identity.clone(),
            source,
            &source.src_range(0..source.len_bytes())?,
            MAPPING,
        )?;
        assert!(matches!(
            coordinates.bind(&identity, reading.id()),
            Err(InputCoordinateError::Mapping(
                MappingError::ProjectionMismatch
            ))
        ));
        let bound = coordinates.bind(&identity, coordinates.projection_id())?;
        assert!(matches!(
            bound.source_from_utf16(&reading.utf16_range(0..0)?),
            Err(MappingError::ProjectionMismatch)
        ));
        assert!(matches!(
            bound.source_from_drawn(&reading.drawn_offset(0)?),
            Err(MappingError::ProjectionMismatch)
        ));

        let literal = bound.utf16_range(0..2)?;
        assert_eq!(bound.drawn_text(), "**甲** &amp;<br>乙");
        assert_eq!(bound.source_from_utf16(&literal)?.byte_range(), 0..2);
        Ok(())
    }

    #[test]
    fn old_document_request_projection_and_draft_reject_without_side_effects()
    -> Result<(), Box<dyn Error>> {
        let mut document = document("甲😀")?;
        let identity = document.current_input_identity()?;
        let current = document.current_snapshot(TEXT)?;
        let source = current.text();
        let scope = source.src_range(0..7)?;
        let coordinates = NativeInputCoordinates::new(identity.clone(), source, &scope, MAPPING)?;
        let layout_id = coordinates.projection_id();

        let mut other = self::document("甲😀")?;
        assert!(matches!(
            coordinates.bind(&other.current_input_identity()?, layout_id),
            Err(InputCoordinateError::Identity(IdentityError::WrongDocument))
        ));
        assert!(matches!(
            NativeInputCoordinates::new(other.current_input_identity()?, source, &scope, MAPPING),
            Err(InputCoordinateError::Identity(IdentityError::StaleSnapshot))
        ));
        let wrong_range = other.current_snapshot(TEXT)?.text().src_range(0..7)?;
        assert!(matches!(
            NativeInputCoordinates::new(identity.clone(), source, &wrong_range, MAPPING),
            Err(InputCoordinateError::Mapping(MappingError::Source(
                BufferError::SnapshotMismatch
            )))
        ));
        let resized = NativeInputCoordinates::new(identity.clone(), source, &scope, MAPPING)?;
        assert!(matches!(
            resized.bind(&identity, layout_id),
            Err(InputCoordinateError::Mapping(
                MappingError::ProjectionMismatch
            ))
        ));
        let old_bound = coordinates.bind(&identity, layout_id)?;
        let new_bound = resized.bind(&identity, resized.projection_id())?;
        assert!(matches!(
            new_bound.source_from_utf16(&old_bound.utf16_range(0..1)?),
            Err(MappingError::ProjectionMismatch)
        ));

        document.advance_parse_config()?;
        let new_configuration = document.current_input_identity()?;
        assert!(matches!(
            coordinates.bind(&new_configuration, layout_id),
            Err(InputCoordinateError::Identity(
                IdentityError::StaleParseConfig
            ))
        ));
        let new_coordinates =
            NativeInputCoordinates::new(new_configuration, source, &scope, MAPPING)?;
        document.switch_mode(EditorMode::Source, 1)?;
        let mode_identity = document.current_input_identity()?;
        assert!(matches!(
            new_coordinates.bind(&mode_identity, new_coordinates.projection_id()),
            Err(InputCoordinateError::Identity(IdentityError::StaleRequest))
        ));
        let current_coordinates =
            NativeInputCoordinates::new(mode_identity.clone(), source, &scope, MAPPING)?;
        document.type_selection(&mode_identity, "乙", 2)?;
        let new_identity = document.current_input_identity()?;
        let capture = |document: &DocCore| {
            (
                document.version(),
                document.current_selection(),
                document.open_block_draft().map(str::to_owned),
                document.undo_depth(),
                document.redo_depth(),
                document.history_node_count(),
            )
        };
        let before = capture(&document);
        assert!(matches!(
            current_coordinates.bind(&new_identity, current_coordinates.projection_id()),
            Err(InputCoordinateError::Identity(
                IdentityError::StaleWorkingVersion
            ))
        ));
        assert_eq!(capture(&document), before);
        assert_eq!(document.current_snapshot(TEXT)?.full_text(TEXT)?, "乙甲😀");
        assert_eq!(document.buffer_text()?, "甲😀");
        Ok(())
    }
}
