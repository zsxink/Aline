//! 调用方意图、编辑模式与分组原因；这里不执行任何修改。

use std::ops::Range;

/// 命令来源模式；决定保护/HTML 边界和隐藏语义的检查强度。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditMode {
    /// 所见编辑：保留隐藏源码语义，受保护区和 HTML 边界不可修改。
    Visual,
    /// 源码编辑：允许直接修改受保护区和 HTML 原文，但仍须重读通过。
    Source,
}

impl EditMode {
    /// 返回稳定名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Visual => "Visual",
            Self::Source => "Source",
        }
    }
}

/// 命令进入编辑引擎前的输入上下文；预检只读取，不结束分组或清空组合。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditContext {
    mode: EditMode,
    pending_input: bool,
    composition_active: bool,
}

impl EditContext {
    /// 创建上下文；默认没有待定输入或组合输入。
    pub const fn new(mode: EditMode) -> Self {
        Self {
            mode,
            pending_input: false,
            composition_active: false,
        }
    }

    /// 标记是否存在尚未结束的输入组。
    pub const fn with_pending_input(mut self, value: bool) -> Self {
        self.pending_input = value;
        self
    }

    /// 标记输入法组合是否仍在进行。
    pub const fn with_composition_active(mut self, value: bool) -> Self {
        self.composition_active = value;
        self
    }

    /// 返回来源模式。
    pub const fn mode(self) -> EditMode {
        self.mode
    }

    /// 判断是否存在待定输入组。
    pub const fn has_pending_input(self) -> bool {
        self.pending_input
    }

    /// 判断输入法组合是否仍在进行。
    pub const fn has_composition(self) -> bool {
        self.composition_active
    }
}

/// 可切换的最小行内格式集合；标记均为 ASCII，便于按字节手算。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlineFormat {
    /// 强强调，标记 `**`。
    Strong,
    /// 删除线，标记 `~~`。
    Strike,
}

impl InlineFormat {
    /// 返回插入源码的标记。
    pub const fn marker(self) -> &'static str {
        match self {
            Self::Strong => "**",
            Self::Strike => "~~",
        }
    }

    /// 返回稳定名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strong => "Strong",
            Self::Strike => "Strike",
        }
    }
}

/// 计划所属的单一操作分组；一个计划只对应一个历史节点。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GroupReason {
    /// 单条源码替换。
    SourceReplace,
    /// 单条所见替换。
    VisibleReplace,
    /// 多条源码补丁组成的批量替换。
    BulkReplace,
    /// 行内格式切换。
    InlineFormat(InlineFormat),
}

impl GroupReason {
    /// 返回稳定名称。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SourceReplace => "SourceReplace",
            Self::VisibleReplace => "VisibleReplace",
            Self::BulkReplace => "BulkReplace",
            Self::InlineFormat(InlineFormat::Strong) => "InlineFormatStrong",
            Self::InlineFormat(InlineFormat::Strike) => "InlineFormatStrike",
        }
    }
}

/// 一条源码替换意图；范围在计划入口按目标快照受检转换。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceEdit {
    range: Range<usize>,
    expected: String,
    replacement: String,
}

impl SourceEdit {
    /// 创建源码替换意图；范围尚未校验，由 `plan_change` 绑定快照。
    pub fn new(
        range: Range<usize>,
        expected: impl Into<String>,
        replacement: impl Into<String>,
    ) -> Self {
        Self {
            range,
            expected: expected.into(),
            replacement: replacement.into(),
        }
    }

    /// 返回待替换的源码字节范围。
    pub fn range(&self) -> &Range<usize> {
        &self.range
    }

    /// 返回调用方期望在范围内读到的原文。
    pub fn expected(&self) -> &str {
        &self.expected
    }

    /// 返回替换文本。
    pub fn replacement(&self) -> &str {
        &self.replacement
    }
}

/// 进入纯计划边界的编辑意图；模式与待定状态由 `EditContext` 单独给出。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditCommand {
    /// 一批源码或所见替换；空批次与同内容替换视为无变化。
    Replace {
        /// 替换意图，允许乱序，计划阶段按源码起点稳定排序。
        edits: Vec<SourceEdit>,
    },
    /// 在所见选区内切换行内格式。
    ToggleInlineFormat {
        /// 选区覆盖的源码字节范围。
        range: Range<usize>,
        /// 目标格式。
        format: InlineFormat,
    },
}

impl EditCommand {
    /// 创建替换命令。
    pub fn replace(edits: Vec<SourceEdit>) -> Self {
        Self::Replace { edits }
    }

    /// 创建行内格式切换命令。
    pub fn toggle_inline_format(range: Range<usize>, format: InlineFormat) -> Self {
        Self::ToggleInlineFormat { range, format }
    }
}
