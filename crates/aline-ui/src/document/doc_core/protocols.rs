//! 宿主驱动的资源登记、派生请求及模式/持久化读取协议。
//!
//! 所有状态继续由父模块 DocCore 持有；本模块不引入副本、调度或文件协调器。
use super::{DocCore, DocError};
use crate::document::{
    CurrentSnapshot, DerivedRequestId, EditorMode, InputIdentity, ModeSwitchOutcome,
    ParseConfigGeneration, ResourceReference,
};
use aline_buffer::TextBudget;

impl DocCore {
    /// 宿主在资源引用生效前登记；只接受当前身份及同一文档的引用。
    ///
    /// M0 保守持有所有登记引用至文档结束，以免草稿、Undo/Redo 或恢复材料仍需
    /// 资源时提前释放；快照只冻结取用时的集合。不自动扫描文本或删除资源。
    pub fn retain_resource(
        &mut self,
        identity: &InputIdentity,
        resource: ResourceReference,
        max_references: usize,
    ) -> Result<(), DocError> {
        self.ensure_writable()?;
        self.accept_derived_result(identity)?;
        if resource.document_id() != &self.document {
            return Err(DocError::WrongDocument);
        }
        if self.resources.contains(&resource) {
            return Ok(());
        }
        if self.resources.len() >= max_references {
            return Err(DocError::ResourceBudgetExceeded);
        }
        let next_version = self.version.next_draft().map_err(DocError::Identity)?;
        let mut resources = self.resources.to_vec();
        resources.push(resource);
        self.resources = resources.into();
        self.version = next_version;
        self.snapshot_cache = None;
        Ok(())
    }

    /// 返回当前保守资源集合，供宿主计划资源事务；不会触发清理。
    pub fn resources(&self) -> &[ResourceReference] {
        &self.resources
    }

    /// 返回等待句柄的下一编号，供宿主跟踪请求且不暴露可变状态。
    pub const fn next_wait_id(&self) -> u64 {
        self.next_wait_id
    }

    /// 返回当前解析配置代次。
    pub const fn parse_config_generation(&self) -> ParseConfigGeneration {
        self.parse_config
    }

    /// 返回最新派生采纳代次；发起请求、关闭目标或实际切换模式时单调推进。
    pub const fn derived_request_id(&self) -> DerivedRequestId {
        self.derived_request
    }

    /// 宿主改变有效解析配置时推进代次，使旧配置的结果失效。
    pub fn advance_parse_config(&mut self) -> Result<(), DocError> {
        self.ensure_document_alive()?;
        let next = self.parse_config.next().map_err(DocError::Identity)?;
        self.parse_config = next;
        Ok(())
    }

    /// 冻结当前全部派生身份；不发起新请求。
    pub fn current_input_identity(&mut self) -> Result<InputIdentity, DocError> {
        let snapshot = self.current_snapshot(self.limits.text)?;
        InputIdentity::for_request(
            self.document.clone(),
            self.version,
            snapshot.text(),
            self.parse_config,
            self.derived_request,
        )
        .map_err(DocError::Identity)
    }

