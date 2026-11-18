//! 文档历史节点、待定输入组与保存点；只保存补丁和选区，不解析或写盘。

use std::ops::Range;
use std::sync::Arc;

use crate::{BufferError, DocumentRevision, SnapshotId};

/// 相邻普通输入合并为同一待定组的最大时间间隔。
pub const TYPING_MERGE_WINDOW_MILLIS: u64 = 900;

/// 产生一次文本变化的上层操作来源；只用于历史分组，不含任何解析语义。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OperationSource {
    /// 普通字符输入。
    Typing,
    /// 替换操作。
    Replace,
    /// 命令式编辑。
    Command,
    /// 输入法提交。
    ImeCommit,
    /// 重载产生的独立边界。
    Reload,
}

/// 未绑定快照的原始字节选区，两端由调用方保证已在目标文本上校验。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionSpec {
    anchor: usize,
    head: usize,
}

impl SelectionSpec {
    /// 创建原始字节选区；方向保留，不在此处校验边界。
    pub const fn new(anchor: usize, head: usize) -> Self {
        Self { anchor, head }
    }

    /// 返回锚点字节位置。
    pub const fn anchor(self) -> usize {
        self.anchor
    }

    /// 返回活动端点字节位置。
    pub const fn head(self) -> usize {
        self.head
    }

    /// 判断活动端点是否位于锚点之前。
    pub const fn is_reversed(self) -> bool {
        self.anchor > self.head
    }

    /// 返回排序后的半开字节范围，不修改方向。
    pub const fn range(self) -> Range<usize> {
        let start = if self.anchor < self.head {
            self.anchor
        } else {
            self.head
        };
        let end = if self.anchor > self.head {
            self.anchor
        } else {
            self.head
        };
        start..end
    }
}

/// 历史保留的节点数上限和补丁文本字节上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryBudget {
    max_nodes: usize,
    max_patch_bytes: usize,
}

impl HistoryBudget {
    /// 创建由宿主注入的历史预算，不声明发布资源阈值。
    pub const fn new(max_nodes: usize, max_patch_bytes: usize) -> Self {
        Self {
            max_nodes,
            max_patch_bytes,
        }
    }

    /// 返回历史节点数上限。
    pub const fn max_nodes(self) -> usize {
        self.max_nodes
    }

    /// 返回全部历史补丁文本的合计字节上限。
    pub const fn max_patch_bytes(self) -> usize {
        self.max_patch_bytes
    }
}

/// 一次已发布变化的正逆补丁与前后状态；文本由 `Arc` 句柄共享。
#[derive(Debug)]
pub struct HistoryStep {
    pub(crate) forward: Arc<Vec<(Range<usize>, String)>>,
    pub(crate) inverse: Arc<Vec<(Range<usize>, String)>>,
    pub(crate) before_selection: SelectionSpec,
    pub(crate) after_selection: SelectionSpec,
    pub(crate) source: OperationSource,
    pub(crate) before_revision: DocumentRevision,
    pub(crate) after_revision: DocumentRevision,
    pub(crate) after_id: SnapshotId,
    pub(crate) before_len: usize,
    pub(crate) after_len: usize,
}

impl HistoryStep {
    /// 返回 before 坐标的正补丁。
    pub fn forward_patches(&self) -> &[(Range<usize>, String)] {
        self.forward.as_slice()
    }

    /// 返回 after 坐标的逆补丁。
    pub fn inverse_patches(&self) -> &[(Range<usize>, String)] {
        self.inverse.as_slice()
    }

    /// 返回应用前的原始选区。
    pub const fn before_selection(&self) -> SelectionSpec {
        self.before_selection
    }

    /// 返回应用后的原始选区。
    pub const fn after_selection(&self) -> SelectionSpec {
        self.after_selection
    }

    /// 返回产生此步的操作来源。
    pub const fn source(&self) -> OperationSource {
        self.source
    }

    /// 返回应用前代次。
    pub const fn before_revision(&self) -> DocumentRevision {
        self.before_revision
    }

