//! 任务 4.1 的独立金样消费：只读规范手写表，验证 bytes -> 源码/元数据。
//!
//! 表中 `unchanged_encode_hex` 是未修改时原 bytes 通道的独立预期；本任务先检查
//! 它与 input 一致，任务 4.2 起由实际 encode 对照。测试不使用 `unwrap`/`expect`。

use aline_source::{
    DecodeError, DecodedSource, DiskVersion, LineEnding, StorageBudget, StorageError,
    StorageSnapshot, decode,
};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::PathBuf;

const GOLDEN_RELATIVE: &str =
    "../../docs/validation/m0-engine-foundation/group-4/step-4.1/decode-golden.v1.txt";
const EXPECTED_CASE_COUNT: usize = 32;

#[derive(Debug)]
struct GoldenCase {
    name: String,
    input: Vec<u8>,
    decode_budget: usize,
    expected_source: Option<Vec<u8>>,
    has_bom: bool,
    crlf_pairs: usize,
    unpaired_lf: usize,
    line_ending: LineEnding,
    unchanged_encode: Vec<u8>,
    expected_error: Option<ExpectedError>,
}

#[derive(Debug)]
enum ExpectedError {
    Budget {
        needed: usize,
        limit: usize,
    },
    Utf16Bom {
        little_endian: bool,
    },
    InvalidUtf8 {
        valid_up_to: usize,
        error_len: Option<usize>,
    },
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
        if case.expected_error.is_none() && case.unchanged_encode != case.input {
            return Err(invalid_data(format!(
                "用例 {}: 金样内部一致性要求 unchanged_encode_hex 等于 input_hex（这不是 encode 证据）",
                case.name
            )));
        }
        cases.push(case);
    }
    if cases.len() != EXPECTED_CASE_COUNT {
        return Err(invalid_data(format!(
            "金样用例数 {} 与独立预期 {} 不符",
            cases.len(),
            EXPECTED_CASE_COUNT
        )));
    }
    Ok(cases)
}

fn parse_case(line: &str) -> Result<GoldenCase, String> {
    let fields: Vec<&str> = line.split('|').collect();
    if fields.len() != 11 {
        return Err(format!("期望 11 个字段，实际 {}", fields.len()));
    }
    let expected = match fields[3] {
        "ok" => true,
        "err" => false,
        other => return Err(format!("未知 expected {other:?}")),
    };
    let source = parse_hex(fields[4])?;
    let line_ending = match fields[8] {
        "lf" => LineEnding::Lf,
        "crlf" => LineEnding::Crlf,
        other => return Err(format!("未知 line_ending {other:?}")),
    };
    let expected_error = match fields[10] {
        "-" => None,
        tag => Some(parse_error(tag)?),
    };
    match (expected, &expected_error) {
        (true, Some(_)) => return Err("expected=ok 却给出了 error 标签".to_owned()),
        (false, None) => return Err("expected=err 却缺少 error 标签".to_owned()),
        _ => {}
    }
    let case = GoldenCase {
        name: fields[0].to_owned(),
        input: parse_hex(fields[1])?,
        decode_budget: parse_usize(fields[2])?,
        expected_source: expected.then_some(source),
        has_bom: parse_flag(fields[5])?,
        crlf_pairs: parse_usize(fields[6])?,
        unpaired_lf: parse_usize(fields[7])?,
        line_ending,
        unchanged_encode: parse_hex(fields[9])?,
        expected_error,
    };
    if case.expected_error.is_some() {
        let unused_columns_are_blank = case.expected_source.is_none()
            && !case.has_bom
            && case.crlf_pairs == 0
            && case.unpaired_lf == 0
            && case.line_ending == LineEnding::Lf
            && case.unchanged_encode.is_empty();
        if !unused_columns_are_blank {
            return Err("err 用例的源码/元数据/未改 bytes 列必须为空或规范零值".to_owned());
        }
    }
    Ok(case)
}

fn parse_usize(text: &str) -> Result<usize, String> {
    text.parse::<usize>()
        .map_err(|_| format!("无法解析无符号整数 {text:?}"))
}

