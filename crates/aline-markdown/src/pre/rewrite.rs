//! 解析副本重写与区间映射；所有重写只作用于副本，源码快照保持不变。

use std::fmt;
use std::ops::Range;

use crate::budget::{CancelCheck, NeverCancel, ParseBudget};
use crate::mapping::MappingPrecision;
use crate::pre::front::{self, FrontMatter};

/// 一次重写编辑；源码范围必须落在 UTF-8 边界，替换文本只进入解析副本。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewriteEdit {
    source: Range<usize>,
    replacement: String,
    precision: MappingPrecision,
}

impl RewriteEdit {
    /// 构造编辑；反向范围拒绝，边界在 apply 时按源码复验。
    pub fn new(
        source: Range<usize>,
        replacement: String,
        precision: MappingPrecision,
    ) -> Result<Self, RewriteError> {
        if source.start > source.end {
            return Err(RewriteError::ReversedRange);
        }
        Ok(Self {
            source,
            replacement,
            precision,
        })
    }

    /// 返回源码范围。
    pub fn source_range(&self) -> Range<usize> {
        self.source.clone()
    }

    /// 返回替换文本。
    pub fn replacement(&self) -> &str {
        &self.replacement
    }

    /// 返回调用方声明的精确性；apply 会按实际内容修正。
    pub const fn precision(&self) -> MappingPrecision {
        self.precision
    }
}

/// 重写映射片段；rewritten 为空表示隐藏源码，source 为空表示合成内容。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewriteSegment {
    source: Range<usize>,
    rewritten: Range<usize>,
    precision: MappingPrecision,
}

impl RewriteSegment {
    /// 返回源码范围。
    pub fn source_range(&self) -> Range<usize> {
        self.source.clone()
    }
    /// 返回重写文本范围。
    pub fn rewritten_range(&self) -> Range<usize> {
        self.rewritten.clone()
    }
    /// 返回精确性。
    pub const fn precision(&self) -> MappingPrecision {
        self.precision
    }
}

/// 重写范围到源码范围的转换结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewriteSource {
    source: Range<usize>,
    precision: MappingPrecision,
}

impl RewriteSource {
    /// 返回源码范围；Synthetic 时为空锚点。
    pub fn source_range(&self) -> Range<usize> {
        self.source.clone()
    }
    /// 返回转换精确性。
    pub const fn precision(&self) -> MappingPrecision {
        self.precision
    }
}

/// 重写映射的区间关系；多段编辑可组合回原快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RewriteMap {
    segments: Vec<RewriteSegment>,
    rewritten_len: usize,
}

impl RewriteMap {
    /// 构造恒等映射。
    pub fn identity(len: usize) -> Self {
        Self {
            segments: vec![RewriteSegment {
                source: 0..len,
                rewritten: 0..len,
                precision: MappingPrecision::Exact,
            }],
            rewritten_len: len,
        }
    }

    /// 返回全部片段。
    pub fn segments(&self) -> &[RewriteSegment] {
        &self.segments
    }

    /// 返回重写文本长度。
    pub const fn rewritten_len(&self) -> usize {
        self.rewritten_len
    }

    /// 将重写范围映射回源码；跨片段范围返回并集和最强精确性。
    pub fn to_source(&self, range: Range<usize>) -> Result<RewriteSource, RewriteError> {
        if range.start > range.end || range.end > self.rewritten_len {
            return Err(RewriteError::InvalidRange);
        }
        if range.is_empty() {
            return self.anchor_to_source(range.start);
        }
        let mut source_start: Option<usize> = None;
        let mut source_end = 0usize;
        let mut precision = MappingPrecision::Exact;
        for segment in &self.segments {
            if segment.rewritten.end <= range.start {
                continue;
            }
            if segment.rewritten.start >= range.end {
                break;
            }
            let start = segment.rewritten.start.max(range.start);
            let end = segment.rewritten.end.min(range.end);
            if start >= end {
                continue;
            }
            let piece = match segment.precision {
                MappingPrecision::Exact => {
                    let delta_start = start
                        .checked_sub(segment.rewritten.start)
                        .ok_or(RewriteError::LengthOverflow)?;
                    let delta_end = end
                        .checked_sub(segment.rewritten.start)
                        .ok_or(RewriteError::LengthOverflow)?;
                    let source_start = segment
                        .source
                        .start
                        .checked_add(delta_start)
                        .ok_or(RewriteError::LengthOverflow)?;
                    let source_end = segment
                        .source
                        .start
                        .checked_add(delta_end)
                        .ok_or(RewriteError::LengthOverflow)?;
                    source_start..source_end
                }
                MappingPrecision::Aligned
                | MappingPrecision::Hidden
                | MappingPrecision::Synthetic => segment.source.clone(),
            };
            source_start = Some(match source_start {
                Some(value) => value.min(piece.start),
                None => piece.start,
            });
            source_end = source_end.max(piece.end);
            precision = combine_precision(precision, segment.precision);
        }
        match source_start {
            Some(start) => Ok(RewriteSource {
                source: start..source_end,
                precision,
            }),
            None => Err(RewriteError::Unmapped),
        }
    }

