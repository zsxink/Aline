//! 隔离违规样例。
/// 故意触发 unreachable lint。
pub fn forbidden() -> u8 { unreachable!() }
