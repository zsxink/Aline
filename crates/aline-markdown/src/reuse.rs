//! 组 6.7：全量解析后的头尾配对复用过滤。
//!
//! 本模块不实现局部增量解析器；它在两次全量结果之间只承认可证明安全的头尾复用，
//! 其余情况显式回退全量。引用定义、脚注、目录、围栏、列表、HTML 与 front matter
//! 可能影响远处块，因此一律拒绝复用。

use std::borrow::Cow;
use std::ops::Range;

use aline_buffer::TextBudget;

use crate::mdast::{Block, BlockKind};
use crate::parse::Parsed;
use crate::protection::ProtectionKind;

/// 拒绝复用的原因；每一种都要求调用方走全量路径。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReuseRefusal {
    /// 两次解析配置代次或语法集合不同。
    ConfigChanged,
    /// 任一文档含 front matter。
    FrontMatter,
    /// 任一文档含脚注定义或引用。
    Footnote,
    /// 任一文档含 `[toc]` 标记。
    Toc,
    /// 任一文档含代码块或围栏行。
    Fence,
    /// 任一文档含列表，缩进可能改变远处块结构。
    List,
    /// 任一文档含链接引用定义。
    ReferenceDefinition,
    /// 任一文档含有限 HTML 源码。
    Html,
    /// 任一次解析已整体回退原文。
    RawFallback,
}

impl ReuseRefusal {
    /// 返回稳定名称，供无头记录和测试比较。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConfigChanged => "ConfigChanged",
            Self::FrontMatter => "FrontMatter",
            Self::Footnote => "Footnote",
            Self::Toc => "Toc",
            Self::Fence => "Fence",
            Self::List => "List",
            Self::ReferenceDefinition => "ReferenceDefinition",
            Self::Html => "Html",
            Self::RawFallback => "RawFallback",
        }
    }
}

/// 一个被承认复用的旧块到新块配对。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusedBlock {
    old_index: usize,
    new_index: usize,
    old_range: Range<usize>,
    new_range: Range<usize>,
    kind: BlockKind,
}

impl ReusedBlock {
    /// 返回旧结果中的块序号。
    pub const fn old_index(&self) -> usize {
        self.old_index
    }
    /// 返回新结果中的块序号。
    pub const fn new_index(&self) -> usize {
        self.new_index
    }
    /// 返回旧快照中的块范围。
    pub fn old_range(&self) -> &Range<usize> {
        &self.old_range
    }
    /// 返回新快照中的块范围。
    pub fn new_range(&self) -> &Range<usize> {
        &self.new_range
    }
    /// 返回块种类。
    pub const fn kind(&self) -> BlockKind {
        self.kind
    }
    /// 把旧块内偏移重定位到新快照；块外偏移返回 None，不做 clamp。
    pub fn translate(&self, old_offset: usize) -> Option<usize> {
        if old_offset < self.old_range.start || old_offset > self.old_range.end {
            return None;
        }
        let delta = old_offset.checked_sub(self.old_range.start)?;
        self.new_range.start.checked_add(delta)
    }
}

/// 复用过滤结果；没有命中且没有拒绝表示两次结果没有可配对块。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ReuseOutcome {
    reused: Vec<ReusedBlock>,
    refusal: Option<ReuseRefusal>,
}

impl ReuseOutcome {
    /// 返回承认复用的头尾块配对，按旧块序号升序。
    pub fn reused(&self) -> &[ReusedBlock] {
        &self.reused
    }
    /// 返回拒绝原因；None 表示已逐块检查。
    pub const fn refusal(&self) -> Option<ReuseRefusal> {
        self.refusal
    }
    /// 判断本次是否没有承认任何复用。
    pub fn is_empty(&self) -> bool {
        self.reused.is_empty()
    }
}

