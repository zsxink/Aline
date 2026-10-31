//! 组 8.2 状态表：显式虚拟时间下的输入分组。
//!
//! 每个测试对应 `typing-groups.tsv` 的一个稳定用例号。900ms 边界取
//! `TYPING_MERGE_WINDOW_MILLIS = 900` 与 `<=`：899/900 合并，901 拆分。

mod common;

use aline_ui::{CommitOutcome, DocCore, DocError};
use common::{
    assert_buffer, assert_closed, assert_committed, assert_composition, assert_history,
    assert_new_snapshot, assert_open, assert_rejected_without_change, assert_same_snapshot,
    assert_snapshot, assert_version, assert_wait, collapsed, new_doc, snapshot_id,
};

fn f1() -> Result<DocCore, DocError> {
    new_doc("x\n", 1)
}

fn f2() -> Result<DocCore, DocError> {
    new_doc("abcd\n", 4)
}

fn p_a() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    Ok(doc)
}

fn after_8_2_01() -> Result<DocCore, DocError> {
    let mut doc = p_a()?;
    doc.type_text(2, "b", 899)?;
    Ok(doc)
}

fn after_8_2_02() -> Result<DocCore, DocError> {
    let mut doc = p_a()?;
    doc.type_text(2, "b", 900)?;
    Ok(doc)
}

fn after_8_2_03() -> Result<DocCore, DocError> {
    let mut doc = p_a()?;
    doc.type_text(2, "b", 901)?;
    Ok(doc)
}

fn after_8_2_07() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    doc.commit_open_block(10)?;
    doc.type_text(2, "b", 100)?;
    Ok(doc)
}

fn after_8_2_08() -> Result<DocCore, DocError> {
    let mut doc = after_8_2_07()?;
    doc.commit_open_block(110)?;
    Ok(doc)
}

fn after_8_2_09() -> Result<DocCore, DocError> {
    let mut doc = f2()?;
    doc.type_text(4, "X", 0)?;
    doc.set_selection(collapsed(0))?;
    doc.type_text(0, "Y", 100)?;
    Ok(doc)
}

fn after_8_2_12() -> Result<DocCore, DocError> {
    let mut doc = f2()?;
    doc.type_text(4, "X", 0)?;
    doc.commit_open_block(10)?;
    doc.type_text(5, "Y", 100)?;
    Ok(doc)
}

fn after_8_2_15() -> Result<DocCore, DocError> {
    let mut doc = p_a()?;
    doc.type_text(2, "b", 100)?;
    doc.undo()?;
    Ok(doc)
}

fn after_8_2_17() -> Result<DocCore, DocError> {
    let mut doc = after_8_2_07()?;
    doc.undo()?;
    Ok(doc)
}

fn after_8_2_18() -> Result<DocCore, DocError> {
    let mut doc = after_8_2_17()?;
    doc.undo()?;
    Ok(doc)
}

fn after_8_2_20() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    let _ = doc.current_snapshot(common::TEXT)?;
    Ok(doc)
}

