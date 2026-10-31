//! 隔离违规样例。
/// 故意触发 unwrap lint。
pub fn forbidden(value: Option<u8>) -> u8 { value.unwrap() }
