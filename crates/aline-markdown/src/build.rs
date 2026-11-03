//! pulldown-cmark 事件到块、间隙、映射片段、保护与诊断的无头构树。

use std::cmp::Ordering;
use std::ops::Range;

use aline_buffer::TextSnapshot;
use pulldown_cmark::{Event, HeadingLevel, Tag, TagEnd};

use crate::budget::{CancelCheck, ParseBudget, ParseStats};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::mapping::{MappingPrecision, MappingSegment};
use crate::mdast::{Block, BlockKind, Gap};
use crate::navigation::{NavigationEntry, NavigationIndex, NavigationKind};
use crate::pre::Rewrite;
use crate::presentation::{self, DraftNode, PresentationBuilder};
use crate::protection::{ProtectedRange, ProtectionKind};
use crate::style::{StyledRange, TextStyle};
use crate::tree::html::{
    HtmlError, HtmlOutput, HtmlSegment, HtmlSegmentKind, HtmlStack, HtmlStyle, is_html_whitespace,
};

/// 构树失败；调用方必须回退原文，不发布部分结果。
#[derive(Debug)]
pub(crate) enum BuildError {
    /// 事件源码范围重叠或反向。
    Overlap,
    /// 源码范围或投影构造失败。
    Mapping,
    /// 重写范围无法映射回源码。
    Unmapped,
    /// 注入预算耗尽。
    BudgetExceeded,
    /// 受检分配失败。
    Allocation,
    /// 调用方请求取消；不返回部分树。
    Cancelled,
}

/// 一次成功构树的全部无头结果。
pub(crate) struct Built {
    pub(crate) drawn: String,
    pub(crate) segments: Vec<MappingSegment>,
    pub(crate) blocks: Vec<Block>,
    pub(crate) gaps: Vec<Gap>,
    pub(crate) protected: Vec<ProtectedRange>,
    pub(crate) styled: Vec<StyledRange>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) navigation: NavigationIndex,
    pub(crate) presentation: Vec<DraftNode>,
}

struct RootBlock {
    start: usize,
    kind: BlockKind,
    heading_level: Option<u8>,
    footnote_label: Option<String>,
    has_display_math: bool,
    has_non_math_visible: bool,
}

/// 构树期间暂存的导航草稿；目标在全部事件结束后统一解析。
struct NavigationDraft {
    source: Range<usize>,
    kind: NavigationDraftKind,
}

enum NavigationDraftKind {
    Heading { level: u8 },
    Toc,
    FootnoteReference { label: String },
    FootnoteDefinition { label: String },
}

struct Builder<'a, 'c> {
    snapshot: &'a TextSnapshot,
    source: &'a str,
    rewrite: &'a Rewrite,
    cancel: &'c dyn CancelCheck,
    budget: ParseBudget,
    drawn: String,
    segments: Vec<MappingSegment>,
    blocks: Vec<(Range<usize>, BlockKind)>,
    protected: Vec<(Range<usize>, ProtectionKind)>,
    styled: Vec<(Range<usize>, TextStyle)>,
    diagnostics: Vec<(Range<usize>, DiagnosticKind)>,
    navigation: Vec<NavigationDraft>,
    last_source_end: usize,
    depth: usize,
    root: Option<RootBlock>,
    code_depth: usize,
    table_depth: usize,
    table_cells: usize,
    html_stack: HtmlStack,
    nodes: usize,
    presentation: PresentationBuilder,
}

