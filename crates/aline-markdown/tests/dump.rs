//! 组 6.3 / 9.3：E3 同夹具可审查 dump —— 绘制文字、逐片段来源、精确性与源码身份。
//!
//! 期望来源：`docs/validation/m0-engine-foundation/remediation/R09/run-1/basic.dump.expected.txt`
//! 与本文下方的常量表，均由 `tests/fixtures/m0/basic.md` 的原始字节、pulldown-cmark
//! 的 offset 事件区间与 M0 映射规则**手写推导**得到（推导过程见同目录 `derivation.md`），
//! 不是把实现输出复制成期望。手写推导与实现不符时先判缺陷归属，不得改实现掩盖。

use std::error::Error;
use std::path::PathBuf;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{MappingPrecision, ParseBudget, ParseConfig, Parsed, parse};

const BUDGET: usize = 1024 * 1024;

/// 手写推导的完整期望 dump（逐行、逐片段）。
const EXPECTED_DUMP: &str = include_str!(
    "../../../docs/validation/m0-engine-foundation/remediation/R09/run-1/basic.dump.expected.txt"
);

/// 手写推导的绘制文字：源标记隐藏后剩下的可见文字，表格单元格之间以 tab 分隔。
const EXPECTED_DRAWN: &str = "标题段落 粗体 与 斜体、code、链接、删除。引用项目fn main() { let x = \"字面 **不解析**\"; }\n列\t值\t甲\t1脚注引用[^1]。脚注内容$$\nE = mc^2\n$$:smile: 末尾无换行";

/// 手写推导的源码原始字节 FNV-1a 64（推导步骤见 `derivation.md` 第 5 节）。
const EXPECTED_FNV1A64: u64 = 0xacb9_6228_ce0d_7842;

/// 手写推导的逐片段表：`(drawn_start, drawn_end, source_start, source_end, precision)`。
///
/// 按绘制起点升序排列，覆盖 `projection().segments()` 全部 49 个片段。
/// 注意 `source_start..source_end` 不随绘制起点单调递增：表格合成 tab 的零宽源锚点
/// 落在随后 `Hidden` 片段的源码区间内部（`209..209` 位于 `207..210` 内），这是
/// 「按绘制顺序」而非「按源码顺序」的必然结果，不是推导错误。
const EXPECTED_SEGMENTS: &[(usize, usize, usize, usize, &str)] = &[
    (0, 0, 0, 23, "Hidden"),
    (0, 6, 23, 29, "Exact"),
    (6, 6, 29, 31, "Hidden"),
    (6, 13, 31, 38, "Exact"),
    (13, 13, 38, 40, "Hidden"),
    (13, 19, 40, 46, "Exact"),
    (19, 19, 46, 48, "Hidden"),
    (19, 24, 48, 53, "Exact"),
    (24, 24, 53, 54, "Hidden"),
    (24, 30, 54, 60, "Exact"),
    (30, 30, 60, 61, "Hidden"),
    (30, 33, 61, 64, "Exact"),
    (33, 37, 64, 70, "Aligned"),
    (37, 40, 70, 73, "Exact"),
    (40, 40, 73, 74, "Hidden"),
    (40, 46, 74, 80, "Exact"),
    (46, 46, 80, 102, "Hidden"),
    (46, 49, 102, 105, "Exact"),
    (49, 49, 105, 107, "Hidden"),
    (49, 55, 107, 113, "Exact"),
    (55, 55, 113, 115, "Hidden"),
    (55, 58, 115, 118, "Exact"),
    (58, 58, 118, 122, "Hidden"),
    (58, 64, 122, 128, "Exact"),
    (64, 64, 128, 135, "Hidden"),
    (64, 70, 135, 141, "Exact"),
    (70, 70, 141, 151, "Hidden"),
    (70, 116, 151, 197, "Exact"),
    (116, 116, 197, 204, "Hidden"),
    (116, 119, 204, 207, "Exact"),
    (119, 120, 209, 209, "Synthetic"),
    (120, 120, 207, 210, "Hidden"),
    (120, 123, 210, 213, "Exact"),
    (123, 124, 227, 227, "Synthetic"),
    (124, 124, 213, 228, "Hidden"),
    (124, 127, 228, 231, "Exact"),
    (127, 128, 233, 233, "Synthetic"),
    (128, 128, 231, 234, "Hidden"),
    (128, 129, 234, 235, "Exact"),
    (129, 129, 235, 239, "Hidden"),
    (129, 141, 239, 251, "Exact"),
    (141, 145, 251, 255, "Exact"),
    (145, 148, 255, 258, "Exact"),
    (148, 148, 258, 266, "Hidden"),
    (148, 160, 266, 278, "Exact"),
    (160, 160, 278, 280, "Hidden"),
    (160, 174, 280, 294, "Exact"),
    (174, 174, 294, 296, "Hidden"),
    (174, 197, 296, 319, "Exact"),
];

