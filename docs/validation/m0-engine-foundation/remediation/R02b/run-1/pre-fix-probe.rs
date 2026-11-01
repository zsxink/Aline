//! R02b 修复前探针（取证后删除）：第二层空来源片锚定到第一层 Aligned/Hidden 时，
//! 复合结果不应获得非空 source。
use std::error::Error;
use std::ops::Range;

use aline_markdown::{MappingPrecision, ParseBudget, Rewrite, RewriteEdit, RewriteMap};

fn dump(label: &str, map: &RewriteMap) {
    let rows: Vec<(Range<usize>, Range<usize>, MappingPrecision)> = map
        .segments()
        .iter()
        .map(|s| (s.source_range(), s.rewritten_range(), s.precision()))
        .collect();
    println!("{label}: {rows:?}");
}

#[test]
fn probe_synthetic_anchors_are_empty() -> Result<(), Box<dyn Error>> {
    // 第一层：源码 "aXYdef"，把 1..3 换成 "XY"（内容相同，声明 Exact 仍判 Aligned）。
    let first = Rewrite::apply(
        "aXYdef",
        &[RewriteEdit::new(1..3, "xy".to_string(), MappingPrecision::Exact)?],
        ParseBudget::m0(),
    )?;
    println!("L1 text = {:?}", first.text());
    dump("L1", first.map());

    // 第二层 @1：在副本偏移 1 插入零来源合成 "!"。
    let at_start = Rewrite::apply(
        first.text(),
        &[RewriteEdit::new(1..1, "!".to_string(), MappingPrecision::Synthetic)?],
        ParseBudget::m0(),
    )?;
    dump("L2@1", at_start.map());
    let composed = first.map().compose(at_start.map())?;
    dump("COMPOSED@1", &composed);
    let synthetic: Vec<_> = composed
        .segments()
        .iter()
        .filter(|s| s.precision() == MappingPrecision::Synthetic)
        .map(|s| (s.source_range(), s.rewritten_range()))
        .collect();
    println!("SYNTHETIC@1 = {synthetic:?}");
    assert!(
        synthetic.iter().all(|(source, _)| source.is_empty()),
        "Synthetic 片段必须锚定空来源，实际 {synthetic:?}"
    );

    // 第二层 @2：在副本偏移 2 插入零来源合成 "!"（落在 Aligned 片内部而非起点）。
    let inside = Rewrite::apply(
        first.text(),
        &[RewriteEdit::new(2..2, "!".to_string(), MappingPrecision::Synthetic)?],
        ParseBudget::m0(),
    )?;
    dump("L2@2", inside.map());
    let composed = first.map().compose(inside.map())?;
    dump("COMPOSED@2", &composed);
    let synthetic: Vec<_> = composed
        .segments()
        .iter()
        .filter(|s| s.precision() == MappingPrecision::Synthetic)
        .map(|s| (s.source_range(), s.rewritten_range()))
        .collect();
    println!("SYNTHETIC@2 = {synthetic:?}");
    assert!(
        synthetic.iter().all(|(source, _)| source.is_empty()),
        "Synthetic 片段必须锚定空来源，实际 {synthetic:?}"
    );

    // 第一层隐藏片作为锚点：空来源合成片落在 Hidden 片结束点。
    let hidden_first = Rewrite::apply(
        "ab",
        &[RewriteEdit::new(1..2, String::new(), MappingPrecision::Hidden)?],
        ParseBudget::m0(),
    )?;
    dump("H1", hidden_first.map());
    let second = Rewrite::apply(
        hidden_first.text(),
        &[RewriteEdit::new(1..1, "!".to_string(), MappingPrecision::Synthetic)?],
        ParseBudget::m0(),
    )?;
    dump("H2@1", second.map());
    let composed = hidden_first.map().compose(second.map())?;
    dump("HCOMPOSED@1", &composed);
    let synthetic: Vec<_> = composed
        .segments()
        .iter()
        .filter(|s| s.precision() == MappingPrecision::Synthetic)
        .map(|s| (s.source_range(), s.rewritten_range()))
        .collect();
    println!("HSYNTHETIC@1 = {synthetic:?}");
    assert!(
        synthetic.iter().all(|(source, _)| source.is_empty()),
        "锚到 Hidden 的 Synthetic 也必须空来源，实际 {synthetic:?}"
    );
    Ok(())
}
