//! 组 6.3：源/绘制/UTF-16 坐标、实体对齐、隐藏与合成片段、选区方向和失败边界。

use std::collections::BTreeMap;
use std::error::Error;
use std::ops::Range;

use aline_buffer::{Affinity, BufferError, SrcSelection, TextBudget, TextSnapshot};
use aline_markdown::{
    MappingBudget, MappingError, MappingPrecision, MappingSegment, ParseBudget, ParseConfig,
    Parsed, Projection, parse,
};

const BUDGET: usize = 1024 * 1024;

fn snapshot(text: &str) -> Result<TextSnapshot, Box<dyn Error>> {
    Ok(TextSnapshot::new(
        text.to_string(),
        TextBudget::new(BUDGET),
    )?)
}

fn identity(text: &str) -> Result<(TextSnapshot, Projection), Box<dyn Error>> {
    let snapshot = snapshot(text)?;
    let range = snapshot.src_range(0..text.len())?;
    let projection =
        Projection::identity(snapshot.clone(), range, MappingBudget::new(BUDGET, BUDGET))?;
    Ok((snapshot, projection))
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = snapshot(text)?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

#[test]
fn identity_roundtrip_covers_all_char_boundaries() -> Result<(), Box<dyn Error>> {
    let text = "a中😀e\u{301}";
    let (_snapshot, projection) = identity(text)?;
    for offset in 0..=text.len() {
        if !text.is_char_boundary(offset) {
            continue;
        }
        let drawn = projection.drawn_offset(offset)?;
        let source = projection.source_at(&drawn, Affinity::Before)?;
        assert_eq!(source.value().byte_offset(), offset);
        let back = projection.drawn_at(source.value(), Affinity::Before)?;
        assert_eq!(back.value().byte_offset(), offset);
        assert_eq!(back.precision(), MappingPrecision::Exact);
    }
    assert!(matches!(
        projection.drawn_offset(2),
        Err(MappingError::NotCharBoundary)
    ));
    assert!(matches!(
        projection.drawn_offset(text.len() + 1),
        Err(MappingError::OutOfBounds { .. })
    ));
    Ok(())
}

#[test]
fn utf16_offsets_reject_surrogate_middles() -> Result<(), Box<dyn Error>> {
    let text = "a中😀e\u{301}";
    let (snapshot, projection) = identity(text)?;
    let expected = [(0, 0), (1, 1), (4, 2), (8, 4), (9, 5), (11, 6)];
    for (byte, unit) in expected {
        let utf16 = projection.utf16_offset(unit)?;
        assert_eq!(utf16.code_units(), unit);
        let mapped = projection.source_from_utf16(&utf16, Affinity::Before)?;
        assert_eq!(mapped.value().byte_offset(), byte);
        let from_source =
            projection.utf16_from_source(&snapshot.src_offset(byte)?, Affinity::Before)?;
        assert_eq!(from_source.value().code_units(), unit);
    }
    assert!(matches!(
        projection.utf16_offset(3),
        Err(MappingError::MiddleSurrogate)
    ));
    assert!(matches!(
        projection.utf16_offset(7),
        Err(MappingError::OutOfBounds { .. })
    ));
    Ok(())
}

#[test]
fn entities_and_escapes_map_as_aligned_segments() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("a &amp; b\n")?;
    let projection = parsed.projection();
    assert_eq!(projection.drawn_text(), "a & b");
    let segments = projection.segments();
    assert_eq!(segments.len(), 4);
    assert_eq!(
        segments.last().map(|segment| segment.precision()),
        Some(MappingPrecision::Hidden)
    );
    assert_eq!(
        segments
            .last()
            .map(|segment| segment.source_range().byte_range()),
        Some(9..10)
    );
    let entity = segments.get(1).ok_or("missing entity segment")?;
    assert_eq!(entity.precision(), MappingPrecision::Aligned);
    assert_eq!(entity.drawn_range(), 2..3);
    assert_eq!(entity.source_range().byte_range(), 2..7);
    let before = projection.source_at(&projection.drawn_offset(2)?, Affinity::Before)?;
    assert_eq!(before.value().byte_offset(), 2);
    assert_eq!(before.source_range().byte_range(), 2..7);
    let after = projection.source_at(&projection.drawn_offset(3)?, Affinity::After)?;
    assert_eq!(after.value().byte_offset(), 7);
    let inside_before = projection.drawn_at(&parsed.snapshot().src_offset(4)?, Affinity::Before)?;
    assert_eq!(inside_before.value().byte_offset(), 2);
    let inside_after = projection.drawn_at(&parsed.snapshot().src_offset(4)?, Affinity::After)?;
    assert_eq!(inside_after.value().byte_offset(), 3);
    Ok(())
}

