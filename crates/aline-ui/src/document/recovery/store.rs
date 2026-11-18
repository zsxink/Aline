//! 恢复谱系的受检存储：根布局、身份分配、high-water、reservation、容量计量、
//! 两阶段代次发布、退役 marker、显式读取与清理。固定锁序为
//! `quota.lock -> ids/<id>/owner.lock`，不得反向。
//!
//! 本模块不调度 10 秒恢复、不接生产根、不修改源文件；所有路径与身份都来自
//! [`RecoveryRoot`]。仅 macOS 真实执行文件操作，其他平台在打开存储时失败关闭。

// 4.1a 只提供存储机制；生产恢复协调器（4.1b）与 UI（4.2）到来前，多数入口
// 仅由 crate 内测试使用。该豁免只覆盖本模块，接线后必须复核删除。
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::HashSet;
use std::fs;
use std::io;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use aline_source::Cancellation;
use aline_source::fs::{self as platform_fs, FsError};

use super::control::{NextId, Reservation};
use super::record::{
    EncodedPath, FileDescriptor, LINEAGE_RESERVE_BYTES, MAX_ACTIVE_LINEAGES,
    MAX_GENERATION_RESOURCE_BYTES, MAX_INCOMPLETE_RESERVATIONS, MAX_MANIFEST_BYTES, MAX_PATH_BYTES,
    MAX_REFERENCE_BYTES, MAX_RESOURCE_BYTES, MAX_RESOURCE_COUNT, MAX_TEXT_BYTES, NEXT_ID_BYTES,
    RESERVATION_BYTES, RecordDefect, RecordError, RecoveryRecord, ResourceEncoding,
    RetiredEncoding, SnapshotEncoding,
};
pub(crate) use super::root::RecoveryDocumentId;
use super::root::{RecoveryRoot, RootError};

/// 已打开的恢复存储；根在构造时已通过验证。
#[derive(Clone, Debug)]
pub(crate) struct RecoveryStore {
    root: RecoveryRoot,
    #[cfg(test)]
    root_payload_limit: u64,
}

/// 一次恢复写入的输入；source/logical 已是 UTF-8 文本，baseline 为存储字节。
pub(crate) struct RecoveryGenerationInput {
    pub(crate) path_generation: u64,
    pub(crate) captured_composition: bool,
    pub(crate) path: RecoveryPath,
    pub(crate) anchor_byte: usize,
    pub(crate) head_byte: usize,
    pub(crate) source: String,
    pub(crate) logical_saved: Option<String>,
    pub(crate) baseline: Option<Vec<u8>>,
    pub(crate) resources: Vec<RecoveryResourceInput>,
}

/// 宿主路径的原始表示；只用于恢复关联，不作授权或稳定 ID。
pub(crate) enum RecoveryPath {
    Unlinked,
    Unix(Vec<u8>),
    WindowsUtf16Le(Vec<u8>),
}

impl RecoveryPath {
    /// 记录宿主路径的原始表示，不作授权；无持久化目标时调用方必须用
    /// [`RecoveryPath::Unlinked`]。
    pub(crate) fn from_os_path(path: &Path) -> Self {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStrExt;
            Self::Unix(path.as_os_str().as_bytes().to_vec())
        }
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            let mut bytes = Vec::new();
            for unit in path.as_os_str().encode_wide() {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
            Self::WindowsUtf16Le(bytes)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = path;
            Self::Unlinked
        }
    }
}

/// 一条待写资源的引用键与正文；引用键只在代内唯一。
pub(crate) struct RecoveryResourceInput {
    pub(crate) reference: String,
    pub(crate) bytes: Vec<u8>,
}

/// 一次发布会话的结果；仅 `Published` 表示 manifest 已提交并结算。
#[derive(Debug)]
pub(crate) enum RecoveryPublishOutcome {
    Published {
        generation: NonZeroU64,
        generation_bytes: u64,
    },
    PublishedUncertain {
        generation: NonZeroU64,
        operation: RecoveryOperation,
    },
    NotPublished {
        generation: NonZeroU64,
        cause: RecoveryPublishCause,
    },
}

/// 未发布或不确定发布的分类；保留旧代与 reservation 的事实。
#[derive(Debug)]
pub(crate) enum RecoveryPublishCause {
    Cancelled,
    LockBusy(RecoveryLock),
    Budget(RecoveryBudget),
    Record(RecordError),
    UnknownFormat {
        path: PathBuf,
    },
    Io {
        operation: RecoveryOperation,
        source: io::Error,
    },
    Uncertain {
        operation: RecoveryOperation,
    },
    ManifestAbsent,
}

/// 一次退役尝试的结果；`Deferred` 保留谱系与 marker。
#[derive(Debug)]
pub(crate) enum RetireOutcome {
    Retired { generation: NonZeroU64 },
    AlreadyRetired { through: u64 },
    Deferred { cause: RecoveryPublishCause },
}

/// 显式恢复出的内存文档；不触碰源文件或原路径授权。
#[derive(Debug)]
pub(crate) struct RecoveredDocument {
    pub(crate) document: RecoveryDocumentId,
    pub(crate) generation: NonZeroU64,
    pub(crate) path_generation: u64,
    pub(crate) captured_composition: bool,
    pub(crate) path: RecoveredPath,
    pub(crate) anchor_byte: usize,
    pub(crate) head_byte: usize,
    pub(crate) source: String,
    pub(crate) logical_saved: Option<String>,
    pub(crate) baseline: Option<Vec<u8>>,
    pub(crate) resources: Vec<RecoveredResource>,
}

/// 恢复出的路径原始表示；与 [`RecoveryPath`] 对称。
#[derive(Debug)]
pub(crate) enum RecoveredPath {
    Unlinked,
    Unix(Vec<u8>),
    WindowsUtf16Le(Vec<u8>),
}

/// 恢复出的一条资源。
#[derive(Debug)]
pub(crate) struct RecoveredResource {
    pub(crate) reference: String,
    pub(crate) bytes: Vec<u8>,
}

/// 双锁中的一把；用于 `LockBusy` 分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryLock {
    Quota,
    Owner,
}

/// 失败发生的存储操作；用于不确定发布的回执。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryOperation {
    OpenRoot,
    OpenLineage,
    QuotaLock,
    OwnerLock,
    Meter,
    AllocateIdentity,
    UpdateHighWater,
    WriteReservation,
    CreateGeneration,
    WriteGenerationFile,
    WriteManifest,
    SyncDirectory,
    SettleReservation,
    Retire,
    Reclaim,
    RemoveTrash,
    ReadManifest,
    ReadGeneration,
    Scan,
}

/// 容量或数量预算类别；不提高任何冻结上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryBudget {
    RootPayload,
    ActiveLineages,
    Ids,
    Reservations,
    Manifest,
    Resources,
    GenerationResources,
    Text,
}

/// 谱系协议缺陷；出现时禁写禁清并保留原 bytes。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryDefect {
    BadIdentityName,
    BadGenerationName,
    MissingIdentityDirectory,
    MissingOwnerLock,
    MissingActiveDirectory,
    MissingEntry,
    BadControl,
    BadReservation,
    BadRetiredMarker,
    ActiveLineagesExceeded,
    NonOrdinaryEntry,
    SymlinkEntry,
    ForeignOwner,
    InvalidPath,
    IdentityChanged,
    OrphanGeneration,
    OrphanReservation,
    RetiredRangeViolation,
    GenerationPayloadIncomplete,
    RecordCorrupt,
    RecordUnknownFormat,
    LocalFailure,
    Locked,
    ConflictingEntry,
}

/// 存储操作失败；`Uncertain` 表示副作用可能已发生。
#[derive(Debug)]
pub(crate) enum RecoveryStoreError {
    Disabled(RootError),
    InvalidInput,
    LockBusy {
        lock: RecoveryLock,
    },
    Retired {
        document: RecoveryDocumentId,
    },
    Protocol {
        document: RecoveryDocumentId,
        defect: RecoveryDefect,
    },
    Budget(RecoveryBudget),
    UnknownFormat {
        path: PathBuf,
    },
    Io {
        operation: RecoveryOperation,
        source: io::Error,
    },
    Uncertain {
        operation: RecoveryOperation,
    },
}

/// 显式读取错误；`Corrupt`/`UnknownFormat` 不触发任何清理。
#[derive(Debug)]
pub(crate) enum RecoveryReadError {
    Disabled(RootError),
    InvalidInput,
    LockBusy {
        lock: RecoveryLock,
    },
    Retired {
        document: RecoveryDocumentId,
    },
    Protocol {
        document: RecoveryDocumentId,
        defect: RecoveryDefect,
    },
    Corrupt {
        generation: NonZeroU64,
        defect: RecordDefect,
    },
    UnknownFormat {
        generation: NonZeroU64,
        path: PathBuf,
    },
    Budget(RecoveryBudget),
    Io {
        operation: RecoveryOperation,
        source: io::Error,
    },
}

struct PreparedInput {
    source: FileDescriptor,
    logical_saved: Option<FileDescriptor>,
    baseline: Option<FileDescriptor>,
    resources: Vec<FileDescriptor>,
    reserved_bytes: u64,
}

impl RecoveryStore {
    /// 打开存储并确保 `v1/{ids,active,reservations}` 存在；不创建谱系目录。
    pub(crate) fn open(root: RecoveryRoot) -> Result<Self, RecoveryStoreError> {
        let store = Self {
            root,
            #[cfg(test)]
            root_payload_limit: super::record::ROOT_PAYLOAD_LIMIT,
        };
        store.ensure_layout()?;
        Ok(store)
    }

    /// 只由测试注入更小的根容量上限，便于验证预算分支。
    #[cfg(test)]
    pub(crate) fn with_root_payload_limit(mut self, limit: u64) -> Self {
        self.root_payload_limit = limit;
        self
    }

