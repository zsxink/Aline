//! 组 6.5：有限 HTML 标签、样式、实体、过滤、未闭合尾部与预算回退。

use std::error::Error;
use std::ops::Range;
use std::path::PathBuf;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    DiagnosticKind, MappingPrecision, MappingSegment, ParseBudget, ParseConfig, Parsed,
    ProtectionKind, parse,
};

const BUDGET: usize = 1024 * 1024;

fn parse_with(text: &str, budget: ParseBudget) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), budget)?)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    parse_with(text, ParseBudget::m0())
}

fn segment_with_source(
    parsed: &Parsed,
    source: Range<usize>,
) -> Result<&MappingSegment, Box<dyn Error>> {
    parsed
        .projection()
        .segments()
        .iter()
        .find(|segment| segment.source_range().byte_range() == source)
        .ok_or_else(|| format!("missing segment for source {source:?}").into())
}

/// 字面文本断言：逐字显示、无样式、每个可见片段都精确回指同偏移源码。
fn assert_literal_segments(parsed: &Parsed, expected: &str) {
    assert_eq!(parsed.projection().drawn_text(), expected);
    assert!(parsed.styled_ranges().is_empty());
    for segment in parsed.projection().segments() {
        if segment.precision() == MappingPrecision::Hidden {
            continue;
        }
        assert_eq!(segment.precision(), MappingPrecision::Exact);
        assert_eq!(segment.drawn_range(), segment.source_range().byte_range());
    }
}

#[test]
fn supported_tags_map_to_styles_and_hide_markup() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<b>粗</b><i>斜</i>\n")?;
    assert_eq!(parsed.projection().drawn_text(), "粗斜");
    let styled = parsed.styled_ranges();
    assert_eq!(styled.len(), 2);
    let bold = styled.first().ok_or("missing bold")?;
    assert_eq!(bold.range().byte_range(), 3..6);
    assert!(bold.style().bold());
    assert!(!bold.style().italic());
    let italic = styled.get(1).ok_or("missing italic")?;
    assert_eq!(italic.range().byte_range(), 13..16);
    assert!(italic.style().italic());
    assert!(!italic.style().bold());
    assert!(
        parsed
            .protected_ranges()
            .iter()
            .any(|range| range.kind() == ProtectionKind::Html)
    );
    Ok(())
}

#[test]
fn nested_supported_tags_merge_styles() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<b><i>x</i></b>\n")?;
    assert_eq!(parsed.projection().drawn_text(), "x");
    let styled = parsed.styled_ranges();
    assert_eq!(styled.len(), 1);
    let style = styled.first().ok_or("missing style")?;
    assert_eq!(style.range().byte_range(), 6..7);
    assert!(style.style().bold());
    assert!(style.style().italic());
    Ok(())
}

#[test]
fn links_require_allowed_href_schemes() -> Result<(), Box<dyn Error>> {
    let https = parse_text("<a href=\"https://e\">x</a>\n")?;
    assert_eq!(https.projection().drawn_text(), "x");
    let styled = https.styled_ranges();
    assert_eq!(styled.len(), 1);
    assert_eq!(
        styled.first().map(|range| range.range().byte_range()),
        Some(20..21)
    );
    assert!(styled.first().is_some_and(|range| range.style().link()));

    let relative = parse_text("<a href=\"/docs\">x</a>\n")?;
    assert!(
        relative
            .styled_ranges()
            .first()
            .is_some_and(|range| range.style().link())
    );

    let unsafe_scheme = parse_text("<a href=\"javascript:alert(1)\">x</a>\n")?;
    assert_eq!(unsafe_scheme.projection().drawn_text(), "x");
    assert!(unsafe_scheme.styled_ranges().is_empty());

    let no_href = parse_text("<a>x</a>\n")?;
    assert_eq!(no_href.projection().drawn_text(), "x");
    assert!(no_href.styled_ranges().is_empty());
    Ok(())
}

#[test]
fn quoted_greater_than_does_not_end_tag_early() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<span title=\"a>b\">x</span>\n")?;
    assert_eq!(parsed.projection().drawn_text(), "x");
    assert!(parsed.styled_ranges().is_empty());
    assert_eq!(parsed.blocks().len(), 1);
    Ok(())
}

