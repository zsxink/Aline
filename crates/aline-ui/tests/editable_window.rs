//! M2 3.1 窗口级可编辑会话集成目标。
//!
//! 以只读文件适配器驱动 `ReadonlySession` 的窗口操作序列：可编辑打开、失败打开
//! 保留原文档、替换作废旧结果、统一 DocCore 输入语义、模式切换、文档/窗口关闭
//! 与跨会话迟到结果拒绝。真实 macOS 中文焦点/IME 与保存出口不在此目标取证。

use std::error::Error;
use std::io::{self, Cursor};
use std::path::{Path, PathBuf};

use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, SnapshotId, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use aline_ui::{
    CloseDisposition, CommitOutcome, DocCore, DocError, DocLimits, EditorMode, InputIdentity,
    ModeSwitchOutcome, OpenDisposition, OpenError, ParseRefreshDisposition, ReadonlyFileAccess,
    ReadonlyFileInput, ReadonlyOpenLimits, ReadonlyOpenPhase, ReadonlySession, WorkingVersion,
};

/// 与小规模夹具匹配的硬字节上限。
const MAX_BYTES: usize = 4096;

/// 固定返回同一段字节的读取适配器；不触碰真实文件系统。
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

/// 固定失败的读取适配器，用于验证失败打开保留原文档。
struct FailingFiles;

impl ReadonlyFileAccess for FailingFiles {
    type Reader = Cursor<&'static [u8]>;

    fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
        Err(io::Error::from(io::ErrorKind::NotFound))
    }
}

fn doc_limits() -> DocLimits {
    DocLimits::new(
        TextBudget::new(MAX_BYTES),
        LineIndexBudget::new(128),
        HistoryBudget::new(16, MAX_BYTES),
    )
}

fn limits() -> ReadonlyOpenLimits {
    ReadonlyOpenLimits::new(
        StorageBudget::new(MAX_BYTES),
        doc_limits(),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )
    .with_editing()
}

/// 走完整选择/读取/发布序列打开一个可编辑文档；非 Applied 直接判失败。
fn open(
    session: &mut ReadonlySession,
    name: &str,
    bytes: &'static [u8],
) -> Result<(), Box<dyn Error>> {
    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![PathBuf::from(name)]))?
        .ok_or("选择结果不应取消")?;
    assert!(
        matches!(
            session.accept_open(job.read_with(&Files(bytes))),
            OpenDisposition::Applied
        ),
        "可编辑打开 {name} 应成功发布"
    );
    Ok(())
}

fn history(core: &DocCore) -> (usize, usize, usize) {
    (
        core.undo_depth(),
        core.redo_depth(),
        core.history_node_count(),
    )
}

/// 窗口文档的完整可观察状态，用于证明失败/迟到结果零副作用。
#[derive(Debug, PartialEq)]
struct WindowState {
    path: PathBuf,
    text: String,
    selection: SelectionSpec,
    version: WorkingVersion,
    history: (usize, usize, usize),
    snapshot: SnapshotId,
    identity: InputIdentity,
}

fn window_state(session: &mut ReadonlySession) -> Result<WindowState, Box<dyn Error>> {
    let document = session.document_mut().ok_or("缺少当前文档")?;
    let snapshot = document.current_snapshot()?;
    let identity = document.input_identity()?;
    Ok(WindowState {
        path: document.path().to_path_buf(),
        text: document.core().buffer_text()?,
        selection: document.core().current_selection(),
        version: document.core().version(),
        history: history(document.core()),
        snapshot: snapshot.snapshot_id().clone(),
        identity,
    })
}

#[test]
fn editable_open_publishes_shared_core_without_history() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits());
    let request = session.request_open()?;
    assert_eq!(
        session.pending_phase(),
        Some(ReadonlyOpenPhase::Selecting),
        "打开意图应进入选择阶段"
    );
    let job = session
        .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
        .ok_or("选择结果不应取消")?;
    assert!(
        matches!(
            session.accept_open(job.read_with(&Files(b"# title\n\nbody"))),
            OpenDisposition::Applied
        ),
        "可编辑打开应发布候选"
    );

    let document = session.document().ok_or("缺少当前文档")?;
    assert!(document.is_editable(), "打开预算应启用可编辑会话");
    assert_eq!(document.path(), Path::new("fixture.md"));
    assert_eq!(document.core().buffer_text()?, "# title\n\nbody");
    assert_eq!(
        document.core().history_node_count(),
        0,
        "打开不得产生历史节点"
    );
    assert_eq!(document.core().undo_depth(), 0);
    assert_eq!(document.core().redo_depth(), 0);
    assert!(document.is_parse_current(), "打开解析应属于当前源码");
    Ok(())
}