    pub(super) fn root_limit(&self) -> u64 {
        #[cfg(test)]
        {
            self.root_payload_limit
        }
        #[cfg(not(test))]
        {
            super::record::ROOT_PAYLOAD_LIMIT
        }
    }

    /// 仅供测试持有 owner 锁，验证发布/扫描的 LockBusy 分支；File drop 即释放。
    #[cfg(test)]
    pub(crate) fn hold_owner_lock_for_test(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<std::fs::File, RecoveryStoreError> {
        let path = self.owner_lock(document);
        let file =
            platform_fs::open_lock_file(&path).map_err(|error| self.owner_fs(document, error))?;
        platform_fs::try_lock_exclusive(&file).map_err(|error| match error {
            FsError::WouldBlock => RecoveryStoreError::LockBusy {
                lock: RecoveryLock::Owner,
            },
            other => self.owner_fs(document, other),
        })?;
        Ok(file)
    }

    /// 分配新的稳定身份并创建空谱系；`ids/<id>` 一旦创建永不删除。
    pub(crate) fn open_lineage(
        &self,
        candidate_nanos: u64,
    ) -> Result<RecoveryDocumentId, RecoveryStoreError> {
        let _quota = self.lock_quota()?;
        let active = self.list_active_documents()?;
        if active.len() > MAX_ACTIVE_LINEAGES {
            return Err(RecoveryStoreError::Budget(RecoveryBudget::ActiveLineages));
        }
        self.sync_quota_scope(None)?;
        let total = self.quota_total()?;
        let admission = total
            .checked_add(NEXT_ID_BYTES)
            .and_then(|current| current.checked_add(LINEAGE_RESERVE_BYTES))
            .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
        if admission > self.root_limit() {
            return Err(RecoveryStoreError::Budget(RecoveryBudget::RootPayload));
        }
        let document = self.allocate_identity(candidate_nanos)?;
        let owner = self.create_owner_lock(document)?;
        self.initialize_lineage(document)?;
        drop(owner);
        Ok(document)
    }

    /// 两阶段发布一代恢复记录；只有 manifest 是提交标记。
    pub(crate) fn publish(
        &self,
        document: RecoveryDocumentId,
        input: RecoveryGenerationInput,
        cancellation: &Cancellation,
    ) -> Result<RecoveryPublishOutcome, RecoveryStoreError> {
        let prepared = self.precheck_input(&input)?;

        // 阶段 A：容量与 high-water 在双锁内确定。
        let quota = self.lock_quota()?;
        let owner = self.lock_owner(document)?;
        if self.read_retired(document)?.is_some() {
            return Err(RecoveryStoreError::Retired { document });
        }
        self.verify_lineage(document)?;
        let (mut next, next_identity) = self.read_next_id(document)?;
        next = self.rebuild_high_water(document, next)?;
        self.sync_quota_scope(Some(document))?;
        let mut total = self.quota_total()?;

        let generation = next.next_generation;
        let ordinal_start = next.next_resource_ordinal;
        let parent = self.latest_complete_generation(document)?;
        let resource_ordinals = ordinal_sequence(ordinal_start, prepared.resources.len())?;
        let resources = prepared
            .resources
            .iter()
            .zip(input.resources.iter())
            .zip(resource_ordinals.iter())
            .map(|((descriptor, resource), ordinal)| ResourceEncoding {
                ordinal: *ordinal,
                reference: &resource.reference,
                bytes: *descriptor,
            })
            .collect::<Vec<_>>();
        let path = match &input.path {
            RecoveryPath::Unlinked => EncodedPath::Unlinked,
            RecoveryPath::Unix(bytes) => EncodedPath::Unix(bytes),
            RecoveryPath::WindowsUtf16Le(bytes) => EncodedPath::WindowsUtf16Le(bytes),
        };
        let manifest = {
            let spec = SnapshotEncoding {
                document_id: document.get(),
                generation,
                parent_generation: parent,
                path_generation: input.path_generation,
                captured_composition: input.captured_composition,
                path,
                anchor_byte: u64::try_from(input.anchor_byte)
                    .map_err(|_| RecoveryStoreError::InvalidInput)?,
                head_byte: u64::try_from(input.head_byte)
                    .map_err(|_| RecoveryStoreError::InvalidInput)?,
                source: prepared.source,
                logical_saved: prepared.logical_saved,
                baseline: prepared.baseline,
                resources: &resources,
            };
            match super::record::encode_snapshot(&spec) {
                Ok(bytes) => bytes,
                Err(RecordError::Budget) => {
                    return Err(RecoveryStoreError::Budget(RecoveryBudget::Manifest));
                }
                Err(_) => return Err(RecoveryStoreError::InvalidInput),
            }
        };

        // 未结算 reservation 数量与配额准入。
        let mut reservations = self.list_reservations(document)?;
        if reservations.len() > MAX_INCOMPLETE_RESERVATIONS {
            self.reclaim_half_products(document, &next)?;
            reservations = self.list_reservations(document)?;
            if reservations.len() > MAX_INCOMPLETE_RESERVATIONS {
                return Err(RecoveryStoreError::Budget(RecoveryBudget::Reservations));
            }
        }
        let admission = total
            .checked_add(prepared.reserved_bytes)
            .and_then(|current| current.checked_add(RESERVATION_BYTES))
            .and_then(|current| current.checked_add(NEXT_ID_BYTES))
            .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
        if admission > self.root_limit() {
            self.reclaim_old_generations(document, parent, generation)
                .ok();
            total = self.quota_total()?;
            let admission = total
                .checked_add(prepared.reserved_bytes)
                .and_then(|current| current.checked_add(RESERVATION_BYTES))
                .and_then(|current| current.checked_add(NEXT_ID_BYTES))
                .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
            if admission > self.root_limit() {
                return Err(RecoveryStoreError::Budget(RecoveryBudget::RootPayload));
            }
        }

        // 先推进 high-water，再创建 reservation。
        let mut advanced = next;
        if advanced.advance_generation(generation).is_err()
            || advanced
                .advance_resource_ordinal(prepared.resources.len())
                .is_err()
        {
            return Err(RecoveryStoreError::Budget(RecoveryBudget::Ids));
        }
        let next_path = self.next_id(document);
        match platform_fs::replace_control_file(&next_path, next_identity, &advanced.encode()) {
            Ok(_) => {}
            Err(FsError::IdentityChanged) => {
                return Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::IdentityChanged,
                });
            }
            Err(_) => {
                return Err(RecoveryStoreError::Uncertain {
                    operation: RecoveryOperation::UpdateHighWater,
                });
            }
        }
        sync_file(&next_path)?;
        platform_fs::sync_directory(&self.active_dir(document)).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::UpdateHighWater,
            }
        })?;

        let reservation = Reservation {
            document: document.get(),
            generation,
            reserved_bytes: prepared.reserved_bytes,
        };
        let reservation_path = self.reservation_path(document, generation);
        let reservation_bytes = reservation.encode();
        let prefix = format!("{}-{}", document.directory_name(), generation.get());
        let staged =
            match platform_fs::create_staging(&self.reservations(), &prefix, &reservation_bytes) {
                Ok(staged) => staged,
                Err(_) => {
                    return Err(RecoveryStoreError::Uncertain {
                        operation: RecoveryOperation::WriteReservation,
                    });
                }
            };
        if platform_fs::publish_staged_new(&staged, &reservation_path).is_err() {
            return Err(RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::WriteReservation,
            });
        }
        discard_staging_quietly(staged);
        if sync_file(&reservation_path).is_err()
            || platform_fs::sync_directory(&self.reservations()).is_err()
        {
            return Err(RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::WriteReservation,
            });
        }
        drop(quota);

        // 阶段 B：目录、文件、目录同步，最后提交 manifest。
        let first_publication = self.list_generation_ids(document)?.is_empty();
        let generation_dir = self.generation_dir(document, generation);
        if platform_fs::create_directory_exclusive(&generation_dir).is_err() {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished {
                generation,
                cause: RecoveryPublishCause::Uncertain {
                    operation: RecoveryOperation::CreateGeneration,
                },
            });
        }
        if !resources.is_empty()
            && platform_fs::create_directory_exclusive(&generation_dir.join("r")).is_err()
        {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished {
                generation,
                cause: RecoveryPublishCause::Uncertain {
                    operation: RecoveryOperation::CreateGeneration,
                },
            });
        }
        let mut resource_dirs = Vec::new();
        for resource in &resources {
            let directory = self.resource_dir(document, generation, resource.ordinal);
            if platform_fs::create_directory_exclusive(&directory).is_err() {
                drop(owner);
                return Ok(RecoveryPublishOutcome::NotPublished {
                    generation,
                    cause: RecoveryPublishCause::Uncertain {
                        operation: RecoveryOperation::CreateGeneration,
                    },
                });
            }
            resource_dirs.push(directory);
        }
        let source_path = generation_dir.join("source.utf8");
        if let Err(cause) = self.write_published_file(
            &generation_dir,
            "source",
            &source_path,
            input.source.as_bytes(),
            RecoveryOperation::WriteGenerationFile,
        ) {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished { generation, cause });
        }
        if let Some(logical) = &input.logical_saved
            && let Err(cause) = self.write_published_file(
                &generation_dir,
                "logical-saved",
                &generation_dir.join("logical-saved.utf8"),
                logical.as_bytes(),
                RecoveryOperation::WriteGenerationFile,
            )
        {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished { generation, cause });
        }
        if let Some(baseline) = &input.baseline
            && let Err(cause) = self.write_published_file(
                &generation_dir,
                "baseline",
                &generation_dir.join("baseline.bytes"),
                baseline,
                RecoveryOperation::WriteGenerationFile,
            )
        {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished { generation, cause });
        }
        for (resource, directory) in input.resources.iter().zip(resource_dirs.iter()) {
            if let Err(cause) = self.write_published_file(
                directory,
                "bytes",
                &directory.join("bytes"),
                &resource.bytes,
                RecoveryOperation::WriteGenerationFile,
            ) {
                drop(owner);
                return Ok(RecoveryPublishOutcome::NotPublished { generation, cause });
            }
        }
        if self
            .sync_generation_dirs(document, generation, &resource_dirs, first_publication)
            .is_err()
        {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished {
                generation,
                cause: RecoveryPublishCause::Uncertain {
                    operation: RecoveryOperation::SyncDirectory,
                },
            });
        }
        if cancellation.is_cancelled() {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished {
                generation,
                cause: RecoveryPublishCause::Cancelled,
            });
        }
        let manifest_path = generation_dir.join("manifest");
        if let Err(cause) = self.write_published_file(
            &generation_dir,
            "manifest",
            &manifest_path,
            &manifest,
            RecoveryOperation::WriteManifest,
        ) {
            drop(owner);
            return Ok(RecoveryPublishOutcome::NotPublished { generation, cause });
        }
        if platform_fs::sync_directory(&generation_dir).is_err()
            || platform_fs::sync_directory(&self.generations(document)).is_err()
        {
            drop(owner);
            return Ok(RecoveryPublishOutcome::PublishedUncertain {
                generation,
                operation: RecoveryOperation::SyncDirectory,
            });
        }
        if cancellation.is_cancelled() {
            drop(owner);
            return Ok(RecoveryPublishOutcome::PublishedUncertain {
                generation,
                operation: RecoveryOperation::WriteManifest,
            });
        }
        drop(owner);

        // 阶段 C：重新取双锁，复验并结算。
        let result =
            self.settle_publish(document, generation, &prepared, &input, first_publication);
        match result {
            Ok(generation_bytes) => Ok(RecoveryPublishOutcome::Published {
                generation,
                generation_bytes,
            }),
            Err(operation) => Ok(RecoveryPublishOutcome::PublishedUncertain {
                generation,
                operation,
            }),
        }
    }

    /// 启动扫描；失败关闭时不清理任何材料。
    pub(crate) fn scan(&self) -> Result<super::scan::RecoveryScan, RecoveryStoreError> {
        super::scan::scan_store(self)
    }

    /// 显式读取一代快照到内存；不改源文件、不删旧代、不写盘。
    pub(crate) fn recover(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
    ) -> Result<RecoveredDocument, RecoveryReadError> {
        self.recover_inner(document, generation)
    }

    /// 逐文档退役：先推进 high-water，再发布终态 marker，再清理活动代。
    pub(crate) fn discard(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<RetireOutcome, RecoveryStoreError> {
        let quota = self.lock_quota()?;
        let owner = self.lock_owner(document)?;
        if let Some(through) = self.read_retired(document)? {
            drop(owner);
            drop(quota);
            return Ok(RetireOutcome::AlreadyRetired { through });
        }
        self.verify_lineage(document)?;
        let (mut next, next_identity) = self.read_next_id(document)?;
        next = self.rebuild_high_water(document, next)?;
        let marker_generation = next.next_generation;
        let retired_through = marker_generation.get().saturating_sub(1);
        let mut advanced = next;
        if advanced.advance_generation(marker_generation).is_err() {
            return Ok(RetireOutcome::Deferred {
                cause: RecoveryPublishCause::Budget(RecoveryBudget::Ids),
            });
        }
        let next_path = self.next_id(document);
        if platform_fs::replace_control_file(&next_path, next_identity, &advanced.encode()).is_err()
        {
            return Ok(RetireOutcome::Deferred {
                cause: RecoveryPublishCause::Uncertain {
                    operation: RecoveryOperation::UpdateHighWater,
                },
            });
        }
        if sync_file(&next_path).is_err()
            || platform_fs::sync_directory(&self.active_dir(document)).is_err()
        {
            return Ok(RetireOutcome::Deferred {
                cause: RecoveryPublishCause::Uncertain {
                    operation: RecoveryOperation::UpdateHighWater,
                },
            });
        }

        let marker = match super::record::encode_retired(RetiredEncoding {
            document_id: document.get(),
            generation: marker_generation,
            retired_through_generation: retired_through,
        }) {
            Ok(marker) => marker,
            Err(error) => {
                return Ok(RetireOutcome::Deferred {
                    cause: RecoveryPublishCause::Record(error),
                });
            }
        };
        let id_dir = self.id_dir(document);
        let marker_path = self.retired_path(document);
        let staged = match platform_fs::create_staging(&id_dir, "retired", &marker) {
            Ok(staged) => staged,
            Err(_) => {
                return Ok(RetireOutcome::Deferred {
                    cause: RecoveryPublishCause::Uncertain {
                        operation: RecoveryOperation::Retire,
                    },
                });
            }
        };
        if platform_fs::publish_staged_new(&staged, &marker_path).is_err() {
            return Ok(RetireOutcome::Deferred {
                cause: RecoveryPublishCause::Uncertain {
                    operation: RecoveryOperation::Retire,
                },
            });
        }
        discard_staging_quietly(staged);
        if sync_file(&marker_path).is_err()
            || platform_fs::sync_directory(&id_dir).is_err()
            || platform_fs::sync_directory(&self.ids()).is_err()
            || platform_fs::sync_directory(&self.v1()).is_err()
            || platform_fs::sync_directory(self.root.path()).is_err()
        {
            return Ok(RetireOutcome::Deferred {
                cause: RecoveryPublishCause::Uncertain {
                    operation: RecoveryOperation::Retire,
                },
            });
        }

        // marker 已确认；之后的清理失败只保留可识别残留。
        match self.retire_cleanup(document) {
            Ok(()) => Ok(RetireOutcome::Retired {
                generation: marker_generation,
            }),
            Err(operation) => Ok(RetireOutcome::Deferred {
                cause: RecoveryPublishCause::Uncertain { operation },
            }),
        }
    }
}

