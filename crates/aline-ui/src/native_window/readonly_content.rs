//! 只读可视化内容计划：把语义树转换为按块虚拟化的行计划。
//!
//! 计划只描述结构、绘制范围、行内样式与装饰意图，不持有 GPUI 元素、窗口状态
//! 或第二份源码。行内文字仍由映射窗口受检定位；UI 只按计划布局，不重新解析
//! Markdown、不执行 HTML、不加载图片或网络资源。所有范围都属于同一 `Parsed`
//! 的全文绘制投影，调用方不能把行内局部偏移当全文偏移。

use std::collections::HashMap;
use std::fmt;
use std::ops::Range;
use std::rc::Rc;

use aline_buffer::{Affinity, SrcRange};
use aline_markdown::{
    AlertKind, CodeLine, NavigationKind, Parsed, PresentationKind, PresentationNode,
    TableAlignment, TextStyle,
};

/// 单个表格在内容计划中的结构上限；与布局预检的节点/列预算对齐。
///
/// 超限表格不再逐行逐单元格物化，渲染层显示受限提示并保留源码视图入口。
const MAX_TABLE_ROWS: usize = 4096;
const MAX_TABLE_CELLS: usize = 4096;
const MAX_TABLE_COLUMNS: usize = 256;

/// 单个目录段落的条目上限；超限时显示受限提示，不把全部标题放进一行。
const MAX_TOC_ENTRIES: usize = 4096;

/// 内容计划构建失败；预算与范围错误分开，避免把预算拒绝当成空文档。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContentError {
    /// 行数或节点数超过注入上限。
    Budget,
    /// 语义树范围不是递增且不重叠的合法来源。
    InvalidRange,
}
impl fmt::Display for ContentError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}
impl std::error::Error for ContentError {}

/// 行内展示样式；由祖先语义节点与有限 HTML 样式合并而成。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct InlineStyle {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) strike: bool,
    pub(super) underline: bool,
    pub(super) mark: bool,
    pub(super) code: bool,
    pub(super) link: bool,
    pub(super) math: bool,
    pub(super) image: bool,
}

impl InlineStyle {
    /// 合并两层样式；任一祖先或 HTML 样式成立即保留。
    fn merged(self, other: Self) -> Self {
        Self {
            bold: self.bold || other.bold,
            italic: self.italic || other.italic,
            strike: self.strike || other.strike,
            underline: self.underline || other.underline,
            mark: self.mark || other.mark,
            code: self.code || other.code,
            link: self.link || other.link,
            math: self.math || other.math,
            image: self.image || other.image,
        }
    }

    fn from_html(style: TextStyle) -> Self {
        Self {
            bold: style.bold(),
            italic: style.italic(),
            strike: style.strike(),
            underline: style.underline(),
            mark: style.mark(),
            code: style.code(),
            link: style.link(),
            ..Self::default()
        }
    }
}

/// 行内叶子种类；只用于呈现与回归断言，不改变映射语义。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LeafKind {
    /// 普通文本。
    Text,
    /// 行内代码。
    Code,
    /// 行内或块公式原文。
    Math,
    /// 有限 HTML 的可见文字或字面标签。
    Html,
    /// 脚注引用；点击按导航索引跳转。
    FootnoteReference,
    /// 图片引用；展示 alt 文字。
    Image,
    /// 任务框只读状态。
    TaskMarker {
        /// 原任务框是否已勾选。
        checked: bool,
    },
    /// 段内源码换行。
    SoftBreak,
    /// Markdown 硬换行。
    HardBreak,
    /// 未启用扩展的原文。
    Unsupported,
}

/// 一段连续绘制文字及其样式；`drawn` 属于全文绘制投影。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Leaf {
    pub(super) drawn: Range<usize>,
    pub(super) style: InlineStyle,
    pub(super) kind: LeafKind,
}

/// 表格单元格计划；`drawn` 是单元格可见文字的全文绘制范围。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TableCell {
    pub(super) source: Range<usize>,
    pub(super) drawn: Range<usize>,
    pub(super) align: TableAlignment,
    pub(super) leaves: Vec<Leaf>,
}

/// 表格行计划；表头与正文行使用同一网格结构。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TableRow {
    pub(super) head: bool,
    pub(super) cells: Vec<TableCell>,
}

/// 目录条目；标题文字是导航副本，跳转目标来自导航索引。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TocEntry {
    pub(super) source: Range<usize>,
    pub(super) title: String,
    pub(super) level: u8,
}

/// 行内容种类；封闭枚举，渲染器穷尽分发。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum Block {
    /// 段落、标题、HTML 块或原文回退的可见文字。
    Text { leaves: Vec<Leaf> },
    /// 围栏或缩进代码；行号来自解析器的代码正文逻辑行。
    Code {
        info: String,
        lines: Vec<CodeLine>,
        leaves: Vec<Leaf>,
    },
    /// 只读表格网格；`truncated` 表示超过结构上限，渲染层只显示受限提示。
    Table {
        alignments: Vec<TableAlignment>,
        rows: Vec<TableRow>,
        truncated: bool,
    },
    /// 顶层目录段落；展示为生成导航条目。
    Toc {
        entries: Rc<Vec<TocEntry>>,
        truncated: bool,
    },
    /// 分隔线。
    Rule,
    /// 文首 Front matter；无绘制文字，只显示灰色只读区。
    FrontMatter,
}

/// 列表项标记；任务框状态属于绘制文字，不在此重复。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ListMarker {
    /// 无序列表。
    Bullet,
    /// 有序列表，保留解析器给出的编号。
    Ordered(u64),
}

/// 灰色只读区种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum GrayKind {
    /// 文首 Front matter。
    FrontMatter,
    /// 脚注定义内容。
    Footnote,
}

/// 容器首行上的生成标签；不是源码文字，不参与映射。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContentLabel {
    /// 脚注定义标签。
    Footnote,
}

/// 一行虚拟内容；源码范围递增且不重叠，绘制范围为全文绘制坐标。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ContentRow {
    pub(super) source: Range<usize>,
    pub(super) drawn: Option<Range<usize>>,
    pub(super) block: Block,
    pub(super) heading: Option<u8>,
    pub(super) quote_depth: u16,
    pub(super) list_depth: u16,
    pub(super) marker: Option<ListMarker>,
    pub(super) alert: Option<AlertKind>,
    pub(super) alert_badge: bool,
    pub(super) gray: Option<GrayKind>,
    pub(super) label: Option<ContentLabel>,
    pub(super) nodes: usize,
    pub(super) columns: usize,
}

