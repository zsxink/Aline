//! 表格结构命令的纯计划。仅重排当前解析所绑定的源片段，复读核对目标网格。
//! 源码、输入组和历史由 DocCore 持有，本模块不写缓冲，不访问窗口或磁盘。

use std::ops::Range;

use aline_buffer::{SelectionSpec, SrcRange, SrcSelection, TextBudget};
use aline_markdown::{
    Parsed, PresentationKind, PresentationNode, SourceRow, SourceTable, TableAlignment,
    TableSourceError, table_source,
};

use crate::EditBudget;
use crate::command::{EditContext, EditMode, GroupReason, SourceEdit};
use crate::errors::map_buffer_error;
use crate::plan::{NoChangeReason, PlanOutcome, StructuralChange, plan_structural, precheck};
use crate::refusal::{BudgetKind, EditRefusal, RereadFailure};
use crate::reread::{Candidate, RereadContract};

/// 绑定源码快照的目标单元格。行零为表头，正文行从一开始。
#[derive(Clone, Debug)]
pub struct TableTarget {
    table: SrcRange,
    row: usize,
    column: usize,
}
impl TableTarget {
    /// 创建目标；表格身份和行列边界在命令入口受检。
    pub const fn new(table: SrcRange, row: usize, column: usize) -> Self {
        Self { table, row, column }
    }
    /// 返回表格源码范围。
    pub fn table_range(&self) -> &SrcRange {
        &self.table
    }
    /// 返回包含表头的行索引。
    pub const fn row(&self) -> usize {
        self.row
    }
    /// 返回列索引。
    pub const fn column(&self) -> usize {
        self.column
    }
}

/// 一次表格结构意图。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableOperation {
    /// 在目标行前或后插入空正文行。
    InsertRow {
        /// 是否插在目标行之后。
        after: bool,
    },
    /// 删除目标正文行；表头使用表头调整或整表删除入口。
    DeleteRow,
    /// 将正文行移动到指定行索引；零为表头且不可作为移动目标。
    MoveRow {
        /// 包含表头的目标行索引。
        to: usize,
    },
    /// 在目标列左侧或右侧插入空列。
    InsertColumn {
        /// 是否插在目标列之后。
        after: bool,
    },
    /// 删除目标列；最后一列须使用整表删除入口。
    DeleteColumn,
    /// 将目标列移动到指定列索引，对齐随列移动。
    MoveColumn {
        /// 移动后的列索引。
        to: usize,
    },
    /// 设置目标列对齐。
    Align(TableAlignment),
    /// 将目标正文行与表头互换，分隔行和各列对齐留在原位。
    PromoteHeader,
    /// 按冻结风格整理边界空格、外侧竖线及短行。
    Tidy,
    /// 删除整个表格。
    DeleteTable,
}

/// 目标与结构操作组成的只读命令。
#[derive(Clone, Debug)]
pub struct TableCommand {
    target: TableTarget,
    operation: TableOperation,
}
impl TableCommand {
    /// 创建结构命令。
    pub const fn new(target: TableTarget, operation: TableOperation) -> Self {
        Self { target, operation }
    }
    /// 返回目标。
    pub fn target(&self) -> &TableTarget {
        &self.target
    }
    /// 返回操作。
    pub const fn operation(&self) -> TableOperation {
        self.operation
    }
}

/// Tab 导航结果；已有单元格移动不产生文本计划。
#[derive(Clone, Debug)]
pub enum TableNavigation {
    /// 在已有单元格中移动源选区。
    Cell {
        /// 目标格身份；短行中多个缺失格可能共用同一源选区。
        target: TableTarget,
        /// 目标源选区。
        selection: SelectionSpec,
    },
    /// 末格向后导航需要独立追加空行。
    AppendRow(TableCommand),
    /// 首格向前导航已经到达表格边界。
    Boundary,
}

