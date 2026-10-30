#![forbid(unsafe_code)]

fn main() {
    let events = aline_markdown_probe::count_events("# 探针\n\n正文\n");
    println!("pulldown-cmark events={events}");
}
