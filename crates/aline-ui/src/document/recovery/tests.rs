//! W4 4.1a/4.1b 恢复层测试：根/存储/扫描/恢复/退役的真实文件行为，以及
//! 会话协调器的准备、发布、跳过、取消与关闭退役。所有真实文件测试只在
//! macOS 运行，使用自建临时目录，绝不写入真实 HOME；不使用
//! `unwrap/expect/panic`，失败一律经 `Result<(), Box<dyn Error>>` 返回。

#![cfg(all(test, target_os = "macos"))]

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fs;
use std::io::{self, Cursor};
use std::num::NonZeroU64;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::{Cancellation, StorageBudget};

use crate::{
    DocLimits, OpenDisposition, ReadonlyDocument, ReadonlyFileAccess, ReadonlyFileInput,
    ReadonlyOpenLimits, ReadonlySession, ResourceReference,
};

use super::control::{NextId, Reservation};
use super::coordinator::{
    PreparedRecovery, RECOVERY_FAILED_KEY, RECOVERY_INCOMPLETE_KEY, RecoverySkip, RecoveryStart,
};
use super::record::{
    MAX_ACTIVE_LINEAGES, MAX_GENERATION_RESOURCE_BYTES, MAX_INCOMPLETE_RESERVATIONS,
    MAX_MANIFEST_BYTES, MAX_RESOURCE_BYTES, MAX_TEXT_BYTES, NEXT_ID_BYTES,
    RECOVERY_INTERVAL_MILLIS, RESERVATION_BYTES, RETIRED_MANIFEST_BYTES, ROOT_PAYLOAD_LIMIT,
    RecoveryRecord,
};
use super::resource::{
    RecoveryConfig, RecoveryConfigError, RecoveryResourceError, RecoveryResourceSource,
};
use super::root::{RECOVERY_DIR, RecoveryRoot, RootError, default_recovery_root};
use super::scan::{RecoveryGenerationStatus, RecoveryLineageDisposition};
use super::store::{
    RecoveredPath, RecoveryBudget, RecoveryDefect, RecoveryDocumentId, RecoveryGenerationInput,
    RecoveryLock, RecoveryOperation, RecoveryPath, RecoveryPublishCause, RecoveryPublishOutcome,
    RecoveryReadError, RecoveryResourceInput, RecoveryStore, RecoveryStoreError, RetireOutcome,
};

struct TempRecoveryDir {
    path: PathBuf,
}

impl TempRecoveryDir {
    fn new() -> Result<Self, Box<dyn Error>> {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aline-recovery-{}-{nanos}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        Ok(Self { path })
    }

    fn root_path(&self) -> PathBuf {
        self.path.join(RECOVERY_DIR)
    }

    fn create_root(&self) -> Result<RecoveryRoot, Box<dyn Error>> {
        fs::create_dir(self.root_path())?;
        Ok(RecoveryRoot::open(&self.root_path()).map_err(|error| format!("{error:?}"))?)
    }
}

impl Drop for TempRecoveryDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn open_store(temp: &TempRecoveryDir) -> Result<RecoveryStore, Box<dyn Error>> {
    let root = temp.create_root()?;
    Ok(RecoveryStore::open(root).map_err(|error| format!("{error:?}"))?)
}

fn open_lineage(store: &RecoveryStore) -> Result<RecoveryDocumentId, Box<dyn Error>> {
    store
        .open_lineage(100)
        .map_err(|error| format!("{error:?}").into())
}

fn sample_input(source: &str) -> RecoveryGenerationInput {
    RecoveryGenerationInput {
        path_generation: 7,
        captured_composition: false,
        path: RecoveryPath::Unlinked,
        anchor_byte: 0,
        head_byte: 0,
        source: source.to_owned(),
        logical_saved: None,
        baseline: None,
        resources: Vec::new(),
    }
}

fn resource(reference: &str, bytes: &[u8]) -> RecoveryResourceInput {
    RecoveryResourceInput {
        reference: reference.to_owned(),
        bytes: bytes.to_vec(),
    }
}

fn publish_ok(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    input: RecoveryGenerationInput,
) -> Result<NonZeroU64, Box<dyn Error>> {
    let cancellation = Cancellation::new();
    match store
        .publish(document, input, &cancellation)
        .map_err(|error| format!("{error:?}"))?
    {
        RecoveryPublishOutcome::Published {
            generation,
            generation_bytes,
        } => {
            assert!(generation_bytes > 0);
            Ok(generation)
        }
        other => Err(format!("期望 Published，实际 {other:?}").into()),
    }
}

fn publish_with_cancellation(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    input: RecoveryGenerationInput,
) -> Result<RecoveryPublishOutcome, Box<dyn Error>> {
    let cancellation = Cancellation::new();
    cancellation.cancel();
    store
        .publish(document, input, &cancellation)
        .map_err(|error| format!("{error:?}").into())
}

/// 1x1 RGBA PNG；真实窗口恢复后由驱动独立解码核对像素尺寸。
const SEED_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xcf, 0xc0, 0x50,
    0x0f, 0x00, 0x04, 0x85, 0x01, 0x80, 0x84, 0xa9, 0x8c, 0x21, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

/// 真实窗口图片恢复证据播种入口：以生产恢复编码器向隔离 HOME 的生产根写入
/// 一条含内嵌 PNG 的完整代，供 `scripts/drive_m2_recovery_image.py` 启动
/// 真实 aline 消费。
///
/// 仅在显式设置 `ALINE_RECOVERY_SEED_HOME` 与 `ALINE_RECOVERY_SEED_TARGET`
/// 时执行；默认忽略，不参与常规测试。HOME 必须与当前进程 HOME 不同，避免
/// 写入真实恢复根。测试只编码记录，不写目标夹具。
#[test]
#[ignore = "证据播种：需 --ignored 与 ALINE_RECOVERY_SEED_HOME/TARGET"]
fn seed_recovery_generation_with_embedded_png_for_real_window() -> Result<(), Box<dyn Error>> {
    let home = std::env::var_os("ALINE_RECOVERY_SEED_HOME")
        .map(PathBuf::from)
        .ok_or("缺少 ALINE_RECOVERY_SEED_HOME")?;
    let target = std::env::var_os("ALINE_RECOVERY_SEED_TARGET")
        .map(PathBuf::from)
        .ok_or("缺少 ALINE_RECOVERY_SEED_TARGET")?;
    if !home.is_absolute() || !target.is_absolute() {
        return Err("播种 HOME 与目标夹具必须是绝对路径".into());
    }
    if std::env::var_os("HOME").as_deref() == Some(home.as_os_str()) {
        return Err("播种 HOME 不得等于当前进程 HOME，避免写入真实恢复根".into());
    }
    if !target.is_file() {
        return Err("播种目标夹具不存在".into());
    }
    let root = super::create_default_root(&home).map_err(|error| format!("{error:?}"))?;
    let store = RecoveryStore::open(root).map_err(|error| format!("{error:?}"))?;
    let document = open_lineage(&store)?;
    // 固定临时文档/资源身份：0xa1 / 0xb2，扩展名 png。
    let locator = "aline-temp:00000000000000a1/00000000000000b2.png";
    let source = format!("# 恢复图片验收\n\n![内嵌图片]({locator})\n\n源文件不应被改写。\n");
    let input = RecoveryGenerationInput {
        path_generation: 7,
        captured_composition: false,
        path: RecoveryPath::from_os_path(&target),
        anchor_byte: 0,
        head_byte: 0,
        source,
        logical_saved: None,
        baseline: None,
        resources: vec![resource(locator, SEED_PNG)],
    };
    let cancellation = Cancellation::new();
    let generation = match store
        .publish(document, input, &cancellation)
        .map_err(|error| format!("{error:?}"))?
    {
        RecoveryPublishOutcome::Published { generation, .. } => generation,
        other => return Err(format!("期望 Published，实际 {other:?}").into()),
    };
    println!(
        "ALINE-RECOVERY-SEED document={} generation={} locator={} target_name={}",
        document.directory_name(),
        generation,
        locator,
        target.file_name().unwrap_or_default().to_string_lossy()
    );
    Ok(())
}

fn read_next(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
) -> Result<NextId, Box<dyn Error>> {
    let bytes = fs::read(store.next_id(document))?;
    Ok(NextId::decode(&bytes).map_err(|error| format!("{error:?}"))?)
}

fn write_next(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    next: NextId,
) -> Result<(), Box<dyn Error>> {
    fs::write(store.next_id(document), next.encode())?;
    Ok(())
}

