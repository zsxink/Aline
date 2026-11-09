//! 只读文件句柄适配与有界读取；生产接口仅提供 open/read/metadata，没有写能力。

use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::Path;

use aline_buffer::SelectionSpec;
use aline_markdown::parse_with_cancel;
use aline_source::{DiskVersion, StorageSnapshot, decode};

use super::{
    DocCore, OpenError, OpenTimings, ReadonlyDocument, ReadonlyOpenCompletion, ReadonlyOpenJob,
    StageTimer,
};

const READ_CHUNK_BYTES: usize = 16 * 1024;

/// 已打开读取器及从同一句柄取得的文件类型/长度事实；不代表内容不会外改。
pub struct ReadonlyFileInput<R: Read> {
    reader: R,
    reported_len: u64,
    regular: bool,
}
impl<R: Read> ReadonlyFileInput<R> {
    /// 从实际文件适配器或故障注入读取器构造受限输入。
    pub const fn new(reader: R, reported_len: u64, regular: bool) -> Self {
        Self {
            reader,
            reported_len,
            regular,
        }
    }
}

/// 文件读取的真实适配边界；一份本地实现加故障读取器支持无窗口验证。
pub trait ReadonlyFileAccess {
    /// 仅需标准 Read 能力的输入，不提供写文件接口。
    type Reader: Read;
    /// 打开选择路径并从同一句柄读取长度与类型；错误由 UI 翻译。
    fn open(&self, path: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>>;
}

/// 本地普通文件读取适配；不创建、不截断、不改权限或写回文件。
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct LocalReadonlyFiles;
impl ReadonlyFileAccess for LocalReadonlyFiles {
    type Reader = File;
    fn open(&self, path: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        let observed = std::fs::metadata(path)?;
        if !observed.is_file() {
            return Err(io::Error::from(io::ErrorKind::InvalidInput));
        }
        open_after_regular_check(path, &observed)
    }
}

/// 路径预检后仍以同句柄复验，Unix 非阻塞打开避免被换成 FIFO 时卡在 open 内。
fn open_after_regular_check(
    path: &Path,
    observed: &Metadata,
) -> io::Result<ReadonlyFileInput<File>> {
    let reader = open_without_fifo_wait(path)?;
    let opened = reader.metadata()?;
    if !opened.is_file() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if observed.dev() != opened.dev() || observed.ino() != opened.ino() {
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    }
    #[cfg(not(unix))]
    let _ = observed;
    Ok(ReadonlyFileInput::new(reader, opened.len(), true))
}

#[cfg(target_os = "macos")]
fn open_without_fifo_wait(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    // Darwin fcntl.h 的 O_NONBLOCK ABI 值；通过 std 安全接口传入，不使用自有 FFI。
    // https://github.com/apple-oss-distributions/xnu/blob/main/bsd/sys/fcntl.h
    const NONBLOCK: i32 = 0x0004;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(NONBLOCK)
        .open(path)
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
fn open_without_fifo_wait(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    // 这两个目标的 O_NONBLOCK 取 Linux asm-generic ABI 值 00004000。
    // https://github.com/torvalds/linux/blob/v6.12/include/uapi/asm-generic/fcntl.h
    const NONBLOCK: i32 = 0x0800;
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(NONBLOCK)
        .open(path)
}

#[cfg(all(
    unix,
    not(any(
        target_os = "macos",
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    ))
))]
fn open_without_fifo_wait(_path: &Path) -> io::Result<File> {
    // 未核对 ABI 的 Unix 目标失败关闭，不能退回可能阻塞 FIFO 的普通 open。
    Err(io::Error::from(io::ErrorKind::Unsupported))
}

#[cfg(not(unix))]
fn open_without_fifo_wait(path: &Path) -> io::Result<File> {
    std::fs::OpenOptions::new().read(true).open(path)
}

