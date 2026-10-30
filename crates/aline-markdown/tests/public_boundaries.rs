//! 绘制/UTF-16 受检分型的外部消费；范围值由中文与 emoji 字节手工计算。

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{DrawnRange, MappingBudget, MappingError, Projection};

#[test]
fn projection_coordinates_are_typed_and_bound_to_one_instance()
-> Result<(), Box<dyn std::error::Error>> {
    let text = TextSnapshot::new(String::from("前甲😀后"), TextBudget::new(64))?;
    let range = text.src_range(3..10)?;
    let first = Projection::identity(text.clone(), range.clone(), MappingBudget::new(64, 16))?;
    let second = Projection::identity(text, range, MappingBudget::new(64, 16))?;
    assert_eq!(first.drawn_text(), "甲😀");
    assert_eq!(first.drawn_range(3..7)?.byte_range(), 3..7);
    assert_eq!(first.utf16_range(1..3)?.code_unit_range(), 1..3);
    assert!(matches!(
        first.drawn_offset(1),
        Err(MappingError::NotCharBoundary)
    ));
    assert!(matches!(
        first.utf16_offset(2),
        Err(MappingError::MiddleSurrogate)
    ));
    assert!(matches!(
        DrawnRange::new(first.drawn_offset(0)?, second.drawn_offset(0)?),
        Err(MappingError::ProjectionMismatch)
    ));
    Ok(())
}

#[test]
fn immutable_projection_can_be_carried_by_future_host_tasks() {
    fn require_send_sync<T: Send + Sync>() {}
    require_send_sync::<TextSnapshot>();
    require_send_sync::<Projection>();
}