fn parse_flag(text: &str) -> Result<bool, String> {
    match text {
        "0" => Ok(false),
        "1" => Ok(true),
        other => Err(format!("无法解析 0/1 标志 {other:?}")),
    }
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

fn parse_error(text: &str) -> Result<ExpectedError, String> {
    let parts: Vec<&str> = text.split(':').collect();
    match parts.as_slice() {
        ["budget", needed, limit] => Ok(ExpectedError::Budget {
            needed: parse_usize(needed)?,
            limit: parse_usize(limit)?,
        }),
        ["utf16-bom", "little"] => Ok(ExpectedError::Utf16Bom {
            little_endian: true,
        }),
        ["utf16-bom", "big"] => Ok(ExpectedError::Utf16Bom {
            little_endian: false,
        }),
        ["invalid-utf8", valid_up_to, error_len] => Ok(ExpectedError::InvalidUtf8 {
            valid_up_to: parse_usize(valid_up_to)?,
            error_len: match *error_len {
                "none" => None,
                value => Some(parse_usize(value)?),
            },
        }),
        _ => Err(format!("未知 error 标签 {text:?}")),
    }
}

fn error_matches(actual: &DecodeError, expected: &ExpectedError) -> bool {
    match (actual, expected) {
        (
            DecodeError::BudgetExceeded {
                needed: actual_needed,
                limit: actual_limit,
            },
            ExpectedError::Budget { needed, limit },
        ) => actual_needed == needed && actual_limit == limit,
        (
            DecodeError::Utf16Bom {
                little_endian: actual_little_endian,
            },
            ExpectedError::Utf16Bom { little_endian },
        ) => actual_little_endian == little_endian,
        (
            DecodeError::InvalidUtf8 {
                valid_up_to: actual_valid_up_to,
                error_len: actual_error_len,
            },
            ExpectedError::InvalidUtf8 {
                valid_up_to,
                error_len,
            },
        ) => actual_valid_up_to == valid_up_to && actual_error_len == error_len,
        _ => false,
    }
}

#[test]
fn decode_golden_cases_match_independent_expectations() -> Result<(), Box<dyn std::error::Error>> {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<DecodedSource>();

    let cases = load_cases()?;
    for case in &cases {
        let snapshot = StorageSnapshot::new(
            case.input.clone(),
            DiskVersion::initial(),
            StorageBudget::new(usize::MAX),
        )?;
        let retained = snapshot.clone();
        let result = decode(&snapshot, StorageBudget::new(case.decode_budget));
        match (result, &case.expected_error) {
            (Ok(decoded), None) => {
                let expected_source = case.expected_source.as_deref().ok_or_else(|| {
                    invalid_data(format!("用例 {}: ok 分支缺少源码预期", case.name))
                })?;
                let metadata = decoded.metadata();
                assert_eq!(
                    decoded.source().as_bytes(),
                    expected_source,
                    "用例 {} 的编辑源码不符",
                    case.name
                );
                assert_eq!(
                    metadata.has_bom(),
                    case.has_bom,
                    "用例 {} 的 BOM 标志不符",
                    case.name
                );
                assert_eq!(
                    metadata.crlf_pairs(),
                    case.crlf_pairs,
                    "用例 {} 的 CRLF 对数不符",
                    case.name
                );
                assert_eq!(
                    metadata.unpaired_lf(),
                    case.unpaired_lf,
                    "用例 {} 的未配对 LF 数不符",
                    case.name
                );
                assert_eq!(
                    metadata.line_ending(),
                    case.line_ending,
                    "用例 {} 的行尾策略不符",
                    case.name
                );
                assert_eq!(
                    decoded.storage().bytes(),
                    case.input.as_slice(),
                    "用例 {} 的存储字节被改写",
                    case.name
                );
                assert_eq!(
                    decoded.storage().id(),
                    snapshot.id(),
                    "用例 {} 丢失了存储基线身份",
                    case.name
                );
            }
            (Err(error), Some(expected)) => {
                assert!(
                    error_matches(&error, expected),
                    "用例 {} 的错误不符：期望 {expected:?}，实际 {error:?}",
                    case.name
                );
            }
            (Ok(decoded), Some(expected)) => {
                return Err(invalid_data(format!(
                    "用例 {}: 期望错误 {expected:?}，实际成功返回源码长度 {}",
                    case.name,
                    decoded.source().len()
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
        assert_eq!(
            retained.bytes(),
            case.input.as_slice(),
            "用例 {} 失败后输入字节不应变化",
            case.name
        );
    }
    Ok(())
}

#[test]
fn snapshot_and_decode_budgets_are_distinct_layers() -> Result<(), Box<dyn std::error::Error>> {
    let rejected =
        StorageSnapshot::new(vec![b'A'; 4], DiskVersion::initial(), StorageBudget::new(3));
    assert!(matches!(
        rejected,
        Err(StorageError::BudgetExceeded {
            needed: 4,
            limit: 3
        })
    ));

    let snapshot =
        StorageSnapshot::new(vec![b'A'; 3], DiskVersion::initial(), StorageBudget::new(3))?;
    let decoded = decode(&snapshot, StorageBudget::new(3))?;
    assert_eq!(decoded.source(), "AAA");

    let rejected_at_decode = decode(&snapshot, StorageBudget::new(2));
    assert!(matches!(
        rejected_at_decode,
        Err(DecodeError::BudgetExceeded {
            needed: 3,
            limit: 2
        })
    ));
    Ok(())
}

#[test]
fn decode_does_not_write_the_original_file() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("aline-source-decode-no-write");
    fs::create_dir_all(&dir)?;

    let invalid_path = dir.join("invalid-utf8.bin");
    let invalid_bytes = [0x41_u8, 0xff, 0x42];
    fs::write(&invalid_path, invalid_bytes)?;
    let snapshot = StorageSnapshot::new(
        fs::read(&invalid_path)?,
        DiskVersion::initial(),
        StorageBudget::new(16),
    )?;
    let error = decode(&snapshot, StorageBudget::new(16));
    assert!(matches!(
        error,
        Err(DecodeError::InvalidUtf8 {
            valid_up_to: 1,
            error_len: Some(1)
        })
    ));
    assert_eq!(fs::read(&invalid_path)?, invalid_bytes);

    let valid_path = dir.join("valid-crlf.bin");
    let valid_bytes = [0xef_u8, 0xbb, 0xbf, 0x41, 0x0d, 0x0a, 0x42];
    fs::write(&valid_path, valid_bytes)?;
    let snapshot = StorageSnapshot::new(
        fs::read(&valid_path)?,
        DiskVersion::initial(),
        StorageBudget::new(16),
    )?;
    let decoded = decode(&snapshot, StorageBudget::new(16))?;
    assert_eq!(decoded.source(), "A\r\nB");
    assert_eq!(decoded.metadata().crlf_pairs(), 1);
    assert_eq!(fs::read(&valid_path)?, valid_bytes);
    Ok(())
}
