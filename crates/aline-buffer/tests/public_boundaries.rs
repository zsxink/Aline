//! 公共快照与坐标的外部消费金样；预期由 UTF-8 字节手工计算。

use std::sync::Arc;

use aline_buffer::{BufferError, DocumentRevision, SrcSelection, TextBudget, TextSnapshot};

#[test]
fn source_coordinates_keep_snapshot_identity_and_selection_direction()
-> Result<(), Box<dyn std::error::Error>> {
    let source = TextSnapshot::new(String::from("甲😀Z"), TextBudget::new(64))?;
    let cloned = source.clone();
    let other = TextSnapshot::new(String::from("甲😀Z"), TextBudget::new(64))?;
    let range = source.src_range(3..7)?;
    assert_eq!(cloned.slice(&range, TextBudget::new(64))?, "😀");
    assert!(matches!(
        other.slice(&range, TextBudget::new(64)),
        Err(BufferError::SnapshotMismatch)
    ));
    assert!(matches!(
        source.src_offset(1),
        Err(BufferError::NotCharBoundary { .. })
    ));
    assert!(matches!(
        source.src_range(std::ops::Range { start: 8, end: 7 }),
        Err(BufferError::ReversedRange)
    ));
    assert!(source.src_range(8..8).is_ok());
    let selection = SrcSelection::new(source.src_offset(8)?, source.src_offset(3)?)?;
    assert!(selection.is_reversed());
    assert_eq!(selection.range().byte_range(), 3..8);
    Ok(())
}

#[test]
fn single_overlay_shares_base_and_only_materializes_when_requested()
-> Result<(), Box<dyn std::error::Error>> {
    let base = TextSnapshot::new(String::from("甲-old-尾"), TextBudget::new(64))?;
    let draft = Arc::new(String::from("😀"));
    let overlay =
        TextSnapshot::with_overlay(&base, base.src_range(4..7)?, draft, TextBudget::new(64))?;
    let pieces: Vec<&str> = overlay.iter_chunks()?.collect();
    assert_eq!(pieces, vec!["甲-", "😀", "-尾"]);
    let full = overlay.src_range(0..overlay.len_bytes())?;
    assert_eq!(overlay.slice(&full, TextBudget::new(64))?, "甲-😀-尾");
    let materialized = overlay.materialize(TextBudget::new(64))?;
    assert_eq!(materialized.id(), overlay.id());
    assert_eq!(materialized.slice(&full, TextBudget::new(64))?, "甲-😀-尾");
    let first = overlay.iter_chunks()?.next().ok_or("缺少前缀片段")?;
    let base_first = base.iter_chunks()?.next().ok_or("缺少基线片段")?;
    assert_eq!(first.as_ptr(), base_first.as_ptr());
    assert!(matches!(
        TextSnapshot::with_overlay(&overlay, full, Arc::new(String::new()), TextBudget::new(64)),
        Err(BufferError::NestedOverlay)
    ));
    assert!(matches!(
        overlay.materialize(TextBudget::new(1)),
        Err(BufferError::BudgetExceeded { .. })
    ));
    assert!(matches!(
        DocumentRevision::from_value(u64::MAX).next(),
        Err(BufferError::RevisionExhausted)
    ));
    Ok(())
}
