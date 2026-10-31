//! 4.3 存储往返与宿主基线结算的无头消费测试；不实现文件 I/O 或编辑历史。

use std::collections::HashSet;
use std::error::Error;
use std::io;
use std::ops::Range;

use aline_source::{
    DiskVersion, EncodedBytes, SourceBaseline, StorageBudget, StorageError, StorageSnapshot,
    decode, encode,
};

type TestResult<T = ()> = Result<T, Box<dyn Error>>;

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn budget() -> StorageBudget {
    StorageBudget::new(usize::MAX)
}

fn baseline(bytes: &[u8], version: DiskVersion) -> TestResult<SourceBaseline> {
    let storage = StorageSnapshot::new(bytes.to_vec(), version, budget())?;
    Ok(decode(&storage, budget())?.into_baseline())
}

fn parse_hex(text: &str) -> TestResult<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return Err(invalid("金样十六进制串长度必须为偶数").into());
    }
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|chunk| {
            let text = std::str::from_utf8(chunk)?;
            Ok(u8::from_str_radix(text, 16)?)
        })
        .collect()
}

/// 模块内部单坐标系的字符串替换；端点先验校验，不导出裸源码范围。
fn replace(source: &str, src_range: Range<usize>, replacement: &str) -> TestResult<String> {
    source
        .get(src_range.clone())
        .ok_or_else(|| invalid("测试替换范围不是合法 UTF-8 半开范围"))?;
    let mut edited = source.to_owned();
    edited.replace_range(src_range, replacement);
    Ok(edited)
}

#[test]
fn roundtrip_golden_preserves_bytes_and_local_patch_with_exact_inverse() -> TestResult {
    // 金样在测试实现前手写；冻结文本、待写 bytes、重开源码分别是独立显式预期。
    let golden = include_str!(
        "../../../docs/validation/m0-engine-foundation/group-4/step-4.3/roundtrip-golden.v1.txt"
    );
    let mut names = HashSet::new();
    for line in golden
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let fields: Vec<_> = line.split('|').collect();
        assert_eq!(fields.len(), 7);
        let name = fields[0];
        assert!(names.insert(name));
        let original_bytes = parse_hex(fields[1])?;
        let (start, end) = fields[2]
            .split_once(':')
            .ok_or_else(|| invalid("金样缺少源码范围"))?;
        let src_start = start.parse::<usize>()?;
        let src_end = end.parse::<usize>()?;
        let replacement_bytes = parse_hex(fields[3])?;
        let replacement = std::str::from_utf8(&replacement_bytes)?;
        let expected_frozen_bytes = parse_hex(fields[4])?;
        let expected_frozen = std::str::from_utf8(&expected_frozen_bytes)?;
        let expected_encoded = parse_hex(fields[5])?;
        let expected_reopened = parse_hex(fields[6])?;
        let original = baseline(&original_bytes, DiskVersion::initial())?;
        let original_encoded = encode(original.source(), &original, budget())?;
        assert!(original_encoded.reused_original(), "{name}");
        assert_eq!(original_encoded.bytes(), original_bytes, "{name}");

        let frozen = replace(original.source(), src_start..src_end, replacement)?;
        assert_eq!(frozen, expected_frozen, "{name}: 冻结源码精确值");
        // 源码只允许指定替换，其余前后缀逐字节不变，混合行尾例外只发生在编码中。
        assert_eq!(
            frozen.get(..src_start),
            original.source().get(..src_start),
            "{name}"
        );
        let after_end = src_start
            .checked_add(replacement.len())
            .ok_or_else(|| invalid("长度溢出"))?;
        assert_eq!(
            frozen.get(after_end..),
            original.source().get(src_end..),
            "{name}"
        );
        let encoded = encode(&frozen, &original, budget())?;
        assert_eq!(encoded.bytes(), expected_encoded, "{name}: 待写 bytes");
        assert_eq!(
            encoded.reused_original(),
            frozen == original.source(),
            "{name}"
        );
        assert_eq!(encoded.baseline_id(), original.id(), "{name}");
        let reopened = baseline(encoded.bytes(), DiskVersion::initial())?;
        assert_eq!(
            reopened.source().as_bytes(),
            expected_reopened,
            "{name}: 重开源码"
        );

        let removed = original
            .source()
            .get(src_start..src_end)
            .ok_or_else(|| invalid("逆补丁缺少原片段"))?;
        let restored = replace(&frozen, src_start..after_end, removed)?;
        assert_eq!(
            restored,
            original.source(),
            "{name}: 逆补丁必须精确恢复源码"
        );
        let restored_encoded = encode(&restored, &original, budget())?;
        assert!(
            restored_encoded.reused_original(),
            "{name}: Undo 回未替代基线"
        );
        assert_eq!(restored_encoded.bytes(), original_bytes, "{name}");
        assert_eq!(
            original.storage().bytes(),
            original_bytes,
            "{name}: 编码不能更新基线"
        );
    }
    assert_eq!(names.len(), 16);
    Ok(())
}

