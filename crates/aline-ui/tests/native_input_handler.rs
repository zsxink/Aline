//! W5 原生输入 handler 的假平台契约：按 macOS 回调顺序驱动公开文档核心。
//!
//! 真实的 `DocumentInput` 实现持有 GPUI `Window` 与私有输入投影，crate 外无法
//! 构造；本目标在公开 API 边界复现平台回调顺序（选中 → marked 更新 → 替换/
//! 确认或取消 → 重复结束回调），锁定 handler 必须维持的文档侧契约：
//! 候选只进组合草稿、确认只产生一个 IME 历史步、取消零副作用、组合期普通输入
//! 被拒绝。窗口内投影、候选矩形与真实 macOS 回调由窗口内单测和
//! `logs/validation/` 中的真实取证覆盖，本目标不声称这些平台行为已通过。

mod common;

use aline_buffer::SelectionSpec;
use aline_ui::{CommitOutcome, DocCore, DocError, EditorMode, ModeSwitchOutcome};
use common::{
    assert_buffer, assert_history, assert_revision, capture, collapsed, limits, selection,
    snapshot_text,
};

fn document(text: &str, selection: SelectionSpec) -> Result<DocCore, DocError> {
    DocCore::new(text, selection, limits())
}

#[test]
fn handler_marked_candidate_only_updates_composition_draft()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("ab\n", collapsed(1))?;
    let before = capture(&mut doc)?;

    doc.composition_update("你", 10)?;
    let composition = doc.composition().ok_or("组合视图缺失")?;
    assert_eq!(composition.range(), 1..1);
    assert_eq!(composition.candidate(), "你");
    assert_buffer(&doc, "ab\n")?;
    assert_eq!(snapshot_text(&mut doc)?, "a你b\n");
    assert_eq!((doc.undo_depth(), doc.redo_depth()), (0, 0));

    doc.composition_update("你好", 20)?;
    let composition = doc.composition().ok_or("组合视图缺失")?;
    assert_eq!(composition.range(), 1..1);
    assert_eq!(composition.candidate(), "你好");
    assert_buffer(&doc, "ab\n")?;
    assert_eq!(snapshot_text(&mut doc)?, "a你好b\n");
    assert_eq!((doc.undo_depth(), doc.redo_depth()), (0, 0));
    // 选中与草稿代次属于候选状态，可由同一快照重复读取而不提交。
    assert_revision(&doc, before.revision);
    Ok(())
}

#[test]
fn handler_commit_publishes_one_ime_step_and_keeps_block_open()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("ab\n", collapsed(1))?;
    doc.composition_update("你好", 10)?;
    let outcome = doc.composition_commit(20)?;

    assert!(matches!(outcome, CommitOutcome::Committed { .. }));
    assert_buffer(&doc, "a你好b\n")?;
    assert_revision(&doc, 1);
    assert_history(&doc, 1, 0, 1);
    assert!(doc.has_open_block(), "确认后必须保持块打开");
    assert!(doc.composition().is_none(), "确认后不得残留组合");
    assert_eq!(
        doc.current_selection(),
        SelectionSpec::new(7, 7),
        "插入点应位于已确认文本之后"
    );

    // 重复的结束回调不得再写一次文本或新增历史步。
    let committed = capture(&mut doc)?;
    assert!(matches!(
        doc.composition_commit(30),
        Err(DocError::NoComposition)
    ));
    assert!(matches!(
        doc.composition_cancel(31),
        Err(DocError::NoComposition)
    ));
    assert_eq!(
        capture(&mut doc)?,
        committed,
        "重复结束回调不得改变任何可观察状态"
    );
    Ok(())
}

#[test]
fn handler_cancel_restores_pre_composition_selection_without_history()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("ab\n", selection(1, 2))?;
    doc.composition_update("候", 10)?;
    assert_eq!(snapshot_text(&mut doc)?, "a候\n");

    doc.composition_cancel(20)?;
    assert_buffer(&doc, "ab\n")?;
    assert_eq!(snapshot_text(&mut doc)?, "ab\n");
    assert_history(&doc, 0, 0, 0);
    assert_revision(&doc, 0);
    assert!(doc.composition().is_none());
    assert_eq!(
        doc.current_selection(),
        SelectionSpec::new(1, 2),
        "取消必须恢复组合前带方向的选区"
    );
    Ok(())
}

#[test]
fn handler_typing_undo_redo_are_rejected_and_mode_switch_defers_while_composing()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("ab\n", collapsed(1))?;
    doc.composition_update("候", 10)?;
    let composing = doc.composition().map(|view| view.candidate().to_owned());

    common::assert_rejected_without_change(
        &mut doc,
        |doc| doc.type_text(1, "x", 20),
        |error| matches!(error, DocError::CompositionActive),
    )?;
    // 模式切换不走拒绝：组合期按 W5 协议返回 Deferred，由宿主在平台自然结束后
    // 重试；同样不得改变任何可观察状态。
    let before_switch = capture(&mut doc)?;
    assert_eq!(
        doc.switch_mode(EditorMode::Source, 21)?,
        ModeSwitchOutcome::Deferred
    );
    assert_eq!(capture(&mut doc)?, before_switch);
    common::assert_rejected_without_change(
        &mut doc,
        |doc| doc.undo().map(|_| ()),
        |error| matches!(error, DocError::CompositionActive),
    )?;
    common::assert_rejected_without_change(
        &mut doc,
        |doc| doc.redo().map(|_| ()),
        |error| matches!(error, DocError::CompositionActive),
    )?;
    assert_eq!(
        doc.composition().map(|view| view.candidate().to_owned()),
        composing
    );
    Ok(())
}
