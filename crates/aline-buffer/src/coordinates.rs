//! 快照内源码坐标与版本值；只承载受检身份，不进行存储或绘制转换。

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use crate::BufferError;

/// 变换或对齐边界时采用的方向偏好；实际转换由所属映射层解释。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Affinity {
    /// 偏向边界之前。
    Before,
    /// 偏向边界之后。
    After,
}

/// 只在当前进程存活期间有效的源码快照身份；clone 保留身份。
#[derive(Clone)]
pub struct SnapshotId(Arc<()>);

impl SnapshotId {
    pub(crate) fn fresh() -> Self {
        Self(Arc::new(()))
    }
}

impl PartialEq for SnapshotId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for SnapshotId {}

impl fmt::Debug for SnapshotId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SnapshotId(..)")
    }
}

/// 文档已提交内容的单调代次；数值相同不证明文档或快照相同。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DocumentRevision(u64);

impl DocumentRevision {
    /// 新文档的初始代次。
    pub const fn initial() -> Self {
        Self(0)
    }

    /// 构造只读代次值；它不能赋予调用方任何快照身份。
    pub const fn from_value(value: u64) -> Self {
        Self(value)
    }

    /// 返回代次数值，供宿主身份信封使用。
    pub const fn value(self) -> u64 {
        self.0
    }

    /// 受检推进代次；耗尽时拒绝，不回绕。
    pub fn next(self) -> Result<Self, BufferError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(BufferError::RevisionExhausted)
    }
}

/// 已在所属源码快照校验过的 UTF-8 字节偏移。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SrcOffset {
    pub(crate) snapshot: SnapshotId,
    pub(crate) byte: usize,
}

impl SrcOffset {
    /// 返回用于观察或受检表示边界的源码字节数值，不代替跨坐标映射。
    pub const fn byte_offset(&self) -> usize {
        self.byte
    }

    /// 返回此偏移所属快照的身份。
    pub fn snapshot_id(&self) -> &SnapshotId {
        &self.snapshot
    }
}

/// 两端 UTF-8 对齐、绑定单一源码快照的半开范围。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SrcRange {
    pub(crate) snapshot: SnapshotId,
    pub(crate) bytes: Range<usize>,
}

impl SrcRange {
    /// 返回底层源码字节范围；消费方仍须向所属快照复验身份。
    pub fn byte_range(&self) -> Range<usize> {
        self.bytes.clone()
    }

    /// 返回范围所属快照身份。
    pub fn snapshot_id(&self) -> &SnapshotId {
        &self.snapshot
    }

    /// 判断半开范围是否为空；空范围允许插入。
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// 返回已校验范围的字节长度。
    pub fn len_bytes(&self) -> usize {
        self.bytes.len()
    }
}

/// 同一快照内的方向性源码选区，anchor/head 顺序不被排序覆盖。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SrcSelection {
    anchor: SrcOffset,
    head: SrcOffset,
}

impl SrcSelection {
    /// 从受检端点构造选区；错快照返回 SnapshotMismatch。
    pub fn new(anchor: SrcOffset, head: SrcOffset) -> Result<Self, BufferError> {
        if anchor.snapshot != head.snapshot {
            return Err(BufferError::SnapshotMismatch);
        }
        Ok(Self { anchor, head })
    }

    /// 返回不随选区方向变化的锚点。
    pub fn anchor(&self) -> &SrcOffset {
        &self.anchor
    }

    /// 返回活动端点。
    pub fn head(&self) -> &SrcOffset {
        &self.head
    }

    /// 判断活动端点是否位于锚点之前。
    pub fn is_reversed(&self) -> bool {
        self.anchor.byte > self.head.byte
    }

    /// 返回排序后的覆盖范围，不修改原始方向。
    pub fn range(&self) -> SrcRange {
        SrcRange {
            snapshot: self.anchor.snapshot.clone(),
            bytes: self.anchor.byte.min(self.head.byte)..self.anchor.byte.max(self.head.byte),
        }
    }
}
