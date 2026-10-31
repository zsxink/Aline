//! 有限 HTML 片段解析；只产生可见文本、样式和源码片段，不执行脚本、不联网。
//!
//! 输入是 pulldown-cmark 给出的原始 HTML 片段（行内标签或 HTML 块的一行/多行），
//! 本模块不把重写坐标当作源码坐标；调用方传入片段在快照中的绝对起点。
//!
//! 展示行为对齐 `docs/engine-design.md` §11.2：支持标签映射样式；过滤集合按字面
//! 文本显示（内部原始文本不再当作标签解析）；块级标签产生合成换行；HTML 文本
//! 折叠连续空白并让折叠后的空格携带被消费的完整源码范围；注释与处理指令不显示。

use std::fmt;
use std::ops::Range;

use crate::budget::ParseBudget;
use crate::mapping::MappingPrecision;

/// 有限 HTML 的可见样式集合；只影响展示，不改变源码范围。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct HtmlStyle {
    pub(crate) bold: bool,
    pub(crate) italic: bool,
    pub(crate) strike: bool,
    pub(crate) underline: bool,
    pub(crate) mark: bool,
    pub(crate) code: bool,
    pub(crate) link: bool,
}

impl HtmlStyle {
    fn merge(self, other: Self) -> Self {
        Self {
            bold: self.bold || other.bold,
            italic: self.italic || other.italic,
            strike: self.strike || other.strike,
            underline: self.underline || other.underline,
            mark: self.mark || other.mark,
            code: self.code || other.code,
            link: self.link || other.link,
        }
    }
}

/// 可见片段的来源类别；构树侧据此去重合成换行、丢弃首尾折叠空白。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HtmlSegmentKind {
    /// 普通可见文本，含实体解码与过滤标签字面文本。
    Text,
    /// 块级标签边界产生的合成换行。
    BlockBreak,
    /// 连续空白折叠成的单个空格。
    CollapsedSpace,
}

/// 一个可见 HTML 片段及其绝对源码范围。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HtmlSegment {
    pub(crate) drawn: Range<usize>,
    pub(crate) source: Range<usize>,
    pub(crate) precision: MappingPrecision,
    pub(crate) style: HtmlStyle,
    pub(crate) kind: HtmlSegmentKind,
}

/// 有限 HTML 解析输出；drawn 是片段内局部文本。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct HtmlOutput {
    pub(crate) drawn: String,
    pub(crate) segments: Vec<HtmlSegment>,
}

/// 有限 HTML 解析失败；调用方必须回退原文，不得截断。
#[derive(Debug)]
pub(crate) enum HtmlError {
    /// 标签栈或扫描预算耗尽。
    BudgetExceeded,
    /// 受检分配失败。
    Allocation(std::collections::TryReserveError),
}

impl fmt::Display for HtmlError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for HtmlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
struct OpenTag {
    name: String,
    style: HtmlStyle,
}

/// 欠写空白：延迟到下一段可见内容再折叠，保证空格片段覆盖全部被消费空白。
#[derive(Clone, Debug)]
struct PendingSpace {
    start: usize,
    end: usize,
    style: HtmlStyle,
}

/// 跨事件保持的有限 HTML 标签栈。
#[derive(Clone, Debug, Default)]
pub(crate) struct HtmlStack {
    open: Vec<OpenTag>,
    filtered: Option<String>,
    pending: Option<PendingSpace>,
    steps: usize,
}

impl HtmlStack {
    /// 返回当前累计样式；过滤标签内部按字面文本显示，不继承或新增样式。
    pub(crate) fn style(&self) -> HtmlStyle {
        if self.filtered.is_some() {
            return HtmlStyle::default();
        }
        self.open
            .last()
            .map_or_else(HtmlStyle::default, |tag| tag.style)
    }

    /// 当前文本事件是否位于 HTML 文本上下文（需要折叠空白）。
    pub(crate) fn in_text_context(&self) -> bool {
        self.filtered.is_none() && !self.open.is_empty()
    }

    /// 返回待合并空白的源码终点，供构树侧识别事件间被丢弃的空白。
    pub(crate) fn pending_end(&self) -> Option<usize> {
        self.pending.as_ref().map(|pending| pending.end)
    }

    /// 若事件间源码间隙全为 HTML 空白，把它并入待合并空白。
    pub(crate) fn note_source_gap(&mut self, gap: &str) {
        if gap.is_empty() || !gap.chars().all(is_html_whitespace) {
            return;
        }
        if let Some(pending) = self.pending.as_mut() {
            pending.end = pending.end.saturating_add(gap.len());
        }
    }

