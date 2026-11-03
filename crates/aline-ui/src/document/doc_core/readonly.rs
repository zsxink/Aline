//! 只读能力与文档级派生目标；状态仍由 DocCore 唯一持有。
//!
//! 本模块消费当前源码/身份，提供不开块构造、受检导航、模式与关闭边界。
//! 不调度任务、不写磁盘，也不把文档存活等同于编辑块存活。

use super::{DocCore, DocError, DocLimits, INITIAL_WAIT_ID};
use crate::document::{
    DerivedRequestId, DocumentId, DraftGeneration, EditorMode, InputIdentity,
    ParseConfigGeneration, WorkingVersion,
};
use aline_buffer::{SelectionSpec, SrcSelection, StringBuffer};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DocumentAccess {
    Editing,
    Readonly,
}

impl DocCore {
    /// 从完整源码建立只读文档；直接使用空打开槽、空草稿/组合及空历史。
    ///
    /// 初始选区必须位于源码 UTF-8 边界；失败不返回部分实例。
    pub fn new_readonly(
        text: impl Into<String>,
        selection: SelectionSpec,
        limits: DocLimits,
    ) -> Result<Self, DocError> {
        let buffer = StringBuffer::new(
            text.into(),
            selection,
            limits.text,
            limits.index,
            limits.history,
        )
        .map_err(DocError::Buffer)?;
        Ok(Self {
            document: DocumentId::new(),
            version: WorkingVersion::new(buffer.revision(), DraftGeneration::initial()),
            buffer,
            open: None,
            wait: None,
            next_wait_id: INITIAL_WAIT_ID,
            limits,
            snapshot_cache: None,
            resources: Arc::from([]),
            parse_config: ParseConfigGeneration::initial(),
            derived_request: DerivedRequestId::initial(),
            mode: EditorMode::Visual,
            access: DocumentAccess::Readonly,
            alive: true,
        })
    }

    /// 判断文档是否具有只读阶段能力；不由有无编辑块推导。
    pub const fn is_readonly(&self) -> bool {
        matches!(self.access, DocumentAccess::Readonly)
    }

    /// 判断文档会话目标是否仍存活；关闭编辑块不关闭此目标。
    pub const fn is_document_alive(&self) -> bool {
        self.alive
    }

    /// 用当前文档结果身份及同快照选区导航；不打开块、不推进文本版本或历史。
    ///
    /// 旧文档/请求/配置/工作版本、旧快照或非法端点在更改选区前拒绝。
    pub fn navigate_readonly(
        &mut self,
        identity: &InputIdentity,
        selection: &SrcSelection,
    ) -> Result<(), DocError> {
        self.ensure_readonly()?;
        self.accept_document_result(identity)?;
        self.buffer
            .set_selection(selection)
            .map_err(DocError::Buffer)?;
        self.snapshot_cache = None;
        Ok(())
    }

    /// 为文档级解析发放新请求；没有打开块的存活文档仍可请求。
    pub fn begin_document_request(&mut self) -> Result<InputIdentity, DocError> {
        self.ensure_document_alive()?;
        let next = self.derived_request.next().map_err(DocError::Identity)?;
        let snapshot = self.current_snapshot(self.limits.text)?;
        let identity = InputIdentity::for_request(
            self.document.clone(),
            self.version,
            snapshot.text(),
            self.parse_config,
            next,
        )
        .map_err(DocError::Identity)?;
        self.derived_request = next;
        Ok(identity)
    }

    /// 复验文档级派生结果的完整身份；不要求编辑块存活，不改变文档状态。
    pub fn accept_document_result(&mut self, identity: &InputIdentity) -> Result<(), DocError> {
        self.ensure_document_alive()?;
        self.validate_result_identity(identity)
    }

    /// 关闭只读文档并失效旧请求；不生成补丁、历史或发布等待。
    ///
    /// 请求身份耗尽在关闭前拒绝，原会话保持存活；重复关闭返回 ClosedTarget。
    pub fn close_readonly(&mut self) -> Result<(), DocError> {
        self.ensure_readonly()?;
        let next = self.derived_request.next().map_err(DocError::Identity)?;
        self.derived_request = next;
        self.alive = false;
        self.snapshot_cache = None;
        self.resources = Arc::from([]);
        Ok(())
    }

    pub(super) fn ensure_document_alive(&self) -> Result<(), DocError> {
        if !self.alive {
            return Err(DocError::ClosedTarget);
        }
        Ok(())
    }

    pub(super) fn ensure_writable(&self) -> Result<(), DocError> {
        self.ensure_document_alive()?;
        if self.is_readonly() {
            return Err(DocError::ReadOnly);
        }
        Ok(())
    }

    fn ensure_readonly(&self) -> Result<(), DocError> {
        self.ensure_document_alive()?;
        if !self.is_readonly() {
            return Err(DocError::RequiresReadonly);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::IdentityError;
    use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};

    fn fixture() -> Result<DocCore, DocError> {
        let mut doc = DocCore::new_readonly(
            "x",
            SelectionSpec::new(0, 0),
            DocLimits::new(
                TextBudget::new(1024),
                LineIndexBudget::new(32),
                HistoryBudget::new(32, 1024),
            ),
        )?;
        doc.derived_request = DerivedRequestId::exhausted_for_test();
        Ok(doc)
    }

    fn state(
        doc: &DocCore,
    ) -> (
        bool,
        EditorMode,
        DerivedRequestId,
        WorkingVersion,
        SelectionSpec,
        usize,
    ) {
        (
            doc.alive,
            doc.mode,
            doc.derived_request,
            doc.version,
            doc.current_selection(),
            doc.history_node_count(),
        )
    }

    #[test]
    fn readonly_identity_exhaustion_rejects_request_mode_and_close_before_change()
    -> Result<(), DocError> {
        let mut doc = fixture()?;
        let before = state(&doc);
        assert!(matches!(
            doc.begin_document_request(),
            Err(DocError::Identity(IdentityError::GenerationExhausted))
        ));
        assert_eq!(state(&doc), before);
        assert!(matches!(
            doc.switch_mode(EditorMode::Source, 0),
            Err(DocError::Identity(IdentityError::GenerationExhausted))
        ));
        assert_eq!(state(&doc), before);
        assert!(matches!(
            doc.close_readonly(),
            Err(DocError::Identity(IdentityError::GenerationExhausted))
        ));
        assert_eq!(state(&doc), before);
        Ok(())
    }
}