/// 测试内的最小宿主：只说明状态归属与实际回执结算，不是保存协调器。
struct Host {
    baseline: SourceBaseline,
    current: String,
    saved_input: Option<String>,
}

struct Prepared {
    frozen: String,
    encoded: EncodedBytes,
}

enum Outcome {
    Failed,
    Written(Vec<u8>),
}

impl Host {
    fn read(bytes: &[u8]) -> TestResult<Self> {
        let baseline = baseline(bytes, DiskVersion::initial())?;
        Ok(Self {
            current: baseline.source().to_owned(),
            saved_input: Some(baseline.source().to_owned()),
            baseline,
        })
    }

    fn prepare(&self) -> TestResult<Prepared> {
        Ok(Prepared {
            frozen: self.current.clone(),
            encoded: encode(&self.current, &self.baseline, budget())?,
        })
    }

    fn settle(&mut self, prepared: Prepared, outcome: Outcome) -> TestResult {
        match outcome {
            Outcome::Failed => Ok(()),
            Outcome::Written(actual_bytes) => {
                // 构造新基线只使用实际成功 bytes；解码失败前不发布半成品。
                let next = baseline(&actual_bytes, self.baseline.storage().version().next()?)?;
                let saved_input = if actual_bytes == prepared.encoded.bytes() {
                    Some(prepared.frozen)
                } else {
                    None
                };
                self.baseline = next;
                self.saved_input = saved_input;
                Ok(())
            }
        }
    }

    fn reload(&mut self, actual_bytes: &[u8]) -> TestResult {
        self.baseline = baseline(actual_bytes, self.baseline.storage().version().next()?)?;
        self.saved_input = None;
        Ok(())
    }
}

#[test]
fn roundtrip_failed_save_preserves_host_baseline_and_input_facts() -> TestResult {
    let mut host = Host::read(b"A\r\nB\nC")?;
    let old = host.baseline.clone();
    let saved_input = host.saved_input.clone();
    host.current = "A\r\nX\nC".to_owned();
    let prepared = host.prepare()?;
    assert_eq!(prepared.encoded.bytes(), b"A\nX\nC");
    assert_eq!(host.baseline.id(), old.id());
    host.settle(prepared, Outcome::Failed)?;
    assert_eq!(host.baseline.id(), old.id());
    assert_eq!(host.baseline.source(), old.source());
    assert_eq!(host.baseline.storage().bytes(), old.storage().bytes());
    assert_eq!(host.baseline.storage().version(), old.storage().version());
    assert_eq!(host.saved_input, saved_input);
    assert_eq!(host.current, "A\r\nX\nC");
    Ok(())
}

#[test]
fn roundtrip_success_uses_written_bytes_and_keeps_newer_input_unsaved() -> TestResult {
    let mut host = Host::read(b"A\r\nB\nC")?;
    let old_id = host.baseline.id().clone();
    host.current = "A\r\nX\nC".to_owned();
    let prepared = host.prepare()?;
    let written = prepared.encoded.bytes().to_vec();
    host.current = "A\r\nY\nC".to_owned();
    host.settle(prepared, Outcome::Written(written))?;
    assert_ne!(host.baseline.id(), &old_id);
    assert_eq!(
        host.baseline.storage().version(),
        DiskVersion::from_value(1)
    );
    assert_eq!(host.baseline.storage().bytes(), b"A\nX\nC");
    assert_eq!(host.baseline.source(), "A\nX\nC");
    assert_eq!(host.saved_input.as_deref(), Some("A\r\nX\nC"));
    assert_eq!(host.current, "A\r\nY\nC");
    assert_ne!(host.saved_input.as_deref(), Some(host.current.as_str()));
    let saved = host
        .saved_input
        .as_deref()
        .ok_or_else(|| invalid("缺成功冻结输入事实"))?;
    let equivalent = encode(saved, &host.baseline, budget())?;
    assert!(
        !equivalent.reused_original(),
        "存储等价不能放行精确源码原 bytes 通道"
    );
    assert_eq!(equivalent.bytes(), b"A\nX\nC");
    let exact = encode(host.baseline.source(), &host.baseline, budget())?;
    assert!(exact.reused_original());
    Ok(())
}

