//! 4.2a 资源管线的 crate 内验证：临时根、条目目录、迁移计划、引用重写、
//! 清理存活规则、生产读取源与恢复保留。真实文件用例仅 macOS；纯函数用例跨平台。

use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
use aline_source::Cancellation;

use super::catalog::{ResourceCatalog, TempResource, materialize_recovered, parse_locator};
use super::cleanup::{CleanupOutcome, ResourceCleanupJob, RetainedReason, plan_cleanup};
use super::migration::{
    ImageDirectory, ImageSettings, MigrationCompletion, MigrationError, ResourceMigration,
    ResourceMigrationJob, rewrite_locators,
};
use super::source::TempResourceSource;
use super::temp_root::TempRoot;
use super::*;
use crate::document::persistence::WriteKind;
use crate::document::recovery::{
    MAX_RESOURCE_BYTES, MAX_RESOURCE_COUNT, RecoveredResource, RecoveryGenerationInput,
    RecoveryPath, RecoveryPublishOutcome, RecoveryResourceError, RecoveryResourceInput,
    RecoveryResourceSource, RecoveryRoot, RecoveryStore,
};
use crate::document::{CurrentSnapshot, DocCore, DocLimits, DocumentId};

const TEXT_BUDGET: TextBudget = TextBudget::new(64 * 1024);

/// 把只实现 `Debug` 的内部错误转成测试用动态错误，避免为生产类型新增 trait。
fn ok<T, E: std::fmt::Debug>(result: Result<T, E>) -> Result<T, Box<dyn Error>> {
    result.map_err(|error| format!("{error:?}").into())
}

/// 4.2b 将经由 `crate::document` 的重导出命名资源面；这里显式引用这些名称，
/// 保证窗口接线前重导出在 crate 内可达，同时确认默认临时根推导不访问磁盘。
#[test]
fn resource_surface_is_nameable_through_document() {
    let home = Path::new("/tmp/aline-reexport-home");
    assert_eq!(
        crate::document::default_temp_root(home),
        home.join("Library/Application Support/Aline/temporary")
    );
    assert_eq!(crate::document::TEMPORARY_DIR, "temporary");

    // 只命名类型与函数项，不构造、不执行；编译通过即证明重导出可达。
    let _ = std::mem::size_of::<crate::document::ImageSettings>();
    let _ = std::mem::size_of::<crate::document::ImageDirectory>();
    let _ = std::mem::size_of::<crate::document::ResourceMigration>();
    let _ = std::mem::size_of::<crate::document::ResourceCleanupJob>();
    let _ = std::mem::size_of::<crate::document::TempDocumentId>();
    let _ = std::mem::size_of::<crate::document::TempResource>();
    let _ = std::mem::size_of::<crate::document::TempResourceId>();
    let _ = std::mem::size_of::<crate::document::TempRootError>();
    let _ = std::mem::size_of::<crate::document::TempResourceSource>();
    let _ = std::mem::size_of::<crate::document::DiscoveredAsset>();
    let _ = std::mem::size_of::<crate::document::CleanupOutcome>();
    let _ = std::mem::size_of::<crate::document::AssociationCommand>();
    let _ = std::mem::size_of::<crate::document::AssociationOutcome>();
    let _ = std::mem::size_of::<crate::document::AssociationStep>();
    let _ = std::mem::size_of::<crate::document::SaveTargetCommand>();
    let _ = crate::document::discover_relative_assets;
    let _ = crate::document::materialize_recovered;
    let _ = crate::document::rewrite_locators;
}

#[cfg(target_os = "macos")]
struct TempDir(PathBuf);

