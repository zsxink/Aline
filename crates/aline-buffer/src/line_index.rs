//! 源码字节行索引；只按 LF 分行，不解析 Markdown，也不规范化 CR/LF。

use std::ops::Range;

use crate::BufferError;

/// 行索引允许的最大行数，包含末尾换行产生的空行。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineIndexBudget {
    max_lines: usize,
}

impl LineIndexBudget {
    /// 创建由宿主注入的行数上限，不声明发布资源阈值。
    pub const fn new(max_lines: usize) -> Self {
        Self { max_lines }
    }

    /// 返回注入的行数上限。
    pub const fn max_lines(self) -> usize {
        self.max_lines
    }
}

/// 与一份源码文本同步构建的行起点和内容范围。
///
/// 行内容范围不含末尾 `\n`；若 `\n` 前紧邻 `\r`，两者一并排除。
/// 孤立 `\r` 不是换行。文本末尾的 `\n` 会产生一个空行。
#[derive(Clone, Debug)]
pub struct LineIndex {
    starts: Vec<usize>,
    content_ends: Vec<usize>,
    len: usize,
}

impl LineIndex {
    /// 在分配前检查行数预算，再建立全部行起点与内容终点。
    pub fn build(text: &str, budget: LineIndexBudget) -> Result<Self, BufferError> {
        let bytes = text.as_bytes();
        let mut count = 1usize;
        for byte in bytes {
            if *byte == b'\n' {
                count = count.checked_add(1).ok_or(BufferError::LengthOverflow)?;
            }
        }
        if count > budget.max_lines() {
            return Err(BufferError::LineCountExceeded {
                count,
                limit: budget.max_lines(),
            });
        }
        let mut starts = Vec::new();
        starts
            .try_reserve_exact(count)
            .map_err(BufferError::Allocation)?;
        starts.push(0);
        for (offset, byte) in bytes.iter().enumerate() {
            if *byte == b'\n' {
                starts.push(offset.checked_add(1).ok_or(BufferError::LengthOverflow)?);
            }
        }
        let mut content_ends = Vec::new();
        content_ends
            .try_reserve_exact(starts.len())
            .map_err(BufferError::Allocation)?;
        for (line, start) in starts.iter().enumerate() {
            let mut end = starts.get(line + 1).copied().unwrap_or(bytes.len());
            if end > *start && bytes[end - 1] == b'\n' {
                end -= 1;
                if end > *start && bytes[end - 1] == b'\r' {
                    end -= 1;
                }
            }
            content_ends.push(end);
        }
        Ok(Self {
            starts,
            content_ends,
            len: bytes.len(),
        })
    }

    /// 返回行数，包含末尾换行后的空行。
    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// 返回建立索引时的源码字节长度。
    pub const fn len_bytes(&self) -> usize {
        self.len
    }

    /// 返回某行的内容字节范围，不含行尾 LF/CRLF；行号越界返回 `None`。
    pub fn line_range(&self, line: usize) -> Option<Range<usize>> {
        let start = *self.starts.get(line)?;
        let end = *self.content_ends.get(line)?;
        Some(start..end)
    }

    /// 返回字节偏移所在的行号；换行字节归属其所在行，文本末尾归属最后一行。
    pub fn line_of_offset(&self, offset: usize) -> Option<usize> {
        if offset > self.len {
            return None;
        }
        match self.starts.binary_search(&offset) {
            Ok(line) => Some(line),
            Err(line) => Some(line.saturating_sub(1)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{LineIndex, LineIndexBudget};
    use crate::BufferError;

    #[test]
    fn line_ranges_exclude_crlf_and_keep_isolated_cr() -> Result<(), BufferError> {
        let index = LineIndex::build("a\r\nb\rc\n", LineIndexBudget::new(10))?;
        assert_eq!(index.line_count(), 3);
        assert_eq!(index.line_range(0), Some(0..1));
        assert_eq!(index.line_range(1), Some(3..6));
        assert_eq!(index.line_range(2), Some(7..7));
        Ok(())
    }

    #[test]
    fn offset_at_line_start_belongs_to_new_line() -> Result<(), BufferError> {
        let index = LineIndex::build("a\nb", LineIndexBudget::new(10))?;
        assert_eq!(index.line_of_offset(0), Some(0));
        assert_eq!(index.line_of_offset(1), Some(0));
        assert_eq!(index.line_of_offset(2), Some(1));
        assert_eq!(index.line_of_offset(3), Some(1));
        assert_eq!(index.line_of_offset(4), None);
        Ok(())
    }

    #[test]
    fn line_budget_is_checked_before_publication() {
        assert!(matches!(
            LineIndex::build("a\nb\nc", LineIndexBudget::new(2)),
            Err(BufferError::LineCountExceeded { count: 3, limit: 2 })
        ));
    }
}