#[test]
fn cross_nesting_pops_through_outer_tag() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<b><i>x</b>y</i>\n")?;
    assert_eq!(parsed.projection().drawn_text(), "xy");
    let styled = parsed.styled_ranges();
    assert_eq!(styled.len(), 1);
    let inner = styled.first().ok_or("missing style")?;
    assert_eq!(inner.range().byte_range(), 6..7);
    assert!(inner.style().bold());
    assert!(inner.style().italic());
    Ok(())
}

#[test]
fn comments_and_processing_instructions_are_hidden() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("a<!-- hidden -->b<?php x ?>c\n")?;
    assert_eq!(parsed.projection().drawn_text(), "abc");
    assert!(parsed.styled_ranges().is_empty());
    let protected = parsed.protected_ranges();
    assert_eq!(protected.len(), 2);
    assert_eq!(
        protected.first().map(|range| range.range().byte_range()),
        Some(1..16)
    );
    assert_eq!(
        protected.get(1).map(|range| range.range().byte_range()),
        Some(17..27)
    );
    Ok(())
}

#[test]
fn entities_inside_html_keep_aligned_mapping() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<b>a &amp; b</b>\n")?;
    let projection = parsed.projection();
    assert_eq!(projection.drawn_text(), "a & b");
    let entity = projection
        .segments()
        .iter()
        .find(|segment| segment.source_range().byte_range() == (5..10))
        .ok_or("missing entity segment")?;
    assert_eq!(entity.drawn_range(), 2..3);
    assert_eq!(
        entity.precision(),
        aline_markdown::MappingPrecision::Aligned
    );
    let styled = parsed.styled_ranges();
    // §11.2 折叠空白：a、两个折叠空格、实体解码出的 &、b 各自成段且都继承粗体，
    // 因此样式区间由 3 段变为 5 段；实体片段本身仍映射到 5..10。
    assert_eq!(styled.len(), 5);
    assert!(styled.iter().all(|range| range.style().bold()));
    Ok(())
}

#[test]
fn filtered_tags_remain_visible_literal_text() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("a <script>alert(1)</script> b\n")?;
    assert_eq!(
        parsed.projection().drawn_text(),
        "a <script>alert(1)</script> b"
    );
    assert!(parsed.styled_ranges().is_empty());
    Ok(())
}

#[test]
fn unclosed_supported_tag_keeps_style_without_panicking() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<b>abc\n")?;
    assert_eq!(parsed.projection().drawn_text(), "abc");
    let styled = parsed.styled_ranges();
    assert_eq!(styled.len(), 1);
    assert_eq!(
        styled.first().map(|range| range.range().byte_range()),
        Some(3..6)
    );
    assert!(styled.first().is_some_and(|range| range.style().bold()));
    Ok(())
}

#[test]
fn html_budget_failure_falls_back_to_whole_source() -> Result<(), Box<dyn Error>> {
    let source = "<b>abc</b>\n";
    let budget = ParseBudget::m0().with_max_scan_steps(0);
    let parsed = parse_with(source, budget)?;
    assert_eq!(parsed.projection().drawn_text(), source);
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(aline_markdown::BlockKind::Raw)
    );
    let range = parsed.snapshot().src_range(0..source.len())?;
    assert_eq!(
        parsed
            .snapshot()
            .slice(&range, TextBudget::new(BUDGET))?
            .as_ref(),
        source
    );
    let diagnostics = parsed.diagnostics();
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics.first().map(|diagnostic| diagnostic.kind()),
        Some(DiagnosticKind::BudgetExceeded)
    );
    Ok(())
}

#[test]
fn html_block_fixture_stays_protected_and_styled() -> Result<(), Box<dyn Error>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("m0")
        .join("md11.md");
    let source = std::fs::read_to_string(path)?;
    let parsed = parse_text(&source)?;
    assert!(parsed.protected_ranges().iter().any(
        |range| range.kind() == ProtectionKind::Html && range.range().byte_range() == (70..99)
    ));
    assert!(
        parsed
            .styled_ranges()
            .iter()
            .any(|range| { range.range().byte_range() == (83..89) && range.style().bold() })
    );
    Ok(())
}

#[test]
fn filtered_block_tags_display_literal_text() -> Result<(), Box<dyn Error>> {
    let title = parse_text("<title>x</title>\n")?;
    assert_literal_segments(&title, "<title>x</title>");
    // HtmlBlock 的保护范围按块内容（不含块尾换行）整体给出。
    assert!(title.protected_ranges().iter().any(|range| {
        range.kind() == ProtectionKind::Html && range.range().byte_range() == (0..16)
    }));

    let textarea = parse_text("<textarea><b>x</b></textarea>\n")?;
    assert_literal_segments(&textarea, "<textarea><b>x</b></textarea>");
    Ok(())
}

