//! 任务 4.2 的独立金样消费：原 bytes 通道、多数派编码与逆补丁恢复。
//!
//! 金样表由人工按规范手写；测试只读该表，不调用实现生成预期。逆补丁用
//! 测试内的字符串区间编辑模拟，验证恢复后的冻结源码与基线原 bytes。

use aline_source::{
    DiskVersion, EncodeError, EncodedBytes, SourceBaseline, StorageBudget, StorageSnapshot, decode,
    encode,
};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

const GOLDEN_RELATIVE: &str =
    "../../docs/validation/m0-engine-foundation/group-4/step-4.2/encode-golden.v1.txt";
const EXPECTED_OK_COUNT: usize = 34;
const EXPECTED_ERR_COUNT: usize = 4;
const BOM: &[u8] = &[0xef, 0xbb, 0xbf];

#[derive(Debug)]
struct GoldenCase {
    name: String,
    input: Vec<u8>,
    frozen: Vec<u8>,
    budget: usize,
    expected_output: Option<Vec<u8>>,
    reused_original: Option<bool>,
    expected_redecode: Option<Vec<u8>>,
    expected_error: Option<(usize, usize)>,
}

fn invalid_data(message: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn load_cases() -> Result<Vec<GoldenCase>, io::Error> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(GOLDEN_RELATIVE);
    let text = fs::read_to_string(&path)
        .map_err(|error| invalid_data(format!("读取金样 {} 失败: {error}", path.display())))?;
    let mut cases = Vec::new();
    let mut names = HashSet::new();
    for (index, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let case = parse_case(line)
            .map_err(|error| invalid_data(format!("金样第 {} 行: {error}", index + 1)))?;
        if !names.insert(case.name.clone()) {
            return Err(invalid_data(format!("金样用例名重复: {}", case.name)));
        }
        cases.push(case);
    }
    let ok_count = cases
        .iter()
        .filter(|case| case.expected_error.is_none())
        .count();
    let err_count = cases.len() - ok_count;
    if (ok_count, err_count) != (EXPECTED_OK_COUNT, EXPECTED_ERR_COUNT) {
        return Err(invalid_data(format!(
            "金样成功/错误用例数 ({ok_count}, {err_count}) 与独立预期 \
             ({EXPECTED_OK_COUNT}, {EXPECTED_ERR_COUNT}) 不符"
        )));
    }
    Ok(cases)
}

fn parse_case(line: &str) -> Result<GoldenCase, String> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() != 9 {
        return Err(format!("期望 9 个字段，实际 {}", fields.len()));
    }
    let (expected_output, reused_original, expected_redecode, expected_error) = match fields[4] {
        "ok" => {
            if fields[8] != "-" {
                return Err("expected=ok 却给出了错误标签".to_owned());
            }
            (
                Some(parse_hex(fields[5])?),
                Some(match fields[6] {
                    "0" => false,
                    "1" => true,
                    other => return Err(format!("无法解析 reused_original {other:?}")),
                }),
                Some(parse_hex(fields[7])?),
                None,
            )
        }
        "err" => {
            if !fields[5].is_empty() || !fields[7].is_empty() || fields[6] != "-" {
                return Err("错误行的输出/再解码/复用列必须为空".to_owned());
            }
            (None, None, None, Some(parse_budget_error(fields[8])?))
        }
        other => return Err(format!("未知 expected {other:?}")),
    };
    Ok(GoldenCase {
        name: fields[0].to_owned(),
        input: parse_hex(fields[1])?,
        frozen: parse_hex(fields[2])?,
        budget: parse_usize(fields[3])?,
        expected_output,
        reused_original,
        expected_redecode,
        expected_error,
    })
}

fn parse_budget_error(text: &str) -> Result<(usize, usize), String> {
    match text.split(':').collect::<Vec<_>>().as_slice() {
        ["budget", needed, limit] => Ok((parse_usize(needed)?, parse_usize(limit)?)),
        _ => Err(format!("未知 error 标签 {text:?}")),
    }
}

fn parse_usize(text: &str) -> Result<usize, String> {
    text.parse::<usize>()
        .map_err(|_| format!("无法解析无符号整数 {text:?}"))
}

fn parse_hex(text: &str) -> Result<Vec<u8>, String> {
    if !text.len().is_multiple_of(2) {
        return Err(format!("十六进制串长度为奇数: {text:?}"));
    }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    let (pairs, remainder) = text.as_bytes().as_chunks::<2>();
    if !remainder.is_empty() {
        return Err(format!("十六进制串长度为奇数: {text:?}"));
    }
    for pair in pairs {
        let high = hex_nibble(pair[0])?;
        let low = hex_nibble(pair[1])?;
        bytes.push(high << 4 | low);
    }
    Ok(bytes)
}

