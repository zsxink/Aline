//! 唯一打开块及其草稿槽。
//!
//! 打开块记录基线 revision、源码范围、基线片段、源码选区、唯一草稿和
//! 输入组状态。草稿只能是「普通输入组」或「组合候选」二者之一，不能同时
//! 存在，也不构成第二份可独立提交的文档。本模块不持有窗口、任务或文件句柄。

use std::ops::Range;

use aline_buffer::{DocumentRevision, SelectionSpec};

use crate::document::composition::CompositionState;

/// 普通输入组：在空白源码范围上累积的单区间替换。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypingGroup {
    pub(crate) range: Range<usize>,
    pub(crate) draft: String,
    pub(crate) before_selection: SelectionSpec,
    pub(crate) last_input_at: u64,
}

/// 打开块的唯一草稿槽。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum DraftSlot {
    /// 没有草稿；当前文本等于已提交 Buffer。
    Empty,
    /// 普通输入组尚未提交。
    Typing(TypingGroup),
    /// 输入法组合候选尚未提交。
    Composition(CompositionState),
}

/// 唯一打开块。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenBlock {
    pub(crate) baseline_revision: DocumentRevision,
    pub(crate) range: Range<usize>,
    pub(crate) baseline_fragment: String,
    pub(crate) selection: SelectionSpec,
    pub(crate) draft: DraftSlot,
    /// 最近一次被撤销的普通输入组；只允许按顺序重做。
    pub(crate) undone_typing: Option<TypingGroup>,
}

impl OpenBlock {
    /// 返回当前草稿文本（普通输入组或组合候选）。
    pub(crate) fn current_draft(&self) -> Option<&str> {
        match &self.draft {
            DraftSlot::Empty => None,
            DraftSlot::Typing(group) => Some(&group.draft),
            DraftSlot::Composition(state) => Some(&state.candidate),
        }
    }

    /// 返回普通输入组。
    pub(crate) fn typing(&self) -> Option<&TypingGroup> {
        match &self.draft {
            DraftSlot::Typing(group) => Some(group),
            DraftSlot::Empty | DraftSlot::Composition(_) => None,
        }
    }

    /// 返回可变的普通输入组。
    pub(crate) fn typing_mut(&mut self) -> Option<&mut TypingGroup> {
        match &mut self.draft {
            DraftSlot::Typing(group) => Some(group),
            DraftSlot::Empty | DraftSlot::Composition(_) => None,
        }
    }

    /// 返回组合状态。
    pub(crate) fn composition(&self) -> Option<&CompositionState> {
        match &self.draft {
            DraftSlot::Composition(state) => Some(state),
            DraftSlot::Empty | DraftSlot::Typing(_) => None,
        }
    }

    /// 返回可变的组合状态。
    pub(crate) fn composition_mut(&mut self) -> Option<&mut CompositionState> {
        match &mut self.draft {
            DraftSlot::Composition(state) => Some(state),
            DraftSlot::Empty | DraftSlot::Typing(_) => None,
        }
    }
}
