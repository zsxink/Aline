//! Unicode17 手写字节金样；不以被测分段器生成预期。
use std::error::Error;

use aline_buffer::{Affinity, TextBudget, TextSnapshot};
use aline_markdown::{GraphemeError, MappingBudget, ParseBudget, ParseConfig, Projection, parse};

fn identity(text: &str) -> Result<Projection, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_owned(), TextBudget::new(1 << 20))?;
    let range = snapshot.src_range(0..text.len())?;
    Ok(Projection::identity(
        snapshot,
        range,
        MappingBudget::new(1 << 20, 1 << 20),
    )?)
}

fn check(text: &str, expected: &[usize]) -> Result<(), Box<dyn Error>> {
    let projection = identity(text)?;
    for byte in 0..=text.len() {
        if !text.is_char_boundary(byte) {
            continue;
        }
        let offset = projection.drawn_offset(byte)?;
        assert_eq!(
            projection.is_grapheme_boundary(&offset)?,
            expected.contains(&byte),
            "{byte}"
        );
        if !expected.contains(&byte) {
            assert!(matches!(
                projection.next_grapheme_boundary(&offset),
                Err(GraphemeError::NotGraphemeBoundary)
            ));
            assert!(matches!(
                projection.previous_grapheme_boundary(&offset),
                Err(GraphemeError::NotGraphemeBoundary)
            ));
        }
    }
    for (ordinal, &byte) in expected.iter().enumerate() {
        let offset = projection.drawn_offset(byte)?;
        assert_eq!(
            projection
                .next_grapheme_boundary(&offset)?
                .map(|value| value.byte_offset()),
            expected.get(ordinal + 1).copied()
        );
        assert_eq!(
            projection
                .previous_grapheme_boundary(&offset)?
                .map(|value| value.byte_offset()),
            ordinal
                .checked_sub(1)
                .and_then(|previous| expected.get(previous))
                .copied()
        );
    }
    Ok(())
}

#[test]
fn grapheme_goldens_move_both_directions_and_reject_scalar_interior() -> Result<(), Box<dyn Error>>
{
    // a/两个空格/中文/e+组合符/家庭25字节/国旗8字节，各自为一个簇。
    check("a  中e\u{301}👨‍👩‍👧‍👦🇨🇳", &[0, 1, 2, 3, 6, 9, 34, 42])?;
    check("👍🏽1\u{fe0f}\u{20e3}", &[0, 8, 15])?;
    check("\r\n", &[0, 2])?;
    check("\u{1100}\u{1161}\u{11a8}", &[0, 9])?;
    check("🇨🇳🇦", &[0, 8, 12])?;
    check("क्क", &[0, 9])?;
    check("", &[0])?;
    Ok(())
}

#[test]
fn unicode17_indic_version_delta_is_explicit() -> Result<(), Box<dyn Error>> {
    // 已批准的版本差异：Unicode17 [0,3,6]，Unicode18 GB9c 为 [0,6]。
    const UNICODE17: &[usize] = &[0, 3, 6];
    const UNICODE18: &[usize] = &[0, 6];
    assert_ne!(UNICODE17, UNICODE18);
    check("\u{94d}\u{915}", UNICODE17)
}

#[test]
fn scalar_utf16_and_grapheme_boundaries_are_distinct() -> Result<(), Box<dyn Error>> {
    let projection = identity("😀e\u{301}")?;
    assert!(projection.utf16_offset(1).is_err());
    let combining_start = projection.drawn_offset(5)?;
    assert!(!projection.is_grapheme_boundary(&combining_start)?);
    assert_eq!(projection.utf16_offset(3)?.code_units(), 3);
    assert_eq!(
        projection
            .next_grapheme_boundary(&projection.drawn_offset(4)?)?
            .map(|offset| offset.byte_offset()),
        Some(7)
    );
    Ok(())
}

#[test]
fn old_and_partial_projections_are_rejected() -> Result<(), Box<dyn Error>> {
    let first = identity("ab")?;
    let second = identity("ab")?;
    let stale = first.drawn_offset(0)?;
    assert!(matches!(
        second.next_grapheme_boundary(&stale),
        Err(GraphemeError::Mapping(
            aline_markdown::MappingError::ProjectionMismatch
        ))
    ));
    let snapshot = TextSnapshot::new("e\u{301}x".to_owned(), TextBudget::new(64))?;
    let range = snapshot.src_range(1..4)?;
    let partial = Projection::identity(snapshot, range, MappingBudget::new(64, 64))?;
    assert!(matches!(
        partial.is_grapheme_boundary(&partial.drawn_offset(0)?),
        Err(GraphemeError::IncompleteProjection)
    ));
    Ok(())
}

#[test]
fn visual_navigation_maps_back_only_through_projection() -> Result<(), Box<dyn Error>> {
    let snapshot = TextSnapshot::new("**e\u{301}** X\n".to_owned(), TextBudget::new(64))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let projection = parsed.projection();
    let next = projection
        .next_grapheme_boundary(&projection.drawn_offset(0)?)?
        .ok_or("missing boundary")?;
    assert_eq!(next.byte_offset(), 3);
    // 紧随可见簇的隐藏结束标记把 Before 锚点定位在源码标记之后。
    assert_eq!(
        projection
            .source_at(&next, Affinity::Before)?
            .value()
            .byte_offset(),
        7
    );
    Ok(())
}

fn budget_rejected(
    result: Result<Option<aline_markdown::DrawnOffset>, GraphemeError>,
) -> Result<(), Box<dyn Error>> {
    let (supplied_bytes, next_bytes, limit) = match result {
        Err(GraphemeError::ScanBudgetExceeded {
            supplied_bytes,
            next_bytes,
            limit,
        }) => (supplied_bytes, next_bytes, limit),
        other => {
            return Err(
                std::io::Error::other(format!("长上下文必须拒绝且不返回边界：{other:?}")).into(),
            );
        }
    };
    assert_eq!(limit, 4096);
    assert!(supplied_bytes <= limit);
    assert!(next_bytes <= 256);
    assert!(supplied_bytes + next_bytes > limit);
    Ok(())
}

#[test]
fn long_clusters_and_ri_context_refuse_within_supply_budget() -> Result<(), Box<dyn Error>> {
    let cluster = format!("a{}b", "\u{301}".repeat(5000));
    let projection = identity(&cluster)?;
    budget_rejected(projection.next_grapheme_boundary(&projection.drawn_offset(0)?))?;
    budget_rejected(
        projection.previous_grapheme_boundary(&projection.drawn_offset(cluster.len() - 1)?),
    )?;
    let regional = format!("{}x", "🇨".repeat(3000));
    let projection = identity(&regional)?;
    // 末端前的合法RI配对边界仍需超过预算的前文；不能猜奇偶。
    budget_rejected(projection.next_grapheme_boundary(&projection.drawn_offset(11992)?))?;
    budget_rejected(projection.previous_grapheme_boundary(&projection.drawn_offset(11992)?))?;
    Ok(())
}
