//! 既有目标的观察、替换与实际发布回执。持久化宿主独占写盘排程和回执结算。

use std::fs::{self, File, Metadata};
use std::io::{Read, Seek, SeekFrom};

use super::*;

/// 绑定路径、文件及父目录身份的磁盘观察。持有只读句柄以防换名后复用 inode。
#[derive(Clone, Debug)]
pub struct ExistingTarget {
    path: PathBuf,
    parent: Arc<File>,
    file: Arc<File>,
    metadata: Metadata,
    storage: StorageSnapshot,
}

impl ExistingTarget {
    /// 冻结现有普通目标的完整实际字节。`version` 是宿主的会话代次。
    ///
    /// 不接受目录、符号链接、特殊文件或多硬链接目标。预算也约束读取分配。
    /// 读取期间身份或内容状态变化时拒绝，观察无法锁住外部写入。
    pub fn observe(
        path: &Path,
        version: DiskVersion,
        budget: IoBudget,
        cancellation: &Cancellation,
    ) -> Result<Self, AtomicFailure> {
        Self::read(path, version, budget, cancellation)
            .map_err(|kind| AtomicFailure::new(kind, None))
    }

    /// 返回观察的绝对目标路径。
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// 返回宿主可解码的实际磁盘字节和会话代次。
    pub fn storage(&self) -> &StorageSnapshot {
        &self.storage
    }

    fn read(
        path: &Path,
        version: DiskVersion,
        budget: IoBudget,
        cancellation: &Cancellation,
    ) -> Result<Self, AtomicFailureKind> {
        let stage = AtomicStage::Observe;
        let parent_path = check_absolute_parent(path)?;
        check_cancel(cancellation, stage)?;
        let parent = open_parent(parent_path, stage)?;
        let (mut file, before) = open_regular(path, stage)?;
        let limit = budget.max_bytes();
        let needed = usize::try_from(before.len()).map_err(|_| AtomicFailureKind::Budget {
            needed: usize::MAX,
            limit,
        })?;
        if needed > limit {
            return Err(AtomicFailureKind::Budget { needed, limit });
        }
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(needed)
            .map_err(|source| AtomicFailureKind::Io {
                stage,
                source: io::Error::other(source),
            })?;
        bytes.resize(needed, 0);
        for chunk in bytes.chunks_mut(WRITE_CHUNK_BYTES) {
            check_cancel(cancellation, stage)?;
            if !read_chunk(&mut file, chunk, stage)? {
                return Err(changed_target(true));
            }
        }
        if !at_eof(&mut file, stage)? {
            return Err(changed_target(true));
        }
        check_path_stamp(&file, path, &before, stage)?;
        check_directory(&parent, parent_path, stage)?;
        check_cancel(cancellation, stage)?;
        let storage = StorageSnapshot::new(bytes, version, StorageBudget::new(limit))
            .map_err(|_| AtomicFailureKind::Budget { needed, limit })?;
        Ok(Self {
            path: path.to_path_buf(),
            parent: Arc::new(parent),
            file: Arc::new(file),
            metadata: before,
            storage,
        })
    }

    fn recheck(
        &self,
        stage: AtomicStage,
        cancel: &Cancellation,
    ) -> Result<File, AtomicFailureKind> {
        check_cancel(cancel, stage)?;
        let (mut file, before) = open_regular(&self.path, stage)?;
        let pinned = self.file.metadata().map_err(io_failure(stage))?;
        if !same_file_identity(&before, &pinned) || before.len() != self.metadata.len() {
            return Err(changed_target(true));
        }
        if !matches_bytes(&mut file, self.storage.bytes(), stage, Some(cancel))? {
            return Err(changed_target(true));
        }
        if !same_stamp(&before, &self.metadata) {
            return Err(changed_target(true));
        }
        check_path_stamp(&file, &self.path, &before, stage)?;
        self.check_parent(stage)?;
        Ok(file)
    }

    fn check_parent(&self, stage: AtomicStage) -> Result<(), AtomicFailureKind> {
        check_directory(&self.parent, check_absolute_parent(&self.path)?, stage)
    }
}

/// 已受检准备的一次既有目标替换，持有冻结写入字节及 staging 句柄。
#[derive(Debug)]
pub struct PreparedReplacement {
    prepared: PreparedAtomic,
    expected: ExistingTarget,
    version: DiskVersion,
}

/// 替换已发生后的目录同步事实。
#[derive(Debug)]
pub enum ReplacementDurability {
    /// 对目标父目录的 `sync_all` 已成功返回。
    Synchronized,
    /// 目标已替换，目录同步返回错误。不得将此结果当作未发布。
    Uncertain(io::Error),
}

/// 替换成功后对 staging 路径的清理事实。
///
/// `RENAME_SWAP` 后 staging 路径持有交换前的旧目标 inode；删除它才释放旧目标。
/// 清理失败不回滚已发布的新目标，宿主须按此事实决定取证或后续清理。
#[derive(Debug)]
pub enum ReplacementCleanup {
    /// staging 路径已不存在（本操作删除成功，或路径在删除前已消失）。
    Removed,
    /// staging 路径仍存在；携带删除失败原因。
    LeftInPlace(io::Error),
}

/// 既有目标已被替换的事实。后续外部写入仍须重新观察。
#[derive(Debug)]
pub struct ReplacementReceipt {
    encoded: EncodedBytes,
    path: PathBuf,
    replaced_version: DiskVersion,
    version: DiskVersion,
    durability: ReplacementDurability,
    cleanup: ReplacementCleanup,
}

