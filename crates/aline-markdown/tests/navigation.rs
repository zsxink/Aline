//! 组 6.7：只读导航索引的标题、目录、脚注、快照身份与预算边界。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{BufferError, TextBudget, TextSnapshot};
use aline_markdown::{
    BlockKind, DiagnosticKind, NavigationEntry, NavigationKind, ParseBudget, ParseConfig,
    ParseError, Parsed, ProtectionKind, parse,
};

const BUDGET: usize = 1024 * 1024;

fn parse_with_budget(text: &str, budget: ParseBudget) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), budget)?)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    parse_with_budget(text, ParseBudget::m0())
}

fn source_range(entry: &NavigationEntry) -> Range<usize> {
    entry.source().byte_range()
}

fn target_ranges(entry: &NavigationEntry) -> Vec<Range<usize>> {
    entry
        .targets()
        .iter()
        .map(|target| target.byte_range())
        .collect()
}

fn protected_ranges(parsed: &Parsed, kind: ProtectionKind) -> Vec<Range<usize>> {
    parsed
        .protected_ranges()
        .iter()
        .filter(|range| range.kind() == kind)
        .map(|range| range.range().byte_range())
        .collect()
}

fn total_targets(parsed: &Parsed) -> usize {
    parsed
        .navigation()
        .entries()
        .iter()
        .map(|entry| entry.targets().len())
        .sum()
}

#[test]
fn atx_headings_record_levels_and_exact_source_ranges() -> Result<(), Box<dyn Error>> {
    let mut text = String::new();
    let mut expected = Vec::new();
    for level in 1..=6u8 {
        let line = format!("{} h\n\n", "#".repeat(usize::from(level)));
        let start = text.len();
        let end = start + line.trim_end().len();
        text.push_str(&line);
        expected.push((start..end, Some(level)));
    }
    let parsed = parse_text(&text)?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), expected.len());
    for (entry, (range, level)) in entries.iter().zip(expected) {
        assert_eq!(entry.kind(), NavigationKind::Heading);
        assert_eq!(source_range(entry), range);
        assert_eq!(entry.heading_level(), level);
        assert!(entry.targets().is_empty());
    }
    Ok(())
}

#[test]
fn setext_headings_record_the_full_block_range() -> Result<(), Box<dyn Error>> {
    let text = "Setext One\n==========\n\nSetext Two\n----------\n";
    let parsed = parse_text(text)?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(source_range(&entries[0]), 0..21);
    assert_eq!(entries[0].heading_level(), Some(1));
    assert_eq!(source_range(&entries[1]), 23..44);
    assert_eq!(entries[1].heading_level(), Some(2));
    for entry in entries {
        assert!(entry.targets().is_empty());
    }
    Ok(())
}

#[test]
fn nested_headings_are_not_indexed() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("> # Nested\n\n- ## Also\n")?;
    assert!(parsed.navigation().is_empty());
    Ok(())
}

#[test]
fn toc_links_top_level_headings_in_source_order() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("# Alpha\n\n## Beta\n\n[toc]\n\nSetext\n======\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].kind(), NavigationKind::Heading);
    assert_eq!(source_range(&entries[0]), 0..7);
    assert_eq!(entries[0].heading_level(), Some(1));
    assert_eq!(entries[1].kind(), NavigationKind::Heading);
    assert_eq!(source_range(&entries[1]), 9..16);
    assert_eq!(entries[1].heading_level(), Some(2));
    assert_eq!(entries[2].kind(), NavigationKind::Toc);
    assert_eq!(source_range(&entries[2]), 18..23);
    assert_eq!(entries[2].heading_level(), None);
    assert_eq!(target_ranges(&entries[2]), vec![0..7, 9..16, 25..38]);
    assert_eq!(entries[3].kind(), NavigationKind::Heading);
    assert_eq!(source_range(&entries[3]), 25..38);
    assert_eq!(entries[3].heading_level(), Some(1));
    for entry in [&entries[0], &entries[1], &entries[3]] {
        assert!(entry.targets().is_empty());
    }
    let toc_sources = entries
        .iter()
        .filter(|entry| entry.kind() == NavigationKind::Toc)
        .map(source_range)
        .collect::<Vec<_>>();
    assert_eq!(toc_sources, vec![18..23]);
    assert_eq!(protected_ranges(&parsed, ProtectionKind::Toc), toc_sources);
    Ok(())
}

