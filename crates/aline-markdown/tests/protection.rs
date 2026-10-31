//! 组 6.5：front matter、脚注、目录、HTML 与回退保护范围。

use std::error::Error;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    BlockKind, DiagnosticKind, ParseBudget, ParseConfig, Parsed, ProtectionKind, parse,
};

const BUDGET: usize = 1024 * 1024;

fn parse_with(text: &str, budget: ParseBudget) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), budget)?)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    parse_with(text, ParseBudget::m0())
}

fn protected_entries(parsed: &Parsed) -> Vec<(std::ops::Range<usize>, ProtectionKind)> {
    parsed
        .protected_ranges()
        .iter()
        .map(|range| (range.range().byte_range(), range.kind()))
        .collect()
}

#[test]
fn front_matter_and_toc_are_protected_independently() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("---\nt: x\n---\n\n[toc]\n")?;
    assert_eq!(
        protected_entries(&parsed),
        vec![
            (0..12, ProtectionKind::FrontMatter),
            (14..19, ProtectionKind::Toc),
        ]
    );
    assert_eq!(parsed.projection().drawn_text(), "[toc]");
    assert_eq!(parsed.blocks().len(), 2);
    Ok(())
}

#[test]
fn adjacent_footnote_references_stay_separate_subranges() -> Result<(), Box<dyn Error>> {
    let source = "a[^1]b[^2]c\n\n[^1]: one\n[^2]: two\n";
    let parsed = parse_text(source)?;
    assert_eq!(
        protected_entries(&parsed),
        vec![
            (1..5, ProtectionKind::FootnoteReference),
            (6..10, ProtectionKind::FootnoteReference),
            (13..22, ProtectionKind::FootnoteDefinition),
            (23..32, ProtectionKind::FootnoteDefinition),
        ]
    );
    let kinds: Vec<BlockKind> = parsed.blocks().iter().map(|block| block.kind()).collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::Paragraph,
            BlockKind::FootnoteDefinition,
            BlockKind::FootnoteDefinition
        ]
    );
    Ok(())
}

#[test]
fn inline_html_tags_are_protected_per_tag() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("a <b>x</b> b\n")?;
    assert_eq!(
        protected_entries(&parsed),
        vec![(2..5, ProtectionKind::Html), (6..10, ProtectionKind::Html),]
    );
    assert_eq!(parsed.projection().drawn_text(), "a x b");
    Ok(())
}

#[test]
fn html_block_is_protected_as_one_range() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<div>x</div>\n")?;
    assert_eq!(
        protected_entries(&parsed),
        vec![(0..12, ProtectionKind::Html)]
    );
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::HtmlBlock)
    );
    Ok(())
}

#[test]
fn literal_code_does_not_trigger_protection_or_front_matter() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("```\n[^a]\n[toc]\n<b>x</b>\n```\n")?;
    assert!(parsed.protected_ranges().is_empty());
    assert!(parsed.front_matter().is_none());
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::CodeBlock)
    );
    Ok(())
}

#[test]
fn raw_fallback_protects_the_whole_source() -> Result<(), Box<dyn Error>> {
    let source = "# 标题\n\n正文\n";
    let parsed = parse_with(source, ParseBudget::m0().with_max_segments(0))?;
    assert_eq!(
        protected_entries(&parsed),
        vec![(0..source.len(), ProtectionKind::RawFallback)]
    );
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::Raw)
    );
    assert_eq!(
        parsed
            .diagnostics()
            .first()
            .map(|diagnostic| diagnostic.kind()),
        Some(DiagnosticKind::BudgetExceeded)
    );
    Ok(())
}