#[test]
fn case_8_2_01_899ms_merges() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-01：899 <= 900，并入同一草稿组；历史仍 0/0/0。
    let mut doc = p_a()?;
    let before = snapshot_id(&mut doc)?;
    doc.type_text(2, "b", 899)?;
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
fn case_8_2_02_900ms_merges_on_closed_boundary() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-02：900 <= 900，闭区间边界仍并入同一组。
    let mut doc = p_a()?;
    let before = snapshot_id(&mut doc)?;
    doc.type_text(2, "b", 900)?;
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
fn case_8_2_03_901ms_splits_and_commits_old_group() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-03：901 > 900，旧组提交并封口，dg 从 1→3（T-8-04）。
    let mut doc = p_a()?;
    let before = snapshot_id(&mut doc)?;
    doc.type_text(2, "b", 901)?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("b"), 3, 3);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_04_commit_after_899_merge_makes_one_node() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-04：899 侧提交为一个节点，提交后块仍打开。
    let mut doc = after_8_2_01()?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.commit_open_block(901)?;
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
fn case_8_2_05_commit_after_900_merge_makes_one_node() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-05：900 侧只产生一个节点，闭区间边界不拆组。
    let mut doc = after_8_2_02()?;
    let outcome = doc.commit_open_block(902)?;
    assert_committed(outcome, 1);
    assert_buffer(&doc, "xab\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_06_commit_after_split_makes_second_node() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-06：901 侧为第二个节点，node=2。
    let mut doc = after_8_2_03()?;
    let outcome = doc.commit_open_block(903)?;
    assert_committed(outcome, 2);
    assert_buffer(&doc, "xab\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 2, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_07_explicit_commit_is_group_boundary() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-07：即使 90ms 且相邻，显式提交后也开启新草稿。
    let mut doc = after_8_2_07()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("b"), 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_08_seal_prevents_merge_of_commits() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-08：node=2 证明封口，未把两次提交并成一个节点。
    let doc = after_8_2_08()?;
    assert_buffer(&doc, "xab\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 2, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_09_non_adjacent_input_splits_group() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-09：非相邻输入拆组，旧组 X 提交，Y 成为新草稿。
    let mut doc = after_8_2_09()?;
    assert_buffer(&doc, "abcdX\n")?;
    assert_snapshot(&mut doc, "YabcdX\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("Y"), 1, 1);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_10_non_adjacent_group_commits_independently() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.2-10：node=2，非相邻输入形成独立组。
    let mut doc = after_8_2_09()?;
    let outcome = doc.commit_open_block(110)?;
    assert_committed(outcome, 2);
    assert_buffer(&doc, "YabcdX\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 2, None, 1, 1);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_11_commit_without_pending_group_is_unchanged() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.2-11：无待定组时不产生空步；snapshot_id 保持 same。
    let mut doc = after_8_2_08()?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.commit_open_block(120)?;
    assert_eq!(outcome, CommitOutcome::Unchanged);
    let after = snapshot_id(&mut doc)?;
    assert_same_snapshot(&before, &after);
    assert_buffer(&doc, "xab\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 2, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_12_leave_commits_remaining_group_and_closes() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.2-12：离块只处理剩余组 Y，node=2。
    let mut doc = after_8_2_12()?;
    let before = snapshot_id(&mut doc)?;
    doc.leave_block(200)?;
    assert_buffer(&doc, "abcdXY\n")?;
    assert_snapshot(&mut doc, "abcdXY\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_closed(&doc);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_13_escape_commits_remaining_group_and_closes() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.2-13：非组合 Escape 提交剩余组而非丢弃。
    let mut doc = after_8_2_12()?;
    doc.escape(200)?;
    assert_buffer(&doc, "abcdXY\n")?;
    assert_snapshot(&mut doc, "abcdXY\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_closed(&doc);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_14_escape_after_close_is_rejected() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-14：关闭后无第二打开槽位，状态零变化。
    let mut doc = after_8_2_12()?;
    doc.escape(200)?;
    assert_closed(&doc);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.escape(210),
        |error| matches!(error, DocError::NoOpenBlock),
    )?;
    assert_closed(&doc);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_15_undo_prefers_pending_draft() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-15：撤销待定组不提交、不新增节点、revision 不变。
    let mut doc = p_a()?;
    doc.type_text(2, "b", 100)?;
    let before = snapshot_id(&mut doc)?;
    doc.undo()?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 0, 3);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, None, 1, 1);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_16_redo_restores_pending_group() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-16：Redo 恢复同一待定组及 after-selection 3:3。
    let mut doc = after_8_2_15()?;
    let before = snapshot_id(&mut doc)?;
    doc.redo()?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 0, 4);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("ab"), 3, 3);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_17_undo_pending_draft_before_committed_node() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.2-17：有已提交节点时仍先撤销待定草稿，节点 1 保持。
    let mut doc = after_8_2_17()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 1, 4);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_18_second_undo_undoes_node_one() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-18：第二次 Undo 才撤销节点 1，选区恢复 1:1。
    let mut doc = after_8_2_18()?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 2, 5);
    assert_history(&doc, 0, 1, 1);
    assert_open(&doc, 2, None, 1, 1);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_19_redo_restores_node_one() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-19：Redo 恢复节点 1 after-selection 2:2。
    let mut doc = after_8_2_18()?;
    doc.redo()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 3, 6);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 3, None, 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_20_snapshot_does_not_seal_group() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-20：快照不封口，草稿与历史状态不变。
    let mut doc = after_8_2_20()?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("a"), 2, 2);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_21_typing_after_snapshot_still_merges() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-21：快照后 100ms 相邻输入仍并入同一组，历史仍 0/0/0。
    let mut doc = after_8_2_20()?;
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
fn case_8_2_22_snapshot_and_typing_produce_one_node() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-22：快照与继续输入交错后只产生 1 个节点。
    let mut doc = after_8_2_20()?;
    doc.type_text(2, "b", 100)?;
    let outcome = doc.commit_open_block(200)?;
    assert_committed(outcome, 1);
    assert_buffer(&doc, "xab\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 3, 3);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_23_mode_switch_has_no_doc_core_entry() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-23：宿主模式切换有协议性期望，但 DocCore 无入口（T-8-15）；
    // outcome/error 列为“待定”，测试不调用任何模式切换，也不断言该列。
    let mut doc = f1()?;
    doc.composition_update("中", 0)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中"), 4, 4);
    assert_composition(&doc, Some((1..1, "中")));
    assert_wait(&doc, None);
    // 等待列为 none（已确定）；组合列本用例即前置状态，不额外断言。
    Ok(())
}

#[test]
fn case_8_2_24_typing_after_ime_commit_is_ordinary() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-24：平台自然确认后，剩余输入走普通协议。
    let mut doc = f1()?;
    doc.composition_update("中", 0)?;
    doc.composition_commit(10)?;
    let before = snapshot_id(&mut doc)?;
    doc.type_text(4, "a", 100)?;
    assert_buffer(&doc, "x中\n")?;
    assert_snapshot(&mut doc, "x中a\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("a"), 5, 5);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    common::assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_2_25_post_ime_input_commits_as_one_step() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.2-25：切换后剩余输入提交为 1 步；切换未追加历史步。
    let mut doc = f1()?;
    doc.composition_update("中", 0)?;
    doc.composition_commit(10)?;
    doc.type_text(4, "a", 100)?;
    let outcome = doc.commit_open_block(110)?;
    assert_committed(outcome, 2);
    assert_buffer(&doc, "x中a\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 2, None, 5, 5);
    common::assert_idle(&doc);
    Ok(())
}
