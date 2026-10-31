//! create-only 受检文件发布：同目录 staging 完整写入并 sync 后，以硬链接发布到新目标。
//!
//! 本模块只提供 M0 的最小受检原语：prepare 阶段独占创建 staging、完整写入并
//! `sync_all`；publish 阶段用 `hard_link` 将 staging 发布到尚不存在的目标。
//! 目标已存在时按类型拒绝，不 rename、不截断、不原地覆盖，也不把能力未验证
//! 伪称 OS Unsupported。现有文件 metadata/ACL/xattr、目录同步、断电、跨进程
//! CAS 与 Windows/Linux 行为不在本模块的已验证范围。
//!
//! staging 由调用方显式命名；成功与失败都不在 Drop 中静默删除，结果携带本操作
//! 拥有的 staging 路径供宿主取证或后续清理。发布后不在本模块实现目录同步。
//! 路径前提只做词法比较（不 canonicalize），同一目录的等价别名可能被拒绝。
//! publish 前的身份检查只比较 staging 路径与已打开句柄的 dev/ino；检查之后外部
//! 进程仍可原地改写同一 inode，M0 不在发布后回读目标核对，因此 [`Written::bytes`]
//! 报告的是本操作写入的 bytes，而不是目标的事后内容。

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::baseline::SourceBaseline;
use crate::decode::{DecodeError, decode};
use crate::encode::EncodedBytes;
use crate::storage::{BaselineId, DiskVersion, StorageBudget, StorageError, StorageSnapshot};

const WRITE_CHUNK_BYTES: usize = 64 * 1024;

/// 可复制的取消令牌；只由宿主在同步调用前或调用间设置。
#[derive(Clone, Debug, Default)]
pub struct Cancellation {
    cancelled: Arc<AtomicBool>,
}

impl Cancellation {
    /// 创建未取消令牌。
    pub fn new() -> Self {
        Self::default()
    }

    /// 标记取消；已发布的目标不会被回滚。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    /// 返回当前是否已取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

/// 单次 staging 写入的注入字节预算；不是磁盘容量或发布大小保证。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct IoBudget {
    max_bytes: usize,
}

impl IoBudget {
    /// 构造字节上限。
    pub const fn new(max_bytes: usize) -> Self {
        Self { max_bytes }
    }

    /// 返回字节上限。
    pub const fn max_bytes(self) -> usize {
        self.max_bytes
    }
}

/// 受检文件操作阶段；用于区分 I/O 与取消发生在哪一步。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AtomicStage {
    /// 参数、预算或取消的前置检查。
    Prepare,
    /// 独占创建 staging。
    Create,
    /// 分块写入 staging。
    Write,
    /// 对 staging 调用 `sync_all`。
    Sync,
    /// 检查并发布新目标。
    Publish,
}

/// prepare 的路径输入不满足同目录、独占新文件等前提。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvalidInput {
    /// target 与 staging 是同一路径。
    SamePath,
    /// 路径不是绝对路径；本原语不解析相对路径语义。
    RelativePath,
    /// 路径没有父目录，无法校验同目录前提。
    MissingParent,
    /// target 与 staging 不在同一父目录。
    DifferentDirectory,
}

/// 已存在目标中不属于普通文件、因此明确不适用 create-only 发布的类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExistingTargetKind {
    /// 符号链接，包括悬空链接。
    Symlink,
    /// 目录。
    Directory,
    /// 其他非普通文件，例如设备、FIFO 或套接字。
    Other,
}