#[cfg(target_os = "macos")]
impl TempDir {
    fn new(prefix: &str) -> Result<Self, Box<dyn Error>> {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for attempt in 0..100u32 {
            let path = std::env::temp_dir().join(format!(
                "aline-resources-{prefix}-{}-{nanos:x}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("无法创建资源测试目录".into())
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn temp_root(&self) -> Result<TempRoot, Box<dyn Error>> {
        fs::create_dir_all(self.0.join("Library/Application Support"))?;
        ok(TempRoot::open_or_create(&self.0))
    }
}

#[cfg(target_os = "macos")]
impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(text: &str) -> Result<CurrentSnapshot, Box<dyn Error>> {
    let mut core = DocCore::new(
        text,
        SelectionSpec::new(0, 0),
        DocLimits::new(
            TEXT_BUDGET,
            LineIndexBudget::new(128),
            HistoryBudget::new(16, 64 * 1024),
        ),
    )?;
    Ok(core.current_snapshot(TEXT_BUDGET)?)
}

#[cfg(target_os = "macos")]
fn migration_ok(completion: MigrationCompletion) -> Result<ResourceMigration, Box<dyn Error>> {
    ok(completion.result)
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
fn run_migration(
    target: &Path,
    text: &str,
    entries: Vec<TempResource>,
    directory: &Path,
    settings: ImageSettings,
    kind: WriteKind,
) -> Result<MigrationCompletion, Box<dyn Error>> {
    Ok(ResourceMigrationJob::new(
        DocumentId::new(),
        1,
        kind,
        target.to_path_buf(),
        None,
        snapshot(text)?,
        entries,
        settings,
        directory.to_path_buf(),
        TEXT_BUDGET,
        Cancellation::new(),
    )
    .run())
}

#[cfg(target_os = "macos")]
#[test]
fn temp_root_creates_private_document_directory() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("root")?;
    let root = temp.temp_root()?;
    let (first, first_path) = ok(root.allocate_document(100))?;
    assert_eq!(first.value(), 100);
    assert_eq!(first.directory_name(), "0000000000000064");
    assert!(first_path.is_dir());
    let (second, second_path) = ok(root.allocate_document(100))?;
    assert_eq!(second.value(), 101);
    assert_ne!(first_path, second_path);
    assert!(second_path.is_dir());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn temp_image_stage_is_document_scoped_and_create_only() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("stage")?;
    let root = temp.temp_root()?;
    let mut first = ok(ResourceCatalog::new(DocumentId::new(), &root, 1))?;
    let mut other = ok(ResourceCatalog::new(DocumentId::new(), &root, 1))?;
    assert_ne!(first.directory(), other.directory());

    // 既有同序号文件不得被覆盖，分配推进到下一条。
    fs::write(first.directory().join("0000000000000000.png"), b"keep")?;
    let entry = ok(first.stage("pic.png", b"one", 1024))?;
    assert_eq!(
        entry.locator,
        format!(
            "aline-temp:{}/0000000000000001.png",
            first.temp_document().directory_name()
        )
    );
    assert_eq!(
        fs::read(first.directory().join("0000000000000000.png"))?,
        b"keep"
    );

    let other_entry = ok(other.stage("pic.png", b"two", 1024))?;
    assert_ne!(entry.locator, other_entry.locator);
    assert_eq!(
        fs::read(other.directory().join(&other_entry.file_name))?,
        b"two"
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn temp_image_stage_rejects_oversize_and_count_budget() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("budget")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 7))?;
    let oversized = vec![0_u8; MAX_RESOURCE_BYTES + 1];
    assert_eq!(
        catalog.stage("big.png", &oversized, u64::MAX).err(),
        Some(ResourceError::TooLarge)
    );
    assert!(catalog.entries().is_empty());
    for index in 0..MAX_RESOURCE_COUNT {
        ok(catalog.stage(&format!("n{index}.png"), b"x", 1024))?;
    }
    assert_eq!(
        catalog.stage("overflow.png", b"x", 1024).err(),
        Some(ResourceError::CountExceeded)
    );
    assert_eq!(catalog.entries().len(), MAX_RESOURCE_COUNT);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn temp_image_stage_failure_leaves_catalog_empty() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::PermissionsExt;

    let temp = TempDir::new("stagefail")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 3))?;
    let directory = catalog.directory().to_path_buf();
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o500))?;
    let result = catalog.stage("pic.png", b"data", 1024);
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    assert_eq!(result.err(), Some(ResourceError::Io));
    assert!(catalog.entries().is_empty());
    assert!(!directory.join("0000000000000000.png").exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn migration_copies_assets_without_overwrite() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("copy")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 11))?;
    let entry = ok(catalog.stage("pic.png", b"png-bytes", 1024))?;
    let text = format!("![x]({})", entry.locator);
    let target = temp.path().join("out/notes.md");
    fs::create_dir_all(target.parent().ok_or("target parent")?)?;

    let migration = migration_ok(run_migration(
        &target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        ImageSettings::default(),
        WriteKind::Create,
    )?)?;
    assert_eq!(migration.mappings.len(), 1);
    assert_eq!(migration.mappings[0].to, "./assets/pic.png");
    let asset = temp.path().join("out/assets/pic.png");
    assert_eq!(fs::read(&asset)?, b"png-bytes");
    assert_eq!(
        fs::read(catalog.directory().join(&entry.file_name))?,
        b"png-bytes"
    );

    // 目标已存在同名但内容不同：生成 -1，不覆盖。
    fs::write(&asset, b"existing")?;
    let migration = migration_ok(run_migration(
        &target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        ImageSettings::default(),
        WriteKind::Create,
    )?)?;
    assert_eq!(migration.mappings[0].to, "./assets/pic-1.png");
    assert_eq!(fs::read(&asset)?, b"existing");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn migration_skips_unreferenced_entries() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("skip")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 13))?;
    let used = ok(catalog.stage("used.png", b"used", 1024))?;
    let unused = ok(catalog.stage("unused.png", b"unused", 1024))?;
    let text = format!("![a]({})", used.locator);
    let target = temp.path().join("out/notes.md");
    fs::create_dir_all(target.parent().ok_or("target parent")?)?;

    let migration = migration_ok(run_migration(
        &target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        ImageSettings::default(),
        WriteKind::Create,
    )?)?;
    assert_eq!(migration.mappings.len(), 1);
    assert_eq!(migration.copied.len(), 1);
    assert!(!temp.path().join("out/assets/unused.png").exists());
    assert_eq!(
        fs::read(catalog.directory().join(&unused.file_name))?,
        b"unused"
    );
    Ok(())
}

