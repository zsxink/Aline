#![forbid(unsafe_code)]

// 审计探针：实际消费 pulldown-cmark 事件，不生成 HTML。
pub fn count_events(source: &str) -> usize {
    pulldown_cmark::Parser::new(source).count()
}
