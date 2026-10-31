//! 保护范围；保护独立于展示，编辑层据此拒绝改写对应源码。

use aline_buffer::SrcRange;

/// 受保护源码的种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProtectionKind {
    /// 文首 front matter。
    FrontMatter,
    /// 脚注引用子范围。
    FootnoteReference,
    /// 脚注定义。
    FootnoteDefinition,
    /// `[toc]` 目录标记。
    Toc,
    /// 有限 HTML 源码。
    Html,
    /// 解析回退时的整段原文。
    RawFallback,
}

impl ProtectionKind {
    /// 返回稳定 dump 名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FrontMatter => "FrontMatter",
            Self::FootnoteReference => "FootnoteReference",
            Self::FootnoteDefinition => "FootnoteDefinition",
            Self::Toc => "Toc",
            Self::Html => "Html",
            Self::RawFallback => "RawFallback",
        }
    }
}

/// 一段只读保护范围。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedRange {
    range: SrcRange,
    kind: ProtectionKind,
}

impl ProtectedRange {
    /// 构造保护范围；范围来自解析快照。
    pub fn new(range: SrcRange, kind: ProtectionKind) -> Self {
        Self { range, kind }
    }

    /// 返回保护范围。
    pub fn range(&self) -> &SrcRange {
        &self.range
    }

    /// 返回保护种类。
    pub const fn kind(&self) -> ProtectionKind {
        self.kind
    }
}
