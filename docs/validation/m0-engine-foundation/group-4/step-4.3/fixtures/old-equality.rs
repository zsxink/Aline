//! 旧 EncodedBytes 不能代替新 SourceBaseline 或相等证据。
use aline_source::{EncodedBytes, StorageBudget, encode};
fn check(old_equality: &EncodedBytes) {
    let _result = encode("changed", old_equality, StorageBudget::new(1024));
}
fn main() {}