    /// 记录一个空白源码区间；相邻区间合并为一次折叠。
    pub(crate) fn note_whitespace(&mut self, start: usize, end: usize, style: HtmlStyle) {
        if start >= end {
            return;
        }
        match self.pending.as_mut() {
            Some(pending) if pending.end == start || pending.start == start => {
                pending.end = pending.end.max(end);
            }
            Some(_) | None => {
                self.pending = Some(PendingSpace { start, end, style });
            }
        }
    }

    /// 取走待合并空白；调用方负责校验来源后写入片段。
    pub(crate) fn take_pending(&mut self) -> Option<(usize, usize, HtmlStyle)> {
        self.pending
            .take()
            .map(|pending| (pending.start, pending.end, pending.style))
    }

    /// 块结束：未闭合的过滤标签与尾部空白都不跨块延续。
    pub(crate) fn finish_block(&mut self) {
        self.filtered = None;
        self.pending = None;
    }

    /// 统计一个原始 HTML 事件在真实扫描器中会消耗的迭代步数。
    ///
    /// 以当前过滤标签状态为起点，复用与 [`HtmlStack::parse`] 相同的过滤集合和
    /// 闭标签规则；预算中止时仍统计完整事件，便于调用方跨次比较阈值。
    pub(crate) fn count_scan_steps(&self, raw: &str) -> usize {
        html_scan_steps(raw, self.filtered.as_deref())
    }

    /// 解析一个行内 HTML 事件；标签栈跨事件保持。
    pub(crate) fn inline(
        &mut self,
        raw: &str,
        base: usize,
        budget: ParseBudget,
    ) -> Result<HtmlOutput, HtmlError> {
        self.parse(raw, base, budget, false)
    }

    /// 解析一个 HTML 块事件（可能包含多行）；标签栈跨事件保持。
    pub(crate) fn block(
        &mut self,
        raw: &str,
        base: usize,
        budget: ParseBudget,
    ) -> Result<HtmlOutput, HtmlError> {
        self.parse(raw, base, budget, true)
    }

    /// 解析 HTML 上下文中的一个 Text 事件。
    ///
    /// 普通文本按事件源码逐段折叠空白；实体解码后的文本长度与源码不同，整体记为
    /// Aligned 片段，使展示字符仍能回指完整实体源码。
    pub(crate) fn text(
        &mut self,
        raw: &str,
        source: Range<usize>,
    ) -> Result<HtmlOutput, HtmlError> {
        let mut output = HtmlOutput::default();
        output
            .drawn
            .try_reserve(raw.len())
            .map_err(HtmlError::Allocation)?;
        if raw.is_empty() {
            return Ok(output);
        }
        if source.is_empty() {
            self.resolve_pending(&mut output);
            self.push_synthetic(
                &mut output,
                raw,
                source.start,
                self.style(),
                HtmlSegmentKind::Text,
            );
            return Ok(output);
        }
        if raw.len() == source.end.saturating_sub(source.start) {
            self.push_collapsed(&mut output, raw, source.start, self.style());
        } else {
            self.resolve_pending(&mut output);
            Self::push_segment(
                &mut output,
                raw,
                source,
                self.style(),
                MappingPrecision::Aligned,
                HtmlSegmentKind::Text,
            );
        }
        Ok(output)
    }