/// 手写推导的块/间隙区间（与 `tests/fixtures/m0/basic.blocks.tsv` 同一来源）。
const EXPECTED_BLOCKS: &[(usize, usize, &str)] = &[
    (0, 19, "FrontMatter"),
    (21, 29, "Heading"),
    (31, 118, "Paragraph"),
    (120, 141, "BlockQuote"),
    (143, 200, "CodeBlock"),
    (202, 237, "Table"),
    (239, 258, "Paragraph"),
    (260, 278, "FootnoteDefinition"),
    (280, 294, "Math"),
    (296, 319, "Paragraph"),
];

/// 手写推导的保护区间。
const EXPECTED_PROTECTED: &[(usize, usize, &str)] = &[
    (0, 19, "FrontMatter"),
    (251, 255, "FootnoteReference"),
    (260, 278, "FootnoteDefinition"),
];

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("m0")
        .join("basic.md")
}

fn read_fixture() -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(fixture_path())?)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

/// 独立实现的 FNV-1a 64，与 `Parsed` 内的摘要实现无关，用于手算同一份源码的身份。
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[test]
fn e3_basic_fixture_dump_matches_independent_expectation() -> Result<(), Box<dyn Error>> {
    let text = read_fixture()?;
    assert_eq!(text.len(), 319, "夹具字节长度与推导前提不符");
    let parsed = parse_text(&text)?;
    let dump = parsed.dump_tree();
    let actual: Vec<&str> = dump.lines().collect();
    let expected: Vec<&str> = EXPECTED_DUMP.lines().collect();
    assert_eq!(
        actual.len(),
        expected.len(),
        "dump 行数 {} 与手写推导 {} 不符",
        actual.len(),
        expected.len()
    );
    for (index, (actual_line, expected_line)) in actual.iter().zip(expected.iter()).enumerate() {
        assert_eq!(
            actual_line,
            expected_line,
            "dump 第 {} 行与独立推导不符",
            index + 1
        );
    }
    Ok(())
}

#[test]
fn e3_drawn_text_matches_visible_source_and_length() -> Result<(), Box<dyn Error>> {
    let text = read_fixture()?;
    let parsed = parse_text(&text)?;
    let drawn = parsed.projection().drawn_text();
    assert_eq!(drawn, EXPECTED_DRAWN, "绘制文字与手写推导不符");
    assert_eq!(drawn.len(), 197, "绘制字节长度与手写推导不符");
    // dump 的 `drawn` 行必须携带完整文字本体，而不是只有长度。
    let dump = parsed.dump_tree();
    let line = dump
        .lines()
        .find(|line| line.starts_with("drawn 197 text "))
        .ok_or("dump 缺少带文字的 drawn 行")?;
    assert_eq!(format!("drawn 197 text {EXPECTED_DRAWN:?}"), line);
    // 隐藏标记必须真的不在绘制文字里：`~~` 与目标 URL 均只在源码侧。
    assert!(!drawn.contains("~~"), "删除线标记未隐藏");
    assert!(!drawn.contains("https://example.com"), "链接目标未隐藏");
    // `**` 只允许剩下代码块里那一对字面源码（两处），正文加粗标记不得出现。
    assert_eq!(drawn.matches("**").count(), 2, "加粗标记未隐藏");
    assert!(!drawn.contains("*斜体*"), "斜体标记未隐藏");
    // 代码块正文里的 `**` 属于字面源码，必须原样保留（不能当作标记吞掉）。
    assert!(drawn.contains("字面 **不解析**"), "代码块字面文本被误隐藏");
    Ok(())
}