fn write_reservation(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    generation: u64,
    reserved_bytes: u64,
) -> Result<(), Box<dyn Error>> {
    let generation = NonZeroU64::new(generation).ok_or("generation 必须非零")?;
    let reservation = Reservation {
        document: document.get(),
        generation,
        reserved_bytes,
    };
    fs::write(
        store.reservation_path(document, generation),
        reservation.encode(),
    )?;
    Ok(())
}

fn generation_names(store: &RecoveryStore, document: RecoveryDocumentId) -> BTreeSet<u64> {
    let mut names = BTreeSet::new();
    if let Ok(entries) = fs::read_dir(store.generations(document)) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str()
                && let Ok(value) = name.parse::<u64>()
            {
                names.insert(value);
            }
        }
    }
    names
}

fn reservation_names(store: &RecoveryStore, document: RecoveryDocumentId) -> BTreeSet<u64> {
    let prefix = format!("{}-", document.directory_name());
    let mut names = BTreeSet::new();
    if let Ok(entries) = fs::read_dir(store.reservations()) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let Some(file_name) = file_name.to_str() else {
                continue;
            };
            let Some(suffix) = file_name.strip_prefix(&prefix) else {
                continue;
            };
            if let Ok(value) = suffix.parse::<u64>() {
                names.insert(value);
            }
        }
    }
    names
}

fn mode(path: &Path) -> Result<u32, Box<dyn Error>> {
    Ok(fs::metadata(path)?.permissions().mode() & 0o777)
}

fn manifest_bytes(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    generation: NonZeroU64,
) -> Result<Vec<u8>, Box<dyn Error>> {
    Ok(fs::read(
        store.generation_dir(document, generation).join("manifest"),
    )?)
}

fn entry_names(path: &Path) -> Result<BTreeSet<String>, Box<dyn Error>> {
    let mut names = BTreeSet::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let name = entry.file_name();
        names.insert(name.to_string_lossy().into_owned());
    }
    Ok(names)
}

#[test]
fn root_rejects_missing_relative_symlink_and_foreign_owner() -> Result<(), Box<dyn Error>> {
    assert_eq!(RECOVERY_DIR, "recovery");
    let home = PathBuf::from("/Users/example");
    assert_eq!(
        default_recovery_root(&home),
        PathBuf::from("/Users/example/Library/Application Support/Aline/recovery")
    );

    let temp = TempRecoveryDir::new()?;
    let relative = Path::new("relative/recovery");
    assert!(matches!(
        RecoveryRoot::open(relative),
        Err(RootError::InvalidPath)
    ));
    assert!(matches!(
        RecoveryRoot::open(&temp.root_path()),
        Err(RootError::Missing)
    ));

    fs::create_dir(temp.root_path())?;
    let symlink_path = temp.path.join("root-link");
    std::os::unix::fs::symlink(temp.root_path(), &symlink_path)?;
    assert!(matches!(
        RecoveryRoot::open(&symlink_path),
        Err(RootError::Symlink)
    ));

    let home_uid = fs::metadata(std::env::temp_dir())?.uid();
    assert!(matches!(
        RecoveryRoot::open_with_expected_uid(&temp.root_path(), home_uid.wrapping_add(1)),
        Err(RootError::ForeignOwner { .. })
    ));
    assert!(
        RecoveryRoot::open_with_expected_uid(&temp.root_path(), home_uid).is_ok(),
        "本用户拥有临时目录"
    );
    Ok(())
}

#[test]
fn store_open_initializes_only_fixed_subdirs_and_syncs() -> Result<(), Box<dyn Error>> {
    assert_eq!(MAX_INCOMPLETE_RESERVATIONS, 32);
    assert_eq!(RECOVERY_INTERVAL_MILLIS, 10_000);
    assert_eq!(RETIRED_MANIFEST_BYTES, 48);
    assert_eq!(NEXT_ID_BYTES, 32);
    assert_eq!(RESERVATION_BYTES, 40);
    assert_eq!(ROOT_PAYLOAD_LIMIT, 1024 * 1024 * 1024);
    assert_eq!(MAX_ACTIVE_LINEAGES, 256);

    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    assert_eq!(
        entry_names(&store.v1())?,
        ["active", "ids", "reservations"]
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>()
    );
    for directory in [store.ids(), store.active(), store.reservations()] {
        assert_eq!(mode(&directory)?, 0o700);
        assert!(entry_names(&directory)?.is_empty());
    }
    assert!(!store.quota_lock_path().exists());

    let document = open_lineage(&store)?;
    assert_eq!(mode(&store.id_dir(document))?, 0o700);
    assert_eq!(mode(&store.active_dir(document))?, 0o700);
    assert_eq!(mode(&store.generations(document))?, 0o700);
    assert_eq!(mode(&store.owner_lock(document))?, 0o600);
    assert_eq!(mode(&store.next_id(document))?, 0o600);
    assert!(store.quota_lock_path().exists());

    let reopened = RecoveryStore::open(
        RecoveryRoot::open(&temp.root_path()).map_err(|error| format!("{error:?}"))?,
    )
    .map_err(|error| format!("{error:?}"))?;
    assert_eq!(generation_names(&reopened, document), BTreeSet::new());
    Ok(())
}

#[test]
fn lineage_allocation_is_exclusive_and_monotonic() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let first = store
        .open_lineage(100)
        .map_err(|error| format!("{error:?}"))?;
    let second = store
        .open_lineage(100)
        .map_err(|error| format!("{error:?}"))?;
    let third = store
        .open_lineage(101)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(first.get().get(), 100);
    assert_eq!(second.get().get(), 101);
    assert_eq!(third.get().get(), 102);
    for document in [first, second, third] {
        assert!(store.id_dir(document).is_dir());
        assert!(store.active_dir(document).is_dir());
        assert_eq!(read_next(&store, document)?.next_generation.get(), 1);
        assert_eq!(read_next(&store, document)?.next_resource_ordinal.get(), 1);
    }
    Ok(())
}

#[test]
fn high_water_advances_before_reservation_and_is_rebuilt_from_disk() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let mut first = sample_input("abc");
    first.resources = vec![resource("a", b"1"), resource("b", b"2")];
    let generation = publish_ok(&store, document, first)?;
    assert_eq!(generation.get(), 1);
    let next = read_next(&store, document)?;
    assert_eq!(next.next_generation.get(), 2);
    assert_eq!(next.next_resource_ordinal.get(), 3);
    assert!(reservation_names(&store, document).is_empty());

    // 模拟 next-id 落后：重建必须从磁盘最大代次/序号恢复，而不是复用。
    write_next(&store, document, NextId::initial())?;
    let mut second = sample_input("abcd");
    second.resources = vec![resource("c", b"3")];
    let generation = publish_ok(&store, document, second)?;
    assert_eq!(generation.get(), 2);
    let bytes = manifest_bytes(&store, document, generation)?;
    match RecoveryRecord::decode(&bytes).map_err(|error| format!("{error:?}"))? {
        RecoveryRecord::Snapshot(snapshot) => {
            assert_eq!(snapshot.generation().get(), 2);
            assert_eq!(snapshot.parent_generation().map(NonZeroU64::get), Some(1));
            let ordinals = snapshot
                .resources()
                .iter()
                .map(|record| record.ordinal().get())
                .collect::<Vec<_>>();
            assert_eq!(ordinals, vec![3]);
        }
        _ => return Err("期望 Snapshot 记录".into()),
    }
    assert_eq!(
        generation_names(&store, document),
        [1, 2].into_iter().collect()
    );
    Ok(())
}

#[test]
fn publish_writes_manifest_last_and_settles_reservation() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let mut input = sample_input("hello");
    input.logical_saved = Some("hello\n".to_owned());
    input.baseline = Some(b"hello\r\n".to_vec());
    input.anchor_byte = 1;
    input.head_byte = 2;
    input.resources = vec![resource("img", b"bytes")];
    let generation = publish_ok(&store, document, input)?;
    assert_eq!(generation.get(), 1);
    assert!(reservation_names(&store, document).is_empty());
    assert_eq!(
        entry_names(&store.generation_dir(document, generation))?,
        [
            "baseline.bytes",
            "logical-saved.utf8",
            "manifest",
            "r",
            "source.utf8"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
    );
    assert_eq!(
        fs::read(
            store
                .generation_dir(document, generation)
                .join("source.utf8")
        )?,
        b"hello"
    );
    assert!(
        store
            .generation_dir(document, generation)
            .join("manifest")
            .is_file()
    );
    let next = read_next(&store, document)?;
    assert_eq!(next.next_generation.get(), 2);
    assert_eq!(next.next_resource_ordinal.get(), 2);
    Ok(())
}

