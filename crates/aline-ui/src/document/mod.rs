//! 宿主文档身份、输入信封与无窗口文档核心；不实现窗口、输入适配或文件协调器。

mod composition;
mod doc_core;
mod identity;
mod mode;
#[cfg(any(test, target_os = "macos"))]
pub(crate) mod native_input_coordinates;
mod open_block;
#[cfg(test)]
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
