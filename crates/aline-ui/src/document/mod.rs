//! 宿主文档身份、输入信封与无窗口文档核心；不实现窗口或输入适配。
//! 文件写盘协调器随可编辑文档会话创建，只处理已打开目标的队列与结算。

mod composition;
mod doc_core;
mod identity;
mod mode;
#[cfg(any(test, target_os = "macos"))]
pub(crate) mod native_input_coordinates;
mod open_block;
mod persistence;
mod publication;
mod readonly_session;
#[cfg(test)]
mod recovery_record;
mod resource;
mod snapshot;
mod statistics;

pub use composition::CompositionView;
pub use doc_core::{CommitOutcome, DocCore, DocError, DocLimits};

/// 原生窗口后台输入任务；仅 macOS 窗口适配消费。
#[cfg(target_os = "macos")]
pub(crate) use doc_core::{NativeInputJob, PreparedNativeInput};

/// 结构化解析计数；仅 macOS 无头输入测试消费。
#[cfg(all(test, target_os = "macos"))]
pub(crate) use doc_core::parse_call_count;
pub use identity::{
    DerivedRequestId, DocumentId, DraftGeneration, IdentityError, InputIdentity,
    ParseConfigGeneration, WorkingVersion,
};
pub use publication::{
    PublicationKind, PublicationOutcome, PublicationResult, WaitHandle, WaitStatus, WaitView,
};
pub use snapshot::CurrentSnapshot;

pub(crate) use readonly_session::{OpenTimings, StageTimer, StageTiming};
pub(crate) use statistics::DocumentStatistics;

pub use mode::{EditorMode, ModeSwitchOutcome};
pub use readonly_session::{
    CloseDisposition, OpenDisposition, OpenError, OpenRequestId, ParseRefreshCompletion,
    ParseRefreshDisposition, ParseRefreshJob, ReadonlyDocument, ReadonlyFileAccess,
    ReadonlyFileInput, ReadonlyOpenCompletion, ReadonlyOpenJob, ReadonlyOpenLimits,
    ReadonlyOpenPhase, ReadonlySession,
};
pub use resource::{ResourceReference, WeakResourceReference};