#[test]
fn toc_is_case_insensitive_with_outer_whitespace() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("  [TOC]  \n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind(), NavigationKind::Toc);
    assert_eq!(source_range(&entries[0]), 2..9);
    assert!(entries[0].targets().is_empty());
    assert_eq!(protected_ranges(&parsed, ProtectionKind::Toc), vec![2..9]);
    Ok(())
}

#[test]
fn multiple_tocs_each_link_the_same_headings() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("# A\n\n[toc]\n\n[toc]\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].kind(), NavigationKind::Heading);
    assert_eq!(source_range(&entries[0]), 0..3);
    assert_eq!(entries[1].kind(), NavigationKind::Toc);
    assert_eq!(source_range(&entries[1]), 5..10);
    assert_eq!(target_ranges(&entries[1]), vec![0..3]);
    assert_eq!(entries[2].kind(), NavigationKind::Toc);
    assert_eq!(source_range(&entries[2]), 12..17);
    assert_eq!(target_ranges(&entries[2]), vec![0..3]);
    assert_eq!(
        protected_ranges(&parsed, ProtectionKind::Toc),
        vec![5..10, 12..17]
    );
    Ok(())
}

#[test]
fn nested_toc_is_not_indexed_or_protected() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("> [toc]\n\n- [toc]\n")?;
    assert!(parsed.navigation().is_empty());
    assert!(protected_ranges(&parsed, ProtectionKind::Toc).is_empty());
    Ok(())
}

#[test]
fn toc_detection_matches_protection_for_non_paragraphs() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("# [toc]\n\n`[toc]`\n\n```\n[toc]\n```\n\n> [toc]\n\n<toc>\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind(), NavigationKind::Heading);
    assert_eq!(source_range(&entries[0]), 0..7);
    assert!(entries[0].targets().is_empty());
    assert!(protected_ranges(&parsed, ProtectionKind::Toc).is_empty());
    Ok(())
}

#[test]
fn toc_ignores_nested_headings() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("# Top\n\n> # Nested\n\n[toc]\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].kind(), NavigationKind::Heading);
    assert_eq!(source_range(&entries[0]), 0..5);
    assert_eq!(entries[1].kind(), NavigationKind::Toc);
    assert_eq!(entries[1].heading_level(), None);
    assert_eq!(target_ranges(&entries[1]), vec![0..5]);
    Ok(())
}

#[test]
fn footnote_reference_and_definition_link_both_directions() -> Result<(), Box<dyn Error>> {
    let text = "Alpha[^a] beta[^a].\n\n[^a]: Note A.\n\n[^b]: Note B.\n\nGamma[^c].\n";
    let parsed = parse_text(text)?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[0]), 5..9);
    assert_eq!(target_ranges(&entries[0]), vec![21..34]);
    assert_eq!(entries[0].heading_level(), None);
    assert_eq!(entries[1].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[1]), 14..18);
    assert_eq!(target_ranges(&entries[1]), vec![21..34]);
    assert_eq!(entries[2].kind(), NavigationKind::FootnoteDefinition);
    assert_eq!(source_range(&entries[2]), 21..34);
    assert_eq!(target_ranges(&entries[2]), vec![5..9, 14..18]);
    assert_eq!(entries[2].heading_level(), None);
    assert_eq!(entries[3].kind(), NavigationKind::FootnoteDefinition);
    assert_eq!(source_range(&entries[3]), 36..49);
    assert!(entries[3].targets().is_empty());
    assert!(
        entries.iter().all(|entry| source_range(entry) != (56..60)),
        "GFM 无定义引用不应产生导航条目"
    );
    Ok(())
}