/// 比较旧/新全量结果并只承认安全头尾复用。
pub fn reuse(old: &Parsed, new: &Parsed) -> ReuseOutcome {
    if old.config() != new.config() {
        return refused(ReuseRefusal::ConfigChanged);
    }
    if let Some(reason) = global_dependency(old).or_else(|| global_dependency(new)) {
        return refused(reason);
    }
    let old_blocks = old.blocks();
    let new_blocks = new.blocks();
    let mut reused = Vec::new();
    let mut prefix = 0usize;
    while let (Some(old_block), Some(new_block)) = (old_blocks.get(prefix), new_blocks.get(prefix))
    {
        if !same_block(old, old_block, new, new_block) {
            break;
        }
        reused.push(pair(old_block, new_block, prefix, prefix));
        prefix = prefix.saturating_add(1);
    }
    let mut suffix = 0usize;
    loop {
        if prefix.saturating_add(suffix) >= old_blocks.len()
            || prefix.saturating_add(suffix) >= new_blocks.len()
        {
            break;
        }
        let old_index = old_blocks.len().saturating_sub(1).saturating_sub(suffix);
        let new_index = new_blocks.len().saturating_sub(1).saturating_sub(suffix);
        let (Some(old_block), Some(new_block)) =
            (old_blocks.get(old_index), new_blocks.get(new_index))
        else {
            break;
        };
        if !same_block(old, old_block, new, new_block) {
            break;
        }
        reused.push(pair(old_block, new_block, old_index, new_index));
        suffix = suffix.saturating_add(1);
    }
    reused.sort_by_key(|pair| pair.old_index);
    ReuseOutcome {
        reused,
        refusal: None,
    }
}

fn refused(reason: ReuseRefusal) -> ReuseOutcome {
    ReuseOutcome {
        reused: Vec::new(),
        refusal: Some(reason),
    }
}

fn pair(old_block: &Block, new_block: &Block, old_index: usize, new_index: usize) -> ReusedBlock {
    ReusedBlock {
        old_index,
        new_index,
        old_range: old_block.range().byte_range(),
        new_range: new_block.range().byte_range(),
        kind: old_block.kind(),
    }
}

fn same_block(old: &Parsed, old_block: &Block, new: &Parsed, new_block: &Block) -> bool {
    if old_block.kind() != new_block.kind() {
        return false;
    }
    match (block_text(old, old_block), block_text(new, new_block)) {
        (Some(old_text), Some(new_text)) => old_text == new_text,
        _ => false,
    }
}

fn block_text<'a>(parsed: &'a Parsed, block: &Block) -> Option<Cow<'a, str>> {
    let budget = TextBudget::new(parsed.snapshot().len_bytes());
    parsed.snapshot().slice(block.range(), budget).ok()
}

fn global_dependency(parsed: &Parsed) -> Option<ReuseRefusal> {
    if parsed.front_matter().is_some() {
        return Some(ReuseRefusal::FrontMatter);
    }
    for block in parsed.blocks() {
        match block.kind() {
            BlockKind::List => return Some(ReuseRefusal::List),
            BlockKind::CodeBlock => return Some(ReuseRefusal::Fence),
            BlockKind::FootnoteDefinition => return Some(ReuseRefusal::Footnote),
            BlockKind::HtmlBlock => return Some(ReuseRefusal::Html),
            BlockKind::Raw => return Some(ReuseRefusal::RawFallback),
            _ => {}
        }
    }
    for protected in parsed.protected_ranges() {
        let refusal = match protected.kind() {
            ProtectionKind::FrontMatter => Some(ReuseRefusal::FrontMatter),
            ProtectionKind::FootnoteReference | ProtectionKind::FootnoteDefinition => {
                Some(ReuseRefusal::Footnote)
            }
            ProtectionKind::Toc => Some(ReuseRefusal::Toc),
            ProtectionKind::Html => Some(ReuseRefusal::Html),
            ProtectionKind::RawFallback => Some(ReuseRefusal::RawFallback),
        };
        if let Some(refusal) = refusal {
            return Some(refusal);
        }
    }
    let range = parsed
        .snapshot()
        .src_range(0..parsed.snapshot().len_bytes())
        .ok()?;
    let source = parsed
        .snapshot()
        .slice(&range, TextBudget::new(parsed.snapshot().len_bytes()))
        .ok()?;
    if has_reference_definition(&source) {
        return Some(ReuseRefusal::ReferenceDefinition);
    }
    if has_fence_line(&source) {
        return Some(ReuseRefusal::Fence);
    }
    None
}