#[test]
fn migration_rewrites_only_known_locators() {
    let mappings = vec![ResourceMapping {
        from: "aline-temp:0000000000000001/0000000000000002.png".to_owned(),
        to: "./assets/pic.png".to_owned(),
    }];
    let source = "![a](aline-temp:0000000000000001/0000000000000002.png) \
                  ![b](https://example.invalid/pic.png) 普通同名文本";
    let rewritten = rewrite_locators(source, &mappings);
    assert_eq!(
        rewritten,
        "![a](./assets/pic.png) ![b](https://example.invalid/pic.png) 普通同名文本"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn migration_relative_and_absolute_settings_choose_expected_reference() -> Result<(), Box<dyn Error>>
{
    let temp = TempDir::new("settings")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 17))?;
    let entry = ok(catalog.stage("pic.png", b"png", 1024))?;
    let text = format!("![x]({})", entry.locator);
    let target = temp.path().join("out/notes.md");
    fs::create_dir_all(target.parent().ok_or("target parent")?)?;

    let migration = migration_ok(run_migration(
        &target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        ImageSettings::default(),
        WriteKind::Create,
    )?)?;
    assert_eq!(migration.mappings[0].to, "./assets/pic.png");

    let absolute = temp.path().join("abs-assets");
    let settings = ImageSettings {
        directory: ImageDirectory::Absolute(absolute.clone()),
    };
    let absolute_target = temp.path().join("out2/notes.md");
    fs::create_dir_all(absolute_target.parent().ok_or("target parent")?)?;
    let migration = migration_ok(run_migration(
        &absolute_target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        settings,
        WriteKind::Create,
    )?)?;
    let reference = &migration.mappings[0].to;
    assert!(reference.starts_with('<') && reference.ends_with('>'));
    assert!(reference.contains(&absolute.to_string_lossy().replace('\\', "/")));
    assert_eq!(fs::read(absolute.join("pic.png"))?, b"png");
    assert!(!temp.path().join("out2/assets").exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn migration_copy_failure_removes_created_assets_and_keeps_temp() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("rollback")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 19))?;
    let first = ok(catalog.stage("first.png", b"first", 1024))?;
    let second = ok(catalog.stage("second.png", b"second", 1024))?;
    // 第二个临时源缺失：第一个资产已创建后复制失败，必须按身份撤回。
    fs::remove_file(catalog.directory().join(&second.file_name))?;
    let text = format!("![a]({}) ![b]({})", first.locator, second.locator);
    let target = temp.path().join("out/notes.md");
    fs::create_dir_all(target.parent().ok_or("target parent")?)?;

    let completion = run_migration(
        &target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        ImageSettings::default(),
        WriteKind::Create,
    )?;
    assert_eq!(completion.result.err(), Some(MigrationError::Io));
    assert!(!temp.path().join("out/assets/first.png").exists());
    assert!(catalog.directory().join(&first.file_name).exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn migration_retry_reuses_identical_destination_bytes() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("retry")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 23))?;
    let entry = ok(catalog.stage("pic.png", b"png", 1024))?;
    let text = format!("![x]({})", entry.locator);
    let target = temp.path().join("out/notes.md");
    fs::create_dir_all(target.parent().ok_or("target parent")?)?;

    let first = migration_ok(run_migration(
        &target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        ImageSettings::default(),
        WriteKind::Create,
    )?)?;
    let second = migration_ok(run_migration(
        &target,
        &text,
        catalog.entries().to_vec(),
        catalog.directory(),
        ImageSettings::default(),
        WriteKind::Create,
    )?)?;
    assert_eq!(first.mappings, second.mappings);
    assert_eq!(first.copied, second.copied);
    assert!(!temp.path().join("out/assets/pic-2.png").exists());
    Ok(())
}

