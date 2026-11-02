//! 投影身份与受检坐标；恒等投影由源码片段构造，供 UI/edit 无头消费。
//!
//! 此阶段只建立恒等投影和单位边界，不解析 Markdown、不伪造实体/HTML 映射。
//! 投影持有不可变源码与受预算索引，查询不重建全文或在调用方手算局部差。

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use super::coordinates::{DrawnOffset, DrawnRange, ProjectionId, Utf16Offset, Utf16Range};

use aline_buffer::{
    Affinity, BufferError, SnapshotId, SrcOffset, SrcRange, SrcSelection, TextBudget, TextSnapshot,
};

/// 映射输入、投影身份、单位边界或预算失败。
#[derive(Debug)]
pub enum MappingError {
    /// 源码快照或范围校验失败，保留原始类别。
    Source(BufferError),
    /// 绘制/UTF-16 端点超出当前投影。
    OutOfBounds {
        /// 请求端点。
        offset: usize,
        /// 当前单位长度。
        len: usize,
    },
    /// 绘制字节端点位于 UTF-8 标量内部。
    NotCharBoundary,
    /// UTF-16 端点位于代理对中间。
    MiddleSurrogate,
    /// 输入属于另一投影。
    ProjectionMismatch,
    /// 合法源码端点位于当前局部投影以外。
    SourceOutsideProjection,
    /// 半开范围反向。
    ReversedRange,
    /// 注入的绘制字节或索引点预算耗尽。
    BudgetExceeded,
    /// 长度累计溢出。
    LengthOverflow,
    /// 内部索引没有满足构造不变量。
    InvalidMapping,
    /// 受检分配失败。
    Allocation(std::collections::TryReserveError),
}

impl fmt::Display for MappingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for MappingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

/// 映射结果的来源精确性；本阶段恒等投影只产生 Exact。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MappingPrecision {
    /// 原样文本边界精确对应。
    Exact,
    /// 多对一来源对齐到规范锚点。
    Aligned,
    /// 隐藏源码的映射锚点。
    Hidden,
    /// 合成展示内容的来源锚点。
    Synthetic,
}

impl MappingPrecision {
    /// 返回稳定 dump 名称；只用于无头审查输出，不构成映射语义。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "Exact",
            Self::Aligned => "Aligned",
            Self::Hidden => "Hidden",
            Self::Synthetic => "Synthetic",
        }
    }
}

/// 一个绘制片段到源码片段的受检关系；片段可精确、对齐、隐藏或合成。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MappingSegment {
    drawn: Range<usize>,
    source: SrcRange,
    precision: MappingPrecision,
}

impl MappingSegment {
    /// 构造片段关系；绘制范围反向或源码范围不属于同一快照时拒绝。
    pub fn new(
        drawn: Range<usize>,
        source: SrcRange,
        precision: MappingPrecision,
    ) -> Result<Self, MappingError> {
        if drawn.start > drawn.end {
            return Err(MappingError::ReversedRange);
        }
        if source.byte_range().start > source.byte_range().end {
            return Err(MappingError::ReversedRange);
        }
        match precision {
            MappingPrecision::Exact if drawn.len() != source.len_bytes() => {
                return Err(MappingError::InvalidMapping);
            }
            MappingPrecision::Aligned if drawn.is_empty() || source.is_empty() => {
                return Err(MappingError::InvalidMapping);
            }
            MappingPrecision::Hidden if !drawn.is_empty() || source.is_empty() => {
                return Err(MappingError::InvalidMapping);
            }
            MappingPrecision::Synthetic if drawn.is_empty() || !source.is_empty() => {
                return Err(MappingError::InvalidMapping);
            }
            _ => {}
        }
        Ok(Self {
            drawn,
            source,
            precision,
        })
    }

    /// 返回绘制片段范围。
    pub fn drawn_range(&self) -> Range<usize> {
        self.drawn.clone()
    }

    /// 返回源码片段范围。
    pub fn source_range(&self) -> &SrcRange {
        &self.source
    }
    /// 返回片段精确性。
    pub const fn precision(&self) -> MappingPrecision {
        self.precision
    }
}

/// 携带精确性、来源和方向偏好的受检转换结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mapped<T> {
    value: T,
    source: SrcRange,
    precision: MappingPrecision,
    affinity: Affinity,
}

