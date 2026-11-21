//! Unicode17 字素边界适配；所有提供给第三方游标的字节在调用前计费。

use std::fmt;
use std::ops::Range;

use unicode_segmentation::{GraphemeCursor, GraphemeIncomplete};

use aline_buffer::{Affinity, SrcOffset, TextBudget, TextSnapshot};

use super::{DrawnOffset, MappingError, MappingPrecision, Projection};

const SCAN_LIMIT: usize = 4096;
const CHUNK_LIMIT: usize = 256;

/// 字素查询失败；不返回截断、猜测的边界，也不改变投影。
#[derive(Debug)]
pub enum GraphemeError {
    /// 坐标不属于当前投影或未通过其 UTF-8 校验。
    Mapping(MappingError),
    /// 投影只含源码子范围，缺少可靠的字素前后文。
    IncompleteProjection,
    /// 移动起点是合法 scalar 端点，但不是 Unicode17 字素边界。
    NotGraphemeBoundary,
    /// 下一次供给会超过本次查询的累计预算；尚未执行该供给。
    ScanBudgetExceeded {
        /// 已交给游标的累计 UTF-8 字节；重复供给重复计数。
        supplied_bytes: usize,
        /// 被拒绝的下一块长度，最多 256 字节。
        next_bytes: usize,
        /// 每查询的累计供给上限，当前为 4096 字节。
        limit: usize,
    },
    /// 第三方游标请求不符合已验证的完整文本/分块契约。
    InvalidCursorState,
}

impl fmt::Display for GraphemeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for GraphemeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Mapping(error) => Some(error),
            _ => None,
        }
    }
}

impl Projection {
    /// 判断绑定当前完整投影的 scalar 端点是否为 Unicode17 扩展字素边界。
    ///
    /// Source 使用完整 identity 投影，Visual 使用完整解析投影。旧投影和缺失
    /// 上下文拒绝；累计供给超过 4096 字节前拒绝，每块最多 256 字节，含重复回扫。
    pub fn is_grapheme_boundary(&self, offset: &DrawnOffset) -> Result<bool, GraphemeError> {
        self.grapheme_navigator(offset)?.boundary()
    }

    /// 返回下一 Unicode17 字素边界；末端返回 `None`。
    ///
    /// 起点必须是当前完整投影上的字素边界。起点校验与移动共享 4096 字节供给
    /// 预算，耗尽时不返回偏移。结果仍是绘制坐标；源码定位必须继续经 `source_at`。
    pub fn next_grapheme_boundary(
        &self,
        offset: &DrawnOffset,
    ) -> Result<Option<DrawnOffset>, GraphemeError> {
        self.move_grapheme(offset, true)
    }

    /// 返回上一 Unicode17 字素边界；开头返回 `None`。
    ///
    /// 起点校验、预算及源码映射要求与 `next_grapheme_boundary` 相同；此查询不证明
    /// 两个端点之间可以安全删除，删除仍须检查来源完整性与保护范围。
    pub fn previous_grapheme_boundary(
        &self,
        offset: &DrawnOffset,
    ) -> Result<Option<DrawnOffset>, GraphemeError> {
        self.move_grapheme(offset, false)
    }

    /// 在完整源码上下文中判断当前局部恒等投影的一个端点。
    ///
    /// 映射必须是 Exact，不把局部片段首尾误判为字素边界。源码视图每次最多只读取受预算的小段。
    pub fn is_source_grapheme_boundary(&self, offset: &SrcOffset) -> Result<bool, GraphemeError> {
        self.source_grapheme_navigator(offset)?.boundary()
    }