impl ReplacementReceipt {
    /// 返回已替换的目标路径。
    pub fn target_path(&self) -> &Path {
        &self.path
    }

    /// 返回发布前核对的宿主磁盘代次。
    pub const fn replaced_version(&self) -> DiskVersion {
        self.replaced_version
    }

    /// 返回本次实际发布推进的宿主磁盘代次。
    pub const fn version(&self) -> DiskVersion {
        self.version
    }

    /// 返回此次操作发布的字节。之后的磁盘状态需重新观察。
    pub fn bytes(&self) -> &[u8] {
        self.encoded.bytes()
    }

    /// 返回产生冻结字节时使用的编码基线身份。
    pub fn input_baseline_id(&self) -> &BaselineId {
        self.encoded.baseline_id()
    }

    /// 返回已发生发布的目录同步事实。
    pub fn durability(&self) -> &ReplacementDurability {
        &self.durability
    }

    /// 返回替换成功后 staging 路径的清理事实。
    pub fn cleanup(&self) -> &ReplacementCleanup {
        &self.cleanup
    }

    /// 从实际发布字节建立基线，即使目录同步失败仍可用于重新观察。
    pub fn into_baseline(self) -> Result<SourceBaseline, BaselineError> {
        let budget = StorageBudget::new(self.encoded.bytes().len());
        let storage = StorageSnapshot::new(self.encoded.bytes().to_vec(), self.version, budget)?;
        Ok(decode(&storage, budget)?.into_baseline())
    }
}

/// 核对目标观察，再独占创建同目录 staging 并写入、同步完整字节。
///
/// 在目标已改变或代次耗尽时拒绝创建 staging。原目标在准备阶段保持不变。
/// 观察与编码基线分别绑定。用户确认覆盖时可以用新观察替换目标，同时使用旧编码策略。
/// 未验证 staging 初始访问权限的平台会在写入前拒绝。
pub fn prepare_checked_replace(
    encoded: &EncodedBytes,
    expected: &ExistingTarget,
    staging: &Path,
    budget: IoBudget,
    cancellation: &Cancellation,
) -> Result<PreparedReplacement, AtomicFailure> {
    prepare_checked_replace_with(
        encoded,
        expected,
        staging,
        budget,
        cancellation,
        open_staging_checked,
    )
}

fn prepare_checked_replace_with(
    encoded: &EncodedBytes,
    expected: &ExistingTarget,
    staging: &Path,
    budget: IoBudget,
    cancellation: &Cancellation,
    open_staging: impl FnOnce(&File, &Path) -> Result<File, super::platform::StagingOpenFailure>,
) -> Result<PreparedReplacement, AtomicFailure> {
    let stage = AtomicStage::Prepare;
    let target = expected.path();
    let no_staging = |kind| AtomicFailure::new(kind, None);
    validate_paths(target, staging)
        .map_err(|reason| no_staging(AtomicFailureKind::InvalidInput { reason }))?;
    let needed = encoded.bytes().len();
    let limit = budget.max_bytes();
    if needed > limit {
        return Err(no_staging(AtomicFailureKind::Budget { needed, limit }));
    }
    let version = expected
        .storage
        .version()
        .next()
        .map_err(|_| no_staging(AtomicFailureKind::VersionExhausted))?;
    expected.recheck(stage, cancellation).map_err(no_staging)?;
    check_cancel(cancellation, AtomicStage::Create).map_err(no_staging)?;
    let mut file = open_staging(&expected.parent, staging).map_err(|failure| {
        // 创建后失败同样携带调用方提供的路径；路径不保证仍存在，宿主需自行核对。
        AtomicFailure::new(failure.kind, failure.created.then(|| staging.to_path_buf()))
    })?;
    let created = || Some(staging.to_path_buf());
    // 独占创建可能发生在另一个目录实例中；创建只是准备，此时仍未写入文档字节。
    expected
        .check_parent(AtomicStage::Create)
        .map_err(|kind| AtomicFailure::new(kind, created()))?;
    check_staging_identity(&file, staging, AtomicStage::Create)?;
    expected
        .check_parent(AtomicStage::Create)
        .map_err(|kind| AtomicFailure::new(kind, created()))?;
    write_staging_owned(&mut file, encoded.bytes(), staging, budget, cancellation)?;
    Ok(PreparedReplacement {
        prepared: PreparedAtomic {
            encoded: encoded.clone(),
            target: target.to_path_buf(),
            staging: staging.to_path_buf(),
            file,
        },
        expected: expected.clone(),
        version,
    })
}

impl PreparedReplacement {
    /// 复验 staging 和目标，保全访问元数据后替换目录项并同步父目录。
    ///
    /// macOS 使用已准入的平台原语（`docs/decisions.md` §44）：先在 staging 上复制并
    /// 复验目标访问元数据，再以 `RENAME_SWAP` 交换目录项并核对旧目标 inode，最后
    /// 同步父目录并删除 staging。其他平台仍缺少已验证的元数据复制与句柄身份替换
    /// 实现，保持失败关闭。若底层报告已替换但回读未能确认归属，返回
    /// `PublishedTargetUnverified`。
    pub fn publish(self, cancellation: &Cancellation) -> Result<ReplacementReceipt, AtomicFailure> {
        self.publish_with(
            cancellation,
            transfer_metadata,
            publish_existing_checked,
            File::sync_all,
        )
    }

