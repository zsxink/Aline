//! M2 4.3 的公开边界契约：组合确认/取消/继续输入、目标变化、关闭与迟到回执
//! 在同一文档核心/会话上互不串扰。
//!
//! 磁盘 bytes、资源身份、保存等待结算与真实 macOS 中文链路不在本目标：可编辑
//! 文档的可变入口（输入调度、保存协调器、资源迁移）不通过公开 API 暴露，由
//! crate 内 `document/persistence/tests.rs`、`native_window/tests.rs` 覆盖
//! （保存等待 × 外改 × 重载的集成链见
//! `session_composition_wait_conflict_preserves_input_then_reload_keeps_undo_boundary`）；
//! 真实磁盘/内存对照见 `logs/validation/m2-editable-core/` 的采样证据。本目标
//! 不声称真实窗口、IME 或多平台行为已通过。

mod common;

use std::error::Error;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use aline_ui::{
    CloseDisposition, DocCore, DocError, DocLimits, OpenDisposition, ParseRefreshDisposition,
    ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenLimits, ReadonlySession,
};
use common::{TEXT, capture, limits};

fn document(text: &str, at: usize) -> Result<DocCore, DocError> {
    DocCore::new(text, SelectionSpec::new(at, at), limits())
}

#[test]
fn confirm_cancel_and_continued_input_share_one_document_and_history() -> Result<(), Box<dyn Error>>
{
    let mut doc = document("ab\n", 1)?;
    let document_id = doc.document_id().clone();

    doc.composition_update("你", 0)?;
    doc.composition_commit(1)?;
    // 确认后插入点位于已确认文本之后，继续普通输入进入同一文档与历史。
    assert_eq!(doc.current_selection(), SelectionSpec::new(4, 4));
    doc.type_text(4, "好", 2)?;
    doc.commit_open_block(3)?;
    assert_eq!(doc.buffer_text()?, "a你好b\n");
    assert_eq!(doc.document_id(), &document_id);
    assert_eq!((doc.undo_depth(), doc.redo_depth()), (2, 0));

    // 取消只回滚候选并恢复组合前选区，不产生第三处文本或历史步。
    doc.set_selection(SelectionSpec::new(1, 1))?;
    doc.composition_update("候", 4)?;
    doc.composition_cancel(5)?;
    assert_eq!(doc.buffer_text()?, "a你好b\n");
    assert_eq!(doc.current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.undo_depth(), 2);

    doc.undo()?;
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "ab\n");
    doc.redo()?;
    doc.redo()?;
    assert_eq!(doc.buffer_text()?, "a你好b\n");
    assert_eq!(
        doc.document_id(),
        &document_id,
        "整个生命周期只有一份文档身份"
    );
    Ok(())
}

#[test]
fn late_callbacks_after_target_change_are_rejected_without_side_effects()
-> Result<(), Box<dyn Error>> {
    let mut doc = document("abc\n\ndef\n", 1)?;
    let first_identity = doc.current_input_identity()?;
    doc.type_text(1, "中", 0)?;
    doc.commit_open_block(1)?;
    doc.leave_block(2)?;

    // 目标变化：关闭旧块后在另一顶层块打开新块。
    doc.open_block(5..8, SelectionSpec::new(6, 6))?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(6, 6));

    let moved = capture(&mut doc)?;
    assert!(matches!(
        doc.accept_derived_result(&first_identity),
        Err(DocError::StaleResult)
    ));
    assert_eq!(capture(&mut doc)?, moved, "旧目标回执不得写回新块");
    let current_identity = doc.current_input_identity()?;
    doc.accept_derived_result(&current_identity)?;

    // 迟到组合结束：取消后重复确认/取消都必须无副作用地拒绝。
    doc.composition_update("候", 3)?;
    doc.composition_cancel(4)?;
    let cancelled = capture(&mut doc)?;
    assert!(matches!(
        doc.composition_commit(5),
        Err(DocError::NoComposition)
    ));
    assert!(matches!(
        doc.composition_cancel(6),
        Err(DocError::NoComposition)
    ));
    assert_eq!(capture(&mut doc)?, cancelled);
    Ok(())
}

#[test]
fn leave_block_commits_pending_input_and_closes_the_target() -> Result<(), Box<dyn Error>> {
    let mut doc = document("ab\n", 1)?;
    let identity = doc.current_input_identity()?;
    doc.type_text(1, "中", 0)?;
    doc.leave_block(1)?;

    assert!(!doc.has_open_block(), "离开块必须封口并关闭目标");
    assert_eq!(doc.buffer_text()?, "a中b\n");
    assert_eq!(doc.undo_depth(), 1);
    assert!(matches!(
        doc.accept_derived_result(&identity),
        Err(DocError::ClosedTarget)
    ));
    Ok(())
}

struct Files(&'static [u8]);

impl ReadonlyFileAccess for Files {
    type Reader = Cursor<&'static [u8]>;

    fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        Ok(ReadonlyFileInput::new(
            Cursor::new(self.0),
            self.0.len() as u64,
            true,
        ))
    }
}

fn open_editable(bytes: &'static [u8]) -> Result<ReadonlySession, Box<dyn Error>> {
    let limits = ReadonlyOpenLimits::new(
        StorageBudget::new(4096),
        DocLimits::new(
            TextBudget::new(4096),
            LineIndexBudget::new(128),
            HistoryBudget::new(16, 4096),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )
    .with_editing();
    let mut session = ReadonlySession::new(limits);
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
        .ok_or("missing open job")?;
    assert!(matches!(
        session.accept_open(job.read_with(&Files(bytes))),
        OpenDisposition::Applied
    ));
    Ok(session)
}

#[test]
fn editable_session_close_is_ready_and_late_refresh_result_is_stale() -> Result<(), Box<dyn Error>>
{
    let mut session = open_editable(b"abc\n")?;
    let job = session.begin_parse_refresh()?;
    let document_id = session
        .document()
        .ok_or("missing document")?
        .core()
        .document_id()
        .clone();

    assert_eq!(session.close_document(), CloseDisposition::Closed);
    assert!(session.document().is_none());
    assert!(
        matches!(
            session.accept_parse_refresh(job.run()),
            ParseRefreshDisposition::Stale
        ),
        "关闭后的解析回执必须作废"
    );
    // 再次关闭是幂等的，不复活文档。
    assert_eq!(session.close_document(), CloseDisposition::Closed);
    assert!(session.document().is_none());

    // 重新打开产生新文档身份，不复用旧身份。
    let session = open_editable(b"abc\n")?;
    assert_ne!(
        session
            .document()
            .ok_or("missing reopened document")?
            .core()
            .document_id(),
        &document_id
    );
    Ok(())
}

#[test]
fn editable_session_mode_switch_keeps_zero_history_and_live_document() -> Result<(), Box<dyn Error>>
{
    let mut session = open_editable(b"abc\n")?;
    let document = session.document_mut().ok_or("missing document")?;
    let text = document.current_snapshot()?.full_text(TEXT)?;
    document.switch_mode(aline_ui::EditorMode::Source)?;
    document.switch_mode(aline_ui::EditorMode::Visual)?;

    let document = session.document_mut().ok_or("document preserved")?;
    assert_eq!(document.current_snapshot()?.full_text(TEXT)?, text);
    assert_eq!(document.core().history_node_count(), 0);
    assert_eq!(document.core().buffer_text()?, "abc\n");
    Ok(())
}
