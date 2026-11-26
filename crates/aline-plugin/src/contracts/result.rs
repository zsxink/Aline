//! 输出意图与类型化诊断；用户可见文案由 UI 经 `t("key")` 本地化。

use super::RequestId;

/// 类型化诊断类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiagnosticKind {
    /// 能力不支持该输入或图类型。
    Unsupported,
    /// 输入不合法。
    InvalidInput,
    /// 处理失败；保留源码与错误说明。
    Failed,
    /// 超出已冻结预算。
    OverBudget,
}

/// 提供者返回的诊断。
///
/// 只携带类别；所属能力由派发方按请求填写到 [`DispatchError::Provider`]，
/// 避免提供者声明与请求不一致。
///
/// [`DispatchError::Provider`]: crate::DispatchError::Provider
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProviderDiagnostic {
    /// 诊断类别。
    pub kind: DiagnosticKind,
}

impl ProviderDiagnostic {
    /// 构造诊断。
    pub const fn new(kind: DiagnosticKind) -> Self {
        Self { kind }
    }
}

/// 插件向宿主返回的语义意图。
///
/// 宿主负责校验、采纳、展示与落盘；插件不能直接修改文档或目标文件。
/// 命令意图与导出产物由各自的能力任务在冻结契约后扩展。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum OutputIntent {
    /// 宿主可展示的派生文本。
    Text(String),
    /// 宿主应校验后发布的二进制资产。
    Asset {
        /// 媒体类型，例如 `image/svg+xml`。
        media_type: String,
        /// 资产字节。
        bytes: Vec<u8>,
    },
}

/// 一次派发的可采纳结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DispatchOutcome {
    /// 对应请求身份。
    pub request_id: RequestId,
    /// 派发时的插件代次；宿主据此拒绝过期结果。
    pub generation: u64,
    /// 提供者返回的意图；按顺序交给宿主处理。
    pub intents: Vec<OutputIntent>,
}
