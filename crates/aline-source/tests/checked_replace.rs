//! W4 既有目标的真实文件回归，以独立磁盘字节为预期。

#![cfg(target_os = "macos")]

use std::error::Error;
use std::fs::{self, File, FileTimes, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use aline_source::{
    AtomicFailureKind, AtomicStage, Cancellation, DiskVersion, ExistingTarget, ExistingTargetKind,
    IoBudget, ReplacementCleanup, ReplacementDurability, StorageBudget, decode, encode,
    prepare_checked_replace,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> TestResult<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        for attempt in 0..100u32 {
            let dir =
                std::env::temp_dir().join(format!("ar-{}-{nonce:x}-{attempt}", std::process::id()));
            match fs::create_dir(&dir) {
                Ok(()) => return Ok(Self(dir)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err("无法创建测试目录".into())
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

fn run(command: &str, arguments: &[&str]) -> TestResult<String> {
    let output = Command::new(command).args(arguments).output()?;
    if !output.status.success() {
        return Err(format!(
            "{command} 失败（{}）: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[test]
fn checked_replace_publishes_exact_bytes_and_preserves_mode_and_xattr() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"base\n")?;
    fs::set_permissions(&target, Permissions::from_mode(0o640))?;
    run(
        "/usr/bin/xattr",
        &["-w", "com.aline.probe", "value-1", path_str(&target)?],
    )?;

    let observed = observe(&target)?;
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("saved\n", &baseline, budget)?;
    let prepared = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )?;
    let receipt = prepared.publish(&Cancellation::new())?;

    assert_eq!(receipt.target_path(), target);
    assert_eq!(receipt.replaced_version(), DiskVersion::from_value(7));
    assert_eq!(receipt.version(), DiskVersion::from_value(8));
    assert_eq!(receipt.bytes(), b"saved\n");
    assert!(matches!(
        receipt.durability(),
        ReplacementDurability::Synchronized
    ));
    assert!(matches!(receipt.cleanup(), ReplacementCleanup::Removed));
    assert!(!staging.exists());
    assert_eq!(fs::read(&target)?, b"saved\n");

    let mode = fs::metadata(&target)?.permissions().mode() & 0o7777;
    assert_eq!(mode, 0o640, "替换必须保留目标 mode");
    let value = run(
        "/usr/bin/xattr",
        &["-p", "com.aline.probe", path_str(&target)?],
    )?;
    assert_eq!(value.trim_end(), "value-1", "替换必须保留目标 xattr");

    let baseline = receipt.into_baseline()?;
    assert_eq!(baseline.storage().bytes(), b"saved\n");
    Ok(())
}

#[test]
fn checked_replace_fails_closed_when_target_acl_denies_deletion() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"base\n")?;
    // 探针实测：target 上的 deny delete ACL 使 rename/swap 返回 EACCES；
    // 复制该 ACL 到 staging 后无法安全发布，只能失败关闭并保留原目标。
    run(
        "/bin/chmod",
        &["+a", "staff deny delete", path_str(&target)?],
    )?;

    let observed = observe(&target)?;
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("unpublished\n", &baseline, budget)?;
    let prepared = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )?;
    let error = prepared
        .publish(&Cancellation::new())
        .err()
        .ok_or("deny delete 目标必须失败关闭")?;
    assert!(
        matches!(
            error.kind(),
            AtomicFailureKind::Io {
                stage: AtomicStage::Publish,
                source,
            } if source.kind() == std::io::ErrorKind::PermissionDenied
        ),
        "发布失败必须保留 EACCES 分类: {error:?}"
    );
    assert_eq!(error.staging_path(), Some(staging.as_path()));
    assert_eq!(fs::read(&target)?, b"base\n");
    assert_eq!(fs::read(&staging)?, encoded.bytes());
    assert_eq!(observed.storage().bytes(), b"base\n");

    // 清理带 deny delete ACL 的测试文件，避免临时目录残留。
    run("/bin/chmod", &["-N", path_str(&target)?])?;
    run("/bin/chmod", &["-N", path_str(&staging)?])?;
    fs::remove_file(&staging)?;
    Ok(())
}

#[test]
fn checked_replace_fails_closed_when_directory_acl_inherits_to_files() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"base\n")?;
    // file_inherit 在 openat 返回前已作用于新 inode；第三方可能在清除前打开并
    // 长期持有描述符，因此创建前失败关闭，不把「清除后继续」当作安全边界。
    run(
        "/bin/chmod",
        &["+a", "staff allow read,file_inherit", path_str(&fixture.0)?],
    )?;

    let observed = observe(&target)?;
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("saved\n", &baseline, budget)?;
    let error = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )
    .err()
    .ok_or("父目录继承 ACL 必须失败关闭")?;
    assert!(
        matches!(
            error.kind(),
            AtomicFailureKind::ReplacementMetadataUnverified
        ),
        "继承 ACL 必须报告访问元数据未验证: {error:?}"
    );
    assert_eq!(error.staging_path(), None);
    assert!(!staging.exists());
    assert_eq!(fs::read(&target)?, b"base\n");

    run("/bin/chmod", &["-N", path_str(&fixture.0)?])?;
    Ok(())
}

