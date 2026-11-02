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
pub(crate) struct RecordPlan {
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

    pub(crate) fn prepare_record(
        &self,
        step: &HistoryStep,
        source: OperationSource,
        now_millis: u64,
    ) -> Result<RecordPlan, BufferError> {
        let truncate_to = self.cursor;
        let merge_node = if source == OperationSource::Typing && truncate_to > 0 {
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
        let existing_bytes = self.bytes_across(truncate_to)?;
        let added_bytes = step_bytes(step)?;
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

    pub(crate) fn commit_record(&mut self, plan: RecordPlan, step: HistoryStep, now_millis: u64) {
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
        let step = Arc::new(step);
        if let Some(merge_node) = plan.merge_node {
            let index = merge_node.saturating_sub(plan.remove_nodes);
            if let Some(node) = self.nodes.get_mut(index) {
                node.steps.push(step);
                node.pending = true;
                node.last_input_at = now_millis;
                return;
            }
        }
        let source = step.source();
        if let Some(previous) = self.nodes.get_mut(self.cursor.saturating_sub(1)) {
            previous.pending = false;
        }
        let last_input_at = now_millis;
        self.nodes.push(HistoryNode {
            source,
            steps: vec![step],
            pending: source == OperationSource::Typing,
            last_input_at,
        });
        self.cursor = self.cursor.saturating_add(1);
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
