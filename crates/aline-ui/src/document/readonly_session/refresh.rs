//! 编辑后刷新阅读解析。会话持有采纳资格，后台任务只读取冻结输入。

use aline_buffer::TextSnapshot;
use aline_markdown::{
    CancelToken, ParseBudget, ParseConfig, ParseError, Parsed, parse_with_cancel,
};

use super::{DocError, InputIdentity, ReadonlyDocument, ReadonlySession};

pub(super) struct PendingParse {
    identity: InputIdentity,
    cancel: CancelToken,
}

/// 一份可交给后台执行的解析请求，绑定文档、源码快照、配置和请求代次。
pub struct ParseRefreshJob {
    identity: InputIdentity,
    snapshot: TextSnapshot,
    config: ParseConfig,
    budget: ParseBudget,
    cancel: CancelToken,
}

impl ParseRefreshJob {
    /// 返回请求身份，供宿主取消指定请求。
    pub fn identity(&self) -> &InputIdentity {
        &self.identity
    }

    /// 对冻结输入执行可合作取消的解析。宿主应将此工作安排到后台。
    pub fn run(self) -> ParseRefreshCompletion {
        let result = parse_with_cancel(&self.snapshot, self.config, self.budget, &self.cancel);
        ParseRefreshCompletion {
            identity: self.identity,
            result,
        }
    }
}

/// 后台解析完成结果，只能交回文档会话复验后采用。
pub struct ParseRefreshCompletion {
    identity: InputIdentity,
    result: Result<Parsed, ParseError>,
}

/// 解析请求的最终结算。只有 `Applied` 发布新的阅读解析。
#[derive(Debug)]
pub enum ParseRefreshDisposition {
    /// 当前请求的解析已采用。
    Applied,
    /// 请求被取消、替换，或相关文档、源码、模式或配置已经变化。
    Stale,
    /// 当前解析发生硬错误，上一份已采用的解析仍可展示。
    Failed(ParseError),
}

impl ReadonlySession {
    /// 冻结当前可编辑文档，发行新解析请求并取消上一个解析请求。
    pub fn begin_parse_refresh(&mut self) -> Result<ParseRefreshJob, DocError> {
        let document = self.document.as_mut().ok_or(DocError::ClosedTarget)?;
        if !document.is_editable() {
            return Err(DocError::ReadOnly);
        }
        let snapshot = document.current_snapshot()?;
        let identity = document.core.begin_document_request()?;
        document.cancel_pending_parse();
        let cancel = CancelToken::new();
        document.pending_parse = Some(PendingParse {
            identity: identity.clone(),
            cancel: cancel.clone(),
        });
        Ok(ParseRefreshJob {
            identity,
            snapshot: snapshot.text().clone(),
            config: self.limits.config,
            budget: self.limits.parse,
            cancel,
        })
    }

    /// 取消当前匹配的解析请求。旧身份不会取消较新的请求。
    pub fn cancel_parse_refresh(&mut self, identity: &InputIdentity) -> bool {
        let Some(document) = self.document.as_mut() else {
            return false;
        };
        if !document.matches_parse_request(identity) {
            return false;
        }
        document.cancel_pending_parse();
        true
    }

    /// 采用仍匹配当前文档的解析结果，供无窗口会话使用。
    pub fn accept_parse_refresh(
        &mut self,
        completion: ParseRefreshCompletion,
    ) -> ParseRefreshDisposition {
        match self
            .accept_parse_refresh_with(completion, |_, _| Ok::<(), std::convert::Infallible>(()))
        {
            Ok((disposition, _)) => disposition,
            Err(never) => match never {},
        }
    }

    /// 采用前先用同一候选准备阅读面。准备失败只结算请求，保留上一份解析。
    pub(crate) fn accept_parse_refresh_with<T, E>(
        &mut self,
        completion: ParseRefreshCompletion,
        prepare: impl FnOnce(&ReadonlyDocument, &Parsed) -> Result<T, E>,
    ) -> Result<(ParseRefreshDisposition, Option<T>), E> {
        use ParseRefreshDisposition::{Applied, Failed, Stale};

        let Some(document) = self.document.as_mut() else {
            return Ok((Stale, None));
        };
        let ParseRefreshCompletion { identity, result } = completion;
        if !document.matches_parse_request(&identity) {
            return Ok((Stale, None));
        }
        document.pending_parse = None;
        if document.core.accept_document_result(&identity).is_err() {
            return Ok((Stale, None));
        }
        let parsed = match result {
            Ok(parsed) => parsed,
            Err(ParseError::Cancelled) => return Ok((Stale, None)),
            Err(error) => return Ok((Failed(error), None)),
        };
        if parsed.snapshot().id() != identity.snapshot_id()
            || parsed.projection().snapshot_id() != identity.snapshot_id()
            || parsed.config() != self.limits.config
        {
            return Ok((Stale, None));
        }
        let prepared = prepare(document, &parsed)?;
        document.parsed = parsed;
        document.parsed_identity = identity;
        Ok((Applied, Some(prepared)))
    }
}

