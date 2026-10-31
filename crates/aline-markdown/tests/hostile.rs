//! 组 6.6：敌意输入下的预算终止、原文保持与不截断；注入值不是发布限额。

use std::error::Error;
use std::sync::atomic::{AtomicUsize, Ordering};

use aline_buffer::{BufferError, TextBudget, TextSnapshot};
use aline_markdown::{
    BlockKind, CancelCheck, CancelToken, DiagnosticKind, MappingError, ParseBudget, ParseConfig,
    ParseError, Parsed, parse, parse_with_cancel,
};

const BUDGET: usize = 64 * 1024 * 1024;

struct CancelAfter {
    checks: AtomicUsize,
    limit: usize,
}

impl CancelCheck for CancelAfter {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::Relaxed) >= self.limit
    }
}

fn parse_with(text: &str, budget: ParseBudget) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), budget)?)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    parse_with(text, ParseBudget::m0())
}

fn assert_raw_fallback(
    parsed: &Parsed,
    source: &str,
    kind: DiagnosticKind,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(parsed.projection().drawn_text(), source);
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::Raw)
    );
    assert_eq!(parsed.diagnostics().len(), 1);
    assert_eq!(
        parsed
            .diagnostics()
            .first()
            .map(|diagnostic| diagnostic.kind()),
        Some(kind)
    );
    let range = parsed.snapshot().src_range(0..source.len())?;
    let text = parsed.snapshot().slice(&range, TextBudget::new(BUDGET))?;
    assert_eq!(text.as_ref(), source);
    let expected_hits = usize::from(kind == DiagnosticKind::BudgetExceeded);
    assert_eq!(parsed.stats().budget_hits(), expected_hits);
    Ok(())
}

/// 交叉验证：报告的 `scan_steps` 必须等于二分得到的真实最小预算阈值。
///
/// 覆盖过滤标签内部“整段消费到闭标签”的样本；阈值减一时必须是整源回退。
fn cross_validate_scan_steps(
    label: &str,
    source: &str,
    expected: usize,
) -> Result<Parsed, Box<dyn Error>> {
    let measured = parse_text(source)?;
    let steps = measured.stats().scan_steps();
    assert_eq!(steps, expected, "{label}: reported scan_steps");
    assert_eq!(measured.stats().budget_hits(), 0, "{label}: no budget hit");
    let mut low = 0usize;
    let mut high = steps.saturating_add(1);
    while low < high {
        let middle = low.saturating_add(high.saturating_sub(low) / 2);
        let parsed = parse_with(source, ParseBudget::m0().with_max_scan_steps(middle))?;
        if parsed.diagnostics().is_empty() {
            high = middle;
        } else {
            low = middle.saturating_add(1);
        }
    }
    assert!(low > 0, "{label}: scan budget threshold must be positive");
    assert_eq!(
        low, steps,
        "{label}: binary-searched threshold must equal reported scan_steps"
    );
    let at_limit = parse_with(source, ParseBudget::m0().with_max_scan_steps(low))?;
    assert!(
        at_limit.diagnostics().is_empty(),
        "{label}: exact threshold must parse"
    );
    assert_eq!(at_limit.stats().scan_steps(), steps);
    let below = parse_with(
        source,
        ParseBudget::m0().with_max_scan_steps(low.saturating_sub(1)),
    )?;
    assert_raw_fallback(&below, source, DiagnosticKind::BudgetExceeded)?;
    assert_eq!(below.stats().scan_steps(), steps);
    Ok(measured)
}

#[test]
fn deep_nesting_terminates_with_raw_fallback() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    for _ in 0..300 {
        source.push_str("> ");
    }
    source.push_str("x\n");
    let parsed = parse_with(&source, ParseBudget::m0().with_max_nesting(8))?;
    assert_raw_fallback(&parsed, &source, DiagnosticKind::BudgetExceeded)?;
    Ok(())
}

#[test]
fn deep_nesting_within_budget_keeps_full_text() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    for _ in 0..300 {
        source.push_str("> ");
    }
    source.push_str("x\n");
    let parsed = parse_with(&source, ParseBudget::m0().with_max_nesting(400))?;
    assert_eq!(parsed.projection().drawn_text(), "x");
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::BlockQuote)
    );
    assert!(parsed.diagnostics().is_empty());
    Ok(())
}

