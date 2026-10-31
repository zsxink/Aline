//! 拒绝原因；每个拒绝都发生在写入之前，调用方状态不被修改。

use std::fmt;
use std::ops::Range;

use aline_buffer::DocumentRevision;
use aline_markdown::{ParseError, ProtectionKind};

use crate::command::EditMode;

/// 预算拒绝的类别；数值缺失表示底层解析器未暴露计数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetKind {
    /// 补丁数量超限。
    PatchCount,
    /// 批次插入或结果字节总量超限。
    BatchBytes,
    /// 源码读取或期望片段物化字节超限。
    SourceBytes,
    /// 候选文本构造字节超限。
    CandidateBytes,
    /// 候选文本重新解析超限。
    Parse,
    /// 映射索引预算超限。
    Mapping,
}

impl BudgetKind {
    /// 返回稳定名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PatchCount => "PatchCount",
            Self::BatchBytes => "BatchBytes",
            Self::SourceBytes => "SourceBytes",
            Self::CandidateBytes => "CandidateBytes",
            Self::Parse => "Parse",
            Self::Mapping => "Mapping",
        }
    }
}

/// 候选文本复读失败的具体语义类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RereadFailure {
    /// 候选文本无法解析。
    Parse,
    /// 可见绘制文本与命令预期不符。
    VisibleTextChanged,
    /// 隐藏源码语义（链接目标、强调标记等）发生变化。
    HiddenSourceChanged,
    /// 格式命令改变了块结构。
    BlockStructureChanged,
    /// 命令给出的目标选区在候选文本上非法。
    InvalidAfterSelection,
    /// 候选文本实际长度与受检批次声明不一致。
    CandidateLengthMismatch,
    /// 源码端点无法映射到当前投影。
    Projection,
}

impl RereadFailure {
    /// 返回稳定名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Parse => "Parse",
            Self::VisibleTextChanged => "VisibleTextChanged",
            Self::HiddenSourceChanged => "HiddenSourceChanged",
            Self::BlockStructureChanged => "BlockStructureChanged",
            Self::InvalidAfterSelection => "InvalidAfterSelection",
            Self::CandidateLengthMismatch => "CandidateLengthMismatch",
            Self::Projection => "Projection",
        }
    }
}

/// 计划或校验失败的原因；失败时真实快照、选区、历史和组合状态均未改变。
#[derive(Debug)]
pub enum EditRefusal {
    /// 仍有未结束的输入组，命令必须等基线稳定。
    PendingInput,
    /// 输入法组合仍在进行，命令必须等待组合提交。
    CompositionActive,
    /// 命令与上下文模式不匹配。
    WrongMode {
        /// 命令要求的模式。
        expected: EditMode,
        /// 实际上下文模式。
        actual: EditMode,
    },
    /// 计划绑定的工作版本已不是当前版本。
    WrongVersion {
        /// 计划生成时的已提交代次。
        expected: DocumentRevision,
        /// 当前已提交代次。
        actual: DocumentRevision,
    },
    /// 代次相同但快照身份不同；旧计划不得复活。
    WrongSnapshot,
    /// 解析结果或映射不属于当前快照/投影。
    ProjectionMismatch,
    /// 计划生成后选区已变化。
    SelectionMismatch,
    /// 范围内原文与调用方期望片段不符。
    ExpectedFragmentMismatch {
        /// 复验失败的源码范围。
        range: Range<usize>,
    },
    /// 源码端点超过快照长度。
    OutOfBounds {
        /// 非法端点。
        offset: usize,
        /// 快照字节长度。
        len: usize,
    },
    /// 半开范围起点大于终点。
    ReversedRange,
    /// 端点位于 UTF-8 标量内部。
    NotCharBoundary {
        /// 非法源码字节端点。
        offset: usize,
    },
    /// 注入预算不足。
    BudgetExceeded {
        /// 超限类别。
        kind: BudgetKind,
        /// 本次需要的数量；解析器未提供时为 `None`。
        needed: Option<usize>,
        /// 注入上限；解析器未提供时为 `None`。
        limit: Option<usize>,
    },
    /// 所见命令触及受保护区间。
    ProtectedRange {
        /// 保护种类。
        kind: ProtectionKind,
        /// 被触及的保护范围。
        range: Range<usize>,
    },
    /// 所见命令触及 HTML 标签或样式区间。
    HtmlBoundary {
        /// 被触及的 HTML 范围。
        range: Range<usize>,
    },
    /// 行内格式要求非空选区。
    EmptySelection,
    /// 当前最小协议只允许单条所见替换。
    UnsupportedVisualBatch,
    /// 内部计划不满足构造不变量。
    InvalidPlan,
    /// 结果长度累计溢出。
    LengthOverflow,
    /// 受检内存预留失败。
    Allocation,
    /// 候选文本重新解析或语义比较失败。
    RereadFailed {
        /// 复读失败类别。
        reason: RereadFailure,
        /// 解析失败时的底层错误。
        cause: Option<ParseError>,
    },
}

impl EditRefusal {
    /// 返回稳定类别名，便于测试、日志与证据对照。
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::PendingInput => "PendingInput",
            Self::CompositionActive => "CompositionActive",
            Self::WrongMode { .. } => "WrongMode",
            Self::WrongVersion { .. } => "WrongVersion",
            Self::WrongSnapshot => "WrongSnapshot",
            Self::ProjectionMismatch => "ProjectionMismatch",
            Self::SelectionMismatch => "SelectionMismatch",
            Self::ExpectedFragmentMismatch { .. } => "ExpectedFragmentMismatch",
            Self::OutOfBounds { .. } => "OutOfBounds",
            Self::ReversedRange => "ReversedRange",
            Self::NotCharBoundary { .. } => "NotCharBoundary",
            Self::BudgetExceeded { .. } => "BudgetExceeded",
            Self::ProtectedRange { .. } => "ProtectedRange",
            Self::HtmlBoundary { .. } => "HtmlBoundary",
            Self::EmptySelection => "EmptySelection",
            Self::UnsupportedVisualBatch => "UnsupportedVisualBatch",
            Self::InvalidPlan => "InvalidPlan",
            Self::LengthOverflow => "LengthOverflow",
            Self::Allocation => "Allocation",
            Self::RereadFailed { .. } => "RereadFailed",
        }
    }
}

impl fmt::Display for EditRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for EditRefusal {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::RereadFailed {
                cause: Some(error), ..
            } => Some(error),
            _ => None,
        }
    }
}
