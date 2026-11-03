//! 同一解析事件链的只读语义树；只持有语义、来源和绘制区间。
//!
//! `Builder` 拥有构建中的栈，`Parsed` 拥有绑定快照/投影后的不可变节点。
//! UI 按节点层级布局并经映射消费范围；本模块不解释颜色、不执行链接或资源，
//! 也不从语义节点重建 Markdown。节点源码范围保留解析器事件的完整来源，
//! 顶层可编辑块排除尾分隔的契约继续由 `Block` 单独表达。

use std::ops::Range;

use aline_buffer::{SrcRange, TextSnapshot};
use pulldown_cmark::{Alignment, BlockQuoteKind, CodeBlockKind, Event, Tag};

use crate::budget::{CancelCheck, ParseBudget};
use crate::build::BuildError;
use crate::mapping::{DrawnRange, Projection};

/// GitHub 五类提示块的语义；不携带主题色。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertKind {
    /// 信息提示。
    Note,
    /// 建议。
    Tip,
    /// 重要内容。
    Important,
    /// 警告。
    Warning,
    /// 注意风险。
    Caution,
}

/// 表格列对齐；顺序与表格行内的单元格顺序一致。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TableAlignment {
    /// 未指定。
    None,
    /// 左对齐。
    Left,
    /// 居中。
    Center,
    /// 右对齐。
    Right,
}

/// 只读语义节点种类；行内样式由相应祖先节点组合，HTML 样式另见 `StyledRange`。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PresentationKind {
    /// 段落。
    Paragraph,
    /// 标题，级别为 1～6。
    Heading {
        /// 标题级别。
        level: u8,
    },
    /// 引用或五类提示块。
    BlockQuote {
        /// 提示块种类；普通引用为 None。
        alert: Option<AlertKind>,
    },
    /// 列表；有序列表保留首项编号。
    List {
        /// 有序列表的首项编号；无序列表为 None。
        start: Option<u64>,
    },
    /// 列表项。
    Item,
    /// 围栏或缩进代码；info 保留解析器给出的语言信息。
    CodeBlock {
        /// 围栏的完整 info；缩进代码为空。
        info: String,
        /// 是否来自围栏代码。
        fenced: bool,
    },
    /// 有限 HTML 块，文字/样式继续走既有 HTML 投影。
    HtmlBlock,
    /// 表格；直接子节点为表头或正文行。
    Table {
        /// 各列的对齐方式。
        alignments: Vec<TableAlignment>,
    },
    /// 表头，直接子节点为单元格。
    TableHead,
    /// 表格正文行，直接子节点为单元格。
    TableRow,
    /// 表格单元格。
    TableCell,
    /// 强调。
    Emphasis,
    /// 粗体。
    Strong,
    /// 删除线。
    Strikethrough,
    /// 链接文字；不暴露或执行目标协议。
    Link,
    /// 图片引用，来源范围可供原文回退。
    Image,
    /// 普通文本。
    Text,
    /// 行内代码。
    Code,
    /// 行内公式原文。
    InlineMath,
    /// 块公式原文。
    DisplayMath,
    /// 行内或块内的 HTML 事件。
    Html,
    /// 段内源码换行；不代表布局软折行。
    SoftBreak,
    /// Markdown 硬换行。
    HardBreak,
    /// 分隔线。
    Rule,
    /// 脚注引用。
    FootnoteReference,
    /// 脚注定义。
    FootnoteDefinition,
    /// 文首 Front matter；用来源提供灰色原文入口。
    FrontMatter,
    /// 顶层目录段落；生成导航文字不成为源码。
    Toc,
    /// 任务框只读状态。
    TaskMarker {
        /// 原任务框是否已勾选。
        checked: bool,
    },
    /// 未启用扩展的事件容器，保留来源供原文回退。
    Unsupported,
    /// 整源可靠原文回退。
    Raw,
}

/// 代码正文的一条逻辑行；范围属于同一 Parsed 的全文绘制投影。
///
/// 行号不进入文字。范围排除终止换行，空正文保留第 1 条空行，终止换行
/// 不再产生一条虚构尾行；源码定位仍经该投影的映射完成，不能手算行首。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CodeLine {
    number: usize,
    drawn: DrawnRange,
    line_break: bool,
}

impl CodeLine {
    /// 返回从 1 开始的逻辑行号。
    pub const fn number(&self) -> usize {
        self.number
    }

