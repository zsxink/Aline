//! 链接目标协议判定：与 V-03 冻结的 allowlist 和拒绝文案键逐条对齐。
//!
//! 判定只回答“能否交给宿主打开”；真正的打开动作在原生窗口层。

use std::error::Error;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    LinkVerdict, ParseBudget, ParseConfig, Parsed, external_link_url, is_allowed_link,
    link_destination_at, link_verdict, parse,
};

fn parse_text(source: &str) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(1 << 20))?;
    Ok(parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?)
}

#[test]
fn frozen_allowlist_accepts_only_http_https_mailto_and_local_prefixes() {
    for destination in [
        "https://example.test/a?b=1",
        "http://example.test",
        "HTTP://EXAMPLE.TEST",
        "mailto:someone@example.test",
        "MAILTO:SOMEONE@EXAMPLE.TEST",
        "#section",
        "/docs/a.md",
        "//example.test/a",
        "  https://example.test  ",
    ] {
        assert_eq!(
            link_verdict(destination),
            LinkVerdict::Allowed,
            "{destination}"
        );
        assert!(is_allowed_link(destination), "{destination}");
    }
}

#[test]
fn empty_and_whitespace_targets_are_rejected_as_empty() {
    for destination in ["", "   ", "\t\n"] {
        assert_eq!(
            link_verdict(destination),
            LinkVerdict::Empty,
            "{destination}"
        );
        assert!(!is_allowed_link(destination), "{destination}");
    }
}

#[test]
fn every_scheme_like_prefix_is_rejected_as_unsupported_scheme() {
    for destination in [
        "javascript:alert(1)",
        "JAVASCRIPT:alert(1)",
        "data:text/html,<svg/>",
        "vbscript:msgbox(1)",
        "file:///tmp/a.md",
        "ftp://example.test/a",
        "tel:+123456",
        "ssh://host/a",
        "smb://host/share",
        "custom:thing",
        "C:\\notes\\a.md",
        "a:b/c",
    ] {
        assert_eq!(
            link_verdict(destination),
            LinkVerdict::UnsupportedScheme,
            "{destination}"
        );
        assert!(!is_allowed_link(destination), "{destination}");
    }
}

#[test]
fn bare_relative_paths_and_unknown_targets_are_rejected_as_unsupported() {
    for destination in [
        "notes/a.md",
        "./a.md",
        "../a.md",
        "a.md",
        "www.example.test",
        "?q=1",
        "notes/a:b.md",
    ] {
        assert_eq!(
            link_verdict(destination),
            LinkVerdict::UnsupportedTarget,
            "{destination}"
        );
        assert!(!is_allowed_link(destination), "{destination}");
    }
}

#[test]
fn verdicts_expose_the_frozen_feedback_keys() {
    assert_eq!(LinkVerdict::Allowed.message_key(), "link.allowed");
    assert_eq!(LinkVerdict::Empty.message_key(), "link.rejected.empty");
    assert_eq!(
        LinkVerdict::UnsupportedScheme.message_key(),
        "link.rejected.scheme"
    );
    assert_eq!(
        LinkVerdict::UnsupportedTarget.message_key(),
        "link.rejected.unsupported"
    );
}

#[test]
fn link_destination_is_returned_only_for_offsets_inside_the_link_text() -> Result<(), Box<dyn Error>>
{
    let source = "before [docs](https://example.test/a) after ![alt](pic.png)\n";
    let parsed = parse_text(source)?;
    let link_start = source.find("docs").ok_or("link 文字")?;
    assert_eq!(
        link_destination_at(&parsed, link_start),
        Some("https://example.test/a")
    );
    assert_eq!(
        link_destination_at(&parsed, link_start + 3),
        Some("https://example.test/a")
    );
    assert_eq!(link_destination_at(&parsed, 0), None);
    assert_eq!(
        link_destination_at(&parsed, source.find("after").ok_or("after")?),
        None
    );
    assert_eq!(
        link_destination_at(&parsed, source.find("alt").ok_or("alt")?),
        None,
        "图片目标不得按链接打开"
    );
    Ok(())
}

#[test]
fn only_external_schemes_are_handed_to_the_host() {
    for (destination, expected) in [
        (
            "https://example.test/a?b=1",
            Some("https://example.test/a?b=1"),
        ),
        ("HTTP://EXAMPLE.TEST", Some("HTTP://EXAMPLE.TEST")),
        (
            "mailto:someone@example.test",
            Some("mailto:someone@example.test"),
        ),
        ("  https://example.test  ", Some("https://example.test")),
        ("#section", None),
        ("/docs/a.md", None),
        ("//example.test/a", None),
        ("javascript:alert(1)", None),
        ("file:///tmp/a.md", None),
        ("docs/a.md", None),
        ("", None),
    ] {
        assert_eq!(external_link_url(destination), expected, "{destination}");
    }
}

#[test]
fn nested_emphasis_inside_a_link_keeps_the_outer_destination() -> Result<(), Box<dyn Error>> {
    let source = "[**bold** text](#anchor)\n";
    let parsed = parse_text(source)?;
    let bold = source.find("bold").ok_or("bold")?;
    assert_eq!(link_destination_at(&parsed, bold), Some("#anchor"));
    assert_eq!(
        link_destination_at(&parsed, source.find("text").ok_or("text")?),
        Some("#anchor")
    );
    Ok(())
}
