//! 源码补丁、坐标、行索引及历史的纯逻辑基础，不解析或写盘。

mod anchors;
mod batch;
mod coordinates;
mod history;
mod line_index;
mod string_buffer;
mod text;

pub use anchors::{AnchorTransform, SelectionTransform, transform_anchor, transform_selection};
pub use batch::{CheckedBatch, PatchBudget};
pub use coordinates::{Affinity, DocumentRevision, SnapshotId, SrcOffset, SrcRange, SrcSelection};
pub use history::{
    HistoryBudget, HistoryStep, HistoryStepHandle, OperationSource, SelectionSpec,
    TYPING_MERGE_WINDOW_MILLIS, TrimOutcome,
};
pub use line_index::{LineIndex, LineIndexBudget};
pub use string_buffer::{AppliedBatch, ApplyOutcome, ReloadOutcome, StringBuffer};
pub use text::{BufferError, TextBudget, TextSnapshot};