    fn publish_with(
        self,
        cancellation: &Cancellation,
        transfer: impl FnOnce(&File, &File) -> Result<(), AtomicFailureKind>,
        replace: impl FnOnce(&File, &File, &File, &Path, &Path, &[u8]) -> Result<(), AtomicFailureKind>,
        sync_parent: impl FnOnce(&File) -> io::Result<()>,
    ) -> Result<ReplacementReceipt, AtomicFailure> {
        let Self {
            prepared,
            expected,
            version,
        } = self;
        let PreparedAtomic {
            encoded,
            target,
            staging,
            mut file,
        } = prepared;
        let owned = |kind| AtomicFailure::new(kind, Some(staging.clone()));
        let stage = AtomicStage::Publish;
        check_cancel(cancellation, stage).map_err(owned)?;
        check_staging_identity(&file, &staging, stage)?;
        let current_target = expected.recheck(stage, cancellation).map_err(owned)?;
        check_cancel(cancellation, AtomicStage::Permissions).map_err(owned)?;
        transfer(&current_target, &file).map_err(owned)?;
        check_cancel(cancellation, AtomicStage::Sync).map_err(owned)?;
        file.sync_all()
            .map_err(|source| owned(io_failure(AtomicStage::Sync)(source)))?;
        let staging_stamp = check_staging_identity(&file, &staging, stage)?;
        file.seek(SeekFrom::Start(0))
            .map_err(|source| owned(io_failure(stage)(source)))?;
        if !matches_bytes(&mut file, encoded.bytes(), stage, Some(cancellation)).map_err(owned)? {
            return Err(owned(AtomicFailureKind::StagingContentChanged));
        }
        // 权限复制、同步和 staging 字节检查可能耗时，发布前复验观察。
        let latest_target = expected.recheck(stage, cancellation).map_err(owned)?;
        if !same_stamp(
            &check_staging_identity(&file, &staging, stage)?,
            &staging_stamp,
        ) {
            return Err(owned(AtomicFailureKind::StagingContentChanged));
        }
        check_cancel(cancellation, stage).map_err(owned)?;
        let old_target = latest_target
            .metadata()
            .map_err(|source| owned(io_failure(stage)(source)))?;
        // 平台发布器必须单独验证已核验的目录、最新目标句柄和 staging 句柄，
        // 并在交换后核对旧目标仍是 `expected.storage` 的字节。回读只是附加核验；
        // 外部进程可能在回读前再次交换目录项，不能靠回读把路径 rename 当作受检发布。
        replace(
            &expected.parent,
            &latest_target,
            &file,
            &staging,
            &target,
            expected.storage.bytes(),
        )
        .map_err(owned)?;
        if let Err(cause) = verify_published(&expected, &file, encoded.bytes()) {
            return Err(AtomicFailure::new(
                AtomicFailureKind::PublishedTargetUnverified {
                    target,
                    cause: Box::new(cause),
                },
                None,
            ));
        }
        let durability = match sync_parent(&expected.parent) {
            Ok(()) => ReplacementDurability::Synchronized,
            Err(source) => ReplacementDurability::Uncertain(source),
        };
        // 交换后 staging 路径持有旧目标 inode；只删除身份仍是该 inode 的目录项，
        // 不改变已发布的新目标。
        let cleanup = match cleanup_staging(&expected.parent, &staging, &old_target) {
            Ok(()) => ReplacementCleanup::Removed,
            Err(source) if source.kind() == io::ErrorKind::NotFound => ReplacementCleanup::Removed,
            Err(source) => ReplacementCleanup::LeftInPlace(source),
        };
        Ok(ReplacementReceipt {
            encoded,
            path: target,
            replaced_version: expected.storage.version(),
            version,
            durability,
            cleanup,
        })
    }
}

/// macOS 接入已准入的平台原语；其他平台保持失败关闭。
#[cfg(target_os = "macos")]
fn transfer_metadata(target: &File, staging: &File) -> Result<(), AtomicFailureKind> {
    super::platform::transfer_access_state(target, staging)
}

#[cfg(not(target_os = "macos"))]
fn transfer_metadata(_: &File, _: &File) -> Result<(), AtomicFailureKind> {
    // 未验证的 ACL/xattr/flags 复制会扩大或丢失实际访问范围。
    Err(AtomicFailureKind::ReplacementMetadataUnverified)
}

/// macOS 以已核验父目录 fd 独占创建 staging，拒绝继承 ACL 并清除 staging 自身 ACL。
#[cfg(target_os = "macos")]
fn open_staging_checked(
    parent: &File,
    path: &Path,
) -> Result<File, super::platform::StagingOpenFailure> {
    super::platform::open_staging(parent, path)
}

#[cfg(not(target_os = "macos"))]
fn open_staging_checked(_: &File, _: &Path) -> Result<File, super::platform::StagingOpenFailure> {
    // 继承 ACL 可能扩大 mode 0600 的 staging 的实际访问范围。
    Err(super::platform::StagingOpenFailure::not_created(
        AtomicFailureKind::ReplacementMetadataUnverified,
    ))
}

/// macOS 以 `RENAME_SWAP` 发布，交换后核对旧目标 inode 与字节，必要时回滚。
#[cfg(target_os = "macos")]
fn publish_existing_checked(
    parent: &File,
    current: &File,
    staging_file: &File,
    staging: &Path,
    target: &Path,
    old_bytes: &[u8],
) -> Result<(), AtomicFailureKind> {
    super::platform::publish_existing(parent, current, staging_file, staging, target, old_bytes)
}

