//! 允许预期测试断言和编译期只读常量的正例。
/// 编译期只读实体名。
pub const ENTITY_NAMES: [&str; 1] = ["amp"];
#[cfg(test)] mod tests { #[test] fn assertion_is_test_expectation() { let left = 1; let right = 1; assert_eq!(left, right); } }