#[test]
fn e3_segments_match_independent_table_one_by_one() -> Result<(), Box<dyn Error>> {
    let text = read_fixture()?;
    let parsed = parse_text(&text)?;
    let segments = parsed.projection().segments();
    assert_eq!(
        segments.len(),
        EXPECTED_SEGMENTS.len(),
        "片段总数与手写推导不符"
    );
    for (index, (segment, expected)) in segments.iter().zip(EXPECTED_SEGMENTS.iter()).enumerate() {
        let drawn = segment.drawn_range();
        let source = segment.source_range().byte_range();
        assert_eq!(
            (drawn.start, drawn.end, source.start, source.end),
            (expected.0, expected.1, expected.2, expected.3),
            "第 {index} 个片段的区间与手写推导不符"
        );
        assert_eq!(
            segment.precision().as_str(),
            expected.4,
            "第 {index} 个片段的精确性与手写推导不符"
        );
        assert_eq!(
            MappingPrecision::Exact.as_str(),
            "Exact",
            "精确性稳定名称必须为英文原名"
        );
    }
    // 片段按绘制起点升序、覆盖整个绘制文本且互不重叠。
    let mut cursor = 0;
    for (index, segment) in segments.iter().enumerate() {
        let drawn = segment.drawn_range();
        assert_eq!(drawn.start, cursor, "第 {index} 个片段起点未接续前一片段");
        cursor = drawn.end;
    }
    assert_eq!(cursor, EXPECTED_DRAWN.len(), "片段未覆盖全部绘制文本");
    Ok(())
}

#[test]
fn e3_hidden_aligned_and_synthetic_segments_are_present() -> Result<(), Box<dyn Error>> {
    let text = read_fixture()?;
    let parsed = parse_text(&text)?;
    let segments = parsed.projection().segments();
    let count = |precision: MappingPrecision| {
        segments
            .iter()
            .filter(|segment| segment.precision() == precision)
            .count()
    };
    assert_eq!(
        count(MappingPrecision::Hidden),
        21,
        "Hidden 片段数与推导不符"
    );
    assert_eq!(count(MappingPrecision::Exact), 24, "Exact 片段数与推导不符");
    assert_eq!(
        count(MappingPrecision::Aligned),
        1,
        "Aligned 片段数（仅 `code` 一处）与推导不符"
    );
    assert_eq!(
        count(MappingPrecision::Synthetic),
        3,
        "Synthetic 片段数（表格单元格间隔）与推导不符"
    );
    // 至少一个 Hidden 与一个 Synthetic 被逐片段断言，不只靠计数。
    let hidden = segments
        .iter()
        .find(|segment| segment.precision() == MappingPrecision::Hidden)
        .ok_or("缺少 Hidden 片段")?;
    assert_eq!(hidden.drawn_range(), 0..0);
    assert_eq!(hidden.source_range().byte_range(), 0..23);
    let synthetic = segments
        .iter()
        .find(|segment| segment.precision() == MappingPrecision::Synthetic)
        .ok_or("缺少 Synthetic 片段")?;
    assert_eq!(synthetic.drawn_range(), 119..120);
    assert_eq!(synthetic.source_range().byte_range(), 209..209);
    // 合成 tab 在绘制侧占 1 字节、源码侧为零宽锚点，不得伪装成精确对应。
    let aligned = segments
        .iter()
        .find(|segment| segment.precision() == MappingPrecision::Aligned)
        .ok_or("缺少 Aligned 片段")?;
    assert_eq!(aligned.drawn_range(), 33..37);
    assert_eq!(aligned.source_range().byte_range(), 64..70);
    Ok(())
}