    /// 返回不含行尾的受检绘制范围。
    pub fn drawn_range(&self) -> &DrawnRange {
        &self.drawn
    }

    /// 原正文中该行是否由换行终止。
    pub const fn has_line_break(&self) -> bool {
        self.line_break
    }
}

/// 绑定同一源码快照与绘制投影的不可变语义节点。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PresentationNode {
    kind: PresentationKind,
    source: SrcRange,
    drawn: DrawnRange,
    children: Vec<Self>,
    code_lines: Vec<CodeLine>,
}

impl PresentationNode {
    /// 返回语义；样式容器可以嵌套组合。
    pub fn kind(&self) -> &PresentationKind {
        &self.kind
    }

    /// 返回语法事件来源；端点已在所属快照校验。
    pub fn source_range(&self) -> &SrcRange {
        &self.source
    }

    /// 返回同一 Parsed 的全文绘制范围；允许隐藏结构的零宽范围。
    pub fn drawn_range(&self) -> &DrawnRange {
        &self.drawn
    }

    /// 返回按解析事件顺序排列的子节点。
    pub fn children(&self) -> &[Self] {
        &self.children
    }

    /// 返回代码正文逻辑行；非 CodeBlock 节点为空。
    pub fn code_lines(&self) -> &[CodeLine] {
        &self.code_lines
    }
}

pub(crate) struct DraftNode {
    kind: PresentationKind,
    source: Range<usize>,
    drawn: Range<usize>,
    children: Vec<Self>,
}

pub(crate) struct PresentationBuilder {
    roots: Vec<DraftNode>,
    stack: Vec<DraftNode>,
    slots: usize,
    max_slots: usize,
}

impl PresentationBuilder {
    pub(crate) fn new(budget: ParseBudget) -> Self {
        Self {
            roots: Vec::new(),
            stack: Vec::new(),
            slots: 0,
            max_slots: budget.max_nodes(),
        }
    }

    fn checked_slots(&self, count: usize) -> Result<usize, BuildError> {
        let next = self
            .slots
            .checked_add(count)
            .ok_or(BuildError::BudgetExceeded)?;
        if next > self.max_slots {
            return Err(BuildError::BudgetExceeded);
        }
        Ok(next)
    }

    fn reserve_slots(&mut self, count: usize) -> Result<(), BuildError> {
        self.slots = self.checked_slots(count)?;
        Ok(())
    }

    /// 在节点、表格列向量或栈容量分配前预检结构槽位。
    pub(crate) fn prepare_tag(
        &self,
        tag: &Tag<'_>,
        toc: bool,
        cancel: &dyn CancelCheck,
    ) -> Result<PresentationKind, BuildError> {
        let columns = match tag {
            Tag::Table(alignments) => alignments.len(),
            _ => 0,
        };
        let slots = columns.checked_add(1).ok_or(BuildError::BudgetExceeded)?;
        self.checked_slots(slots)?;
        tag_kind(tag, toc, cancel)
    }

    pub(crate) fn front_matter(&mut self, source: Range<usize>) -> Result<(), BuildError> {
        self.reserve_slots(1)?;
        self.attach(DraftNode {
            kind: PresentationKind::FrontMatter,
            source,
            drawn: 0..0,
            children: Vec::new(),
        })
    }

    pub(crate) fn start(
        &mut self,
        kind: PresentationKind,
        source: Range<usize>,
        drawn_start: usize,
    ) -> Result<(), BuildError> {
        let columns = match &kind {
            PresentationKind::Table { alignments } => alignments.len(),
            _ => 0,
        };
        self.reserve_slots(columns.checked_add(1).ok_or(BuildError::BudgetExceeded)?)?;
        self.stack
            .try_reserve(1)
            .map_err(|_| BuildError::Allocation)?;
        self.stack.push(DraftNode {
            kind,
            source,
            drawn: drawn_start..drawn_start,
            children: Vec::new(),
        });
        Ok(())
    }

    pub(crate) fn end(&mut self, source_end: usize, drawn_end: usize) -> Result<(), BuildError> {
        let mut node = self.stack.pop().ok_or(BuildError::Overlap)?;
        if source_end < node.source.start || drawn_end < node.drawn.start {
            return Err(BuildError::Overlap);
        }
        node.source.end = source_end;
        node.drawn.end = drawn_end;
        self.attach(node)
    }

