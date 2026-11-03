//! M1 局部布局窗口与源码行投影的独立映射预期。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{Affinity, TextBudget, TextSnapshot};
use aline_markdown::{
    MappingBudget, MappingError, MappingPrecision, MappingSegment, ParseBudget, ParseConfig,
    Projection, parse,
};

const BUDGET: usize = 1024 * 1024;

fn parsed(text: &str) -> Result<aline_markdown::Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_owned(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

fn window(projection: &Projection, range: Range<usize>) -> Result<Projection, Box<dyn Error>> {
    let range = projection.drawn_range(range)?;
    Ok(projection.window(&range, MappingBudget::new(BUDGET, BUDGET))?)
}

#[test]
fn local_exact_window_maps_to_full_source_without_caller_offsets() -> Result<(), Box<dyn Error>> {
    let parsed = parsed("before 中文 👨‍👩‍👧‍👦 after\n")?;
    let parent = parsed.projection();
    let text = parent.drawn_text();
    let start = text.find("中文").ok_or("missing Chinese text")?;
    let child = window(parent, start..start + "中文".len())?;
    assert_eq!(child.drawn_text(), "中文");
    let local = child.drawn_offset("中".len())?;
    let mapped = child.source_at(&local, Affinity::Before)?;
    assert_eq!(mapped.precision(), MappingPrecision::Exact);
    assert_eq!(mapped.value().byte_offset(), start + "中".len());
    assert_eq!(child.snapshot_id(), parsed.snapshot().id());
    Ok(())
}

#[test]
fn entity_hidden_and_synthetic_relations_survive_windowing() -> Result<(), Box<dyn Error>> {
    let entity_parsed = parsed("a &amp; b\n")?;
    let entity_parent = entity_parsed.projection();
    let entity_start = entity_parent
        .drawn_text()
        .find('&')
        .ok_or("missing decoded entity")?;
    let entity = window(entity_parent, entity_start..entity_start + 1)?;
    let entity_map = entity.source_at(&entity.drawn_offset(0)?, Affinity::After)?;
    assert_eq!(entity_map.precision(), MappingPrecision::Aligned);
    assert_eq!(entity_map.source_range().byte_range(), 2..7);
    assert!(entity.segments().iter().any(|segment| {
        segment.precision() == MappingPrecision::Aligned
            && segment.source_range().byte_range() == (2..7)
    }));

    let multi_parsed = parsed("&NotEqualTilde;\n")?;
    let multi_parent = multi_parsed.projection();
    let (_, first_scalar) = multi_parent
        .drawn_text()
        .char_indices()
        .next()
        .ok_or("missing multi-character entity")?;
    let second_start = first_scalar.len_utf8();
    for range in [
        0..second_start,
        second_start..multi_parent.drawn_text().len(),
    ] {
        let part = window(multi_parent, range)?;
        assert_eq!(part.segments()[0].precision(), MappingPrecision::Aligned);
        assert_eq!(part.segments()[0].source_range().byte_range(), 0..15);
    }

    let hidden_parsed = parsed("**word**\n")?;
    let hidden = window(hidden_parsed.projection(), 0..2)?;
    assert!(
        hidden
            .segments()
            .iter()
            .any(|segment| segment.precision() == MappingPrecision::Hidden)
    );

    let synthetic_parsed = parsed("a<br>b\n")?;
    let synthetic_parent = synthetic_parsed.projection();
    let newline = synthetic_parent
        .drawn_text()
        .find('\n')
        .ok_or("missing synthetic HTML break")?;
    let synthetic = window(synthetic_parent, newline..newline + 1)?;
    assert!(synthetic.segments().iter().any(|segment| {
        segment.precision() == MappingPrecision::Synthetic
            && segment.source_range().byte_range().is_empty()
    }));
    Ok(())
}

#[test]
fn source_line_identity_keeps_crlf_tab_and_utf8_global_coordinates() -> Result<(), Box<dyn Error>> {
    let snapshot = TextSnapshot::new(
        "first\r\n\t中文e\u{301}\rthird".to_owned(),
        TextBudget::new(BUDGET),
    )?;
    let line_start = "first\r\n".len();
    let line_end = line_start + "\t中文e\u{301}\r".len();
    let source_range = snapshot.src_range(line_start..line_end)?;
    let line = Projection::identity(
        snapshot.clone(),
        source_range,
        MappingBudget::new(BUDGET, BUDGET),
    )?;
    assert_eq!(line.drawn_text(), "\t中文e\u{301}\r");
    let mapped = line.source_at(&line.drawn_offset("\t中".len())?, Affinity::Before)?;
    assert_eq!(mapped.value().byte_offset(), line_start + "\t中".len());
    assert_eq!(line.source_range().byte_range(), line_start..line_end);
    for sample in ["中文", "e\u{301}"] {
        let start = line
            .drawn_text()
            .find(sample)
            .ok_or("missing source line sample")?;
        let local = window(&line, start..start + sample.len())?;
        assert_eq!(local.drawn_text(), sample);
        assert_eq!(
            local
                .source_at(&local.drawn_offset(0)?, Affinity::Before)?
                .precision(),
            MappingPrecision::Exact
        );
    }
    Ok(())
}

#[test]
fn code_indent_and_zwj_text_keep_the_parser_mapping() -> Result<(), Box<dyn Error>> {
    let code = parsed("```text\n    value\n```\n")?;
    let parent = code.projection();
    let start = parent
        .drawn_text()
        .find("value")
        .ok_or("missing code text")?;
    let local = window(parent, start..start + "value".len())?;
    let mapped = local.source_at(&local.drawn_offset(0)?, Affinity::Before)?;
    assert_eq!(mapped.precision(), MappingPrecision::Exact);
    assert_eq!(mapped.value().byte_offset(), 12);

    let text = "family 👨‍👩‍👧‍👦 done\n";
    let parsed = parsed(text)?;
    let start = parsed
        .projection()
        .drawn_text()
        .find("👨‍👩‍👧‍👦")
        .ok_or("missing ZWJ family")?;
    let local = window(parsed.projection(), start..start + "👨‍👩‍👧‍👦".len())?;
    assert_eq!(local.drawn_text(), "👨‍👩‍👧‍👦");
    assert_eq!(
        local
            .source_at(&local.drawn_offset(0)?, Affinity::Before)?
            .value()
            .byte_offset(),
        start
    );
    Ok(())
}

#[test]
fn invalid_and_stale_window_ranges_are_rejected() -> Result<(), Box<dyn Error>> {
    let first = parsed("中文\n")?;
    let first_projection = first.projection();
    assert!(first_projection.drawn_range(1..3).is_err());
    assert!(first_projection.drawn_range(0..usize::MAX).is_err());

    let stale = first_projection.drawn_range(0.."中".len())?;
    let second = parsed("中文\n")?;
    assert!(
        second
            .projection()
            .window(&stale, MappingBudget::new(BUDGET, BUDGET))
            .is_err()
    );

    let valid = window(first_projection, 0.."中".len())?;
    assert!(
        valid
            .source_at(&first_projection.drawn_offset(0)?, Affinity::Before)
            .is_err()
    );
    let range = first_projection.drawn_range(0.."中".len())?;
    assert!(
        first_projection
            .window(&range, MappingBudget::new(1, BUDGET))
            .is_err()
    );
    assert!(
        first_projection
            .window(&range, MappingBudget::new(BUDGET, 1))
            .is_err()
    );
    Ok(())
}

#[test]
fn empty_window_is_rejected_without_fabricating_exact_entity_anchor() -> Result<(), Box<dyn Error>>
{
    let parsed = parsed("&amp;")?;
    let parent = parsed.projection();
    let at = parent.drawn_offset(0)?;
    for (affinity, expected_anchor) in [(Affinity::Before, 0), (Affinity::After, 5)] {
        let parent_mapping = parent.source_at(&at, affinity)?;
        assert_eq!(parent_mapping.precision(), MappingPrecision::Aligned);
        assert_eq!(parent_mapping.source_range().byte_range(), 0..5);
        assert_eq!(parent_mapping.value().byte_offset(), expected_anchor);
    }
    let empty = parent.drawn_range(0..0)?;
    assert!(matches!(
        parent.window(&empty, MappingBudget::new(BUDGET, BUDGET)),
        Err(MappingError::InvalidMapping)
    ));
    Ok(())
}

#[test]
fn hidden_zero_width_inside_visible_segment_does_not_break_window_lookup()
-> Result<(), Box<dyn Error>> {
    let snapshot = TextSnapshot::new("abcdH".to_owned(), TextBudget::new(BUDGET))?;
    let parent = Projection::from_segments(
        snapshot.clone(),
        snapshot.src_range(0..5)?,
        "abcd".to_owned(),
        vec![
            MappingSegment::new(0..4, snapshot.src_range(0..4)?, MappingPrecision::Exact)?,
            MappingSegment::new(1..1, snapshot.src_range(4..5)?, MappingPrecision::Hidden)?,
        ],
        MappingBudget::new(BUDGET, BUDGET),
    )?;
    let local = window(&parent, 2..3)?;
    assert_eq!(local.drawn_text(), "c");
    let mapped = local.source_at(&local.drawn_offset(0)?, Affinity::Before)?;
    assert_eq!(mapped.precision(), MappingPrecision::Exact);
    assert_eq!(mapped.value().byte_offset(), 2);
    assert_eq!(local.source_range().byte_range(), 2..3);
    let containing_hidden = window(&parent, 0..3)?;
    assert!(containing_hidden.segments().iter().any(|segment| {
        segment.precision() == MappingPrecision::Hidden
            && segment.drawn_range() == (1..1)
            && segment.source_range().byte_range() == (4..5)
    }));
    assert_eq!(window(&containing_hidden, 2..3)?.drawn_text(), "c");
    Ok(())
}