impl ReadonlyOpenJob {
    /// 对本地文件完整执行受限读取、解码及无头解析；必须后台调用，随后交回会话采纳。
    /// 取消只在 OS 调用前后及解析检查点生效；宿主须限制并发，不假定可中断阻塞 read。
    pub fn run(self) -> ReadonlyOpenCompletion {
        self.read_with(&LocalReadonlyFiles)
    }

    /// 经指定只读适配器执行同一流程；取消/错误均不返回部分文档。
    pub fn read_with(self, files: &impl ReadonlyFileAccess) -> ReadonlyOpenCompletion {
        let (result, timings) = match self.build_candidate(files) {
            Ok((document, timings)) => (Ok(document), timings),
            Err(error) => (Err(error), OpenTimings::default()),
        };
        ReadonlyOpenCompletion {
            request: self.request,
            result,
            timings,
        }
    }

    /// 返回候选文档与三段真实耗时；失败时不产生部分文档，也不报计时。
    fn build_candidate(
        &self,
        files: &impl ReadonlyFileAccess,
    ) -> Result<(ReadonlyDocument, OpenTimings), OpenError> {
        let read_started = StageTimer::start();
        self.check_cancel()?;
        let mut input = files
            .open(&self.path)
            .map_err(|error| OpenError::Io(error.kind()))?;
        if !input.regular {
            return Err(OpenError::NotRegularFile);
        }
        let limit = self.limits.storage.max_bytes();
        if usize::try_from(input.reported_len).map_err(|_| OpenError::BudgetExceeded)? > limit {
            return Err(OpenError::BudgetExceeded);
        }
        let bytes = self.read_bounded(&mut input.reader, limit)?;
        // 文件句柄不随候选跨回 UI；返回路径仅供用户标题等显式展示。
        drop(input);
        self.check_cancel()?;
        let storage = StorageSnapshot::new(bytes, DiskVersion::initial(), self.limits.storage)
            .map_err(OpenError::Storage)?;
        let baseline = decode(&storage, self.limits.storage)
            .map_err(OpenError::Decode)?
            .into_baseline();
        let read_decode = read_started.finish();
        self.check_cancel()?;
        let snapshot_started = StageTimer::start();
        // 文档文本预算必须在 UI 全文副本与 StringBuffer 行索引分配之前拒绝。
        if baseline.source().len() > self.limits.document.text().max_bytes() {
            return Err(OpenError::BudgetExceeded);
        }
        let mut source = String::new();
        source
            .try_reserve_exact(baseline.source().len())
            .map_err(OpenError::Allocation)?;
        source.push_str(baseline.source());
        let mut core =
            DocCore::new_readonly(source, SelectionSpec::new(0, 0), self.limits.document)
                .map_err(OpenError::Document)?;
        let snapshot = core
            .current_snapshot(self.limits.document.text())
            .map_err(OpenError::Document)?;
        let identity = core.begin_document_request().map_err(OpenError::Document)?;
        let snapshot_timing = snapshot_started.finish();
        let parse_started = StageTimer::start();
        let parsed = parse_with_cancel(
            snapshot.text(),
            self.limits.config,
            self.limits.parse,
            &self.cancel,
        )
        .map_err(|error| {
            if matches!(error, aline_markdown::ParseError::Cancelled) {
                OpenError::Cancelled
            } else {
                OpenError::Parse(error)
            }
        })?;
        self.check_cancel()?;
        core.accept_document_result(&identity)
            .map_err(OpenError::Document)?;
        let parse_mapping = parse_started.finish();
        let timings = OpenTimings {
            read_decode,
            snapshot: snapshot_timing,
            parse_mapping,
            ..OpenTimings::default()
        };
        Ok((
            ReadonlyDocument {
                path: self.path.clone(),
                baseline,
                core,
                parsed,
            },
            timings,
        ))
    }

