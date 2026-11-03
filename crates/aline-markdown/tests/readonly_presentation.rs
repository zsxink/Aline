//! M1 基础夹具消费测试：逐组核对独立展示文本与源码分区预期。

use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    AlertKind, CancelCheck, ParseBudget, ParseConfig, ParseError, Parsed, PresentationKind,
    PresentationNode, TableAlignment, parse, parse_with_cancel, reuse,
};

const SOURCE_BUDGET: usize = 8 * 1024 * 1024;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/m1")
        .join(name)
}

fn parse_fixture(name: &str) -> Result<(String, Parsed), Box<dyn Error>> {
    let source = std::fs::read_to_string(fixture(name))?;
    let snapshot = TextSnapshot::new(source.clone(), TextBudget::new(SOURCE_BUDGET))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    Ok((source, parsed))
}

fn parse_text(source: &str) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(SOURCE_BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

fn descendants(nodes: &[PresentationNode]) -> Vec<&PresentationNode> {
    let mut output = Vec::new();
    for node in nodes {
        output.push(node);
        output.extend(descendants(node.children()));
    }
    output
}

#[test]
fn nested_semantics_keep_alert_list_order_and_inline_styles() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text(
        "> [!TIP]\n> - first\n>   - **bold**\n\n3. *item* and ~~old~~ `code` [link](https://example.test)\n",
    )?;
    let roots = parsed.presentation();
    assert_eq!(roots.len(), 2);
    assert_eq!(
        roots[0].kind(),
        &PresentationKind::BlockQuote {
            alert: Some(AlertKind::Tip)
        }
    );
    assert_eq!(roots[1].kind(), &PresentationKind::List { start: Some(3) });
    let nodes = descendants(roots);
    assert_eq!(
        nodes
            .iter()
            .filter(|node| matches!(node.kind(), PresentationKind::List { .. }))
            .count(),
        3
    );
    for kind in [
        PresentationKind::Strong,
        PresentationKind::Emphasis,
        PresentationKind::Strikethrough,
        PresentationKind::Code,
        PresentationKind::Link,
    ] {
        assert!(
            nodes.iter().any(|node| node.kind() == &kind),
            "missing {kind:?}"
        );
    }
    for node in nodes {
        assert_eq!(node.source_range().snapshot_id(), parsed.snapshot().id());
        assert_eq!(node.drawn_range().projection_id(), parsed.projection().id());
        let source = node.source_range().byte_range();
        let drawn = node.drawn_range().byte_range();
        for child in node.children() {
            let child_source = child.source_range().byte_range();
            let child_drawn = child.drawn_range().byte_range();
            assert!(source.start <= child_source.start && child_source.end <= source.end);
            assert!(drawn.start <= child_drawn.start && child_drawn.end <= drawn.end);
        }
    }
    Ok(())
}

#[test]
fn table_structure_and_alignment_do_not_depend_on_separator_text() -> Result<(), Box<dyn Error>> {
    let parsed =
        parse_text("| left | center | right |\n| :--- | :---: | ---: |\n| a\\|b | **c** | d |\n")?;
    let table = parsed.presentation().first().ok_or("missing table")?;
    assert_eq!(
        table.kind(),
        &PresentationKind::Table {
            alignments: vec![
                TableAlignment::Left,
                TableAlignment::Center,
                TableAlignment::Right
            ]
        }
    );
    assert_eq!(table.children().len(), 2);
    assert_eq!(table.children()[0].kind(), &PresentationKind::TableHead);
    assert_eq!(table.children()[1].kind(), &PresentationKind::TableRow);
    for row in table.children() {
        assert_eq!(row.children().len(), 3);
        assert!(
            row.children()
                .iter()
                .all(|cell| cell.kind() == &PresentationKind::TableCell)
        );
    }
    Ok(())
}

#[test]
fn code_lines_use_body_text_and_do_not_count_terminal_newline_twice() -> Result<(), Box<dyn Error>>
{
    let parsed = parse_text("```rust\none\n\nthree\n```\n")?;
    let code = parsed.presentation().first().ok_or("missing code")?;
    assert_eq!(
        code.kind(),
        &PresentationKind::CodeBlock {
            info: "rust".into(),
            fenced: true
        }
    );
    let lines = code.code_lines();
    assert_eq!(lines.len(), 3);
    for (line, expected) in lines.iter().zip(["one", "", "three"]) {
        assert_eq!(
            parsed
                .projection()
                .drawn_text()
                .get(line.drawn_range().byte_range()),
            Some(expected)
        );
        assert!(line.has_line_break());
    }
    assert_eq!(
        lines.iter().map(|line| line.number()).collect::<Vec<_>>(),
        vec![1, 2, 3]
    );
    let empty = parse_text("```\n```\n")?;
    let empty_code = empty.presentation().first().ok_or("missing empty code")?;
    assert_eq!(empty_code.code_lines().len(), 1);
    assert!(!empty_code.code_lines()[0].has_line_break());
    Ok(())
}