/// 文档级内容计划；按源码顺序保存所有虚拟行。
pub(super) struct ContentIndex {
    rows: Vec<ContentRow>,
}

impl ContentIndex {
    /// 从同一 `Parsed` 构建行计划；不读取磁盘、不创建 GPUI 对象。
    pub(super) fn new(parsed: &Parsed, max_rows: usize) -> Result<Self, ContentError> {
        let styles = drawn_styles(parsed);
        let (toc, toc_truncated) = toc_entries(parsed);
        let mut builder = Builder {
            styles,
            toc: Rc::new(toc),
            toc_truncated,
            rows: Vec::new(),
            max_rows,
        };
        let mut source_end = 0usize;
        for node in parsed.presentation() {
            let range = node.source_range().byte_range();
            if range.start < source_end {
                return Err(ContentError::InvalidRange);
            }
            if range.start > source_end {
                builder.push_gap(source_end..range.start)?;
            }
            let mut context = Context::default();
            builder.walk(node, &mut context)?;
            source_end = range.end;
        }
        if source_end < parsed.snapshot().len_bytes() {
            builder.push_gap(source_end..parsed.snapshot().len_bytes())?;
        }
        if builder.rows.is_empty() {
            builder.push_gap(0..0)?;
        }
        Ok(Self { rows: builder.rows })
    }

    pub(super) fn rows(&self) -> &[ContentRow] {
        &self.rows
    }

    pub(super) fn row(&self, index: usize) -> Option<&ContentRow> {
        self.rows.get(index)
    }

    pub(super) fn len(&self) -> usize {
        self.rows.len()
    }
}

/// 行内样式来源：有限 HTML 的样式区间已映射到绘制坐标。
fn drawn_styles(parsed: &Parsed) -> Vec<(Range<usize>, TextStyle)> {
    if parsed.styled_ranges().is_empty() {
        return Vec::new();
    }
    let mut styled: Vec<(Range<usize>, TextStyle)> = parsed
        .styled_ranges()
        .iter()
        .map(|range| (range.range().byte_range(), range.style()))
        .collect();
    styled.sort_by_key(|(range, _)| (range.start, range.end));
    let mut output = Vec::new();
    let mut next = 0usize;
    let mut active: Vec<(usize, TextStyle)> = Vec::new();
    for segment in parsed.projection().segments() {
        let source = segment.source_range().byte_range();
        let drawn = segment.drawn_range();
        if source.is_empty() || drawn.is_empty() {
            continue;
        }
        while next < styled.len() && styled[next].0.start <= source.start {
            if styled[next].0.end > source.start {
                active.push((styled[next].0.end, styled[next].1));
            }
            next += 1;
        }
        active.retain(|(end, _)| *end > source.start);
        let mut style = TextStyle::default();
        for (_, active_style) in &active {
            style = TextStyle::new(
                style.bold() || active_style.bold(),
                style.italic() || active_style.italic(),
                style.strike() || active_style.strike(),
                style.underline() || active_style.underline(),
                style.mark() || active_style.mark(),
                style.code() || active_style.code(),
                style.link() || active_style.link(),
            );
        }
        if style != TextStyle::default() {
            output.push((drawn, style));
        }
    }
    output
}

/// 目录条目：标题文字经映射层取自标题绘制文字，目标来自导航索引。
///
/// 返回 `(条目, 是否截断)`；层级查找用哈希索引，避免标题数为 H 时逐条
/// 线性扫描造成 O(H²) 构建成本。
fn toc_entries(parsed: &Parsed) -> (Vec<TocEntry>, bool) {
    let navigation = parsed.navigation();
    let levels: HashMap<(usize, usize), u8> = navigation
        .entries()
        .iter()
        .filter(|entry| entry.kind() == NavigationKind::Heading)
        .filter_map(|entry| {
            entry.heading_level().map(|level| {
                let range = entry.source().byte_range();
                ((range.start, range.end), level)
            })
        })
        .collect();
    let mut output = Vec::new();
    let mut truncated = false;
    'entries: for entry in navigation.entries() {
        if entry.kind() != NavigationKind::Toc {
            continue;
        }
        for target in entry.targets() {
            if output.len() >= MAX_TOC_ENTRIES {
                truncated = true;
                break 'entries;
            }
            let source = target.byte_range();
            let level = levels
                .get(&(source.start, source.end))
                .copied()
                .unwrap_or(1);
            let title = drawn_title(parsed, target).unwrap_or_default();
            output.push(TocEntry {
                source,
                title,
                level,
            });
        }
    }
    (output, truncated)
}

/// 用映射层把标题来源端点转换为绘制端点，不手算标记宽度。
fn drawn_title(parsed: &Parsed, target: &SrcRange) -> Option<String> {
    let snapshot = parsed.snapshot();
    let projection = parsed.projection();
    let start = snapshot.src_offset(target.byte_range().start).ok()?;
    let end = snapshot.src_offset(target.byte_range().end).ok()?;
    let from = projection
        .drawn_at(&start, Affinity::After)
        .ok()?
        .value()
        .byte_offset();
    let to = projection
        .drawn_at(&end, Affinity::After)
        .ok()?
        .value()
        .byte_offset();
    if from > to {
        return None;
    }
    projection.drawn_text().get(from..to).map(str::to_owned)
}

/// 脚注导航意图；点击文字命中后按来源落入的导航范围解释。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum NavigationIntent {
    /// 跳转到脚注定义或引用目标；目标是导航索引绑定的源码范围。
    Jump { target: Range<usize> },
}

/// 命中来源若落在脚注引用或定义内，返回索引给出的目标；否则不改变导航。
pub(super) fn navigation_intent(parsed: &Parsed, source: usize) -> Option<NavigationIntent> {
    for entry in parsed.navigation().entries() {
        if !matches!(
            entry.kind(),
            NavigationKind::FootnoteReference | NavigationKind::FootnoteDefinition
        ) {
            continue;
        }
        if !entry.source().byte_range().contains(&source) {
            continue;
        }
        let target = entry.targets().first()?.byte_range();
        return Some(NavigationIntent::Jump { target });
    }
    None
}

/// 行内脚注引用的键盘目标：只在当前行的源码范围内找引用，不跨行猜目标。
///
/// 多个引用位于同一行时取源码顺序的第一个；回跳目标由导航索引绑定，不能按
/// 显示文字搜索。行内没有引用或索引缺失时返回 `None`，不伪造跳转。
pub(super) fn footnote_reference_target(
    parsed: &Parsed,
    row_source: &Range<usize>,
) -> Option<usize> {
    parsed
        .navigation()
        .entries()
        .iter()
        .filter(|entry| entry.kind() == NavigationKind::FootnoteReference)
        .find(|entry| {
            let source = entry.source().byte_range();
            source.start >= row_source.start && source.end <= row_source.end
        })
        .and_then(|entry| entry.targets().first())
        .map(|target| target.byte_range().start)
}

