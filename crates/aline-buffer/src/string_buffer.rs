//! 单一源码缓冲：原子批次、行索引、选区、单调代次与历史；不解析、不写盘。
//!
//! 所有失败路径都在发布前返回，文本、行索引、选区、代次与历史保持不变。

use std::borrow::Cow;
use std::ops::Range;
use std::sync::Arc;

use crate::history::History;
use crate::{
    BufferError, CheckedBatch, DocumentRevision, HistoryBudget, HistoryStep, HistoryStepHandle,
    LineIndex, LineIndexBudget, OperationSource, SelectionSpec, SrcSelection, TextBudget,
    TextSnapshot, TrimOutcome,
};

/// 一次成功批次的结果与可复核逆补丁。
#[derive(Debug)]
pub struct AppliedBatch {
    inverse: Arc<Vec<(Range<usize>, String)>>,
    revision: DocumentRevision,
    line_count: usize,
    after_selection: SelectionSpec,
}

impl AppliedBatch {
    /// 返回 after 坐标的逆补丁，可用于独立复核或显式回滚。
    pub fn inverse_patches(&self) -> &[(Range<usize>, String)] {
        self.inverse.as_slice()
    }

    /// 返回发布后的文档代次。
    pub const fn revision(&self) -> DocumentRevision {
        self.revision
    }

    /// 返回发布后的行数。
    pub const fn line_count(&self) -> usize {
        self.line_count
    }

    /// 返回发布后的原始字节选区。
    pub const fn after_selection(&self) -> SelectionSpec {
        self.after_selection
    }
}

/// 批次应用结果；无实际补丁时不发布新身份或历史步。
#[derive(Debug)]
pub enum ApplyOutcome {
    /// 全部补丁都是内容相同的替换或空插入，状态零变化。
    Unchanged,
    /// 至少一个补丁实际改变文本，已原子发布。
    Changed(AppliedBatch),
}

/// 显式重载结果；重载只产生内存历史边界，不移动任何磁盘基线。
#[derive(Debug)]
pub enum ReloadOutcome {
    /// 新文本与当前文本逐字节相同，状态零变化。
    Unchanged,
    /// 新文本已作为独立历史边界发布。
    Changed {
        /// 重载后的文档代次。
        revision: DocumentRevision,
    },
}

/// 拥有快照、选区、行索引与历史的可变源码缓冲。
pub struct StringBuffer {
    snapshot: TextSnapshot,
    selection: SrcSelection,
    line_index: LineIndex,
    history: History,
    index_budget: LineIndexBudget,
}

impl StringBuffer {
    /// 接管完整文本并建立初始行索引、选区与空历史；所有预算在分配前检查。
    pub fn new(
        text: String,
        selection: SelectionSpec,
        text_budget: TextBudget,
        index_budget: LineIndexBudget,
        history_budget: HistoryBudget,
    ) -> Result<Self, BufferError> {
        let line_index = LineIndex::build(&text, index_budget)?;
        let snapshot = TextSnapshot::new(text, text_budget)?;
        let selection = bind_selection(&snapshot, selection)?;
        Ok(Self {
            snapshot,
            selection,
            line_index,
            history: History::new(history_budget),
            index_budget,
        })
    }

    /// 返回当前不可变源码快照。
    pub fn snapshot(&self) -> &TextSnapshot {
        &self.snapshot
    }

    /// 返回绑定当前快照的方向性选区。
    pub fn selection(&self) -> &SrcSelection {
        &self.selection
    }

    /// 返回与当前文本同步的行索引。
    pub fn line_index(&self) -> &LineIndex {
        &self.line_index
    }

    /// 返回当前已提交代次。
    pub const fn revision(&self) -> DocumentRevision {
        self.snapshot.revision()
    }

    /// 返回当前文本字节长度。
    pub const fn len_bytes(&self) -> usize {
        self.snapshot.len_bytes()
    }

    /// 返回当前行数。
    pub fn line_count(&self) -> usize {
        self.line_index.line_count()
    }

