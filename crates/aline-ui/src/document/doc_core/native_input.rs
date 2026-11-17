//! 原生输入的受检意图、后台候选和主线程发布。
//!
//! DocCore 唯一拥有当前输入。准备任务只冻结源码快照与意图，供原生窗口调度；
//! 发布复验当前文档、输入状态和实时历史。此模块不持有窗口、执行器或文件句柄。

use std::{mem, ops::Range};

use super::checked_commit::{
    InputPatch, InputTargetUpdate, PreparedInputPatch, StagedInput, plan_single_patch,
};
use super::{
    CommitOutcome, DocCore, DocError, DraftSlot, EditorMode, InputIdentity, InputTarget, OpenBlock,
    TypingGroup, WaitState, bind_selection, copy_bounded, overflow,
};
use aline_buffer::{
    ApplyOutcome, BufferEditBase, OperationSource, SelectionSpec, SnapshotId, SrcRange,
};
use aline_edit::{EditBudget, EditContext, EditMode};

/// UI 线程冻结的原生输入；背景任务只能计算候选。
pub(crate) struct NativeInputJob {
    binding: InputBinding,
    base: BufferEditBase,
    budget: EditBudget,
    intent: FrozenInput,
}

enum FrozenInput {
    Single(InputPatch, InputSettlement),
    DeleteAfterTyping {
        group: TypingGroup,
        range: Range<usize>,
        now_millis: u64,
    },
}

#[derive(Clone, Copy)]
enum InputSettlement {
    #[cfg(test)]
    Typing(SelectionSpec),
    Composition(SelectionSpec),
    Delete,
}

impl InputSettlement {
    fn closing_selection(self) -> Option<SelectionSpec> {
        match self {
            #[cfg(test)]
            Self::Typing(selection) => Some(selection),
            Self::Composition(selection) => Some(selection),
            Self::Delete => None,
        }
    }
}

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
            FrozenInput::Single(patch, settlement) => (
                patch.prepare(self.base, mode, extent, self.budget)?,
                settlement,
            ),
            FrozenInput::DeleteAfterTyping {
                group,
                range,
                now_millis,
            } => {
                let buffer = StagedInput {
                    group: &group,
                    visual_extent: extent,
                    selection: self.binding.selection,
                    context: EditContext::new(mode),
                    now_millis,
                    budget: self.budget,
                }
                .prepare(self.base, |parsed, selection, context, budget| {
                    plan_single_patch(parsed, selection, context, None, range, "", budget)
                })?;
                (
                    PreparedInputPatch {
                        buffer: Some(buffer),
                        target: InputTargetUpdate::Preserve,
                    },
                    InputSettlement::Delete,
                )
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
    #[cfg(test)]
    fn native_typing_commit_required(
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

    #[cfg(test)]
    fn prepare_native_typing_commit(
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

    pub(crate) fn prepare_native_confirmation(
        &mut self,
        identity: &InputIdentity,
        confirmed: Option<&str>,
        now_millis: u64,
    ) -> Result<NativeInputJob, DocError> {
        self.ensure_ready()?;
        self.accept_document_result(identity)?;
        let state = self
            .open
            .as_ref()
            .and_then(OpenBlock::composition)
            .ok_or(DocError::NoComposition)?;
        let selection = match confirmed {
            Some(text) => {
                let end = state
                    .range
                    .start
                    .checked_add(text.len())
                    .ok_or_else(overflow)?;
                SelectionSpec::new(end, end)
            }
            None => state.selection,
        };
        let patch = InputPatch {
            range: state.range.clone(),
            replacement: copy_bounded(confirmed.unwrap_or(&state.candidate), self.limits.text)?,
            selections: (state.before_selection, selection),
            source: OperationSource::ImeCommit,
            now_millis,
            next_target: None,
        };
        Ok(self.freeze_native_input(
            identity,
            FrozenInput::Single(patch, InputSettlement::Composition(selection)),
        ))
    }

    /// 在局部草稿内删除，或冻结需后台准备的独立删除意图。
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
                InputSettlement::Delete,
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
        let revision = patch
            .buffer
            .as_ref()
            .and_then(|buffer| buffer.result_revision());
        let closing_selection = settlement.closing_selection();
        let (next_version, unchanged_selection) = match (revision, closing_selection) {
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
            if matches!(settlement, InputSettlement::Composition(_)) {
                self.typing_redo = None;
            }
            if let Some(selection) = closing_selection {
                self.finish_composition(Some(revision), selection);
            } else {
                self.sync_block_after_buffer_change();
            }
            self.snapshot_cache = None;
            return Ok(CommitOutcome::Committed { revision });
        }
        if let Some(selection) = closing_selection {
            self.finish_composition(None, selection);
        }
        Ok(CommitOutcome::Unchanged)
    }

    /// 同步适配保留既有调用顺序。
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
