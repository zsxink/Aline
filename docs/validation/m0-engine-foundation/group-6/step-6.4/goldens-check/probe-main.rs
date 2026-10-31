//! 组 6.4 手工金样的外部核对：只打印实际片段/锚点，不写入仓库。
use aline_buffer::{Affinity, TextBudget, TextSnapshot};
use aline_markdown::{ParseBudget, ParseConfig, parse};

fn main() {
    let cases: [(&str, &str); 6] = [
        ("entities", "a &amp; b\n"),
        ("multichar_entities", "&NotEqualTilde; &#x1F600;\n"),
        ("soft_break", "a\nb\n"),
        ("hard_break", "a  \nb\n"),
        ("br_newline", "a<br>b\n"),
        ("hidden_synthetic", "**x** y<br>z\n"),
    ];
    for (name, source) in cases {
        let snapshot = match TextSnapshot::new(source.to_string(), TextBudget::new(1 << 20)) {
            Ok(s) => s,
            Err(e) => { println!("{name}\tERROR\t{e:?}"); continue; }
        };
        let parsed = match parse(&snapshot, ParseConfig::m0(), ParseBudget::m0()) {
            Ok(p) => p,
            Err(e) => { println!("{name}\tERROR\t{e:?}"); continue; }
        };
        let projection = parsed.projection();
        println!("{name}\tdrawn\t{:?}", projection.drawn_text());
        for segment in projection.segments() {
            println!(
                "{name}\tsegment\t{}..{}\t{}..{}\t{:?}",
                segment.drawn_range().start,
                segment.drawn_range().end,
                segment.source_range().byte_range().start,
                segment.source_range().byte_range().end,
                segment.precision()
            );
        }
        let offsets: Vec<usize> = projection
            .segments()
            .iter()
            .flat_map(|s| [s.drawn_range().start, s.drawn_range().end])
            .collect();
        let mut seen = Vec::new();
        for offset in offsets {
            if seen.contains(&offset) { continue; }
            seen.push(offset);
            let before = projection.source_at(&projection.drawn_offset(offset).unwrap_or_else(|_| unreachable!()), Affinity::Before);
            let after = projection.source_at(&projection.drawn_offset(offset).unwrap_or_else(|_| unreachable!()), Affinity::After);
            if let (Ok(b), Ok(a)) = (before, after) {
                println!(
                    "{name}\tboundary\t{offset}\tBefore={}\tAfter={}",
                    b.value().byte_offset(),
                    a.value().byte_offset()
                );
            }
        }
    }
}