    fn parse(
        &mut self,
        raw: &str,
        base: usize,
        budget: ParseBudget,
        _block: bool,
    ) -> Result<HtmlOutput, HtmlError> {
        let mut output = HtmlOutput::default();
        output
            .drawn
            .try_reserve(raw.len())
            .map_err(HtmlError::Allocation)?;
        let mut index = 0usize;
        while index < raw.len() {
            self.steps = self.steps.saturating_add(1);
            if self.steps > budget.max_scan_steps() {
                return Err(HtmlError::BudgetExceeded);
            }
            let rest = raw.get(index..).ok_or(HtmlError::BudgetExceeded)?;
            let absolute = base.saturating_add(index);
            if let Some(name) = self.filtered.clone() {
                // 过滤标签内部是原始文本：整段按字面显示，直到大小写不敏感的闭标签。
                if let Some(close_end) = filtered_close_end(rest, &name) {
                    let literal = rest.get(..close_end).ok_or(HtmlError::BudgetExceeded)?;
                    self.push_literal(&mut output, literal, absolute, HtmlStyle::default());
                    self.filtered = None;
                    index = index.saturating_add(close_end);
                } else {
                    self.push_literal(&mut output, rest, absolute, HtmlStyle::default());
                    break;
                }
                continue;
            }
            if rest.starts_with('<') {
                if rest.starts_with("<!--") {
                    if let Some(end) = rest.find("-->") {
                        index = index.saturating_add(end + 3);
                        continue;
                    }
                    self.push_text(&mut output, rest, absolute, HtmlStyle::default());
                    break;
                }
                if rest.starts_with("<?") {
                    if let Some(end) = rest.find("?>") {
                        index = index.saturating_add(end + 2);
                        continue;
                    }
                    self.push_text(&mut output, rest, absolute, self.style());
                    break;
                }
                if let Some((tag_end, markup)) = find_tag_end(rest) {
                    self.handle_tag(markup, absolute, &mut output, budget)?;
                    index = index.saturating_add(tag_end);
                    continue;
                }
                self.push_text(&mut output, rest, absolute, self.style());
                break;
            }
            let end = rest.find('<').unwrap_or(rest.len());
            let text = rest.get(..end).ok_or(HtmlError::BudgetExceeded)?;
            self.push_text(&mut output, text, absolute, self.style());
            index = index.saturating_add(end);
        }
        Ok(output)
    }

    fn handle_tag(
        &mut self,
        markup: &str,
        absolute: usize,
        output: &mut HtmlOutput,
        budget: ParseBudget,
    ) -> Result<(), HtmlError> {
        let trimmed = markup.trim();
        if let Some(inner) = trimmed.strip_prefix("</") {
            let name = tag_name(inner);
            if is_filtered_tag(&name) {
                // 过滤集合的闭标签同样按字面显示，不继承样式。
                self.push_literal(output, markup, absolute, HtmlStyle::default());
                return Ok(());
            }
            self.pop(&name);
            if is_block_tag(&name) {
                self.push_block_break(output, absolute);
            }
            return Ok(());
        }
        let inner = trimmed.strip_prefix('<').unwrap_or(trimmed);
        let name = tag_name(inner);
        if is_filtered_tag(&name) {
            // 过滤集合与 void 集合互不相交：这里的顺序对 input/link/meta/base 没有
            // 影响，它们仍走下方 void 分支（标签隐藏、无可见内容）；只有过滤集合中
            // 的标签才按字面显示。复核 N-F3-1 更正了原先“避免抢占字面路径”的说法。
            self.push_literal(output, markup, absolute, HtmlStyle::default());
            self.filtered = Some(name);
            return Ok(());
        }
        if name == "br" {
            self.pending = None;
            let style = self.style();
            self.push_synthetic(output, "\n", absolute, style, HtmlSegmentKind::Text);
            return Ok(());
        }
        // V-07 待定：input/link/meta/base 等不在过滤集合中的 void 标签仍按
        // “不支持标签”处理（标签隐藏、无可见内容）。§11.2 尚未冻结其展示口径，
        // 这里保持现状，不在组 6.5 中宣称对齐。
        if is_void_tag(&name) {
            return Ok(());
        }
        if is_block_tag(&name) {
            self.push_block_break(output, absolute);
        }
        let style = style_for_tag(&name, inner);
        if is_supported_tag(&name) {
            if self.open.len() >= budget.max_html_depth() {
                return Err(HtmlError::BudgetExceeded);
            }
            let merged = self.style().merge(style);
            self.open.try_reserve(1).map_err(HtmlError::Allocation)?;
            self.open.push(OpenTag {
                name,
                style: merged,
            });
            return Ok(());
        }
        let style = self.style();
        self.push_literal(output, markup, absolute, style);
        Ok(())
    }

    fn pop(&mut self, name: &str) {
        if let Some(position) = self.open.iter().rposition(|tag| tag.name == name) {
            self.open.truncate(position);
        }
    }