    /// 在完整源码上下文中取相邻字素端点。返回值仍须落在当前局部恒等投影的 Exact 范围内。
    pub fn adjacent_source_grapheme_boundary(
        &self,
        offset: &SrcOffset,
        forward: bool,
    ) -> Result<Option<SrcOffset>, GraphemeError> {
        let mut navigator = self.source_grapheme_navigator(offset)?;
        if !navigator.boundary()? {
            return Err(GraphemeError::NotGraphemeBoundary);
        }
        let next = if forward {
            navigator.scan(GraphemeCursor::next_boundary)?
        } else {
            navigator.scan(GraphemeCursor::prev_boundary)?
        };
        let Some(next) = next else {
            return Ok(None);
        };
        let source = self
            .source_snapshot()
            .src_offset(next)
            .map_err(|error| GraphemeError::Mapping(MappingError::Source(error)))?;
        self.check_source_grapheme_mapping(&source)?;
        Ok(Some(source))
    }

    fn check_source_grapheme_mapping(&self, offset: &SrcOffset) -> Result<(), GraphemeError> {
        if self
            .drawn_at(offset, Affinity::Before)
            .map_err(GraphemeError::Mapping)?
            .precision()
            != MappingPrecision::Exact
        {
            return Err(GraphemeError::IncompleteProjection);
        }
        Ok(())
    }

    fn source_grapheme_navigator<'a>(
        &'a self,
        offset: &SrcOffset,
    ) -> Result<Navigator<'a>, GraphemeError> {
        self.check_source_grapheme_mapping(offset)?;
        let snapshot = self.source_snapshot();
        let byte = snapshot
            .validate_offset(offset)
            .map_err(|error| GraphemeError::Mapping(MappingError::Source(error)))?;
        Ok(Navigator::for_context(TextContext::Source(snapshot), byte))
    }

    fn move_grapheme(
        &self,
        offset: &DrawnOffset,
        forward: bool,
    ) -> Result<Option<DrawnOffset>, GraphemeError> {
        let mut navigator = self.grapheme_navigator(offset)?;
        if !navigator.boundary()? {
            return Err(GraphemeError::NotGraphemeBoundary);
        }
        let next = if forward {
            navigator.scan(GraphemeCursor::next_boundary)?
        } else {
            navigator.scan(GraphemeCursor::prev_boundary)?
        };
        next.map(|byte| self.drawn_offset(byte).map_err(GraphemeError::Mapping))
            .transpose()
    }

    fn grapheme_navigator(&self, offset: &DrawnOffset) -> Result<Navigator<'_>, GraphemeError> {
        if offset.projection_id() != self.id() {
            return Err(GraphemeError::Mapping(MappingError::ProjectionMismatch));
        }
        self.drawn_offset(offset.byte_offset())
            .map_err(GraphemeError::Mapping)?;
        if !self.has_full_source_context() {
            return Err(GraphemeError::IncompleteProjection);
        }
        Ok(Navigator::new(self.drawn_text(), offset.byte_offset()))
    }
}

#[derive(Clone, Copy)]
enum TextContext<'a> {
    Drawn(&'a str),
    Source(&'a TextSnapshot),
}

impl TextContext<'_> {
    fn len(self) -> usize {
        match self {
            Self::Drawn(text) => text.len(),
            Self::Source(snapshot) => snapshot.len_bytes(),
        }
    }

    fn is_char_boundary(self, byte: usize) -> bool {
        match self {
            Self::Drawn(text) => text.is_char_boundary(byte),
            Self::Source(snapshot) => snapshot.is_char_boundary(byte).unwrap_or(false),
        }
    }
}

impl<'a> TextContext<'a> {
    fn slice(self, bytes: Range<usize>) -> Result<std::borrow::Cow<'a, str>, GraphemeError> {
        match self {
            Self::Drawn(text) => text
                .get(bytes)
                .map(std::borrow::Cow::Borrowed)
                .ok_or(GraphemeError::InvalidCursorState),
            Self::Source(snapshot) => {
                let range = snapshot
                    .src_range(bytes)
                    .map_err(|error| GraphemeError::Mapping(MappingError::Source(error)))?;
                snapshot
                    .slice(&range, TextBudget::new(CHUNK_LIMIT))
                    .map_err(|error| GraphemeError::Mapping(MappingError::Source(error)))
            }
        }
    }
}

