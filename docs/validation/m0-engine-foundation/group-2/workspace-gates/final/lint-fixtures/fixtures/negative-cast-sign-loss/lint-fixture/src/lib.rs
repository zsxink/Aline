//! 隔离违规样例。
/// 故意触发符号损失 lint。
pub fn forbidden(value: i32) -> usize { value as usize }
