//! tuple 补丁表示边界的外部金样，不在本阶段发布文本或历史。

use std::ops::Range;
use std::sync::Arc;

use aline_buffer::{BufferError, CheckedBatch, PatchBudget, TextBudget, TextSnapshot};

#[test]
fn checked_tuple_batch_keeps_original_shape_and_context() -> Result<(), Box<dyn std::error::Error>>
{
    let before = TextSnapshot::new(String::from("abcXYZ"), TextBudget::new(64))?;
    let patches: Vec<(Range<usize>, String)> =
        vec![(1..2, String::from("QQ")), (3..6, String::from("!"))];
    let batch = CheckedBatch::new(&before, patches, PatchBudget::new(8, 64, 64))?;
    let actual: &[(Range<usize>, String)] = batch.patches();
    assert_eq!(
        actual,
        &[(1..2, String::from("QQ")), (3..6, String::from("!"))]
    );
    assert_eq!(batch.output_len(), 5);
    assert!(batch.validate(&before.clone()).is_ok());
    let other = TextSnapshot::new(String::from("abcXYZ"), TextBudget::new(64))?;
    assert!(matches!(
        batch.validate(&other),
        Err(BufferError::SnapshotMismatch)
    ));
    let revised = TextSnapshot::from_shared(
        Arc::new(String::from("abcXYZ")),
        before.revision().next()?,
        TextBudget::new(64),
    )?;
    assert!(matches!(
        batch.validate(&revised),
        Err(BufferError::RevisionMismatch)
    ));
    Ok(())
}

#[test]
fn checked_tuple_batch_rejects_invalid_endpoints_order_and_budgets()
-> Result<(), Box<dyn std::error::Error>> {
    let before = TextSnapshot::new(String::from("甲😀Z"), TextBudget::new(64))?;
    let budget = PatchBudget::new(8, 64, 64);
    assert!(CheckedBatch::new(&before, vec![(8..8, String::from("!"))], budget).is_ok());
    assert!(CheckedBatch::new(&before, vec![(0..0, String::new())], budget).is_ok());
    assert!(matches!(
        CheckedBatch::new(&before, vec![(1..3, String::new())], budget),
        Err(BufferError::NotCharBoundary { .. })
    ));
    assert!(matches!(
        CheckedBatch::new(&before, vec![(8..9, String::new())], budget),
        Err(BufferError::OutOfBounds { .. })
    ));
    assert!(matches!(
        CheckedBatch::new(
            &before,
            vec![(0..3, String::new()), (0..0, String::from("!"))],
            budget
        ),
        Err(BufferError::OverlappingPatches)
    ));
    assert!(matches!(
        CheckedBatch::new(
            &before,
            vec![(7..8, String::new()), (0..3, String::new())],
            budget
        ),
        Err(BufferError::UnsortedPatches)
    ));
    assert!(matches!(
        CheckedBatch::new(
            &before,
            vec![(3..3, String::from("a")), (3..3, String::from("b"))],
            budget
        ),
        Err(BufferError::AmbiguousInsertion)
    ));
    assert!(matches!(
        CheckedBatch::new(
            &before,
            vec![(0..0, String::from("!"))],
            PatchBudget::new(0, 64, 64)
        ),
        Err(BufferError::PatchCountExceeded { .. })
    ));
    assert!(matches!(
        CheckedBatch::new(
            &before,
            vec![(0..0, String::from("!"))],
            PatchBudget::new(8, 0, 64)
        ),
        Err(BufferError::BudgetExceeded { .. })
    ));
    assert!(matches!(
        CheckedBatch::new(
            &before,
            vec![(0..0, String::from("!"))],
            PatchBudget::new(8, 64, 8)
        ),
        Err(BufferError::BudgetExceeded { .. })
    ));
    Ok(())
}