fn hex_nibble(byte: u8) -> Result<u8, String> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        other => Err(format!("非法十六进制字符 {:?}", char::from(other))),
    }
}

/// 把每个 CRLF/未配对 LF 行尾单元归一为 LF，孤立 CR 原样保留。
fn canonical_line_endings(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while let Some(byte) = bytes.get(index) {
        match byte {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                out.push(b'\n');
                index += 2;
            }
            b'\n' => {
                out.push(b'\n');
                index += 1;
            }
            other => {
                out.push(*other);
                index += 1;
            }
        }
    }
    out
}

fn decode_to_baseline(bytes: Vec<u8>) -> Result<SourceBaseline, Box<dyn std::error::Error>> {
    let snapshot = StorageSnapshot::new(
        bytes,
        DiskVersion::initial(),
        StorageBudget::new(usize::MAX),
    )?;
    Ok(decode(&snapshot, StorageBudget::new(usize::MAX))?.into_baseline())
}

fn frozen_text(bytes: &[u8]) -> Result<&str, io::Error> {
    std::str::from_utf8(bytes).map_err(|error| invalid_data(format!("冻结源码不是 UTF-8: {error}")))
}

#[test]
fn encode_golden_cases_match_independent_expectations() -> Result<(), Box<dyn std::error::Error>> {
    let cases = load_cases()?;
    for case in &cases {
        let baseline = decode_to_baseline(case.input.clone())?;
        let frozen = frozen_text(&case.frozen)?;
        let encoded = encode(frozen, &baseline, StorageBudget::new(case.budget));
        match (encoded, case.expected_error) {
            (Ok(encoded), None) => {
                let expected_output = case.expected_output.as_deref().ok_or_else(|| {
                    invalid_data(format!("用例 {}: ok 分支缺输出预期", case.name))
                })?;
                let expected_reused = case.reused_original.ok_or_else(|| {
                    invalid_data(format!("用例 {}: ok 分支缺复用预期", case.name))
                })?;
                let expected_redecode = case.expected_redecode.as_deref().ok_or_else(|| {
                    invalid_data(format!("用例 {}: ok 分支缺再解码预期", case.name))
                })?;
                assert_eq!(
                    encoded.bytes(),
                    expected_output,
                    "用例 {} 的编码字节不符",
                    case.name
                );
                assert_eq!(
                    encoded.reused_original(),
                    expected_reused,
                    "用例 {} 的原 bytes 通道判定不符",
                    case.name
                );
                assert_eq!(
                    encoded.baseline_id(),
                    baseline.storage().id(),
                    "用例 {} 丢失了基线身份",
                    case.name
                );
                let output_snapshot = StorageSnapshot::new(
                    encoded.bytes().to_vec(),
                    DiskVersion::initial(),
                    StorageBudget::new(usize::MAX),
                )?;
                let redecoded = decode(&output_snapshot, StorageBudget::new(usize::MAX))?;
                assert_eq!(
                    redecoded.source().as_bytes(),
                    expected_redecode,
                    "用例 {} 的再解码源码不符",
                    case.name
                );
                assert_eq!(
                    canonical_line_endings(frozen.as_bytes()),
                    canonical_line_endings(redecoded.source().as_bytes()),
                    "用例 {} 不满足限定行尾等价",
                    case.name
                );
                let protective_bom = !expected_reused && frozen.starts_with('\u{feff}');
                assert_eq!(
                    encoded.bytes().starts_with(BOM),
                    case.input.starts_with(BOM) || protective_bom,
                    "用例 {} 的 BOM 重放/保护不符",
                    case.name
                );
                if expected_reused {
                    assert_eq!(
                        frozen,
                        baseline.source(),
                        "用例 {} 复用判定来源不符",
                        case.name
                    );
                } else {
                    assert_ne!(
                        frozen,
                        baseline.source(),
                        "用例 {} 不应命中原 bytes",
                        case.name
                    );
                }
                assert_eq!(
                    baseline.storage().bytes(),
                    case.input.as_slice(),
                    "用例 {} 修改了基线原 bytes",
                    case.name
                );
            }
            (Err(error), Some((needed, limit))) => {
                assert!(
                    matches!(
                        error,
                        EncodeError::BudgetExceeded {
                            needed: actual_needed,
                            limit: actual_limit
                        } if actual_needed == needed && actual_limit == limit
                    ),
                    "用例 {} 的错误不符：期望 budget:{needed}:{limit}，实际 {error:?}",
                    case.name
                );
            }
            (Ok(encoded), Some(expected)) => {
                return Err(invalid_data(format!(
                    "用例 {}: 期望预算错误 {expected:?}，实际成功返回 {} 字节",
                    case.name,
                    encoded.bytes().len()
                ))
                .into());
            }
            (Err(error), None) => {
                return Err(invalid_data(format!(
                    "用例 {}: 期望成功，实际返回 {error:?}",
                    case.name
                ))
                .into());
            }
        }
    }
    Ok(())
}

