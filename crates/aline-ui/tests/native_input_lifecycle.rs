//! W5 3.2 生命周期门控的无头回归：组合期模式切换、撤销/重做与迟到回调
//! 都必须零副作用地等待平台自然结果。
//!
//! 真实 macOS 菜单借焦、窗口失活与 IME 行为不在本目标取证；窗口层
//! `accept_reading_hit` 的反馈映射依赖 GPUI，亦不在此断言。

mod common;

use aline_buffer::{SelectionSpec, SrcSelection};
use aline_ui::{CommitOutcome, DocCore, DocError, EditorMode, ModeSwitchOutcome};
use common::{TEXT, capture, limits};

fn editable(text: &str, at: usize) -> Result<DocCore, DocError> {
    DocCore::new(text, SelectionSpec::new(at, at), limits())
}

#[test]
fn composition_mode_switch_defers_without_state_change() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = editable("abc\n", 1)?;
    doc.composition_start("候", 0)?;
    let before = capture(&mut doc)?;

    assert_eq!(
        doc.switch_mode(EditorMode::Source, 1)?,
        ModeSwitchOutcome::Deferred
    );
    assert_eq!(capture(&mut doc)?, before);
    assert_eq!(doc.mode(), EditorMode::Visual);
    assert_eq!(
        doc.composition().map(|view| view.candidate().to_owned()),
        Some("候".to_owned())
    );
    Ok(())
}

#[test]
fn composition_undo_and_redo_are_rejected_without_state_change()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = editable("ab\n", 1)?;
    doc.type_text(1, "中", 0)?;
    doc.commit_open_block(1)?;
    assert_eq!((doc.undo_depth(), doc.redo_depth()), (1, 0));
    doc.composition_start("候", 2)?;
    let before = capture(&mut doc)?;

    assert!(matches!(doc.undo(), Err(DocError::CompositionActive)));
    assert_eq!(capture(&mut doc)?, before);
    assert!(matches!(doc.redo(), Err(DocError::CompositionActive)));
    assert_eq!(capture(&mut doc)?, before);
    assert_eq!(doc.buffer_text()?, "a中b\n");
    assert_eq!((doc.undo_depth(), doc.redo_depth()), (1, 0));
    Ok(())
}

#[test]
fn composition_click_selection_is_rejected_without_state_change()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = editable("ab\n\ncd\n", 1)?;
    doc.composition_start("候", 0)?;
    let identity = doc.current_input_identity()?;
    let current = doc.current_snapshot(TEXT)?;
    let selection =
        SrcSelection::new(current.text().src_offset(1)?, current.text().src_offset(1)?)?;
    let before = capture(&mut doc)?;

    // 同块点击在 `open_visual_at` 内走 `DocCore::select`；此处直接验证同一入口。
    assert!(matches!(
        doc.select(&identity, &selection),
        Err(DocError::CompositionActive)
    ));
    assert_eq!(capture(&mut doc)?, before);
    assert_eq!(
        doc.composition().map(|view| view.candidate().to_owned()),
        Some("候".to_owned())
    );
    Ok(())
}

#[test]
fn composition_cancel_rejects_late_end_callbacks_without_state_change()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = editable("abc\n", 1)?;
    doc.composition_update("候", 0)?;
    doc.composition_cancel(1)?;
    let cancelled = capture(&mut doc)?;
    assert!(cancelled.composition.is_none());

    // 裸 `composition_update` 在无组合时会隐式开启新组合（既有协议），
    // 不能用于断言“迟到更新被拒绝”；此处用 commit 与重复 cancel 覆盖迟到回调。
    assert!(matches!(
        doc.composition_commit(2),
        Err(DocError::NoComposition)
    ));
    assert_eq!(capture(&mut doc)?, cancelled);
    assert!(matches!(
        doc.composition_cancel(2),
        Err(DocError::NoComposition)
    ));
    assert_eq!(capture(&mut doc)?, cancelled);
    Ok(())
}

#[test]
fn mode_switch_succeeds_after_composition_commit() -> Result<(), Box<dyn std::error::Error>> {
    let mut doc = editable("ab\n", 1)?;
    doc.composition_update("中", 0)?;
    assert!(matches!(
        doc.composition_commit(1)?,
        CommitOutcome::Committed { .. }
    ));
    assert!(doc.composition().is_none());
    assert_eq!(doc.buffer_text()?, "a中b\n");
    let history = (doc.undo_depth(), doc.redo_depth(), doc.history_node_count());
    let version = doc.version();

    assert_eq!(
        doc.switch_mode(EditorMode::Source, 2)?,
        ModeSwitchOutcome::Ready
    );
    assert_eq!(doc.mode(), EditorMode::Source);
    assert_eq!(doc.buffer_text()?, "a中b\n");
    assert_eq!(doc.version(), version);
    assert_eq!(
        (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
        history
    );
    Ok(())
}