#[test]
fn multichar_and_numeric_entities_keep_drawn_width() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("&NotEqualTilde; &#x1F600;\n")?;
    let projection = parsed.projection();
    assert_eq!(projection.drawn_text(), "≂̸ 😀");
    let segments = projection.segments();
    assert_eq!(segments.len(), 4);
    assert_eq!(
        segments.first().map(|segment| segment.precision()),
        Some(MappingPrecision::Aligned)
    );
    assert_eq!(
        segments.first().map(|segment| segment.drawn_range()),
        Some(0..5)
    );
    let emoji = segments.get(2).ok_or("missing emoji segment")?;
    assert_eq!(emoji.drawn_range(), 6..10);
    assert_eq!(emoji.precision(), MappingPrecision::Aligned);
    let utf16 = projection.utf16_offset(3)?;
    let source = projection.source_from_utf16(&utf16, Affinity::Before)?;
    assert_eq!(source.value().byte_offset(), 16);
    let inside = projection.drawn_at(&parsed.snapshot().src_offset(21)?, Affinity::Before)?;
    assert_eq!(inside.value().byte_offset(), 6);
    let units =
        projection.utf16_from_source(&parsed.snapshot().src_offset(21)?, Affinity::Before)?;
    assert_eq!(units.value().code_units(), 3);
    Ok(())
}

#[test]
fn soft_and_hard_breaks_are_exact_or_aligned() -> Result<(), Box<dyn Error>> {
    let soft = parse_text("a\nb\n")?;
    assert_eq!(soft.projection().drawn_text(), "a\nb");
    let soft_break = soft
        .projection()
        .segments()
        .get(1)
        .ok_or("missing soft break")?;
    assert_eq!(soft_break.precision(), MappingPrecision::Exact);
    assert_eq!(soft_break.drawn_range(), 1..2);
    assert_eq!(soft_break.source_range().byte_range(), 1..2);

    let hard = parse_text("a  \nb\n")?;
    assert_eq!(hard.projection().drawn_text(), "a\nb");
    let hard_break = hard
        .projection()
        .segments()
        .get(1)
        .ok_or("missing hard break")?;
    assert_eq!(hard_break.precision(), MappingPrecision::Aligned);
    assert_eq!(hard_break.drawn_range(), 1..2);
    assert_eq!(hard_break.source_range().byte_range(), 1..4);
    let before = hard
        .projection()
        .source_at(&hard.projection().drawn_offset(1)?, Affinity::Before)?;
    assert_eq!(before.value().byte_offset(), 1);
    let after = hard
        .projection()
        .source_at(&hard.projection().drawn_offset(1)?, Affinity::After)?;
    assert_eq!(after.value().byte_offset(), 4);
    Ok(())
}

#[test]
fn br_tag_produces_synthetic_newline() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("a<br>b\n")?;
    let projection = parsed.projection();
    assert_eq!(projection.drawn_text(), "a\nb");
    let newline = projection.segments().get(1).ok_or("missing br newline")?;
    assert_eq!(newline.precision(), MappingPrecision::Synthetic);
    assert_eq!(newline.drawn_range(), 1..2);
    assert_eq!(newline.source_range().byte_range(), 1..1);
    let mapped = projection.source_at(&projection.drawn_offset(1)?, Affinity::Before)?;
    assert_eq!(mapped.value().byte_offset(), 1);
    Ok(())
}

