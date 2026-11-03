//! 文档导航选区原语：受检拒绝，不产生文本或历史变更。
use aline_buffer::{
    HistoryBudget, LineIndexBudget, SelectionSpec, SrcSelection, StringBuffer, TextBudget,
    TextSnapshot,
};
use std::error::Error;

#[test]
fn checked_selection_changes_only_selection_and_rejects_foreign_source()
-> Result<(), Box<dyn Error>> {
    let mut buffer = StringBuffer::new(
        "中文".to_owned(),
        SelectionSpec::new(0, 0),
        TextBudget::new(1024),
        LineIndexBudget::new(8),
        HistoryBudget::new(8, 1024),
    )?;
    let snapshot = buffer.snapshot().clone();
    let selection = SrcSelection::new(snapshot.src_offset(6)?, snapshot.src_offset(3)?)?;
    buffer.set_selection(&selection)?;
    assert_eq!(buffer.selection(), &selection);
    assert_eq!(buffer.snapshot().id(), snapshot.id());
    assert_eq!(buffer.revision(), snapshot.revision());
    assert_eq!(buffer.history_node_count(), 0);
    let other = TextSnapshot::new("中文".to_owned(), TextBudget::new(1024))?;
    let foreign = SrcSelection::new(other.src_offset(0)?, other.src_offset(3)?)?;
    assert!(buffer.set_selection(&foreign).is_err());
    assert_eq!(buffer.selection(), &selection);
    assert_eq!(buffer.snapshot().id(), snapshot.id());
    Ok(())
}
