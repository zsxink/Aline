//! 链接目标协议判定：与 V-03 冻结的 allowlist 和拒绝文案键逐条对齐。
//!
//! 判定只回答“能否交给宿主打开”；真正的打开动作在原生窗口层。

use aline_markdown::{LinkVerdict, is_allowed_link, link_verdict};

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
