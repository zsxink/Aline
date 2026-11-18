//! W4 资源保全：临时资源根、文档私有资源目录、保存迁移、引用清理与生产读取源。
//! 本模块不修改文本、不登记 `DocCore`；会话层完成登记与引用重写。
//!
//! 恢复专用的 `materialize_recovered` 与 `ResourceCatalog::restore` 已在恢复
//! 打开路径接线；物化句柄保证未采纳时会清理本次新建材料。
pub(crate) mod catalog;
pub(crate) mod cleanup;
pub(crate) mod migration;
pub(crate) mod source;
pub(crate) mod temp_root;
#[cfg(test)]
mod tests;

pub(crate) use catalog::{
    MaterializedRecovery, ResourceCatalog, ResourceError, TempResource, TempResourceId,
    materialize_recovered, materialize_recovered_owned,
};
pub(crate) use cleanup::{CleanupOutcome, ResourceCleanupJob};
/// 图片目录设置的失败分类只由只读会话测试入口消费。
#[cfg(test)]
pub(crate) use migration::ImageSettingsError;
pub(crate) use migration::{
    DiscoveredAsset, ImageDirectory, ImageSettings, MigrationCompletion, MigrationError,
    ResourceMapping, ResourceMigration, ResourceMigrationJob, discover_relative_assets,
    locator_spans, rewrite_locators,
};
pub(crate) use source::TempResourceSource;
pub(crate) use temp_root::{
    TEMPORARY_DIR, TempDocumentId, TempRoot, TempRootError, default_temp_root,
};