#[test]
fn all_five_alert_types_have_distinct_semantics() -> Result<(), Box<dyn Error>> {
    for (label, expected) in [
        ("NOTE", AlertKind::Note),
        ("TIP", AlertKind::Tip),
        ("IMPORTANT", AlertKind::Important),
        ("WARNING", AlertKind::Warning),
        ("CAUTION", AlertKind::Caution),
    ] {
        let parsed = parse_text(&format!("> [!{label}]\n> body\n"))?;
        assert_eq!(
            parsed.presentation()[0].kind(),
            &PresentationKind::BlockQuote {
                alert: Some(expected)
            }
        );
    }
    Ok(())
}

#[test]
fn code_line_budget_returns_only_complete_raw_presentation() -> Result<(), Box<dyn Error>> {
    let source = format!("```\n{}```\n", "line\n".repeat(40));
    let unrestricted = parse_text(&source)?;
    assert_eq!(descendants(unrestricted.presentation()).len(), 2);
    assert_eq!(unrestricted.presentation()[0].code_lines().len(), 40);
    let snapshot = TextSnapshot::new(source.clone(), TextBudget::new(SOURCE_BUDGET))?;
    let parsed = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_nodes(8),
    )?;
    assert_eq!(parsed.presentation().len(), 1);
    assert_eq!(parsed.presentation()[0].kind(), &PresentationKind::Raw);
    assert!(parsed.presentation()[0].children().is_empty());
    assert!(parsed.presentation()[0].code_lines().is_empty());
    assert_eq!(parsed.projection().drawn_text(), source);
    assert_eq!(parsed.stats().budget_hits(), 1);
    Ok(())
}

struct CancelAfter {
    checks: AtomicUsize,
    limit: usize,
}

impl CancelCheck for CancelAfter {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::Relaxed) >= self.limit
    }
}

#[test]
fn code_line_binding_observes_cancel_without_publishing_partial_tree() -> Result<(), Box<dyn Error>>
{
    let source = format!("```\n{}```\n", "line\n".repeat(200));
    let snapshot = TextSnapshot::new(source, TextBudget::new(SOURCE_BUDGET))?;
    let cancel = CancelAfter {
        checks: AtomicUsize::new(0),
        limit: 100,
    };
    assert!(matches!(
        parse_with_cancel(&snapshot, ParseConfig::m0(), ParseBudget::m0(), &cancel),
        Err(ParseError::Cancelled)
    ));
    Ok(())
}

#[test]
fn structural_limit_matrix_publishes_only_complete_source_fallback() -> Result<(), Box<dyn Error>> {
    let table = format!(
        "| a | b | c |\n| --- | --- | --- |\n{}",
        "| x | y | z |\n".repeat(200)
    );
    let cases = [
        (
            "micro-events",
            "*x* ".repeat(200),
            ParseBudget::m0().with_max_nodes(16),
        ),
        (
            "giant-paragraph",
            "x".repeat(128 * 1024),
            ParseBudget::m0().with_max_segments(0),
        ),
        ("giant-table", table, ParseBudget::m0().with_max_nodes(32)),
        (
            "deep-containers",
            format!("{}body\n", "> ".repeat(40)),
            ParseBudget::m0().with_max_nesting(8),
        ),
    ];
    for (label, source, budget) in cases {
        let snapshot = TextSnapshot::new(source.clone(), TextBudget::new(SOURCE_BUDGET))?;
        let parsed = parse(&snapshot, ParseConfig::m0(), budget)?;
        assert_eq!(parsed.presentation().len(), 1, "{label}");
        let raw = &parsed.presentation()[0];
        assert_eq!(raw.kind(), &PresentationKind::Raw, "{label}");
        assert_eq!(raw.source_range().byte_range(), 0..source.len(), "{label}");
        assert_eq!(raw.drawn_range().byte_range(), 0..source.len(), "{label}");
        assert!(raw.children().is_empty(), "{label}");
        assert!(raw.code_lines().is_empty(), "{label}");
        assert!(parsed.navigation().is_empty(), "{label}");
        assert_eq!(parsed.projection().drawn_text(), source, "{label}");
        assert_eq!(parsed.stats().budget_hits(), 1, "{label}");
    }
    Ok(())
}