#[test]
fn nested_filtered_tags_do_not_leak_styles() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<script><title>x</title></script>\n")?;
    assert_literal_segments(&parsed, "<script><title>x</title></script>");
    Ok(())
}

#[test]
fn filtered_inline_tags_display_literal_text() -> Result<(), Box<dyn Error>> {
    let xmp = parse_text("<xmp><i>y</i></xmp>\n")?;
    assert_literal_segments(&xmp, "<xmp><i>y</i></xmp>");

    let plaintext = parse_text("<plaintext>a <b>c</b>\n")?;
    assert_literal_segments(&plaintext, "<plaintext>a <b>c</b>");

    // 行内过滤标签内部的连续空白按原文保留，不参与 HTML 空白折叠。
    let inline = parse_text("x <textarea>a   b</textarea> y\n")?;
    assert_literal_segments(&inline, "x <textarea>a   b</textarea> y");
    Ok(())
}

#[test]
fn unclosed_filtered_tags_keep_remaining_source_visible() -> Result<(), Box<dyn Error>> {
    let title = parse_text("<title>x\n")?;
    assert_literal_segments(&title, "<title>x\n");

    let textarea = parse_text("<textarea><b>x\n")?;
    assert_literal_segments(&textarea, "<textarea><b>x\n");
    Ok(())
}

#[test]
fn html_whitespace_collapses_with_full_source_coverage() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<b>a   b\n c</b>\n")?;
    assert_eq!(parsed.projection().drawn_text(), "a b c");
    let first = segment_with_source(&parsed, 4..7)?;
    assert_eq!(first.drawn_range(), 1..2);
    assert_eq!(first.precision(), MappingPrecision::Aligned);
    // 软换行 8..9 与事件间被丢弃的缩进空格 9..10 合并为一个折叠空格。
    let second = segment_with_source(&parsed, 8..10)?;
    assert_eq!(second.drawn_range(), 3..4);
    assert_eq!(second.precision(), MappingPrecision::Aligned);
    let styled = parsed.styled_ranges();
    assert_eq!(styled.len(), 5);
    assert!(styled.iter().all(|range| range.style().bold()));
    Ok(())
}

#[test]
fn block_tag_boundaries_insert_synthetic_newlines() -> Result<(), Box<dyn Error>> {
    let divs = parse_text("<div>a</div><div>b</div>\n")?;
    assert_eq!(divs.projection().drawn_text(), "a\nb\n");
    assert!(divs.styled_ranges().is_empty());
    let first = segment_with_source(&divs, 6..6)?;
    assert_eq!(first.drawn_range(), 1..2);
    assert_eq!(first.precision(), MappingPrecision::Synthetic);
    let second = segment_with_source(&divs, 18..18)?;
    assert_eq!(second.drawn_range(), 3..4);
    assert_eq!(second.precision(), MappingPrecision::Synthetic);

    let paragraphs = parse_text("<p>a</p>\n<p>b</p>\n")?;
    assert_eq!(paragraphs.projection().drawn_text(), "a\nb\n");
    let first = segment_with_source(&paragraphs, 4..4)?;
    assert_eq!(first.drawn_range(), 1..2);
    assert_eq!(first.precision(), MappingPrecision::Synthetic);

    let list = parse_text("<ul><li>a</li><li>b</li></ul>\n")?;
    assert_eq!(list.projection().drawn_text(), "a\nb\n");
    let first = segment_with_source(&list, 9..9)?;
    assert_eq!(first.drawn_range(), 1..2);
    assert_eq!(first.precision(), MappingPrecision::Synthetic);
    Ok(())
}

#[test]
fn block_whitespace_is_collapsed_and_trimmed_at_edges() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<div>\n a   b\n</div>\n")?;
    assert_eq!(parsed.projection().drawn_text(), "a b\n");
    // 块首换行与缩进折叠出的前导空格不绘制，也不得留下单独的空格片段。
    assert!(
        parsed
            .projection()
            .segments()
            .iter()
            .all(|segment| segment.source_range().byte_range() != (5..7))
    );
    let space = segment_with_source(&parsed, 8..11)?;
    assert_eq!(space.drawn_range(), 1..2);
    assert_eq!(space.precision(), MappingPrecision::Aligned);
    let line_break = segment_with_source(&parsed, 13..13)?;
    assert_eq!(line_break.drawn_range(), 3..4);
    assert_eq!(line_break.precision(), MappingPrecision::Synthetic);
    Ok(())
}

