//! 只读导航索引；M0 只保存来源/目标角色，不保存渲染副本或 UI 状态。

use aline_buffer::SrcRange;

/// 导航条目种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NavigationKind {
    /// 顶层标题。
    Heading,
    /// 顶层 `[toc]` 目录段落。
    Toc,
    /// 解析器发出的脚注引用事件。
    FootnoteReference,
    /// 顶层脚注定义。
    FootnoteDefinition,
}

/// 单个导航条目；字段私有且不提供公开构造器。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigationEntry {
    kind: NavigationKind,
    source: SrcRange,
    targets: Vec<SrcRange>,
    heading_level: Option<u8>,
}

impl NavigationEntry {
    pub(crate) fn new(
        kind: NavigationKind,
        source: SrcRange,
        targets: Vec<SrcRange>,
        heading_level: Option<u8>,
    ) -> Self {
        Self {
            kind,
            source,
            targets,
            heading_level,
        }
    }

    /// 返回条目种类。
    pub const fn kind(&self) -> NavigationKind {
        self.kind
    }

    /// 返回条目来源范围。
    pub fn source(&self) -> &SrcRange {
        &self.source
    }

    /// 返回跳转目标；标题条目为空，其余按源码顺序。
    pub fn targets(&self) -> &[SrcRange] {
        &self.targets
    }

    /// 返回标题级别；仅有 `Heading` 为 `Some(1..=6)`。
    pub const fn heading_level(&self) -> Option<u8> {
        self.heading_level
    }
}

/// 一次解析的只读导航索引；所有范围绑定同一快照。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NavigationIndex {
    entries: Vec<NavigationEntry>,
}

impl NavigationIndex {
    pub(crate) fn new(entries: Vec<NavigationEntry>) -> Self {
        Self { entries }
    }

    /// 返回条目，按源码起点、终点稳定排序。
    pub fn entries(&self) -> &[NavigationEntry] {
        &self.entries
    }

    /// 返回条目数。
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// 判断索引是否为空。
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