/// 解析表格导航，普通格只返回选区，末格返回追加行意图。
///
/// # Errors
/// 目标身份过期、行列非法或表格源片段不明确时拒绝。
pub fn table_navigation(
    parsed: &Parsed,
    target: &TableTarget,
    backward: bool,
    budget: TextBudget,
) -> Result<TableNavigation, EditRefusal> {
    let table = table_source(parsed, &target.table, budget).map_err(map_table_error)?;
    ensure_target(&table, target)?;
    let columns = table.alignments().len();
    let next = if backward {
        if target.column > 0 {
            Some((target.row, target.column - 1))
        } else if target.row > 0 {
            Some((target.row - 1, columns - 1))
        } else {
            None
        }
    } else if target.column + 1 < columns {
        Some((target.row, target.column + 1))
    } else if target.row + 1 < table.rows().len() {
        Some((target.row + 1, 0))
    } else {
        return Ok(TableNavigation::AppendRow(TableCommand::new(
            target.clone(),
            TableOperation::InsertRow { after: true },
        )));
    };
    match next {
        Some((row, column)) => Ok(TableNavigation::Cell {
            target: TableTarget::new(target.table.clone(), row, column),
            selection: cell_selection(parsed, &table, row, column, budget)?,
        }),
        None => Ok(TableNavigation::Boundary),
    }
}

/// 规划表格结构命令并在返回前重新解析核对网格。
///
/// # Errors
/// 目标过期、保护区域、不可保真结构、非法操作、预算或复读失败时拒绝。
pub fn plan_table_command(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    command: &TableCommand,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    let source_table =
        table_source(parsed, &command.target.table, budget.text()).map_err(map_table_error)?;
    ensure_target(&source_table, &command.target)?;
    let source = parsed
        .snapshot()
        .slice(source_table.source_range(), budget.text())
        .map_err(map_buffer_error)?;
    let mut table = TableText::read(parsed, &source_table, budget.text())?;
    let mut target_row = command.target.row;
    let mut target_column = command.target.column;
    let deleting = command.operation == TableOperation::DeleteTable;
    match command.operation {
        TableOperation::Tidy => table.tidy(),
        TableOperation::PromoteHeader => table.rows.swap(0, target_row),
        TableOperation::Align(alignment) => {
            table.separator.raw = None;
            table.separator.cells[target_column] = padded_alignment(alignment);
            table.alignments[target_column] = alignment;
        }
        TableOperation::InsertRow { after } => {
            let insertion = if after {
                target_row + 1
            } else {
                target_row.max(1)
            };
            table.insert_row(insertion)?;
            target_row = insertion;
            target_column = 0;
        }
        TableOperation::DeleteRow => {
            if target_row == 0 {
                return Err(EditRefusal::InvalidPlan);
            }
            let last_ending = table
                .endings
                .last()
                .cloned()
                .ok_or(EditRefusal::InvalidPlan)?;
            let deleting_last = target_row + 1 == table.rows.len();
            table.rows.remove(target_row);
            table.endings.remove(target_row + 1);
            if deleting_last && let Some(ending) = table.endings.last_mut() {
                *ending = last_ending;
            }
            table.prefixes.remove(target_row + 1);
            target_row = target_row.min(table.rows.len() - 1);
        }
        TableOperation::MoveRow { to } => {
            if target_row == 0 || to == 0 || to >= table.rows.len() {
                return Err(EditRefusal::InvalidPlan);
            }
            table.require_complete()?;
            let row = table.rows.remove(target_row);
            table.rows.insert(to, row);
            target_row = to;
        }
        TableOperation::InsertColumn { after } => {
            let insertion = target_column + usize::from(after);
            for row in &mut table.rows {
                row.raw = None;
                row.cells.insert(insertion, "  ".into());
            }
            table.separator.raw = None;
            table.separator.cells.insert(insertion, " --- ".into());
            table.alignments.insert(insertion, TableAlignment::None);
            target_column = insertion;
        }
        TableOperation::DeleteColumn => {
            if table.alignments.len() == 1 {
                return Err(EditRefusal::InvalidPlan);
            }
            table.require_complete()?;
            for row in &mut table.rows {
                row.raw = None;
                row.cells.remove(target_column);
            }
            table.separator.raw = None;
            table.separator.cells.remove(target_column);
            table.alignments.remove(target_column);
            target_column = target_column.min(table.alignments.len() - 1);
        }
        TableOperation::MoveColumn { to } => {
            if to >= table.alignments.len() {
                return Err(EditRefusal::InvalidPlan);
            }
            table.require_complete()?;
            for row in &mut table.rows {
                row.raw = None;
                let cell = row.cells.remove(target_column);
                row.cells.insert(to, cell);
            }
            table.separator.raw = None;
            let cell = table.separator.cells.remove(target_column);
            table.separator.cells.insert(to, cell);
            let alignment = table.alignments.remove(target_column);
            table.alignments.insert(to, alignment);
            target_column = to;
        }
        TableOperation::DeleteTable => {}
    }
    let start = source_table.source_range().byte_range().start;
    let replacement = if deleting {
        String::new()
    } else {
        table.render()?
    };
    let target = if deleting {
        SelectionSpec::new(start, start)
    } else {
        table.selection(start, target_row, target_column)?
    };
    let expectation = TableExpectation {
        start,
        cells: if deleting {
            None
        } else {
            Some(table.expected_cells())
        },
        alignments: table.alignments,
        input_layout: None,
    };
    plan_structural(
        parsed,
        selection,
        context,
        StructuralChange {
            edits: vec![SourceEdit::new(
                source_table.source_range().byte_range(),
                source.as_ref(),
                replacement,
            )],
            target,
            reason: GroupReason::TableCommand,
            contract: RereadContract::Table(expectation),
        },
        budget,
    )
}