/// 受检文件原语的失败分类；不包含已翻译的用户文案。
#[derive(Debug)]
pub enum AtomicFailureKind {
    /// 待写 bytes 超过注入预算。
    Budget {
        /// 需要的字节数。
        needed: usize,
        /// 预算上限。
        limit: usize,
    },
    /// 路径前提不满足。
    InvalidInput {
        /// 不满足的具体前提。
        reason: InvalidInput,
    },
    /// 指定阶段的 I/O 失败；保留原始 `io::Error` 分类。
    Io {
        /// 失败阶段。
        stage: AtomicStage,
        /// 原始 I/O 错误。
        source: io::Error,
    },
    /// 发布前取消；不会生成 [`Written`]。
    Cancelled {
        /// 检测到取消的阶段。
        stage: AtomicStage,
    },
    /// `hard_link` 发现目标已存在，包括检查后竞争出现；不覆盖该目标。
    Conflict {
        /// 目标是否存在；当前发布路径只会产生 `true`。
        target_exists: bool,
    },
    /// 目标已存在且是普通文件；现有文件完整 metadata 替换能力未实现。
    MetadataPreservationUnverified,
    /// 目标已存在但不是普通文件；create-only 发布不适用。
    ExistingTarget {
        /// 已存在目标的类别。
        kind: ExistingTargetKind,
    },
    /// staging 路径在 prepare 后被替换，不再指向本操作持有的 inode。
    ///
    /// 非 Unix 目标尚未验证句柄身份比较；当前实现恒拒绝发布并返回本错误。
    StagingReplaced,
}

/// 受检文件原语失败；若本操作曾创建 staging 且未观察到路径被替换，则携带调用方
/// 提供的路径。该路径不保证当前仍存在，也不保证仍指向本操作写入的 inode。
#[derive(Debug)]
pub struct AtomicFailure {
    kind: AtomicFailureKind,
    staging: Option<PathBuf>,
}

impl AtomicFailure {
    fn new(kind: AtomicFailureKind, staging: Option<PathBuf>) -> Self {
        Self { kind, staging }
    }

    /// 返回失败分类。
    pub fn kind(&self) -> &AtomicFailureKind {
        &self.kind
    }

    /// 返回本操作记录拥有的 staging 路径；仅在 staging 已创建且未观察到替换时存在。
    ///
    /// M0 不重新验证路径当前是否存在或仍指向原 inode；外部删除后仍可能返回该路径。
    pub fn staging_path(&self) -> Option<&Path> {
        self.staging.as_deref()
    }
}

impl fmt::Display for AtomicFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

// 不覆写 `Error::source`：安全守卫把 trait object 生命周期中的 `'static`
// 词法误判为可变静态；原始错误仍由 `kind`/枚举变体持有并可读。
impl std::error::Error for AtomicFailure {}

/// 从 [`Written`] 建立新基线时的防御性失败；正常写出的 bytes 应已通过解码。
#[derive(Debug)]
pub enum BaselineError {
    /// 建立存储快照失败；正常写出路径不应触发预算失败。
    Storage(StorageError),
    /// 解码写出的 bytes 失败；表示内部不变量被破坏。
    Decode(DecodeError),
}

impl fmt::Display for BaselineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

// 与 `AtomicFailure` 相同，不覆写 `Error::source` 以避开安全守卫的词法误报；
// 存储/解码错误仍由公开变体持有。
impl std::error::Error for BaselineError {}

impl From<StorageError> for BaselineError {
    fn from(error: StorageError) -> Self {
        Self::Storage(error)
    }
}

impl From<DecodeError> for BaselineError {
    fn from(error: DecodeError) -> Self {
        Self::Decode(error)
    }
}

/// 已完成 staging 完整写入与 sync 的准备结果；尚未发布目标。
pub struct PreparedAtomic {
    encoded: EncodedBytes,
    target: PathBuf,
    staging: PathBuf,
    file: File,
}

impl fmt::Debug for PreparedAtomic {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedAtomic")
            .field("target", &self.target)
            .field("staging", &self.staging)
            .field("bytes_len", &self.encoded.bytes().len())
            .finish()
    }
}

impl PreparedAtomic {
    /// 返回调用方提供的 staging 路径；该路径当前由本操作拥有。
    pub fn staging_path(&self) -> &Path {
        &self.staging
    }

    /// 返回调用方提供的目标路径。
    pub fn target_path(&self) -> &Path {
        &self.target
    }

    /// 以 `hard_link` 发布到尚不存在的目标；消费准备结果。
    ///
    /// 目标已存在时拒绝并且不修改它；检查后竞争出现的目标同样不被覆盖。
    /// 成功只表示目标已指向完整写入且已 sync 的文件内容；目录持久性、清理与
    /// 跨平台行为未验证。
    pub fn publish_new(self, cancellation: &Cancellation) -> Result<Written, AtomicFailure> {
        self.publish_new_with_hook(cancellation, || {})
    }

