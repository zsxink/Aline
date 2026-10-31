//! 组 6 的解析预算；数值是可注入的保护上限，不是发布性能限额。

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// 解析、重写、映射、树构建与有限 HTML 扫描的上限。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseBudget {
    max_source_bytes: usize,
    max_rewritten_bytes: usize,
    max_segments: usize,
    max_scan_steps: usize,
    max_nesting: usize,
    max_nodes: usize,
    max_html_depth: usize,
    max_mapping_points: usize,
}

impl ParseBudget {
    /// 当前 M0 测试与无头调用使用的保守上限；不冻结为产品阈值。
    pub const fn m0() -> Self {
        Self {
            max_source_bytes: 16 * 1024 * 1024,
            max_rewritten_bytes: 32 * 1024 * 1024,
            max_segments: 1_000_000,
            max_scan_steps: 4_000_000,
            max_nesting: 256,
            max_nodes: 1_000_000,
            max_html_depth: 256,
            max_mapping_points: 4_000_000,
        }
    }

    /// 返回源码字节上限。
    pub const fn max_source_bytes(self) -> usize {
        self.max_source_bytes
    }
    /// 返回重写文本字节上限。
    pub const fn max_rewritten_bytes(self) -> usize {
        self.max_rewritten_bytes
    }
    /// 返回映射片段上限（构树阶段的片段计数）。
    pub const fn max_segments(self) -> usize {
        self.max_segments
    }
    /// 返回事件/扫描步数上限。
    pub const fn max_scan_steps(self) -> usize {
        self.max_scan_steps
    }
    /// 返回块/内联嵌套上限。
    pub const fn max_nesting(self) -> usize {
        self.max_nesting
    }
    /// 返回节点数上限。
    pub const fn max_nodes(self) -> usize {
        self.max_nodes
    }
    /// 返回有限 HTML 标签栈深度上限。
    pub const fn max_html_depth(self) -> usize {
        self.max_html_depth
    }
    /// 返回投影索引点上限。
    ///
    /// M0 中该值同时作为投影片段数上限：`parse` 构造 `MappingBudget` 时把
    /// [`ParseBudget::max_mapping_points`] 传给 `MappingBudget::new` 的索引点参数，
    /// 而该参数也限制片段数。因此片段数超限同样报告
    /// [`crate::ParseError::Mapping`] 的 `BudgetExceeded`，即使索引点本身未超限。
    pub const fn max_mapping_points(self) -> usize {
        self.max_mapping_points
    }

    /// 注入源码字节上限。
    pub const fn with_max_source_bytes(mut self, value: usize) -> Self {
        self.max_source_bytes = value;
        self
    }
    /// 注入重写文本字节上限。
    pub const fn with_max_rewritten_bytes(mut self, value: usize) -> Self {
        self.max_rewritten_bytes = value;
        self
    }
    /// 注入映射片段上限（构树阶段的片段计数）。
    pub const fn with_max_segments(mut self, value: usize) -> Self {
        self.max_segments = value;
        self
    }
    /// 注入扫描步数上限。
    pub const fn with_max_scan_steps(mut self, value: usize) -> Self {
        self.max_scan_steps = value;
        self
    }
    /// 注入嵌套上限。
    pub const fn with_max_nesting(mut self, value: usize) -> Self {
        self.max_nesting = value;
        self
    }
    /// 注入节点数上限。
    pub const fn with_max_nodes(mut self, value: usize) -> Self {
        self.max_nodes = value;
        self
    }
    /// 注入 HTML 深度上限。
    pub const fn with_max_html_depth(mut self, value: usize) -> Self {
        self.max_html_depth = value;
        self
    }
    /// 注入投影索引点上限；该值同时限制投影片段数，见 [`ParseBudget::max_mapping_points`]。
    pub const fn with_max_mapping_points(mut self, value: usize) -> Self {
        self.max_mapping_points = value;
        self
    }
}

/// 协作式取消检查；实现方只需给出低成本的原子标志读取。
///
/// 解析只在重写、扫描与构树循环之间轮询该接口，不会中断第三方解析器内部调用。
pub trait CancelCheck {
    /// 返回调用方是否已经请求取消。
    fn is_cancelled(&self) -> bool;
}

/// 可克隆的取消令牌；任一克隆调用 [`CancelToken::cancel`] 后所有克隆都观察到取消。
#[derive(Clone, Debug, Default)]
pub struct CancelToken {
    cancelled: Arc<AtomicBool>,
}

impl CancelToken {
    /// 创建未取消的令牌。
    pub fn new() -> Self {
        Self::default()
    }

    /// 请求取消；后续检查返回 `true`。
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// 返回当前是否已请求取消。
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

impl CancelCheck for CancelToken {
    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// 永不取消的检查器；供不带取消参数的兼容入口使用。
pub(crate) struct NeverCancel;

impl CancelCheck for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

/// 一次成功解析的只读统计；数值只用于验证注入预算，不是发布性能限额。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ParseStats {
    pub(crate) scan_steps: usize,
    pub(crate) mapping_points: usize,
    pub(crate) budget_hits: usize,
}

impl ParseStats {
    /// 返回本次解析统计到的 HTML 扫描迭代步数。
    ///
    /// 计数与有限 HTML 扫描器使用同一过滤集合与闭标签规则；预算回退时仍包含
    /// 触发超限的那段原始 HTML 的完整计数，因此可以跨次比较阈值。
    pub const fn scan_steps(self) -> usize {
        self.scan_steps
    }

    /// 返回本次成功投影实际建立的索引点数（绘制字符数加一）。
    pub const fn mapping_points(self) -> usize {
        self.mapping_points
    }

    /// 返回本次解析触发预算回退的次数。
    pub const fn budget_hits(self) -> usize {
        self.budget_hits
    }
}
