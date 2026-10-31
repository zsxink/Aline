//! 隔离违规样例。
#[test] fn forbidden() { let value = Some(1_u8); let _copy = value.unwrap(); }
