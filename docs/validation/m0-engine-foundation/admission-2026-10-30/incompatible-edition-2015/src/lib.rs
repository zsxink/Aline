//! 最小反例：旧 edition 不能编译 async 函数。
#![forbid(unsafe_code)]

pub async fn candidate() {}