/// 脚注定义行的键盘回跳目标：定义条目源码覆盖当前行时取该定义绑定的首个引用。
///
/// 定义条目源码已裁掉尾部换行，而行源码保留换行，因此按行首落在定义范围内判断，
/// 不要求整行被定义条目包含。多段定义中每段都是灰色只读行，只有行首在定义内的行
/// 才获得回跳目标；目标来自导航索引，不按标签文字搜索。
pub(super) fn footnote_definition_target(
    parsed: &Parsed,
    row_source: &Range<usize>,
) -> Option<usize> {
    parsed
        .navigation()
        .entries()
        .iter()
        .filter(|entry| entry.kind() == NavigationKind::FootnoteDefinition)
        .find(|entry| {
            let source = entry.source().byte_range();
            source.start <= row_source.start && row_source.start < source.end
        })
        .and_then(|entry| entry.targets().first())
        .map(|target| target.byte_range().start)
}

#[derive(Clone, Copy, Default)]
struct Context {
    quote: u16,
    alert: Option<AlertKind>,
    list: u16,
    gray: Option<GrayKind>,
    marker: Option<ListMarker>,
    label: Option<ContentLabel>,
    alert_badge: bool,
}

impl Context {
    fn with_quote(mut self, alert: Option<AlertKind>) -> Self {
        self.quote = self.quote.saturating_add(1);
        if alert.is_some() {
            self.alert = alert;
            self.alert_badge = true;
        }
        self
    }

    fn with_list(mut self, marker: Option<ListMarker>) -> Self {
        self.list = self.list.saturating_add(1);
        self.marker = marker;
        self
    }

    fn with_gray(mut self, gray: GrayKind) -> Self {
        self.gray = Some(gray);
        self
    }
}

struct Builder {
    styles: Vec<(Range<usize>, TextStyle)>,
    toc: Rc<Vec<TocEntry>>,
    toc_truncated: bool,
    rows: Vec<ContentRow>,
    max_rows: usize,
}

impl Builder {
    fn push(&mut self, row: ContentRow) -> Result<(), ContentError> {
        if self.rows.len() >= self.max_rows {
            return Err(ContentError::Budget);
        }
        self.rows.try_reserve(1).map_err(|_| ContentError::Budget)?;
        self.rows.push(row);
        Ok(())
    }

    fn push_gap(&mut self, source: Range<usize>) -> Result<(), ContentError> {
        self.push(ContentRow {
            source,
            drawn: None,
            block: Block::Text { leaves: Vec::new() },
            heading: None,
            quote_depth: 0,
            list_depth: 0,
            marker: None,
            alert: None,
            alert_badge: false,
            gray: None,
            label: None,
            nodes: 1,
            columns: 0,
        })
    }

    fn walk(&mut self, node: &PresentationNode, context: &mut Context) -> Result<(), ContentError> {
        match node.kind() {
            PresentationKind::BlockQuote { alert } => {
                let saved = *context;
                *context = context.with_quote(*alert);
                for child in node.children() {
                    self.walk(child, context)?;
                }
                *context = saved;
            }
            PresentationKind::List { start } => {
                for (index, item) in node.children().iter().enumerate() {
                    let number = start.map(|start| start.saturating_add(index as u64));
                    let marker = Some(match number {
                        Some(number) => ListMarker::Ordered(number),
                        None => ListMarker::Bullet,
                    });
                    let saved = *context;
                    *context = context.with_list(marker);
                    self.walk_item(item, context)?;
                    *context = saved;
                }
            }
            PresentationKind::Item => self.walk_item(node, context)?,
            PresentationKind::FootnoteDefinition => {
                let saved = *context;
                *context = context.with_gray(GrayKind::Footnote);
                context.label = Some(ContentLabel::Footnote);
                for child in node.children() {
                    self.walk(child, context)?;
                }
                *context = saved;
            }
            PresentationKind::Paragraph | PresentationKind::Heading { .. } => {
                let heading = match node.kind() {
                    PresentationKind::Heading { level } => Some(*level),
                    _ => None,
                };
                self.push_text(node, heading, context)?;
            }
            PresentationKind::HtmlBlock | PresentationKind::Unsupported | PresentationKind::Raw => {
                self.push_text(node, None, context)?
            }
            PresentationKind::CodeBlock { info, .. } => {
                let drawn = node.drawn_range().byte_range();
                let style = InlineStyle {
                    code: true,
                    ..InlineStyle::default()
                };
                let leaves = if drawn.is_empty() {
                    Vec::new()
                } else {
                    vec![Leaf {
                        drawn: drawn.clone(),
                        style,
                        kind: LeafKind::Code,
                    }]
                };
                let row = self.row(
                    node.source_range().byte_range(),
                    Some(drawn),
                    Block::Code {
                        info: info.clone(),
                        lines: node.code_lines().to_vec(),
                        leaves,
                    },
                    None,
                    context,
                );
                self.push(row)?;
            }
            PresentationKind::Table { alignments } => {
                let (rows, truncated) = self.table_rows(node, alignments)?;
                let row = self.row(
                    node.source_range().byte_range(),
                    Some(node.drawn_range().byte_range()),
                    Block::Table {
                        alignments: alignments.clone(),
                        rows,
                        truncated,
                    },
                    None,
                    context,
                );
                self.push(row)?;
            }
            PresentationKind::Toc => {
                let row = self.row(
                    node.source_range().byte_range(),
                    Some(node.drawn_range().byte_range()),
                    Block::Toc {
                        entries: Rc::clone(&self.toc),
                        truncated: self.toc_truncated,
                    },
                    None,
                    context,
                );
                self.push(row)?;
            }
            PresentationKind::FrontMatter => {
                let saved = *context;
                context.gray = Some(GrayKind::FrontMatter);
                let row = self.row(
                    node.source_range().byte_range(),
                    None,
                    Block::FrontMatter,
                    None,
                    context,
                );
                *context = saved;
                self.push(row)?;
            }
            PresentationKind::Rule => {
                let row = self.row(
                    node.source_range().byte_range(),
                    None,
                    Block::Rule,
                    None,
                    context,
                );
                self.push(row)?;
            }
            PresentationKind::Emphasis
            | PresentationKind::Strong
            | PresentationKind::Strikethrough
            | PresentationKind::Link
            | PresentationKind::Image
            | PresentationKind::Text
            | PresentationKind::Code
            | PresentationKind::InlineMath
            | PresentationKind::DisplayMath
            | PresentationKind::Html
            | PresentationKind::SoftBreak
            | PresentationKind::HardBreak
            | PresentationKind::FootnoteReference
            | PresentationKind::TaskMarker { .. }
            | PresentationKind::TableHead
            | PresentationKind::TableRow
            | PresentationKind::TableCell => self.push_text(node, None, context)?,
        }
        Ok(())
    }

