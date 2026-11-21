//! 启动扫描：在 `quota -> owner` 锁序下枚举活动谱系、分类每代、报告 defect，
//! 只回收已确认属于 V1 且 high-water 已覆盖的未发布半成品。任何未知版本、
//! 坏控制记录、坏 reservation 或越界退役都使谱系 `Blocked`，不自动恢复。
//!
//! 扫描只读恢复材料，不触碰源文件；非 macOS 在存储打开阶段已失败关闭。

use std::fs;
use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use super::record::{
    MAX_MANIFEST_BYTES, MAX_RESOURCE_BYTES, MAX_TEXT_BYTES, RecordDefect, RecoveryRecord,
};
use super::root::RecoveryDocumentId;
use super::store::{RecoveryDefect, RecoveryStore, RecoveryStoreError};

/// 一次启动扫描的只读结果；`defects` 是根级枚举缺陷。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryScan {
    pub(crate) root_payload_bytes: u64,
    pub(crate) lineages: Vec<RecoveryLineageScan>,
    pub(crate) defects: Vec<RecoveryScanDefect>,
}

/// 单个活动谱系的扫描结果与处置。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryLineageScan {
    pub(crate) document: RecoveryDocumentId,
    pub(crate) disposition: RecoveryLineageDisposition,
    pub(crate) retired_through: Option<u64>,
    pub(crate) generations: Vec<RecoveryGenerationScan>,
    pub(crate) reservations: Vec<NonZeroU64>,
    pub(crate) defects: Vec<RecoveryScanDefect>,
}

/// 谱系是否允许后续恢复写入与清理。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryLineageDisposition {
    Active,
    Blocked,
}

/// 单个 generation 的分类与实际 payload 字节数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryGenerationScan {
    pub(crate) generation: NonZeroU64,
    pub(crate) status: RecoveryGenerationStatus,
    pub(crate) payload_bytes: u64,
}

/// §7 的四种 generation 状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryGenerationStatus {
    Complete,
    Incomplete,
    Corrupt,
    UnknownFormat,
}

/// 扫描发现的缺陷路径与分类。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryScanDefect {
    pub(crate) path: PathBuf,
    pub(crate) defect: RecoveryDefect,
}

impl RecoveryScan {
    /// 只列出 Active 且至少有一个完整代的谱系；损坏代不进入列表。
    // 4.2b 启动扫描与恢复面板消费；4.2a 由 crate 内测试覆盖。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn recoverable_lineages(&self) -> Vec<&RecoveryLineageScan> {
        self.lineages
            .iter()
            .filter(|lineage| {
                lineage.disposition == RecoveryLineageDisposition::Active
                    && lineage.latest_complete().is_some()
            })
            .collect()
    }
}

impl RecoveryLineageScan {
    /// 返回最高代的完整 generation；更高代的损坏/未知格式不参与选择。
    // 4.2b 恢复面板消费；4.2a 由 crate 内测试覆盖。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn latest_complete(&self) -> Option<&RecoveryGenerationScan> {
        self.generations
            .iter()
            .filter(|generation| generation.status == RecoveryGenerationStatus::Complete)
            .max_by_key(|generation| generation.generation)
    }
}

pub(super) fn scan_store(store: &RecoveryStore) -> Result<RecoveryScan, RecoveryStoreError> {
    let _quota = store.lock_quota()?;
    store.sync_quota_scope(None)?;
    let mut defects = Vec::new();
    let documents = enumerate_active(store, &mut defects)?;
    let mut lineages = Vec::new();
    for document in documents {
        lineages.push(scan_lineage(store, document));
    }
    let meter = store.meter_v1()?;
    defects.extend(meter.defects);
    Ok(RecoveryScan {
        root_payload_bytes: meter.total,
        lineages,
        defects,
    })
}

fn enumerate_active(
    store: &RecoveryStore,
    defects: &mut Vec<RecoveryScanDefect>,
) -> Result<Vec<RecoveryDocumentId>, RecoveryStoreError> {
    let directory = store.active();
    let entries = match fs::read_dir(&directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => {
            return Err(RecoveryStoreError::Io {
                operation: super::store::RecoveryOperation::Scan,
                source: error,
            });
        }
    };
    let mut documents = Vec::new();
    let mut over_limit = false;
    for entry in entries {
        let entry = entry.map_err(|error| RecoveryStoreError::Io {
            operation: super::store::RecoveryOperation::Scan,
            source: error,
        })?;
        let path = entry.path();
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            defects.push(RecoveryScanDefect {
                path,
                defect: RecoveryDefect::BadIdentityName,
            });
            continue;
        };
        let Ok(document) = RecoveryDocumentId::parse_directory_name(name) else {
            defects.push(RecoveryScanDefect {
                path,
                defect: RecoveryDefect::BadIdentityName,
            });
            continue;
        };
        let metadata = fs::symlink_metadata(&path).map_err(|error| RecoveryStoreError::Io {
            operation: super::store::RecoveryOperation::Scan,
            source: error,
        })?;
        if metadata.file_type().is_symlink() {
            defects.push(RecoveryScanDefect {
                path,
                defect: RecoveryDefect::SymlinkEntry,
            });
            continue;
        }
        if !metadata.is_dir() {
            defects.push(RecoveryScanDefect {
                path,
                defect: RecoveryDefect::NonOrdinaryEntry,
            });
            continue;
        }
        if documents.len() >= super::record::MAX_ACTIVE_LINEAGES {
            if !over_limit {
                defects.push(RecoveryScanDefect {
                    path,
                    defect: RecoveryDefect::ActiveLineagesExceeded,
                });
                over_limit = true;
            }
            continue;
        }
        documents.push(document);
    }
    documents.sort_unstable();
    Ok(documents)
}

