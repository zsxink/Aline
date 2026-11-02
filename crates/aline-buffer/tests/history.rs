//! 组 5 历史语义：选区/来源/代次、合并窗口、保存点、裁剪、重载与预算失败原子性。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{
    ApplyOutcome, BufferError, CheckedBatch, DocumentRevision, HistoryBudget, HistoryStepHandle,
    LineIndexBudget, OperationSource, PatchBudget, ReloadOutcome, SelectionSpec, StringBuffer,
    TextBudget,
};

const TEXT_BUDGET: usize = 4096;
const PATCH_BUDGET: usize = 32;

#[derive(Debug)]
struct State {
    text: String,
    line_count: usize,
    selection: SelectionSpec,
    revision: DocumentRevision,
    undo: usize,
    redo: usize,
    nodes: usize,
}

#[test]
fn history_records_source_selections_and_monotonic_revisions() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("abcd", 4, 4, 16)?;
    apply_patch(
        &mut buffer,
        vec![(4..4, String::from("X"))],
        (5, 5),
        OperationSource::Typing,
        1000,
    )?;
    let first = buffer
        .history_step(0)
        .ok_or_else(|| "缺少第一步".to_string())?;
    assert_eq!(first.get().source(), OperationSource::Typing);
    assert_eq!(
        first.get().before_revision(),
        DocumentRevision::from_value(0)
    );
    assert_eq!(
        first.get().after_revision(),
        DocumentRevision::from_value(1)
    );
    assert_eq!(first.get().before_selection(), SelectionSpec::new(4, 4));
    assert_eq!(first.get().after_selection(), SelectionSpec::new(5, 5));
    assert_eq!(first.get().forward_patches(), &[(4..4, String::from("X"))]);
    assert_eq!(first.get().inverse_patches(), &[(4..5, String::new())]);

    apply_patch(
        &mut buffer,
        vec![(1..3, String::from("YZ"))],
        (2, 1),
        OperationSource::Replace,
        1001,
    )?;
    let second = buffer
        .history_step(1)
        .ok_or_else(|| "缺少第二步".to_string())?;
    assert_eq!(second.get().source(), OperationSource::Replace);
    assert_eq!(second.get().before_selection(), SelectionSpec::new(5, 5));
    assert_eq!(second.get().after_selection(), SelectionSpec::new(2, 1));
    assert_eq!(
        second.get().before_revision(),
        DocumentRevision::from_value(1)
    );
    assert_eq!(
        second.get().after_revision(),
        DocumentRevision::from_value(2)
    );
    assert_eq!(text(&buffer)?, "aYZdX");

    let mut revisions = vec![buffer.revision()];
    revisions.push(buffer.undo(TextBudget::new(TEXT_BUDGET))?);
    assert_eq!(text(&buffer)?, "abcdX");
    assert_eq!(selection(&buffer), SelectionSpec::new(5, 5));
    revisions.push(buffer.redo(TextBudget::new(TEXT_BUDGET))?);
    assert_eq!(text(&buffer)?, "aYZdX");
    assert_eq!(selection(&buffer), SelectionSpec::new(2, 1));
    revisions.push(buffer.undo(TextBudget::new(TEXT_BUDGET))?);
    revisions.push(buffer.redo(TextBudget::new(TEXT_BUDGET))?);
    for pair in revisions.windows(2) {
        assert!(pair[0].value() < pair[1].value(), "revision 必须严格单调");
    }
    Ok(())
}