    /// 列表项：连续行内子节点合成一行，块子节点各自成行；标记落到首行。
    fn walk_item(
        &mut self,
        item: &PresentationNode,
        context: &mut Context,
    ) -> Result<(), ContentError> {
        let children = item.children();
        let mut index = 0usize;
        let mut first = true;
        while index < children.len() {
            let child = &children[index];
            if is_block(child.kind()) {
                // 标记与标签只属于该项的首行；后续块行不再继承。
                if !first {
                    context.marker = None;
                    context.label = None;
                }
                self.walk(child, context)?;
                context.marker = None;
                context.label = None;
                first = false;
                index += 1;
                continue;
            }
            let start = index;
            let mut leaves = Vec::new();
            while index < children.len() && !is_block(children[index].kind()) {
                self.collect_inline(&children[index], InlineStyle::default(), &mut leaves);
                index += 1;
            }
            if leaves.is_empty() {
                continue;
            }
            let drawn = leaves
                .first()
                .zip(leaves.last())
                .map(|(first, last)| first.drawn.start..last.drawn.end)
                .ok_or(ContentError::InvalidRange)?;
            let source_start = if first {
                item.source_range().byte_range().start
            } else {
                children[start].source_range().byte_range().start
            };
            let source_end = children[index - 1].source_range().byte_range().end;
            let row = self.row(
                source_start..source_end.max(source_start),
                Some(drawn),
                Block::Text { leaves },
                None,
                context,
            );
            self.push(row)?;
            first = false;
        }
        Ok(())
    }

    fn push_text(
        &mut self,
        node: &PresentationNode,
        heading: Option<u8>,
        context: &mut Context,
    ) -> Result<(), ContentError> {
        let drawn = node.drawn_range().byte_range();
        let mut leaves = Vec::new();
        self.collect_inline(node, InlineStyle::default(), &mut leaves);
        if leaves.is_empty() && !drawn.is_empty() {
            leaves.push(Leaf {
                drawn: drawn.clone(),
                style: InlineStyle::default(),
                kind: LeafKind::Text,
            });
        }
        let row = self.row(
            node.source_range().byte_range(),
            (!drawn.is_empty()).then_some(drawn),
            Block::Text { leaves },
            heading,
            context,
        );
        self.push(row)
    }

    /// 递归收集行内叶子；样式容器只合并样式，不产生独立文字。
    fn collect_inline(
        &mut self,
        node: &PresentationNode,
        style: InlineStyle,
        leaves: &mut Vec<Leaf>,
    ) {
        let merged = match node.kind() {
            PresentationKind::Strong => style.merged(InlineStyle {
                bold: true,
                ..InlineStyle::default()
            }),
            PresentationKind::Emphasis => style.merged(InlineStyle {
                italic: true,
                ..InlineStyle::default()
            }),
            PresentationKind::Strikethrough => style.merged(InlineStyle {
                strike: true,
                ..InlineStyle::default()
            }),
            PresentationKind::Link => style.merged(InlineStyle {
                link: true,
                ..InlineStyle::default()
            }),
            PresentationKind::Image => style.merged(InlineStyle {
                image: true,
                ..InlineStyle::default()
            }),
            PresentationKind::Code => style.merged(InlineStyle {
                code: true,
                ..InlineStyle::default()
            }),
            PresentationKind::InlineMath | PresentationKind::DisplayMath => {
                style.merged(InlineStyle {
                    math: true,
                    ..InlineStyle::default()
                })
            }
            _ => style,
        };
        if matches!(node.kind(), PresentationKind::Image) {
            let drawn = node.drawn_range().byte_range();
            if !drawn.is_empty() {
                let style = merged.merged(self.html_style(&drawn));
                leaves.push(Leaf {
                    drawn,
                    style,
                    kind: LeafKind::Image,
                });
            }
            return;
        }
        let kind = match node.kind() {
            PresentationKind::Text => Some(LeafKind::Text),
            PresentationKind::Code => Some(LeafKind::Code),
            PresentationKind::InlineMath | PresentationKind::DisplayMath => Some(LeafKind::Math),
            PresentationKind::Html => Some(LeafKind::Html),
            PresentationKind::FootnoteReference => Some(LeafKind::FootnoteReference),
            PresentationKind::TaskMarker { checked } => {
                Some(LeafKind::TaskMarker { checked: *checked })
            }
            PresentationKind::SoftBreak => Some(LeafKind::SoftBreak),
            PresentationKind::HardBreak => Some(LeafKind::HardBreak),
            PresentationKind::Unsupported => Some(LeafKind::Unsupported),
            _ => None,
        };
        if let Some(kind) = kind {
            let drawn = node.drawn_range().byte_range();
            if !drawn.is_empty() {
                let style = merged.merged(self.html_style(&drawn));
                leaves.push(Leaf { drawn, style, kind });
            }
        }
        if node.children().is_empty() {
            return;
        }
        for child in node.children() {
            self.collect_inline(child, merged, leaves);
        }
    }

    /// 有限 HTML 样式按绘制范围合并；范围边界由引擎片段给出。
    fn html_style(&self, drawn: &Range<usize>) -> InlineStyle {
        let mut style = InlineStyle::default();
        let mut index = self
            .styles
            .partition_point(|(range, _)| range.end <= drawn.start);
        while let Some((range, html)) = self.styles.get(index) {
            if range.start >= drawn.end {
                break;
            }
            if range.start < drawn.end && range.end > drawn.start {
                style = style.merged(InlineStyle::from_html(*html));
            }
            index += 1;
        }
        style
    }

