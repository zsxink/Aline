//! 组 8.4 状态表：受控结果采纳、拒绝零副作用与重载历史边界。
//!
//! 每个测试对应 `stale-results.tsv` 的一个稳定用例号。`待定` 的 dg 列只做
//! “调用前后相等”的比较，不作为具体期望值；无 DocCore 入口的行只断言
//! 已确定的状态列。

mod common;

use aline_ui::{DocCore, DocError};
use common::{
    TEXT, assert_buffer, assert_buffer_selection, assert_closed, assert_committed, assert_history,
    assert_idle, assert_new_snapshot, assert_open, assert_rejected_without_change, assert_revision,
    assert_same_snapshot, assert_snapshot, assert_version, capture, collapsed, current_identity,
    identity_for, new_doc, snapshot, snapshot_id, working_version,
};

fn f1() -> Result<DocCore, DocError> {
    new_doc("x\n", 1)
}

fn p_a() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    Ok(doc)
}

/// 8.4-09 的前置：类型 "a" 并提交后取 I_closed，再离块。
fn after_8_4_09() -> Result<DocCore, DocError> {
    let mut doc = p_a()?;
    doc.commit_open_block(10)?;
    doc.leave_block(20)?;
    Ok(doc)
}

/// 8.4-10 的前置：草稿存在时外部源变为 "xZ\n"。
fn fixture_x() -> Result<DocCore, DocError> {
    let mut doc = p_a()?;
    let outcome = doc.reload("xZ\n", collapsed(2), 30)?;
    assert!(matches!(
        outcome,
        aline_buffer::ReloadOutcome::Changed { .. }
    ));
    Ok(doc)
}

/// 8.4-13 的前置：提交、离块后宿主模拟外部源变为 "xZ\n"。
fn after_8_4_13() -> Result<DocCore, DocError> {
    let mut doc = after_8_4_09()?;
    let outcome = doc.reload("xZ\n", collapsed(2), 30)?;
    assert!(matches!(
        outcome,
        aline_buffer::ReloadOutcome::Changed { .. }
    ));
    Ok(doc)
}

/// 8.4-14 的前置：撤销重载。
fn after_8_4_14() -> Result<DocCore, DocError> {
    let mut doc = after_8_4_13()?;
    doc.undo()?;
    Ok(doc)
}