impl ReadonlyDocument {
    fn matches_parse_request(&self, identity: &InputIdentity) -> bool {
        self.pending_parse
            .as_ref()
            .is_some_and(|pending| &pending.identity == identity)
    }

    pub(super) fn cancel_pending_parse(&mut self) {
        if let Some(pending) = self.pending_parse.take() {
            pending.cancel.cancel();
        }
    }
}

impl Drop for ReadonlyDocument {
    fn drop(&mut self) {
        self.cancel_pending_parse();
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::io::{self, Cursor};
    use std::path::{Path, PathBuf};

    use aline_buffer::{
        BufferError, HistoryBudget, LineIndexBudget, SelectionSpec, SrcSelection, TextBudget,
    };
    use aline_source::StorageBudget;

    use super::super::{
        OpenDisposition, ReadonlyFileAccess, ReadonlyFileInput, ReadonlyOpenLimits,
    };
    use super::*;
    use crate::{DocLimits, EditorMode};

    struct Fixture(&'static str);

    impl ReadonlyFileAccess for Fixture {
        type Reader = Cursor<&'static [u8]>;

        fn open(&self, _: &Path) -> io::Result<ReadonlyFileInput<Self::Reader>> {
            let bytes = self.0.as_bytes();
            Ok(ReadonlyFileInput::new(
                Cursor::new(bytes),
                bytes.len() as u64,
                true,
            ))
        }
    }

    fn open(text: &'static str, budget: ParseBudget) -> Result<ReadonlySession, Box<dyn Error>> {
        open_with_history(text, budget, HistoryBudget::new(16, 4096))
    }

    fn open_with_history(
        text: &'static str,
        budget: ParseBudget,
        history: HistoryBudget,
    ) -> Result<ReadonlySession, Box<dyn Error>> {
        let mut session = ReadonlySession::new(
            ReadonlyOpenLimits::new(
                StorageBudget::new(4096),
                DocLimits::new(TextBudget::new(4096), LineIndexBudget::new(128), history),
                ParseConfig::m0(),
                budget,
            )
            .with_editing(),
        );
        let request = session.request_open()?;
        let job = session
            .accept_selection(&request, Some(vec![PathBuf::from("fixture.md")]))?
            .ok_or("missing job")?;
        assert!(matches!(
            session.accept_open(job.read_with(&Fixture(text))),
            OpenDisposition::Applied
        ));
        Ok(session)
    }

    fn assert_parsed_text(session: &ReadonlySession, expected: &str) -> Result<(), Box<dyn Error>> {
        let doc = session.document().ok_or("missing document")?;
        let snapshot = doc.parsed().snapshot();
        let range = snapshot.src_range(0..snapshot.len_bytes())?;
        assert_eq!(snapshot.slice(&range, doc.limits.text())?, expected);
        Ok(())
    }

    fn refresh(session: &mut ReadonlySession) -> Result<(), Box<dyn Error>> {
        let job = session.begin_parse_refresh()?;
        assert!(matches!(
            session.accept_parse_refresh(job.run()),
            ParseRefreshDisposition::Applied
        ));
        assert!(
            session
                .document()
                .is_some_and(ReadonlyDocument::is_parse_current)
        );
        Ok(())
    }

    #[test]
    fn refresh_rebinds_unicode_multiline_draft_commit_undo_and_redo() -> Result<(), Box<dyn Error>>
    {
        let mut session = open("a\nb", ParseBudget::m0())?;
        let doc = session.document_mut().ok_or("missing document")?;
        doc.switch_mode(EditorMode::Source)?;
        let identity = doc.input_identity()?;
        doc.core.type_selection(&identity, "汉\n🙂\n", 1)?;
        assert!(!doc.is_parse_current());
        assert_eq!(doc.core.history_node_count(), 0);

        let draft_job = session.begin_parse_refresh()?;
        let draft_snapshot = draft_job.identity().snapshot_id().clone();
        assert!(matches!(
            session.accept_parse_refresh(draft_job.run()),
            ParseRefreshDisposition::Applied
        ));
        assert_parsed_text(&session, "汉\n🙂\na\nb")?;

        let doc = session.document_mut().ok_or("missing document")?;
        assert_eq!(doc.parsed().snapshot().id(), &draft_snapshot);
        doc.core.commit_open_block(2)?;
        assert!(!doc.is_parse_current());
        refresh(&mut session)?;
        assert_parsed_text(&session, "汉\n🙂\na\nb")?;
        let doc = session.document_mut().ok_or("missing document")?;
        assert_ne!(doc.parsed().snapshot().id(), &draft_snapshot);
        assert_eq!(doc.core.history_node_count(), 1);
        doc.core.undo()?;
        assert!(!doc.is_parse_current());
        refresh(&mut session)?;
        assert_parsed_text(&session, "a\nb")?;
        session
            .document_mut()
            .ok_or("missing document")?
            .core
            .redo()?;
        refresh(&mut session)?;
        assert_parsed_text(&session, "汉\n🙂\na\nb")
    }

    fn caret(doc: &ReadonlyDocument, src_off: usize) -> Result<SrcSelection, BufferError> {
        let target = doc.parsed().snapshot().src_offset(src_off)?;
        SrcSelection::new(target.clone(), target)
    }

    fn open_first_block(doc: &mut ReadonlyDocument) -> Result<InputIdentity, Box<dyn Error>> {
        let identity = doc.input_identity()?;
        assert!(doc.open_visual_at(&identity, &caret(doc, 1)?)?);
        Ok(identity)
    }

    fn start_visual_typing(session: &mut ReadonlySession) -> Result<(), Box<dyn Error>> {
        let doc = session.document_mut().ok_or("missing document")?;
        let identity = open_first_block(doc)?;
        doc.core.type_selection(&identity, "可", 1)?;
        refresh(session)?;
        Ok(())
    }

    #[test]
    fn visual_click_commits_typing_and_preserves_group_history_selection()
    -> Result<(), Box<dyn Error>> {
        let mut session = open("abc\n\nxyz", ParseBudget::m0())?;
        start_visual_typing(&mut session)?;

        let doc = session.document_mut().ok_or("missing document")?;
        let end_of_group = SelectionSpec::new(6, 1);
        doc.core.set_selection(end_of_group)?;
        let identity = doc.input_identity()?;
        let target = caret(doc, 8)?;
        assert!(doc.open_visual_at(&identity, &target)?);
        assert_eq!(doc.core.buffer_text()?, "a可bc\n\nxyz");
        assert_eq!(doc.core.history_node_count(), 1);
        assert_eq!(doc.core.current_selection(), SelectionSpec::new(8, 8));
        assert_eq!(doc.core.buffer_selection(), SelectionSpec::new(8, 8));
        assert!(doc.core.open_block_draft().is_none());
        assert!(!doc.is_parse_current());
        let current = doc.current_snapshot()?;
        assert_eq!(
            doc.core
                .visual_block_range(&current)?
                .ok_or("target block")?
                .byte_range(),
            8..11
        );
        doc.core.undo()?;
        assert_eq!(doc.core.buffer_text()?, "abc\n\nxyz");
        assert_eq!(doc.core.current_selection(), SelectionSpec::new(1, 1));
        doc.core.redo()?;
        assert_eq!(doc.core.buffer_text()?, "a可bc\n\nxyz");
        assert_eq!(doc.core.current_selection(), end_of_group);
        Ok(())
    }

    #[test]
    fn visual_navigation_commits_typing_without_opening_the_target() -> Result<(), Box<dyn Error>> {
        let mut session = open("abc\n\nxyz", ParseBudget::m0())?;
        start_visual_typing(&mut session)?;
        let doc = session.document_mut().ok_or("missing document")?;
        let identity = doc.input_identity()?;
        doc.navigate(&identity, &caret(doc, 8)?)?;
        assert_eq!(doc.core.buffer_text()?, "a可bc\n\nxyz");
        assert_eq!(doc.core.history_node_count(), 1);
        assert_eq!(doc.core.current_selection(), SelectionSpec::new(8, 8));
        assert!(!doc.core.has_open_block());
        Ok(())
    }

    #[derive(Debug, PartialEq, Eq)]
    struct RetargetState {
        identity: InputIdentity,
        buffer_text: String,
        buffer_selection: SelectionSpec,
        selection: SelectionSpec,
        target: std::ops::Range<usize>,
        draft: String,
        history: (usize, usize, usize),
    }

    fn retarget_state(doc: &mut ReadonlyDocument) -> Result<RetargetState, Box<dyn Error>> {
        let current = doc.current_snapshot()?;
        Ok(RetargetState {
            identity: doc.input_identity()?,
            buffer_text: doc.core.buffer_text()?,
            buffer_selection: doc.core.buffer_selection(),
            selection: doc.core.current_selection(),
            target: doc
                .core
                .visual_block_range(&current)?
                .ok_or("target block")?
                .byte_range(),
            draft: doc.core.open_block_draft().ok_or("draft")?.to_owned(),
            history: (
                doc.core.history_node_count(),
                doc.core.undo_depth(),
                doc.core.redo_depth(),
            ),
        })
    }

    #[test]
    fn visual_retarget_preserves_draft_when_target_or_commit_is_rejected()
    -> Result<(), Box<dyn Error>> {
        let mut session =
            open_with_history("abc\n\nxyz", ParseBudget::m0(), HistoryBudget::new(0, 0))?;
        let doc = session.document_mut().ok_or("missing document")?;
        let old_identity = doc.input_identity()?;
        let old_caret = caret(doc, 5)?;
        start_visual_typing(&mut session)?;
        let doc = session.document_mut().ok_or("missing document")?;
        let before = retarget_state(doc)?;
        let target = caret(doc, 8)?;
        assert!(matches!(
            doc.open_visual_at(&old_identity, &old_caret),
            Err(DocError::StaleResult)
        ));
        assert!(matches!(
            doc.open_visual_at(&before.identity, &old_caret),
            Err(DocError::Buffer(BufferError::SnapshotMismatch))
        ));
        assert!(matches!(
            doc.open_visual_at(&before.identity, &target),
            Err(DocError::Buffer(BufferError::HistoryBudgetExceeded { .. }))
        ));
        assert!(matches!(
            doc.navigate(&before.identity, &target),
            Err(DocError::Buffer(BufferError::HistoryBudgetExceeded { .. }))
        ));
        assert_eq!(retarget_state(doc)?, before);
        Ok(())
    }

    #[test]
    fn visual_retarget_and_escape_wait_for_the_platform_composition() -> Result<(), Box<dyn Error>>
    {
        let mut session = open("abc\n\nxyz", ParseBudget::m0())?;
        let doc = session.document_mut().ok_or("missing document")?;
        open_first_block(doc)?;
        doc.core.composition_start("候", 1)?;
        refresh(&mut session)?;
        let doc = session.document_mut().ok_or("missing document")?;
        let before = retarget_state(doc)?;
        let target = caret(doc, 8)?;
        assert!(matches!(
            doc.open_visual_at(&before.identity, &target),
            Err(DocError::CompositionActive)
        ));
        assert!(matches!(
            doc.navigate(&before.identity, &target),
            Err(DocError::CompositionActive)
        ));
        assert!(matches!(
            doc.core.escape(2),
            Err(DocError::CompositionActive)
        ));
        assert_eq!(retarget_state(doc)?, before);
        assert_eq!(
            doc.core.composition().ok_or("composition")?.candidate(),
            "候"
        );

        doc.core.composition_cancel(3)?;
        let identity = doc.input_identity()?;
        doc.core.type_selection(&identity, "可", 4)?;
        doc.core.escape(5)?;
        assert_eq!(doc.core.buffer_text()?, "a可bc\n\nxyz");
        assert_eq!(doc.core.history_node_count(), 1);
        assert_eq!(doc.core.current_selection(), SelectionSpec::new(4, 4));
        assert!(!doc.core.has_open_block());
        Ok(())
    }

    #[test]
    fn visual_click_on_same_or_other_block_waits_for_platform_composition()
    -> Result<(), Box<dyn Error>> {
        // 这是 `open_visual_at` 的文档层 API 契约测试，不证明窗口点击路径可达：
        // 窗口在组合期不刷新解析，因此也不会挂载行级命中回调。此处显式
        // `refresh` 只为构造可调用 `open_visual_at` 的文档状态。
        let mut session = open("abc\n\nxyz", ParseBudget::m0())?;
        let doc = session.document_mut().ok_or("missing document")?;
        open_first_block(doc)?;
        doc.core.composition_start("候", 1)?;
        refresh(&mut session)?;
        let doc = session.document_mut().ok_or("missing document")?;
        let before = retarget_state(doc)?;

        // 同块点击在 `open_visual_at` 内走 `core.select`；跨块走 `retarget_visual`。
        // 两条路径都必须在组合活动时拒绝，且不推进版本、选区、草稿、候选或历史。
        let same = caret(doc, 1)?;
        assert!(matches!(
            doc.open_visual_at(&before.identity, &same),
            Err(DocError::CompositionActive)
        ));
        assert_eq!(retarget_state(doc)?, before);
        assert_eq!(
            doc.core.composition().ok_or("composition")?.candidate(),
            "候"
        );

        let other = caret(doc, 8)?;
        assert!(matches!(
            doc.open_visual_at(&before.identity, &other),
            Err(DocError::CompositionActive)
        ));
        assert_eq!(retarget_state(doc)?, before);
        assert_eq!(
            doc.core.composition().ok_or("composition")?.candidate(),
            "候"
        );
        Ok(())
    }

    #[test]
    fn refresh_keeps_last_view_after_hard_parse_error_and_stale_edit() -> Result<(), Box<dyn Error>>
    {
        let mut session = open("ab", ParseBudget::m0().with_max_source_bytes(4))?;
        session
            .document_mut()
            .ok_or("missing document")?
            .switch_mode(EditorMode::Source)?;
        let old_job = session.begin_parse_refresh()?;
        let doc = session.document_mut().ok_or("missing document")?;
        let projection = doc.parsed().projection().id().clone();
        let identity = doc.input_identity()?;
        doc.core.type_selection(&identity, "汉", 1)?;
        assert!(matches!(
            session.accept_parse_refresh(old_job.run()),
            ParseRefreshDisposition::Stale
        ));
        let failed = session.begin_parse_refresh()?.run();
        assert!(matches!(
            session.accept_parse_refresh(failed),
            ParseRefreshDisposition::Failed(ParseError::Source(BufferError::BudgetExceeded {
                needed: 5,
                limit: 4
            }))
        ));
        let doc = session.document_mut().ok_or("missing document")?;
        assert_eq!(doc.parsed().projection().id(), &projection);
        assert_eq!(doc.core.open_block_draft(), Some("汉"));
        assert!(!doc.is_parse_current());
        let old_caret = doc.parsed().snapshot().src_offset(1)?;
        let old_selection = SrcSelection::new(old_caret.clone(), old_caret)?;
        let identity = doc.input_identity()?;
        let before = doc.core.current_selection();
        assert!(matches!(
            doc.navigate(&identity, &old_selection),
            Err(DocError::StaleResult)
        ));
        assert_eq!(doc.core.current_selection(), before);
        Ok(())
    }

    #[test]
    fn refresh_rejects_old_config_generation_and_mismatched_payload() -> Result<(), Box<dyn Error>>
    {
        let mut session = open("abc", ParseBudget::m0())?;
        let old_config = session.begin_parse_refresh()?.run();
        let doc = session.document_mut().ok_or("missing document")?;
        let projection = doc.parsed().projection().id().clone();
        doc.core.advance_parse_config()?;
        assert!(!doc.is_parse_current());
        assert!(matches!(
            session.accept_parse_refresh(old_config),
            ParseRefreshDisposition::Stale
        ));

        let mut wrong_config = session.begin_parse_refresh()?;
        wrong_config.config = ParseConfig::new(2, ParseConfig::m0().features());
        assert!(matches!(
            session.accept_parse_refresh(wrong_config.run()),
            ParseRefreshDisposition::Stale
        ));
        let mut wrong_snapshot = session.begin_parse_refresh()?;
        wrong_snapshot.snapshot = TextSnapshot::new("abc".to_owned(), TextBudget::new(32))?;
        assert!(matches!(
            session.accept_parse_refresh(wrong_snapshot.run()),
            ParseRefreshDisposition::Stale
        ));
        let doc = session.document().ok_or("missing document")?;
        assert_eq!(doc.parsed().projection().id(), &projection);
        refresh(&mut session)
    }
}