#[cfg(not(target_os = "macos"))]
fn publish_existing_checked(
    _: &File,
    _: &File,
    _: &File,
    _: &Path,
    _: &Path,
    _: &[u8],
) -> Result<(), AtomicFailureKind> {
    // 路径 rename 无法约束内核使用已检查的目录和 staging 句柄。
    Err(AtomicFailureKind::ReplacementIdentityUnverified)
}

/// macOS 经父目录 fd 删除 staging，并先核对它仍是交换前的旧目标 inode。
#[cfg(target_os = "macos")]
fn cleanup_staging(parent: &File, staging: &Path, old_target: &Metadata) -> io::Result<()> {
    super::platform::cleanup_staging(parent, staging, metadata_identity(old_target))
}

#[cfg(not(target_os = "macos"))]
fn cleanup_staging(_: &File, staging: &Path, _: &Metadata) -> io::Result<()> {
    fs::remove_file(staging)
}

#[cfg(unix)]
fn metadata_identity(metadata: &Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt as _;
    (metadata.dev(), metadata.ino())
}

#[cfg(not(unix))]
fn metadata_identity(_: &Metadata) -> (u64, u64) {
    (0, 0)
}

fn verify_published(
    expected: &ExistingTarget,
    staging: &File,
    encoded: &[u8],
) -> Result<(), AtomicFailureKind> {
    let stage = AtomicStage::VerifyPublished;
    expected.check_parent(stage)?;
    let (mut target, before) = open_regular(expected.path(), stage)?;
    let prepared = staging.metadata().map_err(io_failure(stage))?;
    if !same_file_identity(&before, &prepared) || !matches_bytes(&mut target, encoded, stage, None)?
    {
        return Err(changed_target(true));
    }
    check_path_stamp(&target, expected.path(), &before, stage)?;
    expected.check_parent(stage)
}

fn check_absolute_parent(path: &Path) -> Result<&Path, AtomicFailureKind> {
    if !path.is_absolute() {
        return Err(AtomicFailureKind::InvalidInput {
            reason: InvalidInput::RelativePath,
        });
    }
    path.parent().ok_or(AtomicFailureKind::InvalidInput {
        reason: InvalidInput::MissingParent,
    })
}

fn open_parent(path: &Path, stage: AtomicStage) -> Result<File, AtomicFailureKind> {
    let metadata = path_metadata(path, stage)?;
    if !metadata.is_dir() {
        return Err(AtomicFailureKind::TargetDirectoryChanged);
    }
    let file = open_nofollow(path, stage)?;
    let actual = file.metadata().map_err(io_failure(stage))?;
    if !actual.is_dir() || !same_file_identity(&actual, &metadata) {
        return Err(AtomicFailureKind::TargetDirectoryChanged);
    }
    Ok(file)
}

fn open_regular(path: &Path, stage: AtomicStage) -> Result<(File, Metadata), AtomicFailureKind> {
    let before = path_metadata(path, stage)?;
    let kind = if before.is_file() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt as _;
            if before.nlink() != 1 {
                return Err(AtomicFailureKind::ExistingTarget {
                    kind: ExistingTargetKind::HardLink,
                });
            }
        }
        let file = open_nofollow(path, stage)?;
        let actual = file.metadata().map_err(io_failure(stage))?;
        if !actual.is_file() || !same_stamp(&actual, &before) {
            return Err(changed_target(true));
        }
        return Ok((file, actual));
    } else if before.file_type().is_symlink() {
        ExistingTargetKind::Symlink
    } else if before.is_dir() {
        ExistingTargetKind::Directory
    } else {
        ExistingTargetKind::Other
    };
    Err(AtomicFailureKind::ExistingTarget { kind })
}

#[cfg(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
))]
fn open_nofollow(path: &Path, stage: AtomicStage) -> Result<File, AtomicFailureKind> {
    use std::os::unix::fs::OpenOptionsExt as _;
    // sys/fcntl.h（Darwin）、uapi/asm-generic/fcntl.h（Linux）：拒绝末端链接，
    // 如果预检后换成了 FIFO，打开也不等待；打开后还需以句柄重新检查类型及身份。
    #[cfg(target_os = "macos")]
    const FLAGS: i32 = 0x0004 | 0x0100;
    #[cfg(target_os = "linux")]
    const FLAGS: i32 = 0o4000 | 0o400000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FLAGS)
        .open(path)
        .map_err(|source| {
            if source.kind() == io::ErrorKind::NotFound {
                changed_target(false)
            } else {
                io_failure(stage)(source)
            }
        })
}

#[cfg(not(any(
    target_os = "macos",
    all(
        target_os = "linux",
        any(target_arch = "aarch64", target_arch = "x86_64")
    )
)))]
fn open_nofollow(_: &Path, _: AtomicStage) -> Result<File, AtomicFailureKind> {
    Err(AtomicFailureKind::ObservationUnverified)
}

fn path_metadata(path: &Path, stage: AtomicStage) -> Result<Metadata, AtomicFailureKind> {
    fs::symlink_metadata(path).map_err(|source| {
        if source.kind() == io::ErrorKind::NotFound {
            changed_target(false)
        } else {
            io_failure(stage)(source)
        }
    })
}

