//! W5 3.4 的公开边界契约：ED-01 发布等待与组合自然结束的交错。
//!
//! 覆盖等待期间继续候选、取消请求后继续组合、平台自然确认/取消取最新快照、
//! 迟到平台结果被拒收、以及组合期自动保存延后。窗口层 `SaveHost` 的迁移、
//! `settle_after_input` 的调用点、真实保存任务与真实输入法自然结束均为
//! crate 私有或平台行为：分别由 `native_window/tests.rs` 的窗口用例与
//! `logs/validation/` 中的真实 macOS 取证覆盖，本目标不声称这些路径已通过。

mod common;

use aline_buffer::SelectionSpec;
use aline_ui::{
    DocCore, DocError, ModeSwitchOutcome, PublicationKind, PublicationOutcome, WaitStatus,
};
use common::{TEXT, capture, limits};

fn document(text: &str, anchor: usize, head: usize) -> Result<DocCore, DocError> {
    DocCore::new(text, SelectionSpec::new(anchor, head), limits())
}

#[test]
fn wait_tracks_candidates_and_confirmation_returns_the_latest_snapshot()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("x\n", 1, 1)?;
    doc.composition_update("中", 0)?;
    // 组合活动时自动保存延后，不取中间候选发任务。
    assert!(doc.autosave_snapshot(TEXT)?.is_none());

    let handle = doc.begin_publication_wait(PublicationKind::Save)?;
    doc.composition_update("中国", 1)?;
    assert_eq!(doc.composition().map(|view| view.candidate()), Some("中国"));

    doc.composition_commit(2)?;
    let result = doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 3)?;
    assert_eq!(result.kind(), PublicationKind::Save);
    assert_eq!(result.snapshot().full_text(TEXT)?, "x中国\n");
    assert_eq!(doc.wait(), None);
    assert!(
        doc.autosave_snapshot(TEXT)?.is_some(),
        "组合结束后自动保存恢复"
    );
    Ok(())
}

#[test]
fn cancel_request_keeps_candidates_and_late_platform_result_is_rejected()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("x\n", 1, 1)?;
    doc.composition_update("候", 0)?;
    let handle = doc.begin_publication_wait(PublicationKind::SaveAs)?;
    doc.request_cancel_wait(handle)?;
    assert_eq!(
        doc.wait().map(|wait| wait.status()),
        Some(WaitStatus::Cancelled)
    );

    // 取消请求只终止等待，不伪造组合结束：候选仍可更新、仍可自然确认。
    doc.composition_update("候选", 1)?;
    doc.composition_commit(2)?;
    let committed = capture(&mut doc)?;
    assert!(matches!(
        doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 3),
        Err(DocError::UnknownWait)
    ));
    assert_eq!(capture(&mut doc)?, committed);
    assert_eq!(doc.buffer_text()?, "x候选\n");
    assert_eq!(doc.undo_depth(), 1);
    Ok(())
}

#[test]
fn natural_platform_cancel_returns_post_cancel_snapshot_without_history()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("xy\n", 2, 1)?;
    doc.composition_update("候", 0)?;
    let handle = doc.begin_publication_wait(PublicationKind::Export)?;
    doc.composition_cancel(1)?;

    let result = doc.finish_publication(handle, PublicationOutcome::PlatformCancelled, 2)?;
    assert_eq!(result.kind(), PublicationKind::Export);
    assert_eq!(result.outcome(), PublicationOutcome::PlatformCancelled);
    assert_eq!(result.snapshot().full_text(TEXT)?, "xy\n");
    assert_eq!(doc.buffer_text()?, "xy\n");
    assert_eq!(doc.undo_depth(), 0);
    assert_eq!(
        doc.current_selection(),
        SelectionSpec::new(2, 1),
        "取消组合恢复带方向的组合前选区"
    );
    assert_eq!(doc.wait(), None);
    Ok(())
}

#[test]
fn mode_switch_defers_while_waiting_and_settles_after_the_platform_ends()
-> Result<(), Box<dyn std::error::Error>> {
    let mut doc = document("x\n", 1, 1)?;
    doc.composition_update("中", 0)?;
    let handle = doc.begin_publication_wait(PublicationKind::Save)?;

    let composing = capture(&mut doc)?;
    assert_eq!(
        doc.switch_mode(aline_ui::EditorMode::Source, 1)?,
        ModeSwitchOutcome::Deferred
    );
    assert_eq!(capture(&mut doc)?, composing);

    doc.composition_commit(2)?;
    let result = doc.finish_publication(handle, PublicationOutcome::PlatformConfirmed, 3)?;
    assert_eq!(result.snapshot().full_text(TEXT)?, "x中\n");
    assert_eq!(
        doc.switch_mode(aline_ui::EditorMode::Source, 4)?,
        ModeSwitchOutcome::Ready
    );
    Ok(())
}
