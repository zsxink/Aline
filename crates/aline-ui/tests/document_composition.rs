//! 组 8.3 状态表：组合候选、发布等待与当前快照。
//!
//! 每个测试对应 `composition.tsv` 的一个稳定用例号。候选只进入草稿视图，
//! 不写 Buffer；发布等待不冻结输入，确认时才取当前快照。

mod common;

use aline_ui::{CommitOutcome, DocCore, DocError, PublicationKind, PublicationOutcome, WaitStatus};
use common::{
    State, TEXT, assert_buffer, assert_committed, assert_composition, assert_handle,
    assert_history, assert_new_snapshot, assert_open, assert_rejected_without_change,
    assert_same_snapshot, assert_snapshot, assert_version, assert_wait, capture, current_identity,
    new_doc, publication_snapshot, snapshot_id,
};

fn f1() -> Result<DocCore, DocError> {
    new_doc("x\n", 1)
}

/// C1 上隐式开始组合并更新为 "中"（8.3-01 的前置）。
fn c1_composition_zhong() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.composition_update("中", 0)?;
    Ok(doc)
}

/// 8.3-04 的前置：更新到 "中国" 并提交。
fn after_8_3_04() -> Result<DocCore, DocError> {
    let mut doc = c1_composition_zhong()?;
    doc.composition_update("中国", 10)?;
    doc.composition_update("中国", 20)?;
    doc.composition_commit(30)?;
    Ok(doc)
}

/// 8.3-08 的前置：普通输入提交后再开始组合 "中"。
fn setup_8_3_08() -> Result<DocCore, DocError> {
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    doc.commit_open_block(10)?;
    doc.composition_update("中", 20)?;
    Ok(doc)
}

/// 8.3-11 的前置：组合 "中" 上建立 Save 等待。
fn after_8_3_11() -> Result<(DocCore, aline_ui::WaitHandle), DocError> {
    let mut doc = c1_composition_zhong()?;
    let handle = doc.begin_publication_wait(PublicationKind::Save)?;
    Ok((doc, handle))
}

/// 8.3-13 的前置：等待保持时把候选替换为 "中国"。
fn after_8_3_13() -> Result<(DocCore, aline_ui::WaitHandle), DocError> {
    let (mut doc, handle) = after_8_3_11()?;
    doc.composition_update("中国", 10)?;
    Ok((doc, handle))
}

/// 比较两份状态，忽略等待字段（等待状态是本用例的被测目标）。
fn assert_unchanged_except_wait(before: &State, after: &State) {
    assert_eq!(before.buffer_text, after.buffer_text);
    assert_eq!(before.snapshot_text, after.snapshot_text);
    assert_eq!(before.revision, after.revision);
    assert_eq!(before.draft_generation, after.draft_generation);
    assert_eq!(before.undo, after.undo);
    assert_eq!(before.redo, after.redo);
    assert_eq!(before.nodes, after.nodes);
    assert_eq!(before.buffer_anchor, after.buffer_anchor);
    assert_eq!(before.buffer_head, after.buffer_head);
    assert_eq!(before.open, after.open);
    assert_eq!(before.composition, after.composition);
}

#[test]
fn case_8_3_01_first_update_opens_composition_implicitly() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.3-01：首次 update 以当前选区隐式开启组合（T-8-08）。
    let mut doc = f1()?;
    let base = snapshot_id(&mut doc)?;
    doc.composition_update("中", 0)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中"), 4, 4);
    assert_composition(&doc, Some((1..1, "中")));
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&base, &after);
    Ok(())
}