#[test]
fn e3_blocks_and_gaps_partition_source_without_overlap() -> Result<(), Box<dyn Error>> {
    let text = read_fixture()?;
    let parsed = parse_text(&text)?;
    let source_len = text.len();

    let mut entries: Vec<(usize, usize)> = Vec::new();
    for block in parsed.blocks() {
        let range = block.range().byte_range();
        entries.push((range.start, range.end));
    }
    let block_count = entries.len();
    assert_eq!(block_count, EXPECTED_BLOCKS.len(), "块数与推导不符");
    for (block, expected) in parsed.blocks().iter().zip(EXPECTED_BLOCKS.iter()) {
        let range = block.range().byte_range();
        assert_eq!(
            (range.start, range.end, block.kind().as_str()),
            (expected.0, expected.1, expected.2),
            "块区间/种类与推导不符"
        );
    }

    let gap_count = parsed.gaps().len();
    assert_eq!(gap_count, EXPECTED_BLOCKS.len() - 1, "间隙数与推导不符");
    for gap in parsed.gaps() {
        let range = gap.range().byte_range();
        entries.push((range.start, range.end));
    }

    // 块与间隙严格相接、无空洞、无重叠，并覆盖全部源码。
    entries.sort_by_key(|(start, _)| *start);
    let mut cursor = 0;
    for (index, (start, end)) in entries.iter().enumerate() {
        assert_eq!(*start, cursor, "第 {index} 个块/间隙起点未接续");
        assert!(start < end, "第 {index} 个块/间隙为空区间");
        cursor = *end;
    }
    assert_eq!(cursor, source_len, "块与间隙未覆盖全部源码");
    assert!(parsed.diagnostics().is_empty(), "basic.md 不应产生诊断");
    Ok(())
}

#[test]
fn e3_protected_ranges_match_independent_expectation() -> Result<(), Box<dyn Error>> {
    let text = read_fixture()?;
    let parsed = parse_text(&text)?;
    let protected = parsed.protected_ranges();
    assert_eq!(
        protected.len(),
        EXPECTED_PROTECTED.len(),
        "保护区间数与推导不符"
    );
    for (entry, expected) in protected.iter().zip(EXPECTED_PROTECTED.iter()) {
        let range = entry.range().byte_range();
        assert_eq!(
            (range.start, range.end, entry.kind().as_str()),
            (expected.0, expected.1, expected.2),
            "保护区间/种类与推导不符"
        );
    }
    // 脚注引用只锁住 `[^1]`，不得把相邻正文 `脚注引用` 或 `。` 一并保护。
    let reference = protected
        .iter()
        .find(|entry| entry.kind().as_str() == "FootnoteReference")
        .ok_or("缺少 FootnoteReference 保护")?;
    assert_eq!(&text[reference.range().byte_range()], "[^1]");
    Ok(())
}

#[test]
fn e3_source_identity_is_byte_derived_not_snapshot_identity() -> Result<(), Box<dyn Error>> {
    let text = read_fixture()?;
    let parsed = parse_text(&text)?;
    let expected = fnv1a64(text.as_bytes());
    assert_eq!(expected, EXPECTED_FNV1A64, "独立 FNV-1a 实现与手写常量不符");
    let dump = parsed.dump_tree();
    let line = dump
        .lines()
        .find(|line| line.starts_with("source 319 identity "))
        .ok_or("dump 缺少带身份的 source 行")?;
    assert_eq!(
        format!("source 319 identity fnv1a64 0x{expected:016x} revision 0"),
        line
    );

    // 同一字节内容的两份不同快照：身份相同（摘要来自字节，不是 `SnapshotId`）。
    let other = parse_text(&text)?;
    assert_ne!(
        parsed.snapshot().id(),
        other.snapshot().id(),
        "两个快照应持有不同身份"
    );
    assert_eq!(
        parsed.dump_tree(),
        other.dump_tree(),
        "dump 不得携带进程相关身份"
    );

    // 长度与块范围相同但字节不同：摘要必须改变，否则 E3 仍看不出「来源错误」。
    let mutated = text.replacen("E = mc^2", "E = mc^3", 1);
    assert_eq!(mutated.len(), text.len(), "变异必须保持长度");
    let mutated_dump = parse_text(&mutated)?.dump_tree();
    assert_ne!(fnv1a64(mutated.as_bytes()), expected, "变异后摘要必须改变");
    assert!(
        !mutated_dump.contains(&format!("identity fnv1a64 0x{expected:016x}")),
        "dump 未反映字节变化"
    );
    Ok(())
}