    /// 组合两层映射：next 把第二副本映射到第一副本，self 把第一副本映射回源码。
    ///
    /// 复合按两层片段边界求交，保留第一层每一片（含空区间锚点）的来源，不再对整段
    /// 调用会并集化的 [`RewriteMap::to_source`]。空区间配对规则显式如下：
    ///
    /// - 第一层 `rewritten` 为空的隐藏片段：其锚点落在第二层哪个片段，就随该片段
    ///   投影到第二副本的对应点；未被第二层覆盖则丢弃。
    /// - 第二层 `source` 为空的合成片段：锚定到第一层片段在源码中的对应点，
    ///   合成内容不伪造非空来源。
    /// - 第二层片段非 `Exact` 且 `rewritten` 非空时，第二层无法表达逐字节对应，
    ///   该片段内部的交集合并为一片：来源取并集，精确性取较强者。
    /// - 第二层 `rewritten` 为空（隐藏片段）时，逐片保留第一层来源，不合并。
    pub fn compose(&self, next: &RewriteMap) -> Result<RewriteMap, RewriteError> {
        let mut segments = Vec::new();
        segments
            .try_reserve(next.segments.len().saturating_add(self.segments.len()))
            .map_err(RewriteError::Allocation)?;
        for (index, next_segment) in next.segments.iter().enumerate() {
            let middle = next_segment.source.clone();
            if middle.start > middle.end || middle.end > self.rewritten_len {
                return Err(RewriteError::InvalidRange);
            }
            if middle.is_empty() {
                let (source, precision) = self.middle_anchor(middle.start)?;
                segments.push(RewriteSegment {
                    source,
                    rewritten: next_segment.rewritten.clone(),
                    precision: combine_precision(precision, next_segment.precision),
                });
                continue;
            }
            // 每片携带排序键：重写起点、锚点/内容次序、第一层边界点。
            let mut pieces: Vec<(usize, u8, usize, RewriteSegment)> = Vec::new();
            pieces
                .try_reserve(self.segments.len().saturating_add(1))
                .map_err(RewriteError::Allocation)?;
            // 非 Exact 第二层片段的合并结果：源码起点、源码终点、精确性、重写起点。
            let mut merged: Option<(usize, usize, MappingPrecision, usize)> = None;
            let mut intersected = false;
            for segment in &self.segments {
                if segment.rewritten.is_empty() {
                    let point = segment.rewritten.start;
                    if middle_owner(&next.segments, point) == Some(index) {
                        let rewritten = rewritten_point(next_segment, &middle, point)?;
                        pieces.push((
                            rewritten,
                            0,
                            point,
                            RewriteSegment {
                                source: segment.source.clone(),
                                rewritten: rewritten..rewritten,
                                precision: combine_precision(
                                    segment.precision,
                                    next_segment.precision,
                                ),
                            },
                        ));
                    }
                    continue;
                }
                let start = segment.rewritten.start.max(middle.start);
                let end = segment.rewritten.end.min(middle.end);
                if start >= end {
                    continue;
                }
                intersected = true;
                let source = project_source(segment, start, end)?;
                let precision = combine_precision(segment.precision, next_segment.precision);
                if next_segment.precision == MappingPrecision::Exact
                    || next_segment.rewritten.is_empty()
                {
                    let rewritten = project_rewritten(next_segment, start, end)?;
                    pieces.push((
                        rewritten.start,
                        1,
                        start,
                        RewriteSegment {
                            source,
                            rewritten,
                            precision,
                        },
                    ));
                } else {
                    // 非 Exact 的第二层片段无法表达逐字节对应，合并为一片。
                    let rewritten = project_rewritten(next_segment, start, end)?;
                    merged = Some(match merged {
                        Some((merged_start, merged_end, merged_precision, merged_rewritten)) => (
                            merged_start.min(source.start),
                            merged_end.max(source.end),
                            combine_precision(merged_precision, precision),
                            merged_rewritten.min(rewritten.start),
                        ),
                        None => (source.start, source.end, precision, rewritten.start),
                    });
                }
            }
            if !intersected {
                return Err(RewriteError::Unmapped);
            }
            if let Some((start, end, precision, rewritten_start)) = merged {
                pieces.push((
                    rewritten_start,
                    1,
                    middle.start,
                    RewriteSegment {
                        source: start..end,
                        rewritten: next_segment.rewritten.clone(),
                        precision,
                    },
                ));
            }
            pieces.sort_by_key(|(rewritten, rank, point, _)| (*rewritten, *rank, *point));
            segments.extend(pieces.into_iter().map(|(_, _, _, segment)| segment));
        }
        Ok(RewriteMap {
            segments,
            rewritten_len: next.rewritten_len,
        })
    }