    fn read_bounded(&self, reader: &mut impl Read, limit: usize) -> Result<Vec<u8>, OpenError> {
        let mut bytes = Vec::new();
        let mut chunk = [0u8; READ_CHUNK_BYTES];
        loop {
            self.check_cancel()?;
            let remaining = limit
                .checked_sub(bytes.len())
                .ok_or(OpenError::BudgetExceeded)?;
            if remaining == 0 {
                let mut sentinel = [0u8; 1];
                let count = reader
                    .read(&mut sentinel)
                    .map_err(|error| OpenError::Io(error.kind()))?;
                self.check_cancel()?;
                if count > 0 {
                    return Err(OpenError::BudgetExceeded);
                }
                return Ok(bytes);
            }
            let available = remaining.min(chunk.len());
            let count = reader
                .read(&mut chunk[..available])
                .map_err(|error| OpenError::Io(error.kind()))?;
            self.check_cancel()?;
            if count == 0 {
                return Ok(bytes);
            }
            let part = chunk
                .get(..count)
                .filter(|_| count <= available)
                .ok_or(OpenError::InvalidRead)?;
            bytes.try_reserve(count).map_err(OpenError::Allocation)?;
            bytes.extend_from_slice(part);
        }
    }

    fn check_cancel(&self) -> Result<(), OpenError> {
        if self.cancel.is_cancelled() {
            return Err(OpenError::Cancelled);
        }
        Ok(())
    }
}

#[cfg(all(
    test,
    any(
        target_os = "macos",
        all(
            target_os = "linux",
            any(target_arch = "x86_64", target_arch = "aarch64")
        )
    )
))]
mod tests {
    use super::*;
    use std::error::Error;
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    #[test]
    fn fifo_replacement_after_precheck_is_rejected_with_deadline() -> Result<(), Box<dyn Error>> {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("aline-fifo-swap-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory)?;
        let path = directory.join("replaced.md");
        std::fs::write(&path, b"regular")?;
        let mut child = Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "document::readonly_session::read::tests::fifo_swap_child",
                "--ignored",
                "--nocapture",
            ])
            .env("ALINE_FIFO_SWAP_PATH", &path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        let started = Instant::now();
        let timed_out = loop {
            if child.try_wait()?.is_some() {
                break false;
            }
            if started.elapsed() > Duration::from_secs(5) {
                child.kill()?;
                break true;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let output = child.wait_with_output()?;
        std::fs::remove_dir_all(&directory)?;
        assert!(
            !timed_out,
            "FIFO replacement blocked after a regular-file precheck"
        );
        assert!(
            output.status.success(),
            "FIFO replacement child failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(())
    }

    #[test]
    #[ignore = "由有进程时限的父回归执行"]
    fn fifo_swap_child() -> Result<(), Box<dyn Error>> {
        let path = std::path::PathBuf::from(
            std::env::var_os("ALINE_FIFO_SWAP_PATH").ok_or("missing FIFO swap path")?,
        );
        let observed = std::fs::metadata(&path)?;
        assert!(observed.is_file());
        std::fs::remove_file(&path)?;
        assert!(Command::new("mkfifo").arg(&path).status()?.success());
        assert!(open_after_regular_check(&path, &observed).is_err());
        Ok(())
    }

    #[test]
    fn regular_file_replacement_is_rejected_by_opened_handle_identity() -> Result<(), Box<dyn Error>>
    {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_nanos();
        let directory =
            std::env::temp_dir().join(format!("aline-file-swap-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&directory)?;
        let path = directory.join("replaced.md");
        std::fs::write(&path, b"first")?;
        let observed = std::fs::metadata(&path)?;
        let second = directory.join("second.md");
        std::fs::write(&second, b"second")?;
        std::fs::rename(&second, &path)?;
        let result = open_after_regular_check(&path, &observed);
        std::fs::remove_dir_all(&directory)?;
        assert!(
            result.is_err(),
            "a different inode must not pass the original precheck"
        );
        Ok(())
    }
}