    /// 在显式消费点物化当前文本，受字节预算约束。
    pub fn text(&self, budget: TextBudget) -> Result<Cow<'_, str>, BufferError> {
        let range = self.snapshot.src_range(0..self.snapshot.len_bytes())?;
        self.snapshot.slice(&range, budget)
    }

    /// 返回字节偏移所在行；换行字节归属其所在行。
    pub fn line_of_offset(&self, offset: usize) -> Option<usize> {
        self.line_index.line_of_offset(offset)
    }

    /// 对当前快照复验并原子应用批次；预检、物化、行索引和历史预算全部成功才发布。
    ///
    /// 内容相同的替换和空文本插入会被剔除；没有实际补丁时返回 `Unchanged`，
    /// 不改选区、代次、历史或 redo 分支。
    pub fn apply(
        &mut self,
        batch: &CheckedBatch,
        after_selection: SelectionSpec,
        source: OperationSource,
        now_millis: u64,
        text_budget: TextBudget,
    ) -> Result<ApplyOutcome, BufferError> {
        batch.validate(&self.snapshot)?;
        let forward = effective_patches(&self.snapshot, batch.patches(), text_budget)?;
        if forward.is_empty() {
            return Ok(ApplyOutcome::Unchanged);
        }
        let after_text =
            build_after_text(&self.snapshot, &forward, batch.output_len(), text_budget)?;
        let inverse = Arc::new(build_inverse_patches(
            &self.snapshot,
            &forward,
            text_budget,
        )?);
        let after_index = LineIndex::build(&after_text, self.index_budget)?;
        let after_revision = self.snapshot.revision().next()?;
        let before_selection = selection_spec(&self.selection);
        let before_len = self.snapshot.len_bytes();
        let after_snapshot =
            TextSnapshot::from_shared(Arc::new(after_text), after_revision, text_budget)?;
        let after_selection_bound = bind_selection(&after_snapshot, after_selection)?;
        let after_len = after_snapshot.len_bytes();
        let step = HistoryStep {
            forward: Arc::new(forward),
            inverse: Arc::clone(&inverse),
            before_selection,
            after_selection,
            source,
            before_revision: self.snapshot.revision(),
            after_revision,
            after_id: after_snapshot.id().clone(),
            before_len,
            after_len,
        };
        let plan = self.history.prepare_record(&step, source, now_millis)?;
        let line_count = after_index.line_count();
        self.snapshot = after_snapshot;
        self.selection = after_selection_bound;
        self.line_index = after_index;
        self.history.commit_record(plan, step, now_millis);
        Ok(ApplyOutcome::Changed(AppliedBatch {
            inverse,
            revision: after_revision,
            line_count,
            after_selection,
        }))
    }

    /// 撤销最近一个历史节点；恢复该节点首个步骤之前的选区和文本。
    ///
    /// 失败时文本、行索引、选区、代次和历史光标都不移动。
    pub fn undo(&mut self, text_budget: TextBudget) -> Result<DocumentRevision, BufferError> {
        let movement = self.history.undo_move().ok_or(BufferError::NothingToUndo)?;
        let mut text = self.contiguous_text(text_budget)?;
        for step in movement.steps.iter().rev() {
            if text.len() != step.after_len() {
                return Err(BufferError::HistoryContextMismatch);
            }
            text = apply_patches_to_text(&text, step.inverse_patches(), text_budget)?;
        }
        let first = movement
            .steps
            .first()
            .ok_or(BufferError::HistoryContextMismatch)?;
        let target_selection = first.before_selection();
        let revision = self.snapshot.revision().next()?;
        let line_index = LineIndex::build(&text, self.index_budget)?;
        let snapshot = TextSnapshot::from_shared(Arc::new(text), revision, text_budget)?;
        let selection = bind_selection(&snapshot, target_selection)?;
        self.snapshot = snapshot;
        self.selection = selection;
        self.line_index = line_index;
        self.history.commit_undo();
        Ok(revision)
    }

    /// 重做下一个历史节点；恢复该节点末个步骤之后的选区。
    ///
    /// 失败时文本、行索引、选区、代次和历史光标都不移动。
    pub fn redo(&mut self, text_budget: TextBudget) -> Result<DocumentRevision, BufferError> {
        let movement = self.history.redo_move().ok_or(BufferError::NothingToRedo)?;
        let mut text = self.contiguous_text(text_budget)?;
        for step in &movement.steps {
            if text.len() != step.before_len() {
                return Err(BufferError::HistoryContextMismatch);
            }
            text = apply_patches_to_text(&text, step.forward_patches(), text_budget)?;
        }
        let last = movement
            .steps
            .last()
            .ok_or(BufferError::HistoryContextMismatch)?;
        let target_selection = last.after_selection();
        let revision = self.snapshot.revision().next()?;
        let line_index = LineIndex::build(&text, self.index_budget)?;
        let snapshot = TextSnapshot::from_shared(Arc::new(text), revision, text_budget)?;
        let selection = bind_selection(&snapshot, target_selection)?;
        self.snapshot = snapshot;
        self.selection = selection;
        self.line_index = line_index;
        self.history.commit_redo();
        Ok(revision)
    }

    /// 将整段新文本作为独立历史边界发布；只改内存，不触碰磁盘基线。
    ///
    /// 逐字节相同的重载返回 `Unchanged`，不产生空历史步，也不清除 redo。
    pub fn reload(
        &mut self,
        new_text: String,
        after_selection: SelectionSpec,
        now_millis: u64,
        text_budget: TextBudget,
    ) -> Result<ReloadOutcome, BufferError> {
        let old_text = self.contiguous_text(text_budget)?;
        if old_text == new_text {
            return Ok(ReloadOutcome::Unchanged);
        }
        let line_index = LineIndex::build(&new_text, self.index_budget)?;
        let revision = self.snapshot.revision().next()?;
        let before_selection = selection_spec(&self.selection);
        let before_len = old_text.len();
        let new_len = new_text.len();
        let forward = Arc::new(vec![(0..before_len, new_text.clone())]);
        let inverse = Arc::new(vec![(0..new_len, old_text)]);
        let after_snapshot = TextSnapshot::from_shared(Arc::new(new_text), revision, text_budget)?;
        let selection = bind_selection(&after_snapshot, after_selection)?;
        let step = HistoryStep {
            forward,
            inverse,
            before_selection,
            after_selection,
            source: OperationSource::Reload,
            before_revision: self.snapshot.revision(),
            after_revision: revision,
            after_id: after_snapshot.id().clone(),
            before_len,
            after_len: new_len,
        };
        let plan = self
            .history
            .prepare_record(&step, OperationSource::Reload, now_millis)?;
        self.snapshot = after_snapshot;
        self.selection = selection;
        self.line_index = line_index;
        self.history.commit_record(plan, step, now_millis);
        Ok(ReloadOutcome::Changed { revision })
    }

    /// 将当前快照标记为保存点；保存点身份与内容证据分开记录。
    pub fn mark_saved(&mut self) {
        self.history
            .mark_saved(self.snapshot.id(), self.snapshot.revision());
    }

    /// 判断当前快照是否就是最近一次 `mark_saved` 的身份。
    pub fn is_at_save_point(&self) -> bool {
        self.history.is_at_save_point(self.snapshot.id())
    }

    /// 返回最近保存点记录的快照代次。
    pub fn saved_revision(&self) -> Option<DocumentRevision> {
        self.history.saved_revision()
    }

    /// 结束当前待定输入组，使后续输入不并入上一个节点。
    pub fn seal_pending(&mut self) {
        self.history.seal_pending();
    }

    /// 可撤销节点数。
    pub fn undo_depth(&self) -> usize {
        self.history.cursor()
    }

    /// 判断当前是否有可撤销节点。
    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    /// 判断当前是否有可重做节点。
    pub fn can_redo(&self) -> bool {
        self.history.can_redo()
    }

    /// 可重做节点数。
    pub fn redo_depth(&self) -> usize {
        self.history.len().saturating_sub(self.history.cursor())
    }

    /// 历史节点总数，包含 redo 分支。
    pub fn history_node_count(&self) -> usize {
        self.history.len()
    }

    /// 返回指定历史节点的共享步骤句柄；句柄存活期间补丁文本不被释放。
    pub fn history_step(&self, index: usize) -> Option<HistoryStepHandle> {
        self.history.step_handle(index)
    }

    /// 从最旧节点裁剪到目标节点数；保存点与当前节点所在前缀受保护。
    pub fn trim_history(&mut self, max_nodes: usize) -> TrimOutcome {
        self.history.trim_to(max_nodes)
    }

    fn contiguous_text(&self, budget: TextBudget) -> Result<String, BufferError> {
        let range = self.snapshot.src_range(0..self.snapshot.len_bytes())?;
        Ok(self.snapshot.slice(&range, budget)?.into_owned())
    }
}