    /// 空来源片段锚定到第一层片段；优先取包含该点的片段，否则取在其结束点的片段。
    fn middle_anchor(
        &self,
        point: usize,
    ) -> Result<(Range<usize>, MappingPrecision), RewriteError> {
        let mut fallback: Option<&RewriteSegment> = None;
        for segment in &self.segments {
            if segment.rewritten.start <= point && point < segment.rewritten.end {
                return Ok((source_point(segment, point)?, segment.precision));
            }
            if segment.rewritten.end == point {
                fallback = Some(segment);
            }
        }
        match fallback {
            Some(segment) => Ok((source_point(segment, point)?, segment.precision)),
            None => Err(RewriteError::Unmapped),
        }
    }

    fn anchor_to_source(&self, offset: usize) -> Result<RewriteSource, RewriteError> {
        let mut fallback: Option<&RewriteSegment> = None;
        for segment in &self.segments {
            if segment.precision == MappingPrecision::Hidden {
                fallback = Some(segment);
                continue;
            }
            if segment.rewritten.start <= offset && offset < segment.rewritten.end {
                return self.segment_anchor(segment, offset);
            }
            if offset == segment.rewritten.end {
                fallback = Some(segment);
            }
        }
        match fallback {
            Some(segment) => self.segment_anchor(segment, offset),
            None => Err(RewriteError::Unmapped),
        }
    }

    fn segment_anchor(
        &self,
        segment: &RewriteSegment,
        offset: usize,
    ) -> Result<RewriteSource, RewriteError> {
        let source = match segment.precision {
            MappingPrecision::Exact => {
                let delta = offset
                    .checked_sub(segment.rewritten.start)
                    .ok_or(RewriteError::LengthOverflow)?;
                let start = segment
                    .source
                    .start
                    .checked_add(delta)
                    .ok_or(RewriteError::LengthOverflow)?;
                start..start
            }
            MappingPrecision::Aligned | MappingPrecision::Hidden => segment.source.clone(),
            MappingPrecision::Synthetic => segment.source.clone(),
        };
        Ok(RewriteSource {
            source,
            precision: segment.precision,
        })
    }
}

/// 重写后的解析副本及其到原始源码的映射。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Rewrite {
    text: String,
    map: RewriteMap,
    front_matter: Option<FrontMatter>,
    stripped_bom: bool,
}

impl Rewrite {
    /// 按升序、不重叠的编辑构造解析副本；空插入和空替换保持显式片段。
    pub fn apply(
        source: &str,
        edits: &[RewriteEdit],
        budget: ParseBudget,
    ) -> Result<Self, RewriteError> {
        Self::apply_with_cancel(source, edits, budget, &NeverCancel)
    }