#[test]
fn case_8_4_01_accepts_current_result_without_side_effects()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-01：文档/版本/快照匹配；接受不改变输入、历史或打开块。
    let mut doc = p_a()?;
    let identity = current_identity(&mut doc)?;
    let before = capture(&mut doc)?;
    doc.accept_derived_result(&identity)?;
    let after = capture(&mut doc)?;
    assert_eq!(before, after);
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("a"), 2, 2);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_02_rejects_stale_initial_identity() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-02：dg0 != dg1；草稿 "a" 与选区保留。
    let mut doc = f1()?;
    let initial = snapshot(&mut doc)?;
    let i0 = identity_for(
        doc.document_id().clone(),
        working_version(0, 0),
        initial.text(),
    )?;
    doc.type_text(1, "a", 0)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&i0),
        |error| matches!(error, DocError::StaleResult),
    )?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 0, 1);
    assert_open(&doc, 0, Some("a"), 2, 2);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_03_rejects_other_document_identity() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-03：DocumentId 检查先于版本/快照；C1 状态零变化。
    let mut doc = f1()?;
    let mut other = f1()?;
    let other_snapshot = snapshot(&mut other)?;
    let i_d2 = identity_for(
        other.document_id().clone(),
        other.version(),
        other_snapshot.text(),
    )?;
    let before = capture(&mut doc)?;
    let error = doc.accept_derived_result(&i_d2);
    assert!(matches!(error, Err(DocError::WrongDocument)));
    let after = capture(&mut doc)?;
    assert_eq!(before, after);
    assert_buffer(&doc, "x\n")?;
    assert_version(&doc, 0, 0);
    assert_open(&doc, 0, None, 1, 1);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_04_rejects_bad_draft_generation() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-04：InputIdentity::new 只要求 revision 与快照一致，dg99 可构造。
    let mut doc = f1()?;
    let initial = snapshot(&mut doc)?;
    let bad = identity_for(
        doc.document_id().clone(),
        working_version(0, 99),
        initial.text(),
    )?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&bad),
        |error| matches!(error, DocError::StaleResult),
    )?;
    assert_buffer(&doc, "x\n")?;
    assert_version(&doc, 0, 0);
    assert_open(&doc, 0, None, 1, 1);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_05_rejects_request_captured_before_commit() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-05：revision 0 != 1；提交结果保留，旧请求不得回退文本。
    let mut doc = p_a()?;
    let i1 = current_identity(&mut doc)?;
    let before_commit = snapshot_id(&mut doc)?;
    let outcome = doc.commit_open_block(10)?;
    assert_committed(outcome, 1);
    let after_commit = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before_commit, &after_commit);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&i1),
        |error| matches!(error, DocError::StaleResult),
    )?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 1, 2);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 2, 2);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_06_rejects_identity_after_pending_undo() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-06：文本回到 "x\n" 但 dg0 != dg2；不能按内容相等接受。
    // 撤销待定组不触碰 Buffer，故撤销后的快照复用未变 Buffer 的 SnapshotId；
    // 这里的 snapshot_id=new 相对撤销前的草稿快照成立。
    let mut doc = f1()?;
    let initial = snapshot(&mut doc)?;
    let i0 = identity_for(
        doc.document_id().clone(),
        working_version(0, 0),
        initial.text(),
    )?;
    doc.type_text(1, "a", 0)?;
    let before_undo = snapshot_id(&mut doc)?;
    doc.undo()?;
    let after_undo = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before_undo, &after_undo);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&i0),
        |error| matches!(error, DocError::StaleResult),
    )?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, None, 1, 1);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_07_document_rejects_stale_parse_config() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = p_a()?;
    let old = doc.begin_derived_request()?;
    doc.advance_parse_config()?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old),
        |error| matches!(error, DocError::StaleResult),
    )?;
    let current = doc.begin_derived_request()?;
    doc.accept_derived_result(&current)?;
    Ok(())
}

#[test]
fn case_8_4_08_document_rejects_old_request_at_same_version()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = p_a()?;
    let old = doc.begin_derived_request()?;
    let latest = doc.begin_derived_request()?;
    assert_eq!(old.working_version(), latest.working_version());
    assert_eq!(old.snapshot_id(), latest.snapshot_id());
    assert_ne!(old.request_id(), latest.request_id());
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old),
        |error| matches!(error, DocError::StaleResult),
    )?;
    doc.accept_derived_result(&latest)?;
    Ok(())
}

#[test]
fn case_8_4_09_rejects_result_for_closed_target() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-09：版本/快照均匹配但目标已关闭；不复活关闭对象。
    let mut doc = p_a()?;
    doc.commit_open_block(10)?;
    let i_closed = current_identity(&mut doc)?;
    let closed_id = snapshot_id(&mut doc)?;
    doc.leave_block(20)?;
    assert_closed(&doc);
    let after_leave = snapshot_id(&mut doc)?;
    assert_same_snapshot(&closed_id, &after_leave);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&i_closed),
        |error| matches!(error, DocError::ClosedTarget),
    )?;
    assert_closed(&doc);
    let after_reject = snapshot_id(&mut doc)?;
    assert_same_snapshot(&closed_id, &after_reject);
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 1, 2);
    assert_history(&doc, 1, 0, 1);
    assert_buffer_selection(&doc, 2, 2);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_10_snapshot_rejects_stale_baseline() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-10：基线 rev0 != 当前 rev1；保留草稿/选区；dg 为“待定”。
    let mut doc = fixture_x()?;
    assert_buffer(&doc, "xZ\n")?;
    assert_revision(&doc, 1);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 0, Some("a"), 2, 2);
    assert_buffer_selection(&doc, 2, 2);
    assert_idle(&doc);
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.current_snapshot(TEXT).map(|_| ()),
        |error| matches!(error, DocError::DraftBaselineConflict),
    )?;
    assert_open(&doc, 0, Some("a"), 2, 2);
    Ok(())
}