#[test]
fn hidden_and_synthetic_segments_anchor_without_pretending_bijection() -> Result<(), Box<dyn Error>>
{
    let source = "abcdef";
    let snapshot = snapshot(source)?;
    let range = snapshot.src_range(0..6)?;
    let segments = vec![
        MappingSegment::new(0..0, snapshot.src_range(0..3)?, MappingPrecision::Hidden)?,
        MappingSegment::new(0..1, snapshot.src_range(3..3)?, MappingPrecision::Synthetic)?,
        MappingSegment::new(1..2, snapshot.src_range(3..6)?, MappingPrecision::Aligned)?,
    ];
    let projection = Projection::from_segments(
        snapshot.clone(),
        range,
        "Xc".to_string(),
        segments,
        MappingBudget::new(BUDGET, BUDGET),
    )?;
    let hidden = projection.source_at(&projection.drawn_offset(0)?, Affinity::Before)?;
    assert_eq!(hidden.precision(), MappingPrecision::Synthetic);
    assert_eq!(hidden.value().byte_offset(), 3);
    let before = projection.source_at(&projection.drawn_offset(1)?, Affinity::Before)?;
    assert_eq!(before.precision(), MappingPrecision::Aligned);
    assert_eq!(before.value().byte_offset(), 3);
    let after = projection.source_at(&projection.drawn_offset(2)?, Affinity::After)?;
    assert_eq!(after.value().byte_offset(), 6);
    let hidden_anchor = projection.drawn_at(&snapshot.src_offset(0)?, Affinity::Before)?;
    assert_eq!(hidden_anchor.precision(), MappingPrecision::Hidden);
    assert_eq!(hidden_anchor.value().byte_offset(), 0);
    let synthetic_anchor = projection.drawn_at(&snapshot.src_offset(3)?, Affinity::Before)?;
    assert_eq!(synthetic_anchor.precision(), MappingPrecision::Synthetic);
    assert_eq!(synthetic_anchor.value().byte_offset(), 0);
    Ok(())
}

#[test]
fn local_projection_rejects_sources_outside_its_range() -> Result<(), Box<dyn Error>> {
    let text = "abcdef";
    let snapshot = snapshot(text)?;
    let range = snapshot.src_range(2..5)?;
    let segments = vec![MappingSegment::new(
        0..3,
        snapshot.src_range(2..5)?,
        MappingPrecision::Exact,
    )?];
    let projection = Projection::from_segments(
        snapshot.clone(),
        range,
        "bcd".to_string(),
        segments,
        MappingBudget::new(BUDGET, BUDGET),
    )?;
    let start = projection.source_at(&projection.drawn_offset(0)?, Affinity::Before)?;
    assert_eq!(start.value().byte_offset(), 2);
    assert!(matches!(
        projection.drawn_at(&snapshot.src_offset(1)?, Affinity::Before),
        Err(MappingError::SourceOutsideProjection)
    ));
    assert!(matches!(
        projection.drawn_at(&snapshot.src_offset(6)?, Affinity::Before),
        Err(MappingError::SourceOutsideProjection)
    ));
    Ok(())
}