impl<T> Mapped<T> {
    /// 借用受检目标坐标。
    pub fn value(&self) -> &T {
        &self.value
    }
    /// 取得受检目标坐标所有权。
    pub fn into_value(self) -> T {
        self.value
    }
    /// 返回实际源码来源，恒等端点的来源是空范围。
    pub fn source_range(&self) -> &SrcRange {
        &self.source
    }
    /// 返回来源精确性，不能把非 Exact 伪装成双射。
    pub const fn precision(&self) -> MappingPrecision {
        self.precision
    }
    /// 返回本次转换采用的方向偏好。
    pub const fn affinity(&self) -> Affinity {
        self.affinity
    }
}

/// 投影构造的绘制字节、片段数与索引点预算，不代表发布性能限额。
///
/// `max_points` 在 M0 中同时限制索引点数和片段数；片段数超限与索引点超限都返回
/// [`MappingError::BudgetExceeded`]。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MappingBudget {
    max_drawn_bytes: usize,
    max_points: usize,
}

impl MappingBudget {
    /// 注入绘制文本和索引点上限。
    ///
    /// 索引点上限同时作为片段数上限（保持 M0 既有语义），因此片段数超限同样返回
    /// [`MappingError::BudgetExceeded`]。
    pub const fn new(max_drawn_bytes: usize, max_points: usize) -> Self {
        Self {
            max_drawn_bytes,
            max_points,
        }
    }
}

/// 只读投影；输入源范围和 UTF-16 索引由映射层统一拥有。
#[derive(Clone)]
pub struct Projection {
    id: ProjectionId,
    snapshot: TextSnapshot,
    source: SrcRange,
    drawn: Arc<String>,
    points: Arc<Vec<(usize, usize)>>,
    segments: Arc<Vec<MappingSegment>>,
}

impl fmt::Debug for Projection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Projection")
            .field("id", &self.id)
            .field("source", &self.source)
            .field("drawn_bytes", &self.drawn.len())
            .field("points", &self.points.len())
            .field("segments", &self.segments.len())
            .finish_non_exhaustive()
    }
}

impl Projection {
    /// 从明确源码片段构造恒等投影；只在显式构造点复制片段并建立受预算索引。
    pub fn identity(
        snapshot: TextSnapshot,
        source: SrcRange,
        budget: MappingBudget,
    ) -> Result<Self, MappingError> {
        snapshot
            .validate_range(&source)
            .map_err(MappingError::Source)?;
        let slice = snapshot
            .slice(&source, TextBudget::new(budget.max_drawn_bytes))
            .map_err(MappingError::Source)?;
        let drawn = slice.into_owned();
        let segments = vec![MappingSegment::new(
            0..drawn.len(),
            source.clone(),
            MappingPrecision::Exact,
        )?];
        Self::from_segments(snapshot, source, drawn, segments, budget)
    }

    /// 从绘制文本与显式片段关系构造投影；片段必须按绘制顺序覆盖可见文本。
    pub fn from_segments(
        snapshot: TextSnapshot,
        source: SrcRange,
        drawn: String,
        mut segments: Vec<MappingSegment>,
        budget: MappingBudget,
    ) -> Result<Self, MappingError> {
        snapshot
            .validate_range(&source)
            .map_err(MappingError::Source)?;
        if drawn.len() > budget.max_drawn_bytes {
            return Err(MappingError::BudgetExceeded);
        }
        // `max_points` 兼作片段数上限；该维度超限与索引点超限共用 BudgetExceeded。
        if segments.len() > budget.max_points {
            return Err(MappingError::BudgetExceeded);
        }
        segments.sort_by(|left, right| {
            left.drawn
                .start
                .cmp(&right.drawn.start)
                .then_with(|| left.drawn.end.cmp(&right.drawn.end))
                .then_with(|| {
                    left.source
                        .byte_range()
                        .start
                        .cmp(&right.source.byte_range().start)
                })
        });
        let source_range = source.byte_range();
        let mut visible_end = 0usize;
        for segment in &segments {
            let drawn_range = segment.drawn_range();
            if drawn_range.end > drawn.len() {
                return Err(MappingError::OutOfBounds {
                    offset: drawn_range.end,
                    len: drawn.len(),
                });
            }
            if !drawn.is_char_boundary(drawn_range.start)
                || !drawn.is_char_boundary(drawn_range.end)
            {
                return Err(MappingError::NotCharBoundary);
            }
            // 身份优先：片段必须携带当前快照的 SrcRange 身份，不能只比较裸字节数值。
            // 只取 `byte_range()` 会把另一快照（含同值不同快照）的片段装进当前投影，
            // 之后 `source_at` 却返回绑定当前快照的坐标与 Exact。
            let segment_source = snapshot
                .validate_range(segment.source_range())
                .map_err(MappingError::Source)?;
            if segment_source.start < source_range.start || segment_source.end > source_range.end {
                return Err(MappingError::SourceOutsideProjection);
            }
            match segment.precision() {
                MappingPrecision::Hidden => {}
                MappingPrecision::Synthetic => {
                    if drawn_range.start != visible_end {
                        return Err(MappingError::InvalidMapping);
                    }
                    visible_end = drawn_range.end;
                }
                _ => {
                    if drawn_range.start != visible_end {
                        return Err(MappingError::InvalidMapping);
                    }
                    visible_end = drawn_range.end;
                }
            }
        }
        if visible_end != drawn.len() {
            return Err(MappingError::InvalidMapping);
        }
        let points = build_points(&drawn, budget)?;
        Ok(Self {
            id: ProjectionId(Arc::new(())),
            snapshot,
            source,
            drawn: Arc::new(drawn),
            points: Arc::new(points),
            segments: Arc::new(segments),
        })
    }

