//! M2 宿主消费的可编辑文档入口。DocCore 统一拥有选区、块、草稿与历史。
//! 接收已绑定当前快照的源码端点，返回纯内存结果；剪贴板和布局由调用方管理。

use std::ops::Range;

use aline_buffer::{
    BufferError, CheckedBatch, OperationSource, SelectionSpec, SrcRange, SrcSelection, TextBudget,
};
use aline_edit::EditRefusal;
use aline_markdown::{ParseConfig, Parsed, ProtectedRange, ProtectionKind, StyledRange};

use super::checked_commit::transform_extent;
use super::{
    CommitOutcome, DocCore, DocError, bind_selection, copy_bounded, overflow, selection_of,
};
use crate::document::open_block::{InputTarget, VisualInput, VisualProtection};
use crate::document::{CurrentSnapshot, EditorMode, InputIdentity};

impl DocCore {
    /// 从当前解析打开可视化块。调用方传入布局命中已转换得到的源码选区。
    /// 区间须对应真实顶层块，选区须位于块内且没有触及受保护语法。
    pub fn open_visual_block(
        &mut self,
        identity: &InputIdentity,
        parsed: &Parsed,
        block: &SrcRange,
        selection: &SrcSelection,
    ) -> Result<(), DocError> {
        self.ensure_writable()?;
        if self.mode != EditorMode::Visual {
            return Err(DocError::RequiresVisual);
        }
        if self.open.is_some() {
            return Err(DocError::BlockAlreadyOpen);
        }
        self.accept_document_result(identity)?;
        if parsed.snapshot().id() != identity.snapshot_id() || parsed.config() != ParseConfig::m0()
        {
            return Err(DocError::StaleResult);
        }
        let snapshot = self.buffer.snapshot();
        let extent = snapshot.validate_range(block).map_err(DocError::Buffer)?;
        if !parsed
            .blocks()
            .iter()
            .any(|candidate| candidate.range() == block)
        {
            return Err(DocError::InvalidVisualBlock);
        }
        snapshot
            .validate_selection(selection)
            .map_err(DocError::Buffer)?;
        let selection = selection_of(selection);
        if !contains(&extent, &selection.range()) {
            return Err(DocError::OutsideOpenBlock);
        }
        let target =
            VisualInput::from_parse(extent, parsed.protected_ranges(), parsed.styled_ranges())?;
        target.check_permissions(&selection.range(), |range| Ok(range.clone()))?;
        // 草稿范围从选区起步，边界单独记录以防写入分隔换行。
        self.open_input_target(InputTarget::Visual(target), selection.range(), selection)
    }