#[test]
fn selections_preserve_direction_and_entity_alignment() -> Result<(), Box<dyn Error>> {
    let (snapshot, projection) = identity("abcdef")?;
    let reversed = SrcSelection::new(snapshot.src_offset(4)?, snapshot.src_offset(1)?)?;
    let aligned = projection.aligned(&reversed)?;
    assert!(aligned.is_reversed());
    assert_eq!(aligned.anchor().byte_offset(), 4);
    assert_eq!(aligned.head().byte_offset(), 1);
    let forward = SrcSelection::new(snapshot.src_offset(1)?, snapshot.src_offset(4)?)?;
    let aligned = projection.aligned(&forward)?;
    assert!(!aligned.is_reversed());
    assert_eq!(aligned.anchor().byte_offset(), 1);
    assert_eq!(aligned.head().byte_offset(), 4);

    let parsed = parse_text("a &amp; b\n")?;
    let selection = SrcSelection::new(
        parsed.snapshot().src_offset(2)?,
        parsed.snapshot().src_offset(7)?,
    )?;
    let aligned = parsed.projection().aligned(&selection)?;
    assert_eq!(aligned.anchor().byte_offset(), 2);
    assert_eq!(aligned.head().byte_offset(), 7);
    Ok(())
}

#[test]
fn stale_projection_and_snapshot_are_rejected() -> Result<(), Box<dyn Error>> {
    let (_snapshot, first) = identity("abc")?;
    let (_snapshot, second) = identity("abc")?;
    assert!(matches!(
        second.source_at(&first.drawn_offset(0)?, Affinity::Before),
        Err(MappingError::ProjectionMismatch)
    ));
    let parsed_a = parse_text("# 标题\n")?;
    let parsed_b = parse_text("# 标题\n")?;
    let range = parsed_a
        .blocks()
        .first()
        .ok_or("missing block")?
        .range()
        .clone();
    assert!(matches!(
        parsed_b.validate_range(&range),
        Err(aline_markdown::ParseError::Source(
            BufferError::SnapshotMismatch
        ))
    ));
    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
struct ExpectedSegment {
    drawn: Range<usize>,
    source: Range<usize>,
    precision: MappingPrecision,
}

#[derive(Debug, Eq, PartialEq)]
struct ExpectedAnchor {
    drawn: usize,
    affinity: Affinity,
    source: usize,
}

#[derive(Debug, Default)]
struct GoldenCase {
    source: Option<String>,
    drawn: Option<String>,
    segments: Vec<ExpectedSegment>,
    anchors: Vec<ExpectedAnchor>,
}

fn decode_quoted(value: &str) -> Result<String, Box<dyn Error>> {
    let inner = value
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .ok_or("golden value is not quoted")?;
    let mut decoded = String::new();
    let mut chars = inner.chars();
    while let Some(character) = chars.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        let escaped = chars.next().ok_or("trailing backslash in golden value")?;
        match escaped {
            'n' => decoded.push('\n'),
            'r' => decoded.push('\r'),
            't' => decoded.push('\t'),
            '\\' => decoded.push('\\'),
            '"' => decoded.push('"'),
            other => return Err(format!("unsupported golden escape: \\{other}").into()),
        }
    }
    Ok(decoded)
}

fn parse_range(value: &str) -> Result<Range<usize>, Box<dyn Error>> {
    let (start, end) = value.split_once("..").ok_or("range without ..")?;
    Ok(start.parse::<usize>()?..end.parse::<usize>()?)
}

fn parse_precision(value: &str) -> Result<MappingPrecision, Box<dyn Error>> {
    match value {
        "Exact" => Ok(MappingPrecision::Exact),
        "Aligned" => Ok(MappingPrecision::Aligned),
        "Hidden" => Ok(MappingPrecision::Hidden),
        "Synthetic" => Ok(MappingPrecision::Synthetic),
        other => Err(format!("unknown precision: {other}").into()),
    }
}

fn parse_affinity(value: &str) -> Result<Affinity, Box<dyn Error>> {
    match value {
        "Before" => Ok(Affinity::Before),
        "After" => Ok(Affinity::After),
        other => Err(format!("unknown affinity: {other}").into()),
    }
}

