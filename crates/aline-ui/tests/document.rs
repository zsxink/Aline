//! 组 8.1 状态表：唯一 Buffer/History/OpenBlock 与当前快照。
//!
//! 每个测试对应 `open-block-snapshot.tsv` 的一个稳定用例号；断言按 TSV 列
//! 逐项进行。TSV 中 `待定` 的列在对应测试里只做“调用前后相等”的零副作用
//! 比较，不作为具体期望值。

mod common;

use aline_buffer::BufferError;
use aline_ui::{DocCore, DocError};
use common::{
    TEXT, assert_buffer, assert_closed, assert_committed, assert_composition, assert_history,
    assert_new_snapshot, assert_open, assert_rejected_without_change, assert_revision,
    assert_same_snapshot, assert_snapshot, assert_version, assert_wait, capture, collapsed,
    new_doc, snapshot_id,
};

fn f1() -> Result<DocCore, DocError> {
    new_doc("x\n", 1)
}

fn f4() -> Result<DocCore, DocError> {
    new_doc("中\n", 3)
}

fn after_8_1_01() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    Ok(doc)
}

fn after_8_1_03() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_01()?;
    doc.type_text(2, "b", 100)?;
    Ok(doc)
}

fn after_8_1_04() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_03()?;
    let _ = doc.current_snapshot(TEXT)?;
    Ok(doc)
}

fn after_8_1_05() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_04()?;
    doc.commit_open_block(200)?;
    Ok(doc)
}

fn after_8_1_06() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_05()?;
    let _ = doc.current_snapshot(TEXT)?;
    Ok(doc)
}

fn after_8_1_07() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_06()?;
    doc.type_text(3, "c", 300)?;
    Ok(doc)
}

fn after_8_1_08() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_07()?;
    let _ = doc.current_snapshot(TEXT)?;
    Ok(doc)
}

fn after_8_1_10() -> Result<DocCore, DocError> {
    after_8_1_01()
}

fn after_8_1_11() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_10()?;
    doc.commit_open_block(10)?;
    Ok(doc)
}

fn after_8_1_12() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_11()?;
    doc.type_text(2, "b", 20)?;
    Ok(doc)
}

fn after_8_1_13() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_12()?;
    doc.commit_open_block(30)?;
    Ok(doc)
}

fn after_8_1_14() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_13()?;
    doc.undo()?;
    Ok(doc)
}

fn after_8_1_15() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_14()?;
    let _ = doc.current_snapshot(TEXT)?;
    Ok(doc)
}

fn after_8_1_21() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    doc.commit_open_block(10)?;
    doc.leave_block(20)?;
    Ok(doc)
}

/// 夹具 X：普通草稿存在时，宿主模拟外部源变化为 "xZ\n"。
fn fixture_x() -> Result<DocCore, DocError> {
    let mut doc = after_8_1_01()?;
    let outcome = doc.reload("xZ\n", collapsed(2), 30)?;
    assert!(matches!(
        outcome,
        aline_buffer::ReloadOutcome::Changed { .. }
    ));
    Ok(doc)
}

