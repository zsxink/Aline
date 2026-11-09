//! W3 source：仅合成 bytes 与私有临时目录；随机编码往返和 create-only 失败保全。
//! 不声称覆盖未实现的 M2 受检替换、进程崩溃恢复或目录断电持久性。

mod support;
use support as fuzz;

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use aline_source::{
    Cancellation, DecodeError, DiskVersion, IoBudget, SourceBaseline, StorageBudget,
    StorageSnapshot, decode, encode, prepare_atomic,
};
use fuzz::{Config, Run, TestResult, boundaries, invalid};

const LIMIT: usize = 1 << 20;

fn baseline(bytes: Vec<u8>) -> TestResult<SourceBaseline> {
    let storage = StorageSnapshot::new(bytes, DiskVersion::initial(), StorageBudget::new(LIMIT))?;
    Ok(decode(&storage, StorageBudget::new(LIMIT))?.into_baseline())
}

struct TestDir(PathBuf);
impl TestDir {
    fn new() -> TestResult<Self> {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = std::env::temp_dir().join(format!("aline-w3-{}-{nonce}", std::process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}
impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn check_atomic(run: &mut Run, original: &SourceBaseline, edited: &str, step: usize) -> TestResult {
    let dir = TestDir::new()?;
    let target = dir.0.join("target.md");
    let staging = dir.0.join("stage.tmp");
    let encoded = encode(edited, original, StorageBudget::new(LIMIT))?;
    let cancellation = Cancellation::new();
    let operation = run.below(5)?;
    run.record(step, format!("atomic case={operation}"));
    match operation {
        0 => {
            cancellation.cancel();
            run.check(
                prepare_atomic(
                    &encoded,
                    &target,
                    &staging,
                    IoBudget::new(LIMIT),
                    &cancellation,
                )
                .is_err(),
                "prepare 取消拒绝",
                edited.as_bytes(),
            )?;
            run.check(
                !target.exists() && !staging.exists(),
                "prepare 取消不留文件",
                edited.as_bytes(),
            )?;
        }
        1 => {
            let budget = encoded.bytes().len().saturating_sub(1);
            if !encoded.bytes().is_empty() {
                run.check(
                    prepare_atomic(
                        &encoded,
                        &target,
                        &staging,
                        IoBudget::new(budget),
                        &cancellation,
                    )
                    .is_err(),
                    "prepare 预算拒绝",
                    edited.as_bytes(),
                )?;
                run.check(
                    !target.exists() && !staging.exists(),
                    "预算拒绝不留文件",
                    edited.as_bytes(),
                )?;
            }
        }
        _ => {
            let prepared = prepare_atomic(
                &encoded,
                &target,
                &staging,
                IoBudget::new(LIMIT),
                &cancellation,
            )?;
            run.check(!target.exists(), "prepare 不能提前发布", edited.as_bytes())?;
            run.equal(
                &fs::read(&staging)?,
                &encoded.bytes().to_vec(),
                "staging 完整 bytes",
                edited.as_bytes(),
            )?;
            match operation {
                2 => {
                    cancellation.cancel();
                    run.check(
                        prepared.publish_new(&cancellation).is_err(),
                        "publish 取消拒绝",
                        edited.as_bytes(),
                    )?;
                    run.check(
                        !target.exists(),
                        "publish 取消保全未发布",
                        edited.as_bytes(),
                    )?;
                    run.equal(
                        &fs::read(&staging)?,
                        &encoded.bytes().to_vec(),
                        "publish 取消保留完整 staging",
                        edited.as_bytes(),
                    )?;
                }
                3 => {
                    let sentinel = b"existing target\r\n";
                    fs::write(&target, sentinel)?;
                    run.check(
                        prepared.publish_new(&cancellation).is_err(),
                        "已有目标禁止覆盖",
                        edited.as_bytes(),
                    )?;
                    run.equal(
                        &fs::read(&target)?,
                        &sentinel.to_vec(),
                        "失败保全真实目标 bytes",
                        edited.as_bytes(),
                    )?;
                }
                _ => {
                    let written = prepared.publish_new(&cancellation)?;
                    run.equal(
                        &fs::read(&target)?,
                        &encoded.bytes().to_vec(),
                        "发布真实 bytes",
                        edited.as_bytes(),
                    )?;
                    run.equal(
                        &written.bytes(),
                        &encoded.bytes(),
                        "回执 bytes",
                        edited.as_bytes(),
                    )?;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn source_fuzz_preserves_unedited_bytes_and_checked_atomic_failures() -> TestResult {
    let config = Config::read()?;
    for seed_index in 0..config.seeds {
        let mut run = Run::new(config.seed + u64::try_from(seed_index)?, config.trace);
        let result = (|| -> TestResult {
            for step in 0..config.steps {
                let pieces = [
                    "中文", "🙂", "\r\n", "\n", "\r", "", "%PDF", "PK", "\u{feff}", "e\u{301}",
                ];
                let mut bytes = if run.below(2)? == 0 {
                    vec![0xef, 0xbb, 0xbf]
                } else {
                    Vec::new()
                };
                for _ in 0..1 + run.below(16)? {
                    bytes.extend_from_slice(pieces[run.below(pieces.len())?].as_bytes());
                }
                let invalid_kind = run.below(8)?;
                if invalid_kind == 0 {
                    bytes.extend_from_slice(&[0xff, 0xfe]);
                }
                if invalid_kind == 1 {
                    bytes = vec![0xff, 0xfe, 0x41, 0];
                }
                run.record(step, format!("bytes={bytes:?}"));
                let storage = StorageSnapshot::new(
                    bytes.clone(),
                    DiskVersion::initial(),
                    StorageBudget::new(LIMIT),
                )?;
                match decode(&storage, StorageBudget::new(LIMIT)) {
                    Err(error) => {
                        run.check(
                            matches!(
                                error,
                                DecodeError::InvalidUtf8 { .. } | DecodeError::Utf16Bom { .. }
                            ),
                            "坏 bytes 仅类型化编码拒绝",
                            &bytes,
                        )?;
                        run.check(
                            std::str::from_utf8(&bytes).is_err(),
                            "合法 UTF-8 不能当坏 bytes 拒绝",
                            &bytes,
                        )?;
                    }
                    Ok(decoded) => {
                        run.check(
                            std::str::from_utf8(&bytes).is_ok(),
                            "非法 UTF-8 不能有损接受",
                            &bytes,
                        )?;
                        let original = decoded.into_baseline();
                        let expected_source = match bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
                            Some(source) => source,
                            None => bytes.as_slice(),
                        };
                        run.equal(
                            &original.source().as_bytes(),
                            &expected_source,
                            "解码仅剥离一个头部 BOM",
                            &bytes,
                        )?;
                        let encoded =
                            encode(original.source(), &original, StorageBudget::new(LIMIT))?;
                        let mut observed = encoded.bytes().to_vec();
                        if config.inject {
                            observed.pop();
                        }
                        if let Err(error) = run.equal(&observed, &bytes, "未编辑逐字节相同", &bytes)
                        {
                            let minimum = fuzz::minimize(bytes.clone(), |candidate| {
                                baseline(candidate.to_vec()).is_ok_and(|base| {
                                    encode(base.source(), &base, StorageBudget::new(LIMIT))
                                        .is_ok_and(|encoded| {
                                            let mut observed = encoded.bytes().to_vec();
                                            if config.inject {
                                                observed.pop();
                                            }
                                            observed != candidate
                                        })
                                })
                            });
                            return Err(invalid(format!(
                                "{error}\n实际性质删除最小化输入={minimum:?}"
                            )));
                        }
                        run.check(encoded.reused_original(), "未编辑命中原 bytes", &bytes)?;
                        let source = original.source();
                        let points = boundaries(source);
                        let source_off = points[run.below(points.len())?];
                        let mut edited = source.to_owned();
                        edited.insert_str(source_off, pieces[run.below(pieces.len())?]);
                        let once = encode(&edited, &original, StorageBudget::new(LIMIT))?;
                        let reopened = baseline(once.bytes().to_vec())?;
                        // 引擎 §5.2 仅准许 CRLF/LF 表示等价；不折叠孤立 CR 或正文 BOM。
                        run.equal(
                            &reopened.source().replace("\r\n", "\n"),
                            &edited.replace("\r\n", "\n"),
                            "编辑重开只准许行尾表示差异",
                            edited.as_bytes(),
                        )?;
                        let twice =
                            encode(reopened.source(), &reopened, StorageBudget::new(LIMIT))?;
                        run.equal(&once.bytes(), &twice.bytes(), "编码重开幂等", &bytes)?;
                        run.equal(
                            &original.storage().bytes(),
                            &bytes.as_slice(),
                            "编码不改磁盘基线",
                            &bytes,
                        )?;
                        if step % 25 == 0 {
                            check_atomic(&mut run, &original, &edited, step)?;
                        }
                    }
                }
                if !bytes.is_empty() {
                    run.check(
                        matches!(
                            decode(&storage, StorageBudget::new(bytes.len() - 1)),
                            Err(DecodeError::BudgetExceeded { .. })
                        ),
                        "预算先于解码",
                        &bytes,
                    )?;
                }
            }
            Ok(())
        })();
        result
            .map_err(|error| run.failure(&error.to_string(), b"input is recorded in operations"))?;
    }
    Ok(())
}