#[test]
fn publish_is_idempotent_after_retry_with_new_generation() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let first = publish_ok(&store, document, sample_input("same"))?;
    let second = publish_ok(&store, document, sample_input("same"))?;
    assert_eq!(first.get(), 1);
    assert_eq!(second.get(), 2);
    assert_eq!(
        fs::read(store.generation_dir(document, first).join("source.utf8"))?,
        b"same"
    );
    assert_eq!(
        fs::read(store.generation_dir(document, second).join("source.utf8"))?,
        b"same"
    );
    let recovered = store
        .recover(document, second)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(recovered.source, "same");
    assert_eq!(recovered.generation.get(), 2);
    assert!(reservation_names(&store, document).is_empty());
    Ok(())
}

#[test]
fn publish_failure_before_manifest_keeps_old_generation_and_reservation()
-> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let first = publish_ok(&store, document, sample_input("old"))?;
    let originals = fs::read(store.generation_dir(document, first).join("source.utf8"))?;

    // 令 g 不可创建：high-water 与 reservation 已推进，但阶段 B 无法建代。
    fs::set_permissions(
        store.generations(document),
        fs::Permissions::from_mode(0o500),
    )?;
    let cancellation = Cancellation::new();
    let outcome = store
        .publish(document, sample_input("new"), &cancellation)
        .map_err(|error| format!("{error:?}"))?;
    fs::set_permissions(
        store.generations(document),
        fs::Permissions::from_mode(0o700),
    )?;
    match outcome {
        RecoveryPublishOutcome::NotPublished { generation, cause } => {
            assert_eq!(generation.get(), 2);
            assert!(matches!(cause, RecoveryPublishCause::Uncertain { .. }));
        }
        other => return Err(format!("期望 NotPublished，实际 {other:?}").into()),
    }
    assert_eq!(
        generation_names(&store, document),
        [1].into_iter().collect()
    );
    assert_eq!(
        reservation_names(&store, document),
        [2].into_iter().collect()
    );
    assert_eq!(read_next(&store, document)?.next_generation.get(), 3);
    assert_eq!(
        fs::read(store.generation_dir(document, first).join("source.utf8"))?,
        originals
    );

    let retried = publish_ok(&store, document, sample_input("new"))?;
    assert_eq!(retried.get(), 3);
    assert_eq!(
        reservation_names(&store, document),
        [2].into_iter().collect()
    );
    // 无 generation 目录的 reservation 是已确认 V1 半成品；扫描回收它。
    store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(reservation_names(&store, document).is_empty());
    Ok(())
}

#[test]
fn publish_cancel_after_reservation_leaves_no_manifest() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let outcome = publish_with_cancellation(&store, document, sample_input("cancel"))?;
    match outcome {
        RecoveryPublishOutcome::NotPublished { generation, cause } => {
            assert_eq!(generation.get(), 1);
            assert!(matches!(cause, RecoveryPublishCause::Cancelled));
        }
        other => return Err(format!("期望 NotPublished，实际 {other:?}").into()),
    }
    let first_generation = first_generation()?;
    assert!(store.generation_dir(document, first_generation).is_dir());
    assert!(
        !store
            .generation_dir(document, first_generation)
            .join("manifest")
            .exists()
    );
    assert_eq!(
        reservation_names(&store, document),
        [1].into_iter().collect()
    );
    assert_eq!(read_next(&store, document)?.next_generation.get(), 2);

    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(
        scan.lineages
            .iter()
            .any(|lineage| lineage.document == document)
    );
    assert!(generation_names(&store, document).is_empty());
    assert!(reservation_names(&store, document).is_empty());

    let generation = publish_ok(&store, document, sample_input("cancel"))?;
    assert_eq!(generation.get(), 2);
    Ok(())
}

#[test]
fn publish_syncs_files_then_generation_then_g_parent_then_active_then_root()
-> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let first = publish_ok(&store, document, sample_input("sync"))?;
    for directory in [
        store.generation_dir(document, first),
        store.generations(document),
        store.active_dir(document),
        store.active(),
        store.v1(),
        temp.root_path(),
    ] {
        assert!(directory.is_dir());
        assert_eq!(mode(&directory)? & 0o700, 0o700);
    }
    assert!(
        store
            .generation_dir(document, first)
            .join("manifest")
            .is_file()
    );
    let second = publish_ok(&store, document, sample_input("sync-2"))?;
    assert_eq!(second.get(), 2);
    assert_eq!(
        generation_names(&store, document),
        [1, 2].into_iter().collect()
    );
    assert!(reservation_names(&store, document).is_empty());
    Ok(())
}

#[test]
fn budget_rejects_manifest_over_one_mib_and_generation_resources_over_256_mib()
-> Result<(), Box<dyn Error>> {
    assert_eq!(MAX_MANIFEST_BYTES, 1024 * 1024);
    assert_eq!(MAX_GENERATION_RESOURCE_BYTES, 256 * 1024 * 1024);
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;

    let mut manifest_input = sample_input("x");
    manifest_input.resources = (0..256)
        .map(|index| {
            let reference = format!("{index:0>4096}");
            RecoveryResourceInput {
                reference,
                bytes: Vec::new(),
            }
        })
        .collect();
    let cancellation = Cancellation::new();
    let error = store
        .publish(document, manifest_input, &cancellation)
        .err()
        .ok_or("超 1 MiB 的 manifest 必须被拒绝")?;
    assert!(
        matches!(error, RecoveryStoreError::Budget(RecoveryBudget::Manifest)),
        "实际 {error:?}"
    );

    let mut resources = sample_input("x");
    resources.resources = (0..5)
        .map(ResourceInputBuilder::large)
        .map(|builder| builder.build())
        .collect();
    let error = store
        .publish(document, resources, &cancellation)
        .err()
        .ok_or("超过 256 MiB 的资源合计必须被拒绝")?;
    assert!(
        matches!(
            error,
            RecoveryStoreError::Budget(RecoveryBudget::GenerationResources)
        ),
        "实际 {error:?}"
    );
    Ok(())
}

struct ResourceInputBuilder {
    index: u32,
}

impl ResourceInputBuilder {
    fn large(index: u32) -> Self {
        Self { index }
    }

    fn build(self) -> RecoveryResourceInput {
        RecoveryResourceInput {
            reference: format!("large-{}", self.index),
            bytes: vec![0_u8; MAX_RESOURCE_BYTES],
        }
    }
}

#[test]
fn budget_rejects_over_256_resources_and_oversized_resource() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let cancellation = Cancellation::new();

    let mut too_many = sample_input("x");
    too_many.resources = (0..257)
        .map(|index| resource(&format!("r{index}"), b"x"))
        .collect();
    let error = store
        .publish(document, too_many, &cancellation)
        .err()
        .ok_or("257 条资源必须被拒绝")?;
    assert!(matches!(
        error,
        RecoveryStoreError::Budget(RecoveryBudget::Resources)
    ));

    let mut oversized = sample_input("x");
    oversized.resources = vec![RecoveryResourceInput {
        reference: "too-big".to_owned(),
        bytes: vec![0_u8; MAX_RESOURCE_BYTES + 1],
    }];
    let error = store
        .publish(document, oversized, &cancellation)
        .err()
        .ok_or("超过 64 MiB 的单条资源必须被拒绝")?;
    assert!(matches!(
        error,
        RecoveryStoreError::Budget(RecoveryBudget::Resources)
    ));
    Ok(())
}

#[test]
fn quota_counts_hard_link_once_and_rejects_over_one_gib() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let original = store.v1().join("payload.bin");
    let linked = store.v1().join("payload-link.bin");
    fs::write(&original, b"0123456789")?;
    fs::hard_link(&original, &linked)?;
    let meter = store.meter_v1().map_err(|error| format!("{error:?}"))?;
    assert_eq!(meter.total, 10);

    let limited = RecoveryStore::open(
        RecoveryRoot::open(&temp.root_path()).map_err(|error| format!("{error:?}"))?,
    )
    .map_err(|error| format!("{error:?}"))?
    .with_root_payload_limit(2 * 1024 * 1024);
    let document = limited
        .open_lineage(500)
        .map_err(|error| format!("{error:?}"))?;
    let mut too_large = sample_input("");
    too_large.source = "x".repeat(MAX_TEXT_BYTES.min(2 * 1024 * 1024));
    let cancellation = Cancellation::new();
    let error = limited
        .publish(document, too_large, &cancellation)
        .err()
        .ok_or("注入的 2 MiB 根上限必须拒绝")?;
    assert!(matches!(
        error,
        RecoveryStoreError::Budget(RecoveryBudget::RootPayload)
    ));
    Ok(())
}