#[test]
fn history_typing_merge_window_and_seal_boundaries() -> Result<(), Box<dyn Error>> {
    let mut merged = make_buffer("a", 1, 1, 16)?;
    apply_patch(
        &mut merged,
        vec![(1..1, String::from("b"))],
        (2, 2),
        OperationSource::Typing,
        1000,
    )?;
    apply_patch(
        &mut merged,
        vec![(2..2, String::from("c"))],
        (3, 3),
        OperationSource::Typing,
        1900,
    )?;
    assert_eq!(merged.history_node_count(), 1, "900ms 内应合并");
    assert_eq!(
        merged
            .history_step(0)
            .ok_or_else(|| "缺少合并步骤".to_string())?
            .get()
            .before_selection(),
        SelectionSpec::new(2, 2)
    );
    merged.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&merged)?, "a", "一次 Undo 应撤销整个待定组");
    assert_eq!(selection(&merged), SelectionSpec::new(1, 1));
    merged.redo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&merged)?, "abc");
    assert_eq!(selection(&merged), SelectionSpec::new(3, 3));

    let mut outside = make_buffer("a", 1, 1, 16)?;
    apply_patch(
        &mut outside,
        vec![(1..1, String::from("b"))],
        (2, 2),
        OperationSource::Typing,
        1000,
    )?;
    apply_patch(
        &mut outside,
        vec![(2..2, String::from("c"))],
        (3, 3),
        OperationSource::Typing,
        1901,
    )?;
    assert_eq!(outside.history_node_count(), 2, "超过 900ms 不得合并");

    let mut sealed = make_buffer("a", 1, 1, 16)?;
    apply_patch(
        &mut sealed,
        vec![(1..1, String::from("b"))],
        (2, 2),
        OperationSource::Typing,
        1000,
    )?;
    sealed.seal_pending();
    apply_patch(
        &mut sealed,
        vec![(2..2, String::from("c"))],
        (3, 3),
        OperationSource::Typing,
        1001,
    )?;
    assert_eq!(sealed.history_node_count(), 2, "seal 后不得合并");
    sealed.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&sealed)?, "ab");
    sealed.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&sealed)?, "a");

    let mut replaced = make_buffer("a", 1, 1, 16)?;
    apply_patch(
        &mut replaced,
        vec![(1..1, String::from("b"))],
        (2, 2),
        OperationSource::Replace,
        2000,
    )?;
    apply_patch(
        &mut replaced,
        vec![(2..2, String::from("c"))],
        (3, 3),
        OperationSource::Typing,
        2001,
    )?;
    assert_eq!(replaced.history_node_count(), 2, "非普通输入不得并入输入组");
    Ok(())
}

#[test]
fn history_same_stack_depth_after_save_undo_new_edit_is_not_save_point()
-> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("a", 1, 1, 16)?;
    apply_patch(
        &mut buffer,
        vec![(1..1, String::from("X"))],
        (2, 2),
        OperationSource::Command,
        1000,
    )?;
    buffer.mark_saved();
    assert!(buffer.is_at_save_point());
    assert_eq!(
        buffer.saved_revision(),
        Some(DocumentRevision::from_value(1))
    );
    let saved_depth = buffer.undo_depth();
    assert_eq!(saved_depth, 1);

    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "a");
    assert!(!buffer.is_at_save_point());

    apply_patch(
        &mut buffer,
        vec![(1..1, String::from("Y"))],
        (2, 2),
        OperationSource::Command,
        1001,
    )?;
    assert_eq!(text(&buffer)?, "aY");
    assert_eq!(buffer.undo_depth(), saved_depth, "栈深相同不能当作保存身份");
    assert!(!buffer.is_at_save_point());
    assert_eq!(buffer.redo_depth(), 0);
    assert_eq!(buffer.history_node_count(), 1);
    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "a", "撤销新编辑不能回到旧保存文本");
    assert!(!buffer.is_at_save_point());
    Ok(())
}

#[test]
fn history_content_equal_redo_keeps_new_identity_dirty() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("ab", 2, 2, 16)?;
    apply_patch(
        &mut buffer,
        vec![(2..2, String::from("X"))],
        (3, 3),
        OperationSource::Command,
        1000,
    )?;
    buffer.mark_saved();
    assert!(buffer.is_at_save_point());
    let saved_revision = buffer.saved_revision();
    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    buffer.redo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "abX", "内容与保存点相等");
    assert!(
        !buffer.is_at_save_point(),
        "新快照身份不能仅凭内容相等视为保存点"
    );
    assert_eq!(buffer.saved_revision(), saved_revision);
    Ok(())
}