fn scan_lineage(store: &RecoveryStore, document: RecoveryDocumentId) -> RecoveryLineageScan {
    let mut lineage = RecoveryLineageScan {
        document,
        disposition: RecoveryLineageDisposition::Active,
        retired_through: None,
        generations: Vec::new(),
        reservations: Vec::new(),
        defects: Vec::new(),
    };
    let owner = match store.lock_owner(document) {
        Ok(owner) => owner,
        Err(RecoveryStoreError::LockBusy { .. }) => {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            lineage.defects.push(RecoveryScanDefect {
                path: store.owner_lock(document),
                defect: RecoveryDefect::Locked,
            });
            return lineage;
        }
        Err(error) => {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            lineage.defects.push(RecoveryScanDefect {
                path: store.owner_lock(document),
                defect: store_defect(&error),
            });
            return lineage;
        }
    };
    if let Err(error) = store.verify_lineage(document) {
        lineage.disposition = RecoveryLineageDisposition::Blocked;
        lineage.defects.push(RecoveryScanDefect {
            path: store.id_dir(document),
            defect: store_defect(&error),
        });
        drop(owner);
        return lineage;
    }
    let next = match store.read_next_id(document) {
        Ok((next, _identity)) => next,
        Err(error) => {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            lineage.defects.push(RecoveryScanDefect {
                path: store.next_id(document),
                defect: store_defect(&error),
            });
            drop(owner);
            return lineage;
        }
    };
    let mut retired_valid = true;
    match store.read_retired(document) {
        Ok(None) => {}
        Ok(Some(through)) => {
            lineage.retired_through = Some(through);
            lineage.disposition = RecoveryLineageDisposition::Blocked;
        }
        Err(error) => {
            retired_valid = false;
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            lineage.defects.push(RecoveryScanDefect {
                path: store.retired_path(document),
                defect: store_defect(&error),
            });
        }
    }
    if read_reservations(store, document, &mut lineage).is_err() {
        lineage.disposition = RecoveryLineageDisposition::Blocked;
        drop(owner);
        return lineage;
    }
    let reservations_valid = !lineage.defects.iter().any(|defect| {
        matches!(
            defect.defect,
            RecoveryDefect::BadReservation | RecoveryDefect::OrphanReservation
        )
    });
    if retired_valid
        && lineage.retired_through.is_none()
        && reservations_valid
        && lineage.disposition == RecoveryLineageDisposition::Active
        && let Err(error) = store.reclaim_half_products(document, &next)
    {
        // 回收失败不静默吞掉：记录诊断，扫描仍继续（不自动恢复）。
        record_if_absent(
            &mut lineage,
            store.generations(document),
            store_defect(&error),
        );
    }
    let reservations = match read_reservations(store, document, &mut lineage) {
        Ok(reservations) => reservations,
        Err(()) => {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            drop(owner);
            return lineage;
        }
    };
    let generations = match store.list_generation_ids(document) {
        Ok(generations) => generations,
        Err(error) => {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            lineage.defects.push(RecoveryScanDefect {
                path: store.generations(document),
                defect: store_defect(&error),
            });
            drop(owner);
            return lineage;
        }
    };
    for generation in &generations {
        if *generation >= next.next_generation {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            record_if_absent(
                &mut lineage,
                store.generation_dir(document, *generation),
                RecoveryDefect::OrphanGeneration,
            );
        }
    }
    let reservation_generations = reservations
        .iter()
        .map(|reservation| reservation.generation)
        .collect::<Vec<_>>();
    for generation in &reservation_generations {
        let has_generation = generations.contains(generation);
        let has_manifest = store
            .generation_dir(document, *generation)
            .join("manifest")
            .exists();
        if !has_generation && !has_manifest && *generation >= next.next_generation {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            record_if_absent(
                &mut lineage,
                store.reservation_path(document, *generation),
                RecoveryDefect::OrphanReservation,
            );
        }
        if *generation >= next.next_generation {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
        }
    }
    if lineage.retired_through.is_some_and(|through| {
        reservation_generations
            .iter()
            .any(|generation| generation.get() > through)
            || generations
                .iter()
                .any(|generation| generation.get() > through)
    }) {
        lineage.disposition = RecoveryLineageDisposition::Blocked;
        record_if_absent(
            &mut lineage,
            store.id_dir(document),
            RecoveryDefect::RetiredRangeViolation,
        );
    }
    lineage.reservations = reservation_generations;
    let mut scans = Vec::new();
    for generation in generations {
        let (status, payload_bytes) = inspect_generation(store, document, generation);
        if status == RecoveryGenerationStatus::UnknownFormat
            || status == RecoveryGenerationStatus::Corrupt
        {
            if status == RecoveryGenerationStatus::UnknownFormat {
                lineage.disposition = RecoveryLineageDisposition::Blocked;
            }
            record_if_absent(
                &mut lineage,
                store.generation_dir(document, generation),
                if status == RecoveryGenerationStatus::UnknownFormat {
                    RecoveryDefect::RecordUnknownFormat
                } else {
                    RecoveryDefect::RecordCorrupt
                },
            );
        }
        scans.push(RecoveryGenerationScan {
            generation,
            status,
            payload_bytes,
        });
    }
    lineage.generations = scans;
    drop(owner);
    lineage
}