    /// 与 [`Rewrite::apply`] 相同，并在编辑校验与输出循环间轮询取消检查。
    ///
    /// 取消时返回 [`RewriteError::Cancelled`]，不返回部分重写文本。
    pub fn apply_with_cancel(
        source: &str,
        edits: &[RewriteEdit],
        budget: ParseBudget,
        cancel: &dyn CancelCheck,
    ) -> Result<Self, RewriteError> {
        if cancel.is_cancelled() {
            return Err(RewriteError::Cancelled);
        }
        if source.len() > budget.max_source_bytes() {
            return Err(RewriteError::BudgetExceeded);
        }
        let mut cursor = 0usize;
        let mut total = 0usize;
        for edit in edits {
            if cancel.is_cancelled() {
                return Err(RewriteError::Cancelled);
            }
            if edit.source.start < cursor {
                return Err(RewriteError::OverlappingEdits);
            }
            if edit.source.end > source.len() {
                return Err(RewriteError::InvalidRange);
            }
            if !source.is_char_boundary(edit.source.start)
                || !source.is_char_boundary(edit.source.end)
            {
                return Err(RewriteError::NotCharBoundary);
            }
            total = total
                .checked_add(edit.source.start - cursor)
                .and_then(|value| value.checked_add(edit.replacement.len()))
                .ok_or(RewriteError::LengthOverflow)?;
            cursor = edit.source.end;
        }
        total = total
            .checked_add(source.len() - cursor)
            .ok_or(RewriteError::LengthOverflow)?;
        if total > budget.max_rewritten_bytes() {
            return Err(RewriteError::BudgetExceeded);
        }
        let mut text = String::new();
        text.try_reserve_exact(total)
            .map_err(RewriteError::Allocation)?;
        let mut segments = Vec::new();
        segments
            .try_reserve(edits.len().saturating_mul(2).saturating_add(1))
            .map_err(RewriteError::Allocation)?;
        let mut source_cursor = 0usize;
        let mut rewritten_cursor = 0usize;
        for edit in edits {
            if cancel.is_cancelled() {
                return Err(RewriteError::Cancelled);
            }
            if source_cursor < edit.source.start {
                let gap = source_cursor..edit.source.start;
                text.push_str(&source[gap.clone()]);
                let rewritten_end = rewritten_cursor
                    .checked_add(gap.len())
                    .ok_or(RewriteError::LengthOverflow)?;
                segments.push(RewriteSegment {
                    source: gap,
                    rewritten: rewritten_cursor..rewritten_end,
                    precision: MappingPrecision::Exact,
                });
                rewritten_cursor = rewritten_end;
            }
            let replacement = edit.replacement.as_str();
            text.push_str(replacement);
            let rewritten_end = rewritten_cursor
                .checked_add(replacement.len())
                .ok_or(RewriteError::LengthOverflow)?;
            let source_slice = source
                .get(edit.source.clone())
                .ok_or(RewriteError::InvalidRange)?;
            let precision = if edit.source.is_empty() && replacement.is_empty() {
                MappingPrecision::Hidden
            } else if edit.source.is_empty() {
                MappingPrecision::Synthetic
            } else if replacement.is_empty() {
                MappingPrecision::Hidden
            } else if edit.precision == MappingPrecision::Exact && source_slice != replacement {
                MappingPrecision::Aligned
            } else {
                edit.precision
            };
            segments.push(RewriteSegment {
                source: edit.source.clone(),
                rewritten: rewritten_cursor..rewritten_end,
                precision,
            });
            rewritten_cursor = rewritten_end;
            source_cursor = edit.source.end;
        }
        if source_cursor < source.len() {
            let gap = source_cursor..source.len();
            text.push_str(&source[gap.clone()]);
            let rewritten_end = rewritten_cursor
                .checked_add(gap.len())
                .ok_or(RewriteError::LengthOverflow)?;
            segments.push(RewriteSegment {
                source: gap,
                rewritten: rewritten_cursor..rewritten_end,
                precision: MappingPrecision::Exact,
            });
            rewritten_cursor = rewritten_end;
        }
        if text.len() != total || rewritten_cursor != total {
            return Err(RewriteError::LengthOverflow);
        }
        Ok(Self {
            text,
            map: RewriteMap {
                segments,
                rewritten_len: total,
            },
            front_matter: None,
            stripped_bom: false,
        })
    }