fn check_path_stamp(
    file: &File,
    path: &Path,
    before: &Metadata,
    stage: AtomicStage,
) -> Result<(), AtomicFailureKind> {
    let after = file.metadata().map_err(io_failure(stage))?;
    let current_path = path_metadata(path, stage)?;
    if !same_stamp(before, &after) || !same_stamp(&after, &current_path) {
        return Err(changed_target(true));
    }
    Ok(())
}

fn check_directory(file: &File, path: &Path, stage: AtomicStage) -> Result<(), AtomicFailureKind> {
    let current = path_metadata(path, stage)?;
    let observed = file.metadata().map_err(io_failure(stage))?;
    if !current.is_dir() || !same_file_identity(&current, &observed) {
        return Err(AtomicFailureKind::TargetDirectoryChanged);
    }
    Ok(())
}

fn check_staging_identity(
    file: &File,
    path: &Path,
    stage: AtomicStage,
) -> Result<Metadata, AtomicFailure> {
    let owned = |source| AtomicFailure::new(io_failure(stage)(source), Some(path.to_path_buf()));
    let current = fs::symlink_metadata(path).map_err(owned)?;
    let actual = file.metadata().map_err(owned)?;
    if !current.is_file() || !same_file_identity(&current, &actual) {
        return Err(AtomicFailure::new(AtomicFailureKind::StagingReplaced, None));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        if current.nlink() != 1 || actual.nlink() != 1 {
            return Err(AtomicFailure::new(AtomicFailureKind::StagingReplaced, None));
        }
    }
    Ok(actual)
}

#[cfg(unix)]
fn same_stamp(left: &Metadata, right: &Metadata) -> bool {
    use std::os::unix::fs::MetadataExt as _;
    same_file_identity(left, right)
        && left.len() == right.len()
        && left.ctime() == right.ctime()
        && left.ctime_nsec() == right.ctime_nsec()
        && left.mtime() == right.mtime()
        && left.mtime_nsec() == right.mtime_nsec()
        && left.mode() == right.mode()
        && left.uid() == right.uid()
        && left.gid() == right.gid()
        && left.nlink() == right.nlink()
}

#[cfg(not(unix))]
fn same_stamp(_: &Metadata, _: &Metadata) -> bool {
    false
}

fn read_chunk(
    file: &mut File,
    chunk: &mut [u8],
    stage: AtomicStage,
) -> Result<bool, AtomicFailureKind> {
    match file.read_exact(chunk) {
        Ok(()) => Ok(true),
        Err(source) if source.kind() == io::ErrorKind::UnexpectedEof => Ok(false),
        Err(source) => Err(io_failure(stage)(source)),
    }
}

fn at_eof(file: &mut File, stage: AtomicStage) -> Result<bool, AtomicFailureKind> {
    Ok(!read_chunk(file, &mut [0], stage)?)
}

fn matches_bytes(
    file: &mut File,
    expected: &[u8],
    stage: AtomicStage,
    cancel: Option<&Cancellation>,
) -> Result<bool, AtomicFailureKind> {
    // 在栈上复用小块，以全量 bytes 对照覆盖同尺寸、同时间戳的外部编辑。
    let mut scratch = [0; 4096];
    for chunk in expected.chunks(scratch.len()) {
        if let Some(cancel) = cancel {
            check_cancel(cancel, stage)?;
        }
        let buffer = &mut scratch[..chunk.len()];
        if !read_chunk(file, buffer, stage)? || chunk != buffer {
            return Ok(false);
        }
    }
    at_eof(file, stage)
}

fn changed_target(target_exists: bool) -> AtomicFailureKind {
    AtomicFailureKind::Conflict { target_exists }
}

fn io_failure(stage: AtomicStage) -> impl FnOnce(io::Error) -> AtomicFailureKind {
    move |source| AtomicFailureKind::Io { stage, source }
}