/// 非 macOS 必须在任何文件系统副作用之前失败关闭；该用例只在对应平台编译运行。
#[cfg(not(target_os = "macos"))]
#[test]
fn migration_fails_closed_on_unsupported_platform() -> Result<(), Box<dyn Error>> {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let home = std::env::temp_dir().join(format!(
        "aline-unsupported-{}-{unique:x}",
        std::process::id()
    ));
    let target = home.join("docs/notes.md");
    let snapshot = snapshot("plain")?;
    let job = ResourceMigrationJob::new(
        snapshot.document_id().clone(),
        0,
        WriteKind::Existing,
        target,
        None,
        snapshot,
        Vec::new(),
        ImageSettings::default(),
        home.join("temp"),
        TEXT_BUDGET,
        Cancellation::new(),
    );
    let completion = job.run();
    assert_eq!(
        completion.result.as_ref().err(),
        Some(&MigrationError::UnsupportedPlatform),
        "非 macOS 迁移必须在写盘前失败关闭"
    );
    assert!(!home.exists(), "非 macOS 不得创建资产目录");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn cleanup_plan_retains_open_recovery_in_flight() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("plan")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 29))?;
    let first = ok(catalog.stage("a.png", b"a", 1024))?;
    let second = ok(catalog.stage("b.png", b"b", 1024))?;
    let entries = catalog.entries().to_vec();

    let open = plan_cleanup(&entries, true, &BTreeSet::new(), 0);
    assert!(open.removable.is_empty());
    assert!(
        open.retained
            .iter()
            .all(|(_, reason)| *reason == RetainedReason::DocumentOpen)
    );

    let mut references = BTreeSet::new();
    references.insert(first.locator.clone());
    let recovery = plan_cleanup(&entries, false, &references, 0);
    assert_eq!(recovery.removable, vec![second.id]);
    assert_eq!(
        recovery.retained,
        vec![(first.id, RetainedReason::Recovery)]
    );

    let in_flight = plan_cleanup(&entries, false, &BTreeSet::new(), 1);
    assert!(in_flight.removable.is_empty());
    assert!(
        in_flight
            .retained
            .iter()
            .all(|(_, reason)| *reason == RetainedReason::InFlight)
    );

    let free = plan_cleanup(&entries, false, &BTreeSet::new(), 0);
    assert_eq!(free.removable.len(), 2);
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn cleanup_job_removes_only_unreferenced_temp_files() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("cleanup")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 31))?;
    let entry = ok(catalog.stage("pic.png", b"png", 1024))?;
    let asset = temp.path().join("assets/pic.png");
    fs::create_dir_all(asset.parent().ok_or("asset parent")?)?;
    fs::write(&asset, b"asset")?;

    let job = ResourceCleanupJob::new(
        catalog.directory().to_path_buf(),
        catalog.entries().to_vec(),
        None,
    );
    assert_eq!(job.run(), CleanupOutcome::Cleaned);
    assert!(!catalog.directory().join(&entry.file_name).exists());
    assert_eq!(fs::read(&asset)?, b"asset");
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn cleanup_job_skips_everything_when_recovery_scan_fails() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("cleanup-corrupt")?;
    let resources = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &resources, 37))?;
    let entry = ok(catalog.stage("pic.png", b"png", 1024))?;

    let recovery_root = temp.path().join("recovery");
    fs::create_dir(&recovery_root)?;
    let root = ok(RecoveryRoot::open(&recovery_root))?;
    let store = ok(RecoveryStore::open(root))?;
    let document = ok(store.open_lineage(100))?;
    // 写入一具无法解码的 manifest，使引用扫描 fail closed。
    let generation = recovery_root
        .join("v1/active")
        .join(document.directory_name())
        .join("g/1");
    fs::create_dir_all(&generation)?;
    fs::write(generation.join("manifest"), b"not-a-manifest")?;

    let job = ResourceCleanupJob::new(
        catalog.directory().to_path_buf(),
        catalog.entries().to_vec(),
        Some((store, document)),
    );
    assert_eq!(job.run(), CleanupOutcome::Skipped);
    assert!(catalog.directory().join(&entry.file_name).exists());
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn temp_resource_source_reads_by_locator_and_rejects_escape() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("source")?;
    let root = temp.temp_root()?;
    let mut catalog = ok(ResourceCatalog::new(DocumentId::new(), &root, 41))?;
    let entry = ok(catalog.stage("pic.png", b"png-bytes", 1024))?;
    let source = TempResourceSource::new(root.clone());
    assert_eq!(
        ok(source.read(&entry.locator, 1024))?,
        b"png-bytes".to_vec()
    );
    assert_eq!(
        source.read(&entry.locator, 3).err(),
        Some(RecoveryResourceError::TooLarge)
    );
    assert_eq!(
        source.read("aline-temp:../../escape.png", 1024).err(),
        Some(RecoveryResourceError::Unavailable)
    );
    // 指向另一个文档目录但文件不存在：跨文档路径不获得读取结果。
    let other = ok(ResourceCatalog::new(DocumentId::new(), &root, 43))?;
    let missing = format!(
        "aline-temp:{}/0000000000000000.png",
        other.temp_document().directory_name()
    );
    assert_eq!(
        source.read(&missing, 1024).err(),
        Some(RecoveryResourceError::Unavailable)
    );

    // 普通文件被替换为符号链接后拒绝读取。
    let path = catalog.directory().join(&entry.file_name);
    fs::remove_file(&path)?;
    let outside = temp.path().join("outside.png");
    fs::write(&outside, b"outside")?;
    std::os::unix::fs::symlink(&outside, &path)?;
    assert_eq!(
        source.read(&entry.locator, 1024).err(),
        Some(RecoveryResourceError::Unavailable)
    );
    Ok(())
}