    /// 物化表格行；超过结构上限时停止并标记截断，由渲染层显示受限提示。
    ///
    /// 截断时不保留部分行：部分网格会被误读为完整表格，且完整源码始终
    /// 可从源码视图阅读。
    fn table_rows(
        &mut self,
        table: &PresentationNode,
        alignments: &[TableAlignment],
    ) -> Result<(Vec<TableRow>, bool), ContentError> {
        let mut rows = Vec::new();
        let mut total_cells = 0usize;
        for row in table.children() {
            let head = matches!(row.kind(), PresentationKind::TableHead);
            if !head && !matches!(row.kind(), PresentationKind::TableRow) {
                continue;
            }
            if rows.len() >= MAX_TABLE_ROWS {
                return Ok((Vec::new(), true));
            }
            let mut cells = Vec::new();
            for (index, cell) in row.children().iter().enumerate() {
                if !matches!(cell.kind(), PresentationKind::TableCell) {
                    continue;
                }
                if cells.len() >= MAX_TABLE_COLUMNS || total_cells >= MAX_TABLE_CELLS {
                    return Ok((Vec::new(), true));
                }
                let drawn = cell.drawn_range().byte_range();
                let mut leaves = Vec::new();
                self.collect_inline(cell, InlineStyle::default(), &mut leaves);
                if leaves.is_empty() && !drawn.is_empty() {
                    leaves.push(Leaf {
                        drawn: drawn.clone(),
                        style: InlineStyle::default(),
                        kind: LeafKind::Text,
                    });
                }
                cells.push(TableCell {
                    source: cell.source_range().byte_range(),
                    drawn,
                    align: alignments
                        .get(index)
                        .copied()
                        .unwrap_or(TableAlignment::None),
                    leaves,
                });
                total_cells += 1;
            }
            rows.try_reserve(1).map_err(|_| ContentError::Budget)?;
            rows.push(TableRow { head, cells });
        }
        Ok((rows, false))
    }

    fn row(
        &self,
        source: Range<usize>,
        drawn: Option<Range<usize>>,
        block: Block,
        heading: Option<u8>,
        context: &mut Context,
    ) -> ContentRow {
        let marker = context.marker.take();
        let label = context.label.take();
        let alert_badge = context.alert_badge;
        context.alert_badge = false;
        let columns = match &block {
            Block::Table { alignments, .. } => alignments.len(),
            _ => 0,
        };
        let nodes = match &block {
            Block::Text { leaves } => leaves.len().max(1),
            Block::Code { lines, .. } => lines.len().max(1),
            // 截断行用超限哨兵值驱动预检拒绝，保证渲染层走受限提示而非部分网格。
            Block::Table {
                rows, truncated, ..
            } => {
                if *truncated {
                    MAX_TABLE_CELLS.saturating_add(1)
                } else {
                    rows.iter().map(|row| row.cells.len()).sum::<usize>().max(1)
                }
            }
            Block::Toc { entries, truncated } => {
                if *truncated {
                    MAX_TOC_ENTRIES.saturating_add(1)
                } else {
                    entries.len().max(1)
                }
            }
            _ => 1,
        };
        ContentRow {
            source,
            drawn,
            block,
            heading,
            quote_depth: context.quote,
            list_depth: context.list,
            marker,
            alert: context.alert,
            alert_badge,
            gray: context.gray,
            label,
            nodes,
            columns,
        }
    }
}

fn is_block(kind: &PresentationKind) -> bool {
    matches!(
        kind,
        PresentationKind::Paragraph
            | PresentationKind::Heading { .. }
            | PresentationKind::BlockQuote { .. }
            | PresentationKind::List { .. }
            | PresentationKind::Item
            | PresentationKind::CodeBlock { .. }
            | PresentationKind::HtmlBlock
            | PresentationKind::Table { .. }
            | PresentationKind::TableHead
            | PresentationKind::TableRow
            | PresentationKind::TableCell
            | PresentationKind::Rule
            | PresentationKind::FootnoteDefinition
            | PresentationKind::FrontMatter
            | PresentationKind::Toc
            | PresentationKind::Raw
    )
}

#[cfg(test)]
mod tests {
    use super::{
        Block, ContentError, ContentIndex, ContentLabel, ContentRow, GrayKind, LeafKind,
        ListMarker, NavigationIntent, footnote_definition_target, footnote_reference_target,
        navigation_intent,
    };
    use aline_buffer::{TextBudget, TextSnapshot};
    use aline_markdown::{
        AlertKind, NavigationKind, ParseBudget, ParseConfig, Parsed, TableAlignment, parse,
    };
    use std::error::Error;