// --- 布局与打开 ---

impl RecoveryStore {
    pub(super) fn ensure_layout(&self) -> Result<(), RecoveryStoreError> {
        let v1 = self.v1();
        match platform_fs::verify_ordinary_directory(&v1) {
            Ok(_) => {}
            Err(FsError::NotFound) => {
                platform_fs::create_directory_exclusive(&v1).map_err(map_open_fs)?;
                platform_fs::sync_directory(self.root.path()).map_err(map_open_fs)?;
            }
            Err(error) => return Err(map_open_fs(error)),
        }
        self.root
            .verify_child(&v1)
            .map_err(RecoveryStoreError::Disabled)?;
        for path in [self.ids(), self.active(), self.reservations()] {
            match platform_fs::verify_ordinary_directory(&path) {
                Ok(_) => {
                    self.root
                        .verify_child(&path)
                        .map_err(RecoveryStoreError::Disabled)?;
                }
                Err(FsError::NotFound) => {
                    platform_fs::create_directory_exclusive(&path).map_err(map_open_fs)?;
                    platform_fs::sync_directory(&v1).map_err(map_open_fs)?;
                }
                Err(error) => return Err(map_open_fs(error)),
            }
        }
        Ok(())
    }

    pub(super) fn v1(&self) -> PathBuf {
        self.root.v1()
    }

    pub(super) fn ids(&self) -> PathBuf {
        self.v1().join("ids")
    }

    pub(super) fn active(&self) -> PathBuf {
        self.v1().join("active")
    }

    pub(super) fn reservations(&self) -> PathBuf {
        self.v1().join("reservations")
    }

    pub(super) fn quota_lock_path(&self) -> PathBuf {
        self.v1().join("quota.lock")
    }

    pub(super) fn id_dir(&self, document: RecoveryDocumentId) -> PathBuf {
        self.ids().join(document.directory_name())
    }

    pub(super) fn owner_lock(&self, document: RecoveryDocumentId) -> PathBuf {
        self.id_dir(document).join("owner.lock")
    }

    pub(super) fn retired_path(&self, document: RecoveryDocumentId) -> PathBuf {
        self.id_dir(document).join("retired")
    }

    pub(super) fn active_dir(&self, document: RecoveryDocumentId) -> PathBuf {
        self.active().join(document.directory_name())
    }

    pub(super) fn next_id(&self, document: RecoveryDocumentId) -> PathBuf {
        self.active_dir(document).join("next-id")
    }

    pub(super) fn generations(&self, document: RecoveryDocumentId) -> PathBuf {
        self.active_dir(document).join("g")
    }

    pub(super) fn generation_dir(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
    ) -> PathBuf {
        self.generations(document)
            .join(generation.get().to_string())
    }

    pub(super) fn resource_dir(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
        ordinal: NonZeroU64,
    ) -> PathBuf {
        self.generation_dir(document, generation)
            .join("r")
            .join(ordinal.get().to_string())
    }

    pub(super) fn reservation_path(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
    ) -> PathBuf {
        self.reservations().join(format!(
            "{}-{}",
            document.directory_name(),
            generation.get()
        ))
    }

    pub(super) fn trash(&self, document: RecoveryDocumentId) -> PathBuf {
        self.active_dir(document).join("trash")
    }
}

// --- 锁与身份 ---