/// 识别单行与跨行标签的引用定义。
///
/// CommonMark 允许链接标签跨行（`[foo\nbar]: /u`），但逐行判断会漏掉：
/// 首行不含 `]:`，续行不以 `[` 开头。这里在逐行剥离容器前缀后维护一个
/// “未闭合标签”状态，标签在后续行以 `]:` 关闭即视为定义并拒绝复用。
/// 只有标签跨行关闭且紧跟 `:` 才算，`[foo\nbar] text` 不触发拒绝；
/// 空行终止段落与标签，转义的 `\[`/`\]` 不参与配对。扫描 O(行数)、不分配。
fn has_reference_definition(source: &str) -> bool {
    let mut open_label = false;
    let mut label_spans_lines = false;
    for line in source.lines() {
        let trimmed = strip_container_prefixes(line);
        if trimmed.starts_with('[') && trimmed.contains("]:") {
            return true;
        }
        if trimmed.is_empty() {
            open_label = false;
            label_spans_lines = false;
            continue;
        }
        let bytes = trimmed.as_bytes();
        let mut index = 0usize;
        while let Some(&byte) = bytes.get(index) {
            match byte {
                b'\\' => index = index.saturating_add(1),
                b'[' => {
                    open_label = true;
                    label_spans_lines = false;
                }
                b']' if open_label => {
                    open_label = false;
                    if label_spans_lines && bytes.get(index.saturating_add(1)) == Some(&b':') {
                        return true;
                    }
                }
                _ => {}
            }
            index = index.saturating_add(1);
        }
        if open_label {
            label_spans_lines = true;
        }
    }
    false
}

fn has_fence_line(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = strip_container_prefixes(line);
        trimmed.starts_with("```") || trimmed.starts_with("~~~")
    })
}

/// 行首最多允许的三个空格缩进；四格起是缩进代码块，不是引用定义。
const MAX_INITIAL_INDENT: usize = 3;
/// 容器标记之后允许的填充：`>` 后可有一个可选空格加三格缩进，列表标记后可为 1–4 格。
const MAX_CONTAINER_PADDING: usize = 4;

/// 剥离一行外层可能嵌套的块引用与列表标记，返回剩余内容。
///
/// 引用定义和围栏在块引用/列表内仍是文档级依赖，因此检测不能只看行首。
/// 这里只做至多常量的前缀剥离，保持 O(行数)、不分配。
fn strip_container_prefixes(mut line: &str) -> &str {
    line = strip_padding(line, MAX_INITIAL_INDENT, false);
    loop {
        let before = line.len();
        if let Some(rest) = line.strip_prefix('>') {
            line = strip_padding(rest, MAX_CONTAINER_PADDING, true);
        }
        if let Some(rest) = strip_list_marker(line) {
            line = strip_padding(rest, MAX_CONTAINER_PADDING, true);
        }
        if line.len() == before {
            return line;
        }
    }
}

/// 剥离至多 `max` 个前导空格（可选制表符）；只做字节切片，不复制。
fn strip_padding(line: &str, max: usize, tabs: bool) -> &str {
    let mut index = 0usize;
    let mut stripped = 0usize;
    for byte in line.bytes() {
        if stripped >= max || !(byte == b' ' || (tabs && byte == b'\t')) {
            break;
        }
        index = index.saturating_add(1);
        stripped = stripped.saturating_add(1);
    }
    &line[index..]
}

/// 识别 `-`/`*`/`+` 或 1–9 位数字加 `.`/`)` 的列表标记；标记后须为空白或行尾。
fn strip_list_marker(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let marker_len = match bytes.first()? {
        b'-' | b'*' | b'+' => 1,
        b'0'..=b'9' => {
            let mut digits = 0usize;
            while digits < 9 && bytes.get(digits).is_some_and(|byte| byte.is_ascii_digit()) {
                digits = digits.saturating_add(1);
            }
            match bytes.get(digits) {
                Some(b'.' | b')') => digits.saturating_add(1),
                _ => return None,
            }
        }
        _ => return None,
    };
    let rest = line.get(marker_len..)?;
    match rest.as_bytes().first() {
        None | Some(b' ' | b'\t') => Some(rest),
        Some(_) => None,
    }
}
