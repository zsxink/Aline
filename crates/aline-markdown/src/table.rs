//! 表格源码切片索引。语义和单元格边界来自当前解析树，原始片段保留供编辑计划重排。
//! 不重建 Markdown，不持有编辑状态，也不执行写入或 I/O。

use std::fmt;

use aline_buffer::{BufferError, SrcRange, TextBudget};

use crate::{Parsed, PresentationKind, PresentationNode, TableAlignment};

/// 表格源码范围读取失败。
#[derive(Debug)]
pub enum TableSourceError {
    /// 范围或读取预算不合法。
    Source(BufferError),
    /// 目标不是当前解析中可完整识别的表格。
    InvalidTable,
}

impl fmt::Display for TableSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}
impl std::error::Error for TableSourceError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::InvalidTable => None,
        }
    }
}
impl From<BufferError> for TableSourceError {
    fn from(value: BufferError) -> Self {
        Self::Source(value)
    }
}

/// 同一源码快照中的一个单元格，包含其原始边界空格。
#[derive(Clone, Debug)]
pub struct SourceCell {
    source: SrcRange,
    present: bool,
}
impl SourceCell {
    /// 返回原单元格片段；短行缺失格为空锚点。
    pub fn source_range(&self) -> &SrcRange {
        &self.source
    }
    /// 返回此格是否实际出现在原行中。
    pub const fn is_present(&self) -> bool {
        self.present
    }
}

/// 表格的一条源行；范围排除容器前缀与行尾。
#[derive(Clone, Debug)]
pub struct SourceRow {
    source: SrcRange,
    prefix: SrcRange,
    cells: Vec<SourceCell>,
    line_ending: &'static str,
    outer_start: bool,
    outer_end: bool,
}
impl SourceRow {
    /// 返回表格行正文，排除引用或列表前缀和行尾。
    pub fn source_range(&self) -> &SrcRange {
        &self.source
    }
    /// 返回物理行首到表格行正文之间的容器前缀。
    pub fn prefix_range(&self) -> &SrcRange {
        &self.prefix
    }
    /// 返回按语义列顺序排列的单元格。
    pub fn cells(&self) -> &[SourceCell] {
        &self.cells
    }
    /// 返回原行尾，取值为空、LF 或 CRLF。
    pub const fn line_ending(&self) -> &'static str {
        self.line_ending
    }
    /// 返回原行是否带左侧竖线。
    pub const fn has_outer_start(&self) -> bool {
        self.outer_start
    }
    /// 返回原行是否带右侧竖线。
    pub const fn has_outer_end(&self) -> bool {
        self.outer_end
    }
}

/// 绑定当前解析快照的表格源码索引。
#[derive(Clone, Debug)]
pub struct SourceTable {
    source: SrcRange,
    rows: Vec<SourceRow>,
    separator: SourceRow,
    alignments: Vec<TableAlignment>,
}
impl SourceTable {
    /// 返回解析事件中的完整表格范围。
    pub fn source_range(&self) -> &SrcRange {
        &self.source
    }
    /// 返回表头和正文行；表头索引为零。
    pub fn rows(&self) -> &[SourceRow] {
        &self.rows
    }
    /// 返回第二物理行的对齐分隔行。
    pub fn separator(&self) -> &SourceRow {
        &self.separator
    }
    /// 返回当前各列的语义对齐。
    pub fn alignments(&self) -> &[TableAlignment] {
        &self.alignments
    }
}