fn read_reservations(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    lineage: &mut RecoveryLineageScan,
) -> Result<Vec<ValidReservation>, ()> {
    let list = match store.list_reservations(document) {
        Ok(list) => list,
        Err(error) => {
            lineage.disposition = RecoveryLineageDisposition::Blocked;
            lineage.defects.push(RecoveryScanDefect {
                path: store.reservations(),
                defect: store_defect(&error),
            });
            return Err(());
        }
    };
    let mut valid = Vec::new();
    for (generation, path) in list {
        let bytes = match read_bounded(&path, 4096) {
            Ok(bytes) => bytes,
            Err(()) => {
                lineage.disposition = RecoveryLineageDisposition::Blocked;
                record_if_absent(lineage, path, RecoveryDefect::BadReservation);
                return Err(());
            }
        };
        match super::control::Reservation::decode(&bytes) {
            Ok(reservation)
                if reservation.document == document.get()
                    && reservation.generation == generation =>
            {
                valid.push(ValidReservation { generation });
            }
            _ => {
                lineage.disposition = RecoveryLineageDisposition::Blocked;
                record_if_absent(lineage, path, RecoveryDefect::BadReservation);
                return Err(());
            }
        }
    }
    Ok(valid)
}

fn inspect_generation(
    store: &RecoveryStore,
    document: RecoveryDocumentId,
    generation: NonZeroU64,
) -> (RecoveryGenerationStatus, u64) {
    let generation_dir = store.generation_dir(document, generation);
    let manifest_path = generation_dir.join("manifest");
    let payload_bytes = tree_size(&generation_dir);
    if !manifest_path.exists() {
        return (
            if known_v1_structure(&generation_dir) {
                RecoveryGenerationStatus::Incomplete
            } else {
                RecoveryGenerationStatus::UnknownFormat
            },
            payload_bytes,
        );
    }
    let manifest = match read_bounded(&manifest_path, MAX_MANIFEST_BYTES) {
        Ok(manifest) => manifest,
        Err(()) => return (RecoveryGenerationStatus::Corrupt, payload_bytes),
    };
    let snapshot = match RecoveryRecord::decode(&manifest) {
        Ok(RecoveryRecord::Snapshot(snapshot)) => snapshot,
        Ok(RecoveryRecord::Retired(_)) => {
            return (RecoveryGenerationStatus::Corrupt, payload_bytes);
        }
        Err(error) => {
            return (
                match error.defect() {
                    RecordDefect::UnknownFormat => RecoveryGenerationStatus::UnknownFormat,
                    _ => RecoveryGenerationStatus::Corrupt,
                },
                payload_bytes,
            );
        }
    };
    if snapshot.document_id() != document.get() || snapshot.generation() != generation {
        return (RecoveryGenerationStatus::Corrupt, payload_bytes);
    }
    match read_bounded(&generation_dir.join("source.utf8"), MAX_TEXT_BYTES) {
        Ok(bytes) => {
            if snapshot.verify_source(&bytes).is_err() {
                return (RecoveryGenerationStatus::Incomplete, payload_bytes);
            }
        }
        Err(()) => return (RecoveryGenerationStatus::Incomplete, payload_bytes),
    }
    if let Some(descriptor) = snapshot.logical_saved() {
        let path = generation_dir.join("logical-saved.utf8");
        match read_bounded(&path, MAX_TEXT_BYTES) {
            Ok(bytes) if descriptor.verify_utf8(&bytes).is_ok() => {}
            _ => return (RecoveryGenerationStatus::Incomplete, payload_bytes),
        }
    }
    if let Some(descriptor) = snapshot.baseline() {
        let path = generation_dir.join("baseline.bytes");
        match read_bounded(&path, MAX_TEXT_BYTES) {
            Ok(bytes) if descriptor.verify(&bytes).is_ok() => {}
            _ => return (RecoveryGenerationStatus::Incomplete, payload_bytes),
        }
    }
    for resource in snapshot.resources() {
        let path = store
            .resource_dir(document, generation, resource.ordinal())
            .join("bytes");
        match read_bounded(&path, MAX_RESOURCE_BYTES) {
            Ok(bytes) if resource.bytes().verify(&bytes).is_ok() => {}
            _ => return (RecoveryGenerationStatus::Incomplete, payload_bytes),
        }
    }
    (RecoveryGenerationStatus::Complete, payload_bytes)
}

