//! 组 6.4 缺口探测：ZWJ、重复文字/空白、转义；只读打印。
use aline_buffer::{Affinity, TextBudget, TextSnapshot};
use aline_markdown::{ParseBudget, ParseConfig, parse};

fn main() {
    let cases: [(&str, &str); 6] = [
        ("zwj", "👨\u{200D}👩\u{200D}👧\n"),
        ("repeated_text", "**a** **a**\n"),
        ("double_space", "a  b\n"),
        ("escaped", "\\*x\\*\n"),
        ("tab_indent", "a\tb\n"),
        ("combining", "e\u{301}\n"),
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
        println!("{name}\tsource_bytes\t{}", source.len());
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
        for offset in 0..=projection.drawn_text().len() {
            if !projection.drawn_text().is_char_boundary(offset) { continue; }
            let before = projection.source_at(&projection.drawn_offset(offset).unwrap(), Affinity::Before);
            let after = projection.source_at(&projection.drawn_offset(offset).unwrap(), Affinity::After);
            println!("{name}\tanchor\t{offset}\t{:?}\t{:?}",
                before.map(|m| (m.value().byte_offset(), m.precision())),
                after.map(|m| (m.value().byte_offset(), m.precision())));
        }
    }
}