#[test]
fn checked_replace_allows_directory_only_inherited_acl() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"base\n")?;
    // directory_inherit 只作用于子目录，不影响 staging 文件本身。
    run(
        "/bin/chmod",
        &[
            "+a",
            "staff allow read,directory_inherit",
            path_str(&fixture.0)?,
        ],
    )?;

    let observed = observe(&target)?;
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("saved\n", &baseline, budget)?;
    let prepared = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )?;
    let listing = run("/bin/ls", &["-le", path_str(&staging)?])?;
    assert!(
        !listing.contains("allow") && !listing.contains("deny"),
        "staging 不得继承目录 ACL: {listing}"
    );
    let receipt = prepared.publish(&Cancellation::new())?;
    assert!(matches!(
        receipt.durability(),
        ReplacementDurability::Synchronized
    ));
    assert_eq!(fs::read(&target)?, b"saved\n");
    assert!(!staging.exists());

    run("/bin/chmod", &["-N", path_str(&fixture.0)?])?;
    Ok(())
}

#[test]
fn checked_replace_preserves_target_allow_acl_on_final_target() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"base\n")?;
    run(
        "/bin/chmod",
        &["+a", "staff allow read", path_str(&target)?],
    )?;

    let observed = observe(&target)?;
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("saved\n", &baseline, budget)?;
    let prepared = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )?;
    let receipt = prepared.publish(&Cancellation::new())?;
    assert_eq!(receipt.target_path(), target);
    assert_eq!(fs::read(&target)?, b"saved\n");
    assert!(!staging.exists());

    // 正向 ACL 必须随新 inode 出现在最终目标上，而不只存在于迁移前的 staging。
    let listing = run("/bin/ls", &["-le", path_str(&target)?])?;
    assert!(
        listing.contains("allow read") && listing.contains("staff"),
        "最终目标必须保留 allow ACL: {listing}"
    );

    run("/bin/chmod", &["-N", path_str(&target)?])?;
    Ok(())
}

#[test]
fn checked_replace_fails_closed_on_nonzero_file_flags() -> TestResult {
    let fixture = Fixture::new()?;
    let target = fixture.target();
    let staging = fixture.staging();
    fs::write(&target, b"base\n")?;
    // M2 不支持 chflags 语义（immutable/append/nounlink 会阻止 rename 或清理）。
    run("/usr/bin/chflags", &["hidden", path_str(&target)?])?;

    let observed = observe(&target)?;
    let budget = StorageBudget::new(1024);
    let baseline = decode(observed.storage(), budget)?.into_baseline();
    let encoded = encode("unpublished\n", &baseline, budget)?;
    let prepared = prepare_checked_replace(
        &encoded,
        &observed,
        &staging,
        IoBudget::new(1024),
        &Cancellation::new(),
    )?;
    let error = prepared
        .publish(&Cancellation::new())
        .err()
        .ok_or("非 0 文件 flags 必须失败关闭")?;
    assert!(
        matches!(
            error.kind(),
            AtomicFailureKind::ReplacementMetadataUnverified
        ),
        "flags 不支持必须报告元数据未验证: {error:?}"
    );
    assert_eq!(error.staging_path(), Some(staging.as_path()));
    assert_eq!(fs::read(&target)?, b"base\n");
    assert_eq!(fs::read(&staging)?, encoded.bytes());

    run("/usr/bin/chflags", &["nohidden", path_str(&target)?])?;
    fs::remove_file(&staging)?;
    Ok(())
}

fn path_str(path: &Path) -> TestResult<&str> {
    path.to_str().ok_or_else(|| "测试路径不是 UTF-8".into())
}