#[test]
fn history_redo_branch_is_discarded_by_new_edit() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("a", 1, 1, 16)?;
    apply_patch(
        &mut buffer,
        vec![(1..1, String::from("X"))],
        (2, 2),
        OperationSource::Command,
        1000,
    )?;
    apply_patch(
        &mut buffer,
        vec![(2..2, String::from("Y"))],
        (3, 3),
        OperationSource::Command,
        1001,
    )?;
    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "aX");
    assert_eq!(buffer.redo_depth(), 1);
    assert!(buffer.can_redo());

    apply_patch(
        &mut buffer,
        vec![(2..2, String::from("Z"))],
        (3, 3),
        OperationSource::Command,
        1002,
    )?;
    assert_eq!(text(&buffer)?, "aXZ");
    assert_eq!(buffer.history_node_count(), 2);
    assert_eq!(buffer.redo_depth(), 0);
    assert_eq!(
        buffer
            .history_step(1)
            .ok_or_else(|| "缺少新分支步骤".to_string())?
            .get()
            .forward_patches(),
        &[(2..2, String::from("Z"))]
    );
    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "aX");
    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "a");
    assert!(!buffer.can_undo());
    assert_eq!(buffer.redo_depth(), 2);
    Ok(())
}

#[test]
fn history_explicit_redo_branch_discard_keeps_current_snapshot_and_savepoint()
-> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("a", 1, 1, 16)?;
    apply_patch(
        &mut buffer,
        vec![(1..1, String::from("X"))],
        (2, 2),
        OperationSource::Command,
        1000,
    )?;
    buffer.mark_saved();
    let saved_revision = buffer.saved_revision();
    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    let current_revision = buffer.revision();
    let current_text = text(&buffer)?;
    let current_selection = buffer.selection().clone();
    assert_eq!(buffer.redo_depth(), 1);

    buffer.discard_redo_branch();

    assert_eq!(text(&buffer)?, current_text);
    assert_eq!(buffer.revision(), current_revision);
    assert_eq!(buffer.selection(), &current_selection);
    assert_eq!(buffer.redo_depth(), 0);
    assert_eq!(buffer.history_node_count(), 0);
    assert!(!buffer.is_at_save_point());
    assert_eq!(buffer.saved_revision(), saved_revision);
    Ok(())
}

#[test]
fn history_trimming_protects_save_point_and_current_node() -> Result<(), Box<dyn Error>> {
    let mut trimmed = make_buffer("a", 1, 1, 3)?;
    for step in 1..=5usize {
        let position = step;
        apply_patch(
            &mut trimmed,
            vec![(position..position, step.to_string())],
            (position + 1, position + 1),
            OperationSource::Command,
            1000 + u64::try_from(step)?,
        )?;
    }
    assert_eq!(trimmed.history_node_count(), 3, "预算只保留最近 3 个节点");
    let outcome = trimmed.trim_history(1);
    assert_eq!(outcome.removed(), 2);
    assert_eq!(outcome.retained(), 1);
    trimmed.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&trimmed)?, "a1234");
    assert!(!trimmed.can_undo());

    let mut protected = make_buffer("a", 1, 1, 4)?;
    apply_patch(
        &mut protected,
        vec![(1..1, String::from("A"))],
        (2, 2),
        OperationSource::Command,
        1000,
    )?;
    protected.mark_saved();
    for (offset, input) in ['B', 'C', 'D'].into_iter().enumerate() {
        let position = 2 + offset;
        apply_patch(
            &mut protected,
            vec![(position..position, input.to_string())],
            (position + 1, position + 1),
            OperationSource::Command,
            1001 + u64::try_from(offset)?,
        )?;
    }
    assert_eq!(text(&protected)?, "aABCD");
    assert_eq!(protected.history_node_count(), 4);
    let state = capture(&protected)?;
    let rejected = CheckedBatch::new(
        protected.snapshot(),
        vec![(5..5, String::from("E"))],
        PatchBudget::new(PATCH_BUDGET, TEXT_BUDGET, TEXT_BUDGET),
    )?;
    assert!(matches!(
        protected.apply(
            &rejected,
            SelectionSpec::new(6, 6),
            OperationSource::Command,
            2000,
            TextBudget::new(TEXT_BUDGET)
        ),
        Err(BufferError::HistoryBudgetExceeded {
            needed: 5,
            limit: 4
        })
    ));
    assert_state(&protected, &state)?;
    let outcome = protected.trim_history(1);
    assert_eq!(outcome.removed(), 0, "保存点所在前缀不得裁剪");
    assert_eq!(outcome.retained(), 4);

    for _ in 0..3 {
        protected.undo(TextBudget::new(TEXT_BUDGET))?;
    }
    assert_eq!(text(&protected)?, "aA", "保存点内容可重放");
    assert!(
        !protected.is_at_save_point(),
        "Undo 生成新快照身份，内容证据待定所以保守 dirty"
    );
    assert_eq!(
        protected.saved_revision(),
        Some(DocumentRevision::from_value(1))
    );
    Ok(())
}

