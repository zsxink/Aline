//! 无头块模型；块只暴露源码范围与种类，不携带可写 UI 状态。

use aline_buffer::SrcRange;

/// 顶层块种类；`Math` 是 M0 dump 对顶层公式段落的稳定分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BlockKind {
    /// 文首成对 front matter。
    FrontMatter,
    /// ATX/Setext 标题。
    Heading,
    /// 段落。
    Paragraph,
    /// 引用块。
    BlockQuote,
    /// 列表。
    List,
    /// 代码块（围栏或缩进）。
    CodeBlock,
    /// 表格。
    Table,
    /// 脚注定义。
    FootnoteDefinition,
    /// 有限 HTML 块。
    HtmlBlock,
    /// 顶层公式块。
    Math,
    /// 主题分隔线。
    ThematicBreak,
    /// 解析回退时的整段原文。
    Raw,
}

impl BlockKind {
    /// 返回稳定 dump 名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FrontMatter => "FrontMatter",
            Self::Heading => "Heading",
            Self::Paragraph => "Paragraph",
            Self::BlockQuote => "BlockQuote",
            Self::List => "List",
            Self::CodeBlock => "CodeBlock",
            Self::Table => "Table",
            Self::FootnoteDefinition => "FootnoteDefinition",
            Self::HtmlBlock => "HtmlBlock",
            Self::Math => "Math",
            Self::ThematicBreak => "ThematicBreak",
            Self::Raw => "Raw",
        }
    }
}

/// 顶层块；范围已在所属快照校验，块尾不含分隔换行。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Block {
    range: SrcRange,
    kind: BlockKind,
}

impl Block {
    /// 构造块；调用方保证范围来自同一快照。
    pub fn new(range: SrcRange, kind: BlockKind) -> Self {
        Self { range, kind }
    }

    /// 返回块源码范围。
    pub fn range(&self) -> &SrcRange {
        &self.range
    }

    /// 返回块种类。
    pub const fn kind(&self) -> BlockKind {
        self.kind
    }
}

/// 两个块之间的显式源码间隙；不包含任何块覆盖的字节。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gap {
    range: SrcRange,
}

impl Gap {
    /// 构造间隙。
    pub fn new(range: SrcRange) -> Self {
        Self { range }
    }

    /// 返回间隙源码范围。
    pub fn range(&self) -> &SrcRange {
        &self.range
    }
}