impl RecoveryStore {
    pub(super) fn lock_quota(&self) -> Result<std::fs::File, RecoveryStoreError> {
        let path = self.quota_lock_path();
        let absent = matches!(fs::symlink_metadata(&path), Err(error) if error.kind() == io::ErrorKind::NotFound);
        let file = platform_fs::open_lock_file(&path).map_err(map_quota_fs)?;
        if absent {
            platform_fs::sync_directory(&self.v1()).map_err(|_| RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::QuotaLock,
            })?;
        }
        platform_fs::try_lock_exclusive(&file).map_err(|error| match error {
            FsError::WouldBlock => RecoveryStoreError::LockBusy {
                lock: RecoveryLock::Quota,
            },
            other => map_quota_fs(other),
        })?;
        Ok(file)
    }

    pub(super) fn lock_owner(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<std::fs::File, RecoveryStoreError> {
        let path = self.owner_lock(document);
        let file =
            platform_fs::open_lock_file(&path).map_err(|error| self.owner_fs(document, error))?;
        platform_fs::try_lock_exclusive(&file).map_err(|error| match error {
            FsError::WouldBlock => RecoveryStoreError::LockBusy {
                lock: RecoveryLock::Owner,
            },
            other => self.owner_fs(document, other),
        })?;
        Ok(file)
    }

    pub(super) fn allocate_identity(
        &self,
        candidate_nanos: u64,
    ) -> Result<RecoveryDocumentId, RecoveryStoreError> {
        let mut candidate = candidate_nanos.max(1);
        for _ in 0..1024 {
            let document = document_id_for(candidate)?;
            let path = self.id_dir(document);
            match platform_fs::create_directory_exclusive(&path) {
                Ok(_) => {
                    platform_fs::sync_directory(&self.ids()).map_err(|_| {
                        RecoveryStoreError::Uncertain {
                            operation: RecoveryOperation::AllocateIdentity,
                        }
                    })?;
                    platform_fs::sync_directory(self.root.path()).map_err(|_| {
                        RecoveryStoreError::Uncertain {
                            operation: RecoveryOperation::AllocateIdentity,
                        }
                    })?;
                    return Ok(document);
                }
                Err(FsError::AlreadyExists) => {
                    candidate = candidate
                        .checked_add(1)
                        .ok_or(RecoveryStoreError::Budget(RecoveryBudget::Ids))?;
                }
                Err(error) => return Err(self.owner_fs(document, error)),
            }
        }
        Err(RecoveryStoreError::Budget(RecoveryBudget::Ids))
    }

    pub(super) fn create_owner_lock(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<std::fs::File, RecoveryStoreError> {
        let path = self.owner_lock(document);
        let file =
            platform_fs::open_lock_file(&path).map_err(|error| self.owner_fs(document, error))?;
        platform_fs::try_lock_exclusive(&file).map_err(|error| match error {
            FsError::WouldBlock => RecoveryStoreError::LockBusy {
                lock: RecoveryLock::Owner,
            },
            other => self.owner_fs(document, other),
        })?;
        platform_fs::sync_directory(&self.id_dir(document)).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::AllocateIdentity,
            }
        })?;
        platform_fs::sync_directory(&self.ids()).map_err(|_| RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::AllocateIdentity,
        })?;
        platform_fs::sync_directory(self.root.path()).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::AllocateIdentity,
            }
        })?;
        Ok(file)
    }

    pub(super) fn initialize_lineage(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<(), RecoveryStoreError> {
        let active_dir = self.active_dir(document);
        if platform_fs::create_directory_exclusive(&active_dir).is_err() {
            return Err(RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::OrphanGeneration,
            });
        }
        if platform_fs::create_directory_exclusive(&self.generations(document)).is_err() {
            return Err(RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::OrphanGeneration,
            });
        }
        let next_bytes = NextId::initial().encode();
        let staged =
            platform_fs::create_staging(&active_dir, "next-id", &next_bytes).map_err(|_| {
                RecoveryStoreError::Uncertain {
                    operation: RecoveryOperation::AllocateIdentity,
                }
            })?;
        if platform_fs::publish_staged_new(&staged, &self.next_id(document)).is_err() {
            return Err(RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::AllocateIdentity,
            });
        }
        discard_staging_quietly(staged);
        sync_file(&self.next_id(document))?;
        for path in [
            active_dir,
            self.active(),
            self.v1(),
            self.root.path().to_path_buf(),
        ] {
            platform_fs::sync_directory(&path).map_err(|_| RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::AllocateIdentity,
            })?;
        }
        Ok(())
    }

    pub(super) fn verify_lineage(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<(), RecoveryStoreError> {
        for path in [self.id_dir(document), self.active_dir(document)] {
            self.root
                .verify_child(&path)
                .map_err(|error| self.root_child(document, error))?;
            platform_fs::verify_ordinary_directory(&path)
                .map_err(|error| self.owner_fs(document, error))?;
        }
        let owner = self.owner_lock(document);
        self.root
            .verify_child(&owner)
            .map_err(|error| self.root_child(document, error))?;
        platform_fs::verify_ordinary_file(&owner)
            .map_err(|error| self.owner_fs(document, error))?;
        Ok(())
    }

    pub(super) fn root_child(
        &self,
        document: RecoveryDocumentId,
        error: RootError,
    ) -> RecoveryStoreError {
        match error {
            RootError::UnsupportedPlatform => {
                RecoveryStoreError::Disabled(RootError::UnsupportedPlatform)
            }
            RootError::HomeUnavailable => RecoveryStoreError::Disabled(RootError::HomeUnavailable),
            RootError::Missing => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::MissingEntry,
            },
            RootError::Symlink => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::SymlinkEntry,
            },
            RootError::NotDirectory => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::NonOrdinaryEntry,
            },
            RootError::InvalidPath => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::InvalidPath,
            },
            RootError::ForeignOwner { .. } => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::ForeignOwner,
            },
            RootError::Io(error) => RecoveryStoreError::Io {
                operation: RecoveryOperation::Meter,
                source: error,
            },
        }
    }

    pub(super) fn owner_fs(
        &self,
        document: RecoveryDocumentId,
        error: FsError,
    ) -> RecoveryStoreError {
        match error {
            FsError::UnsupportedPlatform => {
                RecoveryStoreError::Disabled(RootError::UnsupportedPlatform)
            }
            FsError::InvalidPath => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::InvalidPath,
            },
            FsError::AlreadyExists => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::ConflictingEntry,
            },
            FsError::WouldBlock => RecoveryStoreError::LockBusy {
                lock: RecoveryLock::Owner,
            },
            FsError::NotFound => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::MissingEntry,
            },
            FsError::NotOrdinary => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::NonOrdinaryEntry,
            },
            FsError::Symlink => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::SymlinkEntry,
            },
            FsError::IdentityChanged => RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::IdentityChanged,
            },
            FsError::Io { kind, .. } => RecoveryStoreError::Io {
                operation: RecoveryOperation::Meter,
                source: io::Error::from(kind),
            },
        }
    }
}

// --- 枚举与计量 ---

