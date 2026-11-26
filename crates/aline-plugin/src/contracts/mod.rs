//! 插件公共语义契约：身份、来源、能力声明、版本化请求与输出意图。
//!
//! 这些类型不含 GPUI 实体或可变 Buffer，也不承诺未来外部插件的线协议；
//! 注册与派发只按能力身份路由，不逐项匹配六个内置实现。

mod capability;
mod descriptor;
mod request;
mod result;

pub use capability::CapabilityId;
pub use descriptor::{CapabilityDeclaration, ProviderDescriptor, ProviderId, ProviderSource};
pub use request::{HostRequest, RequestId, RequestInput};
pub use result::{DiagnosticKind, DispatchOutcome, OutputIntent, ProviderDiagnostic};

/// 身份标识的公共约束：非空、不超过 64 字节、仅 ASCII 字母数字与 `-_.`。
pub(crate) fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