    /// 返回投影实例身份。
    pub fn id(&self) -> &ProjectionId {
        &self.id
    }
    /// 返回输入源码的身份，不返回可写句柄。
    pub fn snapshot_id(&self) -> &SnapshotId {
        self.snapshot.id()
    }
    /// 返回投影对应的全局源码范围。
    pub fn source_range(&self) -> &SrcRange {
        &self.source
    }

    /// 字素算法需要完整上下文；局部投影不能假定切口是字素边界。
    pub(super) fn has_full_source_context(&self) -> bool {
        self.source.byte_range() == (0..self.snapshot.len_bytes())
    }

    /// 返回绘制文本；它是只读投影，不是文档真源。
    pub fn drawn_text(&self) -> &str {
        &self.drawn
    }
    /// 返回投影片段关系；调用方只能读取，不能据此假定双射。
    pub fn segments(&self) -> &[MappingSegment] {
        &self.segments
    }

    /// 构造绘制字节偏移；通过预建索引检查 UTF-8，不逐次物化源码。
    pub fn drawn_offset(&self, drawn_off: usize) -> Result<DrawnOffset, MappingError> {
        if drawn_off > self.drawn.len() {
            return Err(MappingError::OutOfBounds {
                offset: drawn_off,
                len: self.drawn.len(),
            });
        }
        self.points
            .binary_search_by_key(&drawn_off, |&(byte, _)| byte)
            .map_err(|_| MappingError::NotCharBoundary)?;
        Ok(DrawnOffset {
            projection: self.id.clone(),
            byte: drawn_off,
        })
    }

    /// 构造 UTF-16 偏移；代理对中间和越界失败，不 clamp。
    pub fn utf16_offset(&self, utf16_off: usize) -> Result<Utf16Offset, MappingError> {
        let total = self.points.last().ok_or(MappingError::InvalidMapping)?.1;
        if utf16_off > total {
            return Err(MappingError::OutOfBounds {
                offset: utf16_off,
                len: total,
            });
        }
        self.points
            .binary_search_by_key(&utf16_off, |&(_, unit)| unit)
            .map_err(|_| MappingError::MiddleSurrogate)?;
        Ok(Utf16Offset {
            projection: self.id.clone(),
            unit: utf16_off,
        })
    }

    /// 校验绘制半开范围的两端和投影身份。
    pub fn drawn_range(&self, bytes: Range<usize>) -> Result<DrawnRange, MappingError> {
        DrawnRange::new(
            self.drawn_offset(bytes.start)?,
            self.drawn_offset(bytes.end)?,
        )
    }

    /// 校验 UTF-16 半开范围的两端和投影身份。
    pub fn utf16_range(&self, units: Range<usize>) -> Result<Utf16Range, MappingError> {
        Utf16Range::new(
            self.utf16_offset(units.start)?,
            self.utf16_offset(units.end)?,
        )
    }