#[test]
fn repeated_unclosed_tags_fall_back_to_whole_source_without_truncation()
-> Result<(), Box<dyn Error>> {
    let source = "<b>".repeat(64);
    let parsed = parse_with(&source, ParseBudget::m0().with_max_html_depth(8))?;
    assert_raw_fallback(&parsed, &source, DiagnosticKind::BudgetExceeded)?;
    Ok(())
}

#[test]
fn repeated_unclosed_quotes_stay_linear_and_keep_source() -> Result<(), Box<dyn Error>> {
    let source = "<a href=\"".repeat(200);
    let parsed = parse_text(&source)?;
    assert_eq!(parsed.projection().drawn_text(), source);
    let range = parsed.snapshot().src_range(0..source.len())?;
    let text = parsed.snapshot().slice(&range, TextBudget::new(BUDGET))?;
    assert_eq!(text.as_ref(), source);
    Ok(())
}

#[test]
fn huge_paragraph_is_not_truncated() -> Result<(), Box<dyn Error>> {
    let mut source = "字".repeat(200_000);
    source.push('\n');
    let parsed = parse_text(&source)?;
    assert_eq!(
        parsed.projection().drawn_text(),
        source.trim_end_matches('\n')
    );
    assert_eq!(parsed.projection().drawn_text().len(), 600_000);
    assert_eq!(parsed.blocks().len(), 1);
    assert!(parsed.diagnostics().is_empty());
    Ok(())
}

#[test]
fn huge_table_is_not_truncated_within_budget() -> Result<(), Box<dyn Error>> {
    let mut source = String::from("| a | b |\n| --- | --- |\n");
    let mut expected = String::from("a\tb");
    for index in 0..2_000 {
        source.push_str(&format!("| {index} | x |\n"));
        expected.push_str(&format!("\t{index}\tx"));
    }
    let parsed = parse_text(&source)?;
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::Table)
    );
    assert_eq!(parsed.projection().drawn_text(), expected);
    Ok(())
}

#[test]
fn injected_node_budget_terminates_before_publishing_partial_tree() -> Result<(), Box<dyn Error>> {
    let source = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
    let parsed = parse_with(source, ParseBudget::m0().with_max_nodes(4))?;
    assert_raw_fallback(&parsed, source, DiagnosticKind::BudgetExceeded)?;
    Ok(())
}

#[test]
fn source_budget_is_checked_before_reading() -> Result<(), Box<dyn Error>> {
    let source = "abcdef\n";
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(BUDGET))?;
    let error = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_source_bytes(3),
    );
    assert!(matches!(
        error,
        Err(aline_markdown::ParseError::Source(
            BufferError::BudgetExceeded { .. }
        ))
    ));
    Ok(())
}

#[test]
fn scan_steps_are_measured_by_the_budget_threshold() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    for _ in 0..40 {
        source.push_str("<b>x</b>");
    }
    source.push('\n');
    // 未过滤样本；过滤标签样本见 filtered_html_scan_steps_match_the_real_scanner。
    let measured = cross_validate_scan_steps("40 inline bold pairs", &source, 80)?;
    assert_eq!(measured.projection().drawn_text(), "x".repeat(40));
    Ok(())
}

#[test]
fn filtered_html_scan_steps_match_the_real_scanner() -> Result<(), Box<dyn Error>> {
    cross_validate_scan_steps(
        "filtered block with newlines",
        "<script>\n<b>x</b>\n</script>\n",
        5,
    )?;
    cross_validate_scan_steps(
        "filtered block on one line",
        "<script><b>x</b></script>\n",
        3,
    )?;
    cross_validate_scan_steps(
        "nested filtered tags",
        "<script><style>x</style></script>\n",
        3,
    )?;
    Ok(())
}

#[test]
fn unfiltered_html_scan_steps_match_the_real_scanner() -> Result<(), Box<dyn Error>> {
    cross_validate_scan_steps("unfiltered div block", "<div>\n<b>x</b>\n</div>\n", 8)?;
    Ok(())
}

#[test]
fn scan_step_budget_failure_falls_back_to_whole_source() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    for _ in 0..40 {
        source.push_str("<b>x</b>");
    }
    source.push('\n');
    let measured = parse_text(&source)?;
    let steps = measured.stats().scan_steps();
    let parsed = parse_with(
        &source,
        ParseBudget::m0().with_max_scan_steps(steps.saturating_sub(1)),
    )?;
    assert_raw_fallback(&parsed, &source, DiagnosticKind::BudgetExceeded)?;
    assert_eq!(parsed.stats().scan_steps(), steps);
    Ok(())
}