impl RecoveryStore {
    pub(super) fn list_active_documents(
        &self,
    ) -> Result<Vec<RecoveryDocumentId>, RecoveryStoreError> {
        let directory = self.active();
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(RecoveryStoreError::Io {
                    operation: RecoveryOperation::Scan,
                    source: error,
                });
            }
        };
        let mut documents = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| RecoveryStoreError::Io {
                operation: RecoveryOperation::Scan,
                source: error,
            })?;
            let name = entry.file_name();
            let name = name
                .to_str()
                .ok_or_else(|| RecoveryStoreError::UnknownFormat { path: entry.path() })?;
            let document = RecoveryDocumentId::parse_directory_name(name)
                .map_err(|_| RecoveryStoreError::UnknownFormat { path: entry.path() })?;
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|error| RecoveryStoreError::Io {
                    operation: RecoveryOperation::Scan,
                    source: error,
                })?;
            if metadata.file_type().is_symlink() {
                return Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::SymlinkEntry,
                });
            }
            if !metadata.is_dir() {
                return Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::NonOrdinaryEntry,
                });
            }
            documents.push(document);
        }
        documents.sort_unstable();
        Ok(documents)
    }

    pub(super) fn list_generation_ids(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<Vec<NonZeroU64>, RecoveryStoreError> {
        let directory = self.generations(document);
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(RecoveryStoreError::Io {
                    operation: RecoveryOperation::Meter,
                    source: error,
                });
            }
        };
        let mut generations = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| RecoveryStoreError::Io {
                operation: RecoveryOperation::Meter,
                source: error,
            })?;
            let name = entry.file_name();
            let name = name.to_str().ok_or(RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::BadGenerationName,
            })?;
            let generation = parse_generation_name(name).ok_or(RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::BadGenerationName,
            })?;
            let metadata =
                fs::symlink_metadata(entry.path()).map_err(|error| RecoveryStoreError::Io {
                    operation: RecoveryOperation::Meter,
                    source: error,
                })?;
            if metadata.file_type().is_symlink() {
                return Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::SymlinkEntry,
                });
            }
            if !metadata.is_dir() {
                return Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::NonOrdinaryEntry,
                });
            }
            generations.push(generation);
        }
        generations.sort_unstable();
        Ok(generations)
    }

    pub(super) fn list_reservations(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<Vec<(NonZeroU64, PathBuf)>, RecoveryStoreError> {
        let prefix = format!("{}-", document.directory_name());
        let entries = match fs::read_dir(self.reservations()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(RecoveryStoreError::Io {
                    operation: RecoveryOperation::Meter,
                    source: error,
                });
            }
        };
        let mut reservations = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| RecoveryStoreError::Io {
                operation: RecoveryOperation::Meter,
                source: error,
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(suffix) = name.strip_prefix(&prefix) else {
                continue;
            };
            let generation = parse_generation_name(suffix).ok_or(RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::BadReservation,
            })?;
            reservations.push((generation, entry.path()));
        }
        reservations.sort_unstable_by_key(|(generation, _)| *generation);
        Ok(reservations)
    }

    pub(super) fn meter_v1(&self) -> Result<Meter, RecoveryStoreError> {
        let mut meter = Meter::default();
        let mut seen = HashSet::new();
        self.meter_path(&self.v1(), &mut seen, &mut meter)?;
        Ok(meter)
    }

    pub(super) fn meter_path(
        &self,
        path: &Path,
        seen: &mut HashSet<(u64, u64)>,
        meter: &mut Meter,
    ) -> Result<(), RecoveryStoreError> {
        let metadata = fs::symlink_metadata(path).map_err(|_| RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::Meter,
        })?;
        let file_type = metadata.file_type();
        if file_type.is_symlink() || (!file_type.is_file() && !file_type.is_dir()) {
            meter.defects.push(super::scan::RecoveryScanDefect {
                path: path.to_path_buf(),
                defect: if file_type.is_symlink() {
                    RecoveryDefect::SymlinkEntry
                } else {
                    RecoveryDefect::NonOrdinaryEntry
                },
            });
            meter.total = meter
                .total
                .checked_add(metadata.len())
                .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
            return Ok(());
        }
        if file_type.is_file() {
            let identity =
                platform_fs::file_identity(path).map_err(|_| RecoveryStoreError::Uncertain {
                    operation: RecoveryOperation::Meter,
                })?;
            if seen.insert((identity.dev, identity.ino)) {
                meter.total = meter
                    .total
                    .checked_add(metadata.len())
                    .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
            }
            return Ok(());
        }
        let entries = fs::read_dir(path).map_err(|_| RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::Meter,
        })?;
        for entry in entries {
            let entry = entry.map_err(|_| RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::Meter,
            })?;
            self.meter_path(&entry.path(), seen, meter)?;
        }
        Ok(())
    }

    /// 配额容量：`v1` 实际 payload 加上未结算预约尚未兑现的承诺。
    /// 每份未结算预约的代次占用按 `max(reserved_bytes, actual_generation_bytes)`
    /// 计，预约记录本身（40 bytes）已按实际文件计入 `v1` payload（design:73、
    /// brief §6.4），因此这里只补 `reserved_bytes - actual` 的差额。
    pub(super) fn quota_total(&self) -> Result<u64, RecoveryStoreError> {
        let mut total = self.meter_v1()?.total;
        let entries = match fs::read_dir(self.reservations()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(total),
            Err(error) => {
                return Err(RecoveryStoreError::Io {
                    operation: RecoveryOperation::Meter,
                    source: error,
                });
            }
        };
        for entry in entries {
            let entry = entry.map_err(|error| RecoveryStoreError::Io {
                operation: RecoveryOperation::Meter,
                source: error,
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some((document_name, generation_name)) = name.split_once('-') else {
                continue;
            };
            let Ok(document) = RecoveryDocumentId::parse_directory_name(document_name) else {
                continue;
            };
            let Some(generation) = parse_generation_name(generation_name) else {
                continue;
            };
            let bytes = fs::read(entry.path()).map_err(|_| RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::Meter,
            })?;
            let reservation =
                Reservation::decode(&bytes).map_err(|_| RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::BadReservation,
                })?;
            if reservation.document != document.get() || reservation.generation != generation {
                return Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::BadReservation,
                });
            }
            let actual = self.generation_payload_bytes(document, generation)?;
            total = total
                .checked_add(reservation.reserved_bytes.saturating_sub(actual))
                .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
        }
        Ok(total)
    }

    /// 一代目录下普通文件的 payload 之和；不跟随符号链接，元数据失败即 `Uncertain`。
    fn generation_payload_bytes(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
    ) -> Result<u64, RecoveryStoreError> {
        fn accumulate(path: &Path, total: &mut u64) -> Result<(), RecoveryStoreError> {
            let metadata = match fs::symlink_metadata(path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(error) => {
                    return Err(RecoveryStoreError::Io {
                        operation: RecoveryOperation::Meter,
                        source: error,
                    });
                }
            };
            if !metadata.file_type().is_dir() {
                *total = total
                    .checked_add(metadata.len())
                    .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
                return Ok(());
            }
            let entries = fs::read_dir(path).map_err(|error| RecoveryStoreError::Io {
                operation: RecoveryOperation::Meter,
                source: error,
            })?;
            for entry in entries {
                let entry = entry.map_err(|error| RecoveryStoreError::Io {
                    operation: RecoveryOperation::Meter,
                    source: error,
                })?;
                accumulate(&entry.path(), total)?;
            }
            Ok(())
        }

        let mut total = 0;
        accumulate(&self.generation_dir(document, generation), &mut total)?;
        Ok(total)
    }

    pub(super) fn sync_quota_scope(
        &self,
        document: Option<RecoveryDocumentId>,
    ) -> Result<(), RecoveryStoreError> {
        platform_fs::sync_directory(&self.reservations()).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::SyncDirectory,
            }
        })?;
        let documents = match document {
            Some(document) => vec![document],
            None => self
                .list_active_documents()
                .map_err(|_| RecoveryStoreError::Uncertain {
                    operation: RecoveryOperation::SyncDirectory,
                })?,
        };
        for document in documents {
            self.sync_lineage_dirs(document)?;
        }
        for path in [self.active(), self.ids(), self.v1()] {
            platform_fs::sync_directory(&path).map_err(|_| RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::SyncDirectory,
            })?;
        }
        platform_fs::sync_directory(self.root.path()).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::SyncDirectory,
            }
        })?;
        Ok(())
    }

    pub(super) fn sync_lineage_dirs(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<(), RecoveryStoreError> {
        sync_optional_directory(&self.trash(document))?;
        for generation in self.list_generation_ids(document)? {
            let generation_dir = self.generation_dir(document, generation);
            let resources = generation_dir.join("r");
            sync_optional_directory(&resources)?;
            sync_optional_directory(&generation_dir)?;
        }
        sync_optional_directory(&self.generations(document))?;
        sync_optional_directory(&self.active_dir(document))?;
        Ok(())
    }

    pub(super) fn sync_generation_dirs(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
        resource_dirs: &[PathBuf],
        first_publication: bool,
    ) -> Result<(), RecoveryStoreError> {
        for directory in resource_dirs {
            platform_fs::sync_directory(directory).map_err(|_| RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::SyncDirectory,
            })?;
        }
        let generation_dir = self.generation_dir(document, generation);
        if !resource_dirs.is_empty() {
            platform_fs::sync_directory(&generation_dir.join("r")).map_err(|_| {
                RecoveryStoreError::Uncertain {
                    operation: RecoveryOperation::SyncDirectory,
                }
            })?;
        }
        platform_fs::sync_directory(&generation_dir).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::SyncDirectory,
            }
        })?;
        platform_fs::sync_directory(&self.generations(document)).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::SyncDirectory,
            }
        })?;
        platform_fs::sync_directory(&self.active_dir(document)).map_err(|_| {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::SyncDirectory,
            }
        })?;
        if first_publication {
            for path in [self.active(), self.v1()] {
                platform_fs::sync_directory(&path).map_err(|_| RecoveryStoreError::Uncertain {
                    operation: RecoveryOperation::SyncDirectory,
                })?;
            }
            platform_fs::sync_directory(self.root.path()).map_err(|_| {
                RecoveryStoreError::Uncertain {
                    operation: RecoveryOperation::SyncDirectory,
                }
            })?;
        }
        Ok(())
    }

    pub(super) fn write_published_file(
        &self,
        directory: &Path,
        prefix: &str,
        target: &Path,
        bytes: &[u8],
        operation: RecoveryOperation,
    ) -> Result<(), RecoveryPublishCause> {
        let staged = platform_fs::create_staging(directory, prefix, bytes).map_err(|_| {
            RecoveryPublishCause::Uncertain {
                operation: RecoveryOperation::WriteGenerationFile,
            }
        })?;
        if platform_fs::publish_staged_new(&staged, target).is_err() {
            return Err(RecoveryPublishCause::Uncertain { operation });
        }
        discard_staging_quietly(staged);
        platform_fs::sync_directory(directory).map_err(|_| RecoveryPublishCause::Uncertain {
            operation: RecoveryOperation::SyncDirectory,
        })
    }

    pub(super) fn latest_complete_generation(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<Option<NonZeroU64>, RecoveryStoreError> {
        let generations = self.list_generation_ids(document)?;
        for generation in generations.into_iter().rev() {
            if self.generation_manifest_matches(document, generation) {
                return Ok(Some(generation));
            }
        }
        Ok(None)
    }

    pub(super) fn generation_manifest_matches(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
    ) -> bool {
        let path = self.generation_dir(document, generation).join("manifest");
        let Ok(bytes) = fs::read(&path) else {
            return false;
        };
        if bytes.len() > MAX_MANIFEST_BYTES {
            return false;
        }
        match RecoveryRecord::decode(&bytes) {
            Ok(RecoveryRecord::Snapshot(snapshot)) => {
                snapshot.document_id() == document.get() && snapshot.generation() == generation
            }
            _ => false,
        }
    }

    pub(super) fn reclaim_half_products(
        &self,
        document: RecoveryDocumentId,
        next: &NextId,
    ) -> Result<(), RecoveryStoreError> {
        for (generation, reservation) in self.list_reservations(document)? {
            if generation.get() >= next.next_generation.get() {
                continue;
            }
            let generation_dir = self.generation_dir(document, generation);
            if generation_dir.join("manifest").exists() {
                continue;
            }
            if generation_dir.exists() && !self.confirmed_half_product(&generation_dir)? {
                continue;
            }
            let trash = self.trash(document);
            if platform_fs::create_directory_exclusive(&trash).is_err() {
                sync_optional_directory(&trash)?;
            }
            if generation_dir.exists() {
                let identity =
                    platform_fs::verify_ordinary_directory(&generation_dir).map_err(|_| {
                        RecoveryStoreError::Uncertain {
                            operation: RecoveryOperation::Reclaim,
                        }
                    })?;
                if platform_fs::rename_no_clobber_checked(
                    &generation_dir,
                    &trash.join(generation.get().to_string()),
                    identity,
                )
                .is_err()
                {
                    continue;
                }
                platform_fs::sync_directory(&self.generations(document)).map_err(|_| {
                    RecoveryStoreError::Uncertain {
                        operation: RecoveryOperation::Reclaim,
                    }
                })?;
            }
            let reservation_identity =
                platform_fs::verify_ordinary_file(&reservation).map_err(|_| {
                    RecoveryStoreError::Uncertain {
                        operation: RecoveryOperation::Reclaim,
                    }
                })?;
            let moved = trash.join(format!("{}.reservation", generation.get()));
            if platform_fs::rename_no_clobber_checked(&reservation, &moved, reservation_identity)
                .is_ok()
            {
                platform_fs::sync_directory(&self.reservations()).ok();
            }
            platform_fs::sync_directory(&trash).ok();
            if trash.exists() {
                remove_tree_quietly(&trash);
            }
            platform_fs::sync_directory(&trash).ok();
        }
        Ok(())
    }

    pub(super) fn confirmed_half_product(
        &self,
        generation_dir: &Path,
    ) -> Result<bool, RecoveryStoreError> {
        let entries = fs::read_dir(generation_dir).map_err(|_| RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::Reclaim,
        })?;
        for entry in entries {
            let entry = entry.map_err(|_| RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::Reclaim,
            })?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                return Ok(false);
            };
            match name {
                "source.utf8" | "logical-saved.utf8" | "baseline.bytes" | "manifest" => {}
                "r" => {
                    let subentries = match fs::read_dir(entry.path()) {
                        Ok(subentries) => subentries,
                        Err(_) => return Ok(false),
                    };
                    for subentry in subentries {
                        let Ok(subentry) = subentry else {
                            return Ok(false);
                        };
                        let name = subentry.file_name();
                        let Some(name) = name.to_str() else {
                            return Ok(false);
                        };
                        if parse_generation_name(name).is_none() || !subentry.path().is_dir() {
                            return Ok(false);
                        }
                    }
                }
                _ => return Ok(false),
            }
        }
        Ok(true)
    }

    pub(super) fn reclaim_old_generations(
        &self,
        document: RecoveryDocumentId,
        keep_parent: Option<NonZeroU64>,
        keep_latest: NonZeroU64,
    ) -> Result<(), RecoveryStoreError> {
        let trash = self.trash(document);
        let mut created_trash = false;
        for generation in self.list_generation_ids(document)? {
            if generation == keep_latest || Some(generation) == keep_parent {
                continue;
            }
            let generation_dir = self.generation_dir(document, generation);
            let identity = match platform_fs::verify_ordinary_directory(&generation_dir) {
                Ok(identity) => identity,
                Err(_) => continue,
            };
            if !created_trash {
                if platform_fs::create_directory_exclusive(&trash).is_ok() {
                    platform_fs::sync_directory(&self.active_dir(document)).ok();
                }
                created_trash = true;
            }
            let target = trash.join(generation.get().to_string());
            if platform_fs::rename_no_clobber_checked(&generation_dir, &target, identity).is_err() {
                continue;
            }
            platform_fs::sync_directory(&self.generations(document)).ok();
            platform_fs::sync_directory(&trash).ok();
            remove_tree_quietly(&target);
            platform_fs::sync_directory(&trash).ok();
        }
        Ok(())
    }

    fn settle_publish(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
        prepared: &PreparedInput,
        input: &RecoveryGenerationInput,
        first_publication: bool,
    ) -> Result<u64, RecoveryOperation> {
        let quota = self
            .lock_quota()
            .map_err(|_| RecoveryOperation::SettleReservation)?;
        let owner = self
            .lock_owner(document)
            .map_err(|_| RecoveryOperation::SettleReservation)?;
        self.verify_lineage(document)
            .map_err(|_| RecoveryOperation::SettleReservation)?;
        let reservation_path = self.reservation_path(document, generation);
        let reservation_bytes =
            fs::read(&reservation_path).map_err(|_| RecoveryOperation::SettleReservation)?;
        let reservation = Reservation::decode(&reservation_bytes)
            .map_err(|_| RecoveryOperation::SettleReservation)?;
        if reservation.document != document.get() || reservation.generation != generation {
            return Err(RecoveryOperation::SettleReservation);
        }
        let generation_dir = self.generation_dir(document, generation);
        let manifest_path = generation_dir.join("manifest");
        let manifest =
            fs::read(&manifest_path).map_err(|_| RecoveryOperation::SettleReservation)?;
        let snapshot = match RecoveryRecord::decode(&manifest) {
            Ok(RecoveryRecord::Snapshot(snapshot))
                if snapshot.document_id() == document.get()
                    && snapshot.generation() == generation =>
            {
                snapshot
            }
            _ => return Err(RecoveryOperation::SettleReservation),
        };
        let source = input.source.as_bytes();
        snapshot
            .verify_source(source)
            .map_err(|_| RecoveryOperation::SettleReservation)?;
        if let Some(descriptor) = snapshot.logical_saved() {
            let bytes = fs::read(generation_dir.join("logical-saved.utf8"))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
            descriptor
                .verify_utf8(&bytes)
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        if let Some(descriptor) = snapshot.baseline() {
            let bytes = fs::read(generation_dir.join("baseline.bytes"))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
            descriptor
                .verify(&bytes)
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        for resource in snapshot.resources() {
            let directory = self.resource_dir(document, generation, resource.ordinal());
            let bytes = fs::read(directory.join("bytes"))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
            resource
                .bytes()
                .verify(&bytes)
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        sync_file(&manifest_path).map_err(|_| RecoveryOperation::SettleReservation)?;
        sync_file(&generation_dir.join("source.utf8"))
            .map_err(|_| RecoveryOperation::SettleReservation)?;
        if snapshot.logical_saved().is_some() {
            sync_file(&generation_dir.join("logical-saved.utf8"))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        if snapshot.baseline().is_some() {
            sync_file(&generation_dir.join("baseline.bytes"))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        for resource in snapshot.resources() {
            let directory = self.resource_dir(document, generation, resource.ordinal());
            sync_file(&directory.join("bytes"))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
            platform_fs::sync_directory(&directory)
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        if !snapshot.resources().is_empty() {
            platform_fs::sync_directory(&generation_dir.join("r"))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        for path in [
            generation_dir.clone(),
            self.generations(document),
            self.active_dir(document),
            self.active(),
            self.v1(),
            self.root.path().to_path_buf(),
        ] {
            platform_fs::sync_directory(&path).map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        if !first_publication {
            platform_fs::sync_directory(&self.active_dir(document))
                .map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        fs::remove_file(&reservation_path).map_err(|_| RecoveryOperation::SettleReservation)?;
        for path in [
            self.reservations(),
            self.v1(),
            self.root.path().to_path_buf(),
        ] {
            platform_fs::sync_directory(&path).map_err(|_| RecoveryOperation::SettleReservation)?;
        }
        self.meter_v1()
            .map_err(|_| RecoveryOperation::SettleReservation)?;
        self.reclaim_old_generations(document, snapshot.parent_generation(), generation)
            .ok();
        drop(owner);
        drop(quota);
        let mut generation_bytes =
            u64::try_from(manifest.len()).map_err(|_| RecoveryOperation::SettleReservation)?;
        generation_bytes = generation_bytes
            .checked_add(u64::try_from(prepared.source.len_bytes()).unwrap_or(u64::MAX))
            .ok_or(RecoveryOperation::SettleReservation)?;
        if let Some(logical) = prepared.logical_saved {
            generation_bytes = generation_bytes
                .checked_add(u64::try_from(logical.len_bytes()).unwrap_or(u64::MAX))
                .ok_or(RecoveryOperation::SettleReservation)?;
        }
        if let Some(baseline) = prepared.baseline {
            generation_bytes = generation_bytes
                .checked_add(u64::try_from(baseline.len_bytes()).unwrap_or(u64::MAX))
                .ok_or(RecoveryOperation::SettleReservation)?;
        }
        for resource in &prepared.resources {
            generation_bytes = generation_bytes
                .checked_add(u64::try_from(resource.len_bytes()).unwrap_or(u64::MAX))
                .ok_or(RecoveryOperation::SettleReservation)?;
        }
        Ok(generation_bytes)
    }

    pub(super) fn read_next_id(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<(NextId, aline_source::fs::FileIdentity), RecoveryStoreError> {
        let path = self.next_id(document);
        self.root
            .verify_child(&path)
            .map_err(|error| self.root_child(document, error))?;
        let identity = platform_fs::verify_ordinary_file(&path)
            .map_err(|error| self.owner_fs(document, error))?;
        let bytes = fs::read(&path).map_err(|error| RecoveryStoreError::Io {
            operation: RecoveryOperation::ReadGeneration,
            source: error,
        })?;
        let next = NextId::decode(&bytes).map_err(|_| RecoveryStoreError::Protocol {
            document,
            defect: RecoveryDefect::BadControl,
        })?;
        Ok((next, identity))
    }

    pub(super) fn rebuild_high_water(
        &self,
        document: RecoveryDocumentId,
        stored: NextId,
    ) -> Result<NextId, RecoveryStoreError> {
        let mut next = stored;
        let mut max_generation = None;
        let mut max_ordinal = None;
        for generation in self.list_generation_ids(document)? {
            max_generation = Some(
                max_generation.map_or(generation, |current: NonZeroU64| current.max(generation)),
            );
            let resources = self.generation_dir(document, generation).join("r");
            if let Ok(entries) = fs::read_dir(&resources) {
                for entry in entries {
                    let Ok(entry) = entry else { continue };
                    let name = entry.file_name();
                    let Some(name) = name.to_str() else { continue };
                    if let Some(ordinal) = parse_generation_name(name) {
                        max_ordinal = Some(
                            max_ordinal.map_or(ordinal, |current: NonZeroU64| current.max(ordinal)),
                        );
                    }
                }
            }
        }
        for (generation, _) in self.list_reservations(document)? {
            max_generation = Some(
                max_generation.map_or(generation, |current: NonZeroU64| current.max(generation)),
            );
        }
        if let Some(generation) = max_generation {
            let target = generation
                .get()
                .checked_add(1)
                .and_then(NonZeroU64::new)
                .ok_or(RecoveryStoreError::Budget(RecoveryBudget::Ids))?;
            if target > next.next_generation {
                next.next_generation = target;
            }
        }
        if let Some(ordinal) = max_ordinal {
            let target = ordinal
                .get()
                .checked_add(1)
                .and_then(NonZeroU64::new)
                .ok_or(RecoveryStoreError::Budget(RecoveryBudget::Ids))?;
            if target > next.next_resource_ordinal {
                next.next_resource_ordinal = target;
            }
        }
        Ok(next)
    }

    pub(super) fn read_retired(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<Option<u64>, RecoveryStoreError> {
        let path = self.retired_path(document);
        match fs::symlink_metadata(&path) {
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(RecoveryStoreError::Io {
                    operation: RecoveryOperation::Scan,
                    source: error,
                });
            }
        }
        self.root
            .verify_child(&path)
            .map_err(|error| self.root_child(document, error))?;
        platform_fs::verify_ordinary_file(&path).map_err(|error| self.owner_fs(document, error))?;
        let bytes = fs::read(&path).map_err(|error| RecoveryStoreError::Io {
            operation: RecoveryOperation::Scan,
            source: error,
        })?;
        match RecoveryRecord::decode(&bytes) {
            Ok(RecoveryRecord::Retired(retired)) if retired.document_id() == document.get() => {
                Ok(Some(retired.retired_through_generation()))
            }
            Ok(_) => Err(RecoveryStoreError::Protocol {
                document,
                defect: RecoveryDefect::BadRetiredMarker,
            }),
            Err(error) => match error.defect() {
                RecordDefect::UnknownFormat => Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::RecordUnknownFormat,
                }),
                _ => Err(RecoveryStoreError::Protocol {
                    document,
                    defect: RecoveryDefect::BadRetiredMarker,
                }),
            },
        }
    }

    fn precheck_input(
        &self,
        input: &RecoveryGenerationInput,
    ) -> Result<PreparedInput, RecoveryStoreError> {
        match &input.path {
            RecoveryPath::Unlinked => {}
            RecoveryPath::Unix(bytes) => {
                if bytes.len() > MAX_PATH_BYTES {
                    return Err(RecoveryStoreError::Budget(RecoveryBudget::Text));
                }
            }
            RecoveryPath::WindowsUtf16Le(bytes) => {
                if !bytes.len().is_multiple_of(2) {
                    return Err(RecoveryStoreError::InvalidInput);
                }
                if bytes.len() > MAX_PATH_BYTES {
                    return Err(RecoveryStoreError::Budget(RecoveryBudget::Text));
                }
            }
        }
        if input.source.len() > MAX_TEXT_BYTES
            || input
                .logical_saved
                .as_ref()
                .is_some_and(|logical| logical.len() > MAX_TEXT_BYTES)
            || input
                .baseline
                .as_ref()
                .is_some_and(|baseline| baseline.len() > MAX_TEXT_BYTES)
        {
            return Err(RecoveryStoreError::Budget(RecoveryBudget::Text));
        }
        if input.anchor_byte > input.source.len() || input.head_byte > input.source.len() {
            return Err(RecoveryStoreError::InvalidInput);
        }
        if input.resources.len() > MAX_RESOURCE_COUNT {
            return Err(RecoveryStoreError::Budget(RecoveryBudget::Resources));
        }
        let mut references = Vec::new();
        references
            .try_reserve_exact(input.resources.len())
            .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::Resources))?;
        let mut total_resources = 0_usize;
        for resource in &input.resources {
            if resource.reference.is_empty() {
                return Err(RecoveryStoreError::InvalidInput);
            }
            if resource.reference.len() > MAX_REFERENCE_BYTES {
                return Err(RecoveryStoreError::Budget(RecoveryBudget::Resources));
            }
            if references.contains(&resource.reference.as_str()) {
                return Err(RecoveryStoreError::InvalidInput);
            }
            if resource.bytes.len() > MAX_RESOURCE_BYTES {
                return Err(RecoveryStoreError::Budget(RecoveryBudget::Resources));
            }
            total_resources = total_resources.checked_add(resource.bytes.len()).ok_or(
                RecoveryStoreError::Budget(RecoveryBudget::GenerationResources),
            )?;
            references.push(resource.reference.as_str());
        }
        if total_resources > MAX_GENERATION_RESOURCE_BYTES {
            return Err(RecoveryStoreError::Budget(
                RecoveryBudget::GenerationResources,
            ));
        }
        let source = FileDescriptor::of_text(input.source.as_bytes())
            .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::Text))?;
        let logical_saved = input
            .logical_saved
            .as_ref()
            .map(|logical| FileDescriptor::of_text(logical.as_bytes()))
            .transpose()
            .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::Text))?;
        let baseline = input
            .baseline
            .as_ref()
            .map(|baseline| FileDescriptor::of_text(baseline))
            .transpose()
            .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::Text))?;
        let resources = input
            .resources
            .iter()
            .map(|resource| FileDescriptor::of_resource(&resource.bytes))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::Resources))?;
        let mut reserved_bytes = u64::try_from(input.source.len())
            .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
        reserved_bytes = reserved_bytes
            .checked_add(
                input
                    .logical_saved
                    .as_ref()
                    .map_or(0, |logical| logical.len() as u64),
            )
            .and_then(|current| {
                current.checked_add(
                    input
                        .baseline
                        .as_ref()
                        .map_or(0, |baseline| baseline.len() as u64),
                )
            })
            .and_then(|current| {
                current.checked_add(
                    u64::try_from(total_resources)
                        .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::RootPayload))
                        .ok()?,
                )
            })
            .and_then(|current| current.checked_add(MAX_MANIFEST_BYTES as u64))
            .ok_or(RecoveryStoreError::Budget(RecoveryBudget::RootPayload))?;
        Ok(PreparedInput {
            source,
            logical_saved,
            baseline,
            resources,
            reserved_bytes,
        })
    }

    pub(super) fn retire_cleanup(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<(), RecoveryOperation> {
        let trash = self.trash(document);
        if platform_fs::create_directory_exclusive(&trash).is_err()
            && platform_fs::verify_ordinary_directory(&trash).is_err()
        {
            return Err(RecoveryOperation::Retire);
        }
        for generation in self
            .list_generation_ids(document)
            .map_err(|_| RecoveryOperation::Retire)?
        {
            let generation_dir = self.generation_dir(document, generation);
            let identity = platform_fs::verify_ordinary_directory(&generation_dir)
                .map_err(|_| RecoveryOperation::Retire)?;
            let target = trash.join(generation.get().to_string());
            platform_fs::rename_no_clobber_checked(&generation_dir, &target, identity)
                .map_err(|_| RecoveryOperation::Retire)?;
        }
        for path in [
            self.generations(document),
            trash.clone(),
            self.active_dir(document),
        ] {
            platform_fs::sync_directory(&path).map_err(|_| RecoveryOperation::Retire)?;
        }
        remove_tree_quietly(&trash);
        platform_fs::sync_directory(&trash).ok();
        match fs::remove_file(self.next_id(document)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(RecoveryOperation::Retire),
        }
        match fs::remove_dir(self.generations(document)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(RecoveryOperation::Retire),
        }
        match fs::remove_dir(self.active_dir(document)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(RecoveryOperation::Retire),
        }
        for path in [self.active(), self.v1(), self.root.path().to_path_buf()] {
            platform_fs::sync_directory(&path).map_err(|_| RecoveryOperation::Retire)?;
        }
        Ok(())
    }

    pub(super) fn recover_inner(
        &self,
        document: RecoveryDocumentId,
        generation: NonZeroU64,
    ) -> Result<RecoveredDocument, RecoveryReadError> {
        let _owner = self.lock_owner(document).map_err(RecoveryReadError::from)?;
        if let Some(through) = self
            .read_retired(document)
            .map_err(RecoveryReadError::from)?
        {
            if generation.get() <= through {
                return Err(RecoveryReadError::Retired { document });
            }
            for other in self.list_generation_ids(document).unwrap_or_default() {
                if other.get() > through {
                    return Err(RecoveryReadError::Protocol {
                        document,
                        defect: RecoveryDefect::RetiredRangeViolation,
                    });
                }
            }
            for (other, _) in self.list_reservations(document).unwrap_or_default() {
                if other.get() > through {
                    return Err(RecoveryReadError::Protocol {
                        document,
                        defect: RecoveryDefect::RetiredRangeViolation,
                    });
                }
            }
            return Err(RecoveryReadError::Protocol {
                document,
                defect: RecoveryDefect::RetiredRangeViolation,
            });
        }
        self.verify_lineage(document)
            .map_err(RecoveryReadError::from)?;
        let (next, _identity) = self
            .read_next_id(document)
            .map_err(RecoveryReadError::from)?;
        if next.next_generation.get() <= generation.get() {
            return Err(RecoveryReadError::Protocol {
                document,
                defect: RecoveryDefect::BadControl,
            });
        }
        let generation_dir = self.generation_dir(document, generation);
        let manifest_path = generation_dir.join("manifest");
        let manifest = fs::read(&manifest_path).map_err(|error| RecoveryReadError::Io {
            operation: RecoveryOperation::ReadManifest,
            source: error,
        })?;
        if manifest.len() > MAX_MANIFEST_BYTES {
            return Err(RecoveryReadError::Budget(RecoveryBudget::Manifest));
        }
        let snapshot = match RecoveryRecord::decode(&manifest) {
            Ok(RecoveryRecord::Snapshot(snapshot)) => snapshot,
            Ok(RecoveryRecord::Retired(_)) => {
                return Err(RecoveryReadError::Corrupt {
                    generation,
                    defect: RecordDefect::Corrupt,
                });
            }
            Err(error) => return Err(read_record_error(generation, &manifest_path, error)),
        };
        if snapshot.document_id() != document.get() || snapshot.generation() != generation {
            return Err(RecoveryReadError::Corrupt {
                generation,
                defect: RecordDefect::Corrupt,
            });
        }
        let source_bytes = fs::read(generation_dir.join("source.utf8")).map_err(|error| {
            RecoveryReadError::Io {
                operation: RecoveryOperation::ReadGeneration,
                source: error,
            }
        })?;
        let source = match snapshot.verify_source(&source_bytes) {
            Ok(source) => source.to_owned(),
            Err(error) => return Err(read_record_error(generation, &manifest_path, error)),
        };
        let logical_saved = match snapshot.logical_saved() {
            Some(descriptor) => {
                let bytes =
                    fs::read(generation_dir.join("logical-saved.utf8")).map_err(|error| {
                        RecoveryReadError::Io {
                            operation: RecoveryOperation::ReadGeneration,
                            source: error,
                        }
                    })?;
                match descriptor.verify_utf8(&bytes) {
                    Ok(text) => Some(text.to_owned()),
                    Err(error) => {
                        return Err(read_record_error(generation, &manifest_path, error));
                    }
                }
            }
            None => None,
        };
        let baseline = match snapshot.baseline() {
            Some(descriptor) => {
                let bytes = fs::read(generation_dir.join("baseline.bytes")).map_err(|error| {
                    RecoveryReadError::Io {
                        operation: RecoveryOperation::ReadGeneration,
                        source: error,
                    }
                })?;
                if let Err(error) = descriptor.verify(&bytes) {
                    return Err(read_record_error(generation, &manifest_path, error));
                }
                Some(bytes)
            }
            None => None,
        };
        let mut resources = Vec::new();
        resources
            .try_reserve_exact(snapshot.resources().len())
            .map_err(|_| RecoveryReadError::Budget(RecoveryBudget::Resources))?;
        for resource in snapshot.resources() {
            let path = self
                .resource_dir(document, generation, resource.ordinal())
                .join("bytes");
            let bytes = fs::read(&path).map_err(|error| RecoveryReadError::Io {
                operation: RecoveryOperation::ReadGeneration,
                source: error,
            })?;
            if let Err(error) = resource.bytes().verify(&bytes) {
                return Err(read_record_error(generation, &path, error));
            }
            resources.push(RecoveredResource {
                reference: resource.reference().to_owned(),
                bytes,
            });
        }
        let path = match snapshot.path() {
            super::record::RecordedPath::Unlinked => RecoveredPath::Unlinked,
            super::record::RecordedPath::Unix(bytes) => RecoveredPath::Unix(bytes.to_vec()),
            super::record::RecordedPath::WindowsUtf16Le(bytes) => {
                RecoveredPath::WindowsUtf16Le(bytes.to_vec())
            }
        };
        Ok(RecoveredDocument {
            document,
            generation,
            path_generation: snapshot.path_generation(),
            captured_composition: snapshot.captured_composition(),
            path,
            anchor_byte: snapshot.selection().anchor_byte(),
            head_byte: snapshot.selection().head_byte(),
            source,
            logical_saved,
            baseline,
            resources,
        })
    }
}

