//! 已授权隔离探针：固定句柄级元数据复制；不属于 Aline 生产代码或依赖准入。

use std::error::Error;
use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

type ProbeResult<T = ()> = Result<T, Box<dyn Error>>;

#[derive(Debug)]
enum CopyError {
    Metadata(io::Error),
    NonRegular,
    SameInode,
    CallCounterOverflow,
    System(io::Error),
}

/// 仅接收探针自身仍持有的两个句柄；固定 flags 与 null state，没有外部路径入口。
fn copy_metadata(source: &File, target: &File, calls: &mut u32) -> Result<(), CopyError> {
    let source_metadata = source.metadata().map_err(CopyError::Metadata)?;
    let target_metadata = target.metadata().map_err(CopyError::Metadata)?;
    if !source_metadata.is_file() || !target_metadata.is_file() {
        return Err(CopyError::NonRegular);
    }
    if source_metadata.dev() == target_metadata.dev()
        && source_metadata.ino() == target_metadata.ino()
    {
        return Err(CopyError::SameInode);
    }
    *calls = calls.checked_add(1).ok_or(CopyError::CallCounterOverflow)?;
    // SAFETY: 用户仅授权本实验。两个 fd 均借自此作用域内仍持有的 File，已验证
    // 是不同 inode 的普通文件；null state 按系统契约可用，不接收任意 state/flags。
    // COPYFILE_METADATA 固定为 STAT|ACL|XATTR，不含 DATA，不操作用户文件。
    let result = unsafe {
        libc::fcopyfile(
            source.as_raw_fd(),
            target.as_raw_fd(),
            std::ptr::null_mut(),
            libc::COPYFILE_METADATA,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(CopyError::System(io::Error::last_os_error()))
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::other(message.into())
}

fn private_root() -> ProbeResult<PathBuf> {
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    for attempt in 0..32u32 {
        let path = std::env::temp_dir().join(format!(
            "aline-macos-metadata-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        match DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(invalid("无法独占创建私有实验目录").into())
}

fn new_file(path: &Path, bytes: &[u8]) -> ProbeResult<File> {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(file)
}

fn tool(program: &str, arguments: &[&str], path: &Path) -> ProbeResult<String> {
    let output = Command::new(program).args(arguments).arg(path).output()?;
    if !output.status.success() {
        return Err(invalid(format!(
            "{program} 失败: status={:?}, stderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ))
        .into());
    }
    Ok(String::from_utf8(output.stdout)?)
}

fn acl(path: &Path) -> ProbeResult<Vec<String>> {
    Ok(tool("/bin/ls", &["-le"], path)?
        .lines()
        .filter(|line| {
            line.trim_start()
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit())
        })
        .map(|line| line.trim().to_owned())
        .collect())
}

fn require_fixture_acl(path: &Path) -> ProbeResult {
    let entries = acl(path)?;
    if entries.is_empty()
        || !entries
            .iter()
            .any(|entry| entry.contains("group:everyone deny write"))
        || !entries
            .iter()
            .any(|entry| entry.contains("group:everyone allow read"))
    {
        return Err(invalid(format!("源ACL夹具未包含预设deny/allow: {entries:?}")).into());
    }
    Ok(())
}

fn bytes(file: &File) -> ProbeResult<Vec<u8>> {
    let mut reader = file.try_clone()?;
    reader.seek(SeekFrom::Start(0))?;
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(output)
}

fn facts(label: &str, file: &File, path: &Path) -> ProbeResult {
    let metadata = file.metadata()?;
    let bytes = bytes(file)?;
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!(
        "FACT label={label} dev={} inode={} mode={:o} bytes_hex={hex} acl={:?} xattrs={:?}",
        metadata.dev(),
        metadata.ino(),
        metadata.mode() & 0o7777,
        acl(path)?,
        tool("/usr/bin/xattr", &[], path)?
            .lines()
            .collect::<Vec<_>>()
    );
    Ok(())
}

fn checked_copy(source: &File, target: &File, calls: &mut u32) -> ProbeResult {
    match copy_metadata(source, target, calls) {
        Ok(()) => Ok(()),
        Err(error) => Err(invalid(format!("预期成功，实际 {error:?}")).into()),
    }
}

fn run(root: &Path) -> ProbeResult {
    let root_metadata = fs::symlink_metadata(root)?;
    if !root_metadata.is_dir() || root_metadata.mode() & 0o777 != 0o700 {
        return Err(invalid("实验根不是独占0700目录").into());
    }
    println!(
        "ROOT path={} mode=700 dev={}",
        root.display(),
        root_metadata.dev()
    );
    println!(
        "FLAGS metadata={} data={} state=null",
        libc::COPYFILE_METADATA,
        libc::COPYFILE_DATA
    );
    if libc::COPYFILE_METADATA & libc::COPYFILE_DATA != 0 {
        return Err(invalid("固定metadata flags意外包含DATA").into());
    }
    let mut calls = 0u32;
    let source_path = root.join("source.md");
    let target_path = root.join("target.md");
    let source = new_file(&source_path, b"SOURCE\n")?;
    let target = new_file(&target_path, b"TARGET\n")?;
    source.set_permissions(fs::Permissions::from_mode(0o640))?;
    tool("/bin/chmod", &["+a", "everyone deny write"], &source_path)?;
    tool("/bin/chmod", &["+a", "everyone allow read"], &source_path)?;
    tool(
        "/usr/bin/xattr",
        &["-w", "com.aline.probe", "aline-preserve"],
        &source_path,
    )?;
    require_fixture_acl(&source_path)?;
    facts("metadata-source-before", &source, &source_path)?;
    facts("metadata-target-before", &target, &target_path)?;
    checked_copy(&source, &target, &mut calls)?;
    facts("metadata-target-after", &target, &target_path)?;
    if bytes(&source)? != b"SOURCE\n" || bytes(&target)? != b"TARGET\n" {
        return Err(invalid("metadata复制改变正文").into());
    }
    if target.metadata()?.mode() & 0o7777 != 0o640 || acl(&source_path)? != acl(&target_path)? {
        return Err(invalid("mode或ACL未按预期复制").into());
    }
    if tool("/usr/bin/xattr", &["-p", "com.aline.probe"], &target_path)?.trim() != "aline-preserve"
    {
        return Err(invalid("xattr未按预期复制").into());
    }
    println!("CASE metadata passed calls={calls}");

    let acl_source_path = root.join("acl-source.md");
    let acl_target_path = root.join("acl-target.md");
    let acl_source = new_file(&acl_source_path, b"ACL-SOURCE\n")?;
    let acl_target = new_file(&acl_target_path, b"ACL-TARGET\n")?;
    tool(
        "/bin/chmod",
        &["+a", "everyone deny write"],
        &acl_source_path,
    )?;
    tool(
        "/bin/chmod",
        &["+a", "everyone allow read"],
        &acl_source_path,
    )?;
    require_fixture_acl(&acl_source_path)?;
    facts("acl-only-source-before", &acl_source, &acl_source_path)?;
    facts("acl-only-target-before", &acl_target, &acl_target_path)?;
    checked_copy(&acl_source, &acl_target, &mut calls)?;
    facts("acl-only-target-after", &acl_target, &acl_target_path)?;
    if acl(&acl_source_path)? != acl(&acl_target_path)?
        || bytes(&acl_source)? != b"ACL-SOURCE\n"
        || bytes(&acl_target)? != b"ACL-TARGET\n"
        || acl_target.metadata()?.mode() & 0o7777 != 0o600
    {
        return Err(invalid("ACL-only样本不符独立预期").into());
    }
    println!("CASE acl-only passed calls={calls}");

    let hardlink_path = root.join("source-alias.md");
    fs::hard_link(&source_path, &hardlink_path)?;
    let alias = File::open(&hardlink_path)?;
    let before = calls;
    if !matches!(
        copy_metadata(&source, &alias, &mut calls),
        Err(CopyError::SameInode)
    ) || calls != before
    {
        return Err(invalid("同inode未在调用前拒绝").into());
    }
    if bytes(&source)? != b"SOURCE\n" {
        return Err(invalid("同inode拒绝改变了正文").into());
    }
    println!("CASE same-inode rejected-before-call calls={calls}");

    let directory = File::open(root)?;
    if !matches!(
        copy_metadata(&source, &directory, &mut calls),
        Err(CopyError::NonRegular)
    ) || calls != before
    {
        return Err(invalid("非普通文件未在调用前拒绝").into());
    }
    println!("CASE non-regular rejected-before-call calls={calls}");

    let readonly_path = root.join("readonly.md");
    let readonly_writable = new_file(&readonly_path, b"READONLY-TARGET\n")?;
    drop(readonly_writable);
    let readonly_target = File::open(&readonly_path)?;
    match copy_metadata(&source, &readonly_target, &mut calls) {
        Ok(()) => {
            facts("readonly-target-success", &readonly_target, &readonly_path)?;
            if acl(&source_path)? != acl(&readonly_path)?
                || readonly_target.metadata()?.mode() & 0o7777 != 0o640
                || tool("/usr/bin/xattr", &["-p", "com.aline.probe"], &readonly_path)?.trim()
                    != "aline-preserve"
            {
                return Err(invalid("只读句柄成功但元数据事实不符").into());
            }
            println!("CASE readonly-target observed-success calls={calls}");
        }
        Err(CopyError::System(error)) => println!(
            "CASE readonly-target observed-system-failure kind={:?} errno={:?} message={error} calls={calls}",
            error.kind(),
            error.raw_os_error()
        ),
        Err(CopyError::Metadata(error)) => return Err(error.into()),
        other => return Err(invalid(format!("只读合法普通句柄意外拒绝: {other:?}")).into()),
    }
    if bytes(&readonly_target)? != b"READONLY-TARGET\n" {
        return Err(invalid("只读目标失败改变了正文").into());
    }
    // 用户授权的合法句柄受控失败：只给私有根内自有文件设置不可变标志。
    let immutable_path = root.join("immutable.md");
    let immutable = new_file(&immutable_path, b"IMMUTABLE-TARGET\n")?;
    tool("/usr/bin/chflags", &["uchg"], &immutable_path)?;
    facts("immutable-target-before", &immutable, &immutable_path)?;
    match copy_metadata(&source, &immutable, &mut calls) {
        Err(CopyError::System(error)) => println!(
            "CASE immutable-target system-failure kind={:?} errno={:?} message={error} calls={calls}",
            error.kind(),
            error.raw_os_error()
        ),
        Err(CopyError::Metadata(error)) => return Err(error.into()),
        other => {
            return Err(invalid(format!(
                "不可变样本未形成预期系统失败，失败覆盖不足: {other:?}"
            ))
            .into());
        }
    }
    if bytes(&immutable)? != b"IMMUTABLE-TARGET\n" {
        return Err(invalid("不可变目标失败改变了正文").into());
    }
    facts(
        "immutable-target-after-failure",
        &immutable,
        &immutable_path,
    )?;
    println!("SUMMARY passed=6 unsafe_calls={calls} production_admission=false");
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = private_root()?;
    let result = run(&root);
    // 清理先去除本探针设置的ACL；每项失败均单独记录，仍尝试清理根目录。
    let mut cleanup_failed = false;
    let immutable = root.join("immutable.md");
    if immutable.exists() {
        if let Err(error) = tool("/usr/bin/chflags", &["nouchg"], &immutable) {
            eprintln!(
                "CLEANUP_FLAGS_ERROR path={} error={error}",
                immutable.display()
            );
            cleanup_failed = true;
        }
    }
    for name in [
        "source.md",
        "target.md",
        "acl-source.md",
        "acl-target.md",
        "readonly.md",
        "immutable.md",
    ] {
        let path = root.join(name);
        if path.exists() {
            if let Err(error) = tool("/bin/chmod", &["-N"], &path) {
                eprintln!("CLEANUP_ACL_ERROR path={} error={error}", path.display());
                cleanup_failed = true;
            }
        }
    }
    match fs::remove_dir_all(&root) {
        Ok(()) => println!("CLEANUP_ROOT removed={}", root.display()),
        Err(error) => {
            eprintln!("CLEANUP_ROOT_ERROR path={} error={error}", root.display());
            cleanup_failed = true;
        }
    }
    if cleanup_failed {
        return Err(invalid(format!("探针结果={result:?}；清理有错误")).into());
    }
    result
}