    fn publish_new_with_hook<F: FnOnce()>(
        self,
        cancellation: &Cancellation,
        after_target_check: F,
    ) -> Result<Written, AtomicFailure> {
        let PreparedAtomic {
            encoded,
            target,
            staging,
            file,
        } = self;
        if cancellation.is_cancelled() {
            return Err(AtomicFailure::new(
                AtomicFailureKind::Cancelled {
                    stage: AtomicStage::Publish,
                },
                Some(staging),
            ));
        }
        let staging_metadata = match std::fs::symlink_metadata(&staging) {
            Ok(metadata) => metadata,
            Err(source) => {
                return Err(AtomicFailure::new(
                    AtomicFailureKind::Io {
                        stage: AtomicStage::Publish,
                        source,
                    },
                    Some(staging),
                ));
            }
        };
        let owned_metadata = match file.metadata() {
            Ok(metadata) => metadata,
            Err(source) => {
                return Err(AtomicFailure::new(
                    AtomicFailureKind::Io {
                        stage: AtomicStage::Publish,
                        source,
                    },
                    Some(staging),
                ));
            }
        };
        if !same_file_identity(&staging_metadata, &owned_metadata) {
            return Err(AtomicFailure::new(AtomicFailureKind::StagingReplaced, None));
        }
        match std::fs::symlink_metadata(&target) {
            Ok(metadata) => {
                let kind = if metadata.file_type().is_symlink() {
                    ExistingTargetKind::Symlink
                } else if metadata.is_dir() {
                    ExistingTargetKind::Directory
                } else if metadata.is_file() {
                    return Err(AtomicFailure::new(
                        AtomicFailureKind::MetadataPreservationUnverified,
                        Some(staging),
                    ));
                } else {
                    ExistingTargetKind::Other
                };
                return Err(AtomicFailure::new(
                    AtomicFailureKind::ExistingTarget { kind },
                    Some(staging),
                ));
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(AtomicFailure::new(
                    AtomicFailureKind::Io {
                        stage: AtomicStage::Publish,
                        source,
                    },
                    Some(staging),
                ));
            }
        }
        after_target_check();
        match std::fs::hard_link(&staging, &target) {
            Ok(()) => Ok(Written {
                encoded,
                target,
                staging,
            }),
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                Err(AtomicFailure::new(
                    AtomicFailureKind::Conflict {
                        target_exists: true,
                    },
                    Some(staging),
                ))
            }
            Err(source) => Err(AtomicFailure::new(
                AtomicFailureKind::Io {
                    stage: AtomicStage::Publish,
                    source,
                },
                Some(staging),
            )),
        }
    }
}

/// 已通过 `hard_link` 发布的新目标事实；目录持久性与 staging 清理未完成。
pub struct Written {
    encoded: EncodedBytes,
    target: PathBuf,
    staging: PathBuf,
}

impl fmt::Debug for Written {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Written")
            .field("target", &self.target)
            .field("staging", &self.staging)
            .field("bytes_len", &self.encoded.bytes().len())
            .finish()
    }
}

impl Written {
    /// 返回新目标路径。
    pub fn target_path(&self) -> &Path {
        &self.target
    }

    /// 返回本操作保留的 staging 路径；发布后它是目标的另一硬链接。
    pub fn staging_path(&self) -> &Path {
        &self.staging
    }

    /// 返回本操作完整写入的 bytes；正常路径下与 staging/target 内容一致。
    ///
    /// 身份检查后外部进程仍可原地改写同一 inode；M0 不在发布后回读目标核对，
    /// 因此本方法与 [`Written::into_baseline`] 报告的是本操作写入的 bytes。
    pub fn bytes(&self) -> &[u8] {
        self.encoded.bytes()
    }

    /// 返回输入编码所依据的基线身份；宿主据此拒绝把较新输入标为已保存。
    pub fn input_baseline_id(&self) -> &BaselineId {
        self.encoded.baseline_id()
    }

    /// 返回写入的字节数。
    pub fn written_len(&self) -> usize {
        self.encoded.bytes().len()
    }

    /// 返回目标目录持久性尚未验证；M0 不执行目录同步。
    pub const fn directory_durability_unverified(&self) -> bool {
        true
    }