/// 以补丁碰到的实际表格为准，不把包住它的顶层引用或列表当成普通原文块。
pub(crate) fn table_touched_by_input<'a>(
    nodes: &'a [PresentationNode],
    patch: &SrcRange,
) -> Option<&'a SrcRange> {
    table_touching_range(nodes, &patch.byte_range())
}

fn table_touching_range<'a>(
    nodes: &'a [PresentationNode],
    patch: &Range<usize>,
) -> Option<&'a SrcRange> {
    nodes.iter().find_map(|node| {
        let extent = node.source_range().byte_range();
        let touches = if patch.is_empty() {
            extent.start <= patch.start && patch.start <= extent.end
        } else {
            extent.start < patch.end && patch.start < extent.end
        };
        if !touches {
            None
        } else if matches!(node.kind(), PresentationKind::Table { .. }) {
            Some(node.source_range())
        } else {
            table_touching_range(node.children(), patch)
        }
    })
}

pub(crate) fn plan_table_raw_input(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    table_range: &SrcRange,
    edit: SourceEdit,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    if edit.replacement().contains(['\n', '\r']) {
        return Err(EditRefusal::InvalidPlan);
    }
    let table = table_source(parsed, table_range, budget.text()).map_err(map_table_error)?;
    let (row, column, cell_range) =
        table_cell_containing(&table, edit.range()).ok_or(EditRefusal::RawInputOutsideBlock)?;
    let mut text = TableText::read(parsed, &table, budget.text())?;
    let relative = edit.range().start - cell_range.start..edit.range().end - cell_range.start;
    let cell = &mut text.rows[row].cells[column];
    if cell.get(relative.clone()) != Some(edit.expected()) {
        return Err(EditRefusal::InvalidPlan);
    }
    cell.try_reserve(
        edit.replacement()
            .len()
            .saturating_sub(edit.expected().len()),
    )
    .map_err(|_| EditRefusal::Allocation)?;
    cell.replace_range(relative, edit.replacement());
    let expectation = table_input_expectation(&table, &text, &edit, None)?;
    let after_end = edit
        .range()
        .start
        .checked_add(edit.replacement().len())
        .ok_or(EditRefusal::LengthOverflow)?;
    plan_structural(
        parsed,
        selection,
        context,
        StructuralChange {
            edits: vec![edit],
            target: SelectionSpec::new(after_end, after_end),
            reason: GroupReason::RawBlockInput,
            contract: RereadContract::Table(expectation),
        },
        budget,
    )
}