    fn parsed(source: &str) -> Result<Parsed, Box<dyn Error>> {
        let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(1 << 20))?;
        Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
    }

    fn index(source: &str) -> Result<(Parsed, ContentIndex), Box<dyn Error>> {
        let parsed = parsed(source)?;
        let index = ContentIndex::new(&parsed, 4096)?;
        Ok((parsed, index))
    }

    fn drawn<'a>(parsed: &'a Parsed, row: &super::ContentRow) -> &'a str {
        let range = row.drawn.clone().unwrap_or_default();
        parsed
            .projection()
            .drawn_text()
            .get(range)
            .unwrap_or_default()
    }

    fn leaves(row: &super::ContentRow) -> &[super::Leaf] {
        match &row.block {
            Block::Text { leaves } => leaves,
            _ => &[],
        }
    }

    fn text_leaf<'a>(
        parsed: &Parsed,
        row: &'a super::ContentRow,
        text: &str,
    ) -> Option<&'a super::Leaf> {
        let drawn = parsed.projection().drawn_text();
        leaves(row).iter().find(|leaf| {
            drawn
                .get(leaf.drawn.clone())
                .is_some_and(|slice| slice == text)
        })
    }

    #[test]
    fn inline_styles_and_entities_plan_runs_over_drawn_text() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("A **bold** and *italic* &amp; text.\n")?;
        assert_eq!(index.len(), 1);
        let row = index.row(0).ok_or("row")?;
        assert_eq!(drawn(&parsed, row), "A bold and italic & text.");
        let bold = text_leaf(&parsed, row, "bold").ok_or("bold")?;
        assert!(bold.style.bold);
        let italic = text_leaf(&parsed, row, "italic").ok_or("italic")?;
        assert!(italic.style.italic);
        let entity = text_leaf(&parsed, row, "&").ok_or("entity")?;
        assert_eq!(entity.kind, LeafKind::Text);
        assert!(!entity.style.bold && !entity.style.italic);
        Ok(())
    }

    #[test]
    fn nested_containers_expose_alert_lists_and_task_markers() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index(
            "> [!NOTE]\n> outer\n>\n> - first\n>   - nested **bold**\n\n- [x] task\n- plain\n",
        )?;
        let rows = index.rows();
        // 引用块与列表之间的空行保留为源码回退行。
        assert_eq!(rows.len(), 6);
        assert!(rows[3].drawn.is_none());
        let outer = &rows[0];
        assert_eq!(drawn(&parsed, outer), "outer");
        assert_eq!(outer.quote_depth, 1);
        assert_eq!(outer.alert, Some(AlertKind::Note));
        assert!(outer.alert_badge);
        assert_eq!(outer.marker, None);
        let first = &rows[1];
        assert_eq!(drawn(&parsed, first), "first");
        assert_eq!(first.list_depth, 1);
        assert_eq!(first.marker, Some(ListMarker::Bullet));
        let nested = &rows[2];
        assert_eq!(drawn(&parsed, nested), "nested bold");
        assert_eq!(nested.list_depth, 2);
        assert_eq!(nested.marker, Some(ListMarker::Bullet));
        assert!(!nested.alert_badge);
        let task = &rows[4];
        assert_eq!(drawn(&parsed, task), "\u{2611} task");
        assert_eq!(task.marker, Some(ListMarker::Bullet));
        assert!(
            leaves(task)
                .iter()
                .any(|leaf| leaf.kind == LeafKind::TaskMarker { checked: true })
        );
        let plain = &rows[5];
        assert_eq!(drawn(&parsed, plain), "plain");
        Ok(())
    }

    #[test]
    fn ordered_lists_keep_parser_numbers_and_loose_paragraphs() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("3. one\n\n4. two\n")?;
        let rows = index.rows();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].marker, Some(ListMarker::Ordered(3)));
        assert_eq!(rows[1].marker, Some(ListMarker::Ordered(4)));
        assert_eq!(drawn(&parsed, &rows[0]), "one");
        assert_eq!(drawn(&parsed, &rows[1]), "two");
        Ok(())
    }

    #[test]
    fn tables_keep_rows_columns_alignment_and_cell_ranges() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("| left | right |\n| :--- | ---: |\n| a\\|b | `x|y` |\n")?;
        assert_eq!(index.len(), 1);
        let row = index.row(0).ok_or("row")?;
        let Block::Table {
            alignments,
            rows,
            truncated,
        } = &row.block
        else {
            return Err("table block".into());
        };
        assert!(!truncated);
        assert_eq!(alignments, &[TableAlignment::Left, TableAlignment::Right]);
        assert_eq!(rows.len(), 2);
        assert!(rows[0].head);
        assert_eq!(rows[0].cells.len(), 2);
        assert_eq!(rows[1].cells.len(), 2);
        let text = parsed.projection().drawn_text();
        assert_eq!(&text[rows[0].cells[0].drawn.clone()], "left");
        assert_eq!(&text[rows[0].cells[1].drawn.clone()], "right");
        assert_eq!(rows[1].cells[0].align, TableAlignment::Left);
        assert_eq!(rows[1].cells[1].align, TableAlignment::Right);
        for cell in &rows[1].cells {
            assert!(!cell.leaves.is_empty());
        }
        Ok(())
    }

    #[test]
    fn oversized_table_rows_stop_materializing_and_mark_truncated() -> Result<(), Box<dyn Error>> {
        let mut source = String::from("| a | b |\n| --- | --- |\n");
        for _ in 0..super::MAX_TABLE_ROWS {
            source.push_str("| x | y |\n");
        }
        let (_, index) = index(&source)?;
        let row = index.row(0).ok_or("row")?;
        let Block::Table {
            rows, truncated, ..
        } = &row.block
        else {
            return Err("table block".into());
        };
        assert!(truncated);
        assert!(rows.is_empty());
        assert!(row.nodes > super::MAX_TABLE_CELLS);
        Ok(())
    }

    #[test]
    fn oversized_table_columns_are_truncated() -> Result<(), Box<dyn Error>> {
        let columns = super::MAX_TABLE_COLUMNS + 1;
        let row = format!("|{}|\n", vec!["a"; columns].join("|"));
        let delimiter = format!("|{}|\n", vec!["---"; columns].join("|"));
        let (_, index) = index(&format!("{row}{delimiter}{row}"))?;
        let content = index.row(0).ok_or("row")?;
        let Block::Table {
            rows, truncated, ..
        } = &content.block
        else {
            return Err("table block".into());
        };
        assert!(truncated);
        assert!(rows.is_empty());
        assert_eq!(content.columns, columns);
        Ok(())
    }

    #[test]
    fn oversized_table_cells_are_truncated_without_row_or_column_overflow()
    -> Result<(), Box<dyn Error>> {
        let columns = 64;
        let row = format!("|{}|\n", vec!["a"; columns].join("|"));
        let delimiter = format!("|{}|\n", vec!["---"; columns].join("|"));
        let mut source = format!("{row}{delimiter}");
        for _ in 0..(super::MAX_TABLE_CELLS / columns) {
            source.push_str(&row);
        }
        let (_, index) = index(&source)?;
        let content = index.row(0).ok_or("row")?;
        let Block::Table { truncated, .. } = &content.block else {
            return Err("table block".into());
        };
        assert!(truncated);
        assert_eq!(content.columns, columns);
        Ok(())
    }

    #[test]
    fn oversized_toc_stops_at_entry_limit() -> Result<(), Box<dyn Error>> {
        let mut source = String::from("[TOC]\n\n");
        for _ in 0..=super::MAX_TOC_ENTRIES {
            source.push_str("# T\n\n");
        }
        let parsed = parsed(&source)?;
        let index = ContentIndex::new(&parsed, 100_000)?;
        let row = index.row(0).ok_or("row")?;
        let Block::Toc { entries, truncated } = &row.block else {
            return Err("toc block".into());
        };
        assert!(truncated);
        assert_eq!(entries.len(), super::MAX_TOC_ENTRIES);
        assert_eq!(row.nodes, super::MAX_TOC_ENTRIES + 1);
        Ok(())
    }

    #[test]
    fn code_rows_number_from_one_and_keep_empty_lines() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("```rust\n\nline\n```\n")?;
        // 末尾换行不属于代码块来源，保留为一行源码回退。
        assert_eq!(index.len(), 2);
        let row = index.row(0).ok_or("row")?;
        let Block::Code {
            info,
            lines,
            leaves,
        } = &row.block
        else {
            return Err("code block".into());
        };
        assert_eq!(info, "rust");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].number(), 1);
        assert_eq!(lines[1].number(), 2);
        let text = parsed.projection().drawn_text();
        assert_eq!(&text[lines[0].drawn_range().byte_range()], "");
        assert_eq!(&text[lines[1].drawn_range().byte_range()], "line");
        assert_eq!(leaves.len(), 1);
        assert_eq!(leaves[0].kind, LeafKind::Code);
        Ok(())
    }

    #[test]
    fn front_matter_toc_and_footnotes_are_gray_plan_rows() -> Result<(), Box<dyn Error>> {
        let (parsed, index) =
            index("---\ntitle: x\n---\n\n# T\n\n[TOC]\n\nref[^Note]\n\n[^note]: def\n")?;
        let rows = index.rows();
        let front = rows
            .iter()
            .find(|row| matches!(row.block, Block::FrontMatter))
            .ok_or("front matter")?;
        assert_eq!(front.gray, Some(GrayKind::FrontMatter));
        let toc = rows
            .iter()
            .find(|row| matches!(row.block, Block::Toc { .. }))
            .ok_or("toc")?;
        let Block::Toc { entries, truncated } = &toc.block else {
            return Err("toc block".into());
        };
        assert!(!truncated);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries.first().map(|entry| entry.title.as_str()), Some("T"));
        assert_eq!(entries.first().map(|entry| entry.level), Some(1));
        let reference = rows
            .iter()
            .find(|row| {
                leaves(row)
                    .iter()
                    .any(|leaf| leaf.kind == LeafKind::FootnoteReference)
            })
            .ok_or("reference")?;
        let definition = rows
            .iter()
            .find(|row| row.gray == Some(GrayKind::Footnote))
            .ok_or("definition")?;
        assert_eq!(definition.label, Some(ContentLabel::Footnote));
        assert!(reference.source.start < definition.source.start);
        assert_eq!(drawn(&parsed, definition), "def");
        let reference_source = parsed
            .navigation()
            .entries()
            .iter()
            .find(|entry| entry.kind() == aline_markdown::NavigationKind::FootnoteReference)
            .map(|entry| entry.source().byte_range().start)
            .ok_or("reference entry")?;
        let intent = navigation_intent(&parsed, reference_source).ok_or("intent")?;
        let NavigationIntent::Jump { target } = intent;
        assert!(target.contains(&definition.source.start));
        let back = navigation_intent(&parsed, definition.source.start).ok_or("back")?;
        let NavigationIntent::Jump { target } = back;
        assert!(target.contains(&reference_source));
        assert_eq!(navigation_intent(&parsed, 0), None);
        Ok(())
    }

    #[test]
    fn duplicate_headings_keep_distinct_toc_targets() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("# T\n\n[TOC]\n\n# T\n\n# T\n")?;
        let toc = index
            .rows()
            .iter()
            .find_map(|row| match &row.block {
                Block::Toc { entries, .. } => Some(entries),
                _ => None,
            })
            .ok_or("toc")?;
        assert_eq!(toc.len(), 3);
        assert!(toc.iter().all(|entry| entry.title == "T"));
        let mut starts: Vec<usize> = toc.iter().map(|entry| entry.source.start).collect();
        starts.sort_unstable();
        starts.dedup();
        assert_eq!(starts.len(), 3);
        // 目录点击目标就是标题来源；重复标题各自保留自己的来源，不做去重。
        let headings: Vec<usize> = parsed
            .navigation()
            .entries()
            .iter()
            .filter(|entry| entry.kind() == aline_markdown::NavigationKind::Heading)
            .map(|entry| entry.source().byte_range().start)
            .collect();
        assert_eq!(headings, starts);
        Ok(())
    }

    #[test]
    fn footnote_reference_keyboard_target_stays_inside_the_row() -> Result<(), Box<dyn Error>> {
        let (parsed, index) =
            index("# 标题\n\n前[^n]后\n\n[^n]: 第一定义\n\n另一段[^m]\n\n[^m]: 第二定义\n")?;
        let rows = index.rows();
        let reference_rows: Vec<&ContentRow> = rows
            .iter()
            .filter(|row| {
                leaves(row)
                    .iter()
                    .any(|leaf| leaf.kind == LeafKind::FootnoteReference)
            })
            .collect();
        assert_eq!(reference_rows.len(), 2);
        let targets: Vec<usize> = parsed
            .navigation()
            .entries()
            .iter()
            .filter(|entry| entry.kind() == NavigationKind::FootnoteReference)
            .filter_map(|entry| entry.targets().first())
            .map(|target| target.byte_range().start)
            .collect();
        assert_eq!(targets.len(), 2);
        assert_ne!(targets[0], targets[1]);
        assert_eq!(
            footnote_reference_target(&parsed, &reference_rows[0].source),
            Some(targets[0])
        );
        assert_eq!(
            footnote_reference_target(&parsed, &reference_rows[1].source),
            Some(targets[1])
        );
        let heading = rows.first().ok_or("heading")?;
        assert_eq!(footnote_reference_target(&parsed, &heading.source), None);
        Ok(())
    }

    #[test]
    fn footnote_definition_keyboard_target_returns_its_reference() -> Result<(), Box<dyn Error>> {
        let (parsed, index) =
            index("# 标题\n\n前[^n]后\n\n[^n]: 第一定义\n\n另一段[^m]\n\n[^m]: 第二定义\n")?;
        let definition_rows: Vec<&ContentRow> = index
            .rows()
            .iter()
            .filter(|row| row.gray == Some(GrayKind::Footnote))
            .collect();
        assert_eq!(definition_rows.len(), 2);
        let references: Vec<usize> = parsed
            .navigation()
            .entries()
            .iter()
            .filter(|entry| entry.kind() == NavigationKind::FootnoteReference)
            .map(|entry| entry.source().byte_range().start)
            .collect();
        assert_eq!(references.len(), 2);
        assert_eq!(
            footnote_definition_target(&parsed, &definition_rows[0].source),
            Some(references[0])
        );
        assert_eq!(
            footnote_definition_target(&parsed, &definition_rows[1].source),
            Some(references[1])
        );
        let heading = index.rows().first().ok_or("heading")?;
        assert_eq!(footnote_definition_target(&parsed, &heading.source), None);
        Ok(())
    }

    #[test]
    fn multiple_footnote_references_share_one_back_jump_target() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("a[^n] b[^n]\n\n[^n]: def\n")?;
        let reference_leaves = index
            .rows()
            .iter()
            .flat_map(leaves)
            .filter(|leaf| leaf.kind == LeafKind::FootnoteReference)
            .count();
        assert_eq!(reference_leaves, 2);
        let references: Vec<usize> = parsed
            .navigation()
            .entries()
            .iter()
            .filter(|entry| entry.kind() == aline_markdown::NavigationKind::FootnoteReference)
            .map(|entry| entry.source().byte_range().start)
            .collect();
        assert_eq!(references.len(), 2);
        let definition = index
            .rows()
            .iter()
            .find(|row| row.gray == Some(GrayKind::Footnote))
            .ok_or("definition")?;
        let Some(NavigationIntent::Jump { target }) =
            navigation_intent(&parsed, definition.source.start)
        else {
            return Err("back jump".into());
        };
        assert!(target.contains(&references[0]));
        assert!(!target.contains(&references[1]));
        Ok(())
    }

    #[test]
    fn empty_and_indented_code_blocks_use_plain_code_rows() -> Result<(), Box<dyn Error>> {
        let (_parsed, index) = index("```\n```\n\n    indented\n")?;
        let mut code = Vec::new();
        for row in index.rows() {
            if let Block::Code { info, lines, .. } = &row.block {
                code.push((
                    info.clone(),
                    lines.len(),
                    lines.iter().all(|line| line.has_line_break()),
                ));
            }
        }
        assert_eq!(code.len(), 2);
        assert_eq!(code[0].0, "");
        assert_eq!(code[1].0, "");
        assert!(code.iter().all(|(_, lines, _)| *lines >= 1));
        Ok(())
    }

    #[test]
    fn long_code_lines_keep_one_number_per_source_line() -> Result<(), Box<dyn Error>> {
        let long = "长".repeat(200);
        let source = format!("```unknown-language\n{long}\n第二行\n\n末行\n```\n");
        let (parsed, index) = index(&source)?;
        let row = index.row(0).ok_or("row")?;
        let Block::Code { info, lines, .. } = &row.block else {
            return Err("code block".into());
        };
        // 未知语言只作为只读标识保留，不改变代码行结构。
        assert_eq!(info, "unknown-language");
        // 长行不因布局软折行拆成多个逻辑行：行号仍按源码行 1..=4。
        assert_eq!(
            lines.iter().map(|line| line.number()).collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        let text = parsed.projection().drawn_text();
        assert_eq!(&text[lines[0].drawn_range().byte_range()], long);
        assert_eq!(&text[lines[1].drawn_range().byte_range()], "第二行");
        assert_eq!(&text[lines[2].drawn_range().byte_range()], "");
        assert_eq!(&text[lines[3].drawn_range().byte_range()], "末行");
        Ok(())
    }

    #[test]
    fn toc_entries_follow_navigation_targets_and_levels() -> Result<(), Box<dyn Error>> {
        let (_parsed, index) = index("# T\n\n[TOC]\n\n## A\n\n### B\n")?;
        let toc = index
            .rows()
            .iter()
            .find_map(|row| match &row.block {
                Block::Toc { entries, .. } => Some(entries),
                _ => None,
            })
            .ok_or("toc")?;
        assert_eq!(toc.len(), 3);
        assert_eq!(
            toc.iter()
                .map(|entry| entry.title.as_str())
                .collect::<Vec<_>>(),
            ["T", "A", "B"]
        );
        assert_eq!(
            toc.iter().map(|entry| entry.level).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        Ok(())
    }

    #[test]
    fn html_styles_map_to_visible_runs_only() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("<u>under <b>bold</b></u> and <mark>hi</mark>\n")?;
        assert_eq!(index.len(), 1);
        let row = index.row(0).ok_or("row")?;
        let under = text_leaf(&parsed, row, "under").ok_or("under")?;
        assert!(under.style.underline && !under.style.bold);
        let bold = text_leaf(&parsed, row, " bold").ok_or("bold")?;
        assert!(bold.style.underline && bold.style.bold);
        let hi = text_leaf(&parsed, row, "hi").ok_or("hi")?;
        assert!(hi.style.mark);
        assert_eq!(drawn(&parsed, row), "under bold and hi");
        Ok(())
    }

    #[test]
    fn literal_fallbacks_keep_text_and_never_create_resources() -> Result<(), Box<dyn Error>> {
        let (_parsed, index) = index(
            "<b>bold &amp; aligned</b>\n\n<img src=\"remote.png\" alt=\"remote\">\n\n<script>keep as protected literal</script>\n\n![diagram](./missing.svg)\n\n$$\nx^2\n$$\n\n```mermaid\ngraph TD; A-->B;\n```\n",
        )?;
        let rows = index.rows();
        let html = rows
            .iter()
            .find(|row| drawn(&_parsed, row).starts_with("<img"))
            .ok_or("img literal")?;
        assert!(matches!(html.block, Block::Text { .. }));
        assert!(
            rows.iter()
                .any(|row| drawn(&_parsed, row).contains("<script>"))
        );
        let image = rows
            .iter()
            .flat_map(leaves)
            .find(|leaf| leaf.kind == LeafKind::Image)
            .ok_or("image alt")?;
        assert_eq!(
            &_parsed.projection().drawn_text()[image.drawn.clone()],
            "diagram"
        );
        assert!(
            rows.iter()
                .flat_map(leaves)
                .any(|leaf| leaf.kind == LeafKind::Math)
        );
        assert!(
            rows.iter()
                .any(|row| matches!(row.block, Block::Code { ref info, .. } if info == "mermaid"))
        );
        Ok(())
    }

    #[test]
    fn rows_are_ordered_and_cover_the_source() -> Result<(), Box<dyn Error>> {
        for source in [
            "",
            "a\n",
            "# H\n\npara\n\n> quote\n\n- a\n- b\n\n| a | b |\n| - | - |\n| 1 | 2 |\n",
        ] {
            let (parsed, index) = index(source)?;
            let rows = index.rows();
            assert!(!rows.is_empty());
            assert_eq!(rows[0].source.start, 0);
            assert_eq!(
                rows[rows.len() - 1].source.end,
                parsed.snapshot().len_bytes()
            );
            for pair in rows.windows(2) {
                assert!(pair[0].source.start <= pair[1].source.start);
                assert!(pair[0].source.end <= pair[1].source.start);
            }
        }
        Ok(())
    }

    #[test]
    fn row_budget_is_enforced() -> Result<(), Box<dyn Error>> {
        let parsed = parsed("a\n\nb\n\nc\n")?;
        assert_eq!(
            ContentIndex::new(&parsed, 1).err(),
            Some(ContentError::Budget)
        );
        Ok(())
    }

    #[test]
    fn gap_rows_keep_source_text_available() -> Result<(), Box<dyn Error>> {
        let (parsed, index) = index("[ref]: https://example.test\n\nuse [ref]\n")?;
        let rows = index.rows();
        let gap = rows
            .iter()
            .find(|row| {
                row.drawn.is_none() && !matches!(row.block, Block::Rule | Block::FrontMatter)
            })
            .ok_or("gap")?;
        assert!(
            parsed
                .snapshot()
                .slice(
                    &parsed.snapshot().src_range(gap.source.clone())?,
                    TextBudget::new(1 << 20)
                )?
                .contains("[ref]:")
        );
        Ok(())
    }
}