#[test]
fn quota_counts_unsettled_reservation_at_max_of_promise_and_actual() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let limited = RecoveryStore::open(temp.create_root()?)
        .map_err(|error| format!("{error:?}"))?
        .with_root_payload_limit(2 * 1024 * 1024);
    let document = limited
        .open_lineage(700)
        .map_err(|error| format!("{error:?}"))?;
    // 只有 reservation 承诺、没有代次文件：容量必须按承诺而非实际 0 bytes 计入。
    write_reservation(&limited, document, 1, 1_500_000)?;
    let total = limited
        .quota_total()
        .map_err(|error| format!("{error:?}"))?;
    assert!(total >= 1_500_000, "未兑现承诺缺失：{total}");
    let cancellation = Cancellation::new();
    let error = limited
        .publish(document, sample_input("x"), &cancellation)
        .err()
        .ok_or("未兑现承诺必须占用配额")?;
    assert!(matches!(
        error,
        RecoveryStoreError::Budget(RecoveryBudget::RootPayload)
    ));
    assert_eq!(
        reservation_names(&limited, document),
        [1].into_iter().collect()
    );
    Ok(())
}

#[test]
fn quota_syncs_visible_dirs_before_metering() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    fs::write(store.v1().join("visible.bin"), b"12345")?;
    store
        .sync_quota_scope(None)
        .map_err(|error| format!("{error:?}"))?;
    let meter = store.meter_v1().map_err(|error| format!("{error:?}"))?;
    assert_eq!(meter.total, 5);
    let document = open_lineage(&store)?;
    assert_eq!(generation_names(&store, document), BTreeSet::new());
    assert!(store.quota_lock_path().is_file());
    Ok(())
}

#[test]
fn trash_move_then_delete_is_the_only_capacity_release() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    publish_ok(&store, document, sample_input("one"))?;
    publish_ok(&store, document, sample_input("two"))?;
    assert_eq!(
        generation_names(&store, document),
        [1, 2].into_iter().collect()
    );
    publish_ok(&store, document, sample_input("three"))?;
    assert_eq!(
        generation_names(&store, document),
        [2, 3].into_iter().collect()
    );
    assert!(store.trash(document).is_dir());
    assert!(fs::read_dir(store.trash(document))?.next().is_none());
    let generator = store
        .recover(document, second_generation()?)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(generator.source, "two");
    Ok(())
}

#[test]
fn retire_advances_high_water_then_publishes_marker_and_hides_generations()
-> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    publish_ok(&store, document, sample_input("one"))?;
    publish_ok(&store, document, sample_input("two"))?;
    let outcome = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    match outcome {
        RetireOutcome::Retired { generation } => assert_eq!(generation.get(), 3),
        other => return Err(format!("期望 Retired，实际 {other:?}").into()),
    }
    assert!(!store.active_dir(document).exists());
    assert!(!store.next_id(document).exists());
    assert!(!store.generations(document).exists());
    assert!(store.id_dir(document).is_dir());
    assert!(store.owner_lock(document).is_file());
    let marker = fs::read(store.retired_path(document))?;
    match RecoveryRecord::decode(&marker).map_err(|error| format!("{error:?}"))? {
        RecoveryRecord::Retired(retired) => {
            assert_eq!(retired.document_id(), document.get());
            assert_eq!(retired.generation().get(), 3);
            assert_eq!(retired.retired_through_generation(), 2);
        }
        _ => return Err("期望 Retired marker".into()),
    }
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty());
    assert!(matches!(
        store.recover(document, second_generation()?),
        Err(RecoveryReadError::Retired { .. })
    ));
    assert!(matches!(
        store.publish(document, sample_input("x"), &Cancellation::new()),
        Err(RecoveryStoreError::Retired { .. })
    ));
    Ok(())
}

#[test]
fn retire_is_terminal_and_id_is_never_reused() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = store
        .open_lineage(100)
        .map_err(|error| format!("{error:?}"))?;
    publish_ok(&store, document, sample_input("one"))?;
    let first = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(first, RetireOutcome::Retired { .. }));
    let second = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    match second {
        RetireOutcome::AlreadyRetired { through } => assert_eq!(through, 1),
        other => return Err(format!("期望 AlreadyRetired，实际 {other:?}").into()),
    }
    let replacement = store
        .open_lineage(100)
        .map_err(|error| format!("{error:?}"))?;
    assert_ne!(replacement, document);
    assert_eq!(replacement.get().get(), 101);
    assert!(store.id_dir(document).is_dir());
    assert!(store.retired_path(document).is_file());
    Ok(())
}

#[test]
fn scan_classifies_complete_corrupt_unknown_partial_and_retired() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let first = publish_ok(&store, document, sample_input("one"))?;
    let second = publish_ok(&store, document, sample_input("two"))?;

    let manifest_path = store.generation_dir(document, second).join("manifest");
    let mut corrupt = fs::read(&manifest_path)?;
    if let Some(byte) = corrupt.get_mut(30) {
        *byte ^= 0xff;
    }
    fs::write(&manifest_path, &corrupt)?;

    let third = store.generation_dir(document, NonZeroU64::new(3).ok_or("3")?);
    fs::create_dir(&third)?;
    fs::write(third.join("manifest"), b"NOTALINEREC-unknown")?;
    let fourth = store.generation_dir(document, NonZeroU64::new(4).ok_or("4")?);
    fs::create_dir(&fourth)?;
    fs::write(fourth.join("source.utf8"), b"partial")?;
    write_next(
        &store,
        document,
        NextId {
            next_generation: NonZeroU64::new(5).ok_or("5")?,
            next_resource_ordinal: NonZeroU64::MIN,
        },
    )?;

    let retired_document = store
        .open_lineage(900)
        .map_err(|error| format!("{error:?}"))?;
    publish_ok(&store, retired_document, sample_input("retired"))?;
    store
        .discard(retired_document)
        .map_err(|error| format!("{error:?}"))?;

    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(
        scan.lineages
            .iter()
            .all(|lineage| lineage.document != retired_document),
        "退役谱系不出现在 active 枚举中"
    );
    let lineage = scan
        .lineages
        .iter()
        .find(|lineage| lineage.document == document)
        .ok_or("扫描缺少谱系")?;
    assert_eq!(lineage.disposition, RecoveryLineageDisposition::Blocked);
    let statuses = lineage
        .generations
        .iter()
        .map(|generation| (generation.generation.get(), generation.status))
        .collect::<Vec<_>>();
    assert_eq!(
        statuses,
        vec![
            (first.get(), RecoveryGenerationStatus::Complete),
            (second.get(), RecoveryGenerationStatus::Corrupt),
            (3, RecoveryGenerationStatus::UnknownFormat),
            (4, RecoveryGenerationStatus::Incomplete),
        ]
    );
    Ok(())
}

#[test]
fn scan_reclaims_only_confirmed_v1_half_products() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    publish_ok(&store, document, sample_input("complete"))?;
    let outcome = publish_with_cancellation(&store, document, sample_input("half"))?;
    match outcome {
        RecoveryPublishOutcome::NotPublished { cause, .. } => {
            assert!(matches!(cause, RecoveryPublishCause::Cancelled));
        }
        other => return Err(format!("期望 NotPublished，实际 {other:?}").into()),
    }
    let unknown = store.generation_dir(document, NonZeroU64::new(3).ok_or("3")?);
    fs::create_dir(&unknown)?;
    fs::write(unknown.join("notes.txt"), b"unknown")?;
    let reservation = Reservation {
        document: document.get(),
        generation: NonZeroU64::new(3).ok_or("3")?,
        reserved_bytes: 0,
    };
    fs::write(
        store.reservation_path(document, NonZeroU64::new(3).ok_or("3")?),
        reservation.encode(),
    )?;
    write_next(
        &store,
        document,
        NextId {
            next_generation: NonZeroU64::new(4).ok_or("4")?,
            next_resource_ordinal: NonZeroU64::MIN,
        },
    )?;

    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    let lineage = scan
        .lineages
        .iter()
        .find(|lineage| lineage.document == document)
        .ok_or("扫描缺少谱系")?;
    assert_eq!(lineage.disposition, RecoveryLineageDisposition::Blocked);
    assert_eq!(
        generation_names(&store, document),
        [1, 3].into_iter().collect()
    );
    assert_eq!(
        reservation_names(&store, document),
        [3].into_iter().collect()
    );
    assert!(unknown.join("notes.txt").is_file());
    let statuses = lineage
        .generations
        .iter()
        .map(|generation| (generation.generation.get(), generation.status))
        .collect::<Vec<_>>();
    assert!(statuses.contains(&(3, RecoveryGenerationStatus::UnknownFormat)));
    Ok(())
}