fn table_cell_containing(
    table: &SourceTable,
    patch: &Range<usize>,
) -> Option<(usize, usize, Range<usize>)> {
    table
        .rows()
        .iter()
        .enumerate()
        .find_map(|(row, source_row)| {
            source_row
                .cells()
                .iter()
                .enumerate()
                .find_map(|(column, cell)| {
                    let range = cell.source_range().byte_range();
                    (cell.is_present() && range.start <= patch.start && patch.end <= range.end)
                        .then_some((row, column, range))
                })
        })
}

/// 首次输入时在短行末尾追加到目标缺失格，保留已有格、行尾和容器前缀。
///
/// # Errors
/// 目标过期或已存在、组合期间、输入含换行或无法维持表格结构时拒绝。
pub fn plan_table_missing_input(
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    target: &TableTarget,
    input: &str,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    precheck(context)?;
    if context.mode() != EditMode::Visual {
        return Err(EditRefusal::WrongMode {
            expected: EditMode::Visual,
            actual: context.mode(),
        });
    }
    parsed
        .snapshot()
        .validate_selection(selection)
        .map_err(map_buffer_error)?;
    let table = table_source(parsed, &target.table, budget.text()).map_err(map_table_error)?;
    ensure_target(&table, target)?;
    let row = &table.rows()[target.row];
    if row.cells()[target.column].is_present() || input.contains(['\n', '\r']) {
        return Err(EditRefusal::InvalidPlan);
    }
    if input.is_empty() {
        return Ok(PlanOutcome::Unchanged(NoChangeReason::ReplacementIdentical));
    }
    let existing = present_cell_count(row);
    let skipped_columns = target
        .column
        .checked_sub(existing)
        .ok_or(EditRefusal::InvalidPlan)?;
    let needs_separator = !row.has_outer_end();
    let required = skipped_columns
        .checked_mul(3)
        .and_then(|bytes| bytes.checked_add(3 + usize::from(needs_separator)))
        .and_then(|bytes| bytes.checked_add(input.len()))
        .ok_or(EditRefusal::LengthOverflow)?;
    let limit = budget.text().max_bytes();
    if required > limit {
        return Err(EditRefusal::BudgetExceeded {
            kind: BudgetKind::CandidateBytes,
            needed: Some(required),
            limit: Some(limit),
        });
    }
    let mut replacement = String::new();
    replacement
        .try_reserve_exact(required)
        .map_err(|_| EditRefusal::Allocation)?;
    if needs_separator {
        replacement.push('|');
    }
    for _ in 0..skipped_columns {
        replacement.push_str("  |");
    }
    replacement.push(' ');
    replacement.push_str(input);
    let row_end = row.source_range().byte_range().end;
    let caret = row_end
        .checked_add(replacement.len())
        .ok_or(EditRefusal::LengthOverflow)?;
    replacement.push_str(" |");
    let edit = SourceEdit::new(row_end..row_end, String::new(), replacement);
    let mut text = TableText::read(parsed, &table, budget.text())?;
    let cell = &mut text.rows[target.row].cells[target.column];
    cell.try_reserve_exact(input.len())
        .map_err(|_| EditRefusal::Allocation)?;
    cell.push_str(input);
    let expectation = table_input_expectation(&table, &text, &edit, Some(target))?;
    plan_structural(
        parsed,
        selection,
        context,
        StructuralChange {
            edits: vec![edit],
            target: SelectionSpec::new(caret, caret),
            reason: GroupReason::TableCommand,
            contract: RereadContract::Table(expectation),
        },
        budget,
    )
}

fn present_cell_count(row: &SourceRow) -> usize {
    row.cells().iter().filter(|cell| cell.is_present()).count()
}