    /// 为新编辑投影任务发放单调请求身份；同文本同配置的旧请求也将拒收。
    /// 无打开块时仍按既有编辑协议拒绝；文档解析使用 begin_document_request。
    pub fn begin_derived_request(&mut self) -> Result<InputIdentity, DocError> {
        self.ensure_ready()?;
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

    /// 返回当前模式；两个模式始终消费同一 Buffer/历史。
    pub const fn mode(&self) -> EditorMode {
        self.mode
    }

    /// 组合期延后切换，由宿主在平台自然结束后重试；不伪造确认或取消。
    /// 非组合期只经统一协议提交剩余输入组，切换本身不写字节、不追加历史，
    /// 并作废旧派生请求；同模式和延后请求均不推进身份。
    pub fn switch_mode(
        &mut self,
        mode: EditorMode,
        now_millis: u64,
    ) -> Result<ModeSwitchOutcome, DocError> {
        self.ensure_document_alive()?;
        if !self.is_readonly() {
            self.ensure_ready()?;
        }
        if self.is_composition_active() {
            return Ok(ModeSwitchOutcome::Deferred);
        }
        if mode == self.mode {
            return Ok(ModeSwitchOutcome::Ready);
        }
        // 即使源码不变，模式变化也使先前请求失效；失败不得先改变输入历史。
        let next_request = self.derived_request.next().map_err(DocError::Identity)?;
        if !self.is_readonly() {
            self.commit_typing_group(now_millis)?;
        }
        self.derived_request = next_request;
        self.mode = mode;
        Ok(ModeSwitchOutcome::Ready)
    }

    /// 组合期自动保存只返回延后信号；调度时钟和真实保存由宿主承担。
    /// 只读阶段返回 ReadOnly，不发放持久化任务输入。
    pub fn autosave_snapshot(
        &mut self,
        budget: TextBudget,
    ) -> Result<Option<CurrentSnapshot>, DocError> {
        self.ensure_writable()?;
        if self.is_composition_active() {
            return Ok(None);
        }
        self.current_snapshot(budget).map(Some)
    }

    /// 恢复保护独立读取当前输入（含组合候选），不发起显式发布等待。
    /// 只读阶段返回 ReadOnly，普通阅读仍可使用 current_snapshot。
    pub fn recovery_snapshot(&mut self, budget: TextBudget) -> Result<CurrentSnapshot, DocError> {
        self.ensure_writable()?;
        self.current_snapshot(budget)
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::document::open_block::OpenBlock;
    use crate::document::publication::WaitState;
    use crate::document::{DocLimits, DocumentId, WorkingVersion};
    use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, SnapshotId};
    use std::sync::Arc;

    #[derive(Debug, PartialEq)]
    struct State {
        document: DocumentId,
        version: WorkingVersion,
        buffer_id: SnapshotId,
        text: String,
        selection: SelectionSpec,
        history: (usize, usize, usize),
        open: Option<OpenBlock>,
        wait: Option<WaitState>,
        next_wait: u64,
        resources: Arc<[ResourceReference]>,
        config: ParseConfigGeneration,
        request: DerivedRequestId,
        mode: EditorMode,
        cache: Option<(WorkingVersion, SnapshotId)>,
        limits: DocLimits,
        access: super::super::readonly::DocumentAccess,
        alive: bool,
    }

    fn state(doc: &DocCore) -> Result<State, DocError> {
        Ok(State {
            document: doc.document.clone(),
            version: doc.version,
            buffer_id: doc.buffer.snapshot().id().clone(),
            text: doc.buffer_text()?,
            selection: doc.buffer_selection(),
            history: (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
            open: doc.open.clone(),
            wait: doc.wait,
            next_wait: doc.next_wait_id,
            resources: doc.resources.clone(),
            config: doc.parse_config,
            request: doc.derived_request,
            mode: doc.mode,
            cache: doc
                .snapshot_cache
                .as_ref()
                .map(|cache| (cache.version, cache.snapshot.snapshot_id().clone())),
            limits: doc.limits,
            access: doc.access,
            alive: doc.alive,
        })
    }

    fn fixture(pending: bool) -> Result<DocCore, DocError> {
        let limits = DocLimits::new(
            TextBudget::new(1024),
            LineIndexBudget::new(128),
            HistoryBudget::new(32, 1024),
        );
        let mut doc = DocCore::new("x\n", SelectionSpec::new(1, 1), limits)?;
        if pending {
            doc.type_text(1, "a", 0)?;
        }
        doc.current_snapshot(TextBudget::new(1024))?;
        doc.derived_request = DerivedRequestId::exhausted_for_test();
        Ok(doc)
    }

    #[test]
    fn document_close_request_exhaustion_is_atomic_before_pending_commit() -> Result<(), DocError> {
        for pending in [false, true] {
            let mut doc = fixture(pending)?;
            let before = state(&doc)?;
            let result = doc.leave_block(1);
            assert!(matches!(
                result,
                Err(DocError::Identity(
                    super::super::IdentityError::GenerationExhausted
                ))
            ));
            assert_eq!(before, state(&doc)?);
        }
        Ok(())
    }

    #[test]
    fn input_group_mode_request_exhaustion_is_atomic_before_pending_commit() -> Result<(), DocError>
    {
        for pending in [false, true] {
            let mut doc = fixture(pending)?;
            let before = state(&doc)?;
            let result = doc.switch_mode(EditorMode::Source, 1);
            assert!(matches!(
                result,
                Err(DocError::Identity(
                    super::super::IdentityError::GenerationExhausted
                ))
            ));
            assert_eq!(before, state(&doc)?);
            assert_eq!(
                doc.switch_mode(EditorMode::Visual, 2)?,
                ModeSwitchOutcome::Ready
            );
            assert_eq!(before, state(&doc)?);
        }
        Ok(())
    }
}