#[test]
fn scan_reports_bad_retired_marker_without_hiding_generations() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let generation = publish_ok(&store, document, sample_input("kept"))?;
    let marker = b"NOTARETD-but-still-a-file".to_vec();
    fs::write(store.retired_path(document), &marker)?;
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    let lineage = scan
        .lineages
        .iter()
        .find(|lineage| lineage.document == document)
        .ok_or("扫描缺少谱系")?;
    assert_eq!(lineage.disposition, RecoveryLineageDisposition::Blocked);
    assert_eq!(lineage.retired_through, None);
    assert_eq!(
        lineage.generations,
        vec![super::scan::RecoveryGenerationScan {
            generation,
            status: RecoveryGenerationStatus::Complete,
            payload_bytes: lineage.generations[0].payload_bytes,
        }]
    );
    assert!(lineage.defects.iter().any(|defect| matches!(
        defect.defect,
        RecoveryDefect::BadRetiredMarker | RecoveryDefect::RecordUnknownFormat
    )));
    assert_eq!(fs::read(store.retired_path(document))?, marker);
    Ok(())
}

#[test]
fn recover_materializes_snapshot_without_touching_source_bytes() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let mut input = sample_input("abc");
    input.path_generation = 9;
    input.captured_composition = true;
    input.path = RecoveryPath::Unix(b"notes.md".to_vec());
    input.anchor_byte = 1;
    input.head_byte = 1;
    input.logical_saved = Some("a\nb".to_owned());
    input.baseline = Some(b"a\r\nb".to_vec());
    input.resources = vec![resource("image", b"png")];
    let source_path = temp.path.join("notes.md");
    fs::write(&source_path, "abc")?;
    let before = fs::metadata(&source_path)?;
    let generation = publish_ok(&store, document, input)?;

    let recovered = store
        .recover(document, generation)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(recovered.document, document);
    assert_eq!(recovered.generation, generation);
    assert_eq!(recovered.path_generation, 9);
    assert!(recovered.captured_composition);
    match recovered.path {
        RecoveredPath::Unix(bytes) => assert_eq!(bytes, b"notes.md"),
        other => return Err(format!("期望 Unix 路径，实际 {other:?}").into()),
    }
    assert_eq!(recovered.anchor_byte, 1);
    assert_eq!(recovered.head_byte, 1);
    assert_eq!(recovered.source, "abc");
    assert_eq!(recovered.logical_saved.as_deref(), Some("a\nb"));
    assert_eq!(recovered.baseline.as_deref(), Some(&b"a\r\nb"[..]));
    assert_eq!(recovered.resources.len(), 1);
    assert_eq!(recovered.resources[0].reference, "image");
    assert_eq!(recovered.resources[0].bytes, b"png");

    let mut windows = sample_input("win");
    windows.path = RecoveryPath::WindowsUtf16Le(vec![b'n', 0, b'o', 0]);
    let windows_generation = publish_ok(&store, document, windows)?;
    let recovered = store
        .recover(document, windows_generation)
        .map_err(|error| format!("{error:?}"))?;
    match recovered.path {
        RecoveredPath::WindowsUtf16Le(bytes) => assert_eq!(bytes, vec![b'n', 0, b'o', 0]),
        other => return Err(format!("期望 UTF-16 路径，实际 {other:?}").into()),
    }

    let after = fs::metadata(&source_path)?;
    assert_eq!(fs::read(&source_path)?, b"abc");
    assert_eq!(after.ino(), before.ino());
    assert_eq!(after.len(), before.len());
    Ok(())
}

#[test]
fn recover_rejects_corrupt_or_unknown_without_cleanup() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let generation = publish_ok(&store, document, sample_input("payload"))?;
    let source_path = store
        .generation_dir(document, generation)
        .join("source.utf8");
    let manifest_path = store.generation_dir(document, generation).join("manifest");
    let original_source = fs::read(&source_path)?;
    let original_manifest = fs::read(&manifest_path)?;

    let mut corrupted = original_source.clone();
    corrupted.push(b'!');
    fs::write(&source_path, &corrupted)?;
    assert!(matches!(
        store.recover(document, generation),
        Err(RecoveryReadError::Corrupt { .. })
    ));
    assert_eq!(fs::read(&source_path)?, corrupted);
    assert_eq!(fs::read(&manifest_path)?, original_manifest);
    assert_eq!(reservation_names(&store, document), BTreeSet::new());

    fs::write(&source_path, &original_source)?;
    let unknown = b"NOTALINEREC-manifest".to_vec();
    fs::write(&manifest_path, &unknown)?;
    assert!(matches!(
        store.recover(document, generation),
        Err(RecoveryReadError::UnknownFormat { .. })
    ));
    assert_eq!(fs::read(&manifest_path)?, unknown);
    assert_eq!(fs::read(&source_path)?, original_source);
    assert!(store.generation_dir(document, generation).is_dir());
    Ok(())
}

#[test]
fn discard_is_idempotent_and_keeps_tombstone() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    publish_ok(&store, document, sample_input("one"))?;
    let first = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(first, RetireOutcome::Retired { .. }));
    let marker = fs::read(store.retired_path(document))?;
    let second = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(
        second,
        RetireOutcome::AlreadyRetired { through: 1 }
    ));
    assert_eq!(fs::read(store.retired_path(document))?, marker);
    assert!(store.id_dir(document).is_dir());
    assert!(store.owner_lock(document).is_file());
    Ok(())
}

#[test]
fn partial_write_then_retry_never_reuses_generation_or_ordinal() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let mut first = sample_input("one");
    first.resources = vec![resource("a", b"1")];
    let first_generation = publish_ok(&store, document, first)?;

    let mut partial = sample_input("partial");
    partial.resources = vec![resource("b", b"2"), resource("c", b"3")];
    match publish_with_cancellation(&store, document, partial)? {
        RecoveryPublishOutcome::NotPublished { generation, .. } => {
            assert_eq!(generation.get(), 2);
        }
        other => return Err(format!("期望 NotPublished，实际 {other:?}").into()),
    }
    assert_eq!(read_next(&store, document)?.next_generation.get(), 3);
    assert_eq!(read_next(&store, document)?.next_resource_ordinal.get(), 4);

    let mut retry = sample_input("retry");
    retry.resources = vec![resource("d", b"4")];
    let generation = publish_ok(&store, document, retry)?;
    assert_eq!(generation.get(), 3);
    let bytes = manifest_bytes(&store, document, generation)?;
    match RecoveryRecord::decode(&bytes).map_err(|error| format!("{error:?}"))? {
        RecoveryRecord::Snapshot(snapshot) => {
            let ordinals = snapshot
                .resources()
                .iter()
                .map(|record| record.ordinal().get())
                .collect::<Vec<_>>();
            assert_eq!(ordinals, vec![4]);
            assert_eq!(
                snapshot.parent_generation().map(NonZeroU64::get),
                Some(first_generation.get())
            );
        }
        _ => return Err("期望 Snapshot 记录".into()),
    }

    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(
        scan.lineages
            .iter()
            .any(|lineage| lineage.document == document)
    );
    assert_eq!(
        generation_names(&store, document),
        [1, 3].into_iter().collect()
    );
    assert!(reservation_names(&store, document).is_empty());
    Ok(())
}