#[test]
fn case_8_4_11_commit_rejects_stale_baseline_without_dropping_block()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-11：不 drop_open；无文本/历史变化；草稿与选区保留。
    let mut doc = fixture_x()?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.commit_open_block(20).map(|_| ()),
        |error| matches!(error, DocError::DraftBaselineConflict),
    )?;
    assert_buffer(&doc, "xZ\n")?;
    assert_revision(&doc, 1);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 0, Some("a"), 2, 2);
    assert_buffer_selection(&doc, 2, 2);
    Ok(())
}

#[test]
fn case_8_4_12_rejects_old_projection_and_keeps_input() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-12：中间版本 S1 的投影不得套到新草稿；"ab" 与选区保留。
    let mut doc = p_a()?;
    let i1 = current_identity(&mut doc)?;
    doc.type_text(2, "b", 100)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&i1),
        |error| matches!(error, DocError::StaleResult),
    )?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "xab\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("ab"), 3, 3);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_13_reload_forms_independent_history_boundary() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.4-13：节点 source=Reload，不并入 Typing；只改内存。
    // dg 为“待定”（T-8-13），按实现 after_commit 推进，故不断言该值。
    let mut doc = after_8_4_09()?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.reload("xZ\n", collapsed(2), 30)?;
    assert!(matches!(
        outcome,
        aline_buffer::ReloadOutcome::Changed { .. }
    ));
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    assert_buffer(&doc, "xZ\n")?;
    assert_snapshot(&mut doc, "xZ\n")?;
    assert_revision(&doc, 2);
    assert_history(&doc, 2, 0, 2);
    assert_closed(&doc);
    assert_buffer_selection(&doc, 2, 2);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_14_undo_reload_restores_previous_text() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-14：取 Reload 首步 before_selection 2:2；节点不删除。
    let mut doc = after_8_4_13()?;
    let before = snapshot_id(&mut doc)?;
    doc.undo()?;
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_revision(&doc, 3);
    assert_history(&doc, 1, 1, 2);
    assert_closed(&doc);
    assert_buffer_selection(&doc, 2, 2);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_15_redo_reload_restores_reloaded_text() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-15：取 Reload 末步 after_selection 2:2；节点不删除。
    let mut doc = after_8_4_14()?;
    let before = snapshot_id(&mut doc)?;
    doc.redo()?;
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    assert_buffer(&doc, "xZ\n")?;
    assert_snapshot(&mut doc, "xZ\n")?;
    assert_revision(&doc, 4);
    assert_history(&doc, 2, 0, 2);
    assert_closed(&doc);
    assert_buffer_selection(&doc, 2, 2);
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_16_unchanged_reload_preserves_redo() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-16：旧文本==新文本时返回 Unchanged，不调用 prepare_record，
    // redo 分支不截断；节点/版本/选区不变，snapshot_id=same。
    let mut doc = after_8_4_14()?;
    let before_state = capture(&mut doc)?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.reload("xa\n", collapsed(2), 40)?;
    assert!(matches!(outcome, aline_buffer::ReloadOutcome::Unchanged));
    let after_state = capture(&mut doc)?;
    assert_eq!(before_state, after_state);
    let after = snapshot_id(&mut doc)?;
    assert_same_snapshot(&before, &after);
    assert_revision(&doc, 3);
    assert_history(&doc, 1, 1, 2);
    assert_closed(&doc);
    assert_buffer_selection(&doc, 2, 2);
    assert_snapshot(&mut doc, "xa\n")?;
    assert_idle(&doc);
    Ok(())
}

#[test]
fn case_8_4_17_file_coordination_has_no_doc_core_entry() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.4-17：磁盘写回执归宿主；DocCore 不承担文件协调（M0 范围外）。
    // outcome/error 列为“待定（无入口）”，测试不调用文件协调，也不断言该列。
    let mut doc = after_8_4_09()?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 1, 2);
    assert_history(&doc, 1, 0, 1);
    assert_closed(&doc);
    assert_buffer_selection(&doc, 2, 2);
    assert_idle(&doc);
    Ok(())
}
