//! 隔离违规样例。
/// 故意触发截断转换 lint。
pub fn forbidden(value: u64) -> u8 { value as u8 }