#[test]
fn failed_open_keeps_the_previous_document_intact() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits());
    open(&mut session, "a.md", b"alpha\n\nbeta")?;
    let before = window_state(&mut session)?;

    let request = session.request_open()?;
    let job = session
        .accept_selection(&request, Some(vec![PathBuf::from("b.md")]))?
        .ok_or("选择结果不应取消")?;
    assert!(
        matches!(
            session.accept_open(job.read_with(&FailingFiles)),
            OpenDisposition::Failed(OpenError::Io(io::ErrorKind::NotFound))
        ),
        "读取失败应结算为 Failed 并保留原文档"
    );

    let after = window_state(&mut session)?;
    assert_eq!(after, before, "失败打开不得改变原文档状态");
    let document = session.document().ok_or("缺少当前文档")?;
    assert_eq!(document.path(), Path::new("a.md"), "路径应仍为 A");
    assert!(document.is_parse_current(), "A 的解析应仍然有效");
    Ok(())
}

#[test]
fn replacement_publishes_new_document_and_stales_old_refresh() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits());
    open(&mut session, "a.md", b"alpha")?;
    let stale = session.begin_parse_refresh()?.run();

    open(&mut session, "b.md", b"bravo\n\ncharlie")?;
    assert!(
        matches!(
            session.accept_parse_refresh(stale),
            ParseRefreshDisposition::Stale
        ),
        "替换后旧刷新结果必须拒绝"
    );

    let document = session.document().ok_or("缺少当前文档")?;
    assert_eq!(document.path(), Path::new("b.md"));
    assert_eq!(document.core().buffer_text()?, "bravo\n\ncharlie");
    assert_eq!(
        document.core().history_node_count(),
        0,
        "替换打开不产生历史"
    );
    Ok(())
}

#[test]
fn source_typing_commit_undo_and_checked_replacement_on_shared_core() -> Result<(), Box<dyn Error>>
{
    let mut core = DocCore::new("abc", SelectionSpec::new(1, 1), doc_limits())?;
    assert_eq!(
        core.switch_mode(EditorMode::Source, 0)?,
        ModeSwitchOutcome::Ready,
        "窗口输入路径消费的同一核心应能进入源码模式"
    );

    let identity = core.current_input_identity()?;
    core.type_selection(&identity, "汉", 1)?;
    assert_eq!(core.open_block_draft(), Some("汉"), "输入应先进入唯一草稿");
    assert_eq!(core.buffer_text()?, "abc", "提交前已提交 Buffer 不应变化");

    let before_commit = core.version();
    let outcome = core.commit_open_block(2)?;
    let committed = match outcome {
        CommitOutcome::Committed { revision } => revision,
        CommitOutcome::Unchanged => return Err("草稿提交不应为 Unchanged".into()),
    };
    assert_eq!(core.buffer_text()?, "a汉bc");
    assert_eq!(core.current_selection(), SelectionSpec::new(4, 4));
    assert_eq!(core.history_node_count(), 1);
    assert_eq!(core.undo_depth(), 1);
    assert_eq!(core.redo_depth(), 0);
    assert_eq!(core.open_block_draft(), None, "提交后草稿应清空");
    assert_eq!(core.version().revision(), committed);
    assert!(
        core.version().revision().value() > before_commit.revision().value(),
        "提交应推进已提交 revision"
    );

    core.undo()?;
    assert_eq!(core.buffer_text()?, "abc", "撤销应恢复输入前文本");
    assert_eq!(core.history_node_count(), 1);
    assert_eq!(core.undo_depth(), 0);
    assert_eq!(core.redo_depth(), 1);
    assert_eq!(core.current_selection(), SelectionSpec::new(1, 1));

    let identity = core.current_input_identity()?;
    let outcome = core.replace_selection(&identity, "XY", 3)?;
    assert!(
        matches!(outcome, CommitOutcome::Committed { .. }),
        "受检替换应提交单个历史节点"
    );
    assert_eq!(core.buffer_text()?, "aXYbc");
    assert_eq!(core.current_selection(), SelectionSpec::new(3, 3));
    assert_eq!(core.history_node_count(), 1, "撤销后新命令应截断重做分支");
    assert_eq!(core.undo_depth(), 1);
    assert_eq!(core.redo_depth(), 0);
    Ok(())
}