    /// 返回应用后代次。
    pub const fn after_revision(&self) -> DocumentRevision {
        self.after_revision
    }

    /// 返回应用前文本字节长度。
    pub const fn before_len(&self) -> usize {
        self.before_len
    }

    /// 返回应用后文本字节长度。
    pub const fn after_len(&self) -> usize {
        self.after_len
    }

    pub(crate) fn after_id(&self) -> &SnapshotId {
        &self.after_id
    }
}

/// 对历史步骤的共享句柄；clone 不复制补丁文本。
#[derive(Clone, Debug)]
pub struct HistoryStepHandle(Arc<HistoryStep>);

impl HistoryStepHandle {
    /// 返回共享步骤的只读引用。
    pub fn get(&self) -> &HistoryStep {
        &self.0
    }
}

/// 一次手动历史裁剪的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrimOutcome {
    removed: usize,
    retained: usize,
}

impl TrimOutcome {
    /// 返回被移除的旧节点数。
    pub const fn removed(self) -> usize {
        self.removed
    }

    /// 返回裁剪后保留的节点数。
    pub const fn retained(self) -> usize {
        self.retained
    }
}

#[derive(Debug)]
struct HistoryNode {
    source: OperationSource,
    steps: Vec<Arc<HistoryStep>>,
    pending: bool,
    last_input_at: u64,
}

#[derive(Debug)]
struct SavePoint {
    snapshot_id: SnapshotId,
    revision: DocumentRevision,
    node_index: Option<usize>,
}

#[derive(Clone, Copy, Debug)]
struct RecordPlan {
    truncate_to: usize,
    merge_node: Option<usize>,
    remove_nodes: usize,
}

#[derive(Debug)]
pub(crate) struct HistoryMove {
    pub(crate) steps: Vec<Arc<HistoryStep>>,
}

/// 线性历史栈；保存点按快照身份保护，裁剪只从最旧节点开始。
#[derive(Debug)]
pub(crate) struct History {
    nodes: Vec<HistoryNode>,
    cursor: usize,
    saved: Option<SavePoint>,
    budget: HistoryBudget,
}

impl History {
    pub(crate) fn new(budget: HistoryBudget) -> Self {
        Self {
            nodes: Vec::new(),
            cursor: 0,
            saved: None,
            budget,
        }
    }

    fn protected_prefix(&self, truncate_to: usize) -> usize {
        let current_guard = truncate_to.saturating_sub(1);
        match &self.saved {
            Some(point) => match point.node_index {
                Some(index) => current_guard.min(index),
                None => 0,
            },
            None => current_guard,
        }
    }

    fn bytes_across(&self, end: usize) -> Result<usize, BufferError> {
        let mut total = 0usize;
        for node in self.nodes.get(..end).unwrap_or(&[]) {
            for step in &node.steps {
                total = total
                    .checked_add(step_bytes(step)?)
                    .ok_or(BufferError::LengthOverflow)?;
            }
        }
        Ok(total)
    }

    fn prepare_record(
        &self,
        step: &HistoryStep,
        now_millis: u64,
    ) -> Result<RecordPlan, BufferError> {
        let truncate_to = self.cursor;
        let merge_node = if step.source() == OperationSource::Typing && truncate_to > 0 {
            let index = truncate_to - 1;
            let node = self.nodes.get(index);
            match node {
                Some(node)
                    if node.pending
                        && node.source == OperationSource::Typing
                        && now_millis >= node.last_input_at
                        && now_millis - node.last_input_at <= TYPING_MERGE_WINDOW_MILLIS
                        && node
                            .steps
                            .last()
                            .is_some_and(|previous| can_merge(previous, step)) =>
                {
                    Some(index)
                }
                _ => None,
            }
        } else {
            None
        };
        let prospective_nodes = truncate_to
            .checked_add(if merge_node.is_some() { 0 } else { 1 })
            .ok_or(BufferError::LengthOverflow)?;
        self.prepare_record_capacity(prospective_nodes, step_bytes(step)?, merge_node)
    }