fn parse_mapping_goldens(tsv: &str) -> Result<BTreeMap<String, GoldenCase>, Box<dyn Error>> {
    let mut cases: BTreeMap<String, GoldenCase> = BTreeMap::new();
    for line in tsv.lines() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let columns: Vec<&str> = line.split('\t').collect();
        if columns.first().copied() == Some("case") {
            if columns.len() != 4 || columns.get(2).copied() != Some("field") {
                return Err(format!("malformed golden header: {line}").into());
            }
            continue;
        }
        let name = columns
            .first()
            .copied()
            .ok_or("golden row without case name")?;
        let field = columns.get(1).copied().ok_or("golden row without field")?;
        let case = cases.entry(name.to_string()).or_default();
        match field {
            "source" | "drawn" => {
                if columns.len() != 3 {
                    return Err(format!("malformed {field} row: {line}").into());
                }
                let value = decode_quoted(columns.get(2).copied().ok_or("golden value missing")?)?;
                if field == "source" {
                    case.source = Some(value);
                } else {
                    case.drawn = Some(value);
                }
            }
            "segment" => {
                if columns.len() != 5 {
                    return Err(format!("malformed segment row: {line}").into());
                }
                case.segments.push(ExpectedSegment {
                    drawn: parse_range(columns.get(2).copied().ok_or("segment drawn missing")?)?,
                    source: parse_range(columns.get(3).copied().ok_or("segment source missing")?)?,
                    precision: parse_precision(
                        columns.get(4).copied().ok_or("segment precision missing")?,
                    )?,
                });
            }
            "anchor" => {
                if columns.len() != 5 {
                    return Err(format!("malformed anchor row: {line}").into());
                }
                case.anchors.push(ExpectedAnchor {
                    drawn: columns
                        .get(2)
                        .copied()
                        .ok_or("anchor drawn missing")?
                        .parse::<usize>()?,
                    affinity: parse_affinity(
                        columns.get(3).copied().ok_or("anchor affinity missing")?,
                    )?,
                    source: columns
                        .get(4)
                        .copied()
                        .ok_or("anchor source missing")?
                        .parse::<usize>()?,
                });
            }
            other => return Err(format!("unknown golden field: {other}").into()),
        }
    }
    Ok(cases)
}

#[test]
fn mapping_goldens_tsv_matches_parser_output() -> Result<(), Box<dyn Error>> {
    let tsv = include_str!(
        "../../../docs/validation/m0-engine-foundation/group-6/step-6.4/goldens/mapping-goldens.tsv"
    );
    let cases = parse_mapping_goldens(tsv)?;
    assert_eq!(cases.len(), 6);
    for (name, case) in &cases {
        let source = case
            .source
            .as_deref()
            .ok_or_else(|| format!("case {name} missing source"))?;
        let drawn = case
            .drawn
            .as_deref()
            .ok_or_else(|| format!("case {name} missing drawn"))?;
        let parsed = parse_text(source)?;
        let projection = parsed.projection();
        assert_eq!(projection.drawn_text(), drawn, "case {name} drawn");
        let actual: Vec<ExpectedSegment> = projection
            .segments()
            .iter()
            .map(|segment| ExpectedSegment {
                drawn: segment.drawn_range(),
                source: segment.source_range().byte_range(),
                precision: segment.precision(),
            })
            .collect();
        assert_eq!(actual, case.segments, "case {name} segments");
        for anchor in &case.anchors {
            let drawn_offset = projection.drawn_offset(anchor.drawn)?;
            let mapped = projection.source_at(&drawn_offset, anchor.affinity)?;
            assert_eq!(
                mapped.value().byte_offset(),
                anchor.source,
                "case {name} anchor {} {:?}",
                anchor.drawn,
                anchor.affinity
            );
        }
    }
    Ok(())
}

