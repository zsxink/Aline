//! 图片目标定位：右键图片菜单按源码偏移找到最内层图片引用。
//!
//! 只沿展示树查找图片节点；偏移来自绘制文字命中或图片元素命中，不重新解析。

use std::error::Error;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    ParseBudget, ParseConfig, Parsed, image_destination_at, link_destination_at, parse,
};

fn parse_text(source: &str) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(1 << 20))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

#[test]
fn image_destination_is_returned_only_for_offsets_inside_the_alt_text() -> Result<(), Box<dyn Error>>
{
    let source = "before [docs](https://example.test/a) and ![替代](assets/pic.png) after\n";
    let parsed = parse_text(source)?;
    let alt = source.find("替代").ok_or("替代文字")?;
    assert_eq!(image_destination_at(&parsed, alt), Some("assets/pic.png"));
    assert_eq!(
        image_destination_at(&parsed, alt + '替'.len_utf8()),
        Some("assets/pic.png")
    );
    assert_eq!(
        image_destination_at(&parsed, source.find("docs").ok_or("链接文字")?),
        None,
        "链接不得按图片处理"
    );
    assert_eq!(image_destination_at(&parsed, 0), None);
    assert_eq!(
        image_destination_at(&parsed, source.find("after").ok_or("after")?),
        None
    );
    Ok(())
}

#[test]
fn nested_image_inside_link_returns_the_inner_image_target() -> Result<(), Box<dyn Error>> {
    let source = "[![替代](pic.png)](https://example.test/a)\n";
    let parsed = parse_text(source)?;
    let alt = source.find("替代").ok_or("替代文字")?;
    assert_eq!(image_destination_at(&parsed, alt), Some("pic.png"));
    assert_eq!(
        link_destination_at(&parsed, alt),
        Some("https://example.test/a"),
        "外层链接仍按链接目标打开"
    );
    assert_eq!(
        image_destination_at(&parsed, source.find("https").ok_or("链接目标")?),
        None,
        "链接目标文字不按图片处理"
    );
    Ok(())
}

#[test]
fn image_nodes_expose_empty_and_raw_destinations_verbatim() -> Result<(), Box<dyn Error>> {
    let source = "![a]() ![b](<my pics/图 (1).png>)\n";
    let parsed = parse_text(source)?;
    assert_eq!(
        image_destination_at(&parsed, source.find('a').ok_or("a")?),
        Some("")
    );
    assert_eq!(
        image_destination_at(&parsed, source.find('b').ok_or("b")?),
        Some("my pics/图 (1).png"),
        "尖括号目标按解析后的目标返回"
    );
    Ok(())
}
