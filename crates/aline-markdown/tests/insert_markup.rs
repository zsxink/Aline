//! 弹窗插入的 Markdown 构件：转义、空文字回退与目标校验。
//!
//! 构件只生成源码文本，不判断文档状态；打开链接的协议判定另有 `link_verdict`。

use aline_markdown::{InlineMarkupKind, InsertRejection, inline_markup};

#[test]
fn link_markup_uses_text_target_shape() {
    assert_eq!(
        inline_markup(InlineMarkupKind::Link, "文档", "https://example.test/a"),
        Ok("[文档](https://example.test/a)".to_owned())
    );
    assert_eq!(
        inline_markup(InlineMarkupKind::Image, "图", "assets/pic.png"),
        Ok("![图](assets/pic.png)".to_owned())
    );
}

#[test]
fn empty_text_falls_back_to_the_destination() {
    assert_eq!(
        inline_markup(InlineMarkupKind::Link, "", "https://example.test/a"),
        Ok("[https://example.test/a](https://example.test/a)".to_owned())
    );
    assert_eq!(
        inline_markup(InlineMarkupKind::Image, "   ", "assets/pic.png"),
        Ok("![assets/pic.png](assets/pic.png)".to_owned())
    );
}

#[test]
fn text_escapes_brackets_and_backslashes_and_flattens_line_breaks() {
    assert_eq!(
        inline_markup(InlineMarkupKind::Link, "a]b[c\\d", "u"),
        Ok("[a\\]b\\[c\\\\d](u)".to_owned())
    );
    assert_eq!(
        inline_markup(
            InlineMarkupKind::Image,
            "第一行\n第二行\r\n第三行",
            "pic.png"
        ),
        Ok("![第一行 第二行 第三行](pic.png)".to_owned())
    );
}

#[test]
fn destinations_with_spaces_or_parentheses_use_angle_brackets() {
    assert_eq!(
        inline_markup(InlineMarkupKind::Image, "图", "my pics/图 (1).png"),
        Ok("![图](<my pics/图 (1).png>)".to_owned())
    );
    assert_eq!(
        inline_markup(InlineMarkupKind::Link, "t", "a<b>c d"),
        Ok("[t](<a\\<b\\>c d>)".to_owned())
    );
    assert_eq!(
        inline_markup(InlineMarkupKind::Link, "t", "a(b)c"),
        Ok("[t](<a(b)c>)".to_owned())
    );
}

#[test]
fn targets_are_trimmed_before_use() {
    assert_eq!(
        inline_markup(InlineMarkupKind::Link, "t", "  https://example.test/a  "),
        Ok("[t](https://example.test/a)".to_owned())
    );
}

#[test]
fn empty_and_line_breaking_targets_are_rejected_with_feedback_keys() {
    for destination in ["", "   ", "\t"] {
        assert_eq!(
            inline_markup(InlineMarkupKind::Link, "t", destination),
            Err(InsertRejection::EmptyDestination),
            "{destination}"
        );
        assert_eq!(
            inline_markup(InlineMarkupKind::Image, "t", destination),
            Err(InsertRejection::EmptyDestination),
            "{destination}"
        );
    }
    for destination in ["a\nb", "a\rb", "a\0b"] {
        assert_eq!(
            inline_markup(InlineMarkupKind::Link, "t", destination),
            Err(InsertRejection::LineBreak),
            "{destination}"
        );
        assert_eq!(
            inline_markup(InlineMarkupKind::Image, "t", destination),
            Err(InsertRejection::LineBreak),
            "{destination}"
        );
    }
    assert_eq!(
        InsertRejection::EmptyDestination.message_key(),
        "insert.rejected.empty"
    );
    assert_eq!(
        InsertRejection::LineBreak.message_key(),
        "insert.rejected.line_break"
    );
    assert_eq!(
        InsertRejection::UnsupportedScheme.message_key(),
        "insert.rejected.scheme"
    );
}

#[test]
fn links_reject_unsupported_schemes_but_keep_relative_targets() {
    for destination in [
        "javascript:alert(1)",
        "data:text/html,x",
        "file:///a.png",
        "custom:x",
    ] {
        assert_eq!(
            inline_markup(InlineMarkupKind::Link, "t", destination),
            Err(InsertRejection::UnsupportedScheme),
            "{destination}"
        );
    }
    for destination in [
        "docs/a.md",
        "./a.md",
        "../a.md",
        "#anchor",
        "/abs/a.md",
        "mailto:a@b.test",
    ] {
        assert!(
            inline_markup(InlineMarkupKind::Link, "t", destination).is_ok(),
            "{destination}"
        );
    }
}

#[test]
fn images_keep_any_single_line_reference_including_unsupported_schemes() {
    for destination in [
        "assets/pic.png",
        "file:///tmp/pic.png",
        "data:image/png;base64,AAAA",
        "https://example.test/pic.png",
        "aline-temp:0000000000000005/0000000000000001.png",
        "javascript:alert(1)",
    ] {
        assert!(
            inline_markup(InlineMarkupKind::Image, "t", destination).is_ok(),
            "{destination}"
        );
    }
}
