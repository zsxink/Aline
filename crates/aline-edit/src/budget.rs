//! 编辑计划、候选文本与重读解析的注入预算。

use aline_buffer::{PatchBudget, TextBudget};
use aline_markdown::ParseBudget;

/// 一次计划及其预演的预算集合；数值由调用方注入，不声明发布阈值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditBudget {
    patch: PatchBudget,
    text: TextBudget,
    parse: ParseBudget,
}

impl EditBudget {
    /// 注入补丁批次、源码/候选文本与重读解析预算。
    pub const fn new(patch: PatchBudget, text: TextBudget, parse: ParseBudget) -> Self {
        Self { patch, text, parse }
    }

    /// 返回补丁批次预算。
    pub const fn patch(self) -> PatchBudget {
        self.patch
    }

    /// 返回源码切片与候选文本预算。
    pub const fn text(self) -> TextBudget {
        self.text
    }

    /// 返回候选解重新解析预算。
    pub const fn parse(self) -> ParseBudget {
        self.parse
    }
}
