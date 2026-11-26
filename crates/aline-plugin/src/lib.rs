//! 插件公共系统：身份/来源/能力声明、注册、启停与派发。
//!
//! 本 crate 只固定进程内公共语义：注册表按能力身份路由到唯一提供者，
//! 生命周期由宿主发布的有效启用集合驱动，提供者返回语义意图或类型化
//! 诊断。这里不实现六个内置插件，不包含 GPUI 实体或可变 Buffer，也不
//! 承诺未来外部插件的加载协议；内置实现与具体能力契约由后续任务接入。
//!
//! 解析扩展契约归 `aline-markdown` 的纯数据类型；本 crate 只聚合启用
//! 能力声明的附加语法位，宿主负责并入核心语法集合并形成解析配置。

mod contracts;
mod dispatch;
mod lifecycle;
mod registry;
mod traits;

pub use contracts::{
    CapabilityDeclaration, CapabilityId, DiagnosticKind, DispatchOutcome, HostRequest,
    OutputIntent, ProviderDescriptor, ProviderDiagnostic, ProviderId, ProviderSource, RequestId,
    RequestInput,
};
pub use dispatch::{DispatchError, dispatch};
pub use lifecycle::{EnabledCapabilities, Lifecycle};
pub use registry::{CapabilityQuery, RegistrationError, RegistrationErrorKind, Registry};
pub use traits::Provider;