    pub(crate) fn leaf(
        &mut self,
        kind: PresentationKind,
        source: Range<usize>,
        drawn: Range<usize>,
    ) -> Result<(), BuildError> {
        self.reserve_slots(1)?;
        self.attach(DraftNode {
            kind,
            source,
            drawn,
            children: Vec::new(),
        })
    }

    fn attach(&mut self, node: DraftNode) -> Result<(), BuildError> {
        let output = match self.stack.last_mut() {
            Some(parent) => &mut parent.children,
            None => &mut self.roots,
        };
        output.try_reserve(1).map_err(|_| BuildError::Allocation)?;
        output.push(node);
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<Vec<DraftNode>, BuildError> {
        if !self.stack.is_empty() {
            return Err(BuildError::Overlap);
        }
        Ok(self.roots)
    }
}

pub(crate) fn leaf_kind(event: &Event<'_>) -> Option<PresentationKind> {
    Some(match event {
        Event::Text(_) => PresentationKind::Text,
        Event::Code(_) => PresentationKind::Code,
        Event::InlineMath(_) => PresentationKind::InlineMath,
        Event::DisplayMath(_) => PresentationKind::DisplayMath,
        Event::Html(_) | Event::InlineHtml(_) => PresentationKind::Html,
        Event::SoftBreak => PresentationKind::SoftBreak,
        Event::HardBreak => PresentationKind::HardBreak,
        Event::Rule => PresentationKind::Rule,
        Event::FootnoteReference(_) => PresentationKind::FootnoteReference,
        Event::TaskListMarker(checked) => PresentationKind::TaskMarker { checked: *checked },
        Event::Start(_) | Event::End(_) => return None,
    })
}

fn tag_kind(
    tag: &Tag<'_>,
    toc: bool,
    cancel: &dyn CancelCheck,
) -> Result<PresentationKind, BuildError> {
    Ok(match tag {
        Tag::Paragraph if toc => PresentationKind::Toc,
        Tag::Paragraph => PresentationKind::Paragraph,
        Tag::Heading { level, .. } => PresentationKind::Heading {
            level: *level as u8,
        },
        Tag::BlockQuote(kind) => PresentationKind::BlockQuote {
            alert: kind.map(alert_kind),
        },
        Tag::CodeBlock(kind) => match kind {
            CodeBlockKind::Indented => PresentationKind::CodeBlock {
                info: String::new(),
                fenced: false,
            },
            CodeBlockKind::Fenced(info) => PresentationKind::CodeBlock {
                info: owned(info)?,
                fenced: true,
            },
        },
        Tag::HtmlBlock => PresentationKind::HtmlBlock,
        Tag::List(start) => PresentationKind::List { start: *start },
        Tag::Item => PresentationKind::Item,
        Tag::FootnoteDefinition(_) => PresentationKind::FootnoteDefinition,
        Tag::Table(alignments) => {
            let mut output = Vec::new();
            output
                .try_reserve(alignments.len())
                .map_err(|_| BuildError::Allocation)?;
            for alignment in alignments {
                if cancel.is_cancelled() {
                    return Err(BuildError::Cancelled);
                }
                output.push(match alignment {
                    Alignment::None => TableAlignment::None,
                    Alignment::Left => TableAlignment::Left,
                    Alignment::Center => TableAlignment::Center,
                    Alignment::Right => TableAlignment::Right,
                });
            }
            PresentationKind::Table { alignments: output }
        }
        Tag::TableHead => PresentationKind::TableHead,
        Tag::TableRow => PresentationKind::TableRow,
        Tag::TableCell => PresentationKind::TableCell,
        Tag::Emphasis => PresentationKind::Emphasis,
        Tag::Strong => PresentationKind::Strong,
        Tag::Strikethrough => PresentationKind::Strikethrough,
        Tag::Link { .. } => PresentationKind::Link,
        Tag::Image { .. } => PresentationKind::Image,
        _ => PresentationKind::Unsupported,
    })
}

fn owned(text: &str) -> Result<String, BuildError> {
    let mut result = String::new();
    result
        .try_reserve(text.len())
        .map_err(|_| BuildError::Allocation)?;
    result.push_str(text);
    Ok(result)
}

fn alert_kind(kind: BlockQuoteKind) -> AlertKind {
    match kind {
        BlockQuoteKind::Note => AlertKind::Note,
        BlockQuoteKind::Tip => AlertKind::Tip,
        BlockQuoteKind::Important => AlertKind::Important,
        BlockQuoteKind::Warning => AlertKind::Warning,
        BlockQuoteKind::Caution => AlertKind::Caution,
    }
}

pub(crate) fn bind(
    drafts: Vec<DraftNode>,
    snapshot: &TextSnapshot,
    projection: &Projection,
    budget: ParseBudget,
    cancel: &dyn CancelCheck,
) -> Result<Vec<PresentationNode>, BuildError> {
    let mut slots = 0usize;
    bind_nodes(drafts, snapshot, projection, budget, cancel, &mut slots)
}

fn take_slot(
    slots: &mut usize,
    budget: ParseBudget,
    cancel: &dyn CancelCheck,
) -> Result<(), BuildError> {
    if cancel.is_cancelled() {
        return Err(BuildError::Cancelled);
    }
    *slots = slots.checked_add(1).ok_or(BuildError::BudgetExceeded)?;
    if *slots > budget.max_nodes() {
        return Err(BuildError::BudgetExceeded);
    }
    Ok(())
}

fn bind_nodes(
    drafts: Vec<DraftNode>,
    snapshot: &TextSnapshot,
    projection: &Projection,
    budget: ParseBudget,
    cancel: &dyn CancelCheck,
    slots: &mut usize,
) -> Result<Vec<PresentationNode>, BuildError> {
    let mut output = Vec::new();
    output
        .try_reserve(drafts.len())
        .map_err(|_| BuildError::Allocation)?;
    for draft in drafts {
        take_slot(slots, budget, cancel)?;
        if let PresentationKind::Table { alignments } = &draft.kind {
            for _ in alignments {
                take_slot(slots, budget, cancel)?;
            }
        }
        let source = snapshot
            .src_range(draft.source)
            .map_err(|_| BuildError::Mapping)?;
        let drawn = projection
            .drawn_range(draft.drawn.clone())
            .map_err(|_| BuildError::Mapping)?;
        let code_lines = if matches!(draft.kind, PresentationKind::CodeBlock { .. }) {
            code_lines(projection, draft.drawn, budget, cancel, slots)?
        } else {
            Vec::new()
        };
        let children = bind_nodes(draft.children, snapshot, projection, budget, cancel, slots)?;
        output.push(PresentationNode {
            kind: draft.kind,
            source,
            drawn,
            children,
            code_lines,
        });
    }
    Ok(output)
}

fn code_lines(
    projection: &Projection,
    drawn: Range<usize>,
    budget: ParseBudget,
    cancel: &dyn CancelCheck,
    slots: &mut usize,
) -> Result<Vec<CodeLine>, BuildError> {
    let text = projection
        .drawn_text()
        .get(drawn.clone())
        .ok_or(BuildError::Mapping)?;
    let mut output = Vec::new();
    let mut drawn_start = drawn.start;
    for part in text.split_inclusive('\n') {
        take_slot(slots, budget, cancel)?;
        let has_break = part.ends_with('\n');
        let content = part.strip_suffix('\n').unwrap_or(part);
        let content = if has_break {
            content.strip_suffix('\r').unwrap_or(content)
        } else {
            content
        };
        let drawn_end = drawn_start
            .checked_add(content.len())
            .ok_or(BuildError::BudgetExceeded)?;
        output.try_reserve(1).map_err(|_| BuildError::Allocation)?;
        output.push(CodeLine {
            number: output.len() + 1,
            drawn: projection
                .drawn_range(drawn_start..drawn_end)
                .map_err(|_| BuildError::Mapping)?,
            line_break: has_break,
        });
        drawn_start = drawn_start
            .checked_add(part.len())
            .ok_or(BuildError::BudgetExceeded)?;
    }
    if output.is_empty() {
        take_slot(slots, budget, cancel)?;
        output.push(CodeLine {
            number: 1,
            drawn: projection
                .drawn_range(drawn)
                .map_err(|_| BuildError::Mapping)?,
            line_break: false,
        });
    }
    Ok(output)
}

pub(crate) fn raw_node(
    snapshot: &TextSnapshot,
    projection: &Projection,
) -> Result<Vec<PresentationNode>, BuildError> {
    if snapshot.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![PresentationNode {
        kind: PresentationKind::Raw,
        source: snapshot
            .src_range(0..snapshot.len_bytes())
            .map_err(|_| BuildError::Mapping)?,
        drawn: projection
            .drawn_range(0..projection.drawn_text().len())
            .map_err(|_| BuildError::Mapping)?,
        children: Vec::new(),
        code_lines: Vec::new(),
    }])
}