#[derive(Default)]
pub(super) struct Meter {
    pub(super) total: u64,
    pub(super) defects: Vec<super::scan::RecoveryScanDefect>,
}

fn document_id_for(value: u64) -> Result<RecoveryDocumentId, RecoveryStoreError> {
    RecoveryDocumentId::parse_directory_name(&format!("{value:016x}"))
        .map_err(|_| RecoveryStoreError::InvalidInput)
}

pub(super) fn parse_generation_name(name: &str) -> Option<NonZeroU64> {
    if name.is_empty() || name.len() > 20 || !name.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    name.parse::<u64>().ok().and_then(NonZeroU64::new)
}

fn ordinal_sequence(
    start: NonZeroU64,
    count: usize,
) -> Result<Vec<NonZeroU64>, RecoveryStoreError> {
    let mut ordinals = Vec::new();
    ordinals
        .try_reserve_exact(count)
        .map_err(|_| RecoveryStoreError::Budget(RecoveryBudget::Resources))?;
    let mut current = start.get();
    for _ in 0..count {
        let ordinal =
            NonZeroU64::new(current).ok_or(RecoveryStoreError::Budget(RecoveryBudget::Ids))?;
        ordinals.push(ordinal);
        current = current
            .checked_add(1)
            .ok_or(RecoveryStoreError::Budget(RecoveryBudget::Ids))?;
    }
    Ok(ordinals)
}

