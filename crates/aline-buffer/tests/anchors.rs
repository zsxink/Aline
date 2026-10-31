//! 组 5 锚点 affinity 金样：替换、插入、多补丁与方向性选区。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{
    Affinity, AnchorTransform, BufferError, CheckedBatch, PatchBudget, SrcOffset, TextBudget,
    TextSnapshot, transform_anchor, transform_selection,
};

const CAPACITY: usize = 256;

#[test]
fn anchors_transform_replacement_with_before_and_after_affinity() -> Result<(), Box<dyn Error>> {
    let before = snapshot("aXYZb")?;
    let after = snapshot("aQb")?;
    let batch = batch(&before, vec![(1..4, String::from("Q"))])?;
    for affinity in [Affinity::Before, Affinity::After] {
        assert_valid(&before, &batch, &after, 0, affinity, 0)?;
        assert_valid(&before, &batch, &after, 5, affinity, 3)?;
        assert_valid(&before, &batch, &after, 4, affinity, 2)?;
    }
    assert_valid(&before, &batch, &after, 1, Affinity::Before, 1)?;
    assert_valid(&before, &batch, &after, 1, Affinity::After, 2)?;
    for offset in [2, 3] {
        assert_eq!(
            transform_anchor(
                &before,
                &before.src_offset(offset)?,
                &batch,
                Affinity::Before
            )?,
            AnchorTransform::Invalidated
        );
    }
    Ok(())
}

#[test]
fn anchors_transform_insertions_and_adjacent_replacement_start() -> Result<(), Box<dyn Error>> {
    let before = snapshot("abcd")?;
    let after = snapshot("abXcd")?;
    let insert_batch = batch(&before, vec![(2..2, String::from("X"))])?;
    assert_valid(&before, &insert_batch, &after, 1, Affinity::Before, 1)?;
    assert_valid(&before, &insert_batch, &after, 2, Affinity::Before, 2)?;
    assert_valid(&before, &insert_batch, &after, 2, Affinity::After, 3)?;
    assert_valid(&before, &insert_batch, &after, 3, Affinity::After, 4)?;
    assert_valid(&before, &insert_batch, &after, 4, Affinity::After, 5)?;

    let mixed = batch(
        &before,
        vec![(2..2, String::from("X")), (2..4, String::from("Y"))],
    )?;
    let mixed_after = snapshot("abXY")?;
    assert_valid(&before, &mixed, &mixed_after, 2, Affinity::Before, 2)?;
    assert_valid(&before, &mixed, &mixed_after, 2, Affinity::After, 4)?;
    assert_eq!(
        transform_anchor(&before, &before.src_offset(3)?, &mixed, Affinity::After)?,
        AnchorTransform::Invalidated
    );
    assert_valid(&before, &mixed, &mixed_after, 4, Affinity::After, 4)?;
    Ok(())
}

#[test]
fn anchors_transform_multiple_patches_by_cumulative_delta() -> Result<(), Box<dyn Error>> {
    let before = snapshot("0123456789")?;
    let after = snapshot("0AB34589")?;
    let batch = batch(
        &before,
        vec![(1..3, String::from("AB")), (6..8, String::new())],
    )?;
    let cases = [(0, 0), (3, 3), (4, 4), (5, 5), (6, 6), (9, 7), (10, 8)];
    for (source, target) in cases {
        assert_valid(&before, &batch, &after, source, Affinity::Before, target)?;
        assert_valid(&before, &batch, &after, source, Affinity::After, target)?;
    }
    assert_valid(&before, &batch, &after, 1, Affinity::Before, 1)?;
    assert_valid(&before, &batch, &after, 1, Affinity::After, 3)?;
    for offset in [2, 7] {
        assert_eq!(
            transform_anchor(
                &before,
                &before.src_offset(offset)?,
                &batch,
                Affinity::Before
            )?,
            AnchorTransform::Invalidated
        );
    }
    Ok(())
}

#[test]
fn anchors_transform_selection_preserves_direction_and_rejects_partial_validity()
-> Result<(), Box<dyn Error>> {
    let before = snapshot("aXYZb")?;
    let after = snapshot("aQb")?;
    let batch = batch(&before, vec![(1..4, String::from("Q"))])?;
    let selection = aline_buffer::SrcSelection::new(before.src_offset(5)?, before.src_offset(1)?)?;
    let transformed = transform_selection(&before, &selection, &batch, Affinity::Before)?
        .ok_or_else(|| "两端都应有效".to_string())?;
    assert_eq!(transformed.anchor(), 3);
    assert_eq!(transformed.head(), 1);
    assert!(transformed.is_reversed());
    assert!(after.src_offset(transformed.anchor()).is_ok());
    assert!(after.src_offset(transformed.head()).is_ok());

    let through_replacement =
        aline_buffer::SrcSelection::new(before.src_offset(5)?, before.src_offset(1)?)?;
    let transformed = transform_selection(&before, &through_replacement, &batch, Affinity::After)?
        .ok_or_else(|| "两端都应有效".to_string())?;
    assert_eq!(transformed.anchor(), 3);
    assert_eq!(transformed.head(), 2);

    let invalid = aline_buffer::SrcSelection::new(before.src_offset(5)?, before.src_offset(2)?)?;
    assert!(transform_selection(&before, &invalid, &batch, Affinity::Before)?.is_none());
    Ok(())
}

#[test]
fn anchors_reject_foreign_snapshot_and_stale_batch() -> Result<(), Box<dyn Error>> {
    let before = snapshot("abc")?;
    let other = snapshot("abc")?;
    let stale_batch = batch(&before, vec![(1..1, String::from("X"))])?;
    let other_anchor = other.src_offset(1)?;
    assert!(matches!(
        transform_anchor(&before, &other_anchor, &stale_batch, Affinity::Before),
        Err(BufferError::SnapshotMismatch)
    ));
    let foreign_batch = batch(&other, vec![(1..1, String::from("Y"))])?;
    let anchor = before.src_offset(1)?;
    assert!(matches!(
        transform_anchor(&before, &anchor, &foreign_batch, Affinity::Before),
        Err(BufferError::SnapshotMismatch)
    ));
    let selection = aline_buffer::SrcSelection::new(before.src_offset(3)?, before.src_offset(3)?)?;
    assert!(matches!(
        transform_selection(&before, &selection, &foreign_batch, Affinity::Before),
        Err(BufferError::SnapshotMismatch)
    ));
    Ok(())
}

fn snapshot(text: &str) -> Result<TextSnapshot, Box<dyn Error>> {
    Ok(TextSnapshot::new(
        text.to_string(),
        TextBudget::new(CAPACITY),
    )?)
}

fn batch(
    before: &TextSnapshot,
    patches: Vec<(Range<usize>, String)>,
) -> Result<CheckedBatch, Box<dyn Error>> {
    Ok(CheckedBatch::new(
        before,
        patches,
        PatchBudget::new(8, CAPACITY, CAPACITY),
    )?)
}

fn assert_valid(
    before: &TextSnapshot,
    batch: &CheckedBatch,
    after: &TextSnapshot,
    source: usize,
    affinity: Affinity,
    expected: usize,
) -> Result<(), Box<dyn Error>> {
    let anchor: SrcOffset = before.src_offset(source)?;
    let transformed = transform_anchor(before, &anchor, batch, affinity)?;
    assert_eq!(
        transformed,
        AnchorTransform::Valid { byte: expected },
        "before {source} with {affinity:?}"
    );
    assert!(
        after.src_offset(expected).is_ok(),
        "映射结果必须是 after 的 UTF-8 边界"
    );
    Ok(())
}
