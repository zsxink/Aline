//! 正文图片入口：来源分类、本地路径解析、预算前置判定、安全 SVG 与格式矩阵。
//!
//! 样本取自 V-03 冻结夹具；这些用例不启动 GPUI、不发网络请求。

use std::error::Error;
use std::path::{Path, PathBuf};

use aline_ui::body_image::{BodyImage, BodyImageError, BodyImageSource, resolve_body_image};
use aline_ui::media::{ImageDimensions, MediaBudget, MediaError, MediaFormat};

fn samples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fixtures/m4-v03/samples")
}

fn budget() -> MediaBudget {
    MediaBudget::new(1 << 20, 1 << 24, 1 << 28, 8)
}

fn resolve(name: &str) -> Result<BodyImage, BodyImageError> {
    resolve_body_image(name, &samples(), Some(budget()))
}

fn expect(name: &str) -> Result<BodyImage, Box<dyn Error>> {
    Ok(resolve(name)?)
}

fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::new();
    for chunk in bytes.chunks(3) {
        let first = u32::from(chunk[0]);
        let second = chunk.get(1).copied().map_or(0, u32::from);
        let third = chunk.get(2).copied().map_or(0, u32::from);
        let value = (first << 16) | (second << 8) | third;
        let digit = |shift: u32| {
            let index = usize::try_from((value >> shift) & 0x3F).unwrap_or(0);
            char::from(TABLE[index])
        };
        output.push(digit(18));
        output.push(digit(12));
        output.push(if chunk.len() > 1 { digit(6) } else { '=' });
        output.push(if chunk.len() > 2 { digit(0) } else { '=' });
    }
    output
}

#[test]
fn supported_samples_resolve_to_their_frozen_format_dimensions_and_frames()
-> Result<(), Box<dyn Error>> {
    for (name, format, width, height, frames) in [
        ("plain.png", MediaFormat::Png, 2, 2, 1),
        ("anim.png", MediaFormat::Png, 4, 4, 1),
        ("plain.jpg", MediaFormat::Jpeg, 2, 2, 1),
        ("plain.gif", MediaFormat::Gif, 2, 2, 1),
        ("anim.gif", MediaFormat::Gif, 4, 4, 3),
        ("plain.webp", MediaFormat::Webp, 2, 2, 1),
        ("anim.webp", MediaFormat::Webp, 4, 4, 3),
        ("plain.bmp", MediaFormat::Bmp, 2, 2, 1),
        ("plain.svg", MediaFormat::Svg, 2, 2, 1),
    ] {
        let image = expect(name)?;
        assert_eq!(image.format(), format, "{name}");
        assert_eq!(
            image.dimensions(),
            ImageDimensions::new(width, height),
            "{name}"
        );
        assert_eq!(image.frames(), frames, "{name}");
        match (name, image.source()) {
            ("plain.svg", BodyImageSource::Bytes(bytes)) => {
                let text = std::str::from_utf8(bytes)?;
                assert!(text.contains("<svg"), "{text}");
            }
            ("plain.svg", other) => return Err(format!("SVG 应走净化字节，实际 {other:?}").into()),
            (_, BodyImageSource::Local(path)) => {
                assert!(path.ends_with(name), "{path:?}");
            }
            (_, other) => return Err(format!("栅格图应保留本地路径，实际 {other:?}").into()),
        }
    }
    Ok(())
}

#[test]
fn remote_targets_are_rejected_before_any_local_read() {
    for destination in [
        "https://example.test/a.png",
        "HTTP://EXAMPLE.TEST/a.png",
        "//example.test/a.png",
    ] {
        assert_eq!(
            resolve(destination),
            Err(BodyImageError::Remote),
            "{destination}"
        );
    }
}

#[test]
fn unknown_schemes_and_empty_targets_are_rejected_as_unsupported() {
    for destination in ["javascript:alert(1)", "ftp://example.test/a.png", "", "   "] {
        assert_eq!(
            resolve(destination),
            Err(BodyImageError::UnsupportedScheme),
            "{destination}"
        );
    }
    // data: 是允许的来源，但非图片载荷按签名拒绝。
    assert_eq!(
        resolve("data:text/html,<b>x</b>"),
        Err(BodyImageError::Media(MediaError::UnsupportedFormat))
    );
}

