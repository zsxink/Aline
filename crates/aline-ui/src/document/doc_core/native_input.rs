//! 原生输入的受检意图、后台只读准备和所有者线程发布。
//!
//! 所有者：DocCore 唯一拥有当前输入草稿、选区、输入组与历史；窗口协调器拥有
//! 后台任务与等待队列；本模块不持有窗口、执行器或文件句柄。
//!
//! 生命周期：`prepare_*` 在所有者线程冻结普通输入组、源码快照、预算与身份，
//! 返回可移交后台的 `NativeInputJob`；`NativeInputJob::run` 在后台只读解析并
//! 规划候选；`publish_native_input` 回到所有者线程复验身份、站点与实时历史后
//! 原子发布。封口（`commit_typing_group`）与离开块（`close_block`）共用同一协议，
//! 发布段不解析、不物化候选、不重建行索引。
//!
//! 副作用边界：准备段只冻结并可能建立草稿快照缓存（拒绝时调用方恢复），后台段
//! 只读，发布段才写 Buffer/历史/块状态。发布段允许的失败点只有绑定复验
//! （`InputBinding::validate`）、关闭收尾的派生请求推进与 `Buffer::publish_prepared`；
//! 前两者纯检查且在任何状态变更之前，Buffer 发布由引擎保证原子性。
//!
//! 无 Buffer 补丁但需回写选区的结算会在发布段调用 `bind_selection` 与
//! `Buffer::set_selection`：该分支 `patch.buffer` 为 `None`，`bind_selection` 用当前
//! 快照完成全部偏移校验后没有任何 Buffer 变更，`set_selection` 只是对同一快照
//! 重复同一校验（`aline-buffer` 的 `StringBuffer::set_selection` 先
//! `validate_selection`），因此它不可能在绑定成功后失败，不构成额外失败点。

use std::{mem, ops::Range};

use super::checked_commit::{InputPatch, InputTargetUpdate, PreparedInputPatch};
#[cfg(any(test, target_os = "macos"))]
use super::checked_commit::{StagedInput, plan_single_patch};
use super::{
    CommitOutcome, DerivedRequestId, DocCore, DocError, DraftSlot, EditorMode, InputIdentity,
    InputTarget, OpenBlock, WaitState, bind_selection, copy_bounded,
};
#[cfg(any(test, target_os = "macos"))]
use super::{TypingGroup, overflow};
use aline_buffer::{
    ApplyOutcome, BufferEditBase, OperationSource, SelectionSpec, SnapshotId, SrcRange,
};
#[cfg(any(test, target_os = "macos"))]
use aline_edit::EditContext;
use aline_edit::{EditBudget, EditMode};

/// UI 线程冻结的原生输入；背景任务只能计算候选。
pub(crate) struct NativeInputJob {
    binding: InputBinding,
    base: BufferEditBase,
    budget: EditBudget,
    intent: FrozenInput,
}

enum FrozenInput {
    Single(InputPatch, InputSettlement),
    #[cfg(any(test, target_os = "macos"))]
    MissingTableCell(InputPatch, aline_edit::TableTarget),
    /// 原生窗口删除适配器专用；无头协议不依赖该意图。
    #[cfg(any(test, target_os = "macos"))]
    DeleteAfterTyping {
        group: TypingGroup,
        range: Range<usize>,
        now_millis: u64,
    },
    /// 提交待定普通输入组并关闭块；无待定组时只做关闭收尾。
    CloseBlock {
        patch: Option<InputPatch>,
        settlement: InputSettlement,
    },
}

#[derive(Clone, Copy)]
enum InputSettlement {
    Typing(SelectionSpec),
    /// 组合确认结算；唯一构造点是原生窗口组合确认适配器（3.3b 接线）。
    /// 非测试的非 macOS 构建不编译该适配器，因此显式允许该变体暂未被构造。
    #[cfg_attr(not(any(test, target_os = "macos")), allow(dead_code))]
    Composition(SelectionSpec),
    /// 原生窗口删除适配器专用；携带删除后的折叠选区。
    ///
    /// 删除与普通输入一样留在打开块内：结算时只换基线、块范围和选区，
    /// 不关闭块，否则后续输入会因没有可重建的输入站点而被静默丢弃。
    #[cfg(any(test, target_os = "macos"))]
    Delete(SelectionSpec),
    /// 关闭块；`Some` 仅在存在待定普通输入组时携带封口后的选区。
    Close(Option<SelectionSpec>),
}