#[test]
fn mode_switch_keeps_text_and_history_and_stales_frozen_refresh() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits());
    open(&mut session, "fixture.md", b"title\n\nbody")?;

    let document = session.document_mut().ok_or("缺少当前文档")?;
    let text_before = document.core().buffer_text()?;
    let version_before = document.core().version();
    let history_before = history(document.core());
    assert_eq!(document.core().mode(), EditorMode::Visual);

    assert_eq!(
        document.switch_mode(EditorMode::Source)?,
        ModeSwitchOutcome::Ready,
        "源码切换应就绪"
    );
    assert_eq!(document.core().mode(), EditorMode::Source);
    assert_eq!(document.core().buffer_text()?, text_before);
    assert_eq!(history(document.core()), history_before);
    assert_eq!(document.core().version(), version_before);

    assert_eq!(
        document.switch_mode(EditorMode::Visual)?,
        ModeSwitchOutcome::Ready,
        "可视化切换应就绪"
    );
    assert_eq!(document.core().mode(), EditorMode::Visual);
    assert_eq!(document.core().buffer_text()?, text_before);
    assert_eq!(history(document.core()), history_before);

    let version_after_round_trip = document.core().version();
    assert_eq!(
        document.switch_mode(EditorMode::Visual)?,
        ModeSwitchOutcome::Ready,
        "同模式切换应直接就绪"
    );
    assert_eq!(
        document.core().version(),
        version_after_round_trip,
        "同模式切换不推进版本"
    );
    assert_eq!(
        history(document.core()),
        history_before,
        "同模式切换不改变历史"
    );

    let frozen = session.begin_parse_refresh()?.run();
    session
        .document_mut()
        .ok_or("缺少当前文档")?
        .switch_mode(EditorMode::Source)?;
    assert!(
        matches!(
            session.accept_parse_refresh(frozen),
            ParseRefreshDisposition::Stale
        ),
        "切换前冻结的刷新结果必须拒绝"
    );
    Ok(())
}

#[test]
fn close_document_and_window_reject_late_results_until_reopen() -> Result<(), Box<dyn Error>> {
    let mut session = ReadonlySession::new(limits());
    open(&mut session, "fixture.md", b"body")?;
    let late = session.begin_parse_refresh()?.run();

    assert_eq!(session.close_document(), CloseDisposition::Closed);
    assert!(session.document().is_none(), "关闭后不应保留文档");
    assert!(
        matches!(session.begin_parse_refresh(), Err(DocError::ClosedTarget)),
        "关闭后不得再发行刷新请求"
    );
    assert!(
        matches!(
            session.accept_parse_refresh(late),
            ParseRefreshDisposition::Stale
        ),
        "关闭后的旧刷新结果必须拒绝"
    );

    open(&mut session, "reopened.md", b"again")?;
    assert_eq!(
        session.document().ok_or("缺少重新打开的文档")?.path(),
        Path::new("reopened.md")
    );

    assert_eq!(session.close_window(), CloseDisposition::Closed);
    assert!(session.document().is_none());
    assert!(
        matches!(session.request_open(), Err(OpenError::ClosedWindow)),
        "关闭窗口后不得再发起打开"
    );
    assert!(session.document().is_none());
    Ok(())
}

#[test]
fn foreign_session_refresh_is_stale_without_target_change() -> Result<(), Box<dyn Error>> {
    let mut first = ReadonlySession::new(limits());
    let mut second = ReadonlySession::new(limits());
    open(&mut first, "first.md", b"first")?;
    open(&mut second, "second.md", b"second")?;

    let foreign = first.begin_parse_refresh()?.run();
    let before = window_state(&mut second)?;
    assert!(
        matches!(
            second.accept_parse_refresh(foreign),
            ParseRefreshDisposition::Stale
        ),
        "跨会话刷新结果必须拒绝"
    );
    let after = window_state(&mut second)?;
    assert_eq!(after, before, "跨会话迟到结果不得改变目标文档");
    Ok(())
}