/// 读取当前解析表格的原始行和逐格范围，拒绝外来快照与不可保真的额外列。
///
/// # Errors
/// 目标身份或范围错误、读取超预算、目标不是表格或源行结构不明确时返回错误。
pub fn table_source(
    parsed: &Parsed,
    target: &SrcRange,
    budget: TextBudget,
) -> Result<SourceTable, TableSourceError> {
    parsed.snapshot().validate_range(target)?;
    let node = find_table(parsed.presentation(), target).ok_or(TableSourceError::InvalidTable)?;
    if node.source_range() != target {
        let suffix = parsed
            .snapshot()
            .src_range(target.byte_range().end..node.source_range().byte_range().end)?;
        let suffix = parsed.snapshot().slice(&suffix, budget)?;
        if suffix.as_ref() != "\n" && suffix.as_ref() != "\r\n" {
            return Err(TableSourceError::InvalidTable);
        }
    }
    let PresentationKind::Table { alignments } = node.kind() else {
        return Err(TableSourceError::InvalidTable);
    };
    let whole = parsed
        .snapshot()
        .src_range(0..parsed.snapshot().len_bytes())?;
    let source = parsed.snapshot().slice(&whole, budget)?;
    let mut rows = Vec::new();
    let head_start = node
        .children()
        .first()
        .ok_or(TableSourceError::InvalidTable)?
        .source_range()
        .byte_range()
        .start;
    let head_line_start = source
        .get(..head_start)
        .and_then(|before| before.rfind('\n').map(|at| at + 1))
        .unwrap_or(0);
    let prefix_width = head_start - head_line_start;
    for row in node.children() {
        if !matches!(
            row.kind(),
            PresentationKind::TableHead | PresentationKind::TableRow
        ) {
            return Err(TableSourceError::InvalidTable);
        }
        rows.push(read_row(
            parsed,
            &source,
            row,
            alignments.len(),
            prefix_width,
        )?);
    }
    let head = rows.first().ok_or(TableSourceError::InvalidTable)?;
    let head_end = head
        .source
        .byte_range()
        .end
        .checked_add(head.line_ending.len())
        .ok_or(TableSourceError::InvalidTable)?;
    let separator_end = source
        .get(head_end..)
        .and_then(|tail| tail.find('\n').map(|length| head_end + length + 1))
        .unwrap_or(source.len());
    let separator_body_start = head_end
        .checked_add(prefix_width)
        .ok_or(TableSourceError::InvalidTable)?;
    let separator = read_separator(
        parsed,
        &source,
        separator_body_start..separator_end,
        alignments.len(),
    )?;
    Ok(SourceTable {
        source: node.source_range().clone(),
        rows,
        separator,
        alignments: alignments.clone(),
    })
}

fn find_table<'a>(
    nodes: &'a [PresentationNode],
    target: &SrcRange,
) -> Option<&'a PresentationNode> {
    nodes.iter().find_map(|node| {
        if matches!(node.kind(), PresentationKind::Table { .. })
            && node.source_range().snapshot_id() == target.snapshot_id()
            && node.source_range().byte_range().start == target.byte_range().start
            && node.source_range().byte_range().end >= target.byte_range().end
            && node.source_range().byte_range().end - target.byte_range().end <= 2
        {
            Some(node)
        } else {
            find_table(node.children(), target)
        }
    })
}

fn line_parts(
    source: &str,
    range: std::ops::Range<usize>,
) -> Result<(std::ops::Range<usize>, &'static str), TableSourceError> {
    let raw = source
        .get(range.clone())
        .ok_or(TableSourceError::InvalidTable)?;
    let ending = if raw.ends_with("\r\n") {
        "\r\n"
    } else if raw.ends_with('\n') {
        "\n"
    } else {
        ""
    };
    let end = range
        .end
        .checked_sub(ending.len())
        .ok_or(TableSourceError::InvalidTable)?;
    if source
        .get(range.start..end)
        .ok_or(TableSourceError::InvalidTable)?
        .contains(['\n', '\r'])
    {
        return Err(TableSourceError::InvalidTable);
    }
    Ok((range.start..end, ending))
}

