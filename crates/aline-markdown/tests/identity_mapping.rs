//! 局部/全文/UTF-16 恒等转换的独立来源金样；复杂 Markdown 映射留后续任务。

use aline_buffer::{Affinity, BufferError, SrcSelection, TextBudget, TextSnapshot};
use aline_markdown::{MappingBudget, MappingError, MappingPrecision, Projection};

#[test]
fn identity_mapping_converts_local_global_and_utf16_through_one_service()
-> Result<(), Box<dyn std::error::Error>> {
    let snapshot = TextSnapshot::new(String::from("前甲😀后"), TextBudget::new(64))?;
    let local = Projection::identity(
        snapshot.clone(),
        snapshot.src_range(3..10)?,
        MappingBudget::new(64, 16),
    )?;
    let whole = Projection::identity(
        snapshot.clone(),
        snapshot.src_range(0..13)?,
        MappingBudget::new(64, 16),
    )?;
    let origin = local.source_at(&local.drawn_offset(0)?, Affinity::Before)?;
    assert_eq!(origin.value().byte_offset(), 3);
    assert_eq!(origin.precision(), MappingPrecision::Exact);
    assert_eq!(origin.affinity(), Affinity::Before);
    assert_eq!(origin.source_range().byte_range(), 3..3);
    let end = local.source_from_utf16(&local.utf16_offset(3)?, Affinity::After)?;
    assert_eq!(end.value().byte_offset(), 10);
    assert_eq!(
        whole
            .utf16_from_source(end.value(), Affinity::After)?
            .value()
            .code_units(),
        4
    );
    for byte in [3, 6, 10] {
        let source = snapshot.src_offset(byte)?;
        let drawn = local.drawn_at(&source, Affinity::Before)?.into_value();
        assert_eq!(local.source_at(&drawn, Affinity::Before)?.value(), &source);
    }
    let selection = SrcSelection::new(snapshot.src_offset(10)?, snapshot.src_offset(3)?)?;
    assert!(selection.is_reversed());
    let aligned = local.aligned(&selection)?;
    assert_eq!(aligned, selection);
    assert_eq!(local.aligned(&aligned)?, aligned);
    assert!(matches!(
        local.drawn_at(&snapshot.src_offset(0)?, Affinity::Before),
        Err(MappingError::SourceOutsideProjection)
    ));
    let crossing = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(13)?)?;
    assert!(matches!(
        local.aligned(&crossing),
        Err(MappingError::SourceOutsideProjection)
    ));
    Ok(())
}

#[test]
fn identity_mapping_refuses_foreign_projection_snapshot_and_invalid_units()
-> Result<(), Box<dyn std::error::Error>> {
    let source = TextSnapshot::new(String::from("甲😀"), TextBudget::new(64))?;
    let first = Projection::identity(
        source.clone(),
        source.src_range(0..7)?,
        MappingBudget::new(64, 16),
    )?;
    let second = Projection::identity(
        source.clone(),
        source.src_range(0..7)?,
        MappingBudget::new(64, 16),
    )?;
    assert!(matches!(
        first.source_at(&second.drawn_offset(0)?, Affinity::Before),
        Err(MappingError::ProjectionMismatch)
    ));
    assert!(matches!(
        first.source_from_utf16(&second.utf16_offset(0)?, Affinity::Before),
        Err(MappingError::ProjectionMismatch)
    ));
    let old = source.src_offset(3)?;
    let replacement = TextSnapshot::new(String::from("甲😀"), TextBudget::new(64))?;
    let current = Projection::identity(
        replacement.clone(),
        replacement.src_range(0..7)?,
        MappingBudget::new(64, 16),
    )?;
    assert!(matches!(
        current.drawn_at(&old, Affinity::Before),
        Err(MappingError::Source(BufferError::SnapshotMismatch))
    ));
    assert!(matches!(
        first.utf16_offset(2),
        Err(MappingError::MiddleSurrogate)
    ));
    assert!(matches!(
        first.drawn_offset(4),
        Err(MappingError::NotCharBoundary)
    ));
    assert!(matches!(
        first.drawn_offset(8),
        Err(MappingError::OutOfBounds { .. })
    ));
    Ok(())
}