fn map_open_fs(error: FsError) -> RecoveryStoreError {
    match error {
        FsError::UnsupportedPlatform => {
            RecoveryStoreError::Disabled(RootError::UnsupportedPlatform)
        }
        FsError::InvalidPath => RecoveryStoreError::Disabled(RootError::InvalidPath),
        FsError::NotFound => RecoveryStoreError::Disabled(RootError::Missing),
        FsError::Symlink => RecoveryStoreError::Disabled(RootError::Symlink),
        FsError::NotOrdinary => RecoveryStoreError::Disabled(RootError::NotDirectory),
        FsError::AlreadyExists | FsError::WouldBlock | FsError::IdentityChanged => {
            RecoveryStoreError::Uncertain {
                operation: RecoveryOperation::OpenRoot,
            }
        }
        FsError::Io { kind, .. } => RecoveryStoreError::Io {
            operation: RecoveryOperation::OpenRoot,
            source: io::Error::from(kind),
        },
    }
}

fn map_quota_fs(error: FsError) -> RecoveryStoreError {
    match error {
        FsError::WouldBlock => RecoveryStoreError::LockBusy {
            lock: RecoveryLock::Quota,
        },
        FsError::UnsupportedPlatform => {
            RecoveryStoreError::Disabled(RootError::UnsupportedPlatform)
        }
        FsError::Symlink => RecoveryStoreError::Disabled(RootError::Symlink),
        FsError::NotOrdinary => RecoveryStoreError::Disabled(RootError::NotDirectory),
        FsError::NotFound => RecoveryStoreError::Disabled(RootError::Missing),
        FsError::InvalidPath => RecoveryStoreError::Disabled(RootError::InvalidPath),
        FsError::AlreadyExists | FsError::IdentityChanged => RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::QuotaLock,
        },
        FsError::Io { kind, .. } => RecoveryStoreError::Io {
            operation: RecoveryOperation::QuotaLock,
            source: io::Error::from(kind),
        },
    }
}