    fn prepare_record_capacity(
        &self,
        prospective_nodes: usize,
        added_bytes: usize,
        merge_node: Option<usize>,
    ) -> Result<RecordPlan, BufferError> {
        let truncate_to = self.cursor;
        let existing_bytes = self.bytes_across(truncate_to)?;
        let protected = self.protected_prefix(truncate_to);
        let mut removed_bytes = 0usize;
        let mut last_candidate = None;
        for remove in 0..=protected {
            let nodes_after = prospective_nodes
                .checked_sub(remove)
                .ok_or(BufferError::LengthOverflow)?;
            let bytes_after = existing_bytes
                .checked_sub(removed_bytes)
                .and_then(|value| value.checked_add(added_bytes))
                .ok_or(BufferError::LengthOverflow)?;
            last_candidate = Some((nodes_after, bytes_after));
            if nodes_after <= self.budget.max_nodes()
                && bytes_after <= self.budget.max_patch_bytes()
            {
                return Ok(RecordPlan {
                    truncate_to,
                    merge_node,
                    remove_nodes: remove,
                });
            }
            if let Some(node) = self.nodes.get(remove) {
                for candidate in &node.steps {
                    removed_bytes = removed_bytes
                        .checked_add(step_bytes(candidate)?)
                        .ok_or(BufferError::LengthOverflow)?;
                }
            }
        }
        let Some((final_nodes, final_bytes)) = last_candidate else {
            return Err(BufferError::HistoryBudgetExceeded {
                needed: prospective_nodes,
                limit: self.budget.max_nodes(),
            });
        };
        if final_nodes > self.budget.max_nodes() {
            return Err(BufferError::HistoryBudgetExceeded {
                needed: prospective_nodes,
                limit: self.budget.max_nodes(),
            });
        }
        Err(BufferError::HistoryBudgetExceeded {
            needed: final_bytes,
            limit: self.budget.max_patch_bytes(),
        })
    }

    fn truncate_and_trim_for_record(&mut self, plan: RecordPlan) {
        self.nodes.truncate(plan.truncate_to);
        self.cursor = plan.truncate_to;
        if let Some(point) = &mut self.saved
            && point
                .node_index
                .is_some_and(|index| index >= self.nodes.len())
        {
            point.node_index = None;
        }
        if plan.remove_nodes > 0 {
            self.nodes.drain(0..plan.remove_nodes);
            self.cursor = self.cursor.saturating_sub(plan.remove_nodes);
            if let Some(point) = &mut self.saved
                && let Some(index) = &mut point.node_index
            {
                *index = index.saturating_sub(plan.remove_nodes);
            }
        }
    }

    /// 预算与容量受检后，发布候选命令及其有变化的输入节点。
    pub(crate) fn record_staged_command(
        &mut self,
        typing: Option<Arc<HistoryStep>>,
        command: Arc<HistoryStep>,
        now_millis: u64,
    ) -> Result<(), BufferError> {
        debug_assert!(
            typing
                .as_ref()
                .is_none_or(|step| step.source() == OperationSource::Typing)
        );
        debug_assert_eq!(command.source(), OperationSource::Command);
        let prospective_nodes = self
            .cursor
            .checked_add(1 + usize::from(typing.is_some()))
            .ok_or(BufferError::LengthOverflow)?;
        let typing_bytes = typing.as_deref().map(step_bytes).transpose()?.unwrap_or(0);
        let added_bytes = typing_bytes
            .checked_add(step_bytes(&command)?)
            .ok_or(BufferError::LengthOverflow)?;
        let plan = self.prepare_record_capacity(prospective_nodes, added_bytes, None)?;
        let needed = prospective_nodes
            .checked_sub(plan.remove_nodes)
            .ok_or(BufferError::LengthOverflow)?;
        let typing = typing
            .map(|step| sealed_node(step, now_millis))
            .transpose()?;
        let command = sealed_node(command, now_millis)?;
        self.nodes
            .try_reserve(needed.saturating_sub(self.nodes.len()))
            .map_err(BufferError::Allocation)?;

        self.truncate_and_trim_for_record(plan);
        self.seal_pending();
        if let Some(typing) = typing {
            self.nodes.push(typing);
        }
        self.nodes.push(command);
        self.cursor = self.nodes.len();
        Ok(())
    }

