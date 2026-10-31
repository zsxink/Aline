//! 组 6.2：解析副本重写、逐级复合、跨片段来源、控制字节与失败回退。

use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    CancelCheck, CancelToken, MappingPrecision, ParseBudget, ParseConfig, Rewrite, RewriteEdit,
    RewriteError, parse, rewrite_front_matter, rewrite_front_matter_with_cancel,
};

struct CancelAfter {
    checks: AtomicUsize,
    limit: usize,
}

impl CancelCheck for CancelAfter {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::Relaxed) >= self.limit
    }
}

fn edit(
    source: std::ops::Range<usize>,
    replacement: &str,
    precision: MappingPrecision,
) -> Result<RewriteEdit, RewriteError> {
    RewriteEdit::new(source, replacement.to_string(), precision)
}

#[test]
fn front_matter_is_hidden_in_parse_copy_and_maps_back_to_source() -> Result<(), Box<dyn Error>> {
    let source = "---\na: 1\n---\n# H\n";
    let rewrite = rewrite_front_matter(source, ParseBudget::m0())?;
    assert_eq!(rewrite.text(), "\n# H\n");
    assert_eq!(rewrite.map().rewritten_len(), 5);
    let heading = rewrite.map().to_source(1..4)?;
    assert_eq!(heading.source_range(), 13..16);
    assert_eq!(heading.precision(), MappingPrecision::Exact);
    let start = rewrite.map().to_source(0..0)?;
    assert_eq!(start.source_range(), 12..12);
    let end = rewrite.map().to_source(5..5)?;
    assert_eq!(end.source_range(), 17..17);
    assert!(rewrite.front_matter().is_some());
    Ok(())
}

#[test]
fn multi_edit_map_preserves_each_source_segment() -> Result<(), Box<dyn Error>> {
    let source = "abcdef";
    let edits = vec![
        edit(1..3, "XY", MappingPrecision::Exact)?,
        edit(4..4, "!", MappingPrecision::Synthetic)?,
    ];
    let rewrite = Rewrite::apply(source, &edits, ParseBudget::m0())?;
    assert_eq!(rewrite.text(), "aXYd!ef");
    assert_eq!(rewrite.map().rewritten_len(), 7);
    let replaced = rewrite.map().to_source(1..3)?;
    assert_eq!(replaced.source_range(), 1..3);
    assert_eq!(replaced.precision(), MappingPrecision::Aligned);
    let inserted = rewrite.map().to_source(4..5)?;
    assert_eq!(inserted.source_range(), 4..4);
    assert_eq!(inserted.precision(), MappingPrecision::Synthetic);
    let gap = rewrite.map().to_source(3..4)?;
    assert_eq!(gap.source_range(), 3..4);
    assert_eq!(gap.precision(), MappingPrecision::Exact);
    let tail = rewrite.map().to_source(5..7)?;
    assert_eq!(tail.source_range(), 4..6);
    assert_eq!(tail.precision(), MappingPrecision::Exact);
    let whole = rewrite.map().to_source(0..7)?;
    assert_eq!(whole.source_range(), 0..6);
    assert_eq!(whole.precision(), MappingPrecision::Synthetic);
    Ok(())
}

#[test]
fn maps_compose_through_two_rewrite_levels() -> Result<(), Box<dyn Error>> {
    let first = Rewrite::apply(
        "abcdef",
        &[edit(1..3, "X", MappingPrecision::Exact)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(first.text(), "aXdef");
    let second = Rewrite::apply(
        first.text(),
        &[edit(0..1, "", MappingPrecision::Hidden)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(second.text(), "Xdef");
    let composed = first.map().compose(second.map())?;
    assert_eq!(composed.rewritten_len(), 4);
    let mapped = composed.to_source(0..4)?;
    assert_eq!(mapped.source_range(), 1..6);
    assert_eq!(mapped.precision(), MappingPrecision::Aligned);
    let anchor = composed.to_source(0..0)?;
    assert_eq!(anchor.source_range(), 1..6);
    Ok(())
}

#[test]
fn control_bytes_do_not_collide_with_synthetic_markers() -> Result<(), Box<dyn Error>> {
    let source = "abc";
    let rewrite = Rewrite::apply(
        source,
        &[edit(0..0, "\u{1}\u{2}\u{8}", MappingPrecision::Synthetic)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(rewrite.text(), "\u{1}\u{2}\u{8}abc");
    let synthetic = rewrite.map().to_source(0..3)?;
    assert_eq!(synthetic.source_range(), 0..0);
    assert_eq!(synthetic.precision(), MappingPrecision::Synthetic);
    let exact = rewrite.map().to_source(3..6)?;
    assert_eq!(exact.source_range(), 0..3);
    assert_eq!(exact.precision(), MappingPrecision::Exact);
    Ok(())
}

#[test]
fn reversed_overlapping_and_non_boundary_edits_are_rejected() -> Result<(), Box<dyn Error>> {
    let (start, end) = (3usize, 1usize);
    assert!(matches!(
        edit(start..end, "x", MappingPrecision::Exact),
        Err(RewriteError::ReversedRange)
    ));
    let source = "abcdef";
    let overlapping = vec![
        edit(1..4, "x", MappingPrecision::Exact)?,
        edit(3..5, "y", MappingPrecision::Exact)?,
    ];
    assert!(matches!(
        Rewrite::apply(source, &overlapping, ParseBudget::m0()),
        Err(RewriteError::OverlappingEdits)
    ));
    let not_boundary = vec![edit(1..2, "x", MappingPrecision::Exact)?];
    assert!(matches!(
        Rewrite::apply("中文", &not_boundary, ParseBudget::m0()),
        Err(RewriteError::NotCharBoundary)
    ));
    let out_of_bounds = vec![edit(0..99, "x", MappingPrecision::Exact)?];
    assert!(matches!(
        Rewrite::apply(source, &out_of_bounds, ParseBudget::m0()),
        Err(RewriteError::InvalidRange)
    ));
    Ok(())
}

#[test]
fn parse_budget_failure_falls_back_to_original_text() -> Result<(), Box<dyn Error>> {
    let source = "# 标题\n\n正文\n";
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(1024))?;
    let budget = ParseBudget::m0().with_max_segments(0);
    let parsed = parse(&snapshot, ParseConfig::m0(), budget)?;
    assert_eq!(parsed.projection().drawn_text(), source);
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(aline_markdown::BlockKind::Raw)
    );
    assert_eq!(parsed.diagnostics().len(), 1);
    assert_eq!(parsed.stats().budget_hits(), 1);
    Ok(())
}

#[test]
fn pre_cancelled_rewrite_returns_cancelled_without_text() -> Result<(), Box<dyn Error>> {
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = Rewrite::apply_with_cancel("abc", &[], ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(RewriteError::Cancelled)));
    Ok(())
}

#[test]
fn cancellable_rewrite_stops_between_edits() -> Result<(), Box<dyn Error>> {
    let edits = vec![
        edit(0..1, "X", MappingPrecision::Exact)?,
        edit(2..3, "Y", MappingPrecision::Exact)?,
    ];
    let cancel = CancelAfter {
        checks: AtomicUsize::new(0),
        limit: 3,
    };
    let result = Rewrite::apply_with_cancel("abcdef", &edits, ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(RewriteError::Cancelled)));
    Ok(())
}

#[test]
fn pre_cancelled_front_matter_rewrite_returns_cancelled() -> Result<(), Box<dyn Error>> {
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = rewrite_front_matter_with_cancel("---\na: 1\n---\n", ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(RewriteError::Cancelled)));
    Ok(())
}