impl InputSettlement {
    /// 删除结算只在原生删除适配器启用时存在，分类须与变体使用相同平台条件。
    fn is_delete(self) -> bool {
        match self {
            #[cfg(any(test, target_os = "macos"))]
            Self::Delete(_) => true,
            Self::Typing(_) | Self::Composition(_) | Self::Close(_) => false,
        }
    }

    /// 结算后应换到的选区；`None` 表示按历史/命令语义同步块。
    ///
    /// 普通输入、组合确认与块内删除都在发布后停留或重建在块内，只有
    /// `Close` 的块关闭由 `closes_block` 单独表达。
    fn settlement_selection(self) -> Option<SelectionSpec> {
        match self {
            Self::Typing(selection) | Self::Composition(selection) => Some(selection),
            #[cfg(any(test, target_os = "macos"))]
            Self::Delete(selection) => Some(selection),
            Self::Close(selection) => selection,
        }
    }

    fn closes_block(self) -> bool {
        matches!(self, Self::Close(_))
    }
}

/// 编译期约束：后台执行器只能接收可跨线程移交的准备值。
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<NativeInputJob>();
    assert_send::<PreparedNativeInput>();
};

struct InputBinding {
    identity: InputIdentity,
    mode: EditorMode,
    selection: SelectionSpec,
    base_snapshot: SnapshotId,
    block_range: Option<Range<usize>>,
    draft_slot: Option<mem::Discriminant<DraftSlot>>,
    visual_extent: Option<Range<usize>>,
    wait: Option<WaitState>,
}

impl InputBinding {
    fn capture(identity: &InputIdentity, core: &DocCore) -> Self {
        Self {
            identity: identity.clone(),
            mode: core.mode,
            selection: core.current_selection(),
            base_snapshot: core.buffer.snapshot().id().clone(),
            block_range: core.open.as_ref().map(|block| block.range.clone()),
            draft_slot: core
                .open
                .as_ref()
                .map(|block| mem::discriminant(&block.draft)),
            visual_extent: core
                .open
                .as_ref()
                .and_then(OpenBlock::visual_extent)
                .cloned(),
            wait: core.wait,
        }
    }

    fn validate(&self, core: &mut DocCore, settlement: InputSettlement) -> Result<(), DocError> {
        core.ensure_writable()?;
        core.accept_document_result(&self.identity)?;
        if self.mode != core.mode
            || self.selection != core.current_selection()
            || &self.base_snapshot != core.buffer.snapshot().id()
            || self.draft_slot != core.open.as_ref().map(|block| mem::discriminant(&block.draft))
            || self.block_range.as_ref() != core.open.as_ref().map(|block| &block.range)
            || self.visual_extent.as_ref() != core.open.as_ref().and_then(OpenBlock::visual_extent)
            // ED-01 的等待取消或新请求不撤销已到达的组合确认。
            || (!matches!(settlement, InputSettlement::Composition(_)) && self.wait != core.wait)
        {
            return Err(DocError::StaleResult);
        }
        Ok(())
    }
}

/// 单次可消费的后台结果。只交还生成它的 DocCore 复验。
pub(crate) struct PreparedNativeInput {
    binding: InputBinding,
    patch: PreparedInputPatch,
    settlement: InputSettlement,
}

impl NativeInputJob {
    pub(crate) fn run(self) -> Result<PreparedNativeInput, DocError> {
        let mode = match self.binding.mode {
            EditorMode::Source => EditMode::Source,
            EditorMode::Visual => EditMode::Visual,
        };
        let extent = self.binding.visual_extent.as_ref();
        let (patch, settlement) = match self.intent {
            FrozenInput::Single(patch, settlement) => {
                // 原生删除以 Replace 发布，但仍须重绑块范围才能留在块内。
                let raw_on_replace = settlement.is_delete();
                (
                    patch.prepare(self.base, mode, extent, self.budget, raw_on_replace)?,
                    settlement,
                )
            }
            #[cfg(any(test, target_os = "macos"))]
            FrozenInput::MissingTableCell(patch, cell) => {
                let (patch, selection) =
                    patch.prepare_missing_cell(self.base, mode, extent, self.budget, &cell)?;
                (patch, InputSettlement::Composition(selection))
            }
            #[cfg(any(test, target_os = "macos"))]
            FrozenInput::DeleteAfterTyping {
                group,
                range,
                now_millis,
            } => {
                let after = SelectionSpec::new(range.start, range.start);
                let (buffer, target) = StagedInput {
                    group: &group,
                    visual_extent: extent,
                    selection: self.binding.selection,
                    context: EditContext::new(mode),
                    now_millis,
                    budget: self.budget,
                }
                .prepare_with_target(
                    self.base,
                    Some(after),
                    |parsed, selection, context, budget| {
                        plan_single_patch(parsed, selection, context, None, range, "", budget)
                    },
                )?;
                (
                    PreparedInputPatch {
                        buffer: Some(buffer),
                        target,
                    },
                    InputSettlement::Delete(after),
                )
            }
            FrozenInput::CloseBlock { patch, settlement } => {
                let patch = match patch {
                    Some(patch) => patch.prepare(self.base, mode, extent, self.budget, false)?,
                    None => PreparedInputPatch {
                        buffer: None,
                        target: InputTargetUpdate::Preserve,
                    },
                };
                (patch, settlement)
            }
        };
        Ok(PreparedNativeInput {
            binding: self.binding,
            patch,
            settlement,
        })
    }
}