struct ValidReservation {
    generation: NonZeroU64,
}

fn known_v1_structure(generation_dir: &Path) -> bool {
    let entries = match fs::read_dir(generation_dir) {
        Ok(entries) => entries,
        Err(_) => return false,
    };
    for entry in entries {
        let Ok(entry) = entry else {
            return false;
        };
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            return false;
        };
        if is_staging_name(name) {
            continue;
        }
        match name {
            "source.utf8" | "logical-saved.utf8" | "baseline.bytes" => {}
            "r" => {
                let Ok(subentries) = fs::read_dir(entry.path()) else {
                    return false;
                };
                for subentry in subentries {
                    let Ok(subentry) = subentry else {
                        return false;
                    };
                    let sub_name = subentry.file_name();
                    let Some(sub_name) = sub_name.to_str() else {
                        return false;
                    };
                    if super::store::parse_generation_name(sub_name).is_none()
                        || !subentry.path().is_dir()
                    {
                        return false;
                    }
                    let Ok(files) = fs::read_dir(subentry.path()) else {
                        return false;
                    };
                    for file in files {
                        let Ok(file) = file else {
                            return false;
                        };
                        let file_name = file.file_name();
                        let Some(file_name) = file_name.to_str() else {
                            return false;
                        };
                        if file_name != "bytes" && !is_staging_name(file_name) {
                            return false;
                        }
                    }
                }
            }
            _ => return false,
        }
    }
    true
}

fn is_staging_name(name: &str) -> bool {
    let Some(prefix) = name.strip_suffix(".staging") else {
        return false;
    };
    let Some((stem, attempt)) = prefix.rsplit_once('.') else {
        return false;
    };
    !stem.is_empty() && attempt.len() == 2 && attempt.bytes().all(|byte| byte.is_ascii_digit())
}

fn tree_size(path: &Path) -> u64 {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return 0;
    };
    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        return 0;
    }
    if file_type.is_file() {
        return metadata.len();
    }
    if !file_type.is_dir() {
        return 0;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    let mut total = 0_u64;
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        total = total.saturating_add(tree_size(&entry.path()));
    }
    total
}

fn record_if_absent(lineage: &mut RecoveryLineageScan, path: PathBuf, defect: RecoveryDefect) {
    if lineage
        .defects
        .iter()
        .any(|existing| existing.defect == defect && existing.path == path)
    {
        return;
    }
    lineage.defects.push(RecoveryScanDefect { path, defect });
}

/// 先按元数据限长再读取：拒绝符号链接与非常规文件，避免无上限读取。
fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, ()> {
    let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() || !file_type.is_file() {
        return Err(());
    }
    let declared = usize::try_from(metadata.len()).map_err(|_| ())?;
    if declared > limit {
        return Err(());
    }
    let bytes = fs::read(path).map_err(|_| ())?;
    if bytes.len() > limit {
        return Err(());
    }
    Ok(bytes)
}

fn store_defect(error: &RecoveryStoreError) -> RecoveryDefect {
    match error {
        RecoveryStoreError::Protocol { defect, .. } => *defect,
        RecoveryStoreError::LockBusy { .. } => RecoveryDefect::Locked,
        RecoveryStoreError::Budget(_) => RecoveryDefect::GenerationPayloadIncomplete,
        RecoveryStoreError::UnknownFormat { .. } => RecoveryDefect::RecordUnknownFormat,
        RecoveryStoreError::Disabled(_) => RecoveryDefect::LocalFailure,
        RecoveryStoreError::InvalidInput => RecoveryDefect::BadIdentityName,
        RecoveryStoreError::Io { .. } | RecoveryStoreError::Uncertain { .. } => {
            RecoveryDefect::LocalFailure
        }
        RecoveryStoreError::Retired { .. } => RecoveryDefect::RetiredRangeViolation,
    }
}
