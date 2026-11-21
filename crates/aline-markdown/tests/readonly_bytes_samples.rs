//! M1 只读字节保真样本的形态回归。
//!
//! 7.3 要求五个保真样本（BOM / 混合行尾 / 孤立 CR / 空白 / 无尾换行）都能走完
//! 打开→滚动→命中→主动跳转→两视图往返→关闭。本文件锁定每个样本必须同时满足
//! 两件事：保真特征仍在，且存在一个可跳转的目录目标（`[toc]` + 至少一个标题）。
//! 只读样本不依赖窗口与线程。

use std::error::Error;
use std::path::{Path, PathBuf};

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{NavigationKind, ParseBudget, ParseConfig, parse};

const SOURCE_BUDGET: usize = 8 * 1024 * 1024;

/// 与 `scripts/run_m1_readonly.py` 的 `BYTES_CASES` 逐字节一致。
const BYTES_CASES: [(&str, &[u8]); 5] = [
    (
        "bom",
        b"\xef\xbb\xbf# BOM\r\n\r\n[toc]\r\n\r\n\xe4\xb8\xad\xe6\x96\x87\xe6\xae\xb5\xe8\x90\xbd\xe3\x80\x82\r\n",
    ),
    (
        "mixed-eol",
        b"# \xe6\xb7\xb7\xe5\x90\x88\r\n\r\n[toc]\n\r\nfirst\r\nsecond\nthird\rfourth\r\n",
    ),
    (
        "lone-cr",
        b"# \xe5\xad\xa4\xe7\xab\x8b\r\r[toc]\r\ralpha\rbeta\rgamma",
    ),
    (
        "whitespace-only",
        b"# \xe7\xa9\xba\xe7\x99\xbd\n\n[toc]\n\n   \n\t\n  \t  \n",
    ),
    (
        "no-trailing-newline",
        b"# \xe6\x97\xa0\xe5\xb0\xbe\n\n[toc]\n\n\xe6\x9c\xab\xe5\xb0\xbe\xe6\xb2\xa1\xe6\x9c\x89\xe6\x8d\xa2\xe8\xa1\x8c",
    ),
];

fn decode(bytes: &[u8]) -> Result<&str, Box<dyn Error>> {
    // 与 `aline_source::decode` 一致：只去掉一个 UTF-8 BOM。
    let body = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    Ok(std::str::from_utf8(body)?)
}

fn toc_targets(source: &str) -> Result<usize, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(SOURCE_BUDGET))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    // 目录条目是 `Toc` 种类，其目标是文档里全部标题范围；标题条目本身不是跳转目标。
    Ok(parsed
        .navigation()
        .entries()
        .iter()
        .filter(|entry| entry.kind() == NavigationKind::Toc)
        .filter(|entry| !entry.targets().is_empty())
        .count())
}

#[test]
fn every_byte_fidelity_sample_has_a_jumpable_toc_target() -> Result<(), Box<dyn Error>> {
    for (name, bytes) in BYTES_CASES {
        let source = decode(bytes)?;
        assert!(toc_targets(source)? > 0, "样本 {name} 没有可跳转的目录目标");
    }
    Ok(())
}

#[test]
fn every_byte_fidelity_sample_keeps_its_fidelity_feature() -> Result<(), Box<dyn Error>> {
    for (name, bytes) in BYTES_CASES {
        let source = decode(bytes)?;
        match name {
            "bom" => {
                assert!(
                    bytes.starts_with(b"\xef\xbb\xbf"),
                    "bom 样本必须仍带 UTF-8 BOM"
                );
                assert_eq!(bom_offsets(bytes), vec![0], "bom 样本的 BOM 只能出现在开头");
                assert!(source.contains("\r\n"), "bom 样本必须仍用 CRLF");
            }
            "mixed-eol" => {
                assert!(source.contains("\r\n"), "mixed-eol 必须仍含 CRLF");
                assert!(source.contains('\n'), "mixed-eol 必须仍含 LF");
                assert!(
                    has_lone_cr(source),
                    "mixed-eol 必须仍含**孤立** CR（CR 不接 LF）"
                );
            }
            "lone-cr" => {
                assert!(source.contains('\r'), "lone-cr 必须仍含孤立 CR");
                assert!(!source.contains("\r\n"), "lone-cr 不得含 CRLF");
                assert!(
                    !source.contains('\n'),
                    "lone-cr 不得含单独的 LF，否则就不是纯 CR 行尾"
                );
            }
            "whitespace-only" => {
                // 标题与 `[toc]` 是为跳转补的目标；正文仍必须只有空白与空行。
                let body: Vec<&str> = source
                    .lines()
                    .filter(|line| {
                        let trimmed = line.trim();
                        !trimmed.starts_with('#') && !trimmed.eq_ignore_ascii_case("[toc]")
                    })
                    .collect();
                assert!(
                    body.iter().all(|line| line.trim().is_empty()),
                    "whitespace-only 正文必须只含空白与空行"
                );
                assert!(
                    body.iter().any(|line| line.contains([' ', '\t'])),
                    "whitespace-only 必须仍含只由空格或制表符构成的行"
                );
            }
            "no-trailing-newline" => {
                assert!(
                    !source.ends_with('\n') && !source.ends_with('\r'),
                    "no-trailing-newline 不得有尾换行"
                );
            }
            _ => return Err(format!("未知样本 {name}").into()),
        }
    }
    Ok(())
}