#[test]
fn rewritten_budget_failure_returns_budget_error_not_partial_text() -> Result<(), Box<dyn Error>> {
    let source = "---\nt: x\n---\n\nA\n";
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(BUDGET))?;
    let error = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_rewritten_bytes(2),
    );
    assert!(matches!(
        error,
        Err(aline_markdown::ParseError::BudgetExceeded)
    ));
    Ok(())
}

#[test]
fn pre_cancelled_parse_returns_cancelled_without_tree() -> Result<(), Box<dyn Error>> {
    let source = "# 标题\n\n正文\n";
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(BUDGET))?;
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = parse_with_cancel(&snapshot, ParseConfig::m0(), ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(ParseError::Cancelled)));
    Ok(())
}

#[test]
fn cooperative_cancel_stops_deep_nesting_without_hanging() -> Result<(), Box<dyn Error>> {
    let mut source = String::new();
    for _ in 0..200 {
        source.push_str("> ");
    }
    source.push_str("x\n");
    let snapshot = TextSnapshot::new(source, TextBudget::new(BUDGET))?;
    let cancel = CancelAfter {
        checks: AtomicUsize::new(0),
        limit: 6,
    };
    let result = parse_with_cancel(&snapshot, ParseConfig::m0(), ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(ParseError::Cancelled)));
    assert!(cancel.checks.load(Ordering::Relaxed) > 0);
    Ok(())
}

#[test]
fn cooperative_cancel_stops_huge_input_without_hanging() -> Result<(), Box<dyn Error>> {
    let mut source = "字".repeat(200_000);
    source.push('\n');
    let snapshot = TextSnapshot::new(source, TextBudget::new(BUDGET))?;
    let cancel = CancelAfter {
        checks: AtomicUsize::new(0),
        limit: 5,
    };
    let result = parse_with_cancel(&snapshot, ParseConfig::m0(), ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(ParseError::Cancelled)));
    Ok(())
}

#[test]
fn uncancelled_token_preserves_default_parse_result() -> Result<(), Box<dyn Error>> {
    let source = "a &amp; b\n";
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(BUDGET))?;
    let cancel = CancelToken::new();
    let parsed = parse_with_cancel(&snapshot, ParseConfig::m0(), ParseBudget::m0(), &cancel)?;
    let default = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    assert_eq!(parsed.dump_tree(), default.dump_tree());
    assert_eq!(parsed.projection().drawn_text(), "a & b");
    assert_eq!(parsed.blocks().len(), 1);
    assert!(!cancel.is_cancelled());
    Ok(())
}

#[test]
fn mapping_point_budget_is_counted_in_stats_and_enforced() -> Result<(), Box<dyn Error>> {
    let source = "abc\n";
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(BUDGET))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let points = parsed.stats().mapping_points();
    assert_eq!(points, 4);
    let segments = parsed.projection().segments().len();
    assert_eq!(segments, 2);
    assert!(
        segments <= points.saturating_sub(1),
        "the failing dimension must be index points, not segments"
    );
    let error = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_mapping_points(points.saturating_sub(1)),
    );
    assert!(
        matches!(
            error,
            Err(ParseError::Mapping(MappingError::BudgetExceeded))
        ),
        "points ({points}) exceed the limit while segments ({segments}) stay within it"
    );
    let exact = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_mapping_points(points),
    )?;
    assert_eq!(exact.stats().mapping_points(), points);
    Ok(())
}

#[test]
fn mapping_point_budget_also_caps_projection_segments() -> Result<(), Box<dyn Error>> {
    let source = "[a](u)\n";
    let baseline = parse_text(source)?;
    let points = baseline.stats().mapping_points();
    let segments = baseline.projection().segments().len();
    assert_eq!(points, 2);
    assert_eq!(segments, 3);
    assert!(
        segments > points,
        "segment count must exceed the point count so the segment limit fires first"
    );

    // max_mapping_points 兼作片段数上限：索引点恰好够用（2 <= 2），
    // 但片段数（3）超限，因此失败来自片段数维度而非索引点维度。
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(BUDGET))?;
    let segment_limited = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_mapping_points(points),
    );
    assert!(
        matches!(
            segment_limited,
            Err(ParseError::Mapping(MappingError::BudgetExceeded))
        ),
        "segments ({segments}) exceed the limit ({points}) while points stay within it"
    );
    let segment_exact = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_mapping_points(segments),
    )?;
    assert!(segment_exact.diagnostics().is_empty());
    assert_eq!(segment_exact.projection().segments().len(), segments);
    assert_eq!(segment_exact.stats().mapping_points(), points);
    Ok(())
}
