//! 有限 HTML 产生的展示样式区间；样式只影响展示，不改变源码或保护语义。

use aline_buffer::SrcRange;

/// 一段可见 HTML 的展示样式集合。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TextStyle {
    bold: bool,
    italic: bool,
    strike: bool,
    underline: bool,
    mark: bool,
    code: bool,
    link: bool,
}

impl TextStyle {
    /// 构造样式集合。
    pub const fn new(
        bold: bool,
        italic: bool,
        strike: bool,
        underline: bool,
        mark: bool,
        code: bool,
        link: bool,
    ) -> Self {
        Self {
            bold,
            italic,
            strike,
            underline,
            mark,
            code,
            link,
        }
    }
    /// 是否粗体。
    pub const fn bold(self) -> bool {
        self.bold
    }
    /// 是否斜体。
    pub const fn italic(self) -> bool {
        self.italic
    }
    /// 是否删除线。
    pub const fn strike(self) -> bool {
        self.strike
    }
    /// 是否下划线。
    pub const fn underline(self) -> bool {
        self.underline
    }
    /// 是否高亮。
    pub const fn mark(self) -> bool {
        self.mark
    }
    /// 是否等宽。
    pub const fn code(self) -> bool {
        self.code
    }
    /// 是否链接文本。
    pub const fn link(self) -> bool {
        self.link
    }
}

/// 绑定源码范围的展示样式；范围来自解析快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StyledRange {
    range: SrcRange,
    style: TextStyle,
}

impl StyledRange {
    /// 构造样式区间。
    pub fn new(range: SrcRange, style: TextStyle) -> Self {
        Self { range, style }
    }
    /// 返回源码范围。
    pub fn range(&self) -> &SrcRange {
        &self.range
    }
    /// 返回样式。
    pub const fn style(&self) -> TextStyle {
        self.style
    }
}