    fn push_text(
        &mut self,
        output: &mut HtmlOutput,
        text: &str,
        absolute: usize,
        style: HtmlStyle,
    ) {
        let mut cursor = 0usize;
        while cursor < text.len() {
            let rest = text.get(cursor..).unwrap_or_default();
            let Some(amp) = rest.find('&') else {
                self.push_collapsed(output, rest, absolute.saturating_add(cursor), style);
                break;
            };
            if amp > 0 {
                let plain = rest.get(..amp).unwrap_or_default();
                self.push_collapsed(output, plain, absolute.saturating_add(cursor), style);
                cursor = cursor.saturating_add(amp);
                continue;
            }
            if let Some(entity_end) = rest.find(';') {
                let entity = rest.get(..=entity_end).unwrap_or_default();
                if let Some(decoded) = decode_entity(entity) {
                    let precision = if decoded == entity {
                        MappingPrecision::Exact
                    } else {
                        MappingPrecision::Aligned
                    };
                    self.resolve_pending(output);
                    let start = absolute.saturating_add(cursor);
                    let end = start.saturating_add(entity.len());
                    Self::push_segment(
                        output,
                        &decoded,
                        start..end,
                        style,
                        precision,
                        HtmlSegmentKind::Text,
                    );
                    cursor = cursor.saturating_add(entity_end + 1);
                    continue;
                }
            }
            self.resolve_pending(output);
            let start = absolute.saturating_add(cursor);
            Self::push_segment(
                output,
                "&",
                start..start.saturating_add(1),
                style,
                MappingPrecision::Exact,
                HtmlSegmentKind::Text,
            );
            cursor = cursor.saturating_add(1);
        }
    }

    /// 按空白/非空白分段；连续空白先欠写，遇到可见文本时折叠为一个空格。
    fn push_collapsed(
        &mut self,
        output: &mut HtmlOutput,
        text: &str,
        absolute: usize,
        style: HtmlStyle,
    ) {
        let mut cursor = 0usize;
        while cursor < text.len() {
            let Some(rest) = text.get(cursor..) else {
                break;
            };
            let Some(first) = rest.chars().next() else {
                break;
            };
            let whitespace = is_html_whitespace(first);
            let mut end = cursor;
            for (offset, character) in rest.char_indices() {
                if is_html_whitespace(character) != whitespace {
                    break;
                }
                end = cursor
                    .saturating_add(offset)
                    .saturating_add(character.len_utf8());
            }
            if end <= cursor {
                break;
            }
            let Some(chunk) = text.get(cursor..end) else {
                break;
            };
            let chunk_start = absolute.saturating_add(cursor);
            let chunk_end = absolute.saturating_add(end);
            if whitespace {
                self.note_whitespace(chunk_start, chunk_end, style);
            } else {
                self.resolve_pending(output);
                Self::push_segment(
                    output,
                    chunk,
                    chunk_start..chunk_end,
                    style,
                    MappingPrecision::Exact,
                    HtmlSegmentKind::Text,
                );
            }
            cursor = end;
        }
    }

    fn resolve_pending(&mut self, output: &mut HtmlOutput) {
        let Some(pending) = self.pending.take() else {
            return;
        };
        if pending.start >= pending.end {
            return;
        }
        Self::push_segment(
            output,
            " ",
            pending.start..pending.end,
            pending.style,
            MappingPrecision::Aligned,
            HtmlSegmentKind::CollapsedSpace,
        );
    }

    fn push_literal(
        &mut self,
        output: &mut HtmlOutput,
        text: &str,
        source: usize,
        style: HtmlStyle,
    ) {
        if text.is_empty() {
            return;
        }
        self.resolve_pending(output);
        let end = source.saturating_add(text.len());
        Self::push_segment(
            output,
            text,
            source..end,
            style,
            MappingPrecision::Exact,
            HtmlSegmentKind::Text,
        );
    }

    fn push_segment(
        output: &mut HtmlOutput,
        text: &str,
        source: Range<usize>,
        style: HtmlStyle,
        precision: MappingPrecision,
        kind: HtmlSegmentKind,
    ) {
        if text.is_empty() {
            return;
        }
        if let Some(last) = output.segments.last_mut()
            && last.kind == kind
            && last.precision == precision
            && last.style == style
            && last.drawn.end == output.drawn.len()
            && last.source.end == source.start
        {
            output.drawn.push_str(text);
            last.drawn.end = output.drawn.len();
            last.source.end = source.end;
            return;
        }
        let drawn_start = output.drawn.len();
        output.drawn.push_str(text);
        output.segments.push(HtmlSegment {
            drawn: drawn_start..output.drawn.len(),
            source,
            precision,
            style,
            kind,
        });
    }

    fn push_synthetic(
        &mut self,
        output: &mut HtmlOutput,
        text: &str,
        anchor: usize,
        style: HtmlStyle,
        kind: HtmlSegmentKind,
    ) {
        if text.is_empty() {
            return;
        }
        let drawn_start = output.drawn.len();
        output.drawn.push_str(text);
        output.segments.push(HtmlSegment {
            drawn: drawn_start..output.drawn.len(),
            source: anchor..anchor,
            precision: MappingPrecision::Synthetic,
            style,
            kind,
        });
    }

