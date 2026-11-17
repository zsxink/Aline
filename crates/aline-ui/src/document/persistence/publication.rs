//! 显式保存请求与 DocCore 的组合结束协议。协调器持有等待与排队资格，
//! 平台负责更新或结束组合，冻结之前不保留候选快照。

use super::{OperationId, PathGeneration, PersistenceCoordinator, SaveError};
use aline_buffer::TextBudget;

use crate::document::{DocCore, DocError, PublicationKind, PublicationOutcome, WaitHandle};

#[derive(Debug)]
pub(in crate::document) enum SaveRequestError {
    Queue(SaveError),
    Input(DocError),
}

impl From<SaveError> for SaveRequestError {
    fn from(error: SaveError) -> Self {
        Self::Queue(error)
    }
}

impl From<DocError> for SaveRequestError {
    fn from(error: DocError) -> Self {
        Self::Input(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::document) enum SaveRequest {
    Waiting(WaitHandle),
    Queued(OperationId),
}

#[derive(Clone, Copy)]
pub(super) struct PendingSave {
    handle: WaitHandle,
    operation: OperationId,
    generation: PathGeneration,
}

impl PersistenceCoordinator {
    /// 预算由当前文档的打开配置提供。组合自然结束后由 DocCore 按同一配置冻结。
    pub(in crate::document) fn request_save(
        &mut self,
        core: &mut DocCore,
        text_budget: TextBudget,
    ) -> Result<SaveRequest, SaveRequestError> {
        self.validate_document(core)?;
        let operation = self.next_operation()?;
        if core.composition().is_some() {
            let handle = core.begin_publication_wait(PublicationKind::Save)?;
            self.waiting = Some(PendingSave {
                handle,
                operation,
                generation: self.target.generation,
            });
            return Ok(SaveRequest::Waiting(handle));
        }
        let snapshot = core.current_snapshot(text_budget)?;
        self.enqueue(operation, snapshot);
        Ok(SaveRequest::Queued(operation))
    }

    /// 平台已自然结束组合后冻结最新文本。确认和取消都可以继续此前的保存意图，
    /// 用户主动取消保存等待则使句柄失效。
    pub(in crate::document) fn finish_save_wait(
        &mut self,
        core: &mut DocCore,
        handle: WaitHandle,
        outcome: PublicationOutcome,
        now_millis: u64,
    ) -> Result<OperationId, SaveRequestError> {
        self.validate_document(core)?;
        let pending = self.check_handle(handle)?;
        if pending.generation != self.target.generation {
            return Err(SaveError::StaleAssociation.into());
        }
        let publication = core.finish_publication(handle, outcome, now_millis)?;
        self.enqueue(pending.operation, publication.snapshot().clone());
        self.waiting = None;
        Ok(pending.operation)
    }

    pub(in crate::document) fn cancel_save_wait(
        &mut self,
        core: &mut DocCore,
        handle: WaitHandle,
    ) -> Result<(), SaveRequestError> {
        self.validate_document(core)?;
        self.check_handle(handle)?;
        core.request_cancel_wait(handle)?;
        self.waiting = None;
        Ok(())
    }

    /// 关闭先撤销尚未冻结的保存资格，再通知后台工作停止。已发布结果仍需结算。
    pub(in crate::document) fn begin_close(
        &mut self,
        core: &mut DocCore,
    ) -> Result<(), SaveRequestError> {
        self.validate_document(core)?;
        if let Some(pending) = self.waiting {
            self.cancel_save_wait(core, pending.handle)?;
        }
        self.stop_for_close();
        Ok(())
    }

    fn validate_document(&self, core: &DocCore) -> Result<(), SaveRequestError> {
        if core.document_id() != &self.document {
            return Err(SaveError::ForeignDocument.into());
        }
        if core.is_readonly() {
            return Err(DocError::ReadOnly.into());
        }
        Ok(())
    }

    fn check_handle(&self, handle: WaitHandle) -> Result<PendingSave, DocError> {
        self.waiting
            .filter(|pending| pending.handle == handle)
            .ok_or(DocError::UnknownWait)
    }
}
