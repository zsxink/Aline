//! 任务 3.1 独立外部消费夹具；不属于正式八成员 workspace，不启动线程或窗口。

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use aline_buffer::{BufferError, DocumentRevision, SrcRange, TextBudget, TextSnapshot};
    use aline_markdown::{MappingBudget, MappingError, Projection};
    use aline_source::{DiskVersion, StorageBudget, StorageError, StorageSnapshot};
    use aline_ui::{DocumentId, DraftGeneration, IdentityError, InputIdentity, WorkingVersion};

    #[test]
    fn overlay_unicode_boundaries_and_old_draft_are_immutable()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = TextSnapshot::new(String::from("A甲B尾"), TextBudget::new(64))?;
        let replaced = base.src_range(1..4)?;
        let mut draft = Arc::new(String::from("😀\r\n"));
        let old = TextSnapshot::with_overlay(&base, replaced.clone(), draft.clone(), TextBudget::new(64))?;
        let full = old.src_range(0..11)?;
        let crossing = old.src_range(1..8)?;
        assert_eq!(old.slice(&full, TextBudget::new(64))?, "A😀\r\nB尾");
        assert_eq!(old.slice(&crossing, TextBudget::new(64))?, "😀\r\nB");
        assert_ne!(old.id(), base.id());
        for byte in 0..=11 {
            let expected = [0, 1, 5, 6, 7, 8, 11].contains(&byte);
            assert_eq!(old.src_offset(byte).is_ok(), expected);
        }
        Arc::make_mut(&mut draft).push('X');
        assert_eq!(old.slice(&full, TextBudget::new(64))?, "A😀\r\nB尾");
        let newer = TextSnapshot::with_overlay(&base, replaced, draft, TextBudget::new(64))?;
        assert_ne!(old.id(), newer.id());
        assert!(matches!(newer.validate_range(&full), Err(BufferError::SnapshotMismatch)));
        let contiguous = old.materialize(TextBudget::new(64))?;
        assert_eq!(contiguous.id(), old.id());
        assert_eq!(contiguous.slice(&full, TextBudget::new(64))?, "A😀\r\nB尾");
        assert!(matches!(TextSnapshot::with_overlay(&old, full, Arc::new(String::new()), TextBudget::new(64)), Err(BufferError::NestedOverlay)));
        Ok(())
    }

    #[test]
    fn projection_construction_keeps_units_and_budget_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        let base = TextSnapshot::new(String::from("A甲B尾"), TextBudget::new(64))?;
        let source = TextSnapshot::with_overlay(&base, base.src_range(1..4)?, Arc::new(String::from("😀\r\n")), TextBudget::new(64))?;
        let range = source.src_range(1..8)?;
        let projection = Projection::identity(source.clone(), range.clone(), MappingBudget::new(7, 5))?;
        assert_eq!(projection.drawn_text(), "😀\r\nB");
        assert!(projection.drawn_offset(4).is_ok());
        assert!(matches!(projection.drawn_offset(1), Err(MappingError::NotCharBoundary)));
        assert!(projection.utf16_offset(5).is_ok());
        assert!(matches!(projection.utf16_offset(1), Err(MappingError::MiddleSurrogate)));
        assert!(matches!(Projection::identity(source, range, MappingBudget::new(7, 4)), Err(MappingError::BudgetExceeded)));
        Ok(())
    }

    #[test]
    fn storage_and_document_identities_do_not_follow_equal_values()
    -> Result<(), Box<dyn std::error::Error>> {
        let first = StorageSnapshot::new(vec![b'x'], DiskVersion::initial(), StorageBudget::new(8))?;
        let second = StorageSnapshot::new(vec![b'x'], DiskVersion::initial(), StorageBudget::new(8))?;
        assert_ne!(first.id(), second.id());
        assert!(matches!(second.slice(&first.range(0..1)?), Err(StorageError::BaselineMismatch)));
        let snapshot = TextSnapshot::new(String::from("x"), TextBudget::new(8))?;
        let version = WorkingVersion::new(DocumentRevision::initial(), DraftGeneration::initial());
        let first = InputIdentity::new(DocumentId::new(), version, &snapshot)?;
        let second = InputIdentity::new(DocumentId::new(), version, &snapshot)?;
        assert!(matches!(first.validate(&second), Err(IdentityError::WrongDocument)));
        fn require_send_sync<T: Send + Sync>() {}
        require_send_sync::<TextSnapshot>();
        require_send_sync::<SrcRange>();
        require_send_sync::<Projection>();
        require_send_sync::<StorageSnapshot>();
        require_send_sync::<InputIdentity>();
        Ok(())
    }

    #[test]
    fn checked_batches_keep_tuple_context_and_refuse_invalid_plans()
    -> Result<(), Box<dyn std::error::Error>> {
        use aline_buffer::{CheckedBatch, PatchBudget};
        let before = TextSnapshot::new(String::from("甲AB尾"), TextBudget::new(64))?;
        let tuples = vec![(3..4, String::from("😀")), (5..8, String::from("!"))];
        let batch = CheckedBatch::new(&before, tuples.clone(), PatchBudget::new(2, 5, 9))?;
        let typed: &[(std::ops::Range<usize>, String)] = batch.patches();
        assert_eq!(typed, tuples.as_slice());
        assert_eq!(batch.output_len(), 9);
        assert!(batch.validate(&before.clone()).is_ok());
        let other = TextSnapshot::new(String::from("甲AB尾"), TextBudget::new(64))?;
        assert!(matches!(batch.validate(&other), Err(BufferError::SnapshotMismatch)));
        assert!(matches!(CheckedBatch::new(&before, vec![(0..3, String::new()), (8..9, String::new())], PatchBudget::new(2, 64, 64)), Err(BufferError::OutOfBounds { .. })));
        assert!(matches!(CheckedBatch::new(&before, vec![(3..3, String::from("x")), (3..3, String::from("y"))], PatchBudget::new(2, 64, 64)), Err(BufferError::AmbiguousInsertion)));
        assert_eq!(before.slice(&before.src_range(0..8)?, TextBudget::new(64))?, "甲AB尾");
        Ok(())
    }

    #[test]
    fn local_identity_mapping_uses_checked_global_and_utf16_coordinates()
    -> Result<(), Box<dyn std::error::Error>> {
        use aline_buffer::{Affinity, SrcSelection};
        let snapshot = TextSnapshot::new(String::from("A甲😀尾"), TextBudget::new(64))?;
        let local = Projection::identity(snapshot.clone(), snapshot.src_range(1..8)?, MappingBudget::new(7, 3))?;
        let source = local.source_from_utf16(&local.utf16_offset(3)?, Affinity::After)?.into_value();
        assert_eq!(source.byte_offset(), 8);
        let drawn = local.drawn_at(&snapshot.src_offset(4)?, Affinity::Before)?.into_value();
        assert_eq!(drawn.byte_offset(), 3);
        assert_eq!(local.utf16_from_source(&snapshot.src_offset(4)?, Affinity::Before)?.value().code_units(), 1);
        let reverse = SrcSelection::new(snapshot.src_offset(8)?, snapshot.src_offset(1)?)?;
        assert_eq!(local.aligned(&reverse)?, reverse);
        assert!(matches!(local.drawn_at(&snapshot.src_offset(0)?, Affinity::Before), Err(MappingError::SourceOutsideProjection)));
        let foreign = Projection::identity(snapshot.clone(), snapshot.src_range(1..8)?, MappingBudget::new(7, 3))?;
        assert!(matches!(local.source_at(&foreign.drawn_offset(0)?, Affinity::Before), Err(MappingError::ProjectionMismatch)));
        let empty = Projection::identity(snapshot.clone(), snapshot.src_range(4..4)?, MappingBudget::new(0, 1))?;
        assert_eq!(empty.source_at(&empty.drawn_offset(0)?, Affinity::Before)?.value().byte_offset(), 4);
        assert_eq!(empty.utf16_from_source(&snapshot.src_offset(4)?, Affinity::After)?.value().code_units(), 0);
        Ok(())
    }

}
