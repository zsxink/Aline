//! 不可变源码视图及单草稿覆盖；宿主拥有可写状态，本模块不解析、不写盘。
//!
//! Whole 共享完整文本，Overlay 仅共享一个完整基线与一个块草稿。
//! 读取不生成新身份；只有显式物化才复制连续文本，且受字节预算约束。

use std::borrow::Cow;
use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::{DocumentRevision, SnapshotId, SrcOffset, SrcRange, SrcSelection};

/// 源码范围、身份、容量或内部表示校验失败。
#[derive(Debug)]
pub enum BufferError {
    /// 范围端点超过文本字节长度。
    OutOfBounds {
        /// 非法端点。
        offset: usize,
        /// 文本字节长度。
        len: usize,
    },
    /// 半开范围的起点大于终点。
    ReversedRange,
    /// 端点位于 UTF-8 标量内部。
    NotCharBoundary {
        /// 非法源码字节端点。
        offset: usize,
    },
    /// 坐标或范围属于另一源码快照。
    SnapshotMismatch,
    /// 字节长度计算溢出或超过 String 可表示上限。
    LengthOverflow,
    /// 请求的字节量超过注入预算。
    BudgetExceeded {
        /// 本次需要的字节数。
        needed: usize,
        /// 注入的上限。
        limit: usize,
    },
    /// 只允许对完整基线创建一层覆盖。
    NestedOverlay,
    /// 不可变内部片段不再满足构造不变量。
    InvalidBacking,
    /// 文档已提交代次耗尽。
    RevisionExhausted,
    /// 计划绑定的已提交代次不再是当前代次。
    RevisionMismatch,
    /// tuple 批次不是升序排列。
    UnsortedPatches,
    /// 同基线范围相互覆盖。
    OverlappingPatches,
    /// 同点多个插入尚未合并成确定计划。
    AmbiguousInsertion,
    /// tuple 批次数量超出注入预算。
    PatchCountExceeded {
        /// 本次批次数量。
        count: usize,
        /// 注入的数量上限。
        limit: usize,
    },
    /// 连续文本的受检预留失败。
    Allocation(std::collections::TryReserveError),
}

impl fmt::Display for BufferError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for BufferError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Allocation(error) => Some(error),
            _ => None,
        }
    }
}

/// 一次文本构造、切片或物化的字节上限，不是发布性能限额。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TextBudget {
    max_bytes: usize,
}

impl TextBudget {
    /// 创建由宿主注入的字节预算。
    pub const fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }

    /// 返回注入的字节上限。
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    pub(crate) fn check(self, needed: usize) -> Result<(), BufferError> {
        if needed > isize::MAX.unsigned_abs() {
            return Err(BufferError::LengthOverflow);
        }
        if needed > self.max_bytes {
            return Err(BufferError::BudgetExceeded {
                needed,
                limit: self.max_bytes,
            });
        }
        Ok(())
    }
}

#[derive(Clone)]
enum Backing {
    Whole(Arc<String>),
    Overlay {
        base: Arc<String>,
        replaced: Range<usize>,
        draft: Arc<String>,
    },
}

/// 带独立身份的不可变源码快照，供 markdown/edit/UI 只读消费。
#[derive(Clone)]
pub struct TextSnapshot {
    id: SnapshotId,
    revision: DocumentRevision,
    len: usize,
    backing: Backing,
}

impl fmt::Debug for TextSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TextSnapshot")
            .field("id", &self.id)
            .field("revision", &self.revision)
            .field("len", &self.len)
            .finish_non_exhaustive()
    }
}

impl TextSnapshot {
    /// 接管完整 UTF-8 文本；超预算返回错误，不规范化原始 CR/LF。
    pub fn new(text: String, budget: TextBudget) -> Result<Self, BufferError> {
        Self::from_shared(Arc::new(text), DocumentRevision::initial(), budget)
    }

    /// 共享完整只读文本并创建新身份；指定代次不构成与其他快照相等的证据。
    pub fn from_shared(
        text: Arc<String>,
        revision: DocumentRevision,
        budget: TextBudget,
    ) -> Result<Self, BufferError> {
        budget.check(text.len())?;
        Ok(Self {
            id: SnapshotId::fresh(),
            revision,
            len: text.len(),
            backing: Backing::Whole(text),
        })
    }

    /// 在完整基线上建立唯一块草稿视图；身份/UTF-8/长度/预算均先检查，拒绝叠套。
    pub fn with_overlay(
        base: &Self,
        replaced: SrcRange,
        draft: Arc<String>,
        budget: TextBudget,
    ) -> Result<Self, BufferError> {
        let Backing::Whole(text) = &base.backing else {
            return Err(BufferError::NestedOverlay);
        };
        let range = base.validate_range(&replaced)?;
        let suffix = base
            .len
            .checked_sub(range.end)
            .ok_or(BufferError::LengthOverflow)?;
        let len = composed_len(range.start, draft.len(), suffix)?;
        budget.check(len)?;
        Ok(Self {
            id: SnapshotId::fresh(),
            revision: base.revision,
            len,
            backing: Backing::Overlay {
                base: text.clone(),
                replaced: range,
                draft,
            },
        })
    }

    /// 返回当前不可变输入身份，clone 不另建身份。
    pub fn id(&self) -> &SnapshotId {
        &self.id
    }

    /// 返回已提交基线代次；草稿代次由 UI 的 WorkingVersion 单独持有。
    pub const fn revision(&self) -> DocumentRevision {
        self.revision
    }

    /// 返回当前逻辑源码的 UTF-8 字节长度。
    pub const fn len_bytes(&self) -> usize {
        self.len
    }

