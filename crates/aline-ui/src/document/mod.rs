//! 宿主文档身份、输入信封与无窗口文档核心；不实现窗口、输入适配或文件协调器。

mod composition;
mod doc_core;
mod identity;
mod mode;
mod open_block;
mod publication;
mod readonly_session;
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

pub(crate) use statistics::DocumentStatistics;

pub use mode::{EditorMode, ModeSwitchOutcome};
pub use readonly_session::{
    OpenDisposition, OpenError, OpenRequestId, ReadonlyDocument, ReadonlyFileAccess,
    ReadonlyFileInput, ReadonlyOpenCompletion, ReadonlyOpenJob, ReadonlyOpenLimits,
    ReadonlyOpenPhase, ReadonlySession,
};
pub use resource::{ResourceReference, WeakResourceReference};