    /// 块级标签边界：先丢弃尾部欠写空白，再产生一个合成换行。
    fn push_block_break(&mut self, output: &mut HtmlOutput, anchor: usize) {
        self.pending = None;
        self.push_synthetic(
            output,
            "\n",
            anchor,
            HtmlStyle::default(),
            HtmlSegmentKind::BlockBreak,
        );
    }
}

/// HTML 空白集合：空格、制表、换行与回车都折叠为一个空格。
pub(crate) fn is_html_whitespace(character: char) -> bool {
    matches!(character, ' ' | '\t' | '\n' | '\r')
}

/// 在 `rest` 中查找大小写不敏感的 `</name ...>` 结束位置（包含闭标签本身）。
fn filtered_close_end(rest: &str, name: &str) -> Option<usize> {
    let lower = rest.to_ascii_lowercase();
    let needle = format!("</{name}");
    let mut search = 0usize;
    while let Some(haystack) = lower.get(search..) {
        let offset = haystack.find(&needle)?;
        let start = search.saturating_add(offset);
        let after = lower
            .as_bytes()
            .get(start.saturating_add(needle.len()))
            .copied();
        if after.is_none_or(|byte| byte == b'>' || byte == b'/' || byte.is_ascii_whitespace()) {
            let tail = rest.get(start..)?;
            let (tag_end, _) = find_tag_end(tail)?;
            return Some(start.saturating_add(tag_end));
        }
        search = start.saturating_add(needle.len());
    }
    None
}

fn find_tag_end(rest: &str) -> Option<(usize, &str)> {
    let mut quote: Option<char> = None;
    for (offset, character) in rest.char_indices() {
        match character {
            '"' | '\'' => {
                if quote == Some(character) {
                    quote = None;
                } else if quote.is_none() {
                    quote = Some(character);
                }
            }
            '>' if quote.is_none() => {
                let end = offset.saturating_add(1);
                return rest.get(..end).map(|markup| (end, markup));
            }
            _ => {}
        }
    }
    None
}

/// 按 [`HtmlStack::parse`] 的循环粒度统计一段原始 HTML 的迭代步数。
///
/// `filtered` 是处理该事件前扫描器所处的过滤标签。过滤集合与闭标签查找直接复用
/// 真实扫描器的 [`is_filtered_tag`]/[`filtered_close_end`]，因此过滤标签内部的
/// “整段消费到闭标签”只计一步，与 `max_scan_steps` 的实际阈值一致。预算中止时
/// 仍返回完整片段的步数，调用方因此可以跨次比较阈值。
pub(crate) fn html_scan_steps(raw: &str, filtered: Option<&str>) -> usize {
    let mut steps = 0usize;
    let mut index = 0usize;
    let mut filtered = filtered.map(str::to_owned);
    while index < raw.len() {
        steps = steps.saturating_add(1);
        let rest = raw.get(index..).unwrap_or_default();
        if let Some(name) = filtered.as_deref() {
            if let Some(close_end) = filtered_close_end(rest, name) {
                index = index.saturating_add(close_end);
                filtered = None;
                continue;
            }
            break;
        }
        if rest.starts_with('<') {
            if rest.starts_with("<!--") {
                match rest.find("-->") {
                    Some(end) => index = index.saturating_add(end.saturating_add(3)),
                    None => break,
                }
                continue;
            }
            if rest.starts_with("<?") {
                match rest.find("?>") {
                    Some(end) => index = index.saturating_add(end.saturating_add(2)),
                    None => break,
                }
                continue;
            }
            if let Some((tag_end, markup)) = find_tag_end(rest) {
                let trimmed = markup.trim();
                if !trimmed.starts_with("</") {
                    let inner = trimmed.strip_prefix('<').unwrap_or(trimmed);
                    let name = tag_name(inner);
                    if is_filtered_tag(&name) {
                        filtered = Some(name);
                    }
                }
                index = index.saturating_add(tag_end);
                continue;
            }
            break;
        }
        let end = rest.find('<').unwrap_or(rest.len());
        index = index.saturating_add(end);
    }
    steps
}

fn tag_name(inner: &str) -> String {
    let end = inner
        .find(|character: char| character.is_whitespace() || character == '/' || character == '>')
        .unwrap_or(inner.len());
    inner.get(..end).unwrap_or_default().to_ascii_lowercase()
}