    /// 返回解析副本文本。
    pub fn text(&self) -> &str {
        &self.text
    }

    /// 返回到源码的映射。
    pub const fn map(&self) -> &RewriteMap {
        &self.map
    }

    /// 返回识别出的 front matter；未识别时为 None。
    pub const fn front_matter(&self) -> Option<&FrontMatter> {
        self.front_matter.as_ref()
    }

    /// 判断是否剥除了文首 BOM。
    pub const fn stripped_bom(&self) -> bool {
        self.stripped_bom
    }
}

/// 识别 front matter 并只重写解析副本；未闭合标记不产生删除编辑。
pub fn rewrite_front_matter(source: &str, budget: ParseBudget) -> Result<Rewrite, RewriteError> {
    rewrite_front_matter_with_cancel(source, budget, &NeverCancel)
}

/// 与 [`rewrite_front_matter`] 相同，并在识别与重写之间轮询取消检查。
///
/// 取消时返回 [`RewriteError::Cancelled`]，不返回部分重写文本。
pub fn rewrite_front_matter_with_cancel(
    source: &str,
    budget: ParseBudget,
    cancel: &dyn CancelCheck,
) -> Result<Rewrite, RewriteError> {
    if cancel.is_cancelled() {
        return Err(RewriteError::Cancelled);
    }
    let detected = front::detect(source);
    if cancel.is_cancelled() {
        return Err(RewriteError::Cancelled);
    }
    let bom = source.starts_with('\u{feff}');
    let mut edits = Vec::new();
    if let Some(front_matter) = &detected {
        edits.push(RewriteEdit::new(
            front_matter.range(),
            String::new(),
            MappingPrecision::Hidden,
        )?);
    } else if bom {
        edits.push(RewriteEdit::new(
            0..3,
            String::new(),
            MappingPrecision::Hidden,
        )?);
    }
    let mut rewrite = Rewrite::apply_with_cancel(source, &edits, budget, cancel)?;
    rewrite.front_matter = detected;
    rewrite.stripped_bom = bom;
    Ok(rewrite)
}

/// 返回第一副本偏移点所属的第二层片段序号；`Exact` 片段按平移取内部点。
///
/// 第二层片段来源区间按顺序不重叠，因此点归属唯一：优先取包含该点的非空来源片段，
/// 否则取在该点结束的片段。
fn middle_owner(segments: &[RewriteSegment], point: usize) -> Option<usize> {
    let mut fallback = None;
    for (index, segment) in segments.iter().enumerate() {
        let source = &segment.source;
        if source.start <= point && point < source.end {
            return Some(index);
        }
        if source.end == point {
            fallback = Some(index);
        }
    }
    fallback
}

/// 把第一副本的某个源码点投影到第二副本。
///
/// `Exact` 片段逐字节平移；非 `Exact` 片段只能取端点：点位于片段起点时取
/// `rewritten.start`（锚点在前），否则取 `rewritten.end`（锚点在后），保持
/// 隐藏来源与相邻内容的先后关系。
fn rewritten_point(
    segment: &RewriteSegment,
    middle: &Range<usize>,
    point: usize,
) -> Result<usize, RewriteError> {
    match segment.precision {
        MappingPrecision::Exact => {
            let delta = point
                .checked_sub(segment.source.start)
                .ok_or(RewriteError::LengthOverflow)?;
            segment
                .rewritten
                .start
                .checked_add(delta)
                .ok_or(RewriteError::LengthOverflow)
        }
        MappingPrecision::Aligned | MappingPrecision::Hidden | MappingPrecision::Synthetic => {
            if point == middle.start {
                Ok(segment.rewritten.start)
            } else {
                Ok(segment.rewritten.end)
            }
        }
    }
}