fn selection_spec(selection: &SrcSelection) -> SelectionSpec {
    SelectionSpec::new(
        selection.anchor().byte_offset(),
        selection.head().byte_offset(),
    )
}

fn bind_selection(
    snapshot: &TextSnapshot,
    selection: SelectionSpec,
) -> Result<SrcSelection, BufferError> {
    SrcSelection::new(
        snapshot.src_offset(selection.anchor())?,
        snapshot.src_offset(selection.head())?,
    )
}

fn effective_patches(
    snapshot: &TextSnapshot,
    patches: &[(Range<usize>, String)],
    budget: TextBudget,
) -> Result<Vec<(Range<usize>, String)>, BufferError> {
    let mut effective = Vec::new();
    effective
        .try_reserve_exact(patches.len())
        .map_err(BufferError::Allocation)?;
    for (range, replacement) in patches {
        let unchanged = if range.len() != replacement.len() {
            false
        } else if range.is_empty() {
            true
        } else {
            let src = snapshot.src_range(range.clone())?;
            snapshot.slice(&src, budget)?.as_bytes() == replacement.as_bytes()
        };
        if !unchanged {
            effective.push((range.clone(), replacement.clone()));
        }
    }
    Ok(effective)
}

fn build_after_text(
    snapshot: &TextSnapshot,
    patches: &[(Range<usize>, String)],
    expected_len: usize,
    budget: TextBudget,
) -> Result<String, BufferError> {
    let full = snapshot.src_range(0..snapshot.len_bytes())?;
    let source = snapshot.slice(&full, budget)?;
    budget.check(expected_len)?;
    let mut output = String::new();
    output
        .try_reserve_exact(expected_len)
        .map_err(BufferError::Allocation)?;
    let mut cursor = 0usize;
    for (range, replacement) in patches {
        output.push_str(
            source
                .get(cursor..range.start)
                .ok_or(BufferError::InvalidBacking)?,
        );
        output.push_str(replacement);
        cursor = range.end;
    }
    output.push_str(source.get(cursor..).ok_or(BufferError::InvalidBacking)?);
    if output.len() != expected_len {
        return Err(BufferError::PatchOutputMismatch {
            expected: expected_len,
            actual: output.len(),
        });
    }
    Ok(output)
}