pub(crate) fn build<'a, 'c>(
    snapshot: &TextSnapshot,
    source: &str,
    rewrite: &Rewrite,
    events: impl Iterator<Item = (Event<'a>, Range<usize>)>,
    budget: ParseBudget,
    cancel: &'c dyn CancelCheck,
    stats: &mut ParseStats,
) -> Result<Built, BuildError> {
    let mut builder = Builder {
        snapshot,
        source,
        rewrite,
        cancel,
        budget,
        drawn: String::new(),
        segments: Vec::new(),
        blocks: Vec::new(),
        protected: Vec::new(),
        styled: Vec::new(),
        diagnostics: Vec::new(),
        navigation: Vec::new(),
        last_source_end: 0,
        depth: 0,
        root: None,
        code_depth: 0,
        table_depth: 0,
        table_cells: 0,
        html_stack: HtmlStack::default(),
        nodes: 0,
        presentation: PresentationBuilder::new(budget),
    };
    builder
        .drawn
        .try_reserve(source.len())
        .map_err(|_| BuildError::Allocation)?;
    builder
        .segments
        .try_reserve(events.size_hint().0.saturating_mul(2).saturating_add(1))
        .map_err(|_| BuildError::Allocation)?;
    if let Some(front_matter) = rewrite.front_matter() {
        let range = front_matter.range();
        if let Err(error) = builder.presentation.front_matter(range.clone()) {
            if matches!(error, BuildError::BudgetExceeded) {
                stats.budget_hits = stats.budget_hits.saturating_add(1);
            }
            return Err(error);
        }
        builder.blocks.push((range.clone(), BlockKind::FrontMatter));
        builder.protected.push((range, ProtectionKind::FrontMatter));
    }
    for (event, range) in events {
        if cancel.is_cancelled() {
            return Err(BuildError::Cancelled);
        }
        if let Event::Html(raw) | Event::InlineHtml(raw) = &event {
            // 统计必须带上扫描器当前的过滤标签状态，否则过滤标签内部会被误算成
            // 多次标签/文本迭代；真实扫描器仍是预算执行的唯一权威。
            stats.scan_steps = stats
                .scan_steps
                .saturating_add(builder.html_stack.count_scan_steps(raw));
        }
        builder.nodes = builder.nodes.saturating_add(1);
        if builder.nodes > budget.max_nodes() {
            stats.budget_hits = stats.budget_hits.saturating_add(1);
            return Err(BuildError::BudgetExceeded);
        }
        match builder.event(event, range) {
            Ok(()) => {}
            Err(BuildError::BudgetExceeded) => {
                stats.budget_hits = stats.budget_hits.saturating_add(1);
                return Err(BuildError::BudgetExceeded);
            }
            Err(error) => return Err(error),
        }
    }
    match builder.flush_hidden(source.len()) {
        Ok(()) => {}
        Err(BuildError::BudgetExceeded) => {
            stats.budget_hits = stats.budget_hits.saturating_add(1);
            return Err(BuildError::BudgetExceeded);
        }
        Err(error) => return Err(error),
    }
    match builder.finish() {
        Ok(built) => Ok(built),
        Err(BuildError::BudgetExceeded) => {
            stats.budget_hits = stats.budget_hits.saturating_add(1);
            Err(BuildError::BudgetExceeded)
        }
        Err(error) => Err(error),
    }
}

impl Builder<'_, '_> {
    fn event(&mut self, event: Event<'_>, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range.clone())?;
        let start_kind = match &event {
            Event::Start(tag) => Some(self.presentation.prepare_tag(
                tag,
                self.depth == 0
                    && is_toc_paragraph(self.source, &trim_line_end(self.source, source.clone())),
                self.cancel,
            )?),
            _ => None,
        };
        let is_end = matches!(event, Event::End(_));
        let leaf_kind = presentation::leaf_kind(&event);
        let drawn_start = self.drawn.len();
        self.event_output(event, range)?;
        if let Some(kind) = start_kind {
            self.presentation.start(kind, source, self.drawn.len())?;
        } else if is_end {
            self.presentation.end(source.end, self.drawn.len())?;
        } else if let Some(kind) = leaf_kind {
            self.presentation
                .leaf(kind, source, drawn_start..self.drawn.len())?;
        }
        Ok(())
    }

    fn event_output(&mut self, event: Event<'_>, range: Range<usize>) -> Result<(), BuildError> {
        match event {
            Event::Start(tag) => self.start(tag, range),
            Event::End(tag) => self.end(tag, range),
            Event::Text(text) => self.text(&text, range),
            Event::Code(text) => self.code(&text, range),
            Event::InlineMath(text) => self.math(&text, range, false),
            Event::DisplayMath(text) => self.math(&text, range, true),
            Event::Html(raw) | Event::InlineHtml(raw) => self.html(&raw, range),
            Event::SoftBreak => self.break_event(range, false),
            Event::HardBreak => self.break_event(range, true),
            Event::Rule => self.rule(range),
            Event::FootnoteReference(name) => self.footnote_reference(&name, range),
            Event::TaskListMarker(checked) => self.task_marker(checked, range),
        }
    }

    fn start(&mut self, tag: Tag<'_>, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        if self.depth == 0
            && let Some(kind) = block_kind(&tag)
        {
            let (heading_level, footnote_label) = match &tag {
                Tag::Heading { level, .. } => (Some(heading_level(*level)), None),
                Tag::FootnoteDefinition(name) => (None, Some(name.as_ref().to_owned())),
                _ => (None, None),
            };
            self.root = Some(RootBlock {
                start: source.start,
                kind,
                heading_level,
                footnote_label,
                has_display_math: false,
                has_non_math_visible: false,
            });
        }
        if matches!(tag, Tag::CodeBlock(_)) {
            self.code_depth = self.code_depth.saturating_add(1);
        }
        if matches!(tag, Tag::Table(_)) {
            self.table_depth = self.table_depth.saturating_add(1);
            self.table_cells = 0;
        }
        if matches!(tag, Tag::TableCell) && self.table_depth > 0 {
            if self.table_cells > 0 {
                self.push_synthetic("\t", source.start)?;
            }
            self.table_cells = self.table_cells.saturating_add(1);
        }
        self.depth = self.depth.saturating_add(1);
        if self.depth > self.budget.max_nesting() {
            return Err(BuildError::BudgetExceeded);
        }
        Ok(())
    }

    fn end(&mut self, tag: TagEnd, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        if matches!(tag, TagEnd::CodeBlock) {
            self.code_depth = self.code_depth.saturating_sub(1);
        }
        if matches!(tag, TagEnd::Table) {
            self.table_depth = self.table_depth.saturating_sub(1);
            self.table_cells = 0;
        }
        self.depth = self.depth.saturating_sub(1);
        if self.depth == 0
            && let Some(root) = self.root.take()
        {
            self.finish_block(root, source.end)?;
        }
        Ok(())
    }

    fn text(&mut self, text: &str, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        // HTML 文本上下文由有限 HTML 扫描器折叠空白；实体解码后的文本整体按
        // Aligned 片段回指完整实体源码。
        if self.html_stack.in_text_context() {
            self.note_html_gap(&source);
            let output = self.html_stack.text(text, source).map_err(html_error)?;
            return self.push_html_output(&output);
        }
        self.resolve_html_pending(source.start, text)?;
        let source_slice = self.source.get(source.clone()).ok_or(BuildError::Overlap)?;
        let precision = if source_slice == text {
            MappingPrecision::Exact
        } else {
            MappingPrecision::Aligned
        };
        self.push_visible(text, source.clone(), precision)?;
        if self
            .root
            .as_ref()
            .is_some_and(|_| text.chars().any(|character| !character.is_whitespace()))
            && let Some(root) = self.root.as_mut()
        {
            root.has_non_math_visible = true;
        }
        self.record_html_style(source);
        Ok(())
    }

    fn code(&mut self, text: &str, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        if text.is_empty() {
            self.push_visible(text, source, MappingPrecision::Aligned)?;
            return Ok(());
        }
        self.push_visible(text, source.clone(), MappingPrecision::Aligned)?;
        self.mark_visible();
        self.record_html_style(source);
        Ok(())
    }

    fn math(&mut self, _text: &str, range: Range<usize>, display: bool) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        let slice = self.source.get(source.clone()).ok_or(BuildError::Overlap)?;
        self.push_visible(slice, source.clone(), MappingPrecision::Exact)?;
        if display {
            if let Some(root) = self.root.as_mut() {
                root.has_display_math = true;
            }
        } else {
            self.mark_visible();
        }
        self.record_html_style(source);
        Ok(())
    }

    fn html(&mut self, raw: &str, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        let in_html_block = self
            .root
            .as_ref()
            .is_some_and(|root| root.kind == BlockKind::HtmlBlock);
        if !in_html_block {
            self.protected.push((source.clone(), ProtectionKind::Html));
        }
        let result = if in_html_block {
            self.html_stack.block(raw, source.start, self.budget)
        } else {
            self.html_stack.inline(raw, source.start, self.budget)
        };
        match result {
            Ok(output) => self.push_html_output(&output)?,
            // 扫描或标签栈超限时不能只放弃当前事件：此前事件可能已隐藏标签，
            // 局部回退会吞掉这些源码。构树失败让入口整体回到原文视图，保证不截断。
            Err(error) => return Err(html_error(error)),
        }
        Ok(())
    }

    fn break_event(&mut self, range: Range<usize>, hard: bool) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        // HTML 文本里的软换行是空白，折叠为至多一个空格，并与事件间被丢弃的
        // 空白一起由待合并片段携带源码。
        if !hard && self.html_stack.in_text_context() && !source.is_empty() {
            self.note_html_gap(&source);
            let style = self.html_stack.style();
            self.html_stack
                .note_whitespace(source.start, source.end, style);
            return Ok(());
        }
        if source.is_empty() {
            self.push_synthetic("\n", source.start)?;
            return Ok(());
        }
        let slice = self.source.get(source.clone()).ok_or(BuildError::Overlap)?;
        let precision = if slice == "\n" {
            MappingPrecision::Exact
        } else {
            MappingPrecision::Aligned
        };
        self.push_visible("\n", source.clone(), precision)?;
        if hard {
            self.mark_visible();
        }
        self.record_html_style(source);
        Ok(())
    }

    fn rule(&mut self, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        if self.depth == 0 {
            let trimmed = trim_line_end(self.source, source);
            if trimmed.start < trimmed.end {
                self.blocks.push((trimmed, BlockKind::ThematicBreak));
            }
        }
        Ok(())
    }

    fn footnote_reference(&mut self, name: &str, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        let slice = self.source.get(source.clone()).ok_or(BuildError::Overlap)?;
        self.push_visible(slice, source.clone(), MappingPrecision::Exact)?;
        self.protected
            .push((source.clone(), ProtectionKind::FootnoteReference));
        self.navigation.push(NavigationDraft {
            source: source.clone(),
            kind: NavigationDraftKind::FootnoteReference {
                label: name.to_owned(),
            },
        });
        self.mark_visible();
        self.record_html_style(source);
        Ok(())
    }

    fn task_marker(&mut self, checked: bool, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        let text = if checked { "\u{2611} " } else { "\u{2610} " };
        self.push_synthetic(text, source.start)?;
        self.mark_visible();
        Ok(())
    }

    fn finish_block(&mut self, root: RootBlock, end: usize) -> Result<(), BuildError> {
        self.html_stack.finish_block();
        self.check_cancel()?;
        let start = root.start;
        if end <= start {
            return Ok(());
        }
        let mut trimmed = end;
        while trimmed > start {
            let byte = self
                .source
                .as_bytes()
                .get(trimmed.saturating_sub(1))
                .copied();
            if byte == Some(b'\n') || byte == Some(b'\r') {
                trimmed = trimmed.saturating_sub(1);
            } else {
                break;
            }
        }
        if trimmed <= start {
            return Ok(());
        }
        let kind = if root.kind == BlockKind::Paragraph
            && root.has_display_math
            && !root.has_non_math_visible
        {
            BlockKind::Math
        } else {
            root.kind
        };
        let heading_level = root.heading_level;
        let footnote_label = root.footnote_label;
        let range = start..trimmed;
        self.blocks.push((range.clone(), kind));
        if kind == BlockKind::FootnoteDefinition {
            self.protected
                .push((range.clone(), ProtectionKind::FootnoteDefinition));
        }
        if kind == BlockKind::HtmlBlock {
            self.protected.push((range.clone(), ProtectionKind::Html));
        }
        let toc = kind == BlockKind::Paragraph && is_toc_paragraph(self.source, &range);
        if toc {
            self.protected.push((range.clone(), ProtectionKind::Toc));
        }
        if let Some(level) = heading_level
            && kind == BlockKind::Heading
        {
            self.navigation.push(NavigationDraft {
                source: range.clone(),
                kind: NavigationDraftKind::Heading { level },
            });
        }
        if let Some(label) = footnote_label
            && kind == BlockKind::FootnoteDefinition
        {
            self.navigation.push(NavigationDraft {
                source: range,
                kind: NavigationDraftKind::FootnoteDefinition { label },
            });
        } else if toc {
            self.navigation.push(NavigationDraft {
                source: range,
                kind: NavigationDraftKind::Toc,
            });
        }
        Ok(())
    }

    fn source_of(&self, range: Range<usize>) -> Result<Range<usize>, BuildError> {
        self.rewrite
            .map()
            .to_source(range)
            .map(|mapped| mapped.source_range())
            .map_err(|_| BuildError::Unmapped)
    }

    fn push_visible(
        &mut self,
        text: &str,
        source: Range<usize>,
        precision: MappingPrecision,
    ) -> Result<(), BuildError> {
        if source.start > source.end {
            return Err(BuildError::Overlap);
        }
        self.flush_hidden(source.start)?;
        if text.is_empty() {
            return Ok(());
        }
        let source_range = self
            .snapshot
            .src_range(source.clone())
            .map_err(|_| BuildError::Overlap)?;
        let precision = if source.is_empty() {
            MappingPrecision::Synthetic
        } else if precision == MappingPrecision::Exact && text.len() != source.len() {
            MappingPrecision::Aligned
        } else {
            precision
        };
        if precision == MappingPrecision::Aligned && (text.is_empty() || source.is_empty()) {
            return Err(BuildError::Overlap);
        }
        let drawn_start = self.drawn.len();
        self.drawn.push_str(text);
        let segment = MappingSegment::new(drawn_start..self.drawn.len(), source_range, precision)
            .map_err(|_| BuildError::Mapping)?;
        self.segments.push(segment);
        self.last_source_end = source.end;
        self.check_segments()
    }

    fn push_synthetic(&mut self, text: &str, anchor: usize) -> Result<(), BuildError> {
        if text.is_empty() {
            return Ok(());
        }
        let source = self
            .snapshot
            .src_range(anchor..anchor)
            .map_err(|_| BuildError::Overlap)?;
        let drawn_start = self.drawn.len();
        self.drawn.push_str(text);
        let segment = MappingSegment::new(
            drawn_start..self.drawn.len(),
            source,
            MappingPrecision::Synthetic,
        )
        .map_err(|_| BuildError::Mapping)?;
        self.segments.push(segment);
        self.check_segments()
    }

    fn flush_hidden(&mut self, until: usize) -> Result<(), BuildError> {
        self.check_cancel()?;
        if until < self.last_source_end {
            return Err(BuildError::Overlap);
        }
        if until == self.last_source_end {
            return Ok(());
        }
        let source = self
            .snapshot
            .src_range(self.last_source_end..until)
            .map_err(|_| BuildError::Overlap)?;
        let drawn = self.drawn.len();
        let segment = MappingSegment::new(drawn..drawn, source, MappingPrecision::Hidden)
            .map_err(|_| BuildError::Mapping)?;
        self.segments.push(segment);
        self.last_source_end = until;
        self.check_segments()
    }

    fn check_segments(&self) -> Result<(), BuildError> {
        if self.segments.len() > self.budget.max_segments() {
            return Err(BuildError::BudgetExceeded);
        }
        Ok(())
    }

    fn check_cancel(&self) -> Result<(), BuildError> {
        if self.cancel.is_cancelled() {
            Err(BuildError::Cancelled)
        } else {
            Ok(())
        }
    }

    fn mark_visible(&mut self) {
        if let Some(root) = self.root.as_mut() {
            root.has_non_math_visible = true;
        }
    }

    fn record_html_style(&mut self, source: Range<usize>) {
        let style = self.html_stack.style();
        if style != HtmlStyle::default() && !source.is_empty() {
            self.styled.push((source, style_of(style)));
        }
    }

    /// 把有限 HTML 扫描输出写入全局投影：块级换行去重、首尾折叠空白丢弃，
    /// 其余片段保留扫描器给出的精确/对齐/合成来源。
    fn push_html_output(&mut self, output: &HtmlOutput) -> Result<(), BuildError> {
        for segment in &output.segments {
            let text = output.drawn.get(segment.drawn.clone()).unwrap_or_default();
            match segment.kind {
                HtmlSegmentKind::BlockBreak => {
                    if self.drawn.is_empty() || self.drawn.ends_with('\n') {
                        continue;
                    }
                    self.push_synthetic("\n", segment.source.start)?;
                }
                HtmlSegmentKind::CollapsedSpace => {
                    if self.drawn.is_empty()
                        || self.drawn.ends_with('\n')
                        || self.drawn.ends_with(' ')
                    {
                        continue;
                    }
                    self.push_visible(text, segment.source.clone(), segment.precision)?;
                    self.push_html_style(segment);
                }
                HtmlSegmentKind::Text => {
                    if segment.precision == MappingPrecision::Synthetic {
                        self.push_synthetic(text, segment.source.start)?;
                    } else {
                        self.push_visible(text, segment.source.clone(), segment.precision)?;
                        self.push_html_style(segment);
                    }
                }
            }
        }
        if output
            .drawn
            .chars()
            .any(|character| !character.is_whitespace())
        {
            self.mark_visible();
        }
        Ok(())
    }

    fn push_html_style(&mut self, segment: &HtmlSegment) {
        if segment.style != HtmlStyle::default() && !segment.source.is_empty() {
            self.styled
                .push((segment.source.clone(), style_of(segment.style)));
        }
    }

    /// 事件间被 pulldown 丢弃的空白（例如软换行旁的缩进空格）并入欠写空白。
    fn note_html_gap(&mut self, source: &Range<usize>) {
        if let Some(end) = self.html_stack.pending_end()
            && let Some(gap) = self.source.get(end..source.start)
        {
            self.html_stack.note_source_gap(gap);
        }
    }

    /// 在非 HTML 文本事件前结算欠写空白：折叠空格必须覆盖全部被消费空白。
    fn resolve_html_pending(&mut self, before: usize, next: &str) -> Result<(), BuildError> {
        let Some((start, mut end, style)) = self.html_stack.take_pending() else {
            return Ok(());
        };
        if next.chars().next().is_some_and(is_html_whitespace) {
            return Ok(());
        }
        if end > before {
            end = before;
        }
        if start >= end || start < self.last_source_end {
            return Ok(());
        }
        if self.drawn.is_empty() || self.drawn.ends_with('\n') || self.drawn.ends_with(' ') {
            return Ok(());
        }
        let Some(slice) = self.source.get(start..end) else {
            return Ok(());
        };
        if !slice.chars().all(is_html_whitespace) {
            return Ok(());
        }
        self.flush_hidden(start)?;
        let source_range = self
            .snapshot
            .src_range(start..end)
            .map_err(|_| BuildError::Overlap)?;
        let drawn_start = self.drawn.len();
        self.drawn.push(' ');
        let segment = MappingSegment::new(
            drawn_start..self.drawn.len(),
            source_range,
            MappingPrecision::Aligned,
        )
        .map_err(|_| BuildError::Mapping)?;
        self.segments.push(segment);
        self.last_source_end = end;
        if style != HtmlStyle::default() {
            self.styled.push((start..end, style_of(style)));
        }
        self.check_segments()
    }

    fn finish_navigation(&self) -> Result<NavigationIndex, BuildError> {
        self.check_cancel()?;
        let entries = self.navigation.len();
        let mut headings = Vec::new();
        let mut tocs = Vec::new();
        let mut definitions = Vec::new();
        let mut references = Vec::new();
        headings
            .try_reserve(entries)
            .map_err(|_| BuildError::Allocation)?;
        tocs.try_reserve(entries)
            .map_err(|_| BuildError::Allocation)?;
        definitions
            .try_reserve(entries)
            .map_err(|_| BuildError::Allocation)?;
        references
            .try_reserve(entries)
            .map_err(|_| BuildError::Allocation)?;
        for (index, draft) in self.navigation.iter().enumerate() {
            self.check_cancel()?;
            match draft.kind {
                NavigationDraftKind::Heading { .. } => headings.push(index),
                NavigationDraftKind::Toc => tocs.push(index),
                NavigationDraftKind::FootnoteDefinition { .. } => definitions.push(index),
                NavigationDraftKind::FootnoteReference { .. } => references.push(index),
            }
        }
        for index in definitions.iter().chain(references.iter()) {
            if self.navigation_label(*index).is_none() {
                return Err(BuildError::Mapping);
            }
        }
        headings.sort_by_key(|index| self.navigation_source_key(*index));
        for sorted in [&mut definitions, &mut references] {
            sorted.sort_by(|left, right| {
                compare_footnote_labels(
                    self.navigation_label_for_sort(*left),
                    self.navigation_label_for_sort(*right),
                )
                .then_with(|| {
                    self.navigation_source_key(*left)
                        .cmp(&self.navigation_source_key(*right))
                })
            });
        }

        // 预检只使用分组计数；任何目标向量分配都必须发生在这里之后。
        let mut slots = entries;
        if slots > self.budget.max_nodes() {
            return Err(BuildError::BudgetExceeded);
        }
        let toc_slots = tocs
            .len()
            .checked_mul(headings.len())
            .ok_or(BuildError::BudgetExceeded)?;
        slots = slots
            .checked_add(toc_slots)
            .ok_or(BuildError::BudgetExceeded)?;
        let mut definition_index = 0usize;
        let mut reference_index = 0usize;
        while definition_index < definitions.len() && reference_index < references.len() {
            let definition_label = self
                .navigation_label(definitions[definition_index])
                .ok_or(BuildError::Mapping)?;
            let reference_label = self
                .navigation_label(references[reference_index])
                .ok_or(BuildError::Mapping)?;
            match compare_footnote_labels(definition_label, reference_label) {
                Ordering::Less => {
                    definition_index =
                        self.label_run_end(&definitions, definition_label, definition_index)?;
                }
                Ordering::Greater => {
                    reference_index =
                        self.label_run_end(&references, reference_label, reference_index)?;
                }
                Ordering::Equal => {
                    let definition_end =
                        self.label_run_end(&definitions, definition_label, definition_index)?;
                    let reference_end =
                        self.label_run_end(&references, reference_label, reference_index)?;
                    let product = (definition_end - definition_index)
                        .checked_mul(reference_end - reference_index)
                        .ok_or(BuildError::BudgetExceeded)?;
                    let both = product.checked_mul(2).ok_or(BuildError::BudgetExceeded)?;
                    slots = slots.checked_add(both).ok_or(BuildError::BudgetExceeded)?;
                    if slots > self.budget.max_nodes() {
                        return Err(BuildError::BudgetExceeded);
                    }
                    definition_index = definition_end;
                    reference_index = reference_end;
                }
            }
        }
        if slots > self.budget.max_nodes() {
            return Err(BuildError::BudgetExceeded);
        }

        let mut heading_ranges = Vec::new();
        heading_ranges
            .try_reserve(headings.len())
            .map_err(|_| BuildError::Allocation)?;
        for index in &headings {
            self.check_cancel()?;
            let draft = self.navigation.get(*index).ok_or(BuildError::Mapping)?;
            heading_ranges.push(
                self.snapshot
                    .src_range(draft.source.clone())
                    .map_err(|_| BuildError::Overlap)?,
            );
        }

        let mut output = Vec::new();
        output
            .try_reserve(entries)
            .map_err(|_| BuildError::Allocation)?;
        for draft in &self.navigation {
            self.check_cancel()?;
            let targets = match &draft.kind {
                NavigationDraftKind::Heading { .. } => Vec::new(),
                NavigationDraftKind::Toc => {
                    let mut targets = Vec::new();
                    targets
                        .try_reserve(heading_ranges.len())
                        .map_err(|_| BuildError::Allocation)?;
                    targets.extend(heading_ranges.iter().cloned());
                    targets
                }
                NavigationDraftKind::FootnoteReference { label } => {
                    self.target_ranges(&definitions, label)?
                }
                NavigationDraftKind::FootnoteDefinition { label } => {
                    self.target_ranges(&references, label)?
                }
            };
            let source = self
                .snapshot
                .src_range(draft.source.clone())
                .map_err(|_| BuildError::Overlap)?;
            let (kind, heading_level) = match &draft.kind {
                NavigationDraftKind::Heading { level } => (NavigationKind::Heading, Some(*level)),
                NavigationDraftKind::Toc => (NavigationKind::Toc, None),
                NavigationDraftKind::FootnoteReference { .. } => {
                    (NavigationKind::FootnoteReference, None)
                }
                NavigationDraftKind::FootnoteDefinition { .. } => {
                    (NavigationKind::FootnoteDefinition, None)
                }
            };
            output.push(NavigationEntry::new(kind, source, targets, heading_level));
        }
        output.sort_by_key(|entry| {
            let source = entry.source().byte_range();
            (source.start, source.end)
        });
        Ok(NavigationIndex::new(output))
    }

    fn navigation_label(&self, index: usize) -> Option<&str> {
        match &self.navigation.get(index)?.kind {
            NavigationDraftKind::FootnoteReference { label }
            | NavigationDraftKind::FootnoteDefinition { label } => Some(label),
            NavigationDraftKind::Heading { .. } | NavigationDraftKind::Toc => None,
        }
    }

    fn navigation_label_for_sort(&self, index: usize) -> &str {
        self.navigation_label(index).unwrap_or_default()
    }

    fn navigation_source(&self, index: usize) -> Option<&Range<usize>> {
        self.navigation.get(index).map(|draft| &draft.source)
    }

    fn navigation_source_key(&self, index: usize) -> (usize, usize) {
        self.navigation_source(index)
            .map_or((usize::MAX, usize::MAX), |source| {
                (source.start, source.end)
            })
    }

    fn label_run_end(
        &self,
        indices: &[usize],
        label: &str,
        mut start: usize,
    ) -> Result<usize, BuildError> {
        while start < indices.len() {
            self.check_cancel()?;
            let current = self
                .navigation_label(indices[start])
                .ok_or(BuildError::Mapping)?;
            if !footnote_labels_equal(current, label) {
                break;
            }
            start += 1;
        }
        Ok(start)
    }

    fn label_range(&self, indices: &[usize], label: &str) -> Result<Range<usize>, BuildError> {
        let start = indices.partition_point(|index| {
            self.navigation_label(*index)
                .is_some_and(|value| compare_footnote_labels(value, label) == Ordering::Less)
        });
        let end = indices.partition_point(|index| {
            self.navigation_label(*index)
                .is_some_and(|value| compare_footnote_labels(value, label) != Ordering::Greater)
        });
        if start > end {
            return Err(BuildError::Mapping);
        }
        for index in &indices[start..end] {
            self.check_cancel()?;
            if !self
                .navigation_label(*index)
                .is_some_and(|value| footnote_labels_equal(value, label))
            {
                return Err(BuildError::Mapping);
            }
        }
        Ok(start..end)
    }

    fn target_ranges(
        &self,
        indices: &[usize],
        label: &str,
    ) -> Result<Vec<aline_buffer::SrcRange>, BuildError> {
        let range = self.label_range(indices, label)?;
        let mut targets = Vec::new();
        targets
            .try_reserve(range.len())
            .map_err(|_| BuildError::Allocation)?;
        for index in &indices[range] {
            self.check_cancel()?;
            let draft = self.navigation.get(*index).ok_or(BuildError::Mapping)?;
            targets.push(
                self.snapshot
                    .src_range(draft.source.clone())
                    .map_err(|_| BuildError::Overlap)?,
            );
        }
        Ok(targets)
    }

    fn finish(mut self) -> Result<Built, BuildError> {
        self.blocks.sort_by_key(|(range, _)| range.start);
        let mut previous_end = 0usize;
        for (range, _) in &self.blocks {
            self.check_cancel()?;
            if range.start < previous_end {
                return Err(BuildError::Overlap);
            }
            previous_end = range.end;
        }
        let mut blocks = Vec::new();
        blocks
            .try_reserve(self.blocks.len())
            .map_err(|_| BuildError::Allocation)?;
        for (range, kind) in &self.blocks {
            self.check_cancel()?;
            let source = self
                .snapshot
                .src_range(range.clone())
                .map_err(|_| BuildError::Overlap)?;
            blocks.push(Block::new(source, *kind));
        }
        let mut gaps = Vec::new();
        let mut previous_end = 0usize;
        for (range, _) in &self.blocks {
            self.check_cancel()?;
            if range.start > previous_end {
                let source = self
                    .snapshot
                    .src_range(previous_end..range.start)
                    .map_err(|_| BuildError::Overlap)?;
                gaps.push(Gap::new(source));
            }
            previous_end = range.end;
        }
        if previous_end < self.source.len() {
            let source = self
                .snapshot
                .src_range(previous_end..self.source.len())
                .map_err(|_| BuildError::Overlap)?;
            gaps.push(Gap::new(source));
        }
        let protected = self.finish_protected()?;
        let styled = self.finish_styled()?;
        let diagnostics = self.finish_diagnostics()?;
        let navigation = self.finish_navigation()?;
        let presentation = self.presentation.finish()?;
        Ok(Built {
            drawn: self.drawn,
            segments: self.segments,
            blocks,
            gaps,
            protected,
            styled,
            diagnostics,
            navigation,
            presentation,
        })
    }

    fn finish_protected(&mut self) -> Result<Vec<ProtectedRange>, BuildError> {
        self.protected
            .sort_by_key(|(range, _)| (range.start, range.end));
        let mut merged: Vec<(Range<usize>, ProtectionKind)> = Vec::new();
        for (range, kind) in &self.protected {
            self.check_cancel()?;
            if let Some((last, last_kind)) = merged.last_mut()
                && *last_kind == *kind
                && range.start <= last.end
            {
                if range.end > last.end {
                    last.end = range.end;
                }
                continue;
            }
            merged.push((range.clone(), *kind));
        }
        let mut output = Vec::new();
        output
            .try_reserve(merged.len())
            .map_err(|_| BuildError::Allocation)?;
        for (range, kind) in merged {
            self.check_cancel()?;
            let source = self
                .snapshot
                .src_range(range)
                .map_err(|_| BuildError::Overlap)?;
            output.push(ProtectedRange::new(source, kind));
        }
        Ok(output)
    }

    fn finish_styled(&mut self) -> Result<Vec<StyledRange>, BuildError> {
        let cancel = self.cancel;
        let mut output = Vec::new();
        output
            .try_reserve(self.styled.len())
            .map_err(|_| BuildError::Allocation)?;
        for (range, style) in self.styled.drain(..) {
            if cancel.is_cancelled() {
                return Err(BuildError::Cancelled);
            }
            let source = self
                .snapshot
                .src_range(range)
                .map_err(|_| BuildError::Overlap)?;
            output.push(StyledRange::new(source, style));
        }
        Ok(output)
    }

    fn finish_diagnostics(&mut self) -> Result<Vec<Diagnostic>, BuildError> {
        let cancel = self.cancel;
        let mut output = Vec::new();
        output
            .try_reserve(self.diagnostics.len())
            .map_err(|_| BuildError::Allocation)?;
        for (range, kind) in self.diagnostics.drain(..) {
            if cancel.is_cancelled() {
                return Err(BuildError::Cancelled);
            }
            let source = self
                .snapshot
                .src_range(range)
                .map_err(|_| BuildError::Overlap)?;
            output.push(Diagnostic::new(source, kind));
        }
        Ok(output)
    }
}

