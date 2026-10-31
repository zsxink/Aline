//! 底层错误到拒绝类别的受检映射；不把错误静默降级为成功。

use aline_buffer::BufferError;
use aline_markdown::{MappingError, ParseError};

use crate::refusal::{BudgetKind, EditRefusal, RereadFailure};

/// 源码读取、选区或范围校验错误的默认映射。
pub(crate) fn map_buffer_error(error: BufferError) -> EditRefusal {
    match error {
        BufferError::OutOfBounds { offset, len } => EditRefusal::OutOfBounds { offset, len },
        BufferError::ReversedRange => EditRefusal::ReversedRange,
        BufferError::NotCharBoundary { offset } => EditRefusal::NotCharBoundary { offset },
        BufferError::SnapshotMismatch => EditRefusal::WrongSnapshot,
        BufferError::LengthOverflow => EditRefusal::LengthOverflow,
        BufferError::BudgetExceeded { needed, limit } => EditRefusal::BudgetExceeded {
            kind: BudgetKind::SourceBytes,
            needed: Some(needed),
            limit: Some(limit),
        },
        BufferError::PatchCountExceeded { count, limit } => EditRefusal::BudgetExceeded {
            kind: BudgetKind::PatchCount,
            needed: Some(count),
            limit: Some(limit),
        },
        BufferError::Allocation(_) => EditRefusal::Allocation,
        _ => EditRefusal::InvalidPlan,
    }
}

/// 受检补丁批次错误的映射；插入/结果字节超限归为批次预算。
pub(crate) fn map_batch_error(error: BufferError) -> EditRefusal {
    match error {
        BufferError::BudgetExceeded { needed, limit } => EditRefusal::BudgetExceeded {
            kind: BudgetKind::BatchBytes,
            needed: Some(needed),
            limit: Some(limit),
        },
        other => map_buffer_error(other),
    }
}

/// 候选快照构造错误的映射；文本预算归为候选字节预算。
pub(crate) fn map_candidate_error(error: BufferError) -> EditRefusal {
    match error {
        BufferError::BudgetExceeded { needed, limit } => EditRefusal::BudgetExceeded {
            kind: BudgetKind::CandidateBytes,
            needed: Some(needed),
            limit: Some(limit),
        },
        other => map_buffer_error(other),
    }
}

/// 映射错误的映射；错投影/局部投影外统一归为投影不匹配。
pub(crate) fn map_mapping_error(error: MappingError) -> EditRefusal {
    match error {
        MappingError::Source(error) => map_buffer_error(error),
        MappingError::OutOfBounds { offset, len } => EditRefusal::OutOfBounds { offset, len },
        MappingError::ReversedRange => EditRefusal::ReversedRange,
        MappingError::ProjectionMismatch
        | MappingError::SourceOutsideProjection
        | MappingError::NotCharBoundary
        | MappingError::MiddleSurrogate
        | MappingError::InvalidMapping => EditRefusal::ProjectionMismatch,
        MappingError::BudgetExceeded => EditRefusal::BudgetExceeded {
            kind: BudgetKind::Mapping,
            needed: None,
            limit: None,
        },
        MappingError::LengthOverflow => EditRefusal::LengthOverflow,
        MappingError::Allocation(_) => EditRefusal::Allocation,
    }
}

/// 候选解析错误的映射；预算与其余解析失败保持可区分。
pub(crate) fn map_parse_error(error: ParseError) -> EditRefusal {
    match error {
        ParseError::BudgetExceeded => EditRefusal::BudgetExceeded {
            kind: BudgetKind::Parse,
            needed: None,
            limit: None,
        },
        ParseError::Source(BufferError::BudgetExceeded { needed, limit }) => {
            EditRefusal::BudgetExceeded {
                kind: BudgetKind::Parse,
                needed: Some(needed),
                limit: Some(limit),
            }
        }
        ParseError::Source(error) => map_buffer_error(error),
        ParseError::Mapping(error) => map_mapping_error(error),
        other => EditRefusal::RereadFailed {
            reason: RereadFailure::Parse,
            cause: Some(other),
        },
    }
}