struct Navigator<'a> {
    text: TextContext<'a>,
    cursor: GraphemeCursor,
    chunk: Range<usize>,
    supplied: usize,
}

impl<'a> Navigator<'a> {
    fn new(text: &'a str, byte: usize) -> Self {
        Self::for_context(TextContext::Drawn(text), byte)
    }

    fn for_context(text: TextContext<'a>, byte: usize) -> Self {
        // 保留一部分前文，避免短簇反复回扫。源码只在所需块跨 overlay 接缝时复制。
        let mut start = byte.saturating_sub(CHUNK_LIMIT / 2);
        while !text.is_char_boundary(start) {
            start += 1;
        }
        Self {
            text,
            cursor: GraphemeCursor::new(byte, text.len(), true),
            chunk: following_chunk(text, start),
            supplied: 0,
        }
    }

    fn boundary(&mut self) -> Result<bool, GraphemeError> {
        self.scan(GraphemeCursor::is_boundary)
    }

    fn charge(&mut self, bytes: usize) -> Result<(), GraphemeError> {
        if bytes > CHUNK_LIMIT || bytes > SCAN_LIMIT.saturating_sub(self.supplied) {
            return Err(GraphemeError::ScanBudgetExceeded {
                supplied_bytes: self.supplied,
                next_bytes: bytes,
                limit: SCAN_LIMIT,
            });
        }
        self.supplied += bytes;
        debug_assert!(self.supplied <= SCAN_LIMIT);
        Ok(())
    }

    fn scan<T>(
        &mut self,
        query: impl Fn(&mut GraphemeCursor, &str, usize) -> Result<T, GraphemeIncomplete>,
    ) -> Result<T, GraphemeError> {
        loop {
            self.charge(self.chunk.len())?;
            let chunk = self.text.slice(self.chunk.clone())?;
            match query(&mut self.cursor, &chunk, self.chunk.start) {
                Ok(value) => return Ok(value),
                Err(GraphemeIncomplete::PreContext(end)) => {
                    if end == 0 || end > self.text.len() || !self.text.is_char_boundary(end) {
                        return Err(GraphemeError::InvalidCursorState);
                    }
                    let context_range = preceding_chunk(self.text, end);
                    self.charge(context_range.len())?;
                    let context = self.text.slice(context_range.clone())?;
                    self.cursor.provide_context(&context, context_range.start);
                }
                Err(GraphemeIncomplete::NextChunk) if self.chunk.end < self.text.len() => {
                    self.chunk = following_chunk(self.text, self.chunk.end);
                }
                Err(GraphemeIncomplete::PrevChunk) if self.chunk.start > 0 => {
                    self.chunk = preceding_chunk(self.text, self.chunk.start);
                }
                Err(_) => return Err(GraphemeError::InvalidCursorState),
            }
        }
    }
}

fn following_chunk(text: TextContext, start: usize) -> Range<usize> {
    let mut end = start.saturating_add(CHUNK_LIMIT).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    start..end
}

fn preceding_chunk(text: TextContext, end: usize) -> Range<usize> {
    let mut start = end.saturating_sub(CHUNK_LIMIT);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    start..end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supplied_bytes_are_charged_before_every_call_and_include_retries()
    -> Result<(), GraphemeError> {
        let mut navigator = Navigator::new("abcd", 1);
        assert!(navigator.boundary()?);
        assert_eq!(navigator.supplied, 4);
        assert_eq!(navigator.scan(GraphemeCursor::next_boundary)?, Some(2));
        assert_eq!(navigator.supplied, 8);
        navigator.supplied = SCAN_LIMIT;
        assert!(matches!(
            navigator.charge(1),
            Err(GraphemeError::ScanBudgetExceeded {
                supplied_bytes: SCAN_LIMIT,
                next_bytes: 1,
                ..
            })
        ));
        assert_eq!(navigator.supplied, SCAN_LIMIT);
        Ok(())
    }
}