#[test]
fn document_case_8_1_01_typing_enters_unique_draft() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-01：普通输入只进入唯一草稿，Buffer/History 不变。
    let mut doc = f1()?;
    let base = snapshot_id(&mut doc)?;
    doc.type_text(1, "a", 0)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("a"), 2, 2);
    assert_composition(&doc, None);
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&base, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_02_snapshot_keeps_identity_and_does_not_commit()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-02：同一工作版本重复读取保留同一 SnapshotId。
    let mut doc = after_8_1_01()?;
    let first = snapshot_id(&mut doc)?;
    let second = snapshot_id(&mut doc)?;
    assert_same_snapshot(&first, &second);
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("a"), 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_03_adjacent_input_merges_within_900ms()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-03：100 <= 900 并入同一草稿组，dg 前进，历史仍为 0。
    let mut doc = after_8_1_01()?;
    let before = snapshot_id(&mut doc)?;
    doc.type_text(2, "b", 100)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("ab"), 3, 3);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_04_snapshot_text_equals_latest_draft() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.1-04：快照文本逐字节等于最新草稿，且未弹历史节点。
    let mut doc = after_8_1_03()?;
    let first = snapshot_id(&mut doc)?;
    let second = snapshot_id(&mut doc)?;
    assert_same_snapshot(&first, &second);
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("ab"), 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_05_commit_group_keeps_block_open() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-05：组提交为单个节点，块保持打开并封口。
    let mut doc = after_8_1_04()?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.commit_open_block(200)?;
    assert_committed(outcome, 1);
    assert_buffer(&doc, "xab\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 3, 3);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_06_snapshot_after_commit_has_no_side_effects()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-06：快照不压缩历史、不封口、不清 redo。
    let mut doc = after_8_1_05()?;
    let first = snapshot_id(&mut doc)?;
    let second = snapshot_id(&mut doc)?;
    assert_same_snapshot(&first, &second);
    assert_buffer(&doc, "xab\n")?;
    assert_history(&doc, 1, 0, 1);
    assert_version(&doc, 1, 3);
    assert_open(&doc, 1, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_07_typing_after_commit_opens_new_draft()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-07：提交后的新输入开启新草稿，Buffer 仍落后快照。
    let mut doc = after_8_1_06()?;
    let before = snapshot_id(&mut doc)?;
    doc.type_text(3, "c", 300)?;
    assert_buffer(&doc, "xab\n")?;
    assert_snapshot(&mut doc, "xabc\n")?;
    assert_version(&doc, 1, 4);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("c"), 4, 4);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_08_snapshot_after_new_draft() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-08：快照文本等于最新草稿，历史未压缩。
    let mut doc = after_8_1_07()?;
    let first = snapshot_id(&mut doc)?;
    let second = snapshot_id(&mut doc)?;
    assert_same_snapshot(&first, &second);
    assert_snapshot(&mut doc, "xabc\n")?;
    assert_version(&doc, 1, 4);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("c"), 4, 4);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_09_repeated_snapshot_keeps_same_id() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.1-09：同版本连续读取 SnapshotId 相同，零副作用。
    let mut doc = after_8_1_08()?;
    let first = snapshot_id(&mut doc)?;
    let second = snapshot_id(&mut doc)?;
    let third = snapshot_id(&mut doc)?;
    assert_same_snapshot(&first, &second);
    assert_same_snapshot(&second, &third);
    assert_buffer(&doc, "xab\n")?;
    assert_snapshot(&mut doc, "xabc\n")?;
    assert_history(&doc, 1, 0, 1);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_10_redo_sequence_starts_from_fresh_draft()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-10：开始 redo 保持序列的首次输入。
    let mut doc = after_8_1_10()?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("a"), 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_11_first_commit_makes_node_one() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-11：提交 1 组，Buffer 追平草稿。
    let mut doc = after_8_1_11()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 1, 2);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_12_sealed_group_is_not_merged_again() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.1-12：旧组已封口，本次输入是新草稿。
    let mut doc = after_8_1_12()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("b"), 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_13_second_commit_makes_node_two() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-13：第二个节点，node=2。
    let mut doc = after_8_1_13()?;
    assert_buffer(&doc, "xab\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 2, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_14_undo_restores_before_selection() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-14：无待定草稿时撤销节点 2，revision 前进并恢复选区。
    let mut doc = after_8_1_14()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 3, 5);
    assert_history(&doc, 1, 1, 2);
    assert_open(&doc, 3, None, 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_15_snapshot_preserves_redo() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-15：快照不清 redo、不压缩历史。
    let mut doc = after_8_1_14()?;
    let first = snapshot_id(&mut doc)?;
    let second = snapshot_id(&mut doc)?;
    assert_same_snapshot(&first, &second);
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 3, 5);
    assert_history(&doc, 1, 1, 2);
    assert_open(&doc, 3, None, 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_16_redo_restores_after_selection() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-16：Redo 恢复节点 2 after-selection 3:3。
    let mut doc = after_8_1_15()?;
    doc.redo()?;
    assert_buffer(&doc, "xab\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 4, 6);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 4, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_17_snapshot_rejects_stale_baseline() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.1-17：基线 rev0 != 当前 rev1，拒绝且不产生快照。
    // TSV 的 dg 为“待定”（T-8-13），因此这里不断言 dg 的绝对值。
    let mut doc = fixture_x()?;
    assert_buffer(&doc, "xZ\n")?;
    assert_revision(&doc, 1);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 0, Some("a"), 2, 2);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.current_snapshot(TEXT).map(|_| ()),
        |error| matches!(error, DocError::DraftBaselineConflict),
    )?;
    assert_open(&doc, 0, Some("a"), 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_18_typing_rejects_stale_baseline() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-18：拒绝输入且完整状态零变化；dg 为“待定”。
    let mut doc = fixture_x()?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.type_text(2, "b", 100),
        |error| matches!(error, DocError::DraftBaselineConflict),
    )?;
    assert_buffer(&doc, "xZ\n")?;
    assert_open(&doc, 0, Some("a"), 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_19_commit_rejects_stale_baseline() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-19：不把旧基线组写入新源；dg 为“待定”。
    let mut doc = fixture_x()?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.commit_open_block(100).map(|_| ()),
        |error| matches!(error, DocError::DraftBaselineConflict),
    )?;
    assert_buffer(&doc, "xZ\n")?;
    assert_open(&doc, 0, Some("a"), 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_20_leave_rejects_stale_baseline_without_dropping_block()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-20：拒绝关闭，块仍打开，草稿/选区保留；dg 为“待定”。
    let mut doc = fixture_x()?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.leave_block(100),
        |error| matches!(error, DocError::DraftBaselineConflict),
    )?;
    assert!(doc.has_open_block());
    assert_open(&doc, 0, Some("a"), 2, 2);
    assert_buffer(&doc, "xZ\n")?;
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_21_commit_after_close_returns_no_open_block()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.1-21：唯一打开槽位已关闭，拒绝且状态零变化。
    let mut doc = after_8_1_21()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 1, 2);
    assert_history(&doc, 1, 0, 1);
    assert_closed(&doc);
    let before = snapshot_id(&mut doc)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.commit_open_block(30).map(|_| ()),
        |error| matches!(error, DocError::NoOpenBlock),
    )?;
    assert_closed(&doc);
    let after = snapshot_id(&mut doc)?;
    assert_same_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn document_case_8_1_22_non_boundary_typing_is_rejected() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.1-22：偏移 1 位于三字节 UTF-8 字符内部，文本/版本/历史/选区零副作用。
    let mut doc = f4()?;
    let before = capture(&mut doc)?;
    let error = doc.type_text(1, "a", 0);
    assert!(matches!(
        error,
        Err(DocError::Buffer(BufferError::NotCharBoundary { offset: 1 }))
    ));
    let after = capture(&mut doc)?;
    assert_eq!(before, after);
    assert_buffer(&doc, "中\n")?;
    assert_snapshot(&mut doc, "中\n")?;
    assert_version(&doc, 0, 0);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}
