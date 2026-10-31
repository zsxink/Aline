//! 解析诊断；诊断只描述已观察到的回退或限额，不改变源码。

use aline_buffer::SrcRange;

/// 解析诊断种类；稳定名称用于无头 dump。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    /// 重写或构树失败后整段回到原文视图。
    RawFallback,
    /// 解析节点或扫描步数超出注入预算。
    BudgetExceeded,
}

impl DiagnosticKind {
    /// 返回稳定 dump 名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::RawFallback => "RawFallback",
            Self::BudgetExceeded => "BudgetExceeded",
        }
    }
}

/// 一条绑定源码范围的诊断。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    range: SrcRange,
    kind: DiagnosticKind,
}

impl Diagnostic {
    /// 构造诊断；范围来自解析快照。
    pub fn new(range: SrcRange, kind: DiagnosticKind) -> Self {
        Self { range, kind }
    }

    /// 返回诊断范围。
    pub fn range(&self) -> &SrcRange {
        &self.range
    }

    /// 返回诊断种类。
    pub const fn kind(&self) -> DiagnosticKind {
        self.kind
    }
}
