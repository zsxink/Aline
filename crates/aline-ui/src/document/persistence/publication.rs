//! 显式保存请求与 DocCore 的组合结束协议。协调器持有等待与排队资格，
//! 平台负责更新或结束组合，冻结之前不保留候选快照。

use super::{OperationId, PathGeneration, PersistenceCoordinator, SaveError, WriteKind};
use aline_buffer::TextBudget;
use std::path::PathBuf;

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

#[derive(Clone)]
pub(super) struct PendingSave {
    handle: WaitHandle,
    operation: OperationId,
    generation: PathGeneration,
    kind: WriteKind,
    path: Option<PathBuf>,
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
        let kind = if self.target.creating {
            WriteKind::Create
        } else {
            WriteKind::Existing
        };
        let path = self.target.path.clone();
        if core.composition().is_some() {
            let handle = core.begin_publication_wait(PublicationKind::Save)?;
            self.waiting = Some(PendingSave {
                handle,
                operation,
                generation: self.target.generation,
                kind,
                path: None,
            });
            return Ok(SaveRequest::Waiting(handle));
        }
        let snapshot = core.current_snapshot(text_budget)?;
        self.enqueue(operation, snapshot, kind, path);
        Ok(SaveRequest::Queued(operation))
    }

    /// 另存为：选定的绝对路径入队；与当前目标相同时退化为普通保存。
    /// `overwrite` 固定为 `true`（保存面板已让用户确认替换）。
    pub(in crate::document) fn request_save_as(
        &mut self,
        core: &mut DocCore,
        text_budget: TextBudget,
        path: PathBuf,
    ) -> Result<SaveRequest, SaveRequestError> {
        self.validate_document(core)?;
        if !path.is_absolute() {
            return Err(SaveError::InvalidPath.into());
        }
        if path == self.target.path {
            return self.request_save(core, text_budget);
        }
        let operation = self.next_operation()?;
        if core.composition().is_some() {
            let handle = core.begin_publication_wait(PublicationKind::Save)?;
            self.waiting = Some(PendingSave {
                handle,
                operation,
                generation: self.target.generation,
                kind: WriteKind::SaveAs,
                path: Some(path),
            });
            return Ok(SaveRequest::Waiting(handle));
        }
        let snapshot = core.current_snapshot(text_budget)?;
        self.enqueue(operation, snapshot, WriteKind::SaveAs, path);
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
        let path = pending
            .path
            .clone()
            .unwrap_or_else(|| self.target.path.clone());
        self.enqueue(
            pending.operation,
            publication.snapshot().clone(),
            pending.kind,
            path,
        );
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
    /// 生产关闭走会话退役；本入口仅 crate 测试调用。
    #[cfg(test)]
    pub(in crate::document) fn begin_close(
        &mut self,
        core: &mut DocCore,
    ) -> Result<(), SaveRequestError> {
        self.validate_document(core)?;
        if let Some(pending) = self.waiting.clone() {
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
            .clone()
            .filter(|pending| pending.handle == handle)
            .ok_or(DocError::UnknownWait)
    }
}
