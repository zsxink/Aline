//! 只读存储字节的实例身份、范围与代次；不解码、不写盘、不持编辑历史。
//!
//! StorageSnapshot 只是宿主提供的字节视图，不代表这些 bytes 已通过 UTF-8 验证，
//! 也不伪造磁盘写入成功。后续 decode/encode 在此身份之上建立受检内容证据。

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

/// 存储身份、字节范围、预算或代次校验失败。
#[derive(Debug, Eq, PartialEq)]
pub enum StorageError {
    /// 范围起点大于终点。
    ReversedRange,
    /// 字节端点超出存储视图。
    OutOfBounds {
        /// 非法端点。
        offset: usize,
        /// 当前视图长度。
        len: usize,
    },
    /// 范围属于另一存储基线。
    BaselineMismatch,
    /// 请求量超出注入预算。
    BudgetExceeded {
        /// 请求字节数。
        needed: usize,
        /// 上限。
        limit: usize,
    },
    /// 存储会话代次耗尽。
    VersionExhausted,
}

impl fmt::Display for StorageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for StorageError {}

/// 一次存储字节消费的注入预算，不是发布大小限额。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageBudget {
    max_bytes: usize,
}

impl StorageBudget {
    /// 构造字节上限。
    pub const fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }

    /// 返回字节上限。
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }

    fn check(self, needed: usize) -> Result<(), StorageError> {
        if needed > self.max_bytes {
            return Err(StorageError::BudgetExceeded {
                needed,
                limit: self.max_bytes,
            });
        }
        Ok(())
    }
}

/// 存储基线的实例局部身份；不能由路径、版本整数或字符串伪造。
#[derive(Clone)]
pub struct BaselineId(Arc<()>);

impl PartialEq for BaselineId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for BaselineId {}

impl fmt::Debug for BaselineId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("BaselineId(..)")
    }
}

/// 由宿主会话推进的磁盘内容代次；它不能单独充当基线相等证据。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DiskVersion(u64);

impl DiskVersion {
    /// 初始存储代次。
    pub const fn initial() -> Self {
        Self(0)
    }

    /// 构造只读代次值；不证明任何文件写入事实。
    pub const fn from_value(value: u64) -> Self {
        Self(value)
    }

    /// 返回会话代次数值。
    pub const fn value(self) -> u64 {
        self.0
    }

    /// 受检推进代次，耗尽时明确失败。
    pub fn next(self) -> Result<Self, StorageError> {
        self.0
            .checked_add(1)
            .map(Self)
            .ok_or(StorageError::VersionExhausted)
    }
}

/// 绑定存储基线的原始字节半开范围，不等同源码 UTF-8 范围。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageRange {
    baseline: BaselineId,
    bytes: Range<usize>,
}

impl StorageRange {
    /// 返回原始字节区间，含 BOM/CRLF 等存储表示。
    pub fn byte_range(&self) -> Range<usize> {
        self.bytes.clone()
    }

    /// 返回所属基线身份。
    pub fn baseline_id(&self) -> &BaselineId {
        &self.baseline
    }
}

/// 不可变原始存储视图；内容由宿主输入，source 不导入 buffer/UI 快照。
#[derive(Clone)]
pub struct StorageSnapshot {
    id: BaselineId,
    version: DiskVersion,
    bytes: Arc<Vec<u8>>,
}

impl fmt::Debug for StorageSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StorageSnapshot")
            .field("id", &self.id)
            .field("version", &self.version)
            .field("len", &self.bytes.len())
            .finish_non_exhaustive()
    }
}

impl StorageSnapshot {
    /// 接管原始 bytes 并创建新身份；超预算拒绝，不进行编码判断或 I/O。
    pub fn new(
        bytes: Vec<u8>,
        version: DiskVersion,
        budget: StorageBudget,
    ) -> Result<Self, StorageError> {
        budget.check(bytes.len())?;
        Ok(Self {
            id: BaselineId(Arc::new(())),
            version,
            bytes: Arc::new(bytes),
        })
    }

    /// 返回实例基线身份，clone 视图保留该身份。
    pub fn id(&self) -> &BaselineId {
        &self.id
    }

    /// 返回宿主提供的会话代次。
    pub const fn version(&self) -> DiskVersion {
        self.version
    }

    /// 返回原始字节的只读切片；不把它当作已解码源码。
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// 构造绑定本基线的字节范围；不把存储字节误作 UTF-8 字符边界。
    pub fn range(&self, bytes: Range<usize>) -> Result<StorageRange, StorageError> {
        if bytes.start > bytes.end {
            return Err(StorageError::ReversedRange);
        }
        if bytes.end > self.bytes.len() {
            return Err(StorageError::OutOfBounds {
                offset: bytes.end,
                len: self.bytes.len(),
            });
        }
        Ok(StorageRange {
            baseline: self.id.clone(),
            bytes,
        })
    }

    /// 复验基线身份后读取原 bytes；错误基线即使文本相同也拒绝。
    pub fn slice(&self, range: &StorageRange) -> Result<&[u8], StorageError> {
        if range.baseline != self.id {
            return Err(StorageError::BaselineMismatch);
        }
        self.bytes
            .get(range.bytes.clone())
            .ok_or(StorageError::OutOfBounds {
                offset: range.bytes.end,
                len: self.bytes.len(),
            })
    }
}
