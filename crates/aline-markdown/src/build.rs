//! pulldown-cmark 事件到块、间隙、映射片段、保护与诊断的无头构树。

use std::ops::Range;

use aline_buffer::TextSnapshot;
use pulldown_cmark::{Event, Tag, TagEnd};

use crate::budget::{CancelCheck, ParseBudget, ParseStats};
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::mapping::{MappingPrecision, MappingSegment};
use crate::mdast::{Block, BlockKind, Gap};
use crate::pre::Rewrite;
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
}

struct RootBlock {
    start: usize,
    kind: BlockKind,
    has_display_math: bool,
    has_non_math_visible: bool,
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
    last_source_end: usize,
    depth: usize,
    root: Option<RootBlock>,
    code_depth: usize,
    table_depth: usize,
    table_cells: usize,
    html_stack: HtmlStack,
    nodes: usize,
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
        last_source_end: 0,
        depth: 0,
        root: None,
        code_depth: 0,
        table_depth: 0,
        table_cells: 0,
        html_stack: HtmlStack::default(),
        nodes: 0,
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
    builder.finish()
}

impl Builder<'_, '_> {
    fn event(&mut self, event: Event<'_>, range: Range<usize>) -> Result<(), BuildError> {
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
            self.root = Some(RootBlock {
                start: source.start,
                kind,
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

    fn footnote_reference(&mut self, _name: &str, range: Range<usize>) -> Result<(), BuildError> {
        let source = self.source_of(range)?;
        let slice = self.source.get(source.clone()).ok_or(BuildError::Overlap)?;
        self.push_visible(slice, source.clone(), MappingPrecision::Exact)?;
        self.protected
            .push((source.clone(), ProtectionKind::FootnoteReference));
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
        let range = start..trimmed;
        self.blocks.push((range.clone(), kind));
        if kind == BlockKind::FootnoteDefinition {
            self.protected
                .push((range.clone(), ProtectionKind::FootnoteDefinition));
        }
        if kind == BlockKind::HtmlBlock {
            self.protected.push((range, ProtectionKind::Html));
        }
        if kind == BlockKind::Paragraph
            && let Some(slice) = self.source.get(start..trimmed)
            && slice.trim().eq_ignore_ascii_case("[toc]")
        {
            self.protected.push((start..trimmed, ProtectionKind::Toc));
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
        Ok(Built {
            drawn: self.drawn,
            segments: self.segments,
            blocks,
            gaps,
            protected,
            styled,
            diagnostics,
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

fn trim_line_end(source: &str, mut range: Range<usize>) -> Range<usize> {
    while range.start < range.end {
        match source.as_bytes().get(range.end.saturating_sub(1)).copied() {
            Some(b'\n' | b'\r') => range.end = range.end.saturating_sub(1),
            _ => break,
        }
    }
    range
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
