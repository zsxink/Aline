//! 能力路由；不写死六个内置实现。

use std::fmt;

use crate::contracts::{CapabilityId, DispatchOutcome, HostRequest, ProviderDiagnostic, RequestId};
use crate::lifecycle::Lifecycle;
use crate::registry::Registry;

/// 派发失败。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DispatchError {
    /// 请求发起后插件代次已推进；旧请求不能复活。
    StaleGeneration {
        /// 请求身份。
        request: RequestId,
        /// 请求携带的代次。
        expected: u64,
        /// 当前代次。
        current: u64,
    },
    /// 没有提供者声明该能力。
    UnknownCapability {
        /// 请求身份。
        request: RequestId,
        /// 目标能力。
        capability: CapabilityId,
    },
    /// 能力已注册但当前未启用。
    Disabled {
        /// 请求身份。
        request: RequestId,
        /// 目标能力。
        capability: CapabilityId,
    },
    /// 提供者返回诊断；不产生输出意图。
    Provider {
        /// 请求身份。
        request: RequestId,
        /// 目标能力；按请求填写，不依赖提供者声明。
        capability: CapabilityId,
        /// 提供者诊断。
        diagnostic: ProviderDiagnostic,
    },
}

impl fmt::Display for DispatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for DispatchError {}

/// 按能力路由请求。
///
/// 校验顺序固定为：代次、能力是否存在、是否启用，最后调用提供者。
/// 只有全部通过才返回带当前代次的输出意图。
pub fn dispatch(
    registry: &Registry,
    lifecycle: &Lifecycle,
    request: &HostRequest,
) -> Result<DispatchOutcome, DispatchError> {
    if request.generation != lifecycle.generation() {
        return Err(DispatchError::StaleGeneration {
            request: request.request_id,
            expected: request.generation,
            current: lifecycle.generation(),
        });
    }
    let query =
        registry
            .capability(request.capability)
            .ok_or(DispatchError::UnknownCapability {
                request: request.request_id,
                capability: request.capability,
            })?;
    if !lifecycle.enabled().contains(request.capability) {
        return Err(DispatchError::Disabled {
            request: request.request_id,
            capability: request.capability,
        });
    }
    let intents =
        query
            .provider()
            .dispatch(request)
            .map_err(|diagnostic| DispatchError::Provider {
                request: request.request_id,
                capability: request.capability,
                diagnostic,
            })?;
    Ok(DispatchOutcome {
        request_id: request.request_id,
        generation: lifecycle.generation(),
        intents,
    })
}