/// 与 pulldown-cmark 的 `UniCase` 脚注键一致：仅折叠 ASCII 字母大小写。
fn compare_footnote_labels(left: &str, right: &str) -> Ordering {
    left.bytes()
        .map(|byte| byte.to_ascii_lowercase())
        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
}

fn footnote_labels_equal(left: &str, right: &str) -> bool {
    compare_footnote_labels(left, right) == Ordering::Equal
}

fn trim_line_end(source: &str, mut range: Range<usize>) -> Range<usize> {
    while range.start < range.end {
        match source.as_bytes().get(range.end.saturating_sub(1)).copied() {
            Some(b'\n' | b'\r') => range.end = range.end.saturating_sub(1),
            _ => break,
        }
    }
    range
}

fn is_toc_paragraph(source: &str, range: &Range<usize>) -> bool {
    source
        .get(range.clone())
        .is_some_and(|slice| slice.trim().eq_ignore_ascii_case("[toc]"))
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

fn block_kind(tag: &Tag<'_>) -> Option<BlockKind> {
    match tag {
        Tag::Paragraph => Some(BlockKind::Paragraph),
        Tag::Heading { .. } => Some(BlockKind::Heading),
        Tag::BlockQuote(_) => Some(BlockKind::BlockQuote),
        Tag::CodeBlock(_) => Some(BlockKind::CodeBlock),
        Tag::HtmlBlock => Some(BlockKind::HtmlBlock),
        Tag::List(_) => Some(BlockKind::List),
        Tag::FootnoteDefinition(_) => Some(BlockKind::FootnoteDefinition),
        Tag::Table(_) => Some(BlockKind::Table),
        _ => None,
    }
}

fn style_of(style: HtmlStyle) -> TextStyle {
    TextStyle::new(
        style.bold,
        style.italic,
        style.strike,
        style.underline,
        style.mark,
        style.code,
        style.link,
    )
}

fn html_error(error: HtmlError) -> BuildError {
    match error {
        HtmlError::BudgetExceeded => BuildError::BudgetExceeded,
        HtmlError::Allocation(_) => BuildError::Allocation,
    }
}

#[cfg(test)]
mod presentation_budget_tests {
    use std::error::Error;

    use aline_buffer::{TextBudget, TextSnapshot};
    use pulldown_cmark::{Alignment, Event, Tag, TagEnd};

    use super::{BuildError, build};
    use crate::budget::{NeverCancel, ParseBudget, ParseStats};
    use crate::pre::rewrite_front_matter;

    #[test]
    fn front_matter_node_budget_is_checked_during_build() -> Result<(), Box<dyn Error>> {
        let source = "---\na: b\n---\n";
        let budget = ParseBudget::m0().with_max_nodes(0);
        let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(1024))?;
        let rewrite = rewrite_front_matter(source, budget)?;
        let mut stats = ParseStats::default();
        let result = build(
            &snapshot,
            source,
            &rewrite,
            std::iter::empty(),
            budget,
            &NeverCancel,
            &mut stats,
        );
        assert!(matches!(result, Err(BuildError::BudgetExceeded)));
        assert_eq!(stats.budget_hits(), 1);
        Ok(())
    }

    #[test]
    fn table_alignment_budget_precedes_child_events_and_allocation() -> Result<(), Box<dyn Error>> {
        let source = "| a | b | c | d | e |\n";
        let budget = ParseBudget::m0().with_max_nodes(4);
        let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(1024))?;
        let rewrite = rewrite_front_matter(source, budget)?;
        // 故障注入只交付表格开始/结束，证明列向量不能等待后续单元格事件才检查预算。
        let events = vec![
            (
                Event::Start(Tag::Table(vec![Alignment::None; 5])),
                0..source.len(),
            ),
            (Event::End(TagEnd::Table), 0..source.len()),
        ];
        let mut stats = ParseStats::default();
        let result = build(
            &snapshot,
            source,
            &rewrite,
            events.into_iter(),
            budget,
            &NeverCancel,
            &mut stats,
        );
        assert!(matches!(result, Err(BuildError::BudgetExceeded)));
        assert_eq!(stats.budget_hits(), 1);
        Ok(())
    }
}