    /// 在修改历史前预留新增节点或步骤的容量。
    pub(crate) fn record(
        &mut self,
        step: Arc<HistoryStep>,
        now_millis: u64,
    ) -> Result<(), BufferError> {
        let plan = self.prepare_record(&step, now_millis)?;
        if let Some(index) = plan.merge_node {
            let node = self
                .nodes
                .get_mut(index)
                .ok_or(BufferError::HistoryContextMismatch)?;
            node.steps.try_reserve(1).map_err(BufferError::Allocation)?;
            // 被合并节点在受保护前缀之后，截断和裁剪均会保留此节点。
            node.steps.push(step);
            node.pending = true;
            node.last_input_at = now_millis;
            self.truncate_and_trim_for_record(plan);
            return Ok(());
        }
        let retained = plan
            .truncate_to
            .checked_sub(plan.remove_nodes)
            .ok_or(BufferError::HistoryContextMismatch)?;
        let needed = retained.checked_add(1).ok_or(BufferError::LengthOverflow)?;
        let mut node = sealed_node(step, now_millis)?;
        node.pending = node.source == OperationSource::Typing;
        self.nodes
            .try_reserve(needed.saturating_sub(self.nodes.len()))
            .map_err(BufferError::Allocation)?;
        self.truncate_and_trim_for_record(plan);
        self.seal_pending();
        self.nodes.push(node);
        self.cursor = self.nodes.len();
        Ok(())
    }

    pub(crate) fn undo_move(&self) -> Option<HistoryMove> {
        if self.cursor == 0 {
            return None;
        }
        let node = self.nodes.get(self.cursor - 1)?;
        Some(HistoryMove {
            steps: node.steps.clone(),
        })
    }

    pub(crate) fn redo_move(&self) -> Option<HistoryMove> {
        let node = self.nodes.get(self.cursor)?;
        Some(HistoryMove {
            steps: node.steps.clone(),
        })
    }

    /// 下一次撤销将触及的源码范围并集；逆补丁使用 after 坐标。
    ///
    /// 只读查询，不移动光标、不改文本或选区；没有可撤销节点时返回 `None`。
    pub(crate) fn undo_target_ranges(&self) -> Option<Vec<Range<usize>>> {
        let node = self.nodes.get(self.cursor.checked_sub(1)?)?;
        Some(collect_patch_ranges(&node.steps, true))
    }

    /// 下一次重做将触及的源码范围并集；正补丁使用 before 坐标。
    ///
    /// 只读查询，不移动光标、不改文本或选区；没有可重做节点时返回 `None`。
    pub(crate) fn redo_target_ranges(&self) -> Option<Vec<Range<usize>>> {
        let node = self.nodes.get(self.cursor)?;
        Some(collect_patch_ranges(&node.steps, false))
    }

    pub(crate) fn commit_undo(&mut self) {
        if self.cursor > 0 {
            if let Some(node) = self.nodes.get_mut(self.cursor) {
                node.pending = false;
            }
            self.cursor -= 1;
        }
    }

    pub(crate) fn commit_redo(&mut self) {
        if let Some(node) = self.nodes.get_mut(self.cursor) {
            node.pending = false;
        }
        self.cursor = self.cursor.saturating_add(1);
    }

    pub(crate) fn discard_redo_branch(&mut self) {
        self.nodes.truncate(self.cursor);
        if let Some(point) = &mut self.saved
            && point
                .node_index
                .is_some_and(|index| index >= self.nodes.len())
        {
            point.node_index = None;
        }
    }

    pub(crate) fn seal_pending(&mut self) {
        if let Some(node) = self.nodes.get_mut(self.cursor.saturating_sub(1)) {
            node.pending = false;
        }
    }

