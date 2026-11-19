use super::super::checked_commit::parse_call_count;
use super::*;
use crate::document::{PublicationKind, WaitStatus, WorkingVersion};
use aline_buffer::{BufferError, HistoryBudget, LineIndexBudget, TextBudget};
use aline_edit::EditRefusal;
use std::{error::Error, io, thread};

type TestResult = Result<(), Box<dyn Error>>;
const TEXT: TextBudget = TextBudget::new(1024);

fn limits(history: HistoryBudget) -> super::super::DocLimits {
    super::super::DocLimits::new(TEXT, LineIndexBudget::new(32), history)
}

fn document(text: &str, selection: SelectionSpec) -> Result<DocCore, DocError> {
    let mut doc = DocCore::new_editable(text, selection, limits(HistoryBudget::new(8, 1024)))?;
    doc.switch_mode(EditorMode::Source, 0)?;
    Ok(doc)
}

/// 失效矩阵要求的零副作用观测面：文本、选区、版本、撤销/重做深度、历史节点与草稿。
#[derive(Debug, PartialEq)]
struct DocState {
    text: String,
    buffer_id: SnapshotId,
    history: Vec<String>,
    selection: SelectionSpec,
    version: WorkingVersion,
    undo_depth: usize,
    redo_depth: usize,
    history_nodes: usize,
    draft: Option<String>,
    open: Option<OpenBlock>,
    wait: Option<WaitState>,
    mode: EditorMode,
    request: DerivedRequestId,
    alive: bool,
}

fn state(doc: &DocCore) -> Result<DocState, DocError> {
    Ok(DocState {
        text: doc.buffer_text()?,
        buffer_id: doc.buffer.snapshot().id().clone(),
        history: (0..doc.history_node_count())
            .filter_map(|index| doc.buffer.history_step(index))
            .map(|step| format!("{:?}", step.get()))
            .collect(),
        selection: doc.current_selection(),
        version: doc.version(),
        undo_depth: doc.undo_depth(),
        redo_depth: doc.redo_depth(),
        history_nodes: doc.history_node_count(),
        draft: doc.open_block_draft().map(str::to_owned),
        open: doc.open.clone(),
        wait: doc.wait,
        mode: doc.mode(),
        request: doc.derived_request,
        alive: doc.alive,
    })
}

#[test]
fn native_delete_prepares_off_thread_and_restores_direction_on_undo() -> TestResult {
    let initial = SelectionSpec::new(8, 4);
    let mut doc = document("a中🙂x", initial)?;
    let identity = doc.current_input_identity()?;
    let current = doc.current_snapshot(TEXT)?;
    let job = doc
        .prepare_native_delete(&identity, &current.selection().range(), 1)?
        .ok_or("缺少删除准备")?;
    let prepared = thread::spawn(|| job.run())
        .join()
        .map_err(|_| io::Error::other("后台准备线程异常"))??;
    assert_eq!(doc.buffer_text()?, "a中🙂x");
    assert_eq!(doc.current_selection(), initial);
    assert_eq!(doc.undo_depth(), 0);

    assert!(matches!(
        doc.publish_native_input(prepared)?,
        CommitOutcome::Committed { .. }
    ));
    assert_eq!(doc.buffer_text()?, "a中x");
    assert_eq!(doc.current_selection(), SelectionSpec::new(4, 4));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "a中🙂x");
    assert_eq!(doc.current_selection(), initial);
    Ok(())
}

#[test]
fn native_staged_delete_publishes_two_steps_and_keeps_draft_before_completion() -> TestResult {
    let mut doc = document("甲乙", SelectionSpec::new(3, 3))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "🙂", 1)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let range = snapshot.text().src_range(0..3)?;
    let before = state(&doc)?;
    let job = doc
        .prepare_native_delete(&identity, &range, 2)?
        .ok_or("缺少两步删除准备")?;
    let prepared = job.run()?;
    // 准备与后台规划只读；发布前文本、选区、版本、历史与草稿均不得变化。
    assert_eq!(state(&doc)?, before, "两步删除准备不得改变文档状态");
    assert_eq!(doc.buffer_text()?, "甲乙");
    assert_eq!(doc.open_block_draft(), Some("🙂"));
    assert_eq!(doc.undo_depth(), 0);

    doc.publish_native_input(prepared)?;
    assert_eq!(doc.buffer_text()?, "🙂乙");
    assert_eq!(doc.current_selection(), SelectionSpec::new(0, 0));
    assert_ne!(doc.version(), before.version);
    assert_eq!(doc.undo_depth(), 2);
    assert_eq!(doc.history_node_count(), 2);
    assert_eq!(doc.open_block_draft(), None);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲🙂乙");
    assert_eq!(doc.current_selection(), SelectionSpec::new(7, 7));
    assert_eq!((doc.undo_depth(), doc.redo_depth()), (1, 1));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲乙");
    assert_eq!(doc.current_selection(), SelectionSpec::new(3, 3));
    assert_eq!((doc.undo_depth(), doc.redo_depth()), (0, 2));
    assert_eq!(doc.history_node_count(), 2);
    Ok(())
}

