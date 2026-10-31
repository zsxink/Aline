//! 正常公共编码消费者。
use aline_source::{SourceBaseline, StorageBudget, encode};
fn check(baseline: &SourceBaseline) {
    let _result = encode(baseline.source(), baseline, StorageBudget::new(1024));
}
fn main() {}