#[test]
fn dangling_footnote_reference_has_no_event_entry() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("Gamma[^c].\n")?;
    assert!(parsed.navigation().is_empty());
    Ok(())
}

#[test]
fn footnote_label_case_divergence_yields_empty_targets() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("A[^Note].\n\n[^note]: Note.\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[0]), 1..8);
    assert!(entries[0].targets().is_empty());
    assert_eq!(entries[1].kind(), NavigationKind::FootnoteDefinition);
    assert_eq!(source_range(&entries[1]), 11..25);
    assert!(entries[1].targets().is_empty());
    Ok(())
}

#[test]
fn duplicate_labels_keep_all_targets_in_source_order() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("[^a]: first\n\n[^a]: second\n\nOne[^a] Two[^a].\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].kind(), NavigationKind::FootnoteDefinition);
    assert_eq!(source_range(&entries[0]), 0..11);
    assert_eq!(target_ranges(&entries[0]), vec![30..34, 38..42]);
    assert_eq!(entries[1].kind(), NavigationKind::FootnoteDefinition);
    assert_eq!(source_range(&entries[1]), 13..25);
    assert_eq!(target_ranges(&entries[1]), vec![30..34, 38..42]);
    assert_eq!(entries[2].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[2]), 30..34);
    assert_eq!(target_ranges(&entries[2]), vec![0..11, 13..25]);
    assert_eq!(entries[3].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[3]), 38..42);
    assert_eq!(target_ranges(&entries[3]), vec![0..11, 13..25]);
    Ok(())
}

#[test]
fn nested_footnote_reference_is_indexed() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("Top[^a]\n\n[^a]: See[^b].\n\n[^b]: B.\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 4);
    assert_eq!(entries[0].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[0]), 3..7);
    assert_eq!(target_ranges(&entries[0]), vec![9..23]);
    assert_eq!(entries[1].kind(), NavigationKind::FootnoteDefinition);
    assert_eq!(source_range(&entries[1]), 9..23);
    assert_eq!(target_ranges(&entries[1]), vec![3..7]);
    assert_eq!(entries[2].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[2]), 18..22);
    assert_eq!(target_ranges(&entries[2]), vec![25..33]);
    assert_eq!(entries[3].kind(), NavigationKind::FootnoteDefinition);
    assert_eq!(source_range(&entries[3]), 25..33);
    assert_eq!(target_ranges(&entries[3]), vec![18..22]);
    Ok(())
}

#[test]
fn nested_footnote_definition_is_not_indexed() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("> [^nested]: nested\n\ninner[^nested]\n")?;
    let entries = parsed.navigation().entries();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].kind(), NavigationKind::FootnoteReference);
    assert_eq!(source_range(&entries[0]), 26..35);
    assert!(
        entries[0].targets().is_empty(),
        "块引用内的定义不是顶层定义，不能作为目标"
    );
    assert!(
        protected_ranges(&parsed, ProtectionKind::FootnoteDefinition).is_empty(),
        "块引用内的定义不应产生顶层定义保护"
    );
    Ok(())
}

#[test]
fn navigation_ranges_share_the_snapshot_and_reject_old_snapshot() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("# A\n\n[toc]\n\nText[^a].\n\n[^a]: note\n")?;
    let snapshot_id = parsed.snapshot().id();
    for entry in parsed.navigation().entries() {
        assert_eq!(entry.source().snapshot_id(), snapshot_id);
        assert_eq!(parsed.validate_range(entry.source())?, source_range(entry));
        for target in entry.targets() {
            assert_eq!(target.snapshot_id(), snapshot_id);
            assert_eq!(parsed.validate_range(target)?, target.byte_range());
        }
    }
    let old = parse_text("# Old\n")?;
    let new = parse_text("# New\n")?;
    let old_entry = &old.navigation().entries()[0];
    assert!(matches!(
        new.validate_range(old_entry.source()),
        Err(ParseError::Source(BufferError::SnapshotMismatch))
    ));
    Ok(())
}

