//! 4.4 create-only 受检文件原语的证据测试；使用私有临时目录与手写金样。

use std::error::Error;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aline_source::{
    AtomicFailure, AtomicFailureKind, AtomicStage, Cancellation, DiskVersion, EncodedBytes,
    ExistingTargetKind, InvalidInput, IoBudget, PreparedAtomic, SourceBaseline, StorageBudget,
    StorageSnapshot, decode, encode, prepare_atomic,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

struct TestDir {
    path: PathBuf,
}

impl TestDir {
    fn new() -> TestResult<Self> {
        let base = std::env::temp_dir();
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for attempt in 0..100u32 {
            let path = base.join(format!(
                "aline-atomic-integration-{}-{nanos}-{attempt}",
                std::process::id()
            ));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(invalid("无法创建私有测试目录").into())
    }

    fn path(&self) -> &Path {
        &self.path
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

fn baseline(bytes: &[u8]) -> TestResult<SourceBaseline> {
    let budget = StorageBudget::new(usize::MAX);
    let storage = StorageSnapshot::new(bytes.to_vec(), DiskVersion::initial(), budget)?;
    Ok(decode(&storage, budget)?.into_baseline())
}

fn encoded(frozen: &str, baseline: &SourceBaseline) -> TestResult<EncodedBytes> {
    Ok(encode(frozen, baseline, StorageBudget::new(usize::MAX))?)
}

fn parse_hex(text: &str) -> TestResult<Vec<u8>> {
    let bytes = text.as_bytes();
    if !bytes.len().is_multiple_of(2) {
        return Err(invalid("十六进制长度必须为偶数").into());
    }
    let mut output = Vec::with_capacity(bytes.len() / 2);
    for pair in bytes.as_chunks::<2>().0 {
        let pair = std::str::from_utf8(pair)?;
        output.push(u8::from_str_radix(pair, 16)?);
    }
    Ok(output)
}

fn expect_prepare_failure(
    encoded: &EncodedBytes,
    target: &Path,
    staging: &Path,
    budget: IoBudget,
    cancellation: &Cancellation,
) -> TestResult<AtomicFailure> {
    match prepare_atomic(encoded, target, staging, budget, cancellation) {
        Ok(_) => Err(invalid("prepare 应失败").into()),
        Err(failure) => Ok(failure),
    }
}

fn expect_publish_failure(
    prepared: PreparedAtomic,
    cancellation: &Cancellation,
) -> TestResult<AtomicFailure> {
    match prepared.publish_new(cancellation) {
        Ok(_) => Err(invalid("publish 应失败").into()),
        Err(failure) => Ok(failure),
    }
}

fn assert_invalid(failure: &AtomicFailure, expected: InvalidInput) -> TestResult {
    match failure.kind() {
        AtomicFailureKind::InvalidInput { reason } => assert_eq!(*reason, expected),
        other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
    }
    Ok(())
}

#[test]
fn atomic_golden_publish_new_and_existing_target_match_independent_expectation() -> TestResult {
    let golden = include_str!("fixtures/atomic-golden.v1.txt");
    let mut cases = 0usize;
    for line in golden
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('|').collect();
        assert_eq!(fields.len(), 5, "金样字段数");
        let name = fields[0];
        let original = parse_hex(fields[1])?;
        let frozen_bytes = parse_hex(fields[2])?;
        let frozen = std::str::from_utf8(&frozen_bytes)?;
        let expected = parse_hex(fields[3])?;
        let sentinel = parse_hex(fields[4])?;
        let baseline = baseline(&original)?;
        let encoded = encoded(frozen, &baseline)?;
        assert_eq!(encoded.bytes(), expected.as_slice(), "case {name} 编码");
        assert_eq!(encoded.baseline_id(), baseline.id(), "case {name} 基线身份");

        let new_dir = TestDir::new()?;
        let target = new_dir.target();
        let staging = new_dir.staging();
        let prepared = prepare_atomic(
            &encoded,
            &target,
            &staging,
            IoBudget::new(usize::MAX),
            &Cancellation::new(),
        )?;
        assert!(!target.exists(), "case {name} prepare 不得创建目标");
        assert_eq!(fs::read(&staging)?, expected, "case {name} staging 内容");
        let written = prepared.publish_new(&Cancellation::new())?;
        assert_eq!(written.target_path(), target.as_path());
        assert_eq!(written.staging_path(), staging.as_path());
        assert_eq!(written.bytes(), expected.as_slice());
        assert_eq!(written.written_len(), expected.len());
        assert_eq!(written.input_baseline_id(), baseline.id());
        assert!(written.directory_durability_unverified());
        assert_eq!(fs::read(&target)?, expected, "case {name} 目标内容");
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let target_metadata = fs::metadata(&target)?;
            let staging_metadata = fs::metadata(&staging)?;
            assert_eq!(target_metadata.dev(), staging_metadata.dev());
            assert_eq!(target_metadata.ino(), staging_metadata.ino());
        }
        let new_baseline = written.into_baseline(DiskVersion::from_value(1))?;
        assert_eq!(new_baseline.storage().bytes(), expected.as_slice());

        let refused_dir = TestDir::new()?;
        let refused_target = refused_dir.target();
        let refused_staging = refused_dir.staging();
        fs::write(&refused_target, &sentinel)?;
        let prepared = prepare_atomic(
            &encoded,
            &refused_target,
            &refused_staging,
            IoBudget::new(usize::MAX),
            &Cancellation::new(),
        )?;
        assert_eq!(
            fs::read(&refused_target)?,
            sentinel,
            "case {name} prepare 不得修改现有目标"
        );
        let failure = expect_publish_failure(prepared, &Cancellation::new())?;
        match failure.kind() {
            AtomicFailureKind::MetadataPreservationUnverified => {}
            other => return Err(invalid(format!("case {name} 错误分类: {other:?}")).into()),
        }
        assert_eq!(failure.staging_path(), Some(refused_staging.as_path()));
        assert_eq!(
            fs::read(&refused_target)?,
            sentinel,
            "case {name} 拒绝后目标必须保持原 bytes"
        );
        assert_eq!(
            fs::read(&refused_staging)?,
            expected,
            "case {name} 拒绝后 staging 保留完整 bytes"
        );
        assert_eq!(
            encoded.bytes(),
            expected.as_slice(),
            "case {name} 调用方 bytes"
        );
        cases += 1;
    }
    assert_eq!(cases, 3, "金样 case 数");
    Ok(())
}

#[test]
fn atomic_staging_creation_failure_preserves_target_and_existing_staging() -> TestResult {
    let dir = TestDir::new()?;
    let target = dir.target();
    let staging = dir.staging();
    let target_bytes = b"TARGET-ORIGINAL\n";
    let staging_bytes = b"EXISTING-STAGING\n";
    fs::write(&target, target_bytes)?;
    fs::write(&staging, staging_bytes)?;
    let baseline = baseline(b"A\n")?;
    let encoded = encoded("A\n", &baseline)?;
    let failure = expect_prepare_failure(
        &encoded,
        &target,
        &staging,
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    match failure.kind() {
        AtomicFailureKind::Io {
            stage: AtomicStage::Create,
            source,
        } => assert_eq!(source.kind(), io::ErrorKind::AlreadyExists),
        other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
    }
    assert_eq!(failure.staging_path(), None);
    assert_eq!(fs::read(&target)?, target_bytes);
    assert_eq!(fs::read(&staging)?, staging_bytes);
    Ok(())
}

#[test]
fn atomic_invalid_paths_budget_and_prepare_cancellation_create_nothing() -> TestResult {
    let dir = TestDir::new()?;
    let target = dir.target();
    let staging = dir.staging();
    let baseline = baseline(b"A\n")?;
    let encoded = encoded("A\n", &baseline)?;

    let failure = expect_prepare_failure(
        &encoded,
        &target,
        &target,
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    assert_invalid(&failure, InvalidInput::SamePath)?;
    assert_eq!(failure.staging_path(), None);

    let failure = expect_prepare_failure(
        &encoded,
        Path::new("target.md"),
        Path::new("staging.tmp"),
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    assert_invalid(&failure, InvalidInput::RelativePath)?;
    assert_eq!(failure.staging_path(), None);

    let other = TestDir::new()?;
    let failure = expect_prepare_failure(
        &encoded,
        &target,
        &other.path().join("staging.tmp"),
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    assert_invalid(&failure, InvalidInput::DifferentDirectory)?;
    assert_eq!(failure.staging_path(), None);

    let failure = expect_prepare_failure(
        &encoded,
        Path::new("/"),
        Path::new("/staging.tmp"),
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    assert_invalid(&failure, InvalidInput::MissingParent)?;
    assert_eq!(failure.staging_path(), None);

    let failure = expect_prepare_failure(
        &encoded,
        &target,
        &staging,
        IoBudget::new(encoded.bytes().len().saturating_sub(1)),
        &Cancellation::new(),
    )?;
    match failure.kind() {
        AtomicFailureKind::Budget { needed, limit } => {
            assert_eq!(*needed, encoded.bytes().len());
            assert_eq!(*limit, encoded.bytes().len().saturating_sub(1));
        }
        other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
    }
    assert_eq!(failure.staging_path(), None);

    let cancellation = Cancellation::new();
    cancellation.cancel();
    let failure = expect_prepare_failure(
        &encoded,
        &target,
        &staging,
        IoBudget::new(usize::MAX),
        &cancellation,
    )?;
    match failure.kind() {
        AtomicFailureKind::Cancelled {
            stage: AtomicStage::Prepare,
        } => {}
        other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
    }
    assert_eq!(failure.staging_path(), None);
    assert!(!target.exists());
    assert!(!staging.exists());
    Ok(())
}

#[test]
fn atomic_pre_publish_cancellation_preserves_target_and_staging() -> TestResult {
    let dir = TestDir::new()?;
    let target = dir.target();
    let staging = dir.staging();
    let target_bytes = b"TARGET-ORIGINAL\n";
    fs::write(&target, target_bytes)?;
    let baseline = baseline(b"A\n")?;
    let encoded = encoded("A\n", &baseline)?;
    let cancellation = Cancellation::new();
    let prepared = prepare_atomic(
        &encoded,
        &target,
        &staging,
        IoBudget::new(usize::MAX),
        &cancellation,
    )?;
    cancellation.cancel();
    let failure = expect_publish_failure(prepared, &cancellation)?;
    match failure.kind() {
        AtomicFailureKind::Cancelled {
            stage: AtomicStage::Publish,
        } => {}
        other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
    }
    assert_eq!(failure.staging_path(), Some(staging.as_path()));
    assert_eq!(fs::read(&target)?, target_bytes);
    assert_eq!(fs::read(&staging)?, encoded.bytes());
    Ok(())
}

#[test]
fn atomic_replaced_staging_path_is_rejected_without_touching_target() -> TestResult {
    let dir = TestDir::new()?;
    let target = dir.target();
    let staging = dir.staging();
    let baseline = baseline(b"A\n")?;
    let encoded = encoded("A\n", &baseline)?;
    let prepared = prepare_atomic(
        &encoded,
        &target,
        &staging,
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    fs::remove_file(&staging)?;
    let replacement = b"REPLACEMENT\n";
    fs::write(&staging, replacement)?;
    let failure = expect_publish_failure(prepared, &Cancellation::new())?;
    match failure.kind() {
        AtomicFailureKind::StagingReplaced => {}
        other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
    }
    assert_eq!(failure.staging_path(), None);
    assert!(!target.exists());
    assert_eq!(fs::read(&staging)?, replacement);
    Ok(())
}

#[test]
fn atomic_existing_symlink_and_directory_are_rejected_without_touching_target() -> TestResult {
    let baseline = baseline(b"A\n")?;
    let encoded = encoded("A\n", &baseline)?;

    #[cfg(unix)]
    {
        let dir = TestDir::new()?;
        let sentinel = dir.path().join("sentinel.md");
        let sentinel_bytes = b"SENTINEL\n";
        fs::write(&sentinel, sentinel_bytes)?;
        let target = dir.target();
        std::os::unix::fs::symlink(&sentinel, &target)?;
        let staging = dir.staging();
        let prepared = prepare_atomic(
            &encoded,
            &target,
            &staging,
            IoBudget::new(usize::MAX),
            &Cancellation::new(),
        )?;
        let failure = expect_publish_failure(prepared, &Cancellation::new())?;
        match failure.kind() {
            AtomicFailureKind::ExistingTarget {
                kind: ExistingTargetKind::Symlink,
            } => {}
            other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
        }
        assert_eq!(failure.staging_path(), Some(staging.as_path()));
        assert_eq!(fs::read(&sentinel)?, sentinel_bytes);
        assert_eq!(fs::read_link(&target)?, sentinel);
    }

    let dir = TestDir::new()?;
    let target = dir.target();
    fs::create_dir(&target)?;
    let staging = dir.staging();
    let prepared = prepare_atomic(
        &encoded,
        &target,
        &staging,
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    let failure = expect_publish_failure(prepared, &Cancellation::new())?;
    match failure.kind() {
        AtomicFailureKind::ExistingTarget {
            kind: ExistingTargetKind::Directory,
        } => {}
        other => return Err(invalid(format!("错误分类不符: {other:?}")).into()),
    }
    assert_eq!(failure.staging_path(), Some(staging.as_path()));
    assert!(target.is_dir());
    Ok(())
}

#[test]
fn atomic_large_source_crosses_write_chunks_and_publishes_exact_bytes() -> TestResult {
    let dir = TestDir::new()?;
    let mut body = "a".repeat(70_000);
    body.push('\n');
    let baseline = baseline(body.as_bytes())?;
    let encoded = encoded(&body, &baseline)?;
    assert!(encoded.bytes().len() > 64 * 1024);
    let target = dir.target();
    let staging = dir.staging();
    let prepared = prepare_atomic(
        &encoded,
        &target,
        &staging,
        IoBudget::new(usize::MAX),
        &Cancellation::new(),
    )?;
    assert_eq!(fs::read(&staging)?, encoded.bytes());
    let written = prepared.publish_new(&Cancellation::new())?;
    assert_eq!(written.written_len(), encoded.bytes().len());
    assert_eq!(fs::read(&target)?, encoded.bytes());
    Ok(())
}