    /// 绘制端点转换为全局源码端点；即使恒等转换也复验投影身份。
    pub fn source_at(
        &self,
        drawn: &DrawnOffset,
        affinity: Affinity,
    ) -> Result<Mapped<SrcOffset>, MappingError> {
        if drawn.projection != self.id {
            return Err(MappingError::ProjectionMismatch);
        }
        self.drawn_offset(drawn.byte)?;
        let segment = match self.segment_for_drawn(drawn.byte) {
            Some(segment) => segment,
            None if self.drawn.is_empty() => {
                let offset = self.source.byte_range().start;
                let value = self
                    .snapshot
                    .src_offset(offset)
                    .map_err(MappingError::Source)?;
                let source = self
                    .snapshot
                    .src_range(offset..offset)
                    .map_err(MappingError::Source)?;
                return Ok(Mapped {
                    value,
                    source,
                    precision: MappingPrecision::Exact,
                    affinity,
                });
            }
            None => return Err(MappingError::InvalidMapping),
        };
        let source_range = segment.source_range().byte_range();
        let (source_off, source) = match segment.precision() {
            MappingPrecision::Exact => {
                let delta = drawn
                    .byte
                    .checked_sub(segment.drawn_range().start)
                    .ok_or(MappingError::LengthOverflow)?;
                let offset = source_range
                    .start
                    .checked_add(delta)
                    .ok_or(MappingError::LengthOverflow)?;
                let source = self
                    .snapshot
                    .src_range(offset..offset)
                    .map_err(MappingError::Source)?;
                (offset, source)
            }
            MappingPrecision::Aligned => {
                let offset = match affinity {
                    Affinity::Before => source_range.start,
                    Affinity::After => source_range.end,
                };
                let source = self
                    .snapshot
                    .src_range(source_range)
                    .map_err(MappingError::Source)?;
                (offset, source)
            }
            MappingPrecision::Hidden => {
                let offset = match affinity {
                    Affinity::Before => source_range.start,
                    Affinity::After => source_range.end,
                };
                let source = self
                    .snapshot
                    .src_range(source_range)
                    .map_err(MappingError::Source)?;
                (offset, source)
            }
            MappingPrecision::Synthetic => {
                let offset = source_range.start;
                let source = self
                    .snapshot
                    .src_range(offset..offset)
                    .map_err(MappingError::Source)?;
                (offset, source)
            }
        };
        let value = self
            .snapshot
            .src_offset(source_off)
            .map_err(MappingError::Source)?;
        Ok(Mapped {
            value,
            source,
            precision: segment.precision(),
            affinity,
        })
    }

    /// 全局源码端点转换为当前局部绘制端点；不由调用方减块起点。
    pub fn drawn_at(
        &self,
        source: &SrcOffset,
        affinity: Affinity,
    ) -> Result<Mapped<DrawnOffset>, MappingError> {
        let src_off = self
            .snapshot
            .validate_offset(source)
            .map_err(MappingError::Source)?;
        let range = self.source.byte_range();
        if src_off < range.start || src_off > range.end {
            return Err(MappingError::SourceOutsideProjection);
        }
        let segment = match self.segment_for_source(src_off) {
            Some(segment) => segment,
            None if self.drawn.is_empty() && src_off == range.start => {
                let value = self.drawn_offset(0)?;
                let source = self
                    .snapshot
                    .src_range(src_off..src_off)
                    .map_err(MappingError::Source)?;
                return Ok(Mapped {
                    value,
                    source,
                    precision: MappingPrecision::Exact,
                    affinity,
                });
            }
            None => return Err(MappingError::InvalidMapping),
        };
        let segment_source = segment.source_range().byte_range();
        let drawn_off = match segment.precision() {
            MappingPrecision::Exact => {
                let delta = src_off
                    .checked_sub(segment_source.start)
                    .ok_or(MappingError::LengthOverflow)?;
                segment
                    .drawn_range()
                    .start
                    .checked_add(delta)
                    .ok_or(MappingError::LengthOverflow)?
            }
            MappingPrecision::Aligned | MappingPrecision::Hidden => match affinity {
                Affinity::Before => segment.drawn_range().start,
                Affinity::After => segment.drawn_range().end,
            },
            MappingPrecision::Synthetic => match affinity {
                Affinity::Before => segment.drawn_range().start,
                Affinity::After => segment.drawn_range().end,
            },
        };
        let value = self.drawn_offset(drawn_off)?;
        let source = match segment.precision() {
            MappingPrecision::Exact | MappingPrecision::Synthetic => self
                .snapshot
                .src_range(src_off..src_off)
                .map_err(MappingError::Source)?,
            MappingPrecision::Aligned | MappingPrecision::Hidden => self
                .snapshot
                .src_range(segment_source)
                .map_err(MappingError::Source)?,
        };
        Ok(Mapped {
            value,
            source,
            precision: segment.precision(),
            affinity,
        })
    }

    /// 原生 UTF-16 端点经同一索引转换到源码；代理对中间不接受。
    pub fn source_from_utf16(
        &self,
        offset: &Utf16Offset,
        affinity: Affinity,
    ) -> Result<Mapped<SrcOffset>, MappingError> {
        if offset.projection != self.id {
            return Err(MappingError::ProjectionMismatch);
        }
        self.utf16_offset(offset.unit)?;
        let index = self
            .points
            .binary_search_by_key(&offset.unit, |&(_, unit)| unit)
            .map_err(|_| MappingError::InvalidMapping)?;
        let drawn_off = self
            .points
            .get(index)
            .ok_or(MappingError::InvalidMapping)?
            .0;
        self.source_at(&self.drawn_offset(drawn_off)?, affinity)
    }