#[test]
fn source_file_bytes_unchanged_after_publish_scan_recover_and_discard() -> Result<(), Box<dyn Error>>
{
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let source_path = temp.path.join("source.md");
    fs::write(&source_path, b"# title\nbody\n")?;
    let before = fs::metadata(&source_path)?;
    let before_bytes = fs::read(&source_path)?;

    let mut input = sample_input("# title\nbody\n");
    input.path = RecoveryPath::from_os_path(&source_path);
    let generation = publish_ok(&store, document, input)?;
    store.scan().map_err(|error| format!("{error:?}"))?;
    let recovered = store
        .recover(document, generation)
        .map_err(|error| format!("{error:?}"))?;
    assert_eq!(recovered.source, "# title\nbody\n");
    match recovered.path {
        RecoveredPath::Unix(bytes) => assert_eq!(bytes, source_path.as_os_str().as_encoded_bytes()),
        other => return Err(format!("期望 Unix 路径，实际 {other:?}").into()),
    }
    store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;

    let after = fs::metadata(&source_path)?;
    assert_eq!(fs::read(&source_path)?, before_bytes);
    assert_eq!(after.ino(), before.ino());
    assert_eq!(after.dev(), before.dev());
    assert_eq!(after.len(), before.len());
    Ok(())
}

#[test]
fn publish_and_scan_report_owner_lock_busy() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    publish_ok(&store, document, sample_input("one"))?;
    let held = store
        .hold_owner_lock_for_test(document)
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(
        store.publish(document, sample_input("two"), &Cancellation::new()),
        Err(RecoveryStoreError::LockBusy {
            lock: RecoveryLock::Owner
        })
    ));
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    let lineage = scan
        .lineages
        .iter()
        .find(|lineage| lineage.document == document)
        .ok_or("扫描缺少谱系")?;
    assert_eq!(lineage.disposition, RecoveryLineageDisposition::Blocked);
    assert!(
        lineage
            .defects
            .iter()
            .any(|defect| defect.defect == RecoveryDefect::Locked)
    );
    drop(held);
    let generation = publish_ok(&store, document, sample_input("two"))?;
    assert_eq!(generation.get(), 2);
    Ok(())
}

fn second_generation() -> Result<NonZeroU64, Box<dyn Error>> {
    Ok(NonZeroU64::new(2).ok_or("2")?)
}

fn first_generation() -> Result<NonZeroU64, Box<dyn Error>> {
    Ok(NonZeroU64::new(1).ok_or("1")?)
}

// ---- 4.1b 会话协调器：准备、发布、跳过、取消与关闭退役 ----

/// 读取器适配：只提供调用方给定的字节，不触碰真实文件。
struct Files<'a>(&'a [u8]);
impl<'a> ReadonlyFileAccess for Files<'a> {
    type Reader = Cursor<&'a [u8]>;
    fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        Ok(ReadonlyFileInput::new(
            Cursor::new(self.0),
            self.0.len() as u64,
            true,
        ))
    }
}

/// 恢复测试的打开预算；全部为测试注入值，不声明发布阈值。
fn recovery_limits(editable: bool) -> ReadonlyOpenLimits {
    let bytes = 64 * 1024;
    let limits = ReadonlyOpenLimits::new(
        StorageBudget::new(bytes),
        DocLimits::new(
            TextBudget::new(bytes),
            LineIndexBudget::new(1024),
            HistoryBudget::new(32, bytes),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    );
    if editable {
        limits.with_editing()
    } else {
        limits
    }
}

fn recovery_config(
    store: &RecoveryStore,
    resources: Arc<dyn RecoveryResourceSource>,
) -> RecoveryConfig {
    RecoveryConfig {
        store: store.clone(),
        resources,
    }
}

/// 以给定字节打开一个可编辑/只读恢复会话；恢复配置在打开前注入。
fn open_recovery_session(
    store: &RecoveryStore,
    resources: Arc<dyn RecoveryResourceSource>,
    path: &Path,
    bytes: &[u8],
    editable: bool,
) -> Result<ReadonlySession, Box<dyn Error>> {
    let mut session = ReadonlySession::new(recovery_limits(editable));
    session
        .configure_recovery(Some(recovery_config(store, resources)))
        .map_err(|error| format!("{error:?}"))?;
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![path.to_path_buf()]))?
        .ok_or("应生成打开工作")?;
    if !matches!(
        session.accept_open(job.read_with(&Files(bytes))),
        OpenDisposition::Applied
    ) {
        return Err("打开应成功应用".into());
    }
    Ok(session)
}

/// 经本地真实读取器打开恢复会话；源文件必须已存在。
fn open_recovery_session_real(
    store: &RecoveryStore,
    resources: Arc<dyn RecoveryResourceSource>,
    path: &Path,
) -> Result<ReadonlySession, Box<dyn Error>> {
    let mut session = ReadonlySession::new(recovery_limits(true));
    session
        .configure_recovery(Some(recovery_config(store, resources)))
        .map_err(|error| format!("{error:?}"))?;
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![path.to_path_buf()]))?
        .ok_or("应生成打开工作")?;
    if !matches!(session.accept_open(job.run()), OpenDisposition::Applied) {
        return Err("打开应成功应用".into());
    }
    Ok(session)
}

/// 在全文范围打开旧式输入块；恢复测试需要可编辑草稿或组合。
fn open_recovery_block(document: &mut ReadonlyDocument) -> Result<(), Box<dyn Error>> {
    let len = document.baseline().source().len();
    document
        .edit_core()?
        .open_block(0..len, SelectionSpec::new(0, 0))?;
    Ok(())
}

fn recovery_type(
    document: &mut ReadonlyDocument,
    at: usize,
    text: &str,
) -> Result<(), Box<dyn Error>> {
    document.edit_core()?.type_text(at, text, 0)?;
    Ok(())
}

/// 驱动一次完整的准备→发布→结算；返回最终反馈键。
fn run_recovery(
    document: &mut ReadonlyDocument,
    now: u64,
) -> Result<Option<&'static str>, Box<dyn Error>> {
    let start = document.recovery_start(now).ok_or("恢复会话应可开始")?;
    let job = match start {
        RecoveryStart::Prepare(job) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    let prepared = job.run();
    match document.recovery_prepared(prepared, now) {
        Some(RecoveryStart::Publish(job)) => {
            let settlement = job.run();
            Ok(document.recovery_settled(settlement, now))
        }
        Some(RecoveryStart::Settled { feedback }) => Ok(feedback),
        Some(RecoveryStart::Prepare(_)) => Err("准备结果不应再次准备".into()),
        Some(RecoveryStart::Retire(_)) => Err("准备结果不应直接退役".into()),
        None => Err("准备结果应被当前会话采纳".into()),
    }
}

fn sole_lineage(store: &RecoveryStore) -> Result<RecoveryDocumentId, Box<dyn Error>> {
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    let mut lineages = scan.lineages;
    if lineages.len() != 1 {
        return Err(format!("期望 1 个谱系，实际 {}", lineages.len()).into());
    }
    Ok(lineages.remove(0).document)
}

fn complete_generations(store: &RecoveryStore) -> Result<Vec<NonZeroU64>, Box<dyn Error>> {
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    let mut generations = Vec::new();
    for lineage in scan.lineages {
        for generation in lineage.generations {
            if generation.status == RecoveryGenerationStatus::Complete {
                generations.push(generation.generation);
            }
        }
    }
    Ok(generations)
}

/// 测试资源源：按引用键返回给定分类，未知引用一律不可用。
enum FakeResource {
    /// 伪资源保留的未用分类，便于扩展读取失败路径。
    #[allow(dead_code)]
    Bytes(Vec<u8>),
    Unavailable,
    Corrupt,
    /// 伪资源保留的未用分类，配合 `Bytes` 覆盖超限分支。
    #[allow(dead_code)]
    TooLarge,
}

struct FakeResources {
    entries: BTreeMap<String, FakeResource>,
}

impl FakeResources {
    fn new(entries: impl IntoIterator<Item = (String, FakeResource)>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }
}

impl RecoveryResourceSource for FakeResources {
    fn read(&self, reference: &str, budget: usize) -> Result<Vec<u8>, RecoveryResourceError> {
        match self.entries.get(reference) {
            Some(FakeResource::Bytes(bytes)) if bytes.len() <= budget => Ok(bytes.clone()),
            Some(FakeResource::Bytes(_) | FakeResource::TooLarge) => {
                Err(RecoveryResourceError::TooLarge)
            }
            Some(FakeResource::Corrupt) => Err(RecoveryResourceError::Corrupt),
            Some(FakeResource::Unavailable) | None => Err(RecoveryResourceError::Unavailable),
        }
    }
}

