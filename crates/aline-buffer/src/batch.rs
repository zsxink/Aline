//! 原生 tuple 补丁的受检表示边界；只验证并持有计划，不修改文本或历史。

use std::ops::Range;

use crate::{BufferError, DocumentRevision, SnapshotId, TextBudget, TextSnapshot};

/// 一次批次的数量、替换输入字节和结果字节预算。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PatchBudget {
    max_patches: usize,
    max_inserted_bytes: usize,
    max_output_bytes: usize,
}

impl PatchBudget {
    /// 创建调用方注入的上限，不声明发布资源阈值。
    pub const fn new(
        max_patches: usize,
        max_inserted_bytes: usize,
        max_output_bytes: usize,
    ) -> Self {
        Self {
            max_patches,
            max_inserted_bytes,
            max_output_bytes,
        }
    }
}

/// 同一 before 源码上的受检批次；底层保持 Vec<(Range<usize>, String)>。
#[derive(Clone, Debug)]
pub struct CheckedBatch {
    before_id: SnapshotId,
    before_revision: DocumentRevision,
    patches: Vec<(Range<usize>, String)>,
    output_len: usize,
}

impl CheckedBatch {
    /// 验证全部 tuple、排序/重叠/同点歧义和长度预算后取得所有权；不发布任何修改。
    pub fn new(
        before: &TextSnapshot,
        patches: Vec<(Range<usize>, String)>,
        budget: PatchBudget,
    ) -> Result<Self, BufferError> {
        if patches.len() > budget.max_patches {
            return Err(BufferError::PatchCountExceeded {
                count: patches.len(),
                limit: budget.max_patches,
            });
        }
        let mut removed = 0usize;
        let mut inserted = 0usize;
        let mut previous: Option<&Range<usize>> = None;
        for (range, replacement) in &patches {
            before.src_range(range.clone())?;
            if let Some(prior) = previous {
                if range.start < prior.start {
                    return Err(BufferError::UnsortedPatches);
                }
                if range.start < prior.end {
                    return Err(BufferError::OverlappingPatches);
                }
                if prior.is_empty() && range.is_empty() && prior.start == range.start {
                    return Err(BufferError::AmbiguousInsertion);
                }
            }
            removed = removed
                .checked_add(range.len())
                .ok_or(BufferError::LengthOverflow)?;
            inserted = inserted
                .checked_add(replacement.len())
                .ok_or(BufferError::LengthOverflow)?;
            TextBudget::new(budget.max_inserted_bytes).check(inserted)?;
            previous = Some(range);
        }
        let output_len = checked_output_len(before.len_bytes(), removed, inserted)?;
        TextBudget::new(budget.max_output_bytes).check(output_len)?;
        Ok(Self {
            before_id: before.id().clone(),
            before_revision: before.revision(),
            patches,
            output_len,
        })
    }

    /// 复验实际消费快照；相同文本不能使旧计划复活。
    pub fn validate(&self, current: &TextSnapshot) -> Result<(), BufferError> {
        if self.before_revision != current.revision() {
            return Err(BufferError::RevisionMismatch);
        }
        if self.before_id != *current.id() {
            return Err(BufferError::SnapshotMismatch);
        }
        Ok(())
    }

    /// 返回原生 tuple 的只读切片，不暴露可变补丁容器。
    pub fn patches(&self) -> &[(Range<usize>, String)] {
        &self.patches
    }

    /// 返回完整预检得到的结果字节长度，尚未生成或发布该文本。
    pub const fn output_len(&self) -> usize {
        self.output_len
    }

    /// 返回 before 快照身份。
    pub fn before_id(&self) -> &SnapshotId {
        &self.before_id
    }

    /// 返回 before 的已提交代次。
    pub const fn before_revision(&self) -> DocumentRevision {
        self.before_revision
    }
}

fn checked_output_len(
    before: usize,
    removed: usize,
    inserted: usize,
) -> Result<usize, BufferError> {
    before
        .checked_sub(removed)
        .and_then(|value| value.checked_add(inserted))
        .ok_or(BufferError::LengthOverflow)
}

#[cfg(test)]
mod tests {
    use super::{BufferError, checked_output_len};

    #[test]
    fn output_length_arithmetic_refuses_overflow_and_underflow() {
        assert!(matches!(
            checked_output_len(usize::MAX, 0, 1),
            Err(BufferError::LengthOverflow)
        ));
        assert!(matches!(
            checked_output_len(0, 1, 0),
            Err(BufferError::LengthOverflow)
        ));
    }
}