#[test]
fn composition_case_8_3_02_update_replaces_candidate() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-02：update 是替换而非追加；汉字各 3 字节。
    let mut doc = c1_composition_zhong()?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_update("中国", 10)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中国\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中国"), 7, 7);
    assert_composition(&doc, Some((1..1, "中国")));
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn composition_case_8_3_03_identical_update_advances_draft_generation()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-03：同等文本仍推进 dg（T-8-06），历史与 revision 不变。
    let mut doc = c1_composition_zhong()?;
    doc.composition_update("中国", 10)?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_update("中国", 20)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中国\n")?;
    assert_version(&doc, 0, 3);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中国"), 7, 7);
    assert_composition(&doc, Some((1..1, "中国")));
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn composition_case_8_3_04_commit_makes_single_ime_node() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.3-04：单区间补丁 1..1→"中国"；ImeCommit 独立成步。
    let mut doc = c1_composition_zhong()?;
    doc.composition_update("中国", 10)?;
    doc.composition_update("中国", 20)?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.composition_commit(30)?;
    assert_committed(outcome, 1);
    assert_buffer(&doc, "x中国\n")?;
    assert_snapshot(&mut doc, "x中国\n")?;
    assert_version(&doc, 1, 4);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 7, 7);
    assert_composition(&doc, None);
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn case_8_3_05_update_after_commit_opens_new_composition() -> Result<(), Box<dyn std::error::Error>>
{
    // TSV 8.3-05：确认后再输入开启新组合；候选不落盘。
    let mut doc = after_8_3_04()?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_update("文", 40)?;
    assert_buffer(&doc, "x中国\n")?;
    assert_snapshot(&mut doc, "x中国文\n")?;
    assert_version(&doc, 1, 5);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, Some("文"), 10, 10);
    assert_composition(&doc, Some((7..7, "文")));
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn case_8_3_06_cancel_restores_pre_composition_state() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-06：取消恢复组合前选区；无空史步，dg 推进。
    let mut doc = c1_composition_zhong()?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_cancel(10)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, None, 1, 1);
    assert_composition(&doc, None);
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn case_8_3_07_cancelled_composition_identity_is_stale() -> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-07：取消后旧候选身份的 SnapshotId/WorkingVersion 不得复活。
    let mut doc = c1_composition_zhong()?;
    let old_identity = current_identity(&mut doc)?;
    doc.composition_cancel(10)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| doc.accept_derived_result(&old_identity),
        |error| matches!(error, DocError::StaleResult),
    )?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 0, 2);
    assert_open(&doc, 0, None, 1, 1);
    assert_composition(&doc, None);
    Ok(())
}

