//! 宿主文档身份、输入信封与无窗口文档核心；不实现窗口、输入适配或文件协调器。

mod composition;
mod doc_core;
mod identity;
mod mode;
mod open_block;
mod publication;
mod resource;
mod snapshot;

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

pub use mode::{EditorMode, ModeSwitchOutcome};
pub use resource::{ResourceReference, WeakResourceReference};