#[test]
fn html_entities_decode_with_aligned_mapping() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("<div><b>&#65; &#x1F600; &NotEqualTilde;</b></div>\n")?;
    assert_eq!(
        parsed.projection().drawn_text(),
        "A \u{1f600} \u{2242}\u{338}\n"
    );
    let decimal = segment_with_source(&parsed, 8..13)?;
    assert_eq!(decimal.drawn_range(), 0..1);
    assert_eq!(decimal.precision(), MappingPrecision::Aligned);
    let first_space = segment_with_source(&parsed, 13..14)?;
    assert_eq!(first_space.drawn_range(), 1..2);
    assert_eq!(first_space.precision(), MappingPrecision::Aligned);
    let hex = segment_with_source(&parsed, 14..23)?;
    assert_eq!(hex.drawn_range(), 2..6);
    assert_eq!(hex.precision(), MappingPrecision::Aligned);
    let named = segment_with_source(&parsed, 24..39)?;
    assert_eq!(named.drawn_range(), 7..12);
    assert_eq!(named.precision(), MappingPrecision::Aligned);
    let line_break = segment_with_source(&parsed, 43..43)?;
    assert_eq!(line_break.drawn_range(), 12..13);
    assert_eq!(line_break.precision(), MappingPrecision::Synthetic);
    let styled = parsed.styled_ranges();
    assert_eq!(styled.len(), 5);
    assert!(styled.iter().all(|range| range.style().bold()));
    Ok(())
}

#[test]
fn unsupported_non_void_container_is_preserved_as_literal_source() -> Result<(), Box<dyn Error>> {
    let source = "<foo>a   b</foo>\n";
    let parsed = parse_text(source)?;
    assert_eq!(parsed.projection().drawn_text(), "<foo>a   b</foo>");
    assert_literal_segments(&parsed, "<foo>a   b</foo>");
    assert!(parsed.styled_ranges().is_empty());

    // 对照：支持的 `span` 隐藏标签并折叠 HTML 文本空白。
    let supported = parse_text("<span>e   f</span>\n")?;
    assert_eq!(supported.projection().drawn_text(), "e f");
    Ok(())
}

#[test]
fn unsupported_void_tags_are_preserved_as_literal_source() -> Result<(), Box<dyn Error>> {
    for tag in [
        "<input>", "<link>", "<meta>", "<base>", "<img>", "<hr>", "<source>", "<area>", "<col>",
    ] {
        let parsed = parse_text(&format!("{tag}\n"))?;
        assert_eq!(parsed.projection().drawn_text(), tag, "tag {tag}");
        assert_literal_segments(&parsed, tag);
    }
    Ok(())
}

#[test]
fn tags_only_html_produces_no_visible_text() -> Result<(), Box<dyn Error>> {
    let empty = parse_text("<b></b>\n")?;
    assert_eq!(empty.projection().drawn_text(), "");
    assert!(empty.styled_ranges().is_empty());
    assert!(empty.diagnostics().is_empty());
    assert!(
        empty
            .projection()
            .segments()
            .iter()
            .all(|segment| segment.precision() == MappingPrecision::Hidden)
    );
    assert!(empty.protected_ranges().iter().any(|range| {
        range.kind() == ProtectionKind::Html && range.range().byte_range() == (0..7)
    }));

    let blank = parse_text("<i> </i>\n")?;
    assert_eq!(blank.projection().drawn_text(), "");
    assert!(blank.styled_ranges().is_empty());
    assert!(blank.diagnostics().is_empty());
    assert!(
        blank
            .projection()
            .segments()
            .iter()
            .all(|segment| segment.precision() == MappingPrecision::Hidden)
    );
    assert!(blank.protected_ranges().iter().any(|range| {
        range.kind() == ProtectionKind::Html && range.range().byte_range() == (0..3)
    }));
    assert!(blank.protected_ranges().iter().any(|range| {
        range.kind() == ProtectionKind::Html && range.range().byte_range() == (4..8)
    }));
    Ok(())
}