#[test]
fn composition_case_8_3_08_ime_commit_does_not_merge_adjacent_typing_node()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-08：补丁 2..2→"中"；IME 确认独立成组，未与 Typing 合并。
    let mut doc = setup_8_3_08()?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.composition_commit(30)?;
    assert_committed(outcome, 2);
    assert_buffer(&doc, "xa中\n")?;
    assert_snapshot(&mut doc, "xa中\n")?;
    assert_version(&doc, 2, 4);
    assert_history(&doc, 2, 0, 2);
    assert_open(&doc, 2, None, 5, 5);
    assert_composition(&doc, None);
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn composition_case_8_3_09_cancel_keeps_existing_committed_input()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-09：既有输入 "a" 不丢；取消不新增历史步。
    let mut doc = setup_8_3_08()?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_cancel(30)?;
    assert_buffer(&doc, "xa\n")?;
    assert_snapshot(&mut doc, "xa\n")?;
    assert_version(&doc, 1, 4);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 2, 2);
    assert_composition(&doc, None);
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn composition_case_8_3_10_empty_candidate_commit_is_unchanged()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-10：空候选确认走 apply 的 Unchanged；组合结束推进 dg（T-8-07）。
    let mut doc = f1()?;
    doc.composition_update("", 0)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 0, 1);
    assert_open(&doc, 0, Some(""), 1, 1);
    assert_composition(&doc, Some((1..1, "")));
    assert_wait(&doc, None);
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.composition_commit(10)?;
    assert_eq!(outcome, CommitOutcome::Unchanged);
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, None, 1, 1);
    assert_composition(&doc, None);
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_11_begin_wait_has_no_snapshot_side_effect()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-11：begin 不取快照、不封口、不改输入/历史/版本。
    let mut doc = c1_composition_zhong()?;
    let before = capture(&mut doc)?;
    let handle = doc.begin_publication_wait(PublicationKind::Save)?;
    assert_handle(handle);
    assert_wait(&doc, Some((PublicationKind::Save, WaitStatus::Pending)));
    let after = capture(&mut doc)?;
    assert_unchanged_except_wait(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_12_second_wait_is_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-12：同一 DocCore 同时只允许一个等待；W1 保持。
    let (mut doc, handle) = after_8_3_11()?;
    assert_handle(handle);
    assert_rejected_without_change(
        &mut doc,
        |doc| {
            doc.begin_publication_wait(PublicationKind::SaveAs)
                .map(|_| ())
        },
        |error| matches!(error, DocError::PublicationInFlight),
    )?;
    assert_wait(&doc, Some((PublicationKind::Save, WaitStatus::Pending)));
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_13_wait_does_not_freeze_candidate_updates()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-13：发布等待不冻结输入；确认时须取最终快照。
    let (mut doc, _) = after_8_3_11()?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_update("中国", 10)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中国\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中国"), 7, 7);
    assert_composition(&doc, Some((1..1, "中国")));
    assert_wait(&doc, Some((PublicationKind::Save, WaitStatus::Pending)));
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_case_8_3_14_finish_during_composition_is_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-14：平台自然结束前不得发布候选，等待保持。
    let (mut doc, handle) = after_8_3_13()?;
    assert_rejected_without_change(
        &mut doc,
        |doc| {
            doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 20)
                .map(|_| ())
        },
        |error| matches!(error, DocError::CompositionActive),
    )?;
    assert_wait(&doc, Some((PublicationKind::Save, WaitStatus::Pending)));
    assert_composition(&doc, Some((1..1, "中国")));
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_15_commit_keeps_wait_pending()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-15：平台自然确认后组合结束；发布等待尚未完成。
    let (mut doc, _) = after_8_3_13()?;
    let before = snapshot_id(&mut doc)?;
    let outcome = doc.composition_commit(30)?;
    assert_committed(outcome, 1);
    assert_buffer(&doc, "x中国\n")?;
    assert_snapshot(&mut doc, "x中国\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 7, 7);
    assert_composition(&doc, None);
    assert_wait(&doc, Some((PublicationKind::Save, WaitStatus::Pending)));
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_case_8_3_16_finish_returns_post_composition_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-16：取自然结束后的当前快照；发布不新增步、不封口、不清 redo。
    let (mut doc, handle) = after_8_3_13()?;
    doc.composition_commit(30)?;
    let before = snapshot_id(&mut doc)?;
    let result = doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 40)?;
    let current = publication_snapshot(
        result,
        PublicationKind::Save,
        PublicationOutcome::PlatformConfirmed,
    );
    let after = current.snapshot_id().clone();
    assert_same_snapshot(&before, &after);
    assert_eq!(current.full_text(TEXT)?, "x中国\n");
    assert_wait(&doc, None);
    assert_buffer(&doc, "x中国\n")?;
    assert_version(&doc, 1, 3);
    assert_history(&doc, 1, 0, 1);
    assert_open(&doc, 1, None, 7, 7);
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_17_save_as_wait_is_established()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-17：kind 不改变等待状态机。夹具独立，句柄从 W1 开始编号。
    let mut doc = c1_composition_zhong()?;
    let before = capture(&mut doc)?;
    let handle = doc.begin_publication_wait(PublicationKind::SaveAs)?;
    assert_handle(handle);
    assert_wait(&doc, Some((PublicationKind::SaveAs, WaitStatus::Pending)));
    let after = capture(&mut doc)?;
    assert_unchanged_except_wait(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_18_export_wait_is_established()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-18：导出与保存共用同一条当前快照等待。
    let mut doc = c1_composition_zhong()?;
    let before = capture(&mut doc)?;
    let handle = doc.begin_publication_wait(PublicationKind::Export)?;
    assert_handle(handle);
    assert_wait(&doc, Some((PublicationKind::Export, WaitStatus::Pending)));
    let after = capture(&mut doc)?;
    assert_unchanged_except_wait(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_19_cancel_request_only_changes_wait()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-19：只取消发布请求；不结束组合、不改输入/历史/版本。
    let (mut doc, handle) = after_8_3_11()?;
    let before = capture(&mut doc)?;
    doc.request_cancel_wait(handle)?;
    assert_wait(&doc, Some((PublicationKind::Save, WaitStatus::Cancelled)));
    let after = capture(&mut doc)?;
    assert_unchanged_except_wait(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_20_finish_cancelled_wait_is_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-20：取消请求后同一等待不再有效；状态不变。
    let (mut doc, handle) = after_8_3_11()?;
    doc.request_cancel_wait(handle)?;
    assert_rejected_without_change(
        &mut doc,
        |doc| {
            doc.finish_publication(handle, PublicationOutcome::PlatformCancelled, 20)
                .map(|_| ())
        },
        |error| matches!(error, DocError::UnknownWait),
    )?;
    assert_wait(&doc, Some((PublicationKind::Save, WaitStatus::Cancelled)));
    Ok(())
}

#[test]
fn publication_wait_composition_case_8_3_21_update_clears_cancelled_wait()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-21：取消发布请求不等于取消组合；候选可继续更新。
    let (mut doc, handle) = after_8_3_11()?;
    doc.request_cancel_wait(handle)?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_update("中国", 10)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中国\n")?;
    assert_version(&doc, 0, 2);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中国"), 7, 7);
    assert_composition(&doc, Some((1..1, "中国")));
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn publication_wait_case_8_3_22_cancel_after_wait_restores_pre_composition()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-22：无空史步、revision 不前进；无发布发生。
    let (mut doc, handle) = after_8_3_11()?;
    doc.request_cancel_wait(handle)?;
    doc.composition_update("中国", 10)?;
    let before = snapshot_id(&mut doc)?;
    doc.composition_cancel(20)?;
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x\n")?;
    assert_version(&doc, 0, 3);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, None, 1, 1);
    assert_composition(&doc, None);
    assert_wait(&doc, None);
    let after = snapshot_id(&mut doc)?;
    assert_new_snapshot(&before, &after);
    Ok(())
}