#[test]
fn no_budget_never_decodes_a_supported_file() {
    assert_eq!(
        resolve_body_image("plain.png", &samples(), None),
        Err(BodyImageError::NoBudget)
    );
}

#[test]
fn source_pixel_frame_and_decoded_budgets_are_enforced_before_decode() {
    let source_too_small = MediaBudget::new(10, 1 << 24, 1 << 28, 8);
    assert!(matches!(
        resolve_body_image("plain.png", &samples(), Some(source_too_small)),
        Err(BodyImageError::Media(MediaError::SourceTooLarge { .. }))
    ));

    let pixels_too_small = MediaBudget::new(1 << 20, 3, 1 << 28, 8);
    assert!(matches!(
        resolve_body_image("plain.png", &samples(), Some(pixels_too_small)),
        Err(BodyImageError::Media(MediaError::PixelLimit { .. }))
    ));

    let frames_too_small = MediaBudget::new(1 << 20, 1 << 24, 1 << 28, 2);
    assert!(matches!(
        resolve_body_image("anim.gif", &samples(), Some(frames_too_small)),
        Err(BodyImageError::Media(MediaError::FrameLimit { .. }))
    ));

    let decoded_too_small = MediaBudget::new(1 << 20, 1 << 24, 15, 8);
    assert!(matches!(
        resolve_body_image("plain.png", &samples(), Some(decoded_too_small)),
        Err(BodyImageError::Media(MediaError::DecodedLimit { .. }))
    ));
}

#[test]
fn data_urls_are_decoded_and_svg_is_sanitized_before_rendering() -> Result<(), Box<dyn Error>> {
    let png = std::fs::read(samples().join("plain.png"))?;
    let destination = format!("data:image/png;base64,{}", base64(&png));
    let image = resolve(&destination)?;
    assert_eq!(image.format(), MediaFormat::Png);
    assert_eq!(image.dimensions(), ImageDimensions::new(2, 2));
    match image.source() {
        BodyImageSource::Bytes(bytes) => assert_eq!(bytes, &png),
        other => return Err(format!("data: PNG 应为解码字节，实际 {other:?}").into()),
    }

    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="4" height="4"><script>alert(1)</script><image href="file:///etc/passwd" width="4" height="4"/><rect width="4" height="4"/></svg>"#;
    let destination = format!("data:image/svg+xml;base64,{}", base64(svg));
    let image = resolve(&destination)?;
    assert_eq!(image.format(), MediaFormat::Svg);
    assert_eq!(image.dimensions(), ImageDimensions::new(4, 4));
    match image.source() {
        BodyImageSource::Bytes(bytes) => {
            let text = std::str::from_utf8(bytes)?;
            assert!(!text.contains("script"), "{text}");
            assert!(!text.contains("etc/passwd"), "{text}");
            assert!(text.contains("<svg"), "{text}");
        }
        other => return Err(format!("data: SVG 应为净化字节，实际 {other:?}").into()),
    }
    Ok(())
}

#[test]
fn unsupported_signatures_and_missing_files_are_rejected() {
    let destination = format!("data:image/png;base64,{}", base64(b"not an image"));
    assert!(matches!(
        resolve(&destination),
        Err(BodyImageError::Media(MediaError::UnsupportedFormat))
    ));
    assert_eq!(
        resolve("missing.png"),
        Err(BodyImageError::Io),
        "缺失文件不得当成空图"
    );
}

#[test]
fn relative_file_urls_and_percent_escapes_resolve_against_the_document_directory()
-> Result<(), Box<dyn Error>> {
    let direct = expect("plain.png")?;
    let BodyImageSource::Local(direct_path) = direct.source() else {
        return Err("本地样本应保留路径".into());
    };
    assert_eq!(direct_path, &samples().join("plain.png"));

    let escaped = expect("plain%2Epng")?;
    let BodyImageSource::Local(escaped_path) = escaped.source() else {
        return Err("百分号转义应解析为同一文件".into());
    };
    assert_eq!(escaped_path, direct_path);

    let file_url = format!("file://{}", direct_path.display());
    let from_url = expect(&file_url)?;
    let BodyImageSource::Local(url_path) = from_url.source() else {
        return Err("file: URL 应解析为本地路径".into());
    };
    assert_eq!(url_path, direct_path);
    Ok(())
}