fn attribute(inner: &str, name: &str) -> Option<String> {
    let lower = inner.to_ascii_lowercase();
    let mut search = 0usize;
    while let Some(offset) = lower.get(search..)?.find(name) {
        let start = search.saturating_add(offset);
        let before_ok = start == 0
            || lower
                .as_bytes()
                .get(start.saturating_sub(1))
                .is_none_or(|byte| byte.is_ascii_whitespace());
        if !before_ok {
            search = start.saturating_add(name.len());
            continue;
        }
        let mut cursor = start.saturating_add(name.len());
        while lower
            .as_bytes()
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor = cursor.saturating_add(1);
        }
        if lower.as_bytes().get(cursor) != Some(&b'=') {
            search = start.saturating_add(name.len());
            continue;
        }
        cursor = cursor.saturating_add(1);
        while lower
            .as_bytes()
            .get(cursor)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            cursor = cursor.saturating_add(1);
        }
        let quote = lower.as_bytes().get(cursor).copied();
        if quote == Some(b'"') || quote == Some(b'\'') {
            let quote_char = char::from(quote.unwrap_or(b'"'));
            let value_start = cursor.saturating_add(1);
            let value_rest = inner.get(value_start..)?;
            let value_end = value_rest.find(quote_char)?;
            return value_rest.get(..value_end).map(ToString::to_string);
        }
        let value_rest = inner.get(cursor..)?;
        let value_end = value_rest
            .find(|character: char| character.is_whitespace() || character == '>')
            .unwrap_or(value_rest.len());
        return value_rest.get(..value_end).map(ToString::to_string);
    }
    None
}

fn style_for_tag(name: &str, inner: &str) -> HtmlStyle {
    let mut style = HtmlStyle::default();
    match name {
        "b" | "strong" => style.bold = true,
        "i" | "em" => style.italic = true,
        "s" | "del" | "strike" => style.strike = true,
        "u" => style.underline = true,
        "mark" => style.mark = true,
        "code" | "kbd" => style.code = true,
        "a" => {
            if let Some(href) = attribute(inner, "href") {
                style.link = is_allowed_link(&href);
            }
        }
        _ => {}
    }
    style
}

fn is_allowed_link(href: &str) -> bool {
    let trimmed = href.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('#') || trimmed.starts_with('/') {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

/// void 标签不产生可见内容。`input/link/meta/base` 的展示口径属于 V-07 待定项。
fn is_void_tag(name: &str) -> bool {
    matches!(
        name,
        "img" | "hr" | "input" | "meta" | "link" | "source" | "area" | "base" | "col"
    )
}

/// 过滤集合：整体按字面文本显示，内部原始文本不再解析为标签。
fn is_filtered_tag(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "style"
            | "iframe"
            | "title"
            | "textarea"
            | "xmp"
            | "noembed"
            | "noframes"
            | "plaintext"
            | "object"
            | "embed"
            | "form"
            | "button"
            | "svg"
            | "math"
    )
}

/// 块级标签：边界产生合成换行（§11.2）。
fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "div"
            | "p"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "ul"
            | "ol"
            | "li"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "table"
            | "tr"
            | "td"
            | "th"
            | "blockquote"
            | "pre"
    )
}

fn is_supported_tag(name: &str) -> bool {
    matches!(
        name,
        "b" | "strong"
            | "i"
            | "em"
            | "s"
            | "del"
            | "strike"
            | "u"
            | "mark"
            | "code"
            | "kbd"
            | "a"
            | "span"
            | "thead"
            | "tbody"
    ) || is_block_tag(name)
}

fn decode_entity(entity: &str) -> Option<String> {
    let body = entity.strip_prefix('&')?.strip_suffix(';')?;
    if let Some(numeric) = body.strip_prefix('#') {
        let value = if let Some(hex) = numeric
            .strip_prefix('x')
            .or_else(|| numeric.strip_prefix('X'))
        {
            u32::from_str_radix(hex, 16).ok()?
        } else {
            numeric.parse::<u32>().ok()?
        };
        return Some(char::from_u32(value).unwrap_or('\u{fffd}').to_string());
    }
    let decoded = match body {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        "nbsp" => "\u{00a0}",
        "copy" => "\u{00a9}",
        "hellip" => "\u{2026}",
        "mdash" => "\u{2014}",
        "ndash" => "\u{2013}",
        "laquo" => "\u{00ab}",
        "raquo" => "\u{00bb}",
        "NotEqualTilde" => "\u{2242}\u{0338}",
        _ => return None,
    };
    Some(decoded.to_string())
}
