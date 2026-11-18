//! 文件字节与存储元数据的受检边界，独立于文档缓冲。

mod atomic;
mod baseline;
mod decode;
mod encode;
pub mod fs;
mod storage;

pub use atomic::{
    AtomicFailure, AtomicFailureKind, AtomicStage, BaselineError, Cancellation, ExistingTarget,
    ExistingTargetKind, InvalidInput, IoBudget, PreparedAtomic, PreparedReplacement,
    ReplacementCleanup, ReplacementDurability, ReplacementReceipt, Written, prepare_atomic,
    prepare_checked_replace,
};
pub use baseline::SourceBaseline;
pub use decode::{DecodeError, DecodedSource, LineEnding, StorageMetadata, decode};
pub use encode::{EncodeError, EncodedBytes, encode};
pub use storage::{
    BaselineId, DiskVersion, StorageBudget, StorageError, StorageRange, StorageSnapshot,
};
