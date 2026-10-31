//! 组 6.1/6.3：夹具、显式选项、块/Gap 手工金样与 dump 入口。

use std::collections::HashMap;
use std::error::Error;
use std::path::PathBuf;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{BlockKind, ParseBudget, ParseConfig, Parsed, parse};

const BUDGET: usize = 1024 * 1024;

fn fixture(name: &str) -> Result<PathBuf, Box<dyn Error>> {
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("m0")
        .join(name))
}

fn read_fixture(name: &str) -> Result<String, Box<dyn Error>> {
    Ok(std::fs::read_to_string(fixture(name)?)?)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

fn range_of(value: &str) -> Result<(usize, usize), Box<dyn Error>> {
    let (start, end) = value.split_once("..").ok_or("range without ..")?;
    Ok((start.parse::<usize>()?, end.parse::<usize>()?))
}

fn actual_entries(parsed: &Parsed) -> Vec<String> {
    let mut entries = Vec::new();
    for block in parsed.blocks() {
        let range = block.range().byte_range();
        entries.push((
            range.start,
            format!(
                "block {}..{} {}",
                range.start,
                range.end,
                block.kind().as_str()
            ),
        ));
    }
    for gap in parsed.gaps() {
        let range = gap.range().byte_range();
        entries.push((range.start, format!("gap {}..{}", range.start, range.end)));
    }
    entries.sort_by_key(|(start, _)| *start);
    entries.into_iter().map(|(_, entry)| entry).collect()
}

#[test]
fn fixture_blocks_and_gaps_match_handwritten_goldens() -> Result<(), Box<dyn Error>> {
    let spec = read_fixture("spec.blocks.tsv")?;
    let mut expected: HashMap<String, Vec<String>> = HashMap::new();
    for line in spec.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() != 4 {
            return Err("spec line does not have four columns".into());
        }
        let (start, end) = range_of(columns[2])?;
        let entry = if columns[1] == "block" {
            format!("block {start}..{end} {}", columns[3])
        } else if columns[1] == "gap" {
            format!("gap {start}..{end}")
        } else {
            return Err("spec entry is neither block nor gap".into());
        };
        expected
            .entry(columns[0].to_string())
            .or_default()
            .push(entry);
    }
    for (name, expected_entries) in expected {
        let text = read_fixture(&name)?;
        let parsed = parse_text(&text)?;
        assert_eq!(
            actual_entries(&parsed),
            expected_entries,
            "fixture {name} block/gap mismatch"
        );
    }
    Ok(())
}

#[test]
fn basic_blocks_and_gaps_match_handwritten_goldens() -> Result<(), Box<dyn Error>> {
    let golden = read_fixture("basic.blocks.tsv")?;
    let mut expected = Vec::new();
    for line in golden.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.len() != 3 {
            return Err("basic golden line does not have three columns".into());
        }
        let (start, end) = range_of(columns[1])?;
        let entry = if columns[0] == "block" {
            format!("block {start}..{end} {}", columns[2])
        } else if columns[0] == "gap" {
            format!("gap {start}..{end}")
        } else {
            return Err("basic golden entry is neither block nor gap".into());
        };
        expected.push(entry);
    }
    let text = read_fixture("basic.md")?;
    let parsed = parse_text(&text)?;
    assert_eq!(actual_entries(&parsed), expected);
    Ok(())
}

#[test]
fn literal_code_block_avoids_front_matter_toc_footnote_and_html() -> Result<(), Box<dyn Error>> {
    let text = read_fixture("literal.md")?;
    let parsed = parse_text(&text)?;
    assert_eq!(parsed.blocks().len(), 1);
    let block = parsed.blocks().first().ok_or("missing block")?;
    assert_eq!(block.kind(), BlockKind::CodeBlock);
    assert!(parsed.protected_ranges().is_empty());
    assert!(parsed.front_matter().is_none());
    Ok(())
}

#[test]
fn unclosed_front_matter_is_not_swallowed() -> Result<(), Box<dyn Error>> {
    let text = read_fixture("front_matter_unclosed.md")?;
    let parsed = parse_text(&text)?;
    assert!(parsed.front_matter().is_none());
    let kinds: Vec<BlockKind> = parsed.blocks().iter().map(|block| block.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::ThematicBreak,
            BlockKind::Paragraph,
            BlockKind::Heading
        ]
    );
    Ok(())
}