impl DocCore {
    fn freeze_native_input(&self, identity: &InputIdentity, intent: FrozenInput) -> NativeInputJob {
        NativeInputJob {
            binding: InputBinding::capture(identity, self),
            base: self.buffer.edit_base(),
            budget: self.edit_budget(),
            intent,
        }
    }

    /// 查询本次普通输入是否需先封口旧组。调用者必须先结算准备结果。
    ///
    /// 原生窗口按键处理器在 900ms 边界处消费；平台适配仅在 macOS 编译，
    /// 无头测试仍可直接驱动。准备段只读、有界、不解析。
    #[cfg(any(test, target_os = "macos"))]
    pub(crate) fn native_typing_commit_required(
        &mut self,
        identity: &InputIdentity,
        text: &str,
        now_millis: u64,
    ) -> Result<bool, DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        let Some(block) = self.open.as_ref().filter(|block| block.typing().is_some()) else {
            return Ok(false);
        };
        if text.is_empty() {
            return Ok(false);
        }
        let range = block.selection.range();
        self.check_open_edit_range(&range)?;
        if range.is_empty() {
            return Ok(!Self::can_continue_typing(
                block,
                block.selection.head(),
                now_millis,
            )?);
        }
        let current = self.cached_current_text()?;
        let selected = current.src_range(range)?;
        Ok(current.slice(&selected, self.limits.text)? != text)
    }

    /// 冻结普通输入组的封口意图；准备段只读，发布由 `publish_native_input` 完成。
    ///
    /// `next_target` 供 3.3b 的原生输入回调保留；同步封口传 `None`。
    pub(crate) fn prepare_native_typing_commit(
        &mut self,
        identity: &InputIdentity,
        now_millis: u64,
        next_target: Option<&SrcRange>,
    ) -> Result<Option<NativeInputJob>, DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        let Some(block) = self.open.as_ref() else {
            return Ok(None);
        };
        let Some(group) = block.typing() else {
            return Ok(None);
        };
        let next_target = next_target
            .map(|target| {
                self.cached_current_text()?
                    .validate_range(target)
                    .map_err(DocError::Buffer)
            })
            .transpose()?;
        let after = block.selection;
        let patch = InputPatch {
            range: group.range.clone(),
            replacement: copy_bounded(&group.draft, self.limits.text)?,
            selections: (group.before_selection, after),
            source: OperationSource::Typing,
            now_millis,
            next_target,
        };
        Ok(Some(self.freeze_native_input(
            identity,
            FrozenInput::Single(patch, InputSettlement::Typing(after)),
        )))
    }

    /// 冻结「提交待定普通输入组 → 关闭块」的整体意图。
    ///
    /// 准备段预检派生请求可推进；发布段在任何状态变更前再次执行同一纯计数检查。
    pub(crate) fn prepare_native_close(
        &mut self,
        identity: &InputIdentity,
        now_millis: u64,
    ) -> Result<NativeInputJob, DocError> {
        self.ensure_ready()?;
        self.accept_document_result(identity)?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        self.derived_request.next().map_err(DocError::Identity)?;
        let block = self.open.as_ref().ok_or(DocError::NoOpenBlock)?;
        let patch = block
            .typing()
            .map(|group| {
                Ok::<_, DocError>(InputPatch {
                    range: group.range.clone(),
                    replacement: copy_bounded(&group.draft, self.limits.text)?,
                    selections: (group.before_selection, block.selection),
                    source: OperationSource::Typing,
                    now_millis,
                    next_target: None,
                })
            })
            .transpose()?;
        let settlement = InputSettlement::Close(patch.is_some().then_some(block.selection));
        Ok(self.freeze_native_input(identity, FrozenInput::CloseBlock { patch, settlement }))
    }

    /// 原生窗口组合适配器；无头协议不依赖该入口。
    #[cfg(any(test, target_os = "macos"))]
    pub(crate) fn update_native_candidate(
        &mut self,
        identity: &InputIdentity,
        candidate: &str,
        selected_in_candidate: Option<SelectionSpec>,
        now_millis: u64,
    ) -> Result<(), DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        if self.is_composition_active() {
            self.ensure_ready()?;
            return self.replace_candidate(candidate, selected_in_candidate);
        }

        let locally_opened = self.open.is_none() && self.mode == EditorMode::Source;
        if locally_opened {
            let selection = self.current_selection();
            self.open_input_target(InputTarget::Source, selection.range(), selection)?;
        }
        let result = self
            .ensure_ready()
            .and_then(|()| self.start_composition(candidate, selected_in_candidate, now_millis));
        if locally_opened && result.is_err() {
            self.open = None;
        }
        result
    }

    /// 原生窗口组合确认适配器；无头协议不依赖该入口。
    #[cfg(any(test, target_os = "macos"))]
    pub(crate) fn prepare_native_confirmation(
        &mut self,
        identity: &InputIdentity,
        confirmed: Option<&str>,
        now_millis: u64,
    ) -> Result<NativeInputJob, DocError> {
        self.prepare_native_confirmation_at(identity, confirmed, now_millis, None)
    }

    #[cfg(any(test, target_os = "macos"))]
    pub(crate) fn prepare_native_confirmation_at(
        &mut self,
        identity: &InputIdentity,
        confirmed: Option<&str>,
        now_millis: u64,
        missing_cell: Option<&aline_edit::TableTarget>,
    ) -> Result<NativeInputJob, DocError> {
        self.ensure_ready()?;
        self.accept_document_result(identity)?;
        let block = self.open.as_ref().ok_or(DocError::NoComposition)?;
        let state = block.composition().ok_or(DocError::NoComposition)?;
        let replacement = match confirmed {
            Some(text) => self.copy_input_with_separator(block.leading_separator(), text)?,
            None => copy_bounded(&state.candidate, self.limits.text)?,
        };
        let selection = match confirmed {
            Some(_) => {
                let end = state
                    .range
                    .start
                    .checked_add(replacement.len())
                    .ok_or_else(overflow)?;
                SelectionSpec::new(end, end)
            }
            None => state.selection,
        };
        let patch = InputPatch {
            range: state.range.clone(),
            replacement,
            selections: (state.before_selection, selection),
            source: OperationSource::ImeCommit,
            now_millis,
            next_target: None,
        };
        let intent = match missing_cell {
            Some(cell) => FrozenInput::MissingTableCell(patch, cell.clone()),
            None => FrozenInput::Single(patch, InputSettlement::Composition(selection)),
        };
        Ok(self.freeze_native_input(identity, intent))
    }

    /// 在局部草稿内删除，或冻结需后台准备的独立删除意图。
    #[cfg(any(test, target_os = "macos"))]
    pub(crate) fn prepare_native_delete(
        &mut self,
        identity: &InputIdentity,
        range: &SrcRange,
        now_millis: u64,
    ) -> Result<Option<NativeInputJob>, DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        let bytes = self.cached_current_text()?.validate_range(range)?;
        if bytes.is_empty() {
            return Ok(None);
        }
        self.check_open_edit_range(&bytes)?;
        let next = self.version.next_draft().map_err(DocError::Identity)?;
        let intent = if let Some(group) = self.open.as_ref().and_then(OpenBlock::typing) {
            let draft_start = group.range.start;
            let draft_end = draft_start
                .checked_add(group.draft.len())
                .ok_or_else(overflow)?;
            let same_group = now_millis
                .checked_sub(group.last_input_at)
                .is_some_and(|millis| millis <= super::TYPING_MERGE_WINDOW_MILLIS);
            if same_group && bytes.start >= draft_start && bytes.end <= draft_end {
                let local = bytes.start - draft_start..bytes.end - draft_start;
                let selection = SelectionSpec::new(bytes.start, bytes.start);
                let block = self.open.as_mut().ok_or(DocError::NoOpenBlock)?;
                let group = block.typing_mut().ok_or(DocError::PendingInput)?;
                if group.draft.get(local.clone()).is_none() {
                    return Err(DocError::OutsideOpenBlock);
                }
                group.draft.replace_range(local, "");
                group.after_selection = selection;
                group.last_input_at = now_millis;
                block.selection = selection;
                self.version = next;
                self.snapshot_cache = None;
                return Ok(None);
            }
            FrozenInput::DeleteAfterTyping {
                group: TypingGroup {
                    draft: copy_bounded(&group.draft, self.limits.text)?,
                    range: group.range.clone(),
                    before_selection: group.before_selection,
                    after_selection: group.after_selection,
                    last_input_at: group.last_input_at,
                },
                range: bytes,
                now_millis,
            }
        } else {
            let after = SelectionSpec::new(bytes.start, bytes.start);
            FrozenInput::Single(
                InputPatch {
                    range: bytes,
                    replacement: String::new(),
                    selections: (self.current_selection(), after),
                    source: OperationSource::Replace,
                    now_millis,
                    next_target: None,
                },
                InputSettlement::Delete(after),
            )
        };
        Ok(Some(self.freeze_native_input(identity, intent)))
    }

    pub(crate) fn publish_native_input(
        &mut self,
        prepared: PreparedNativeInput,
    ) -> Result<CommitOutcome, DocError> {
        let PreparedNativeInput {
            binding,
            patch,
            settlement,
        } = prepared;
        binding.validate(self, settlement)?;
        // 关闭收尾的请求推进是纯计数器检查，必须在任何状态变更之前完成。
        let close_request = if settlement.closes_block() {
            Some(self.derived_request.next().map_err(DocError::Identity)?)
        } else {
            None
        };
        let revision = patch
            .buffer
            .as_ref()
            .and_then(|buffer| buffer.result_revision());
        let settlement_selection = settlement.settlement_selection();
        let (next_version, unchanged_selection) = match (revision, settlement_selection) {
            (Some(revision), _) => (self.version.after_commit(revision), None),
            (None, Some(selection)) => (
                self.version.next_draft(),
                Some(bind_selection(self.buffer.snapshot(), selection)?),
            ),
            (None, None) => (Ok(self.version), None),
        };
        let next_version = next_version.map_err(DocError::Identity)?;

        let outcome = match patch.buffer {
            Some(buffer) => self.buffer.publish_prepared(buffer)?,
            None => ApplyOutcome::Unchanged,
        };
        debug_assert_eq!(
            revision,
            match outcome {
                ApplyOutcome::Changed(ref applied) => Some(applied.revision()),
                ApplyOutcome::Unchanged => None,
            }
        );
        if let Some(selection) = unchanged_selection {
            self.buffer.set_selection(&selection)?;
        }
        self.version = next_version;
        if let Some(revision) = revision {
            self.buffer.seal_pending();
            self.update_raw_input_target(patch.target);
            if matches!(settlement, InputSettlement::Composition(_)) || settlement.is_delete() {
                self.typing_redo = None;
            }
            if let Some(selection) = settlement_selection {
                // 组合确认与块内删除都在发布后换基线并停留或重建在块内。
                self.finish_composition(Some(revision), selection);
            } else {
                self.sync_block_after_buffer_change();
            }
            self.snapshot_cache = None;
            self.finish_native_close(close_request);
            return Ok(CommitOutcome::Committed { revision });
        }
        if let Some(selection) = settlement_selection {
            self.finish_composition(None, selection);
        }
        self.finish_native_close(close_request);
        Ok(CommitOutcome::Unchanged)
    }

    /// 关闭块的收尾；与既有 `close_block` 的收尾语义逐项一致。
    fn finish_native_close(&mut self, next_request: Option<DerivedRequestId>) {
        let Some(next_request) = next_request else {
            return;
        };
        if self
            .typing_redo
            .as_ref()
            .is_some_and(|redo| redo.target == InputTarget::Legacy)
        {
            // M0 无头入口把待定重做的生命周期限定在调用者手动打开的块内。
            self.typing_redo = None;
        }
        self.derived_request = next_request;
        self.open = None;
        self.wait = None;
        self.snapshot_cache = None;
    }

    /// 同步适配保留既有调用顺序；仅原生窗口使用。
    #[cfg(target_os = "macos")]
    pub(crate) fn delete_native_range(
        &mut self,
        identity: &InputIdentity,
        range: &SrcRange,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        let Some(job) = self.prepare_native_delete(identity, range, now_millis)? else {
            return Ok(CommitOutcome::Unchanged);
        };
        self.publish_native_input(job.run()?)
    }

    /// 同步适配保留既有调用顺序；仅原生窗口使用。
    #[cfg(target_os = "macos")]
    pub(crate) fn commit_native_candidate(
        &mut self,
        identity: &InputIdentity,
        confirmed: &str,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        let job = self.prepare_native_confirmation(identity, Some(confirmed), now_millis)?;
        self.publish_native_input(job.run()?)
    }
}

#[cfg(test)]
mod tests;