    /// 从本操作完整写入并经 `hard_link` 发布的 encoded bytes 建立新基线。
    ///
    /// 不重新读取目标；身份检查后外部原地改写同一 inode 不会被发现，因此新基线
    /// 反映本操作写入的 bytes，而非对当前目标内容的事后核对。
    pub fn into_baseline(self, version: DiskVersion) -> Result<SourceBaseline, BaselineError> {
        let budget = StorageBudget::new(usize::MAX);
        let storage = StorageSnapshot::new(self.encoded.bytes().to_vec(), version, budget)?;
        Ok(decode(&storage, budget)?.into_baseline())
    }
}

/// 创建同目录专属 staging，完整写入并 sync；不修改目标。
///
/// 路径必须是绝对路径、同父目录且不相同；staging 使用 `create_new`，已存在的
/// 路径不会被覆盖、截断、删除或复用。成功返回的准备结果仍持有 staging 的
/// `File` 句柄；失败不会生成成功基线。
pub fn prepare_atomic(
    encoded: &EncodedBytes,
    target: &Path,
    staging: &Path,
    budget: IoBudget,
    cancellation: &Cancellation,
) -> Result<PreparedAtomic, AtomicFailure> {
    let file = prepare_atomic_with(encoded, target, staging, budget, cancellation, open_staging)?;
    Ok(PreparedAtomic {
        encoded: encoded.clone(),
        target: target.to_path_buf(),
        staging: staging.to_path_buf(),
        file,
    })
}

/// 生产 prepare 的注入缝：前置检查与失败映射固定，只有打开 staging 的句柄可由测试替换。
///
/// 单元测试用真实 staging `File` 的包装器走完整 prepare，验证写入/同步失败时目标
/// 未被提前触碰、staging 路径随失败返回且 sync 未被误调用。
fn prepare_atomic_with<W, F>(
    encoded: &EncodedBytes,
    target: &Path,
    staging: &Path,
    budget: IoBudget,
    cancellation: &Cancellation,
    open: F,
) -> Result<W, AtomicFailure>
where
    W: StagingWriter,
    F: FnOnce(&Path) -> io::Result<W>,
{
    let needed = encoded.bytes().len();
    if needed > budget.max_bytes() {
        return Err(AtomicFailure::new(
            AtomicFailureKind::Budget {
                needed,
                limit: budget.max_bytes(),
            },
            None,
        ));
    }
    if let Err(reason) = validate_paths(target, staging) {
        return Err(AtomicFailure::new(
            AtomicFailureKind::InvalidInput { reason },
            None,
        ));
    }
    if cancellation.is_cancelled() {
        return Err(AtomicFailure::new(
            AtomicFailureKind::Cancelled {
                stage: AtomicStage::Prepare,
            },
            None,
        ));
    }
    let mut writer = match open(staging) {
        Ok(writer) => writer,
        Err(source) => {
            return Err(AtomicFailure::new(
                AtomicFailureKind::Io {
                    stage: AtomicStage::Create,
                    source,
                },
                None,
            ));
        }
    };
    write_staging_owned(&mut writer, encoded.bytes(), staging, budget, cancellation)?;
    Ok(writer)
}

fn write_staging_owned<W: StagingWriter>(
    writer: &mut W,
    bytes: &[u8],
    staging: &Path,
    budget: IoBudget,
    cancellation: &Cancellation,
) -> Result<(), AtomicFailure> {
    match write_staging(writer, bytes, budget, cancellation) {
        Ok(()) => Ok(()),
        Err(StagingFailure::Budget { needed, limit }) => Err(AtomicFailure::new(
            AtomicFailureKind::Budget { needed, limit },
            Some(staging.to_path_buf()),
        )),
        Err(StagingFailure::Cancelled { stage }) => Err(AtomicFailure::new(
            AtomicFailureKind::Cancelled { stage },
            Some(staging.to_path_buf()),
        )),
        Err(StagingFailure::Io { stage, source }) => Err(AtomicFailure::new(
            AtomicFailureKind::Io { stage, source },
            Some(staging.to_path_buf()),
        )),
    }
}

