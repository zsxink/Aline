//! 进程内提供者适配接口；不是外部 ABI。

use crate::contracts::{HostRequest, OutputIntent, ProviderDescriptor, ProviderDiagnostic};

/// 进程内提供者。
///
/// 注册表按能力路由到唯一提供者；提供者只返回语义意图或诊断，不持有
/// 可变 Buffer、文件写队列或完整窗口状态。
///
/// 提供者在宿主线程内被调用；需要后台计算时由宿主另行调度纯数据任务。
/// 本契约不要求实现 `Send`/`Sync`，也不允许把注册表中的实例移出宿主线程。
pub trait Provider {
    /// 注册时读取一次的声明；注册表保存校验后的副本。
    fn descriptor(&self) -> &ProviderDescriptor;

    /// 处理已通过代次与门控校验的请求。
    fn dispatch(&self, request: &HostRequest) -> Result<Vec<OutputIntent>, ProviderDiagnostic>;
}
