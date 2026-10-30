#![forbid(unsafe_code)]

// 审计探针：验证纯标准库 crate 可消费。
pub fn probe() -> bool { true }