fn build_inverse_patches(
    snapshot: &TextSnapshot,
    patches: &[(Range<usize>, String)],
    budget: TextBudget,
) -> Result<Vec<(Range<usize>, String)>, BufferError> {
    let mut inverse: Vec<(Range<usize>, String)> = Vec::new();
    inverse
        .try_reserve_exact(patches.len())
        .map_err(BufferError::Allocation)?;
    let mut removed = 0usize;
    let mut inserted = 0usize;
    let mut inverse_bytes = 0usize;
    for (range, replacement) in patches {
        let after_start = range
            .start
            .checked_sub(removed)
            .and_then(|value| value.checked_add(inserted))
            .ok_or(BufferError::LengthOverflow)?;
        let after_end = after_start
            .checked_add(replacement.len())
            .ok_or(BufferError::LengthOverflow)?;
        let src = snapshot.src_range(range.clone())?;
        let restored = snapshot.slice(&src, budget)?;
        inverse_bytes = inverse_bytes
            .checked_add(restored.len())
            .ok_or(BufferError::LengthOverflow)?;
        budget.check(inverse_bytes)?;
        if let Some((last_range, last_text)) = inverse.last_mut()
            && last_range.end == after_start
        {
            last_text
                .try_reserve(restored.len())
                .map_err(BufferError::Allocation)?;
            last_text.push_str(&restored);
            last_range.end = after_end;
        } else {
            inverse.push((after_start..after_end, restored.into_owned()));
        }
        removed = removed
            .checked_add(range.len())
            .ok_or(BufferError::LengthOverflow)?;
        inserted = inserted
            .checked_add(replacement.len())
            .ok_or(BufferError::LengthOverflow)?;
    }
    Ok(inverse)
}

fn apply_patches_to_text(
    input: &str,
    patches: &[(Range<usize>, String)],
    budget: TextBudget,
) -> Result<String, BufferError> {
    let mut removed = 0usize;
    let mut inserted = 0usize;
    let mut previous: Option<&Range<usize>> = None;
    for (range, replacement) in patches {
        if range.start > range.end {
            return Err(BufferError::ReversedRange);
        }
        if range.end > input.len() {
            return Err(BufferError::OutOfBounds {
                offset: range.end,
                len: input.len(),
            });
        }
        if !input.is_char_boundary(range.start) {
            return Err(BufferError::NotCharBoundary {
                offset: range.start,
            });
        }
        if !input.is_char_boundary(range.end) {
            return Err(BufferError::NotCharBoundary { offset: range.end });
        }
        if let Some(prior) = previous {
            if range.start < prior.start {
                return Err(BufferError::UnsortedPatches);
            }
            if range.start < prior.end {
                return Err(BufferError::OverlappingPatches);
            }
            if prior.is_empty() && range.is_empty() && prior.start == range.start {
                return Err(BufferError::AmbiguousInsertion);
            }
        }
        removed = removed
            .checked_add(range.len())
            .ok_or(BufferError::LengthOverflow)?;
        inserted = inserted
            .checked_add(replacement.len())
            .ok_or(BufferError::LengthOverflow)?;
        previous = Some(range);
    }
    let output_len = input
        .len()
        .checked_sub(removed)
        .and_then(|value| value.checked_add(inserted))
        .ok_or(BufferError::LengthOverflow)?;
    budget.check(output_len)?;
    let mut output = String::new();
    output
        .try_reserve_exact(output_len)
        .map_err(BufferError::Allocation)?;
    let mut cursor = 0usize;
    for (range, replacement) in patches {
        output.push_str(
            input
                .get(cursor..range.start)
                .ok_or(BufferError::HistoryContextMismatch)?,
        );
        output.push_str(replacement);
        cursor = range.end;
    }
    output.push_str(
        input
            .get(cursor..)
            .ok_or(BufferError::HistoryContextMismatch)?,
    );
    if output.len() != output_len {
        return Err(BufferError::PatchOutputMismatch {
            expected: output_len,
            actual: output.len(),
        });
    }
    Ok(output)
}