#[test]
fn history_undo_and_redo_budget_failures_leave_state_unchanged() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("abc", 3, 3, 16)?;
    apply_patch(
        &mut buffer,
        vec![(3..3, String::from("X"))],
        (4, 4),
        OperationSource::Command,
        1000,
    )?;
    let before_undo = capture(&buffer)?;
    assert!(matches!(
        buffer.undo(TextBudget::new(3)),
        Err(BufferError::BudgetExceeded { .. })
    ));
    assert_state(&buffer, &before_undo)?;

    buffer.undo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "abc");
    let before_redo = capture(&buffer)?;
    assert!(matches!(
        buffer.redo(TextBudget::new(3)),
        Err(BufferError::BudgetExceeded { .. })
    ));
    assert_state(&buffer, &before_redo)?;
    assert_eq!(before_redo.redo, 1);
    buffer.redo(TextBudget::new(TEXT_BUDGET))?;
    assert_eq!(text(&buffer)?, "abcX");
    Ok(())
}

#[test]
fn history_reload_is_independent_memory_boundary() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("abc", 3, 3, 16)?;
    apply_patch(
        &mut buffer,
        vec![(3..3, String::from("X"))],
        (4, 4),
        OperationSource::Typing,
        1000,
    )?;
    buffer.mark_saved();
    let outcome = buffer.reload(
        String::from("xyz"),
        SelectionSpec::new(3, 3),
        2000,
        TextBudget::new(TEXT_BUDGET),
    )?;
    let ReloadOutcome::Changed { revision } = outcome else {
        return Err("重载必须产生独立边界".to_string().into());
    };
    assert_eq!(revision, DocumentRevision::from_value(2));
    assert_eq!(text(&buffer)?, "xyz");
    assert_eq!(buffer.history_node_count(), 2);
    let reload_step = buffer
        .history_step(1)
        .ok_or_else(|| "缺少重载步骤".to_string())?;
    assert_eq!(reload_step.get().source(), OperationSource::Reload);
    assert_eq!(
        reload_step.get().forward_patches(),
        &[(0..4, String::from("xyz"))]
    );
    assert_eq!(
        reload_step.get().inverse_patches(),
        &[(0..3, String::from("abcX"))]
    );
    assert!(!buffer.is_at_save_point());
    assert_eq!(
        buffer.saved_revision(),
        Some(DocumentRevision::from_value(1))
    );

    let mut revisions = vec![revision];
    revisions.push(buffer.undo(TextBudget::new(TEXT_BUDGET))?);
    assert_eq!(text(&buffer)?, "abcX");
    assert_eq!(selection(&buffer), SelectionSpec::new(4, 4));
    revisions.push(buffer.undo(TextBudget::new(TEXT_BUDGET))?);
    assert_eq!(text(&buffer)?, "abc");
    assert_eq!(selection(&buffer), SelectionSpec::new(3, 3));
    revisions.push(buffer.redo(TextBudget::new(TEXT_BUDGET))?);
    assert_eq!(text(&buffer)?, "abcX");
    assert_eq!(buffer.redo_depth(), 1);

    let unchanged = buffer.reload(
        String::from("abcX"),
        SelectionSpec::new(4, 4),
        2001,
        TextBudget::new(TEXT_BUDGET),
    )?;
    assert!(matches!(unchanged, ReloadOutcome::Unchanged));
    assert_eq!(buffer.redo_depth(), 1, "无变化重载不得清除 redo");
    assert_eq!(
        buffer.revision(),
        revisions.last().copied().unwrap_or(revision)
    );

    revisions.push(buffer.redo(TextBudget::new(TEXT_BUDGET))?);
    assert_eq!(text(&buffer)?, "xyz");
    for pair in revisions.windows(2) {
        assert!(pair[0].value() < pair[1].value());
    }
    Ok(())
}

