//! 唯一打开块及其草稿槽。
//!
//! 打开块记录基线 revision、源码范围、基线片段、源码选区、唯一草稿和
//! 输入组状态。草稿只能是「普通输入组」或「组合候选」二者之一，不能同时
//! 存在，也不构成第二份可独立提交的文档。本模块不持有窗口、任务或文件句柄。

use std::ops::Range;

use aline_buffer::{DocumentRevision, SelectionSpec};
use aline_markdown::ProtectionKind;

use crate::document::composition::CompositionState;

/// 普通输入组：在一个源码范围上累积的单区间替换。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypingGroup {
    pub(crate) range: Range<usize>,
    pub(crate) draft: String,
    /// 组开始时的逻辑选区；撤销恢复该值。
    pub(crate) before_selection: SelectionSpec,
    /// 撤销时刻的逻辑选区；重做恢复该值（可反向、可非折叠）。
    pub(crate) after_selection: SelectionSpec,
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

/// 文档当前的输入目标。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InputTarget {
    /// M0 无头入口，保留原有生命周期和语义。
    Legacy,
    /// 已检验的可视化块边界，排除尾部分隔换行，独立于草稿补丁范围。
    Visual(VisualInput),
    /// 源码模式的唯一输入组槽；不表示打开了一个可视化块。
    Source,
}

/// 与打开块基线共享源码坐标的范围和保护约束。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VisualInput {
    pub(crate) extent: Range<usize>,
    pub(crate) protections: Vec<VisualProtection>,
    /// 列表后的空行第一次输入时，与正文同一补丁写入的分隔符。
    pub(crate) leading_separator: &'static str,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VisualProtection {
    pub(crate) range: Range<usize>,
    pub(crate) kind: ProtectionKind,
}

/// 唯一打开块与输入槽。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct OpenBlock {
    pub(crate) baseline_revision: DocumentRevision,
    pub(crate) target: InputTarget,
    pub(crate) range: Range<usize>,
    pub(crate) baseline_fragment: String,
    pub(crate) selection: SelectionSpec,
    pub(crate) draft: DraftSlot,
}

impl OpenBlock {
    pub(super) fn leading_separator(&self) -> &'static str {
        self.visual_input()
            .map_or("", |input| input.leading_separator)
    }

    pub(crate) fn visual_extent(&self) -> Option<&Range<usize>> {
        self.visual_input().map(|target| &target.extent)
    }

    pub(crate) fn visual_input(&self) -> Option<&VisualInput> {
        match &self.target {
            InputTarget::Visual(target) => Some(target),
            InputTarget::Legacy | InputTarget::Source => None,
        }
    }

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