#[test]
fn prepared_typing_checks_900ms_and_refuses_after_new_draft() -> TestResult {
    let mut doc = document("abc", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 10)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let next = snapshot.selection().range();
    assert!(!doc.native_typing_commit_required(&identity, "2", 910)?);
    assert!(doc.native_typing_commit_required(&identity, "2", 911)?);
    let stale = doc
        .prepare_native_typing_commit(&identity, 911, Some(&next))?
        .ok_or("缺少封口准备")?
        .run()?;
    doc.type_selection(&identity, "2", 910)?;
    let version = doc.version();
    assert!(matches!(
        doc.publish_native_input(stale),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.version(), version);
    assert_eq!(doc.open_block_draft(), Some("12"));
    assert_eq!(doc.current_snapshot(TEXT)?.full_text(TEXT)?, "a12bc");
    assert_eq!(doc.buffer_text()?, "abc");
    assert_eq!(doc.undo_depth(), 0);
    Ok(())
}

#[test]
fn native_confirmation_preserves_cancelled_publication_request_and_commits_input() -> TestResult {
    let mut doc = document("前后", SelectionSpec::new(3, 3))?;
    let identity = doc.current_input_identity()?;
    doc.update_native_candidate(&identity, "未确定", None, 1)?;
    let wait = doc.begin_publication_wait(PublicationKind::Save)?;
    let identity = doc.current_input_identity()?;
    let prepared = doc
        .prepare_native_confirmation(&identity, Some("中"), 2)?
        .run()?;
    doc.cancel_wait(wait)?;
    assert_eq!(doc.composition().ok_or("缺少组合")?.candidate(), "未确定");
    assert_eq!(doc.undo_depth(), 0);

    doc.publish_native_input(prepared)?;
    assert_eq!(doc.buffer_text()?, "前中后");
    assert!(doc.composition().is_none());
    assert_eq!(doc.undo_depth(), 1);
    assert_eq!(
        doc.wait().ok_or("缺少已取消等待")?.status(),
        WaitStatus::Cancelled
    );
    Ok(())
}

#[test]
fn late_native_candidate_and_confirmation_after_cancel_are_stale_without_side_effects() -> TestResult
{
    let mut doc = document("ab", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.update_native_candidate(&identity, "候", None, 1)?;
    doc.composition_cancel(2)?;
    let cancelled = state(&doc)?;
    assert!(doc.composition().is_none());

    // 取消后迟到的平台更新/确认携带旧身份，必须拒绝且不复活旧候选。
    assert!(matches!(
        doc.update_native_candidate(&identity, "迟到", None, 3),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, cancelled);
    assert!(matches!(
        doc.prepare_native_confirmation(&identity, Some("迟到"), 4),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, cancelled);
    Ok(())
}

#[test]
fn native_preparation_refuses_other_document_or_changed_selection() -> TestResult {
    let start = SelectionSpec::new(0, 1);
    let mut doc = document("ab", start)?;
    let identity = doc.current_input_identity()?;
    let current = doc.current_snapshot(TEXT)?;
    let range = current.selection().range();
    let foreign = doc
        .prepare_native_delete(&identity, &range, 1)?
        .ok_or("缺少准备")?
        .run()?;
    let stale = doc
        .prepare_native_delete(&identity, &range, 1)?
        .ok_or("缺少准备")?
        .run()?;
    let mut other = document("ab", start)?;
    assert!(matches!(
        other.publish_native_input(foreign),
        Err(DocError::WrongDocument)
    ));
    assert_eq!(other.buffer_text()?, "ab");
    assert_eq!(other.undo_depth(), 0);

    let reverse = bind_selection(current.text(), SelectionSpec::new(1, 0))?;
    doc.select(&identity, &reverse)?;
    assert_eq!(doc.version(), current.version());
    assert!(matches!(
        doc.publish_native_input(stale),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.current_selection(), SelectionSpec::new(1, 0));
    assert_eq!(doc.buffer_text()?, "ab");
    assert_eq!(doc.undo_depth(), 0);
    Ok(())
}

/// 冻结当前普通输入组并完成后台准备；调用者负责发布或丢弃。
fn prepared_seal(doc: &mut DocCore, now_millis: u64) -> Result<PreparedNativeInput, DocError> {
    let identity = doc.current_input_identity()?;
    doc.prepare_native_typing_commit(&identity, now_millis, None)?
        .ok_or(DocError::NoOpenBlock)?
        .run()
}

/// 在冻结快照上准备一次独立删除；空范围返回 `None`。
fn prepared_delete(
    doc: &mut DocCore,
    range: Range<usize>,
    now_millis: u64,
) -> Result<Option<PreparedNativeInput>, DocError> {
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let source = snapshot.text().src_range(range)?;
    let Some(job) = doc.prepare_native_delete(&identity, &source, now_millis)? else {
        return Ok(None);
    };
    job.run().map(Some)
}

#[test]
fn publish_native_input_never_parses_while_preparation_does() -> TestResult {
    let mut doc = document("abc", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 10)?;
    let identity = doc.current_input_identity()?;
    let baseline = parse_call_count();
    let job = doc
        .prepare_native_typing_commit(&identity, 20, None)?
        .ok_or("缺少封口准备")?;
    assert_eq!(parse_call_count(), baseline, "准备段只冻结，不得解析");
    let prepared = job.run()?;
    let prepared_calls = parse_call_count();
    assert!(prepared_calls > baseline, "候选准备必须解析源码");
    assert!(matches!(
        doc.publish_native_input(prepared)?,
        CommitOutcome::Committed { .. }
    ));
    assert_eq!(parse_call_count(), prepared_calls, "发布段不得解析");
    Ok(())
}

#[test]
fn two_prepared_deletes_record_one_history_node_each() -> TestResult {
    let mut doc = document("abcd", SelectionSpec::new(1, 1))?;
    let mut text = "abcd";
    for (index, (range, expected)) in [(1..2usize, "acd"), (1..2, "ad")].into_iter().enumerate() {
        let before = state(&doc)?;
        let prepared = prepared_delete(&mut doc, range, 1)?.ok_or("缺少删除准备")?;
        // 独立删除的准备与后台规划只读；发布前状态不得变化。
        assert_eq!(state(&doc)?, before, "删除准备不得改变文档状态");
        assert_eq!(doc.buffer_text()?, text);
        assert!(matches!(
            doc.publish_native_input(prepared)?,
            CommitOutcome::Committed { .. }
        ));
        text = expected;
        assert_eq!(doc.buffer_text()?, text);
        assert_eq!(doc.current_selection(), SelectionSpec::new(1, 1));
        assert_ne!(doc.version(), before.version);
        assert_eq!(doc.open_block_draft(), None);
        assert_eq!(doc.undo_depth(), index + 1);
        assert_eq!(doc.history_node_count(), index + 1);
    }
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "acd");
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "abcd");
    Ok(())
}

#[test]
fn prepared_seal_merges_899_and_900_but_splits_at_901() -> TestResult {
    for (second_at, nodes) in [(899, 1), (900, 1), (901, 2)] {
        let mut doc = document("ab", SelectionSpec::new(1, 1))?;
        let identity = doc.current_input_identity()?;
        doc.type_selection(&identity, "1", 0)?;
        let identity = doc.current_input_identity()?;
        doc.type_selection(&identity, "2", second_at)?;
        let before = state(&doc)?;
        let prepared = prepared_seal(&mut doc, second_at + 1)?;
        // 封口准备只冻结组与快照；发布前状态不得变化。
        assert_eq!(state(&doc)?, before, "封口准备不得改变文档状态");
        assert!(matches!(
            doc.publish_native_input(prepared)?,
            CommitOutcome::Committed { .. }
        ));
        assert_eq!(doc.buffer_text()?, "a12b");
        assert_eq!(doc.undo_depth(), nodes);
        assert_eq!(doc.history_node_count(), nodes);
        doc.undo()?;
        if nodes == 1 {
            assert_eq!(doc.buffer_text()?, "ab");
        } else {
            assert_eq!(doc.buffer_text()?, "a1b");
            doc.undo()?;
            assert_eq!(doc.buffer_text()?, "ab");
        }
    }
    Ok(())
}

#[test]
fn sealed_reversed_selection_restores_direction_on_undo() -> TestResult {
    let mut doc = document("abcd", SelectionSpec::new(3, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "X", 0)?;
    let before = state(&doc)?;
    let prepared = prepared_seal(&mut doc, 1)?;
    // 封口准备只冻结组与快照；发布前状态不得变化。
    assert_eq!(state(&doc)?, before, "封口准备不得改变文档状态");
    assert!(matches!(
        doc.publish_native_input(prepared)?,
        CommitOutcome::Committed { .. }
    ));
    assert_eq!(doc.buffer_text()?, "aXd");
    assert_eq!(doc.current_selection(), SelectionSpec::new(2, 2));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "abcd");
    assert_eq!(doc.current_selection(), SelectionSpec::new(3, 1));
    assert!(doc.current_selection().is_reversed());
    Ok(())
}

#[test]
fn protected_region_seal_and_delete_reject_without_side_effects() -> TestResult {
    let limits = limits(HistoryBudget::new(8, 1024));
    let mut doc = DocCore::new("---\nt: x\n---\n", SelectionSpec::new(7, 7), limits)?;
    doc.type_text(7, "Y", 0)?;
    let before = state(&doc)?;
    let identity = doc.current_input_identity()?;
    let sealed = doc
        .prepare_native_typing_commit(&identity, 1, None)?
        .ok_or("缺少封口准备")?
        .run();
    assert!(matches!(
        sealed,
        Err(DocError::Edit(EditRefusal::ProtectedRange { .. }))
    ));
    assert_eq!(state(&doc)?, before);

    let mut doc = DocCore::new("---\nt: x\n---\n", SelectionSpec::new(7, 7), limits)?;
    let before = state(&doc)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let range = snapshot.text().src_range(4..6)?;
    let job = doc
        .prepare_native_delete(&identity, &range, 1)?
        .ok_or("缺少删除准备")?;
    let deleted = job.run();
    assert!(matches!(
        deleted,
        Err(DocError::Edit(EditRefusal::ProtectedRange { .. }))
    ));
    assert_eq!(state(&doc)?, before);
    Ok(())
}

#[test]
fn history_budget_rejects_publish_after_successful_sealing_prepare() -> TestResult {
    let mut doc = DocCore::new_editable(
        "abc",
        SelectionSpec::new(1, 1),
        limits(HistoryBudget::new(1, 1024)),
    )?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 0)?;
    let prepared = prepared_seal(&mut doc, 1)?;
    assert!(matches!(
        doc.publish_native_input(prepared)?,
        CommitOutcome::Committed { .. }
    ));
    assert_eq!(doc.history_node_count(), 1);

    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "2", 5000)?;
    let before = state(&doc)?;
    let prepared = prepared_seal(&mut doc, 5001)?;
    assert!(matches!(
        doc.publish_native_input(prepared),
        Err(DocError::Buffer(BufferError::HistoryBudgetExceeded {
            needed: 2,
            limit: 1
        }))
    ));
    assert_eq!(state(&doc)?, before);
    Ok(())
}

#[test]
fn duplicate_and_superseded_prepared_results_are_stale() -> TestResult {
    let mut doc = document("abc", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 10)?;
    let identity = doc.current_input_identity()?;
    let job = doc
        .prepare_native_typing_commit(&identity, 20, None)?
        .ok_or("缺少封口准备")?;
    let first = thread::spawn(move || job.run())
        .join()
        .map_err(|_| io::Error::other("后台准备线程异常"))??;
    // 所有权禁止字面意义地二次发布同一值；同基线重复准备是等价的重复结果。
    let second = doc
        .prepare_native_typing_commit(&identity, 20, None)?
        .ok_or("缺少封口准备")?
        .run()?;
    assert!(matches!(
        doc.publish_native_input(first)?,
        CommitOutcome::Committed { .. }
    ));
    let committed = state(&doc)?;
    assert!(matches!(
        doc.publish_native_input(second),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, committed);

    // 准备期间继续输入使先前结果失效。
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "2", 30)?;
    let identity = doc.current_input_identity()?;
    let stale = doc
        .prepare_native_typing_commit(&identity, 40, None)?
        .ok_or("缺少封口准备")?
        .run()?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "3", 50)?;
    let edited = state(&doc)?;
    assert!(matches!(
        doc.publish_native_input(stale),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, edited);
    Ok(())
}

#[test]
fn mode_or_selection_change_after_prepare_yields_stale_without_side_effects() -> TestResult {
    let mut doc = document("abcd", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let range = snapshot.text().src_range(1..2)?;
    let prepared = doc
        .prepare_native_delete(&identity, &range, 1)?
        .ok_or("缺少删除准备")?
        .run()?;
    doc.switch_mode(EditorMode::Visual, 2)?;
    let switched = state(&doc)?;
    assert!(matches!(
        doc.publish_native_input(prepared),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, switched);

    let mut doc = document("abcd", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 10)?;
    let identity = doc.current_input_identity()?;
    let prepared = doc
        .prepare_native_typing_commit(&identity, 20, None)?
        .ok_or("缺少封口准备")?
        .run()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let reversed = bind_selection(snapshot.text(), SelectionSpec::new(3, 1))?;
    doc.select(&identity, &reversed)?;
    let selected = state(&doc)?;
    assert!(matches!(
        doc.publish_native_input(prepared),
        Err(DocError::StaleResult)
    ));
    assert_eq!(state(&doc)?, selected);
    Ok(())
}

#[test]
fn closed_document_rejects_prepared_publish_without_side_effects() -> TestResult {
    let mut doc = document("abc", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 10)?;
    let prepared = prepared_seal(&mut doc, 20)?;
    // 测试内直接关闭文档目标：可编辑文档没有公开关闭入口，`close_readonly`
    // 只服务只读文档，此处等价复用其存活位语义。
    doc.alive = false;
    let closed = state(&doc)?;
    assert!(matches!(
        doc.publish_native_input(prepared),
        Err(DocError::ClosedTarget)
    ));
    assert_eq!(state(&doc)?, closed);
    Ok(())
}

#[test]
fn dropped_prepared_input_changes_nothing_unlike_undo_compensation() -> TestResult {
    let mut doc = document("abcd", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let range = snapshot.text().src_range(1..2)?;
    let prepared = doc
        .prepare_native_delete(&identity, &range, 1)?
        .ok_or("缺少删除准备")?
        .run()?;
    let before = state(&doc)?;
    thread::spawn(move || drop(prepared))
        .join()
        .map_err(|_| io::Error::other("后台丢弃线程异常"))?;
    assert_eq!(state(&doc)?, before);

    // 对照：真实发布后再用 Undo 补偿，文本与选区相同，但版本与历史留下痕迹。
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let range = snapshot.text().src_range(1..2)?;
    let prepared = doc
        .prepare_native_delete(&identity, &range, 2)?
        .ok_or("缺少删除准备")?
        .run()?;
    doc.publish_native_input(prepared)?;
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "abcd");
    assert_eq!(doc.current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.undo_depth(), 0);
    assert_eq!(doc.redo_depth(), 1);
    assert_eq!(doc.history_node_count(), 1);
    assert_ne!(doc.version(), before.version);
    Ok(())
}

#[test]
fn close_block_commits_pending_group_and_closes_in_one_publish() -> TestResult {
    let mut doc = document("abc", SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "1", 0)?;
    doc.leave_block(1)?;
    assert_eq!(doc.buffer_text()?, "a1bc");
    assert!(!doc.has_open_block());
    assert_eq!(doc.open_block_draft(), None);
    assert_eq!((doc.undo_depth(), doc.history_node_count()), (1, 1));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "abc");
    assert_eq!(doc.current_selection(), SelectionSpec::new(1, 1));

    let mut doc = DocCore::new(
        "abc",
        SelectionSpec::new(1, 1),
        limits(HistoryBudget::new(8, 1024)),
    )?;
    let version = doc.version();
    doc.leave_block(1)?;
    assert!(!doc.has_open_block());
    assert_eq!(doc.version(), version);
    assert_eq!(doc.history_node_count(), 0);
    Ok(())
}

#[test]
fn close_block_clears_legacy_typing_redo_without_touching_the_buffer() -> TestResult {
    let mut doc = document("ab", SelectionSpec::new(1, 1))?;
    // 手动打开的块携带 Legacy 目标；撤销其待定组会留下同名重做记录。
    doc.open_block(1..2, SelectionSpec::new(1, 1))?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "x", 0)?;
    doc.undo()?;
    assert!(
        doc.typing_redo
            .as_ref()
            .is_some_and(|redo| redo.target == InputTarget::Legacy)
    );
    doc.leave_block(1)?;
    assert!(
        doc.typing_redo.is_none(),
        "关闭块必须直接清理 Legacy 重做记录"
    );
    assert!(!doc.has_open_block());
    assert_eq!(doc.buffer_text()?, "ab");
    assert_eq!(doc.undo_depth(), 0);
    Ok(())
}
