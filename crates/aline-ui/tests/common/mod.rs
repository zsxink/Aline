//! 组 8 状态表测试的共享夹具与逐列断言。
//!
//! 本模块只使用 `aline-ui` 与 `aline-buffer` 的公开 API；虚拟毫秒由测试显式
//! 传入，不依赖窗口、线程或真实输入法。

#![allow(dead_code)]

use std::ops::Range;

use aline_buffer::{DocumentRevision, HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
use aline_ui::{
    CommitOutcome, DocCore, DocError, DocLimits, DocumentId, DraftGeneration, InputIdentity,
    PublicationKind, PublicationResult, WaitHandle, WaitStatus, WorkingVersion,
};

/// 测试统一注入的文本预算。
pub const TEXT: TextBudget = TextBudget::new(1 << 20);
/// 测试统一注入的行索引预算。
pub const INDEX: LineIndexBudget = LineIndexBudget::new(4096);
/// 测试统一注入的历史预算。
pub const HISTORY: HistoryBudget = HistoryBudget::new(4096, 1 << 20);

/// 返回测试统一注入的预算集合。
pub fn limits() -> DocLimits {
    DocLimits::new(TEXT, INDEX, HISTORY)
}

/// 构造折叠选区。
pub fn collapsed(at: usize) -> SelectionSpec {
    SelectionSpec::new(at, at)
}

/// 构造定向选区。
pub fn selection(anchor: usize, head: usize) -> SelectionSpec {
    SelectionSpec::new(anchor, head)
}

/// 以给定文本与折叠插入点创建文档。
pub fn new_doc(text: &str, at: usize) -> Result<DocCore, DocError> {
    DocCore::new(text, collapsed(at), limits())
}

/// 取一次当前快照。
pub fn snapshot(doc: &mut DocCore) -> Result<aline_ui::CurrentSnapshot, DocError> {
    doc.current_snapshot(TEXT)
}

/// 取当前快照全文。
pub fn snapshot_text(doc: &mut DocCore) -> Result<String, DocError> {
    let current = snapshot(doc)?;
    current.full_text(TEXT).map_err(DocError::Buffer)
}

/// 取当前快照身份，用于 `snapshot_id` 列。
pub fn snapshot_id(doc: &mut DocCore) -> Result<aline_buffer::SnapshotId, DocError> {
    Ok(snapshot(doc)?.snapshot_id().clone())
}

/// 断言已提交 Buffer 文本。
pub fn assert_buffer(doc: &DocCore, expected: &str) -> Result<(), DocError> {
    assert_eq!(doc.buffer_text()?, expected);
    Ok(())
}

/// 断言当前快照全文。
pub fn assert_snapshot(doc: &mut DocCore, expected: &str) -> Result<(), DocError> {
    assert_eq!(snapshot_text(doc)?, expected);
    Ok(())
}

/// 断言 revision 与草稿代次。
pub fn assert_version(doc: &DocCore, revision: u64, draft: u64) {
    assert_eq!(doc.version().revision().value(), revision);
    assert_eq!(doc.version().draft_generation().value(), draft);
}

/// 只断言 revision；用于 dg 在 TSV 中为 `待定` 的用例。
pub fn assert_revision(doc: &DocCore, revision: u64) {
    assert_eq!(doc.version().revision().value(), revision);
}

/// 断言 undo/redo 深度与历史节点数。
pub fn assert_history(doc: &DocCore, undo: usize, redo: usize, nodes: usize) {
    assert_eq!(doc.undo_depth(), undo);
    assert_eq!(doc.redo_depth(), redo);
    assert_eq!(doc.history_node_count(), nodes);
}

/// 断言打开块基线、草稿与逻辑选区。
pub fn assert_open(doc: &DocCore, baseline: u64, draft: Option<&str>, anchor: usize, head: usize) {
    assert!(doc.has_open_block());
    assert_eq!(
        doc.open_block_baseline().map(DocumentRevision::value),
        Some(baseline)
    );
    assert_eq!(doc.open_block_draft(), draft);
    assert!(doc.open_block_selection().is_some());
    if let Some(selection) = doc.open_block_selection() {
        assert_eq!(selection.anchor(), anchor);
        assert_eq!(selection.head(), head);
    }
}

/// 断言块已关闭且没有隐藏的第二草稿槽。
pub fn assert_closed(doc: &DocCore) {
    assert!(!doc.has_open_block());
    assert!(doc.open_block_baseline().is_none());
    assert!(doc.open_block_range().is_none());
    assert!(doc.open_block_selection().is_none());
    assert!(doc.open_block_draft().is_none());
    assert!(doc.composition().is_none());
}

/// 断言组合视图（范围与候选）。
pub fn assert_composition(doc: &DocCore, expected: Option<(Range<usize>, &str)>) {
    let actual = doc
        .composition()
        .map(|view| (view.range(), view.candidate().to_owned()));
    let wanted = expected.map(|(range, candidate)| (range, candidate.to_owned()));
    assert_eq!(actual, wanted, "组合状态不一致");
}

/// 断言发布等待视图（种类与状态）。
pub fn assert_wait(doc: &DocCore, expected: Option<(PublicationKind, WaitStatus)>) {
    let actual = doc.wait().map(|view| (view.kind(), view.status()));
    assert_eq!(actual, expected, "等待状态不一致");
}

/// 断言提交结果。
pub fn assert_committed(outcome: CommitOutcome, revision: u64) {
    assert_eq!(
        outcome,
        CommitOutcome::Committed {
            revision: DocumentRevision::from_value(revision)
        }
    );
}

/// 由当前快照与工作版本构造身份信封。
pub fn current_identity(doc: &mut DocCore) -> Result<InputIdentity, DocError> {
    doc.current_input_identity()
}

/// 由指定文档身份、工作版本与当前文本快照构造身份信封。
pub fn identity_for(
    document: DocumentId,
    version: WorkingVersion,
    snapshot: &aline_buffer::TextSnapshot,
) -> Result<InputIdentity, DocError> {
    InputIdentity::new(document, version, snapshot).map_err(DocError::Identity)
}

/// 断言发布结果并返回其中的快照。
pub fn publication_snapshot(
    result: PublicationResult,
    kind: PublicationKind,
    outcome: aline_ui::PublicationOutcome,
) -> aline_ui::CurrentSnapshot {
    assert_eq!(result.kind(), kind);
    assert_eq!(result.outcome(), outcome);
    result.snapshot().clone()
}

/// 断言两个快照身份不同（TSV `snapshot_id=new`）。
pub fn assert_new_snapshot(before: &aline_buffer::SnapshotId, after: &aline_buffer::SnapshotId) {
    assert_ne!(before, after);
}

/// 断言两个快照身份相同（TSV `snapshot_id=same`）。
pub fn assert_same_snapshot(before: &aline_buffer::SnapshotId, after: &aline_buffer::SnapshotId) {
    assert_eq!(before, after);
}

/// 断言等待句柄可比较且只暴露单调编号。
pub fn assert_handle(handle: WaitHandle) {
    assert!(handle.value() >= 1);
}

/// 断言不存在组合与发布等待（TSV 的 `composition=none`、`wait=none`）。
pub fn assert_idle(doc: &DocCore) {
    assert_composition(doc, None);
    assert_wait(doc, None);
}

/// 断言 Buffer 选区（块关闭时的逻辑选区）。
pub fn assert_buffer_selection(doc: &DocCore, anchor: usize, head: usize) {
    let selection = doc.buffer_selection();
    assert_eq!((selection.anchor(), selection.head()), (anchor, head));
}

/// 一列完整可观察状态，供拒绝用例比较“零副作用”。
///
/// 其中 `draft_generation` 在 TSV 为 `待定` 的行也会被捕获；比较前后相等
/// 只证明调用没有改变该值，不把 `待定` 列当作具体期望值。
#[derive(Debug, PartialEq)]
pub struct State {
    pub buffer_text: String,
    pub snapshot_text: Option<String>,
    pub snapshot_id: Option<aline_buffer::SnapshotId>,
    pub resources: Vec<aline_ui::ResourceReference>,
    pub mode: aline_ui::EditorMode,
    pub next_wait_id: u64,
    pub parse_config: aline_ui::ParseConfigGeneration,
    pub request: aline_ui::DerivedRequestId,
    pub revision: u64,
    pub draft_generation: u64,
    pub undo: usize,
    pub redo: usize,
    pub nodes: usize,
    pub buffer_anchor: usize,
    pub buffer_head: usize,
    pub open: Option<OpenState>,
    pub composition: Option<(Range<usize>, String)>,
    pub wait: Option<(u64, PublicationKind, WaitStatus)>,
}

/// 打开块的可观察状态。
#[derive(Debug, PartialEq)]
pub struct OpenState {
    pub baseline: u64,
    pub range: Option<Range<usize>>,
    pub baseline_fragment: Option<String>,
    pub draft: Option<String>,
    pub anchor: usize,
    pub head: usize,
}

/// 捕获当前完整可观察状态；快照被拒绝时记为 `None`。
pub fn capture(doc: &mut DocCore) -> Result<State, DocError> {
    let buffer_text = doc.buffer_text()?;
    let (snapshot_text, snapshot_id) = match doc.current_snapshot(TEXT) {
        Ok(current) => (
            Some(current.full_text(TEXT).map_err(DocError::Buffer)?),
            Some(current.snapshot_id().clone()),
        ),
        Err(_) => (None, None),
    };
    let buffer_selection = doc.buffer_selection();
    let open = doc.open_block_selection().map(|selection| OpenState {
        baseline: doc
            .open_block_baseline()
            .map(DocumentRevision::value)
            .unwrap_or_default(),
        range: doc.open_block_range(),
        baseline_fragment: doc.open_block_baseline_fragment().map(str::to_owned),
        draft: doc.open_block_draft().map(str::to_owned),
        anchor: selection.anchor(),
        head: selection.head(),
    });
    let composition = doc.composition().map(|view| {
        let range = view.range();
        (range, view.candidate().to_owned())
    });
    let wait = doc
        .wait()
        .map(|view| (view.handle().value(), view.kind(), view.status()));
    Ok(State {
        buffer_text,
        snapshot_text,
        snapshot_id,
        resources: doc.resources().to_vec(),
        mode: doc.mode(),
        next_wait_id: doc.next_wait_id(),
        parse_config: doc.parse_config_generation(),
        request: doc.derived_request_id(),
        revision: doc.version().revision().value(),
        draft_generation: doc.version().draft_generation().value(),
        undo: doc.undo_depth(),
        redo: doc.redo_depth(),
        nodes: doc.history_node_count(),
        buffer_anchor: buffer_selection.anchor(),
        buffer_head: buffer_selection.head(),
        open,
        composition,
        wait,
    })
}

/// 断言一次调用被拒绝且完整状态保持不变。
pub fn assert_rejected_without_change(
    doc: &mut DocCore,
    operation: impl FnOnce(&mut DocCore) -> Result<(), DocError>,
    expected: impl FnOnce(&DocError) -> bool,
) -> Result<State, DocError> {
    let before = capture(doc)?;
    let error = operation(doc);
    assert!(error.is_err());
    if let Err(error) = error {
        assert!(expected(&error), "拒绝原因不符: {error:?}");
    }
    let after = capture(doc)?;
    assert_eq!(before, after);
    Ok(after)
}

/// 断言当前工作版本的身份值为给定值。
pub fn working_version(revision: u64, draft: u64) -> WorkingVersion {
    WorkingVersion::new(
        DocumentRevision::from_value(revision),
        DraftGeneration::from_value(draft),
    )
}
