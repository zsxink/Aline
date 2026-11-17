//! W4 既有目标的真实文件回归，以独立磁盘字节为预期。

#![cfg(target_os = "macos")]

use std::error::Error;
use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use aline_source::{
    AtomicFailureKind, Cancellation, DiskVersion, ExistingTarget, ExistingTargetKind, IoBudget,
    StorageBudget, decode, encode, prepare_checked_replace,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> TestResult<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let dir = std::env::temp_dir().join(format!("ar-{}-{nonce:x}", std::process::id()));
        fs::create_dir(&dir)?;
        Ok(Self(dir))
    }

    fn target(&self) -> PathBuf {
        self.0.join("note.md")
    }

    fn staging(&self) -> PathBuf {
        self.0.join("pending.tmp")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn observe(path: &Path) -> TestResult<ExistingTarget> {
    Ok(ExistingTarget::observe(
        path,
        DiskVersion::from_value(7),
        IoBudget::new(1024),
        &Cancellation::new(),
    )?)
}

#[test]
fn checked_replace_rejects_same_size_and_mtime_content_before_staging() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"abc\n")?;
    let observed = observe(&target)?;
    assert_eq!(observed.storage().bytes(), b"abc\n");
    assert_eq!(observed.storage().version(), DiskVersion::from_value(7));
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("saved\n", &baseline, budget)?;

    let original_mtime = fs::metadata(&target)?.modified()?;
    fs::write(&target, b"xyz\n")?;
    File::options()
        .write(true)
        .open(&target)?
        .set_times(FileTimes::new().set_modified(original_mtime))?;
    assert_eq!(fs::metadata(&target)?.modified()?, original_mtime);
    assert_eq!(fs::metadata(&target)?.len(), 4);

    let error = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )
    .err()
    .ok_or("同尺寸同时间戳外改必须拒绝")?;
    assert!(matches!(
        error.kind(),
        AtomicFailureKind::Conflict {
            target_exists: true
        }
    ));
    assert!(!staging.exists());
    assert_eq!(fs::read(target)?, b"xyz\n");
    Ok(())
}

#[test]
fn checked_replace_refuses_special_paths_and_observation_over_budget() -> TestResult {
    use std::os::unix::{fs::symlink, net::UnixListener};

    let fixture = Fixture::new()?;
    let target = fixture.target();
    let link = fixture.0.join("link.md");
    let socket = fixture.0.join("socket.md");
    fs::write(&target, b"base\n")?;
    symlink(&target, &link)?;
    let _listener = UnixListener::bind(&socket)?;
    for (path, expected) in [
        (link.as_path(), ExistingTargetKind::Symlink),
        (socket.as_path(), ExistingTargetKind::Other),
        (fixture.0.as_path(), ExistingTargetKind::Directory),
    ] {
        let error = ExistingTarget::observe(
            path,
            DiskVersion::initial(),
            IoBudget::new(1024),
            &Cancellation::new(),
        )
        .err()
        .ok_or("必须拒绝非普通目标")?;
        assert!(matches!(
            error.kind(),
            AtomicFailureKind::ExistingTarget { kind } if *kind == expected
        ));
    }
    let error = ExistingTarget::observe(
        &target,
        DiskVersion::initial(),
        IoBudget::new(4),
        &Cancellation::new(),
    )
    .err()
    .ok_or("必须拒绝超限目标")?;
    assert!(matches!(
        error.kind(),
        AtomicFailureKind::Budget {
            needed: 5,
            limit: 4
        }
    ));
    assert_eq!(fs::read(target)?, b"base\n");
    Ok(())
}

#[test]
fn checked_replace_rejects_unverified_staging_access_before_writing() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"base\n")?;
    let observed = observe(&target)?;
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("unpublished\n", &baseline, budget)?;
    let error = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )
    .err()
    .ok_or("未验证 staging 初始访问权限必须拒绝")?;
    assert!(matches!(
        error.kind(),
        AtomicFailureKind::ReplacementMetadataUnverified
    ));
    assert_eq!(error.staging_path(), None);
    assert!(!staging.exists());
    assert_eq!(fs::read(target)?, b"base\n");
    Ok(())
}