#[test]
fn recovery_session_skips_clean_document_without_lineage() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("clean.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"alpha\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    // 清理判定在后台准备任务内完成（W4-54），因此由准备结果给出静默跳过。
    let start = document.recovery_start(0).ok_or("恢复会话应可开始")?;
    let job = match start {
        RecoveryStart::Prepare(job) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    let prepared = job.run();
    assert!(matches!(
        &prepared,
        PreparedRecovery::Skipped {
            skip: RecoverySkip::Clean,
            ..
        }
    ));
    match document.recovery_prepared(prepared, 0) {
        Some(RecoveryStart::Settled { feedback }) => assert_eq!(feedback, None),
        other => return Err(format!("期望静默结算，实际 {other:?}").into()),
    }
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty(), "未分配谱系");
    Ok(())
}

#[test]
fn recovery_session_publishes_untitled_document_without_path() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("untitled.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    document.detach_persistence_for_test();
    open_recovery_block(document)?;
    recovery_type(document, 0, "x")?;
    assert_eq!(run_recovery(document, 0)?, None);

    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert_eq!(scan.lineages.len(), 1);
    let lineage = &scan.lineages[0];
    assert_eq!(lineage.disposition, RecoveryLineageDisposition::Active);
    let generation = lineage
        .generations
        .iter()
        .find(|generation| generation.status == RecoveryGenerationStatus::Complete)
        .ok_or("应存在完整代")?
        .generation;
    let recovered = store
        .recover(lineage.document, generation)
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(recovered.path, RecoveredPath::Unlinked));
    assert_eq!(recovered.path_generation, 0);
    assert!(!recovered.captured_composition);
    assert_eq!(recovered.source, "xabc\n");
    Ok(())
}

#[test]
fn recovery_session_skips_unchanged_version_and_snapshot() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("unchanged.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    assert_eq!(run_recovery(document, 0)?, None);
    assert_eq!(complete_generations(&store)?.len(), 1);

    match document.recovery_start(1) {
        Some(RecoveryStart::Settled { feedback }) => assert_eq!(feedback, None),
        other => return Err(format!("期望未变化跳过，实际 {other:?}").into()),
    }
    assert_eq!(complete_generations(&store)?.len(), 1);
    Ok(())
}

#[test]
fn recovery_session_records_composition_candidates() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("composing.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    document.edit_core()?.composition_start("候", 0)?;
    // 组合活动不延后准备：同一轮即可准备并发布候选文本。
    assert_eq!(run_recovery(document, 0)?, None);

    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    let lineage = scan.lineages.first().ok_or("应存在谱系")?;
    let generation = lineage
        .generations
        .iter()
        .find(|generation| generation.status == RecoveryGenerationStatus::Complete)
        .ok_or("应存在完整代")?
        .generation;
    let recovered = store
        .recover(lineage.document, generation)
        .map_err(|error| format!("{error:?}"))?;
    assert!(recovered.captured_composition);
    assert!(
        recovered.source.contains("候"),
        "恢复文本应包含组合候选：{}",
        recovered.source
    );
    Ok(())
}

#[test]
fn recovery_session_discards_prepared_when_version_changes() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("changing.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    let mut retained = None;
    // 连续 3 次在准备冻结后、采纳前继续输入；每次都必须复用同一谱系，不得增长。
    for round in 0..3 {
        let start = document.recovery_start(0).ok_or("恢复会话应可开始")?;
        let job = match start {
            RecoveryStart::Prepare(job) => job,
            other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
        };
        recovery_type(document, 1, "b")?;
        let prepared = job.run();
        match document.recovery_prepared(prepared, 0) {
            Some(RecoveryStart::Settled { feedback }) => assert_eq!(feedback, None),
            other => return Err(format!("期望版本变化结算，实际 {other:?}").into()),
        }
        let scan = store.scan().map_err(|error| format!("{error:?}"))?;
        assert!(
            scan.lineages.len() <= 1,
            "第 {round} 次版本变化后谱系不得增长，实际 {}",
            scan.lineages.len()
        );
        let current = sole_lineage(&store)?;
        if let Some(first) = retained {
            assert_eq!(current, first, "版本变化后应复用同一谱系");
        } else {
            retained = Some(current);
        }
    }
    assert!(complete_generations(&store)?.is_empty());
    Ok(())
}

#[test]
fn recovery_session_retires_stale_prepared_lineage() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("stale.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    let start = document.recovery_start(0).ok_or("恢复会话应可开始")?;
    let job = match start {
        RecoveryStart::Prepare(job) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    // 模拟旧尝试的迟到完成：标签仍属同一会话，但尝试号已不属于当前在途尝试。
    let prepared = match job.run() {
        PreparedRecovery::Ready(mut ready) => {
            ready.tag.attempt = ready.tag.attempt.saturating_add(1);
            PreparedRecovery::Ready(ready)
        }
        _ => return Err("期望已分配谱系的准备结果".into()),
    };
    match document.recovery_prepared(prepared, 0) {
        Some(RecoveryStart::Retire(job)) => {
            job.run();
        }
        _ => return Err("迟到准备应返回后台退役任务".into()),
    }
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty(), "迟到准备的谱系应被退役");
    Ok(())
}

#[test]
fn recovery_close_with_in_flight_prepare_leaves_no_lineage() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("closing.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    let start = document.recovery_start(0).ok_or("恢复会话应可开始")?;
    let job = match start {
        RecoveryStart::Prepare(job) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    // 关闭发生在后台准备完成前：会话尚未持有谱系，取消由准备任务自行收尾。
    let close = document.take_recovery_close();
    assert!(close.is_none(), "谱系尚未分配时无退役任务");
    let prepared = job.run();
    assert!(matches!(&prepared, PreparedRecovery::Cancelled { .. }));
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty(), "关闭后在途准备不得留下活动谱系");
    Ok(())
}

#[test]
fn recovery_close_retires_prepared_completed_before_callback() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("late.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    let start = document.recovery_start(0).ok_or("恢复会话应可开始")?;
    let job = match start {
        RecoveryStart::Prepare(job) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    // 后台准备已完成、完成回调尚未处理；此时关闭应仍能退役已分配的谱系。
    let prepared = job.run();
    let close = document.take_recovery_close();
    assert!(close.is_none(), "未采纳的准备尚未记入会话谱系");
    let close = prepared
        .into_close_job()
        .ok_or("已分配谱系的准备结果应可退役")?;
    close.run();
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty(), "关闭后迟到准备的谱系应被退役");
    Ok(())
}

#[test]
fn recovery_session_reports_incomplete_for_unreadable_resource() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("missing-resource.md");
    let resources = Arc::new(FakeResources::new([(
        "img.png".to_owned(),
        FakeResource::Unavailable,
    )]));
    let mut session = open_recovery_session(&store, resources, &path, b"abc\n", true)?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "x")?;
    let identity = document.input_identity()?;
    let reference = ResourceReference::new(document.core().document_id().clone(), "img.png");
    document
        .edit_core()?
        .retain_resource(&identity, reference, 16)?;

    assert_eq!(run_recovery(document, 0)?, Some(RECOVERY_INCOMPLETE_KEY));
    assert!(complete_generations(&store)?.is_empty());
    Ok(())
}

#[test]
fn recovery_session_reports_failure_for_corrupt_resource() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("corrupt-resource.md");
    let resources = Arc::new(FakeResources::new([(
        "img.png".to_owned(),
        FakeResource::Corrupt,
    )]));
    let mut session = open_recovery_session(&store, resources, &path, b"abc\n", true)?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "x")?;
    let identity = document.input_identity()?;
    let reference = ResourceReference::new(document.core().document_id().clone(), "img.png");
    document
        .edit_core()?
        .retain_resource(&identity, reference, 16)?;

    // 任何资源读取失败都只让本次准备不完整，不发布、不清旧代。
    assert_eq!(run_recovery(document, 0)?, Some(RECOVERY_INCOMPLETE_KEY));
    assert!(complete_generations(&store)?.is_empty());
    Ok(())
}

