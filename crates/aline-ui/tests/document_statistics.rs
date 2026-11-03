//! 文档统计的独立无窗口预期；纯函数，可直接以 `#[path]` 引入。

#[path = "../src/document/statistics.rs"]
mod statistics;

use statistics::DocumentStatistics;

fn compute(source: &str) -> (usize, usize) {
    let stats = DocumentStatistics::compute(source);
    (stats.characters(), stats.lines())
}

#[test]
fn empty_and_whitespace_only_sources() {
    assert_eq!(compute(""), (0, 1));
    assert_eq!(compute(" \t\n\r\n"), (0, 3));
}

#[test]
fn counts_non_whitespace_scalars_and_lf_lines() {
    assert_eq!(compute("你好，world!"), (9, 1));
    assert_eq!(compute("a\r\nb"), (2, 2));
    assert_eq!(compute("a\rb"), (2, 1));
}

#[test]
fn trailing_newline_adds_an_empty_line() {
    assert_eq!(compute("a\n"), (1, 2));
    assert_eq!(compute("a\n\n"), (1, 3));
}

#[test]
fn unicode_whitespace_boundaries_are_explicit() {
    // U+0085 NEL 是 Unicode White_Space，Rust 排除它；JavaScript /\s/ 不排除。
    assert_eq!(compute("\u{0085}a"), (1, 1));
    // U+FEFF ZWNBSP 不是 Unicode White_Space，Rust 计入；JavaScript /\s/ 排除。
    assert_eq!(compute("\u{FEFF}a"), (2, 1));
}

#[test]
fn scalar_count_includes_zwj_sequences_and_combining_marks() {
    assert_eq!(compute("👨\u{200D}👩\u{200D}👧"), (5, 1));
    assert_eq!(compute("e\u{301}"), (2, 1));
}
