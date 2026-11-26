//! 带版本的宿主请求；只含只读数据，不含 GPUI 实体或可变 Buffer。

use super::CapabilityId;

/// 宿主分配的请求身份。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId(u64);

impl RequestId {
    /// 由宿主给定编号构造。
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 返回编号。
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 请求输入。
///
/// 后续能力任务在需要更丰富的只读上下文时扩展此枚举；派发通道和代次
/// 校验不因此改变。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RequestInput {
    /// 无输入（查询或控制类请求）。
    None,
    /// 只读文本片段。
    Text(String),
}

/// 带版本的宿主请求。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostRequest {
    /// 宿主分配的请求身份，用于采纳与诊断。
    pub request_id: RequestId,
    /// 目标能力。
    pub capability: CapabilityId,
    /// 发起请求时的插件代次；派发时与当前代次不一致即拒绝。
    pub generation: u64,
    /// 只读输入。
    pub input: RequestInput,
}

impl HostRequest {
    /// 构造请求。
    pub fn new(
        request_id: RequestId,
        capability: CapabilityId,
        generation: u64,
        input: RequestInput,
    ) -> Self {
        Self {
            request_id,
            capability,
            generation,
            input,
        }
    }
}
