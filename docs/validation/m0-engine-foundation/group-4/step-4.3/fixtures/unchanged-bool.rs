//! 无来源 unchanged bool 不属于公开编码契约。
use aline_source::{SourceBaseline, StorageBudget, encode};
fn check(baseline: &SourceBaseline) {
    let _result = encode("changed", baseline, true, StorageBudget::new(1024));
}
fn main() {}