/// 词法路径前提检查；不 canonicalize，也不解析符号链接父目录。
fn validate_paths(target: &Path, staging: &Path) -> Result<(), InvalidInput> {
    if target == staging {
        return Err(InvalidInput::SamePath);
    }
    if !target.is_absolute() || !staging.is_absolute() {
        return Err(InvalidInput::RelativePath);
    }
    let target_parent = target.parent().ok_or(InvalidInput::MissingParent)?;
    let staging_parent = staging.parent().ok_or(InvalidInput::MissingParent)?;
    if target_parent != staging_parent {
        return Err(InvalidInput::DifferentDirectory);
    }
    Ok(())
}

fn open_staging(path: &Path) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

trait StagingWriter: Write {
    fn sync_all(&self) -> io::Result<()>;
}

impl StagingWriter for File {
    fn sync_all(&self) -> io::Result<()> {
        File::sync_all(self)
    }
}

#[derive(Debug)]
enum StagingFailure {
    Budget {
        needed: usize,
        limit: usize,
    },
    Cancelled {
        stage: AtomicStage,
    },
    Io {
        stage: AtomicStage,
        source: io::Error,
    },
}

fn write_staging<W: StagingWriter>(
    writer: &mut W,
    bytes: &[u8],
    budget: IoBudget,
    cancellation: &Cancellation,
) -> Result<(), StagingFailure> {
    let needed = bytes.len();
    if needed > budget.max_bytes() {
        return Err(StagingFailure::Budget {
            needed,
            limit: budget.max_bytes(),
        });
    }
    for chunk in bytes.chunks(WRITE_CHUNK_BYTES) {
        let mut offset = 0usize;
        while offset < chunk.len() {
            if cancellation.is_cancelled() {
                return Err(StagingFailure::Cancelled {
                    stage: AtomicStage::Write,
                });
            }
            let remaining = chunk.get(offset..).ok_or_else(|| StagingFailure::Io {
                stage: AtomicStage::Write,
                source: io::Error::other("写入偏移越界"),
            })?;
            match writer.write(remaining) {
                Ok(0) => {
                    return Err(StagingFailure::Io {
                        stage: AtomicStage::Write,
                        source: io::Error::from(io::ErrorKind::WriteZero),
                    });
                }
                Ok(written) => {
                    if written > remaining.len() {
                        return Err(StagingFailure::Io {
                            stage: AtomicStage::Write,
                            source: io::Error::other("写入长度超过缓冲区"),
                        });
                    }
                    offset = offset
                        .checked_add(written)
                        .ok_or_else(|| StagingFailure::Io {
                            stage: AtomicStage::Write,
                            source: io::Error::other("写入偏移溢出"),
                        })?;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(source) => {
                    return Err(StagingFailure::Io {
                        stage: AtomicStage::Write,
                        source,
                    });
                }
            }
        }
    }
    if cancellation.is_cancelled() {
        return Err(StagingFailure::Cancelled {
            stage: AtomicStage::Sync,
        });
    }
    writer.sync_all().map_err(|source| StagingFailure::Io {
        stage: AtomicStage::Sync,
        source,
    })
}

#[cfg(unix)]
fn same_file_identity(left: &std::fs::Metadata, right: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    left.dev() == right.dev() && left.ino() == right.ino()
}

#[cfg(not(unix))]
fn same_file_identity(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    // M0 未验证非 Unix 目标；恒拒绝发布而不伪造身份相等。
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::error::Error;
    use std::fs;
    use std::rc::Rc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::decode::decode;
    use crate::encode::encode;
    use crate::storage::{DiskVersion, StorageBudget, StorageSnapshot};

    type TestResult<T = ()> = Result<T, Box<dyn Error>>;

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new() -> TestResult<Self> {
            let base = std::env::temp_dir();
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            for attempt in 0..100u32 {
                let path = base.join(format!(
                    "aline-atomic-unit-{}-{nanos}-{attempt}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self { path }),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Err(io::Error::other("无法创建单元测试目录").into())
        }

        fn target(&self) -> PathBuf {
            self.path.join("target.md")
        }

        fn staging(&self) -> PathBuf {
            self.path.join("staging.tmp")
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn encoded(bytes: &[u8]) -> TestResult<EncodedBytes> {
        let budget = StorageBudget::new(usize::MAX);
        let storage = StorageSnapshot::new(bytes.to_vec(), DiskVersion::initial(), budget)?;
        let baseline = decode(&storage, budget)?.into_baseline();
        Ok(encode(baseline.source(), &baseline, budget)?)
    }

    struct FailingWriter {
        remaining: usize,
        written: usize,
    }

    impl Write for FailingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::other("注入写入失败"));
            }
            let count = buffer.len().min(self.remaining);
            self.remaining -= count;
            self.written += count;
            Ok(count)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StagingWriter for FailingWriter {
        fn sync_all(&self) -> io::Result<()> {
            Ok(())
        }
    }

    struct FailingSyncWriter {
        written: usize,
    }

    impl Write for FailingSyncWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.written += buffer.len();
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StagingWriter for FailingSyncWriter {
        fn sync_all(&self) -> io::Result<()> {
            Err(io::Error::other("注入同步失败"))
        }
    }

    struct CancellingWriter {
        cancellation: Cancellation,
        writes: usize,
    }

    impl Write for CancellingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.writes += 1;
            self.cancellation.cancel();
            Ok(buffer.len().min(1))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StagingWriter for CancellingWriter {
        fn sync_all(&self) -> io::Result<()> {
            Ok(())
        }
    }

    struct InterruptingWriter {
        interrupted: bool,
        written: usize,
    }

    impl Write for InterruptingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if !self.interrupted {
                self.interrupted = true;
                return Err(io::Error::from(io::ErrorKind::Interrupted));
            }
            self.written += buffer.len();
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StagingWriter for InterruptingWriter {
        fn sync_all(&self) -> io::Result<()> {
            Ok(())
        }
    }

    struct CancelOnCompleteWriter {
        cancellation: Cancellation,
        written: usize,
    }

    impl Write for CancelOnCompleteWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            self.written += buffer.len();
            self.cancellation.cancel();
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StagingWriter for CancelOnCompleteWriter {
        fn sync_all(&self) -> io::Result<()> {
            Ok(())
        }
    }

    struct PartialFileWriter {
        file: File,
        remaining: usize,
        written: Rc<Cell<usize>>,
        sync_calls: Rc<Cell<usize>>,
    }

    impl Write for PartialFileWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if self.remaining == 0 {
                return Err(io::Error::other("注入写入失败"));
            }
            let count = buffer.len().min(self.remaining);
            let written = self.file.write(&buffer[..count])?;
            self.remaining -= written;
            self.written.set(self.written.get() + written);
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    impl StagingWriter for PartialFileWriter {
        fn sync_all(&self) -> io::Result<()> {
            self.sync_calls.set(self.sync_calls.get() + 1);
            self.file.sync_all()
        }
    }

    struct SyncFailingFileWriter {
        file: File,
        written: Rc<Cell<usize>>,
        sync_calls: Rc<Cell<usize>>,
    }

    impl Write for SyncFailingFileWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            let written = self.file.write(buffer)?;
            self.written.set(self.written.get() + written);
            Ok(written)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    impl StagingWriter for SyncFailingFileWriter {
        fn sync_all(&self) -> io::Result<()> {
            self.sync_calls.set(self.sync_calls.get() + 1);
            Err(io::Error::other("注入同步失败"))
        }
    }

    struct ZeroWriter {
        sync_calls: Cell<usize>,
    }

    impl Write for ZeroWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Ok(0)
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl StagingWriter for ZeroWriter {
        fn sync_all(&self) -> io::Result<()> {
            self.sync_calls.set(self.sync_calls.get() + 1);
            Ok(())
        }
    }

    #[test]
    fn atomic_partial_write_failure_reports_write_stage_without_sync() -> TestResult {
        let cancellation = Cancellation::new();
        let mut writer = FailingWriter {
            remaining: 3,
            written: 0,
        };
        let failure = match write_staging(
            &mut writer,
            b"abcdef",
            IoBudget::new(usize::MAX),
            &cancellation,
        ) {
            Ok(()) => return Err(io::Error::other("应注入写入失败").into()),
            Err(failure) => failure,
        };
        match failure {
            StagingFailure::Io {
                stage: AtomicStage::Write,
                source,
            } => assert_eq!(source.kind(), io::ErrorKind::Other),
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(writer.written, 3);
        Ok(())
    }

    #[test]
    fn atomic_sync_failure_reports_sync_stage_after_full_write() -> TestResult {
        let cancellation = Cancellation::new();
        let mut writer = FailingSyncWriter { written: 0 };
        let failure = match write_staging(
            &mut writer,
            b"abcdef",
            IoBudget::new(usize::MAX),
            &cancellation,
        ) {
            Ok(()) => return Err(io::Error::other("应注入同步失败").into()),
            Err(failure) => failure,
        };
        match failure {
            StagingFailure::Io {
                stage: AtomicStage::Sync,
                source,
            } => assert_eq!(source.kind(), io::ErrorKind::Other),
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(writer.written, 6);
        Ok(())
    }

    #[test]
    fn atomic_existing_target_survives_staging_partial_write_failure() -> TestResult {
        let dir = TestDir::new()?;
        let target = dir.target();
        let staging = dir.staging();
        let sentinel = b"TARGET-ORIGINAL\n";
        fs::write(&target, sentinel)?;
        let encoded = encoded(b"ABCDEF\n")?;
        let written = Rc::new(Cell::new(0usize));
        let sync_calls = Rc::new(Cell::new(0usize));
        let failure = match prepare_atomic_with(
            &encoded,
            &target,
            &staging,
            IoBudget::new(usize::MAX),
            &Cancellation::new(),
            |path| {
                let file = open_staging(path)?;
                Ok(PartialFileWriter {
                    file,
                    remaining: 3,
                    written: Rc::clone(&written),
                    sync_calls: Rc::clone(&sync_calls),
                })
            },
        ) {
            Ok(_) => return Err(io::Error::other("应注入部分写入失败").into()),
            Err(failure) => failure,
        };
        assert_eq!(written.get(), 3);
        assert_eq!(sync_calls.get(), 0, "部分写入失败后不得调用 sync");
        match failure.kind() {
            AtomicFailureKind::Io {
                stage: AtomicStage::Write,
                source,
            } => assert_eq!(source.kind(), io::ErrorKind::Other),
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(failure.staging_path(), Some(staging.as_path()));
        assert_eq!(fs::read(&target)?, sentinel, "目标必须保持原 bytes");
        assert_eq!(
            fs::read(&staging)?.as_slice(),
            &encoded.bytes()[..3],
            "staging 只保留已写入前缀"
        );
        Ok(())
    }

    #[test]
    fn atomic_existing_target_survives_staging_sync_failure_after_full_write() -> TestResult {
        let dir = TestDir::new()?;
        let target = dir.target();
        let staging = dir.staging();
        let sentinel = b"TARGET-ORIGINAL\n";
        fs::write(&target, sentinel)?;
        let encoded = encoded(b"ABCDEF\n")?;
        let written = Rc::new(Cell::new(0usize));
        let sync_calls = Rc::new(Cell::new(0usize));
        let failure = match prepare_atomic_with(
            &encoded,
            &target,
            &staging,
            IoBudget::new(usize::MAX),
            &Cancellation::new(),
            |path| {
                let file = open_staging(path)?;
                Ok(SyncFailingFileWriter {
                    file,
                    written: Rc::clone(&written),
                    sync_calls: Rc::clone(&sync_calls),
                })
            },
        ) {
            Ok(_) => return Err(io::Error::other("应注入同步失败").into()),
            Err(failure) => failure,
        };
        assert_eq!(written.get(), encoded.bytes().len());
        assert_eq!(sync_calls.get(), 1, "完整写入后必须尝试且只尝试一次 sync");
        match failure.kind() {
            AtomicFailureKind::Io {
                stage: AtomicStage::Sync,
                source,
            } => assert_eq!(source.kind(), io::ErrorKind::Other),
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(failure.staging_path(), Some(staging.as_path()));
        assert_eq!(fs::read(&target)?, sentinel, "目标必须保持原 bytes");
        assert_eq!(
            fs::read(&staging)?,
            encoded.bytes(),
            "staging 保留完整写入 bytes"
        );
        Ok(())
    }

    #[test]
    fn atomic_zero_write_is_rejected_as_write_zero_without_sync() -> TestResult {
        let cancellation = Cancellation::new();
        let mut writer = ZeroWriter {
            sync_calls: Cell::new(0),
        };
        let failure = match write_staging(
            &mut writer,
            b"abcdef",
            IoBudget::new(usize::MAX),
            &cancellation,
        ) {
            Ok(()) => return Err(io::Error::other("零字节写入应失败").into()),
            Err(failure) => failure,
        };
        match failure {
            StagingFailure::Io {
                stage: AtomicStage::Write,
                source,
            } => assert_eq!(source.kind(), io::ErrorKind::WriteZero),
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(writer.sync_calls.get(), 0, "零字节写入失败后不得调用 sync");
        Ok(())
    }

    #[test]
    fn atomic_write_cancellation_stops_before_next_write() -> TestResult {
        let cancellation = Cancellation::new();
        let mut writer = CancellingWriter {
            cancellation: cancellation.clone(),
            writes: 0,
        };
        let failure = match write_staging(
            &mut writer,
            b"abcdef",
            IoBudget::new(usize::MAX),
            &cancellation,
        ) {
            Ok(()) => return Err(io::Error::other("应注入取消").into()),
            Err(failure) => failure,
        };
        match failure {
            StagingFailure::Cancelled {
                stage: AtomicStage::Write,
            } => {}
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(writer.writes, 1);
        Ok(())
    }

    #[test]
    fn atomic_interrupted_write_is_retried_without_losing_bytes() -> TestResult {
        let cancellation = Cancellation::new();
        let mut writer = InterruptingWriter {
            interrupted: false,
            written: 0,
        };
        write_staging(
            &mut writer,
            b"abcdef",
            IoBudget::new(usize::MAX),
            &cancellation,
        )
        .map_err(|failure| io::Error::other(format!("不应失败: {failure:?}")))?;
        assert!(writer.interrupted);
        assert_eq!(writer.written, 6);
        Ok(())
    }

    #[test]
    fn atomic_cancellation_after_complete_write_stops_before_sync() -> TestResult {
        let cancellation = Cancellation::new();
        let mut writer = CancelOnCompleteWriter {
            cancellation: cancellation.clone(),
            written: 0,
        };
        let failure = match write_staging(
            &mut writer,
            b"abcdef",
            IoBudget::new(usize::MAX),
            &cancellation,
        ) {
            Ok(()) => return Err(io::Error::other("应注入同步前取消").into()),
            Err(failure) => failure,
        };
        match failure {
            StagingFailure::Cancelled {
                stage: AtomicStage::Sync,
            } => {}
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(writer.written, 6);
        Ok(())
    }

    #[test]
    fn atomic_write_budget_rejects_before_any_write() -> TestResult {
        let cancellation = Cancellation::new();
        let mut writer = FailingSyncWriter { written: 0 };
        let failure = match write_staging(&mut writer, b"abcdef", IoBudget::new(2), &cancellation) {
            Ok(()) => return Err(io::Error::other("应拒绝预算").into()),
            Err(failure) => failure,
        };
        match failure {
            StagingFailure::Budget { needed, limit } => {
                assert_eq!(needed, 6);
                assert_eq!(limit, 2);
            }
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(writer.written, 0);
        Ok(())
    }

    #[test]
    fn atomic_target_race_after_check_is_conflict_and_preserves_racer_bytes() -> TestResult {
        let dir = TestDir::new()?;
        let target = dir.target();
        let staging = dir.staging();
        let encoded = encoded(b"A\n")?;
        let prepared = prepare_atomic(
            &encoded,
            &target,
            &staging,
            IoBudget::new(usize::MAX),
            &Cancellation::new(),
        )?;
        let failure = match prepared.publish_new_with_hook(&Cancellation::new(), || {
            assert!(fs::write(&target, b"RACER\n").is_ok());
        }) {
            Ok(_) => return Err(io::Error::other("竞争目标应被拒绝").into()),
            Err(failure) => failure,
        };
        match failure.kind() {
            AtomicFailureKind::Conflict { target_exists } => assert!(*target_exists),
            other => return Err(io::Error::other(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(failure.staging_path(), Some(staging.as_path()));
        assert_eq!(fs::read(&target)?, b"RACER\n");
        assert_eq!(fs::read(&staging)?, encoded.bytes());
        Ok(())
    }
}