#[test]
fn case_8_3_23_publication_wait_autosave_defers_during_composition()
-> Result<(), Box<dyn std::error::Error>> {
    // 纯状态入口只返回延后信号，不启动计时器或磁盘保存。
    let mut doc = c1_composition_zhong()?;
    let before = common::capture(&mut doc)?;
    assert!(doc.autosave_snapshot(TEXT)?.is_none());
    assert_eq!(before, common::capture(&mut doc)?);
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中"), 4, 4);
    assert_composition(&doc, Some((1..1, "中")));
    assert_wait(&doc, None);
    Ok(())
}

#[test]
fn composition_case_8_3_24_restore_read_keeps_snapshot_identity()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-24：恢复读取返回含候选的快照；不建立等待、不结束组合。
    let mut doc = c1_composition_zhong()?;
    let first = snapshot_id(&mut doc)?;
    let recovery = doc.recovery_snapshot(TEXT)?;
    let second = recovery.snapshot_id().clone();
    assert_same_snapshot(&first, &second);
    assert_buffer(&doc, "x\n")?;
    assert_snapshot(&mut doc, "x中\n")?;
    assert_version(&doc, 0, 1);
    assert_history(&doc, 0, 0, 0);
    assert_open(&doc, 0, Some("中"), 4, 4);
    assert_composition(&doc, Some((1..1, "中")));
    assert_wait(&doc, None);
    Ok(())
}

#[test]
fn case_8_3_25_composition_does_not_merge_pending_typing_draft()
-> Result<(), Box<dyn std::error::Error>> {
    // TSV 8.3-25：组合开始时已有普通待定组——状态列全为“待定”，
    // TSV 待定，按实现决定先提交并封口该组再进入组合（T-8-05），
    // 见 report 待确认节。规范只禁止误并，故这里仅断言：
    // 1) 等待列（已确定）为 none；2) 候选文本未被写入已提交 Buffer。
    let mut doc = f1()?;
    doc.type_text(1, "a", 0)?;
    doc.composition_update("中", 10)?;
    assert_wait(&doc, None);
    assert!(!doc.buffer_text()?.contains('中'));
    // 唯一草稿槽的候选若存在，必须与组合视图一致，不能变成第二个草稿。
    if let Some(candidate) = doc.composition().map(|view| view.candidate().to_owned()) {
        assert_eq!(doc.open_block_draft(), Some(candidate.as_str()));
    }
    Ok(())
}