fn table_input_expectation(
    table: &SourceTable,
    text: &TableText,
    edit: &SourceEdit,
    materialized: Option<&TableTarget>,
) -> Result<TableExpectation, EditRefusal> {
    let range = table.source_range().byte_range();
    let end = range
        .end
        .checked_sub(edit.range().len())
        .and_then(|end| end.checked_add(edit.replacement().len()))
        .ok_or(EditRefusal::LengthOverflow)?;
    let mut present_cells: Vec<_> = table.rows().iter().map(present_cell_count).collect();
    if let Some(target) = materialized {
        present_cells[target.row] = target.column + 1;
    }
    Ok(TableExpectation {
        start: range.start,
        cells: Some(text.expected_cells()),
        alignments: table.alignments().to_vec(),
        input_layout: Some(TableInputLayout { end, present_cells }),
    })
}

/// 格式化消费者使用同一表格整理规则，保留每行原行尾。
pub(crate) fn tidy_table(
    parsed: &Parsed,
    target: &SrcRange,
    budget: TextBudget,
) -> Result<SourceEdit, EditRefusal> {
    let table = table_source(parsed, target, budget).map_err(map_table_error)?;
    let original = parsed
        .snapshot()
        .slice(table.source_range(), budget)
        .map_err(map_buffer_error)?;
    let mut text = TableText::read(parsed, &table, budget)?;
    text.tidy();
    Ok(SourceEdit::new(
        table.source_range().byte_range(),
        original.as_ref(),
        text.render()?,
    ))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableExpectation {
    start: usize,
    cells: Option<Vec<Vec<String>>>,
    alignments: Vec<TableAlignment>,
    input_layout: Option<TableInputLayout>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableInputLayout {
    end: usize,
    present_cells: Vec<usize>,
}

pub(crate) fn verify_table(
    expectation: &TableExpectation,
    candidate: &Candidate,
    budget: EditBudget,
) -> Result<(), EditRefusal> {
    let node = find_at(candidate.parsed.presentation(), expectation.start);
    let Some(expected) = &expectation.cells else {
        return if node.is_none() {
            Ok(())
        } else {
            Err(table_reread_failure())
        };
    };
    let node = node.ok_or_else(table_reread_failure)?;
    let table = table_source(&candidate.parsed, node.source_range(), budget.text())
        .map_err(map_table_error)?;
    if let Some(layout) = &expectation.input_layout
        && (table.source_range().byte_range().end != layout.end
            || !table
                .rows()
                .iter()
                .map(present_cell_count)
                .eq(layout.present_cells.iter().copied()))
    {
        return Err(table_reread_failure());
    }
    let text = TableText::read(&candidate.parsed, &table, budget.text())?;
    if text.expected_cells() != *expected || text.alignments != expectation.alignments {
        return Err(table_reread_failure());
    }
    Ok(())
}

fn find_at(
    nodes: &[aline_markdown::PresentationNode],
    start: usize,
) -> Option<&aline_markdown::PresentationNode> {
    nodes.iter().find_map(|node| {
        if matches!(node.kind(), aline_markdown::PresentationKind::Table { .. })
            && node.source_range().byte_range().start == start
        {
            Some(node)
        } else {
            find_at(node.children(), start)
        }
    })
}
fn table_reread_failure() -> EditRefusal {
    EditRefusal::RereadFailed {
        reason: RereadFailure::BlockStructureChanged,
        cause: None,
    }
}
fn map_table_error(error: TableSourceError) -> EditRefusal {
    match error {
        TableSourceError::Source(error) => map_buffer_error(error),
        TableSourceError::InvalidTable => EditRefusal::InvalidPlan,
    }
}
fn ensure_target(table: &SourceTable, target: &TableTarget) -> Result<(), EditRefusal> {
    if target.row >= table.rows().len() || target.column >= table.alignments().len() {
        Err(EditRefusal::InvalidPlan)
    } else {
        Ok(())
    }
}
fn cell_selection(
    parsed: &Parsed,
    table: &SourceTable,
    row: usize,
    column: usize,
    budget: TextBudget,
) -> Result<SelectionSpec, EditRefusal> {
    let cell = &table.rows()[row].cells()[column];
    let range = cell.source_range().byte_range();
    let text = parsed
        .snapshot()
        .slice(cell.source_range(), budget)
        .map_err(map_buffer_error)?;
    let off = range.start + text.len() - text.trim_start_matches([' ', '\t']).len();
    Ok(SelectionSpec::new(off, off))
}

struct RowText {
    cells: Vec<String>,
    left: String,
    right: String,
    complete: bool,
    raw: Option<String>,
}
impl RowText {
    fn read(parsed: &Parsed, row: &SourceRow, budget: TextBudget) -> Result<Self, EditRefusal> {
        let cells = row
            .cells()
            .iter()
            .map(|cell| {
                parsed
                    .snapshot()
                    .slice(cell.source_range(), budget)
                    .map(|text| text.into_owned())
                    .map_err(map_buffer_error)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let range = row.source_range().byte_range();
        let first = row
            .cells()
            .iter()
            .find(|cell| cell.is_present())
            .ok_or(EditRefusal::InvalidPlan)?
            .source_range()
            .byte_range()
            .start;
        let last = row
            .cells()
            .iter()
            .rev()
            .find(|cell| cell.is_present())
            .ok_or(EditRefusal::InvalidPlan)?
            .source_range()
            .byte_range()
            .end;
        let left = parsed
            .snapshot()
            .slice(
                &parsed
                    .snapshot()
                    .src_range(range.start..first)
                    .map_err(map_buffer_error)?,
                budget,
            )
            .map_err(map_buffer_error)?
            .into_owned();
        let right = parsed
            .snapshot()
            .slice(
                &parsed
                    .snapshot()
                    .src_range(last..range.end)
                    .map_err(map_buffer_error)?,
                budget,
            )
            .map_err(map_buffer_error)?
            .into_owned();
        let raw = Some(
            parsed
                .snapshot()
                .slice(row.source_range(), budget)
                .map_err(map_buffer_error)?
                .into_owned(),
        );
        Ok(Self {
            cells,
            left,
            right,
            complete: row.cells().iter().all(|cell| cell.is_present()),
            raw,
        })
    }
    fn render(&self) -> String {
        if let Some(raw) = &self.raw {
            return raw.clone();
        }
        let mut result = self.left.clone();
        result.push_str(&self.cells.join("|"));
        result.push_str(&self.right);
        result
    }
}
struct TableText {
    rows: Vec<RowText>,
    separator: RowText,
    prefixes: Vec<String>,
    endings: Vec<String>,
    alignments: Vec<TableAlignment>,
}
impl TableText {
    fn read(parsed: &Parsed, table: &SourceTable, budget: TextBudget) -> Result<Self, EditRefusal> {
        let rows = table
            .rows()
            .iter()
            .map(|row| RowText::read(parsed, row, budget))
            .collect::<Result<Vec<_>, _>>()?;
        let mut prefixes = Vec::new();
        let mut endings = Vec::new();
        for row in std::iter::once(&table.rows()[0])
            .chain(std::iter::once(table.separator()))
            .chain(table.rows().iter().skip(1))
        {
            prefixes.push(
                parsed
                    .snapshot()
                    .slice(row.prefix_range(), budget)
                    .map_err(map_buffer_error)?
                    .into_owned(),
            );
            endings.push(row.line_ending().into());
        }
        prefixes[0].clear();
        Ok(Self {
            rows,
            separator: RowText::read(parsed, table.separator(), budget)?,
            prefixes,
            endings,
            alignments: table.alignments().to_vec(),
        })
    }
    fn require_complete(&self) -> Result<(), EditRefusal> {
        if self.rows.iter().all(|row| row.complete) {
            Ok(())
        } else {
            Err(EditRefusal::InvalidPlan)
        }
    }
    fn tidy(&mut self) {
        for row in &mut self.rows {
            row.left = "|".into();
            row.right = "|".into();
            row.complete = true;
            row.raw = None;
            for cell in &mut row.cells {
                *cell = format!(" {} ", cell.trim_matches([' ', '\t']));
            }
        }
        self.separator.left = "|".into();
        self.separator.right = "|".into();
        self.separator.raw = None;
        self.separator.cells = self
            .alignments
            .iter()
            .copied()
            .map(padded_alignment)
            .collect();
    }
    fn insert_row(&mut self, index: usize) -> Result<(), EditRefusal> {
        let columns = self.alignments.len();
        let ending = self
            .endings
            .iter()
            .find(|value| !value.is_empty())
            .cloned()
            .unwrap_or_else(|| "\n".into());
        let at_end = index == self.rows.len();
        let original_last = self
            .endings
            .last()
            .cloned()
            .ok_or(EditRefusal::InvalidPlan)?;
        let next_ending = if at_end {
            original_last.clone()
        } else {
            ending.clone()
        };
        if at_end
            && original_last.is_empty()
            && let Some(last) = self.endings.last_mut()
        {
            *last = ending;
        }
        let prefix = self
            .prefixes
            .get(1)
            .cloned()
            .ok_or(EditRefusal::InvalidPlan)?;
        self.rows.insert(
            index,
            RowText {
                cells: vec!["  ".into(); columns],
                left: "|".into(),
                right: "|".into(),
                complete: true,
                raw: None,
            },
        );
        self.prefixes.insert(index + 1, prefix);
        self.endings.insert(index + 1, next_ending);
        Ok(())
    }
    fn render(&self) -> Result<String, EditRefusal> {
        let mut result = String::new();
        for (physical, row) in std::iter::once(&self.rows[0])
            .chain(std::iter::once(&self.separator))
            .chain(self.rows.iter().skip(1))
            .enumerate()
        {
            result.push_str(
                self.prefixes
                    .get(physical)
                    .ok_or(EditRefusal::InvalidPlan)?,
            );
            result.push_str(&row.render());
            result.push_str(self.endings.get(physical).ok_or(EditRefusal::InvalidPlan)?);
        }
        Ok(result)
    }
    fn selection(
        &self,
        start: usize,
        row: usize,
        column: usize,
    ) -> Result<SelectionSpec, EditRefusal> {
        let physical = if row == 0 { 0 } else { row + 1 };
        let mut offset = start;
        for (index, line) in std::iter::once(&self.rows[0])
            .chain(std::iter::once(&self.separator))
            .chain(self.rows.iter().skip(1))
            .enumerate()
        {
            offset = offset
                .checked_add(self.prefixes[index].len())
                .ok_or(EditRefusal::LengthOverflow)?;
            if index == physical {
                offset = offset
                    .checked_add(line.left.len())
                    .ok_or(EditRefusal::LengthOverflow)?;
                for cell in line.cells.iter().take(column) {
                    offset = offset
                        .checked_add(cell.len() + 1)
                        .ok_or(EditRefusal::LengthOverflow)?;
                }
                let cell = &line.cells[column];
                offset = offset
                    .checked_add(cell.len() - cell.trim_start_matches([' ', '\t']).len())
                    .ok_or(EditRefusal::LengthOverflow)?;
                return Ok(SelectionSpec::new(offset, offset));
            }
            offset = offset
                .checked_add(line.render().len())
                .and_then(|off| off.checked_add(self.endings[index].len()))
                .ok_or(EditRefusal::LengthOverflow)?;
        }
        Err(EditRefusal::InvalidPlan)
    }
    fn expected_cells(&self) -> Vec<Vec<String>> {
        self.rows
            .iter()
            .map(|row| {
                row.cells
                    .iter()
                    .map(|cell| cell.trim_matches([' ', '\t']).to_owned())
                    .collect()
            })
            .collect()
    }
}
fn padded_alignment(alignment: TableAlignment) -> String {
    match alignment {
        TableAlignment::None => " --- ",
        TableAlignment::Left => " :--- ",
        TableAlignment::Center => " :---: ",
        TableAlignment::Right => " ---: ",
    }
    .into()
}