    /// 源码端点转换为原生 UTF-16；不让调用方另建 UTF-16 表。
    pub fn utf16_from_source(
        &self,
        source: &SrcOffset,
        affinity: Affinity,
    ) -> Result<Mapped<Utf16Offset>, MappingError> {
        let drawn = self.drawn_at(source, affinity)?;
        let index = self
            .points
            .binary_search_by_key(&drawn.value.byte, |&(byte, _)| byte)
            .map_err(|_| MappingError::InvalidMapping)?;
        let units = self
            .points
            .get(index)
            .ok_or(MappingError::InvalidMapping)?
            .1;
        Ok(Mapped {
            value: self.utf16_offset(units)?,
            source: drawn.source,
            precision: drawn.precision,
            affinity,
        })
    }

    /// 对齐方向性源码选区；恒等投影保持原端点与方向且验证范围。
    pub fn aligned(&self, selection: &SrcSelection) -> Result<SrcSelection, MappingError> {
        self.snapshot
            .validate_selection(selection)
            .map_err(MappingError::Source)?;
        let (anchor_affinity, head_affinity) = if selection.is_reversed() {
            (Affinity::After, Affinity::Before)
        } else {
            (Affinity::Before, Affinity::After)
        };
        let anchor = self.drawn_at(selection.anchor(), anchor_affinity)?;
        let head = self.drawn_at(selection.head(), head_affinity)?;
        SrcSelection::new(
            self.source_at(anchor.value(), anchor_affinity)?
                .into_value(),
            self.source_at(head.value(), head_affinity)?.into_value(),
        )
        .map_err(MappingError::Source)
    }

    fn segment_for_drawn(&self, drawn_off: usize) -> Option<&MappingSegment> {
        if self.drawn.is_empty() {
            return None;
        }
        for segment in self.segments.iter() {
            if segment.precision() == MappingPrecision::Hidden {
                continue;
            }
            let range = segment.drawn_range();
            if range.start <= drawn_off && drawn_off < range.end {
                return Some(segment);
            }
        }
        if drawn_off == self.drawn.len() {
            for segment in self.segments.iter().rev() {
                if segment.precision() == MappingPrecision::Hidden {
                    continue;
                }
                if segment.drawn_range().end == drawn_off {
                    return Some(segment);
                }
            }
        }
        None
    }

    fn segment_for_source(&self, source_off: usize) -> Option<&MappingSegment> {
        let mut best: Option<(u8, &MappingSegment)> = None;
        for segment in self.segments.iter() {
            let range = segment.source_range().byte_range();
            let contains = match segment.precision() {
                MappingPrecision::Synthetic => range.start == source_off && range.end == source_off,
                MappingPrecision::Exact | MappingPrecision::Aligned | MappingPrecision::Hidden => {
                    range.start <= source_off && source_off <= range.end
                }
            };
            if !contains {
                continue;
            }
            let rank = match segment.precision() {
                MappingPrecision::Exact => 0,
                MappingPrecision::Synthetic => 1,
                MappingPrecision::Aligned => 2,
                MappingPrecision::Hidden => 3,
            };
            if best.is_none_or(|(best_rank, _)| rank < best_rank) {
                best = Some((rank, segment));
            }
        }
        best.map(|(_, segment)| segment)
    }
}

fn build_points(drawn: &str, budget: MappingBudget) -> Result<Vec<(usize, usize)>, MappingError> {
    let mut points = Vec::new();
    let mut units = 0usize;
    push_point(&mut points, (0, 0), budget)?;
    for (drawn_off, character) in drawn.char_indices() {
        let end = drawn_off
            .checked_add(character.len_utf8())
            .ok_or(MappingError::LengthOverflow)?;
        units = units
            .checked_add(character.len_utf16())
            .ok_or(MappingError::LengthOverflow)?;
        push_point(&mut points, (end, units), budget)?;
    }
    Ok(points)
}

fn push_point(
    points: &mut Vec<(usize, usize)>,
    point: (usize, usize),
    budget: MappingBudget,
) -> Result<(), MappingError> {
    if points.len() >= budget.max_points {
        return Err(MappingError::BudgetExceeded);
    }
    points.try_reserve(1).map_err(MappingError::Allocation)?;
    points.push(point);
    Ok(())
}
