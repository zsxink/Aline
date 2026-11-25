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
mod recovery;
mod resource;
mod resources;
mod snapshot;
mod statistics;

pub use composition::CompositionView;
pub use doc_core::{CommitOutcome, DocCore, DocError, DocLimits};

/// 原生窗口后台输入任务；仅 macOS 窗口适配消费。
#[cfg(target_os = "macos")]
pub(crate) use doc_core::{NativeInputJob, PreparedCommand, PreparedNativeInput};

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

/// 恢复打开失败分类仅由 crate 内测试断言 Busy/DocumentOpen；生产路径只取失败。
#[cfg(test)]
pub(crate) use readonly_session::RecoveredOpenError;
pub(crate) use readonly_session::{AutosaveAction, OpenTimings, StageTimer, StageTiming};
pub(crate) use statistics::DocumentStatistics;

pub(crate) use recovery::{
    PreparedRecovery, RecoveryAction, RecoveryCloseJob, RecoveryConfig, RecoveryConfigError,
    RecoveryDiscardCompletion, RecoveryDiscardJob, RecoveryDocumentId, RecoveryGenerationStatus,
    RecoveryLineageDisposition, RecoveryPrepareJob, RecoveryPublishJob, RecoveryResourceSource,
    RecoveryRestoreCompletion, RecoveryRestoreJob, RecoveryScan, RecoverySettlement, RecoveryStart,
    RecoveryStore, RecoveryStoreError, RestoredRecovery, RetireOutcome, UnavailableResources,
    create_default_root,
};
/// 恢复根仅由 crate 内测试直接构造；生产根接线归 4.2。
#[cfg(test)]
pub(crate) use recovery::{
    RecoveredDocument, RecoveredPath, RecoveryGenerationInput, RecoveryGenerationScan,
    RecoveryLineageScan, RecoveryPath, RecoveryPublishOutcome, RecoveryResourceInput,
    RecoveryRestoreError, RecoveryRoot,
};

pub(crate) use persistence::{
    ExternalApplyResult, ExternalCheckCommand, ExternalCheckCompletion, ExternalCheckIntent,
    ExternalCheckJob, ExternalCheckRejection, ExternalConflictView, ExternalDiskState, SaveCommand,
    SaveJob, SaveRejection, SaveResult, WriteCompletion, WriteKind,
};

/// 4.2b 首次保存/另存为动作消费的关联命令；窗口接线前只由 crate 内测试驱动。
#[allow(unused_imports)]
pub(crate) use readonly_session::{
    AssociationCommand, AssociationOutcome, AssociationStep, SaveTargetCommand,
};
/// 图片目录设置的失败分类只由只读会话测试入口使用。
#[cfg(test)]
pub(crate) use resources::ImageSettingsError;
/// 4.2b 窗口与恢复接线将消费的资源面；接线前仅由 crate 内测试命名。
#[allow(unused_imports)]
pub(crate) use resources::{
    CleanupOutcome, DiscoveredAsset, ImageDirectory, ResourceMigration, TEMPORARY_DIR,
    TempDocumentId, TempResource, TempResourceId, TempResourceSource, TempRootError,
    default_temp_root, discover_relative_assets, materialize_recovered, rewrite_locators,
};
/// 4.2a 会话已消费的资源面；从这里导入避免会话与资源模块直接耦合。
pub(crate) use resources::{
    ImageSettings, MigrationCompletion, MigrationError, ResourceCatalog, ResourceCleanupJob,
    ResourceError, ResourceMigrationJob, TempRoot, locator_spans,
};

pub use mode::{EditorMode, ModeSwitchOutcome};
pub use readonly_session::{
    CloseDisposition, OpenDisposition, OpenError, OpenRequestId, ParseRefreshCompletion,
    ParseRefreshDisposition, ParseRefreshJob, ReadonlyDocument, ReadonlyFileAccess,
    ReadonlyFileInput, ReadonlyOpenCompletion, ReadonlyOpenJob, ReadonlyOpenLimits,
    ReadonlyOpenPhase, ReadonlySession,
};
pub use resource::{ResourceReference, WeakResourceReference};
