//! 文档所有者消费 edit 计划/校验的唯一普通输入与命令发布边界。
//!
//! 状态仍全部属于 DocCore；本模块只在组/IME提交或显式命令时解析和校验，
//! 不持第二全文、派生缓存或历史。输入携带自身范围和 before/after 选区；
//! 命令携带宿主身份信封和只读 Parsed，全部检查完成后才允许 Buffer 发布。
use super::{CommitOutcome, DocCore, DocError, bind_selection};
use crate::document::{EditorMode, InputIdentity};
use aline_buffer::{ApplyOutcome, OperationSource, SelectionSpec};
use aline_edit::{
    ChangePlan, EditBudget, EditCommand, EditContext, EditMode, EditRefusal, PlanOutcome,
    RereadFailure, SourceEdit, plan_change, validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, Parsed, parse};
use std::ops::Range;

impl DocCore {
    /// 执行已规划命令；不适用的 pending/组合状态在任何封口或快照变更前拒绝。
    ///
    /// `identity` 必须来自本 DocCore 的当前输入身份；`parsed` 必须绑定该源码
    /// 快照、M0 配置和计划投影。复验文档/工作版本/快照/配置/请求及当前选区后
    /// 才原子发布。拒绝保留草稿、组合和历史；redo 本身不拒绝实际新编辑。
    /// 调用者应先处理 `PlanOutcome::Unchanged`，不制造空计划或历史节点。
    pub fn apply_change(
        &mut self,
        identity: &InputIdentity,
        parsed: &Parsed,
        plan: &ChangePlan,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        self.ensure_ready()?;
        let context = self.command_context();
        if context.has_pending_input() {
            return Err(DocError::PendingInput);
        }
        if context.has_composition() {
            return Err(DocError::CompositionActive);
        }
        // 已排除草稿，基线快照就是当前全文；不调用会写入草稿缓存的读取入口。
        self.accept_derived_result(identity)?;
        if parsed.config() != ParseConfig::m0() {
            return Err(DocError::StaleResult);
        }
        let selection = bind_selection(self.buffer.snapshot(), self.current_selection())?;
        let change = validate_change(plan, self.buffer.snapshot(), parsed, &selection, &context)
            .map_err(DocError::Edit)?;
        self.version.next_draft().map_err(DocError::Identity)?;
        let outcome = self
            .buffer
            .apply(
                change.batch(),
                change.target_selection(),
                OperationSource::Command,
                now_millis,
                self.limits.text,
            )
            .map_err(DocError::Buffer)?;
        match outcome {
            ApplyOutcome::Unchanged => Ok(CommitOutcome::Unchanged),
            ApplyOutcome::Changed(applied) => {
                let revision = applied.revision();
                self.buffer.seal_pending();
                self.version = self
                    .version
                    .after_commit(revision)
                    .map_err(DocError::Identity)?;
                self.sync_block_after_buffer_change();
                self.snapshot_cache = None;
                Ok(CommitOutcome::Committed { revision })
            }
        }
    }

    fn command_context(&self) -> EditContext {
        let mode = match self.mode {
            EditorMode::Visual => EditMode::Visual,
            EditorMode::Source => EditMode::Source,
        };
        EditContext::new(mode)
            .with_pending_input(
                self.open
                    .as_ref()
                    .and_then(|block| block.typing())
                    .is_some(),
            )
            .with_composition_active(self.is_composition_active())
    }

    pub(super) fn publish_input(
        &mut self,
        range: Range<usize>,
        replacement: String,
        selections: (SelectionSpec, SelectionSpec),
        source: OperationSource,
        now_millis: u64,
    ) -> Result<ApplyOutcome, DocError> {
        let snapshot = self.buffer.snapshot();
        let src_range = snapshot
            .src_range(range.clone())
            .map_err(DocError::Buffer)?;
        let expected = snapshot
            .slice(&src_range, self.limits.text)
            .map_err(DocError::Buffer)?;
        let selection = bind_selection(snapshot, selections.0)?;
        let parse_budget = ParseBudget::m0().with_max_source_bytes(
            ParseBudget::m0()
                .max_source_bytes()
                .min(self.limits.text.max_bytes()),
        );
        let budget = EditBudget::new(self.patch_budget(), self.limits.text, parse_budget);
        let parsed = parse(snapshot, ParseConfig::m0(), parse_budget).map_err(|cause| {
            DocError::Edit(EditRefusal::RereadFailed {
                reason: RereadFailure::Parse,
                cause: Some(cause),
            })
        })?;
        // 这是唯一输入槽自身的发布，不是待定输入期间执行其它命令。
        // 草稿始终保留至发布成功；edit 读取的是该输入开始前的稳定 Buffer 基线。
        let context = EditContext::new(self.command_context().mode());
        let command =
            EditCommand::replace(vec![SourceEdit::new(range, expected.as_ref(), replacement)]);
        let outcome = plan_change(snapshot, &parsed, &selection, &context, &command, budget)
            .map_err(DocError::Edit)?;
        let PlanOutcome::Changed(plan) = outcome else {
            return Ok(ApplyOutcome::Unchanged);
        };
        let change = validate_change(&plan, snapshot, &parsed, &selection, &context)
            .map_err(DocError::Edit)?;
        self.buffer
            .apply(
                change.batch(),
                selections.1,
                source,
                now_millis,
                self.limits.text,
            )
            .map_err(DocError::Buffer)
    }
}