    /// 判断当前逻辑源码是否为空。
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn parts(&self) -> Result<[&str; 3], BufferError> {
        match &self.backing {
            Backing::Whole(text) => Ok([text.as_str(), "", ""]),
            Backing::Overlay {
                base,
                replaced,
                draft,
            } => Ok([
                base.get(..replaced.start)
                    .ok_or(BufferError::InvalidBacking)?,
                draft.as_str(),
                base.get(replaced.end..)
                    .ok_or(BufferError::InvalidBacking)?,
            ]),
        }
    }

    /// 只读迭代至多三个片段；不会物化全文，也不改变身份。
    pub fn iter_chunks(&self) -> Result<std::array::IntoIter<&str, 3>, BufferError> {
        Ok(self.parts()?.into_iter())
    }

    /// 检查当前逻辑文本的 UTF-8 边界，未知内部表示受检失败。
    pub fn is_char_boundary(&self, src_off: usize) -> Result<bool, BufferError> {
        if src_off > self.len {
            return Ok(false);
        }
        let mut start = 0usize;
        for part in self.iter_chunks()? {
            let end = start
                .checked_add(part.len())
                .ok_or(BufferError::LengthOverflow)?;
            if src_off <= end {
                return Ok(part.is_char_boundary(src_off - start));
            }
            start = end;
        }
        Ok(src_off == self.len)
    }

    /// 创建绑定本快照的源码偏移；越界或半字符端点明确拒绝。
    pub fn src_offset(&self, src_off: usize) -> Result<SrcOffset, BufferError> {
        if src_off > self.len {
            return Err(BufferError::OutOfBounds {
                offset: src_off,
                len: self.len,
            });
        }
        if !self.is_char_boundary(src_off)? {
            return Err(BufferError::NotCharBoundary { offset: src_off });
        }
        Ok(SrcOffset {
            snapshot: self.id.clone(),
            byte: src_off,
        })
    }

    /// 创建半开源码范围，合法空范围和文件末端允许。
    pub fn src_range(&self, bytes: Range<usize>) -> Result<SrcRange, BufferError> {
        if bytes.start > bytes.end {
            return Err(BufferError::ReversedRange);
        }
        self.src_offset(bytes.start)?;
        self.src_offset(bytes.end)?;
        Ok(SrcRange {
            snapshot: self.id.clone(),
            bytes,
        })
    }

    /// 复验偏移所属快照，再返回表示边界所需源码字节数值。
    pub fn validate_offset(&self, offset: &SrcOffset) -> Result<usize, BufferError> {
        if offset.snapshot != self.id {
            return Err(BufferError::SnapshotMismatch);
        }
        self.src_offset(offset.byte)?;
        Ok(offset.byte)
    }

    /// 复验范围所属快照及两端，返回 tuple 补丁表示边界的范围。
    pub fn validate_range(&self, range: &SrcRange) -> Result<Range<usize>, BufferError> {
        if range.snapshot != self.id {
            return Err(BufferError::SnapshotMismatch);
        }
        self.src_range(range.bytes.clone())?;
        Ok(range.bytes.clone())
    }

    /// 复验方向性源码选区，不把旧选区贴到新身份。
    pub fn validate_selection(&self, selection: &SrcSelection) -> Result<(), BufferError> {
        self.validate_offset(selection.anchor())?;
        self.validate_offset(selection.head())?;
        Ok(())
    }

    /// 片段内借用，跨片段只复制请求范围；容量和字节预算先验。
    pub fn slice(&self, range: &SrcRange, budget: TextBudget) -> Result<Cow<'_, str>, BufferError> {
        let range = self.validate_range(range)?;
        budget.check(range.len())?;
        let mut start = 0usize;
        for part in self.iter_chunks()? {
            let end = start
                .checked_add(part.len())
                .ok_or(BufferError::LengthOverflow)?;
            if range.start >= start && range.end <= end {
                return part
                    .get(range.start - start..range.end - start)
                    .map(Cow::Borrowed)
                    .ok_or(BufferError::InvalidBacking);
            }
            start = end;
        }
        let mut output = String::new();
        output
            .try_reserve_exact(range.len())
            .map_err(BufferError::Allocation)?;
        start = 0;
        for part in self.iter_chunks()? {
            let end = start
                .checked_add(part.len())
                .ok_or(BufferError::LengthOverflow)?;
            let left = range.start.max(start);
            let right = range.end.min(end);
            if left < right {
                output.push_str(
                    part.get(left - start..right - start)
                        .ok_or(BufferError::InvalidBacking)?,
                );
            }
            start = end;
        }
        Ok(Cow::Owned(output))
    }

    /// 在显式消费点物化连续文本，保留输入身份与版本；不用于每键路径。
    pub fn materialize(&self, budget: TextBudget) -> Result<Self, BufferError> {
        budget.check(self.len)?;
        if matches!(self.backing, Backing::Whole(_)) {
            return Ok(self.clone());
        }
        let mut text = String::new();
        text.try_reserve_exact(self.len)
            .map_err(BufferError::Allocation)?;
        for part in self.iter_chunks()? {
            text.push_str(part);
        }
        Ok(Self {
            id: self.id.clone(),
            revision: self.revision,
            len: self.len,
            backing: Backing::Whole(Arc::new(text)),
        })
    }
}

fn composed_len(prefix: usize, draft: usize, suffix: usize) -> Result<usize, BufferError> {
    prefix
        .checked_add(draft)
        .and_then(|value| value.checked_add(suffix))
        .ok_or(BufferError::LengthOverflow)
}

#[cfg(test)]
mod tests {
    use super::{BufferError, composed_len};

    #[test]
    fn composed_length_overflow_is_checked_before_allocation() {
        assert!(matches!(
            composed_len(usize::MAX, 1, 0),
            Err(BufferError::LengthOverflow)
        ));
    }
}
