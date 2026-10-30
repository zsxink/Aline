//! 文件字节与存储元数据的受检边界，独立于文档缓冲。

mod storage;

pub use storage::{
    BaselineId, DiskVersion, StorageBudget, StorageError, StorageRange, StorageSnapshot,
};
