//! 输入法组合候选的只读视图与内部状态。
//!
//! 组合候选占用 OpenBlock 的唯一草稿槽，只形成当前文本快照，不直接写入
//! Buffer、History 或磁盘。本模块不处理平台 IME 回调、窗口焦点或文字绘制。

use std::ops::Range;

use aline_buffer::SelectionSpec;

/// 当前组合的只读视图；候选文本仍由打开块独占。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionView<'a> {
    pub(crate) range: Range<usize>,
    pub(crate) candidate: &'a str,
}

impl<'a> CompositionView<'a> {
    /// 返回 Buffer 坐标中的组合原始范围（半开字节区间）。
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// 返回当前候选文本；它尚未写入已提交 Buffer。
    pub fn candidate(&self) -> &'a str {
        self.candidate
    }
}

/// 内部组合状态：原始范围、替换候选和组合前选区。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CompositionState {
    pub(crate) range: Range<usize>,
    pub(crate) candidate: String,
    pub(crate) before_selection: SelectionSpec,
    pub(crate) selection: SelectionSpec,
}