/// 孤立 CR：后面不接 LF 的 CR。CRLF 里的 CR 不算。
fn has_lone_cr(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes
        .iter()
        .enumerate()
        .any(|(index, byte)| *byte == b'\r' && bytes.get(index + 1) != Some(&b'\n'))
}

/// UTF-8 BOM 字节序列在字节流中的全部起始偏移。
fn bom_offsets(bytes: &[u8]) -> Vec<usize> {
    bytes
        .windows(3)
        .enumerate()
        .filter(|(_, window)| *window == b"\xef\xbb\xbf")
        .map(|(index, _)| index)
        .collect()
}

/// 逐字节核对：真实取证入口 `scripts/run_m1_readonly.py` 的 `BYTES_CASES`
/// 与本文件的样本必须是同一批字节。两处各自维护一份字面量，任何一侧改动都可能
/// 让「保真形态回归」与「真实取证」悄悄脱钩，因此在此锁定两侧一致。
#[test]
fn the_real_evidence_entry_uses_the_same_sample_bytes() -> Result<(), Box<dyn Error>> {
    let script = repo_path("scripts/run_m1_readonly.py");
    let text = std::fs::read_to_string(&script)?;
    for (name, bytes) in BYTES_CASES {
        let literal = python_bytes_literal(bytes);
        let block = case_block(&text, name).ok_or_else(|| format!("{script:?} 缺少用例 {name}"))?;
        assert!(
            block_uses_literal(block, &literal),
            "{script:?} 的用例 {name} 与本文件字节不一致：\n  期望字面量：{literal}\n  脚本用例块：{block}"
        );
    }
    Ok(())
}

/// 最小违规反例：脚本里的样本字节一旦改动，守卫必须判定为不一致。
///
/// 反例走的是与正例**同一条**判定路径：先取出脚本里 `BYTES_CASES` 中某个
/// 用例的块，再拿本文件的字节字面量去比对。只改一个字面量文本不足以说明
/// 守卫有效——必须证明「真实脚本文本被改动」这件事会被守卫抓住。
#[test]
fn the_sample_drift_guard_rejects_a_changed_literal() -> Result<(), Box<dyn Error>> {
    let script = repo_path("scripts/run_m1_readonly.py");
    let text = std::fs::read_to_string(&script)?;
    let (name, bytes) = BYTES_CASES[0];
    let literal = python_bytes_literal(bytes);

    // 正例：未改动的真实脚本必须通过同一判定路径。
    let intact = case_block(&text, name).ok_or_else(|| format!("{script:?} 缺少用例 {name}"))?;
    assert!(
        block_uses_literal(intact, &literal),
        "未改动的真实脚本必须被判为一致"
    );

    // 反例：只改动真实脚本文本里那一个字节，再走同一判定路径。
    let drifted_text = text.replacen(&literal, &literal.replacen("\\xef", "\\xee", 1), 1);
    assert_ne!(drifted_text, text, "反例没有真正改动脚本");
    let drifted = case_block(&drifted_text, name).ok_or("反例脚本缺少用例")?;
    assert!(
        !block_uses_literal(drifted, &literal),
        "样本字节被改动后必须被判为不一致"
    );
    Ok(())
}

/// 取出脚本里 `BYTES_CASES` 中某个用例的字典块（从 `"name": {` 到其缩进结束）。
///
/// 锚定在 `BYTES_CASES = {` 之后查找，避免命中文件别处同名字典；块尾取第一
/// 个同缩进的 `},`，保证读到的是该用例自身的结束。
fn case_block<'a>(text: &'a str, name: &'a str) -> Option<&'a str> {
    let anchor = text.find("BYTES_CASES = {")?;
    let rest = &text[anchor..];
    let marker = format!("\"{name}\": {{");
    let start = rest.find(&marker)?;
    let block = &rest[start..];
    let end = block.find("\n    },")?;
    Some(&block[..end])
}

/// 比对：块内是否使用了给定的字节字面量。
fn block_uses_literal(block: &str, literal: &str) -> bool {
    block.contains(literal)
}

/// 把字节渲染成 Python 字节串字面量；转义方式与脚本里手写的形式一致。
fn python_bytes_literal(bytes: &[u8]) -> String {
    let mut out = String::from("b\"");
    for byte in bytes {
        match byte {
            b'\\' => out.push_str("\\\\"),
            b'"' => out.push_str("\\\""),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(*byte as char),
            other => out.push_str(&format!("\\x{other:02x}")),
        }
    }
    out.push('"');
    out
}

fn repo_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative)
}
