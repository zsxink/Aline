//! 源码补丁、坐标及历史的纯逻辑基础，不解析或写盘。

mod batch;
mod coordinates;
mod text;

pub use batch::{CheckedBatch, PatchBudget};
pub use coordinates::{Affinity, DocumentRevision, SnapshotId, SrcOffset, SrcRange, SrcSelection};
pub use text::{BufferError, TextBudget, TextSnapshot};