    pub(crate) fn mark_saved(&mut self, snapshot_id: &SnapshotId, revision: DocumentRevision) {
        let node_index = self
            .nodes
            .iter()
            .enumerate()
            .take(self.cursor)
            .find(|(_, node)| {
                node.steps
                    .last()
                    .is_some_and(|step| step.after_id() == snapshot_id)
            })
            .map(|(index, _)| index);
        self.saved = Some(SavePoint {
            snapshot_id: snapshot_id.clone(),
            revision,
            node_index,
        });
    }

    pub(crate) fn is_at_save_point(&self, snapshot_id: &SnapshotId) -> bool {
        self.saved
            .as_ref()
            .is_some_and(|point| &point.snapshot_id == snapshot_id)
    }

    pub(crate) fn saved_revision(&self) -> Option<DocumentRevision> {
        self.saved.as_ref().map(|point| point.revision)
    }

    pub(crate) fn trim_to(&mut self, max_nodes: usize) -> TrimOutcome {
        let protected = self.protected_prefix(self.cursor);
        let desired = self.nodes.len().saturating_sub(max_nodes);
        let remove = desired.min(protected);
        if remove > 0 {
            self.nodes.drain(0..remove);
            self.cursor = self.cursor.saturating_sub(remove);
            if let Some(point) = &mut self.saved
                && let Some(index) = &mut point.node_index
            {
                *index = index.saturating_sub(remove);
            }
        }
        TrimOutcome {
            removed: remove,
            retained: self.nodes.len(),
        }
    }

    pub(crate) fn step_handle(&self, index: usize) -> Option<HistoryStepHandle> {
        let node = self.nodes.get(index)?;
        let last = node.steps.last()?;
        Some(HistoryStepHandle(last.clone()))
    }

    pub(crate) fn len(&self) -> usize {
        self.nodes.len()
    }

    pub(crate) fn cursor(&self) -> usize {
        self.cursor
    }

    pub(crate) fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    pub(crate) fn can_redo(&self) -> bool {
        self.cursor < self.nodes.len()
    }
}

/// 汇总节点内各步补丁区间；撤销取逆补丁，重做取正补丁。
fn collect_patch_ranges(steps: &[Arc<HistoryStep>], inverse: bool) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for step in steps {
        let patches = if inverse {
            step.inverse_patches()
        } else {
            step.forward_patches()
        };
        ranges.extend(patches.iter().map(|(range, _)| range.clone()));
    }
    ranges
}

fn sealed_node(step: Arc<HistoryStep>, now_millis: u64) -> Result<HistoryNode, BufferError> {
    let mut steps = Vec::new();
    steps
        .try_reserve_exact(1)
        .map_err(BufferError::Allocation)?;
    let source = step.source();
    steps.push(step);
    Ok(HistoryNode {
        source,
        steps,
        pending: false,
        last_input_at: now_millis,
    })
}

fn can_merge(previous: &HistoryStep, current: &HistoryStep) -> bool {
    if previous.after_selection != current.before_selection {
        return false;
    }
    let collapsed = previous.before_selection.anchor == previous.before_selection.head
        && previous.after_selection.anchor == previous.after_selection.head
        && current.before_selection.anchor == current.before_selection.head
        && current.after_selection.anchor == current.after_selection.head;
    if !collapsed {
        return false;
    }
    let Some((previous_range, previous_text)) = previous.forward.first() else {
        return false;
    };
    let Some((current_range, current_text)) = current.forward.first() else {
        return false;
    };
    if previous.forward.len() != 1 || current.forward.len() != 1 {
        return false;
    }
    if previous_range.is_empty() && current_range.is_empty() {
        return previous_range
            .start
            .checked_add(previous_text.len())
            .is_some_and(|end| end == current_range.start);
    }
    if previous_text.is_empty() && current_text.is_empty() && !previous_range.is_empty() {
        return current_range.end == previous_range.start;
    }
    false
}

fn step_bytes(step: &HistoryStep) -> Result<usize, BufferError> {
    let mut total = 0usize;
    for (_, text) in step.forward.iter().chain(step.inverse.iter()) {
        total = total
            .checked_add(text.len())
            .ok_or(BufferError::LengthOverflow)?;
    }
    Ok(total)
}