#[cfg(target_os = "macos")]
#[test]
fn recovered_resources_remain_readable_until_explicit_discard() -> Result<(), Box<dyn Error>> {
    let temp = TempDir::new("recovered")?;
    let recovery_root = temp.path().join("recovery");
    fs::create_dir(&recovery_root)?;
    let root = ok(RecoveryRoot::open(&recovery_root))?;
    let store = ok(RecoveryStore::open(root))?;
    let document = ok(store.open_lineage(100))?;

    let locator = "aline-temp:0000000000000005/0000000000000001.png".to_owned();
    let source = format!("# 恢复\n\n![x]({locator})");
    let input = RecoveryGenerationInput {
        path_generation: 7,
        captured_composition: false,
        path: RecoveryPath::Unlinked,
        anchor_byte: 0,
        head_byte: 0,
        source: source.clone(),
        logical_saved: None,
        baseline: None,
        resources: vec![RecoveryResourceInput {
            reference: locator.clone(),
            bytes: b"recovered-bytes".to_vec(),
        }],
    };
    let cancellation = Cancellation::new();
    let generation = match ok(store.publish(document, input, &cancellation))? {
        RecoveryPublishOutcome::Published { generation, .. } => generation,
        other => return Err(format!("期望 Published，实际 {other:?}").into()),
    };

    // 临时源文件从未存在也不影响恢复读取。
    let recovered = ok(store.recover(document, generation))?;
    assert_eq!(recovered.source, source);
    assert_eq!(recovered.resources.len(), 1);
    assert_eq!(recovered.resources[0].reference, locator);
    assert_eq!(recovered.resources[0].bytes, b"recovered-bytes");
    let scan = ok(store.scan())?;
    let lineage = scan
        .recoverable_lineages()
        .into_iter()
        .find(|lineage| lineage.document == document)
        .ok_or("应列出完整代")?;
    assert_eq!(
        lineage
            .latest_complete()
            .map(|complete| complete.generation),
        Some(generation)
    );

    ok(store.discard(document))?;
    let scan = ok(store.scan())?;
    assert!(
        scan.recoverable_lineages()
            .iter()
            .all(|lineage| lineage.document != document)
    );
    Ok(())
}

// 未在 13.1 列表中的恢复专用入口由 4.2b 用例覆盖；此处保留编译期引用。
#[cfg(target_os = "macos")]
#[allow(dead_code)]
fn _materialize_reference(root: &TempRoot, resources: &[RecoveredResource]) -> ResourceError {
    match materialize_recovered(root, resources) {
        Ok(_) => ResourceError::Io,
        Err(error) => error,
    }
}

// 纯编译期引用：确保 locator 解析器保持可用。
#[allow(dead_code)]
fn _pure_reference() -> Option<super::catalog::ParsedLocator> {
    parse_locator("aline-temp:0000000000000001/0000000000000002.png")
}