#[test]
fn raw_fallback_has_empty_navigation() -> Result<(), Box<dyn Error>> {
    let parsed = parse_with_budget("# A\n", ParseBudget::m0().with_max_nodes(0))?;
    assert!(parsed.navigation().is_empty());
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(parsed.blocks()[0].kind(), BlockKind::Raw);
    assert_eq!(parsed.stats().budget_hits(), 1);
    assert!(
        parsed
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BudgetExceeded)
    );
    assert!(
        parsed
            .protected_ranges()
            .iter()
            .any(|range| range.kind() == ProtectionKind::RawFallback)
    );
    Ok(())
}

fn heading_and_toc_document(count: usize) -> String {
    let mut text = String::new();
    for index in 0..count {
        text.push_str(&format!("# H{index}\n\n[toc]\n\n"));
    }
    text
}

#[test]
fn toc_navigation_budget_counts_every_entry_and_target() -> Result<(), Box<dyn Error>> {
    let count = 10;
    let text = heading_and_toc_document(count);
    let exact = count * 2 + count * count;
    let parsed = parse_with_budget(&text, ParseBudget::m0().with_max_nodes(exact))?;
    assert_eq!(parsed.navigation().len(), count * 2);
    assert_eq!(total_targets(&parsed), count * count);
    assert_eq!(parsed.stats().budget_hits(), 0);

    let over = parse_with_budget(&text, ParseBudget::m0().with_max_nodes(exact - 1))?;
    assert!(over.navigation().is_empty());
    assert_eq!(over.stats().budget_hits(), 1);
    assert_eq!(over.blocks()[0].kind(), BlockKind::Raw);
    assert!(
        over.diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BudgetExceeded)
    );
    assert!(
        over.protected_ranges()
            .iter()
            .all(|range| range.kind() == ProtectionKind::RawFallback)
    );
    Ok(())
}

fn footnote_document(count: usize) -> String {
    let mut text = String::new();
    for _ in 0..count {
        text.push_str("x[^a]");
    }
    text.push_str("\n\n");
    for _ in 0..count {
        text.push_str("[^a]: note\n\n");
    }
    text
}

#[test]
fn footnote_navigation_budget_counts_both_directions() -> Result<(), Box<dyn Error>> {
    let count = 10;
    let text = footnote_document(count);
    let exact = count * 2 + count * count * 2;
    let parsed = parse_with_budget(&text, ParseBudget::m0().with_max_nodes(exact))?;
    assert_eq!(parsed.navigation().len(), count * 2);
    assert_eq!(total_targets(&parsed), count * count * 2);
    assert_eq!(parsed.stats().budget_hits(), 0);

    let over = parse_with_budget(&text, ParseBudget::m0().with_max_nodes(exact - 1))?;
    assert!(over.navigation().is_empty());
    assert_eq!(over.stats().budget_hits(), 1);
    assert_eq!(over.blocks()[0].kind(), BlockKind::Raw);
    assert!(
        over.diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.kind() == DiagnosticKind::BudgetExceeded)
    );
    Ok(())
}

#[test]
fn navigation_budget_sums_multiple_footnote_label_groups() -> Result<(), Box<dyn Error>> {
    let count = 4;
    let mut text = String::new();
    for _ in 0..count {
        text.push_str("x[^a]y[^b]");
    }
    text.push_str("\n\n");
    for _ in 0..count {
        text.push_str("[^a]: note a\n\n");
    }
    for _ in 0..count {
        text.push_str("[^b]: note b\n\n");
    }
    let exact = count * 4 + count * count * 4;
    let parsed = parse_with_budget(&text, ParseBudget::m0().with_max_nodes(exact))?;
    assert_eq!(parsed.navigation().len(), count * 4);
    assert_eq!(total_targets(&parsed), count * count * 4);
    assert_eq!(parsed.stats().budget_hits(), 0);

    let over = parse_with_budget(&text, ParseBudget::m0().with_max_nodes(exact - 1))?;
    assert!(over.navigation().is_empty());
    assert_eq!(over.stats().budget_hits(), 1);
    assert_eq!(over.blocks()[0].kind(), BlockKind::Raw);
    Ok(())
}