#[test]
fn reused_block_structure_rebases_sources_and_keeps_styles_and_text() -> Result<(), Box<dyn Error>>
{
    let old = parse_text("old\n\n**same**")?;
    let new = parse_text("new longer\n\n**same**")?;
    let outcome = reuse(&old, &new);
    let pair = outcome
        .reused()
        .iter()
        .find(|pair| pair.old_index() == 1)
        .ok_or("missing real reused suffix")?;
    let old_root = old.presentation().last().ok_or("missing old tree")?;
    let new_root = new.presentation().last().ok_or("missing new tree")?;
    let old_nodes = descendants(std::slice::from_ref(old_root));
    let new_nodes = descendants(std::slice::from_ref(new_root));
    assert_eq!(old_nodes.len(), 3);
    assert_eq!(new_nodes.len(), old_nodes.len());
    for (old_node, new_node) in old_nodes.iter().zip(&new_nodes) {
        assert_eq!(old_node.kind(), new_node.kind());
        assert_eq!(old_node.children().len(), new_node.children().len());
        assert_eq!(
            old.projection()
                .drawn_text()
                .get(old_node.drawn_range().byte_range()),
            new.projection()
                .drawn_text()
                .get(new_node.drawn_range().byte_range())
        );
        let old_range = old_node.source_range().byte_range();
        let translated_start =
            pair.translate(new.snapshot(), &old.snapshot().src_offset(old_range.start)?)?;
        let translated_end =
            pair.translate(new.snapshot(), &old.snapshot().src_offset(old_range.end)?)?;
        assert_eq!(
            new_node.source_range().byte_range(),
            translated_start.byte_offset()..translated_end.byte_offset()
        );
        assert!(new.validate_range(old_node.source_range()).is_err());
        assert_ne!(
            old_node.drawn_range().projection_id(),
            new_node.drawn_range().projection_id()
        );
    }
    Ok(())
}

fn assert_partition_covers_source(parsed: &Parsed, source: &str) {
    let mut entries = parsed
        .blocks()
        .iter()
        .map(|block| block.range().byte_range())
        .chain(parsed.gaps().iter().map(|gap| gap.range().byte_range()))
        .collect::<Vec<_>>();
    entries.sort_by_key(|range| (range.start, range.end));

    let mut source_offset = 0usize;
    for range in entries {
        assert_eq!(range.start, source_offset, "source partition has a gap");
        assert!(
            range.end > range.start,
            "source partition has an empty entry"
        );
        assert!(
            source.get(range.clone()).is_some(),
            "range splits a UTF-8 scalar"
        );
        source_offset = range.end;
    }
    assert_eq!(
        source_offset,
        source.len(),
        "source partition is incomplete"
    );
    assert_eq!(parsed.snapshot().len_bytes(), source.len());
}

fn source_fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn source_expectations() -> Result<HashMap<String, (usize, u64)>, Box<dyn Error>> {
    let manifest = std::fs::read_to_string(fixture("expected/source.tsv"))?;
    let mut expected = HashMap::new();
    for line in manifest
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let columns = line.split('\t').collect::<Vec<_>>();
        if columns.len() != 3 {
            return Err("source expectation row must have three columns".into());
        }
        expected.insert(
            columns[0].to_owned(),
            (columns[1].parse()?, u64::from_str_radix(columns[2], 16)?),
        );
    }
    Ok(expected)
}

#[test]
fn m1_fixture_groups_match_independent_drawn_and_source_expectations() -> Result<(), Box<dyn Error>>
{
    let cases = [
        "basic-inline",
        "nested-containers",
        "code-tables",
        "literal-formats",
        "images-html",
    ];
    let source_goldens = source_expectations()?;
    for name in cases {
        let source_name = format!("{name}.md");
        let (source, parsed) = parse_fixture(&source_name)?;
        assert_partition_covers_source(&parsed, &source);
        let (expected_len, expected_hash) = source_goldens
            .get(&source_name)
            .ok_or("missing independent source expectation")?;
        assert_eq!(source.len(), *expected_len, "fixture {name} source length");
        assert_eq!(
            source_fingerprint(source.as_bytes()),
            *expected_hash,
            "fixture {name} source bytes"
        );
        let expected = std::fs::read_to_string(fixture(&format!("expected/{name}.drawn")))?;
        assert_eq!(parsed.projection().drawn_text(), expected, "fixture {name}");
    }
    Ok(())
}

#[test]
fn empty_and_line_ending_fixture_preserve_their_source_bytes() -> Result<(), Box<dyn Error>> {
    let source_goldens = source_expectations()?;
    for name in ["empty.md", "line-endings.md"] {
        let (source, parsed) = parse_fixture(name)?;
        assert_partition_covers_source(&parsed, &source);
        let (expected_len, expected_hash) = source_goldens
            .get(name)
            .ok_or("missing independent source expectation")?;
        assert_eq!(source.len(), *expected_len);
        assert_eq!(source_fingerprint(source.as_bytes()), *expected_hash);
        assert_eq!(
            parsed.snapshot().slice(
                &parsed.snapshot().src_range(0..source.len())?,
                TextBudget::new(SOURCE_BUDGET),
            )?,
            source
        );
    }
    Ok(())
}