#[test]
fn zwj_family_emoji_stays_exact_before_hidden_newline() -> Result<(), Box<dyn Error>> {
    let source = "👨\u{200d}👩\u{200d}👧\n";
    assert_eq!(source.len(), 19);
    let parsed = parse_text(source)?;
    let projection = parsed.projection();
    assert_eq!(projection.drawn_text(), "👨\u{200d}👩\u{200d}👧");
    assert_eq!(projection.drawn_text().len(), 18);
    let segments = projection.segments();
    assert_eq!(segments.len(), 2);
    let emoji = segments.first().ok_or("missing emoji segment")?;
    assert_eq!(emoji.drawn_range(), 0..18);
    assert_eq!(emoji.source_range().byte_range(), 0..18);
    assert_eq!(emoji.precision(), MappingPrecision::Exact);
    let hidden = segments.get(1).ok_or("missing hidden newline")?;
    assert_eq!(hidden.drawn_range(), 18..18);
    assert_eq!(hidden.source_range().byte_range(), 18..19);
    assert_eq!(hidden.precision(), MappingPrecision::Hidden);
    let end = projection.utf16_offset(8)?;
    let mapped = projection.source_from_utf16(&end, Affinity::Before)?;
    assert_eq!(mapped.value().byte_offset(), 18);
    assert!(matches!(
        projection.utf16_offset(4),
        Err(MappingError::MiddleSurrogate)
    ));
    assert!(matches!(
        projection.utf16_offset(9),
        Err(MappingError::OutOfBounds { .. })
    ));
    Ok(())
}

#[test]
fn repeated_text_keeps_distinct_segments_and_canonical_anchors() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("**a** **a**\n")?;
    let projection = parsed.projection();
    assert_eq!(projection.drawn_text(), "a a");
    let actual: Vec<ExpectedSegment> = projection
        .segments()
        .iter()
        .map(|segment| ExpectedSegment {
            drawn: segment.drawn_range(),
            source: segment.source_range().byte_range(),
            precision: segment.precision(),
        })
        .collect();
    assert_eq!(
        actual,
        vec![
            ExpectedSegment {
                drawn: 0..0,
                source: 0..2,
                precision: MappingPrecision::Hidden,
            },
            ExpectedSegment {
                drawn: 0..1,
                source: 2..3,
                precision: MappingPrecision::Exact,
            },
            ExpectedSegment {
                drawn: 1..1,
                source: 3..5,
                precision: MappingPrecision::Hidden,
            },
            ExpectedSegment {
                drawn: 1..2,
                source: 5..6,
                precision: MappingPrecision::Exact,
            },
            ExpectedSegment {
                drawn: 2..2,
                source: 6..8,
                precision: MappingPrecision::Hidden,
            },
            ExpectedSegment {
                drawn: 2..3,
                source: 8..9,
                precision: MappingPrecision::Exact,
            },
            ExpectedSegment {
                drawn: 3..3,
                source: 9..12,
                precision: MappingPrecision::Hidden,
            },
        ]
    );
    for (offset, expected) in [(0usize, 2usize), (1, 5), (2, 8), (3, 9)] {
        let mapped = projection.source_at(&projection.drawn_offset(offset)?, Affinity::Before)?;
        assert_eq!(mapped.value().byte_offset(), expected, "anchor {offset}");
    }
    let end = projection.source_at(&projection.drawn_offset(3)?, Affinity::After)?;
    assert_eq!(end.value().byte_offset(), 9);
    Ok(())
}

#[test]
fn consecutive_spaces_stay_exact_and_trailing_newline_hidden() -> Result<(), Box<dyn Error>> {
    let parsed = parse_text("a  b\n")?;
    let projection = parsed.projection();
    assert_eq!(projection.drawn_text(), "a  b");
    let segments = projection.segments();
    assert_eq!(segments.len(), 2);
    let text = segments.first().ok_or("missing text segment")?;
    assert_eq!(text.drawn_range(), 0..4);
    assert_eq!(text.source_range().byte_range(), 0..4);
    assert_eq!(text.precision(), MappingPrecision::Exact);
    let hidden = segments.get(1).ok_or("missing hidden newline")?;
    assert_eq!(hidden.drawn_range(), 4..4);
    assert_eq!(hidden.source_range().byte_range(), 4..5);
    assert_eq!(hidden.precision(), MappingPrecision::Hidden);
    Ok(())
}