    /// 更新文档源码选区。可跨块，且对两种模式和同一 Buffer 生效。
    pub fn select(
        &mut self,
        identity: &InputIdentity,
        selection: &SrcSelection,
    ) -> Result<(), DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        let current = self.current_snapshot(self.limits.text)?;
        current
            .text()
            .validate_selection(selection)
            .map_err(DocError::Buffer)?;
        if self.open.is_some() {
            return self.set_selection(selection_of(selection));
        }
        self.buffer
            .set_selection(selection)
            .map_err(DocError::Buffer)
    }

    /// 选择当前源码全文；读取包含待定草稿的快照并保留历史。
    pub fn select_all(&mut self, identity: &InputIdentity) -> Result<(), DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        let current = self.current_snapshot(self.limits.text)?;
        let selection = bind_selection(current.text(), SelectionSpec::new(0, current.len_bytes()))?;
        self.select(identity, &selection)
    }

    /// 从当前快照提取选中的原文，宿主负责写入系统剪贴板。
    pub fn copy_selection(&mut self, budget: TextBudget) -> Result<String, DocError> {
        let current = self.current_snapshot(self.limits.text)?;
        Ok(current
            .text()
            .slice(&current.selection().range(), budget)
            .map_err(DocError::Buffer)?
            .into_owned())
    }

    /// 删除并返回选区原文。仅成功后宿主才应改写剪贴板。
    /// 空选区返回空字符串，宿主应保留剪贴板。
    pub fn cut_selection(
        &mut self,
        identity: &InputIdentity,
        budget: TextBudget,
        now_millis: u64,
    ) -> Result<String, DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        let text = self.copy_selection(budget)?;
        if text.is_empty() {
            return Ok(text);
        }
        self.replace_selection(identity, "", now_millis)?;
        Ok(text)
    }

    /// 使用纯文本独立替换当前选区；删除、粘贴和纯文本粘贴共用此入口。
    /// 未封口的普通输入与独立命令一并预检后原子发布；活动组合拒绝。
    pub fn replace_selection(
        &mut self,
        identity: &InputIdentity,
        text: &str,
        now_millis: u64,
    ) -> Result<CommitOutcome, DocError> {
        self.ensure_writable()?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        self.accept_document_result(identity)?;
        let before = self.current_selection();
        let range = before.range();
        if range.is_empty() && text.is_empty() {
            return Ok(CommitOutcome::Unchanged);
        }
        self.version.next_draft().map_err(DocError::Identity)?;
        let outcome = if self
            .open
            .as_ref()
            .and_then(|block| block.typing())
            .is_some()
        {
            self.publish_staged_replacement(text, now_millis)?
        } else {
            let after_src_off = range.start.checked_add(text.len()).ok_or_else(overflow)?;
            let after = SelectionSpec::new(after_src_off, after_src_off);
            let replacement = copy_bounded(text, self.limits.text)?;
            self.publish_input(
                range,
                replacement,
                (before, after),
                OperationSource::Replace,
                now_millis,
                None,
            )?
        };
        self.settle_command_outcome(outcome)
    }

    /// 按当前源码选区接收普通文本。源码模式复用普通输入草稿和 900ms 协议。
    pub fn type_selection(
        &mut self,
        identity: &InputIdentity,
        text: &str,
        now_millis: u64,
    ) -> Result<(), DocError> {
        self.ensure_writable()?;
        self.accept_document_result(identity)?;
        if self.is_composition_active() {
            return Err(DocError::CompositionActive);
        }
        if text.is_empty() {
            return Ok(());
        }
        let selection = self.current_selection();
        let range = selection.range();
        if self.mode == EditorMode::Visual {
            let extent = self.logical_visual_extent()?.ok_or(DocError::NoOpenBlock)?;
            if !contains(&extent, &range) {
                return Err(DocError::OutsideOpenBlock);
            }
        }
        let replacing = !range.is_empty();
        if replacing {
            self.check_open_edit_range(&range)?;
            let current = self.cached_current_text()?;
            let selected = current.src_range(range.clone())?;
            if current.slice(&selected, self.limits.text)? == text {
                let caret = bind_selection(current, SelectionSpec::new(range.end, range.end))?;
                return self.select(identity, &caret);
            }
        }
        let locally_opened = self.open.is_none() && self.mode == EditorMode::Source;
        if locally_opened {
            self.open_input_target(InputTarget::Source, selection.range(), selection)?;
        }
        let result = if replacing {
            self.start_group(selection, text, now_millis)
        } else {
            self.type_text(selection.head(), text, now_millis)
        };
        if locally_opened && result.is_err() {
            self.open = None;
        }
        result
    }

    /// 返回当前打开的整个可视化块，含其草稿，不含块尾分隔换行。
    /// 调用方须传本会话的当前快照。旧工作版本或快照拒绝；只读及源码模式无可视化块。
    pub fn visual_block_range(
        &self,
        snapshot: &CurrentSnapshot,
    ) -> Result<Option<SrcRange>, DocError> {
        self.ensure_document_alive()?;
        if snapshot.document_id() != &self.document {
            return Err(DocError::WrongDocument);
        }
        if snapshot.version() != self.version
            || snapshot.snapshot_id() != self.cached_current_text()?.id()
        {
            return Err(DocError::StaleResult);
        }
        self.logical_visual_extent()?
            .map(|extent| snapshot.text().src_range(extent).map_err(DocError::Buffer))
            .transpose()
    }

    fn logical_visual_extent(&self) -> Result<Option<Range<usize>>, DocError> {
        let Some(block) = &self.open else {
            return Ok(None);
        };
        let Some(extent) = block.visual_extent() else {
            return Ok(None);
        };
        let mut logical = extent.clone();
        if let Some(draft) = block.current_draft() {
            // 这里仅在原有源码单位上计算块的新长度；投影转换由调用方完成。
            logical.end = logical
                .end
                .checked_sub(block.range.len())
                .and_then(|value| value.checked_add(draft.len()))
                .ok_or_else(overflow)?;
        }
        Ok(Some(logical))
    }

    /// 核对普通输入请求没有越过当前逻辑块边界。
    pub(super) fn check_open_edit_range(&self, range: &Range<usize>) -> Result<(), DocError> {
        if self
            .logical_visual_extent()?
            .is_some_and(|extent| !contains(&extent, range))
        {
            return Err(DocError::OutsideOpenBlock);
        }
        if let Some(block) = self.open.as_ref()
            && let Some(target) = block.visual_input()
            && !target.protections.is_empty()
        {
            if let Some(draft) = block.current_draft() {
                // 只在该块含保护区时变换它们。当前草稿只可能是一条受检源码补丁。
                let snapshot = self.buffer.snapshot();
                let draft = copy_bounded(draft, self.limits.text)?;
                let batch = CheckedBatch::new(
                    snapshot,
                    vec![(block.range.clone(), draft)],
                    self.patch_budget(),
                )
                .map_err(DocError::Buffer)?;
                target.check_permissions(range, |protected| {
                    transform_extent(snapshot, &batch, protected)
                })?;
            } else {
                target.check_permissions(range, |protected| Ok(protected.clone()))?;
            }
        }
        Ok(())
    }
}

impl VisualInput {
    pub(super) fn from_parse(
        extent: Range<usize>,
        protected: &[ProtectedRange],
        styled: &[StyledRange],
    ) -> Result<Self, DocError> {
        let mut protections = Vec::new();
        let ranges = protected
            .iter()
            .map(|item| (item.range(), item.kind()))
            .chain(
                styled
                    .iter()
                    .map(|item| (item.range(), ProtectionKind::Html)),
            );
        for (range, kind) in ranges {
            let range = range.byte_range();
            if touches(&extent, &range) {
                protections
                    .try_reserve(1)
                    .map_err(|error| DocError::Buffer(BufferError::Allocation(error)))?;
                protections.push(VisualProtection { range, kind });
            }
        }
        Ok(Self {
            extent,
            protections,
        })
    }

    pub(super) fn check_permissions(
        &self,
        target: &Range<usize>,
        mut project: impl FnMut(&Range<usize>) -> Result<Range<usize>, DocError>,
    ) -> Result<(), DocError> {
        for protection in &self.protections {
            let range = project(&protection.range)?;
            if touches(target, &range) {
                return Err(DocError::Edit(match protection.kind {
                    ProtectionKind::Html => EditRefusal::HtmlBoundary { range },
                    kind => EditRefusal::ProtectedRange { kind, range },
                }));
            }
        }
        Ok(())
    }
}

fn contains(block: &Range<usize>, range: &Range<usize>) -> bool {
    block.start <= range.start && range.end <= block.end
}

fn touches(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start <= right.end && right.start <= left.end
}