fn sync_file(path: &Path) -> Result<(), RecoveryStoreError> {
    let file = fs::File::open(path).map_err(|error| RecoveryStoreError::Io {
        operation: RecoveryOperation::SyncDirectory,
        source: error,
    })?;
    file.sync_all().map_err(|error| RecoveryStoreError::Io {
        operation: RecoveryOperation::SyncDirectory,
        source: error,
    })
}

fn sync_optional_directory(path: &Path) -> Result<(), RecoveryStoreError> {
    match platform_fs::verify_ordinary_directory(path) {
        Ok(_) => platform_fs::sync_directory(path).map_err(|_| RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::SyncDirectory,
        }),
        Err(FsError::NotFound) => Ok(()),
        Err(_) => Err(RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::SyncDirectory,
        }),
    }
}

fn discard_staging_quietly(staged: aline_source::fs::StagedFile) {
    let _ = platform_fs::discard_staging(staged);
}

fn remove_tree_quietly(path: &Path) {
    let _ = remove_tree(path);
}

fn remove_tree(path: &Path) -> Result<bool, RecoveryStoreError> {
    let metadata = fs::symlink_metadata(path).map_err(|_| RecoveryStoreError::Uncertain {
        operation: RecoveryOperation::RemoveTrash,
    })?;
    let file_type = metadata.file_type();
    if file_type.is_symlink() || (!file_type.is_file() && !file_type.is_dir()) {
        return Ok(false);
    }
    if file_type.is_file() {
        fs::remove_file(path).map_err(|_| RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::RemoveTrash,
        })?;
        return Ok(true);
    }
    let entries = fs::read_dir(path).map_err(|_| RecoveryStoreError::Uncertain {
        operation: RecoveryOperation::RemoveTrash,
    })?;
    for entry in entries {
        let entry = entry.map_err(|_| RecoveryStoreError::Uncertain {
            operation: RecoveryOperation::RemoveTrash,
        })?;
        if !remove_tree(&entry.path())? {
            return Ok(false);
        }
    }
    fs::remove_dir(path).map_err(|_| RecoveryStoreError::Uncertain {
        operation: RecoveryOperation::RemoveTrash,
    })?;
    Ok(true)
}

fn read_record_error(generation: NonZeroU64, path: &Path, error: RecordError) -> RecoveryReadError {
    match error.defect() {
        RecordDefect::UnknownFormat => RecoveryReadError::UnknownFormat {
            generation,
            path: path.to_path_buf(),
        },
        defect => RecoveryReadError::Corrupt { generation, defect },
    }
}

impl From<RecoveryStoreError> for RecoveryReadError {
    fn from(error: RecoveryStoreError) -> Self {
        match error {
            RecoveryStoreError::Disabled(error) => RecoveryReadError::Disabled(error),
            RecoveryStoreError::InvalidInput => RecoveryReadError::InvalidInput,
            RecoveryStoreError::LockBusy { lock } => RecoveryReadError::LockBusy { lock },
            RecoveryStoreError::Retired { document } => RecoveryReadError::Retired { document },
            RecoveryStoreError::Protocol { document, defect } => {
                RecoveryReadError::Protocol { document, defect }
            }
            RecoveryStoreError::Budget(budget) => RecoveryReadError::Budget(budget),
            RecoveryStoreError::UnknownFormat { path } => RecoveryReadError::UnknownFormat {
                generation: NonZeroU64::MIN,
                path,
            },
            RecoveryStoreError::Io { operation, source } => {
                RecoveryReadError::Io { operation, source }
            }
            RecoveryStoreError::Uncertain { operation } => RecoveryReadError::Io {
                operation,
                source: io::Error::other("不确定的恢复副作用"),
            },
        }
    }
}