/// 把第一副本的源码点投影回源码锚点。
///
/// `Exact` 片段逐字节平移，返回零宽点；其余片段无法定位内部点，只能取端点，且必须返回
/// **零宽**锚点：空来源的第二层片段经 `middle_anchor` 走这里，若返回整段来源就会把
/// `Synthetic` 片伪造成非空来源，与 [`RewriteMap::compose`] 文档「合成内容不伪造非空来源」
/// 冲突。取端点沿用 [`rewritten_point`] 的方向约定：点在片段起点取 `source.start`，
/// 否则取 `source.end`，保持隐藏来源与相邻内容的先后关系。
fn source_point(segment: &RewriteSegment, point: usize) -> Result<Range<usize>, RewriteError> {
    match segment.precision {
        MappingPrecision::Exact => {
            let delta = point
                .checked_sub(segment.rewritten.start)
                .ok_or(RewriteError::LengthOverflow)?;
            let start = segment
                .source
                .start
                .checked_add(delta)
                .ok_or(RewriteError::LengthOverflow)?;
            Ok(start..start)
        }
        MappingPrecision::Aligned | MappingPrecision::Hidden | MappingPrecision::Synthetic => {
            let anchor = if point == segment.rewritten.start {
                segment.source.start
            } else {
                segment.source.end
            };
            Ok(anchor..anchor)
        }
    }
}

/// 取第一层片段与交集的源码范围；`Exact` 逐字节平移，其余取整段来源。
fn project_source(
    segment: &RewriteSegment,
    start: usize,
    end: usize,
) -> Result<Range<usize>, RewriteError> {
    match segment.precision {
        MappingPrecision::Exact => {
            let delta_start = start
                .checked_sub(segment.rewritten.start)
                .ok_or(RewriteError::LengthOverflow)?;
            let delta_end = end
                .checked_sub(segment.rewritten.start)
                .ok_or(RewriteError::LengthOverflow)?;
            let source_start = segment
                .source
                .start
                .checked_add(delta_start)
                .ok_or(RewriteError::LengthOverflow)?;
            let source_end = segment
                .source
                .start
                .checked_add(delta_end)
                .ok_or(RewriteError::LengthOverflow)?;
            Ok(source_start..source_end)
        }
        MappingPrecision::Aligned | MappingPrecision::Hidden | MappingPrecision::Synthetic => {
            Ok(segment.source.clone())
        }
    }
}

/// 取第二层片段与交集的重写范围；`Exact` 逐字节平移，其余整段无法细分。
fn project_rewritten(
    segment: &RewriteSegment,
    start: usize,
    end: usize,
) -> Result<Range<usize>, RewriteError> {
    match segment.precision {
        MappingPrecision::Exact => {
            let delta_start = start
                .checked_sub(segment.source.start)
                .ok_or(RewriteError::LengthOverflow)?;
            let delta_end = end
                .checked_sub(segment.source.start)
                .ok_or(RewriteError::LengthOverflow)?;
            let rewritten_start = segment
                .rewritten
                .start
                .checked_add(delta_start)
                .ok_or(RewriteError::LengthOverflow)?;
            let rewritten_end = segment
                .rewritten
                .start
                .checked_add(delta_end)
                .ok_or(RewriteError::LengthOverflow)?;
            Ok(rewritten_start..rewritten_end)
        }
        MappingPrecision::Aligned | MappingPrecision::Hidden | MappingPrecision::Synthetic => {
            Ok(segment.rewritten.clone())
        }
    }
}

fn combine_precision(left: MappingPrecision, right: MappingPrecision) -> MappingPrecision {
    fn rank(precision: MappingPrecision) -> u8 {
        match precision {
            MappingPrecision::Exact => 0,
            MappingPrecision::Aligned => 1,
            MappingPrecision::Hidden => 2,
            MappingPrecision::Synthetic => 3,
        }
    }
    if rank(left) >= rank(right) {
        left
    } else {
        right
    }
}

/// 重写构造或映射失败；失败时调用方必须回到原文视图。
#[derive(Debug)]
pub enum RewriteError {
    /// 源码范围反向或越界。
    ReversedRange,
    /// 编辑范围越界。
    InvalidRange,
    /// 源码端点位于 UTF-8 标量内部。
    NotCharBoundary,
    /// 编辑未按升序排列或相互重叠。
    OverlappingEdits,
    /// 字节或范围预算耗尽。
    BudgetExceeded,
    /// 重写范围没有对应源码片段。
    Unmapped,
    /// 长度累计溢出。
    LengthOverflow,
    /// 受检分配失败。
    Allocation(std::collections::TryReserveError),
    /// 调用方请求取消；没有返回部分结果。
    Cancelled,
}

impl fmt::Display for RewriteError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for RewriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}