#[test]
fn budget_boundary_and_categories_are_distinct() -> Result<(), Box<dyn std::error::Error>> {
    let baseline = decode_to_baseline(b"A\n".to_vec())?;

    let exact = encode("AB\n", &baseline, StorageBudget::new(3))?;
    assert_eq!(exact.bytes(), b"AB\n");

    let rejected = encode("AB\n", &baseline, StorageBudget::new(2));
    assert!(matches!(
        rejected,
        Err(EncodeError::BudgetExceeded {
            needed: 3,
            limit: 2
        })
    ));

    let unchanged = encode(baseline.source(), &baseline, StorageBudget::new(2))?;
    assert!(unchanged.reused_original());
    assert_eq!(unchanged.bytes(), b"A\n");

    let unchanged_rejected = encode(baseline.source(), &baseline, StorageBudget::new(1));
    assert!(matches!(
        unchanged_rejected,
        Err(EncodeError::BudgetExceeded {
            needed: 2,
            limit: 1
        })
    ));
    Ok(())
}

#[test]
fn encoded_bytes_carry_their_own_baseline_identity() -> Result<(), Box<dyn std::error::Error>> {
    let first = decode_to_baseline(b"A\nB\n".to_vec())?;
    let second = decode_to_baseline(b"A\nB\n".to_vec())?;
    let first_encoded = encode(first.source(), &first, StorageBudget::new(usize::MAX))?;
    let second_encoded = encode(second.source(), &second, StorageBudget::new(usize::MAX))?;
    assert_eq!(first_encoded.bytes(), second_encoded.bytes());
    assert_eq!(first_encoded.baseline_id(), first.storage().id());
    assert_eq!(second_encoded.baseline_id(), second.storage().id());
    assert_ne!(first.storage().id(), second.storage().id());
    Ok(())
}

fn assert_edit_and_inverse(
    input: &[u8],
    expected_frozen: &str,
    edit: impl FnOnce(&str) -> String,
    inverse: impl FnOnce(&str) -> String,
    expected_encoded: &[u8],
) -> Result<(), Box<dyn std::error::Error>> {
    let baseline = decode_to_baseline(input.to_vec())?;
    let original = baseline.source().to_owned();
    let frozen = edit(&original);
    assert_eq!(frozen, expected_frozen, "冻结源码精确值不符");
    let encoded = encode(&frozen, &baseline, StorageBudget::new(usize::MAX))?;
    assert_eq!(encoded.bytes(), expected_encoded);
    assert!(!encoded.reused_original());

    let output_snapshot = StorageSnapshot::new(
        encoded.bytes().to_vec(),
        DiskVersion::initial(),
        StorageBudget::new(usize::MAX),
    )?;
    let redecoded = decode(&output_snapshot, StorageBudget::new(usize::MAX))?;
    assert_eq!(
        canonical_line_endings(frozen.as_bytes()),
        canonical_line_endings(redecoded.source().as_bytes())
    );

    let restored = inverse(&frozen);
    assert_eq!(restored, original, "逆补丁必须精确恢复冻结源码");
    let restored_encoded = encode(&restored, &baseline, StorageBudget::new(usize::MAX))?;
    assert!(restored_encoded.reused_original());
    assert_eq!(restored_encoded.bytes(), input, "恢复后必须返回原 bytes");
    Ok(())
}