fn read_row(
    parsed: &Parsed,
    source: &str,
    row: &PresentationNode,
    columns: usize,
    prefix_width: usize,
) -> Result<SourceRow, TableSourceError> {
    let (mut range, ending) = line_parts(source, row.source_range().byte_range())?;
    let line_start = source
        .get(..range.start)
        .and_then(|before| before.rfind('\n').map(|at| at + 1))
        .unwrap_or(0);
    range.start = line_start
        .checked_add(prefix_width)
        .ok_or(TableSourceError::InvalidTable)?;
    if range.start > row.source_range().byte_range().start {
        return Err(TableSourceError::InvalidTable);
    }
    let mut cells = Vec::new();
    let mut previous = range.start;
    for cell in row.children() {
        if !matches!(cell.kind(), PresentationKind::TableCell) {
            return Err(TableSourceError::InvalidTable);
        }
        let raw = cell.source_range().byte_range();
        let present = raw.start < range.end && raw.end <= range.end;
        let cell_range = if present { raw } else { range.end..range.end };
        if present {
            let gap = source
                .get(previous..cell_range.start)
                .ok_or(TableSourceError::InvalidTable)?;
            if !gap.chars().all(|c| c == '|' || c == ' ' || c == '\t') {
                return Err(TableSourceError::InvalidTable);
            }
            previous = cell_range.end;
        }
        cells.push(SourceCell {
            source: parsed.snapshot().src_range(cell_range)?,
            present,
        });
    }
    if cells.len() != columns {
        return Err(TableSourceError::InvalidTable);
    }
    let tail = source
        .get(previous..range.end)
        .ok_or(TableSourceError::InvalidTable)?;
    let closing_pipes = tail.bytes().filter(|byte| *byte == b'|').count();
    if !tail.chars().all(|c| c == '|' || c == ' ' || c == '\t') || closing_pipes > 1 {
        return Err(TableSourceError::InvalidTable);
    }
    let body = source
        .get(range.clone())
        .ok_or(TableSourceError::InvalidTable)?;
    Ok(SourceRow {
        source: parsed.snapshot().src_range(range.clone())?,
        prefix: parsed.snapshot().src_range(line_start..range.start)?,
        cells,
        line_ending: ending,
        outer_start: body.trim_start_matches([' ', '\t']).starts_with('|'),
        // 转义竖线包含在末个实际格内，只有格范围之外的竖线才是右侧分隔符。
        outer_end: closing_pipes == 1,
    })
}

fn read_separator(
    parsed: &Parsed,
    source: &str,
    raw: std::ops::Range<usize>,
    columns: usize,
) -> Result<SourceRow, TableSourceError> {
    let (range, ending) = line_parts(source, raw)?;
    let body = source
        .get(range.clone())
        .ok_or(TableSourceError::InvalidTable)?;
    let start_pipe = body.trim_start_matches([' ', '\t']).starts_with('|');
    let end_pipe = body.trim_end_matches([' ', '\t']).ends_with('|');
    let content_start = if start_pipe {
        range.start + body.find('|').ok_or(TableSourceError::InvalidTable)? + 1
    } else {
        range.start
    };
    let content_end = if end_pipe {
        range.start + body.rfind('|').ok_or(TableSourceError::InvalidTable)?
    } else {
        range.end
    };
    let mut cells = Vec::new();
    let mut start = content_start;
    for end in source
        .get(content_start..content_end)
        .ok_or(TableSourceError::InvalidTable)?
        .match_indices('|')
        .map(|(offset, _)| content_start + offset)
        .chain(std::iter::once(content_end))
    {
        let raw_cell = source
            .get(start..end)
            .ok_or(TableSourceError::InvalidTable)?;
        if !raw_cell
            .chars()
            .all(|c| matches!(c, ' ' | '\t' | ':' | '-'))
            || !raw_cell.contains('-')
        {
            return Err(TableSourceError::InvalidTable);
        }
        cells.push(SourceCell {
            source: parsed.snapshot().src_range(start..end)?,
            present: true,
        });
        start = end + 1;
    }
    if cells.len() != columns {
        return Err(TableSourceError::InvalidTable);
    }
    let line_start = source
        .get(..range.start)
        .and_then(|before| before.rfind('\n').map(|at| at + 1))
        .unwrap_or(0);
    Ok(SourceRow {
        source: parsed.snapshot().src_range(range.clone())?,
        prefix: parsed.snapshot().src_range(line_start..range.start)?,
        cells,
        line_ending: ending,
        outer_start: start_pipe,
        outer_end: end_pipe,
    })
}
