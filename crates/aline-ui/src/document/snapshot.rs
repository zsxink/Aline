//! 当前只读全文快照：已提交 Buffer 加上唯一草稿或组合候选。
//!
//! 快照只为宿主提供一致的只读视图；取快照不提交草稿、不封口输入组、不压缩
//! 历史、不清 redo，也不写保存点。草稿存在时用 `TextSnapshot::with_overlay`
//! 形成覆盖视图，快照身份随工作版本缓存，保证同一工作版本重复读取得到同一
//! `SnapshotId`。

use aline_buffer::{BufferError, SnapshotId, SrcSelection, TextBudget, TextSnapshot};

use crate::document::identity::{DocumentId, WorkingVersion};

/// 一致的只读全文快照与它的工作版本。
#[derive(Clone, Debug)]
pub struct CurrentSnapshot {
    document: DocumentId,
    version: WorkingVersion,
    text: TextSnapshot,
    selection: SrcSelection,
}

impl CurrentSnapshot {
    /// 构造当前快照；仅由 `DocCore` 在完成基线校验后调用。
    pub(crate) fn new(
        document: DocumentId,
        version: WorkingVersion,
        text: TextSnapshot,
        selection: SrcSelection,
    ) -> Self {
        Self {
            document,
            version,
            text,
            selection,
        }
    }

    /// 返回快照所属的文档身份。
    pub fn document_id(&self) -> &DocumentId {
        &self.document
    }

    /// 返回快照对应的工作版本。
    pub fn version(&self) -> WorkingVersion {
        self.version
    }

    /// 返回快照底层文本视图；覆盖视图的 id 与 `snapshot_id` 相同。
    pub fn text(&self) -> &TextSnapshot {
        &self.text
    }

    /// 返回与快照绑定的源码选区。
    pub fn selection(&self) -> &SrcSelection {
        &self.selection
    }

    /// 返回快照身份。
    pub fn snapshot_id(&self) -> &SnapshotId {
        self.text.id()
    }

    /// 返回快照全文长度（UTF-8 字节）。
    pub fn len_bytes(&self) -> usize {
        self.text.len_bytes()
    }

    /// 按给定文本预算物化快照全文。
    pub fn full_text(&self, budget: TextBudget) -> Result<String, BufferError> {
        let range = self.text.src_range(0..self.text.len_bytes())?;
        Ok(self.text.slice(&range, budget)?.into_owned())
    }
}

/// 按工作版本缓存当前视图，保证同一工作版本重复读取保留同一 `SnapshotId`。
#[derive(Clone, Debug)]
pub(crate) struct SnapshotCache {
    pub(crate) version: WorkingVersion,
    pub(crate) snapshot: CurrentSnapshot,
}