fn check_cancel(cancel: &Cancellation, stage: AtomicStage) -> Result<(), AtomicFailureKind> {
    if cancel.is_cancelled() {
        return Err(AtomicFailureKind::Cancelled { stage });
    }
    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use std::error::Error;

    use super::super::tests::TestDir;
    use super::*;
    use crate::encode;

    const ORIGINAL: &[u8] = b"\xef\xbb\xbfalpha\r\n";
    const SAVED: &[u8] = b"\xef\xbb\xbfchanged\r\n";
    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    fn prepare(directory: &TestDir, cancel: &Cancellation) -> TestResult<PreparedReplacement> {
        prepare_paths(&directory.target(), &directory.staging(), cancel)
    }

    fn observe_and_encode(
        target: &Path,
        cancel: &Cancellation,
    ) -> TestResult<(ExistingTarget, EncodedBytes)> {
        fs::write(target, ORIGINAL)?;
        let expected = ExistingTarget::observe(
            target,
            DiskVersion::from_value(2),
            IoBudget::new(1024),
            cancel,
        )?;
        let budget = StorageBudget::new(1024);
        let baseline = decode(expected.storage(), budget)?.into_baseline();
        let encoded = encode("changed\n", &baseline, budget)?;
        Ok((expected, encoded))
    }

    fn prepare_paths(
        target: &Path,
        staging: &Path,
        cancel: &Cancellation,
    ) -> TestResult<PreparedReplacement> {
        let (expected, encoded) = observe_and_encode(target, cancel)?;
        Ok(prepare_checked_replace_with(
            &encoded,
            &expected,
            staging,
            IoBudget::new(1024),
            cancel,
            |_, path| {
                create_staging(path)
                    .map_err(super::super::platform::StagingOpenFailure::not_created)
            },
        )?)
    }

    fn create_staging(path: &Path) -> Result<File, AtomicFailureKind> {
        use std::os::unix::fs::OpenOptionsExt as _;

        OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(path)
            .map_err(io_failure(AtomicStage::Create))
    }

    fn test_path_rename(from: &Path, to: &Path) -> Result<(), AtomicFailureKind> {
        fs::rename(from, to).map_err(io_failure(AtomicStage::Publish))
    }

    #[test]
    fn replacement_rename_yields_exact_receipt_even_if_directory_sync_fails() -> TestResult {
        let directory = TestDir::new()?;
        let cancel = Cancellation::new();
        let prepared = prepare(&directory, &cancel)?;
        let encoded_baseline = prepared.prepared.encoded.baseline_id().clone();
        let receipt = prepared.publish_with(
            &cancel,
            |_, _| Ok(()),
            |_, _, _, from, to, _| {
                test_path_rename(from, to)?;
                cancel.cancel();
                Ok(())
            },
            |_| Err(io::Error::from(io::ErrorKind::PermissionDenied)),
        )?;
        assert!(cancel.is_cancelled());
        assert_eq!(receipt.target_path(), directory.target());
        assert_eq!(receipt.replaced_version(), DiskVersion::from_value(2));
        assert_eq!(receipt.version(), DiskVersion::from_value(3));
        assert_eq!(receipt.input_baseline_id(), &encoded_baseline);
        assert_eq!(receipt.bytes(), SAVED);
        assert!(matches!(
            receipt.durability(),
            ReplacementDurability::Uncertain(error)
                if error.kind() == io::ErrorKind::PermissionDenied
        ));
        assert_eq!(receipt.into_baseline()?.storage().bytes(), SAVED);
        assert_eq!(fs::read(directory.target())?, SAVED);
        assert!(!directory.staging().exists());
        Ok(())
    }

    /// 真实平台交换成功、目录同步失败时仍生成完整回执并清理 staging。
    ///
    /// 替换回调走 macOS `RENAME_SWAP`，仅在 `sync_parent` 接缝注入错误；用于
    /// 证明「已替换但持久性不确定」的事实分类不依赖路径 rename 替身。
    #[test]
    fn replacement_platform_swap_reports_uncertain_durability_when_directory_sync_fails()
    -> TestResult {
        use std::cell::Cell;

        let directory = TestDir::new()?;
        let cancel = Cancellation::new();
        let prepared = prepare(&directory, &cancel)?;
        let encoded_baseline = prepared.prepared.encoded.baseline_id().clone();
        let sync_calls = Cell::new(0);
        let receipt =
            prepared.publish_with(&cancel, transfer_metadata, publish_existing_checked, |_| {
                sync_calls.set(sync_calls.get() + 1);
                Err(io::Error::from(io::ErrorKind::PermissionDenied))
            })?;
        assert_eq!(sync_calls.get(), 1, "目录同步只应在交换后尝试一次");
        assert_eq!(receipt.target_path(), directory.target());
        assert_eq!(receipt.replaced_version(), DiskVersion::from_value(2));
        assert_eq!(receipt.version(), DiskVersion::from_value(3));
        assert_eq!(receipt.input_baseline_id(), &encoded_baseline);
        assert_eq!(receipt.bytes(), SAVED);
        assert!(matches!(
            receipt.durability(),
            ReplacementDurability::Uncertain(error)
                if error.kind() == io::ErrorKind::PermissionDenied
        ));
        assert!(matches!(receipt.cleanup(), ReplacementCleanup::Removed));
        assert_eq!(receipt.into_baseline()?.storage().bytes(), SAVED);
        assert_eq!(fs::read(directory.target())?, SAVED);
        assert!(!directory.staging().exists());
        Ok(())
    }

    #[test]
    fn replacement_permission_and_rename_errors_preserve_target_and_staging() -> TestResult {
        use std::cell::Cell;

        let cancel = Cancellation::new();
        let calls = Cell::new(0);
        for stage in [AtomicStage::Permissions, AtomicStage::Publish] {
            let directory = TestDir::new()?;
            let prepared = prepare(&directory, &cancel)?;
            let result = prepared.publish_with(
                &cancel,
                |_, _| {
                    calls.set(calls.get() + 1);
                    if stage == AtomicStage::Permissions {
                        return Err(io_failure(stage)(io::Error::from(
                            io::ErrorKind::PermissionDenied,
                        )));
                    }
                    Ok(())
                },
                |_, _, _, _, _, _| {
                    calls.set(calls.get() + 1);
                    Err(io_failure(stage)(io::Error::from(
                        io::ErrorKind::PermissionDenied,
                    )))
                },
                |_| {
                    calls.set(calls.get() + 1);
                    Ok(())
                },
            );
            let failure = result.err().ok_or("注入的 I/O 失败必须阻止发布")?;
            assert!(matches!(
                failure.kind(),
                AtomicFailureKind::Io { stage: actual, source }
                    if *actual == stage && source.kind() == io::ErrorKind::PermissionDenied
            ));
            assert_eq!(failure.staging_path(), Some(directory.staging().as_path()));
            assert_eq!(fs::read(directory.target())?, ORIGINAL);
            assert_eq!(fs::read(directory.staging())?, SAVED);
        }
        assert_eq!(calls.get(), 3);
        Ok(())
    }

    #[test]
    fn replacement_rechecks_target_after_permission_transfer() -> TestResult {
        let directory = TestDir::new()?;
        let cancel = Cancellation::new();
        let prepared = prepare(&directory, &cancel)?;
        let result = prepared.publish_with(
            &cancel,
            |_, _| {
                fs::write(directory.target(), b"EXTERNAL\n")
                    .map_err(io_failure(AtomicStage::Permissions))
            },
            |_, _, _, from, to, _| test_path_rename(from, to),
            File::sync_all,
        );
        let failure = result.err().ok_or("权限处理后检测到外改时必须拒绝发布")?;
        assert!(matches!(
            failure.kind(),
            AtomicFailureKind::Conflict {
                target_exists: true
            }
        ));
        assert_eq!(fs::read(directory.target())?, b"EXTERNAL\n");
        assert_eq!(fs::read(directory.staging())?, SAVED);
        Ok(())
    }

    #[test]
    fn replacement_refuses_equal_bytes_from_a_different_inode_after_staging() -> TestResult {
        let directory = TestDir::new()?;
        let cancel = Cancellation::new();
        let prepared = prepare(&directory, &cancel)?;
        let external = directory.target().with_extension("external");
        fs::write(&external, ORIGINAL)?;
        fs::rename(external, directory.target())?;
        let failure = prepared.publish(&cancel).err().ok_or("必须拒绝新 inode")?;
        assert!(matches!(
            failure.kind(),
            AtomicFailureKind::Conflict {
                target_exists: true
            }
        ));
        assert_eq!(failure.staging_path(), Some(directory.staging().as_path()));
        assert_eq!(fs::read(directory.target())?, ORIGINAL);
        assert_eq!(fs::read(directory.staging())?, SAVED);
        Ok(())
    }

    #[test]
    fn replacement_cancel_after_prepare_preserves_target_and_staging() -> TestResult {
        let directory = TestDir::new()?;
        let cancel = Cancellation::new();
        let prepared = prepare(&directory, &cancel)?;
        cancel.cancel();
        let failure = prepared.publish(&cancel).err().ok_or("必须拒绝取消发布")?;
        assert!(matches!(
            failure.kind(),
            AtomicFailureKind::Cancelled {
                stage: AtomicStage::Publish
            }
        ));
        assert_eq!(failure.staging_path(), Some(directory.staging().as_path()));
        assert_eq!(fs::read(directory.target())?, ORIGINAL);
        assert_eq!(fs::read(directory.staging())?, SAVED);
        Ok(())
    }

    #[test]
    fn replacement_platform_swap_rolls_back_when_target_changed_after_observation() -> TestResult {
        let directory = TestDir::new()?;
        fs::write(directory.target(), ORIGINAL)?;
        let target_file = File::open(directory.target())?;
        let staging_file = create_staging(&directory.staging())
            .map_err(|kind| io::Error::other(format!("{kind:?}")))?;
        fs::write(directory.staging(), SAVED)?;
        super::super::platform::transfer_access_state(&target_file, &staging_file)
            .map_err(|kind| io::Error::other(format!("{kind:?}")))?;
        // 交换前用另一 inode 顶替目标路径；调用方持有的观察句柄仍指向旧 inode。
        let external = directory.target().with_extension("external");
        fs::write(&external, ORIGINAL)?;
        fs::rename(&external, directory.target())?;
        let parent = File::open(directory.target().parent().ok_or("测试目标缺少父目录")?)?;
        let failure = super::super::platform::publish_existing(
            &parent,
            &target_file,
            &staging_file,
            &directory.staging(),
            &directory.target(),
            ORIGINAL,
        )
        .err()
        .ok_or("旧目标观察失效时必须回滚")?;
        assert!(
            matches!(
                failure,
                AtomicFailureKind::Conflict {
                    target_exists: true
                }
            ),
            "回滚后必须按外部改写结算: {failure:?}"
        );
        assert_eq!(fs::read(directory.target())?, ORIGINAL);
        assert_eq!(fs::read(directory.staging())?, SAVED);
        Ok(())
    }

    #[test]
    fn replacement_refuses_parent_swapped_during_creation_before_writing_staging() -> TestResult {
        let directory = TestDir::new()?;
        let parent = directory.target().with_file_name("parent");
        let displaced = parent.with_extension("displaced");
        fs::create_dir(&parent)?;
        let target = parent.join("note.md");
        let staging = parent.join("staging.tmp");
        let cancel = Cancellation::new();
        let (expected, encoded) = observe_and_encode(&target, &cancel)?;

        let result = prepare_checked_replace_with(
            &encoded,
            &expected,
            &staging,
            IoBudget::new(1024),
            &cancel,
            |_, path| {
                let displace = || {
                    fs::rename(&parent, &displaced).map_err(io_failure(AtomicStage::Create))?;
                    fs::create_dir(&parent).map_err(io_failure(AtomicStage::Create))
                };
                displace().map_err(super::super::platform::StagingOpenFailure::not_created)?;
                create_staging(path)
                    .map_err(super::super::platform::StagingOpenFailure::not_created)
            },
        );

        let failure = result.err().ok_or("创建窗口父目录换名后必须拒绝")?;
        assert!(matches!(
            failure.kind(),
            AtomicFailureKind::TargetDirectoryChanged
        ));
        // 创建已发生；即使未写入任何文档字节，失败也必须携带 staging 路径供清理。
        assert_eq!(failure.staging_path(), Some(staging.as_path()));
        assert!(fs::read(staging)?.is_empty());
        assert_eq!(fs::read(displaced.join("note.md"))?, ORIGINAL);
        Ok(())
    }

    #[test]
    fn replacement_never_receipts_the_expected_bytes_for_swapped_or_modified_staging() -> TestResult
    {
        use std::cell::Cell;

        const FOREIGN: &[u8] = b"foreign bytes";
        let sync_calls = Cell::new(0);
        for replace_inode in [false, true] {
            let directory = TestDir::new()?;
            let cancel = Cancellation::new();
            let prepared = prepare(&directory, &cancel)?;
            let result = prepared.publish_with(
                &cancel,
                |_, _| Ok(()),
                |_, _, _, from, to, _| {
                    if replace_inode {
                        test_path_rename(from, &from.with_extension("displaced"))?;
                    }
                    fs::write(from, FOREIGN).map_err(io_failure(AtomicStage::Publish))?;
                    test_path_rename(from, to)
                },
                |_| {
                    sync_calls.set(sync_calls.get() + 1);
                    Ok(())
                },
            );

            let failure = result
                .err()
                .ok_or("发布后无法归属写入字节时必须报告副作用")?;
            assert!(
                matches!(failure.kind(), AtomicFailureKind::PublishedTargetUnverified { target, .. } if target == &directory.target())
            );
            assert_eq!(failure.staging_path(), None);
            assert_eq!(fs::read(directory.target())?, FOREIGN);
        }
        assert_eq!(sync_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn replacement_never_syncs_an_unrelated_directory_after_a_parent_swap() -> TestResult {
        use std::cell::Cell;

        let directory = TestDir::new()?;
        let parent = directory.target().with_file_name("parent");
        let displaced = parent.with_extension("displaced");
        fs::create_dir(&parent)?;
        let target = parent.join("note.md");
        let staging = parent.join("staging.tmp");
        let cancel = Cancellation::new();
        let prepared = prepare_paths(&target, &staging, &cancel)?;
        let sync_calls = Cell::new(0);
        let result = prepared.publish_with(
            &cancel,
            |_, _| Ok(()),
            |_, _, _, from, to, _| {
                test_path_rename(&parent, &displaced)?;
                fs::create_dir(&parent).map_err(io_failure(AtomicStage::Publish))?;
                // 连 staging 文件身份与字节都沿用原句柄，单靠目标回读仍不足。
                test_path_rename(&displaced.join("staging.tmp"), from)?;
                test_path_rename(from, to)
            },
            |_| {
                sync_calls.set(sync_calls.get() + 1);
                Ok(())
            },
        );

        let failure = result
            .err()
            .ok_or("目标父目录归属无法确认时不能生成正常回执")?;
        assert!(
            matches!(failure.kind(), AtomicFailureKind::PublishedTargetUnverified { target: path, .. } if path == &target)
        );
        assert_eq!(fs::read(target)?, SAVED);
        assert_eq!(fs::read(displaced.join("note.md"))?, ORIGINAL);
        assert_eq!(sync_calls.get(), 0);
        Ok(())
    }

    #[test]
    fn replacement_platform_swap_detects_in_place_rewrite_in_final_window() -> TestResult {
        // 与 ORIGINAL 同长度，模拟复验后、交换前的原地改写。
        const REWRITTEN: &[u8] = b"\xef\xbb\xbfbravo\r\n";
        let directory = TestDir::new()?;
        fs::write(directory.target(), ORIGINAL)?;
        let target_file = File::open(directory.target())?;
        let staging_file = create_staging(&directory.staging())
            .map_err(|kind| io::Error::other(format!("{kind:?}")))?;
        fs::write(directory.staging(), SAVED)?;
        super::super::platform::transfer_access_state(&target_file, &staging_file)
            .map_err(|kind| io::Error::other(format!("{kind:?}")))?;
        fs::write(directory.target(), REWRITTEN)?;
        let parent = File::open(directory.target().parent().ok_or("测试目标缺少父目录")?)?;
        let failure = super::super::platform::publish_existing(
            &parent,
            &target_file,
            &staging_file,
            &directory.staging(),
            &directory.target(),
            ORIGINAL,
        )
        .err()
        .ok_or("最终窗口内的原地改写必须发现并回滚")?;
        assert!(
            matches!(
                failure,
                AtomicFailureKind::Conflict {
                    target_exists: true
                }
            ),
            "回滚后必须按外部改写结算: {failure:?}"
        );
        assert_eq!(fs::read(directory.target())?, REWRITTEN);
        assert_eq!(fs::read(directory.staging())?, SAVED);
        Ok(())
    }

    #[test]
    fn replacement_platform_cleanup_refuses_a_replaced_staging_path() -> TestResult {
        use std::os::unix::fs::MetadataExt as _;

        let directory = TestDir::new()?;
        let staging = create_staging(&directory.staging())
            .map_err(|kind| io::Error::other(format!("{kind:?}")))?;
        let metadata = staging.metadata()?;
        drop(staging);
        fs::remove_file(directory.staging())?;
        // 同一路径换成另一 inode；清理必须保留它而不是删除。
        fs::write(directory.staging(), b"foreign")?;
        let parent = File::open(directory.target().parent().ok_or("测试目标缺少父目录")?)?;
        let result = super::super::platform::cleanup_staging(
            &parent,
            &directory.staging(),
            (metadata.dev(), metadata.ino()),
        );
        assert!(result.is_err(), "身份不符时不得删除 staging 路径");
        assert_eq!(fs::read(directory.staging())?, b"foreign");
        Ok(())
    }
}