#[test]
fn roundtrip_undo_after_success_never_restores_expired_original_bytes() -> TestResult {
    let mut host = Host::read(b"A\r\nB\nC")?;
    let old = host.baseline.clone();
    let old_equality = encode(old.source(), &old, budget())?;
    host.current = "A\r\nX\nC".to_owned();
    let prepared = host.prepare()?;
    let written = prepared.encoded.bytes().to_vec();
    host.settle(prepared, Outcome::Written(written))?;
    host.current = old.source().to_owned();
    let after_undo = host.prepare()?;
    assert_eq!(old_equality.baseline_id(), old.id());
    assert_ne!(old_equality.baseline_id(), host.baseline.id());
    assert_eq!(after_undo.encoded.baseline_id(), host.baseline.id());
    assert!(!after_undo.encoded.reused_original());
    assert_eq!(after_undo.encoded.bytes(), b"A\nB\nC");
    assert_ne!(after_undo.encoded.bytes(), old_equality.bytes());
    assert_eq!(host.baseline.storage().bytes(), b"A\nX\nC");
    assert_eq!(
        host.baseline.storage().version(),
        DiskVersion::from_value(1)
    );
    Ok(())
}

#[test]
fn roundtrip_protective_bom_becomes_new_baseline_storage_fact() -> TestResult {
    let mut host = Host::read(b"A\xef\xbb\xbfB")?;
    let old = host.baseline.clone();
    host.current = "\u{feff}B".to_owned();
    let prepared = host.prepare()?;
    assert_eq!(prepared.encoded.bytes(), b"\xef\xbb\xbf\xef\xbb\xbfB");
    assert!(!host.baseline.metadata().has_bom());
    let written = prepared.encoded.bytes().to_vec();
    host.settle(prepared, Outcome::Written(written))?;
    assert!(host.baseline.metadata().has_bom());
    assert_eq!(host.baseline.source(), "\u{feff}B");
    let exact = host.prepare()?;
    assert!(exact.encoded.reused_original());
    assert_eq!(exact.encoded.bytes(), b"\xef\xbb\xbf\xef\xbb\xbfB");
    host.current = old.source().to_owned();
    let undo = host.prepare()?;
    assert!(!undo.encoded.reused_original());
    assert_eq!(undo.encoded.bytes(), b"\xef\xbb\xbfA\xef\xbb\xbfB");
    assert_ne!(undo.encoded.bytes(), old.storage().bytes());
    Ok(())
}

#[test]
fn roundtrip_success_cannot_substitute_requested_text_for_actual_bytes() -> TestResult {
    let mut host = Host::read(b"A\n")?;
    host.current = "B\n".to_owned();
    let prepared = host.prepare()?;
    // 反例用不同的完整 UTF-8 bytes，证明结算以实际输入事实而非请求正文建基线。
    host.settle(prepared, Outcome::Written(b"C\n".to_vec()))?;
    assert_eq!(host.baseline.storage().bytes(), b"C\n");
    assert_eq!(host.baseline.source(), "C\n");
    assert_eq!(host.current, "B\n");
    assert_eq!(host.saved_input, None);
    Ok(())
}

#[test]
fn roundtrip_reload_replaces_identity_policy_and_rejects_old_ranges() -> TestResult {
    let mut host = Host::read(b"A\r\nB\nC")?;
    let old = host.baseline.clone();
    let old_range = old.storage().range(0..1)?;
    host.reload(b"R\r\nS\r\n")?;
    assert_eq!(
        host.baseline.storage().version(),
        DiskVersion::from_value(1)
    );
    assert!(matches!(
        host.baseline.storage().slice(&old_range),
        Err(StorageError::BaselineMismatch)
    ));
    let old_text = encode(old.source(), &host.baseline, budget())?;
    assert!(!old_text.reused_original());
    assert_eq!(old_text.bytes(), b"A\r\nB\r\nC");
    assert_eq!(old_text.baseline_id(), host.baseline.id());
    assert_eq!(host.current, old.source());
    // 新读内容即使相同也建立新身份；整数版本或相同 bytes 都不是旧证据通行证。
    host.reload(old.storage().bytes())?;
    assert_ne!(host.baseline.id(), old.id());
    assert_eq!(
        host.baseline.storage().version(),
        DiskVersion::from_value(2)
    );
    assert!(matches!(
        host.baseline.storage().slice(&old_range),
        Err(StorageError::BaselineMismatch)
    ));
    let exact = host.prepare()?;
    assert!(exact.encoded.reused_original());
    assert_eq!(exact.encoded.baseline_id(), host.baseline.id());
    Ok(())
}