#[test]
fn recovery_session_retains_old_generation_when_publish_fails() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("locked.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    assert_eq!(run_recovery(document, 0)?, None);
    let document_id = sole_lineage(&store)?;
    let first = complete_generations(&store)?;
    assert_eq!(first.len(), 1);

    let held = store
        .hold_owner_lock_for_test(document_id)
        .map_err(|error| format!("{error:?}"))?;
    recovery_type(document, 1, "b")?;
    let start = document.recovery_start(1).ok_or("恢复会话应可开始")?;
    let job = match start {
        RecoveryStart::Prepare(job) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    let prepared = job.run();
    let job = match document.recovery_prepared(prepared, 1) {
        Some(RecoveryStart::Publish(job)) => job,
        other => return Err(format!("期望发布阶段，实际 {other:?}").into()),
    };
    let settlement = job.run();
    assert_eq!(
        document.recovery_settled(settlement, 1),
        Some(RECOVERY_FAILED_KEY)
    );
    drop(held);

    assert_eq!(complete_generations(&store)?, first);
    Ok(())
}

#[test]
fn recovery_session_does_not_publish_cancelled_preparation() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("cancelled.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    let start = document.recovery_start(0).ok_or("恢复会话应可开始")?;
    let job = match start {
        RecoveryStart::Prepare(job) => job,
        other => return Err(format!("期望准备阶段，实际 {other:?}").into()),
    };
    document.cancel_recovery();
    let prepared = job.run();
    assert!(matches!(&prepared, PreparedRecovery::Cancelled { .. }));
    match document.recovery_prepared(prepared, 0) {
        Some(RecoveryStart::Settled { feedback }) => assert_eq!(feedback, None),
        other => return Err(format!("期望取消结算，实际 {other:?}").into()),
    }
    assert!(complete_generations(&store)?.is_empty());
    Ok(())
}

#[test]
fn recovery_close_job_retires_and_is_idempotent() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("closing.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        true,
    )?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "a")?;
    assert_eq!(run_recovery(document, 0)?, None);

    let job = document.take_recovery_close().ok_or("应返回关闭退役任务")?;
    match job.run() {
        RetireOutcome::Retired { .. } => {}
        other => return Err(format!("期望退役，实际 {other:?}").into()),
    }
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty(), "退役后谱系应隐藏");
    assert!(document.take_recovery_close().is_none(), "重复取走应为空");
    Ok(())
}

#[test]
fn recovery_readonly_document_never_attaches() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let path = temp.path.join("readonly.md");
    let mut session = open_recovery_session(
        &store,
        Arc::new(FakeResources::new([])),
        &path,
        b"abc\n",
        false,
    )?;
    {
        let document = session.document_mut().ok_or("应存在只读文档")?;
        assert!(document.recovery_start(0).is_none());
        assert!(document.recovery_deadline().is_none());
        assert!(document.recovery_poll(0).is_none());
        assert!(document.take_recovery_close().is_none());
    }
    assert_eq!(
        session.configure_recovery(Some(recovery_config(
            &store,
            Arc::new(FakeResources::new([]))
        ))),
        Err(RecoveryConfigError::DocumentOpen)
    );
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    assert!(scan.lineages.is_empty());
    Ok(())
}

#[test]
fn recovery_source_file_bytes_unchanged_after_publish_and_retire() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let target = temp.path.join("notes.md");
    fs::write(&target, b"# hi\n")?;
    let before = fs::read(&target)?;
    let mut session =
        open_recovery_session_real(&store, Arc::new(FakeResources::new([])), &target)?;
    let document = session.document_mut().ok_or("应存在可编辑文档")?;
    open_recovery_block(document)?;
    recovery_type(document, 0, "x")?;
    assert_eq!(run_recovery(document, 0)?, None);
    assert_eq!(fs::read(&target)?, before);

    let job = document.take_recovery_close().ok_or("应返回关闭退役任务")?;
    assert!(matches!(job.run(), RetireOutcome::Retired { .. }));
    assert_eq!(fs::read(&target)?, before);
    Ok(())
}

#[test]
fn active_lineage_quota_rejects_at_exact_boundary() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    for _ in 0..MAX_ACTIVE_LINEAGES {
        open_lineage(&store)?;
    }
    // 已达 256 条活动谱系；再开一条会变成 257，必须拒绝。
    match store.open_lineage(100) {
        Err(RecoveryStoreError::Budget(RecoveryBudget::ActiveLineages)) => {}
        other => return Err(format!("期望 ActiveLineages 预算拒绝，实际 {other:?}").into()),
    }
    Ok(())
}

#[test]
fn incomplete_reservation_quota_rejects_at_exact_boundary() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    // 31 个未结算 reservation：低于上限，发布仍应成功。
    for generation in 2..(2 + MAX_INCOMPLETE_RESERVATIONS as u64 - 1) {
        write_reservation(&store, document, generation, 0)?;
    }
    publish_ok(&store, document, sample_input("one"))?;
    assert_eq!(
        reservation_names(&store, document).len(),
        MAX_INCOMPLETE_RESERVATIONS - 1
    );
    // 补到正好 32 个未结算 reservation，下一次发布必须拒绝。
    let full = 2 + MAX_INCOMPLETE_RESERVATIONS as u64 - 1;
    write_reservation(&store, document, full, 0)?;
    // reservation 目录只读使半成品回收无法搬走文件，确保观察到配额拒绝
    // 而不是回收后的成功路径。
    let reservations = store.reservations();
    let original = fs::metadata(&reservations)?.permissions();
    let mut readonly = original.clone();
    readonly.set_mode(0o500);
    fs::set_permissions(&reservations, readonly)?;
    let result = store.publish(document, sample_input("two"), &Cancellation::new());
    fs::set_permissions(&reservations, original)?;
    match result {
        Err(RecoveryStoreError::Budget(RecoveryBudget::Reservations)) => {}
        other => return Err(format!("期望 Reservations 预算拒绝，实际 {other:?}").into()),
    }
    assert_eq!(
        reservation_names(&store, document).len(),
        MAX_INCOMPLETE_RESERVATIONS,
        "拒绝不得顺带删除既有 reservation"
    );
    Ok(())
}

#[test]
fn already_retired_discard_finishes_residual_cleanup() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    publish_ok(&store, document, sample_input("one"))?;
    let first = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    assert!(matches!(first, RetireOutcome::Retired { .. }));

    // 模拟 marker 已发布但清理中断：重建活动目录、代目录与 next-id。
    let generation = NonZeroU64::new(1).ok_or("generation 必须非零")?;
    fs::create_dir_all(store.generation_dir(document, generation))?;
    fs::write(store.next_id(document), NextId::initial().encode())?;
    let second = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    match second {
        RetireOutcome::AlreadyRetired { through } => assert_eq!(through, 1),
        other => return Err(format!("期望 AlreadyRetired，实际 {other:?}").into()),
    }
    assert!(!store.active_dir(document).exists(), "残留活动目录应被清理");
    assert!(!store.generations(document).exists());

    // 残留无法清理时不得假装 AlreadyRetired，而应返回 Deferred。
    fs::create_dir_all(store.generations(document))?;
    fs::write(store.generations(document).join("1"), b"not-a-directory")?;
    let third = store
        .discard(document)
        .map_err(|error| format!("{error:?}"))?;
    assert!(
        matches!(third, RetireOutcome::Deferred { .. }),
        "清理失败应返回 Deferred，实际 {third:?}"
    );
    Ok(())
}

#[test]
fn publish_failure_discards_created_staging() -> Result<(), Box<dyn Error>> {
    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let directory = store.generations(document);
    let target = directory.join("source.utf8");
    fs::write(&target, b"existing")?;
    let result = store.write_published_file(
        &directory,
        "source",
        &target,
        b"new",
        RecoveryOperation::WriteGenerationFile,
    );
    assert!(result.is_err(), "目标已存在时发布必须失败");
    let staged = fs::read_dir(&directory)?
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".staging"))
        .count();
    assert_eq!(staged, 0, "发布失败不得残留 staging");
    assert_eq!(fs::read(&target)?, b"existing", "既有目标不得被改动");
    Ok(())
}

#[test]
fn scan_rejects_symlinked_manifest() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;

    let temp = TempRecoveryDir::new()?;
    let store = open_store(&temp)?;
    let document = open_lineage(&store)?;
    let generation = publish_ok(&store, document, sample_input("one"))?;
    let manifest = store.generation_dir(document, generation).join("manifest");
    let real = store
        .generation_dir(document, generation)
        .join("manifest.real");
    fs::rename(&manifest, &real)?;
    symlink(&real, &manifest)?;
    let scan = store.scan().map_err(|error| format!("{error:?}"))?;
    let lineage = scan
        .lineages
        .iter()
        .find(|lineage| lineage.document == document)
        .ok_or("扫描应包含该谱系")?;
    assert!(
        lineage
            .generations
            .iter()
            .any(|entry| entry.generation == generation
                && entry.status == RecoveryGenerationStatus::Corrupt),
        "符号链接 manifest 必须判为 Corrupt"
    );
    Ok(())
}