#[test]
fn inverse_patches_restore_exact_source_and_original_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    // CR-X-LF 删除 X：LF 策略允许新拼接的 CRLF 重开为 LF。
    assert_edit_and_inverse(
        b"A\rX\nB",
        "A\r\nB",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(2..3, "");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(2..2, "X");
            restored
        },
        b"A\nB",
    )?;

    // 同一删除后孤立 CR 紧邻 CRLF：保留例外，编码逐字节不变。
    assert_edit_and_inverse(
        b"A\rX\r\nB\nC\nD",
        "A\r\r\nB\nC\nD",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(2..3, "");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(2..2, "X");
            restored
        },
        b"A\r\r\nB\nC\nD",
    )?;

    // 边界插入 LF：删除孤立 CR 后的 LF 会形成新 CRLF 单元。
    assert_edit_and_inverse(
        b"A\rX",
        "A\r\nX",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(2..2, "\n");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(2..3, "");
            restored
        },
        b"A\nX",
    )?;

    // 边界删除 CRLF 中的 LF：留下孤立 CR，必须逐字节保留。
    assert_edit_and_inverse(
        b"A\r\nB",
        "A\rB",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(2..3, "");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(2..2, "\n");
            restored
        },
        b"A\rB",
    )?;

    // 粘贴混合行尾：LF 策略统一为 LF，孤立 CR 不丢失。
    assert_edit_and_inverse(
        b"A\nB\n",
        "A\nx\r\ny\nB\n",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(2..2, "x\r\ny\n");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(2..7, "");
            restored
        },
        b"A\nx\ny\nB\n",
    )?;

    // 行首正文 U+FEFF：删除其前字符后补写保护性 BOM，逆补丁恢复原 bytes。
    assert_edit_and_inverse(
        b"A\xEF\xBB\xBFB",
        "\u{feff}B",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(0..1, "");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.insert(0, 'A');
            restored
        },
        b"\xEF\xBB\xBF\xEF\xBB\xBFB",
    )?;

    // 删除 CRLF 中的 LF：结果 bytes 恰与输入相同，但未走原 bytes 通道。
    assert_edit_and_inverse(
        b"A\r\nB",
        "A\rB",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(2..3, "");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(2..2, "\n");
            restored
        },
        b"A\rB",
    )?;

    // 粘贴孤立 CR 到 CRLF 文档：CRLF 策略下孤立 CR 不丢失。
    assert_edit_and_inverse(
        b"A\r\nB\r\n",
        "A\r\nx\ry\r\nB\r\n",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(3..3, "x\ry\r\n");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(3..8, "");
            restored
        },
        b"A\r\nx\ry\r\nB\r\n",
    )?;

    // CRLF 策略逆补丁：未配对 LF 编码为 CRLF，逆补丁精确恢复原 bytes。
    assert_edit_and_inverse(
        b"A\r\nB\r\n",
        "A\r\nC\nB\r\n",
        |source| {
            let mut edited = source.to_owned();
            edited.replace_range(3..3, "C\n");
            edited
        },
        |edited| {
            let mut restored = edited.to_owned();
            restored.replace_range(3..5, "");
            restored
        },
        b"A\r\nC\r\nB\r\n",
    )?;
    Ok(())
}

#[test]
fn guard_rule_roundtrips_all_small_cr_lf_sources() -> Result<(), Box<dyn std::error::Error>> {
    // 属性检查：对 {A, CR, LF} 长度不超过 6 的全部串，在 LF/CRLF 两种策略下
    // encode 后再 decode 必须与冻结源码在限定行尾表示上等价。该检查直接暴露
    // 守卫规则的反例，不依赖参考编码器的同一邻接判定。
    let lf_baseline = decode_to_baseline(b"A\n".to_vec())?;
    let crlf_baseline = decode_to_baseline(b"A\r\n".to_vec())?;
    for baseline in [&lf_baseline, &crlf_baseline] {
        for length in 0..=6usize {
            let mut indexes = vec![0usize; length];
            loop {
                let mut source = String::with_capacity(length);
                for &index in &indexes {
                    source.push(match index {
                        0 => 'A',
                        1 => '\r',
                        _ => '\n',
                    });
                }
                let encoded = encode(&source, baseline, StorageBudget::new(usize::MAX))?;
                let snapshot = StorageSnapshot::new(
                    encoded.bytes().to_vec(),
                    DiskVersion::initial(),
                    StorageBudget::new(usize::MAX),
                )?;
                let redecoded = decode(&snapshot, StorageBudget::new(usize::MAX))?;
                assert_eq!(
                    canonical_line_endings(source.as_bytes()),
                    canonical_line_endings(redecoded.source().as_bytes()),
                    "源码 {source:?} 在行尾策略 {:?} 下不满足限定行尾等价",
                    baseline.metadata().line_ending()
                );
                let mut position = 0usize;
                loop {
                    if position == length {
                        break;
                    }
                    indexes[position] += 1;
                    if indexes[position] < 3 {
                        break;
                    }
                    indexes[position] = 0;
                    position += 1;
                }
                if position == length {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[test]
fn public_encoding_types_are_send_and_sync() {
    // 静态度量：类型可跨线程传递不等于引擎启动线程；引擎仍无线程与运行时全局状态。
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<SourceBaseline>();
    assert_send_sync::<EncodedBytes>();
    assert_send_sync::<EncodeError>();
}