#[test]
fn tab_and_combining_character_keep_utf8_boundaries() -> Result<(), Box<dyn Error>> {
    let tabbed = parse_text("a\tb\n")?;
    let projection = tabbed.projection();
    assert_eq!(projection.drawn_text(), "a\tb");
    let segments = projection.segments();
    assert_eq!(segments.len(), 2);
    let text = segments.first().ok_or("missing tab text")?;
    assert_eq!(text.drawn_range(), 0..3);
    assert_eq!(text.source_range().byte_range(), 0..3);
    assert_eq!(text.precision(), MappingPrecision::Exact);
    assert_eq!(
        segments
            .get(1)
            .map(|segment| segment.source_range().byte_range()),
        Some(3..4)
    );
    assert!(projection.drawn_offset(1).is_ok());
    assert!(projection.drawn_offset(2).is_ok());
    assert!(matches!(
        projection.drawn_offset(4),
        Err(MappingError::OutOfBounds { .. })
    ));

    let combining_source = "e\u{301}\n";
    assert_eq!(combining_source.len(), 4);
    let combined = parse_text(combining_source)?;
    let projection = combined.projection();
    assert_eq!(projection.drawn_text(), "e\u{301}");
    let text = projection
        .segments()
        .first()
        .ok_or("missing combining text")?;
    assert_eq!(text.drawn_range(), 0..3);
    assert_eq!(text.source_range().byte_range(), 0..3);
    assert_eq!(text.precision(), MappingPrecision::Exact);
    assert!(matches!(
        projection.drawn_offset(2),
        Err(MappingError::NotCharBoundary)
    ));
    let unit = projection.utf16_offset(2)?;
    let mapped = projection.source_from_utf16(&unit, Affinity::Before)?;
    assert_eq!(mapped.value().byte_offset(), 3);
    Ok(())
}

#[test]
fn local_projection_combines_utf16_offsets_with_source_ranges() -> Result<(), Box<dyn Error>> {
    let text = "a😀中b";
    assert_eq!(text.len(), 9);
    let snapshot = snapshot(text)?;
    let range = snapshot.src_range(1..8)?;
    let segments = vec![MappingSegment::new(
        0..7,
        snapshot.src_range(1..8)?,
        MappingPrecision::Exact,
    )?];
    let projection = Projection::from_segments(
        snapshot.clone(),
        range,
        "😀中".to_string(),
        segments,
        MappingBudget::new(BUDGET, BUDGET),
    )?;
    assert_eq!(projection.drawn_text(), "😀中");
    let start = projection.utf16_offset(0)?;
    assert_eq!(
        projection
            .source_from_utf16(&start, Affinity::Before)?
            .value()
            .byte_offset(),
        1
    );
    let after_emoji = projection.utf16_offset(2)?;
    assert_eq!(
        projection
            .source_from_utf16(&after_emoji, Affinity::Before)?
            .value()
            .byte_offset(),
        5
    );
    let end = projection.utf16_offset(3)?;
    assert_eq!(
        projection
            .source_from_utf16(&end, Affinity::Before)?
            .value()
            .byte_offset(),
        8
    );
    assert!(matches!(
        projection.utf16_offset(1),
        Err(MappingError::MiddleSurrogate)
    ));
    assert!(matches!(
        projection.utf16_offset(4),
        Err(MappingError::OutOfBounds { .. })
    ));
    let units = projection.utf16_from_source(&snapshot.src_offset(5)?, Affinity::Before)?;
    assert_eq!(units.value().code_units(), 2);
    assert!(matches!(
        projection.utf16_from_source(&snapshot.src_offset(0)?, Affinity::Before),
        Err(MappingError::SourceOutsideProjection)
    ));
    assert!(matches!(
        projection.drawn_at(&snapshot.src_offset(9)?, Affinity::Before),
        Err(MappingError::SourceOutsideProjection)
    ));
    Ok(())
}