#[test]
fn front_matter_plus_and_yaml_are_paired() -> Result<(), Box<dyn Error>> {
    let plus = parse_text(&read_fixture("front_matter_plus.md")?)?;
    assert!(plus.front_matter().is_some());
    let basic = parse_text(&read_fixture("basic.md")?)?;
    assert!(basic.front_matter().is_some());
    Ok(())
}

#[test]
fn gfm_features_are_enabled_explicitly() -> Result<(), Box<dyn Error>> {
    let text = read_fixture("gfm.md")?;
    let parsed = parse_text(&text)?;
    let kinds: Vec<BlockKind> = parsed.blocks().iter().map(|block| block.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::Heading,
            BlockKind::BlockQuote,
            BlockKind::List,
            BlockKind::Paragraph,
            BlockKind::Table
        ]
    );
    assert_eq!(parsed.config().version(), 1);
    Ok(())
}

#[test]
fn dump_contains_config_blocks_gaps_source_protection_and_diagnostics() -> Result<(), Box<dyn Error>>
{
    let text = read_fixture("md11.md")?;
    let parsed = parse_text(&text)?;
    let dump = parsed.dump_tree();
    for expected in [
        "config 1",
        "source 100",
        "block 0..16 FrontMatter",
        "gap 16..18",
        "protected 0..16 FrontMatter",
        "protected 28..33 Toc",
        "protected 41..45 FootnoteReference",
        "protected 70..99 Html",
    ] {
        assert!(dump.contains(expected), "dump missing {expected}: {dump}");
    }
    Ok(())
}

#[test]
fn empty_document_has_no_blocks_or_gaps() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("")?;
    assert!(parsed.blocks().is_empty());
    assert!(parsed.gaps().is_empty());
    assert!(parsed.protected_ranges().is_empty());
    assert_eq!(parsed.projection().drawn_text(), "");
    Ok(())
}

fn assert_seamless(parsed: &Parsed, source: &str) -> Result<(), Box<dyn Error>> {
    let mut entries: Vec<(usize, usize, bool)> = Vec::new();
    for block in parsed.blocks() {
        let range = block.range().byte_range();
        entries.push((range.start, range.end, true));
    }
    for gap in parsed.gaps() {
        let range = gap.range().byte_range();
        entries.push((range.start, range.end, false));
    }
    entries.sort_by_key(|(start, _, _)| *start);
    let mut cursor = 0usize;
    for (start, end, is_block) in entries {
        assert_eq!(start, cursor, "partition is not seamless at byte {start}");
        assert!(end > start, "empty partition entry at byte {start}");
        let slice = source
            .get(start..end)
            .ok_or("partition range is not on a char boundary")?;
        if is_block {
            assert!(
                !slice.ends_with('\n') && !slice.ends_with('\r'),
                "block {start}..{end} ends with a separator newline"
            );
        } else {
            assert!(
                slice.bytes().all(|byte| byte == b'\n' || byte == b'\r'),
                "gap {start}..{end} contains a non-newline byte"
            );
        }
        cursor = end;
    }
    assert_eq!(
        cursor,
        source.len(),
        "partition does not cover the whole source"
    );
    Ok(())
}

#[test]
fn multiple_blank_lines_are_gaps_between_paragraphs() -> Result<(), Box<dyn Error>> {
    let source = "a\n\n\n\nb\n";
    let parsed = parse_text(source)?;
    assert_eq!(
        actual_entries(&parsed),
        vec![
            "block 0..1 Paragraph".to_string(),
            "gap 1..5".to_string(),
            "block 5..6 Paragraph".to_string(),
            "gap 6..7".to_string(),
        ]
    );
    assert_seamless(&parsed, source)?;
    Ok(())
}

#[test]
fn leading_and_trailing_blank_lines_are_gaps() -> Result<(), Box<dyn Error>> {
    let source = "\n\n\na\n\n\n";
    assert_eq!(source.len(), 7);
    let parsed = parse_text(source)?;
    assert_eq!(
        actual_entries(&parsed),
        vec![
            "gap 0..3".to_string(),
            "block 3..4 Paragraph".to_string(),
            "gap 4..7".to_string(),
        ]
    );
    assert_seamless(&parsed, source)?;
    Ok(())
}
