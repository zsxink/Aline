//! 两种编辑视图共用 DocCore 文本和历史；这里只验证切换的纯状态协议。

/// 宿主展示模式；具体 UI 和映射消费留后续阶段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EditorMode {
    /// 所见即所得视图。
    Visual,
    /// 原始源码视图。
    Source,
}

/// 模式请求在组合期延后；宿主在平台自然结束后重试同一请求。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModeSwitchOutcome {
    /// 组合仍活动，没有改变模式或输入状态。
    Deferred,
    /// 已切换，或目标本就是当前模式。
    Ready,
}