#[test]
fn history_handles_keep_trimmed_patch_resources_alive() -> Result<(), Box<dyn Error>> {
    let mut buffer = make_buffer("a", 1, 1, 2)?;
    apply_patch(
        &mut buffer,
        vec![(1..1, String::from("X"))],
        (2, 2),
        OperationSource::Command,
        1000,
    )?;
    let handle: HistoryStepHandle = buffer
        .history_step(0)
        .ok_or_else(|| "缺少旧步骤句柄".to_string())?;
    assert_eq!(handle.get().forward_patches(), &[(1..1, String::from("X"))]);

    apply_patch(
        &mut buffer,
        vec![(2..2, String::from("Y"))],
        (3, 3),
        OperationSource::Command,
        1001,
    )?;
    apply_patch(
        &mut buffer,
        vec![(3..3, String::from("Z"))],
        (4, 4),
        OperationSource::Command,
        1002,
    )?;
    assert_eq!(buffer.history_node_count(), 2);
    assert_eq!(
        buffer
            .history_step(0)
            .ok_or_else(|| "缺少当前首节点".to_string())?
            .get()
            .forward_patches(),
        &[(2..2, String::from("Y"))]
    );
    assert_eq!(
        handle.get().forward_patches(),
        &[(1..1, String::from("X"))],
        "句柄必须让被裁剪节点的补丁文本继续可达"
    );
    Ok(())
}

fn make_buffer(
    text: &str,
    anchor: usize,
    head: usize,
    max_nodes: usize,
) -> Result<StringBuffer, Box<dyn Error>> {
    Ok(StringBuffer::new(
        text.to_string(),
        SelectionSpec::new(anchor, head),
        TextBudget::new(TEXT_BUDGET),
        LineIndexBudget::new(64),
        HistoryBudget::new(max_nodes, 1 << 20),
    )?)
}

fn apply_patch(
    buffer: &mut StringBuffer,
    patches: Vec<(Range<usize>, String)>,
    after: (usize, usize),
    source: OperationSource,
    now_millis: u64,
) -> Result<(), Box<dyn Error>> {
    let batch = CheckedBatch::new(
        buffer.snapshot(),
        patches,
        PatchBudget::new(PATCH_BUDGET, TEXT_BUDGET, TEXT_BUDGET),
    )?;
    let outcome = buffer.apply(
        &batch,
        SelectionSpec::new(after.0, after.1),
        source,
        now_millis,
        TextBudget::new(TEXT_BUDGET),
    )?;
    if !matches!(outcome, ApplyOutcome::Changed(_)) {
        return Err("测试批次必须实际改变文本".to_string().into());
    }
    Ok(())
}

fn text(buffer: &StringBuffer) -> Result<String, Box<dyn Error>> {
    Ok(buffer.text(TextBudget::new(TEXT_BUDGET))?.into_owned())
}

fn selection(buffer: &StringBuffer) -> SelectionSpec {
    SelectionSpec::new(
        buffer.selection().anchor().byte_offset(),
        buffer.selection().head().byte_offset(),
    )
}

fn capture(buffer: &StringBuffer) -> Result<State, Box<dyn Error>> {
    Ok(State {
        text: text(buffer)?,
        line_count: buffer.line_count(),
        selection: selection(buffer),
        revision: buffer.revision(),
        undo: buffer.undo_depth(),
        redo: buffer.redo_depth(),
        nodes: buffer.history_node_count(),
    })
}

fn assert_state(buffer: &StringBuffer, expected: &State) -> Result<(), Box<dyn Error>> {
    let actual = capture(buffer)?;
    assert_eq!(actual.text, expected.text);
    assert_eq!(actual.line_count, expected.line_count);
    assert_eq!(actual.selection, expected.selection);
    assert_eq!(actual.revision, expected.revision);
    assert_eq!(actual.undo, expected.undo);
    assert_eq!(actual.redo, expected.redo);
    assert_eq!(actual.nodes, expected.nodes);
    Ok(())
}
