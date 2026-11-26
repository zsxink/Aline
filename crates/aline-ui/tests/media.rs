//! 正文图片与链接的纯策略：来源分类、魔数嗅探、入口判维、预算拒绝、
//! data: 解码、安全 SVG、链接 allowlist 与剪贴板路由。
//!
//! 这些用例不启动 GPUI、不读网络、不读磁盘；原生展示另由窗口侧接线验证。

use std::error::Error;

use aline_ui::media::{
    ClipboardImageFormat, ClipboardRoute, ClipboardWriteItem, ClipboardWritePlan, ImageDimensions,
    MediaBudget, MediaError, MediaFormat, SourceKind, classify_source, decode_data_url,
    plan_clipboard_write, probe_image, sanitize_svg, sniff_format,
};

fn budget(source: u64, pixels: u64, decoded: u64, frames: u32) -> MediaBudget {
    MediaBudget::new(source, pixels, decoded, frames)
}

fn png_header(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    bytes.extend_from_slice(&13u32.to_be_bytes());
    bytes.extend_from_slice(b"IHDR");
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
    // CRC 占位；入口判维不校验 CRC，保留真实 PNG 头部长度便于预算边界。
    bytes.extend_from_slice(&[0, 0, 0, 0]);
    bytes
}

fn jpeg_header(width: u16, height: u16) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xC0, 0x00, 0x11, 0x08];
    bytes.extend_from_slice(&height.to_be_bytes());
    bytes.extend_from_slice(&width.to_be_bytes());
    bytes.extend_from_slice(&[3, 1, 0x22, 0, 2, 0x11, 1, 3, 0x11, 1]);
    bytes
}

fn gif_header(width: u16, height: u16, frames: usize) -> Vec<u8> {
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[0x00, 0x00, 0x00]);
    for _ in 0..frames {
        bytes.extend_from_slice(&[0x21, 0xF9, 0x04, 0x00, 0x0A, 0x00, 0x00, 0x00]);
        bytes.extend_from_slice(&[0x2C, 0, 0, 0, 0]);
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&height.to_le_bytes());
        bytes.push(0x00);
        bytes.extend_from_slice(&[0x02, 0x02, 0x44, 0x01, 0x00]);
    }
    bytes.push(0x3B);
    bytes
}

fn webp_animated(width: u32, height: u32, frames: usize) -> Vec<u8> {
    let mut chunks = Vec::new();
    let mut vp8x = Vec::new();
    vp8x.push(0x02);
    vp8x.extend_from_slice(&[0, 0, 0]);
    let w = (width - 1).to_le_bytes();
    let h = (height - 1).to_le_bytes();
    vp8x.extend_from_slice(&[w[0], w[1], w[2], h[0], h[1], h[2]]);
    chunks.extend_from_slice(b"VP8X");
    chunks.extend_from_slice(&10u32.to_le_bytes());
    chunks.extend_from_slice(&vp8x);
    if frames > 0 {
        chunks.extend_from_slice(b"ANIM");
        chunks.extend_from_slice(&6u32.to_le_bytes());
        chunks.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    }
    for _ in 0..frames {
        chunks.extend_from_slice(b"ANMF");
        chunks.extend_from_slice(&16u32.to_le_bytes());
        chunks.extend_from_slice(&[0; 16]);
    }
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(
        &u32::try_from(chunks.len() + 4)
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    bytes.extend_from_slice(b"WEBP");
    bytes.extend_from_slice(&chunks);
    bytes
}

fn bmp_header(width: i32, height: i32) -> Vec<u8> {
    let mut bytes = b"BM".to_vec();
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&[0; 4]);
    bytes.extend_from_slice(&54u32.to_le_bytes());
    bytes.extend_from_slice(&40u32.to_le_bytes());
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    bytes.extend_from_slice(&[1, 0, 24, 0]);
    bytes
}

#[test]
fn source_classification_never_treats_remote_or_unknown_as_local() {
    assert_eq!(
        classify_source("data:image/png;base64,AAAA"),
        SourceKind::DataUrl
    );
    assert_eq!(
        classify_source("DATA:image/png;base64,AAAA"),
        SourceKind::DataUrl
    );
    assert_eq!(
        classify_source("https://example.test/a.png"),
        SourceKind::Remote
    );
    assert_eq!(
        classify_source("HTTP://example.test/a.png"),
        SourceKind::Remote
    );
    assert_eq!(classify_source("//example.test/a.png"), SourceKind::Remote);
    assert_eq!(classify_source("assets/pic.png"), SourceKind::Local);
    assert_eq!(classify_source("./assets/pic.png"), SourceKind::Local);
    assert_eq!(classify_source("/tmp/pic.png"), SourceKind::Local);
    assert_eq!(classify_source("file:///tmp/pic.png"), SourceKind::Local);
    assert_eq!(classify_source("file:/tmp/pic.png"), SourceKind::Local);
    assert_eq!(
        classify_source("javascript:alert(1)"),
        SourceKind::Unsupported
    );
    assert_eq!(
        classify_source("ftp://example.test/a.png"),
        SourceKind::Unsupported
    );
    assert_eq!(classify_source(""), SourceKind::Unsupported);
    assert_eq!(classify_source("   "), SourceKind::Unsupported);
}

#[test]
fn sniffing_uses_signatures_not_extensions() {
    assert_eq!(sniff_format(&png_header(1, 1)), Some(MediaFormat::Png));
    assert_eq!(sniff_format(&jpeg_header(1, 1)), Some(MediaFormat::Jpeg));
    assert_eq!(sniff_format(&gif_header(1, 1, 1)), Some(MediaFormat::Gif));
    assert_eq!(
        sniff_format(&webp_animated(1, 1, 1)),
        Some(MediaFormat::Webp)
    );
    assert_eq!(sniff_format(&bmp_header(1, 1)), Some(MediaFormat::Bmp));
    assert_eq!(
        sniff_format(b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>"),
        Some(MediaFormat::Svg)
    );
    assert_eq!(sniff_format(b"not an image"), None);
    assert_eq!(sniff_format(&[]), None);
}

#[test]
fn dimensions_and_frames_are_read_before_decode() -> Result<(), Box<dyn Error>> {
    let limits = budget(1 << 20, 1 << 24, 1 << 28, 8);

    let png = probe_image(&png_header(32, 16), limits)?;
    assert_eq!(png.format, MediaFormat::Png);
    assert_eq!(png.dimensions, ImageDimensions::new(32, 16));
    assert_eq!(png.frames, 1);

    let jpeg = probe_image(&jpeg_header(64, 48), limits)?;
    assert_eq!(jpeg.format, MediaFormat::Jpeg);
    assert_eq!(jpeg.dimensions, ImageDimensions::new(64, 48));
    assert_eq!(jpeg.frames, 1);

    let gif = probe_image(&gif_header(20, 10, 3), limits)?;
    assert_eq!(gif.format, MediaFormat::Gif);
    assert_eq!(gif.dimensions, ImageDimensions::new(20, 10));
    assert_eq!(gif.frames, 3);

    let webp = probe_image(&webp_animated(40, 30, 3), limits)?;
    assert_eq!(webp.format, MediaFormat::Webp);
    assert_eq!(webp.dimensions, ImageDimensions::new(40, 30));
    assert_eq!(webp.frames, 3);

    let bmp = probe_image(&bmp_header(12, 8), limits)?;
    assert_eq!(bmp.format, MediaFormat::Bmp);
    assert_eq!(bmp.dimensions, ImageDimensions::new(12, 8));
    assert_eq!(bmp.frames, 1);
    Ok(())
}

#[test]
fn budget_rejects_source_pixels_frames_and_decoded_bytes_before_decode()
-> Result<(), Box<dyn Error>> {
    let source = png_header(100, 100);
    assert_eq!(source.len(), 33);

    match probe_image(&source, budget(32, 1 << 24, 1 << 28, 1)) {
        Err(MediaError::SourceTooLarge {
            limit: 32,
            actual: 33,
        }) => {}
        other => return Err(format!("期望 SourceTooLarge，实际 {other:?}").into()),
    }

    match probe_image(&source, budget(33, 9_999, 1 << 28, 1)) {
        Err(MediaError::PixelLimit {
            width: 100,
            height: 100,
            limit: 9_999,
        }) => {}
        other => return Err(format!("期望 PixelLimit，实际 {other:?}").into()),
    }

    // 边界：恰好 10,000 像素允许；9,999 拒绝。
    assert!(probe_image(&source, budget(33, 10_000, 40_000, 1)).is_ok());
    assert!(probe_image(&source, budget(33, 9_999, 40_000, 1)).is_err());

    let gif = gif_header(10, 10, 4);
    match probe_image(&gif, budget(1 << 20, 1 << 24, 1 << 28, 3)) {
        Err(MediaError::FrameLimit {
            frames: 4,
            limit: 3,
        }) => {}
        other => return Err(format!("期望 FrameLimit，实际 {other:?}").into()),
    }

    match probe_image(&source, budget(33, 10_000, 39_999, 1)) {
        Err(MediaError::DecodedLimit {
            bytes: 40_000,
            limit: 39_999,
        }) => {}
        other => return Err(format!("期望 DecodedLimit，实际 {other:?}").into()),
    }
    Ok(())
}

#[test]
fn data_url_decodes_base64_and_percent_encoding_with_size_limit() -> Result<(), Box<dyn Error>> {
    let png = png_header(2, 2);
    let encoded = "iVBORw0KGgoAAAANSUhEUgAAAAIAAAACCAYAAABytg0k";
    let decoded = decode_data_url(
        &format!("data:image/png;base64,{encoded}"),
        budget(1024, 1024, 4096, 1),
    )?;
    assert_eq!(decoded.len(), png.len());
    // 夹具的 CRC 是真实值，本地 helper 用零占位；入口判维不校验 CRC。
    assert_eq!(&decoded[..29], &png[..29]);

    let percent = decode_data_url(
        "data:image/png,%89PNG%0D%0A%1A%0A",
        budget(1024, 1024, 4096, 1),
    )?;
    assert_eq!(percent, b"\x89PNG\x0D\x0A\x1A\x0A");

    match decode_data_url("data:image/png;base64,AAAA", budget(2, 1024, 4096, 1)) {
        Err(MediaError::SourceTooLarge {
            limit: 2,
            actual: 3,
        }) => {}
        other => return Err(format!("期望 SourceTooLarge，实际 {other:?}").into()),
    }
    assert!(matches!(
        decode_data_url("data:image/png;base64,!!!", budget(1024, 1024, 4096, 1)),
        Err(MediaError::InvalidDataUrl)
    ));
    assert!(matches!(
        decode_data_url("data:image/png;base64", budget(1024, 1024, 4096, 1)),
        Err(MediaError::InvalidDataUrl)
    ));
    Ok(())
}

#[test]
fn svg_is_sanitized_without_scripts_foreign_objects_or_remote_hrefs() -> Result<(), Box<dyn Error>>
{
    let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50" viewBox="0 0 100 50">
  <script>alert(1)</script>
  <foreignObject width="10" height="10"><div xmlns="http://www.w3.org/1999/xhtml">x</div></foreignObject>
  <image href="https://evil.test/x.png" width="10" height="10"/>
  <rect width="100" height="50" fill="red"/>
</svg>"#;
    let safe = sanitize_svg(source, budget(1 << 20, 1 << 24, 1 << 28, 1))?;
    assert_eq!(safe.dimensions(), ImageDimensions::new(100, 50));
    assert_eq!(safe.frames(), 1);
    let text = std::str::from_utf8(safe.bytes())?;
    assert!(text.contains("<svg"), "{text}");
    // usvg 把基础图形规范化为 path；保留可见几何即可。
    assert!(text.contains("<path"), "{text}");
    assert!(!text.contains("script"), "{text}");
    assert!(!text.contains("foreignObject"), "{text}");
    assert!(!text.contains("evil.test"), "{text}");
    Ok(())
}

#[test]
fn svg_data_href_is_kept_but_never_fetched() -> Result<(), Box<dyn Error>> {
    let tiny_png = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==";
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="10"><image href="data:image/png;base64,{tiny_png}" width="10" height="10"/></svg>"#
    );
    let safe = sanitize_svg(source.as_bytes(), budget(1 << 20, 1 << 24, 1 << 28, 1))?;
    let text = std::str::from_utf8(safe.bytes())?;
    assert!(text.contains("data:image/png"), "{text}");
    Ok(())
}

#[test]
fn svg_budget_is_checked_at_the_fixed_render_scale() -> Result<(), Box<dyn Error>> {
    let source = br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="100" height="100"/></svg>"#;
    let info = probe_image(source, budget(1 << 20, 40_000, 160_000, 1))?;
    assert_eq!(info.format, MediaFormat::Svg);
    assert_eq!(info.dimensions, ImageDimensions::new(100, 100));
    assert_eq!(info.frames, 1);

    match probe_image(source, budget(1 << 20, 39_999, 160_000, 1)) {
        Err(MediaError::PixelLimit {
            width: 200,
            height: 200,
            limit: 39_999,
        }) => {}
        other => return Err(format!("期望 PixelLimit，实际 {other:?}").into()),
    }
    match probe_image(source, budget(1 << 20, 40_000, 159_999, 1)) {
        Err(MediaError::DecodedLimit {
            bytes: 160_000,
            limit: 159_999,
        }) => {}
        other => return Err(format!("期望 DecodedLimit，实际 {other:?}").into()),
    }
    Ok(())
}

#[test]
fn clipboard_write_plan_keeps_single_items_and_only_strings_for_multi() {
    assert_eq!(
        plan_clipboard_write(&[ClipboardWriteItem::Text("写入的纯文本")]),
        ClipboardWritePlan::Text("写入的纯文本".into())
    );
    assert_eq!(
        plan_clipboard_write(&[ClipboardWriteItem::Image(ClipboardImageFormat::Png)]),
        ClipboardWritePlan::Image(ClipboardImageFormat::Png)
    );
    assert_eq!(
        plan_clipboard_write(&[
            ClipboardWriteItem::Text("文本先"),
            ClipboardWriteItem::Image(ClipboardImageFormat::Png),
        ]),
        ClipboardWritePlan::Text("文本先".into())
    );
    assert_eq!(
        plan_clipboard_write(&[ClipboardWriteItem::Text("a"), ClipboardWriteItem::Text("b"),]),
        ClipboardWritePlan::Text("ab".into())
    );
    assert_eq!(
        plan_clipboard_write(&[
            ClipboardWriteItem::Image(ClipboardImageFormat::Png),
            ClipboardWriteItem::Image(ClipboardImageFormat::Jpeg),
        ]),
        ClipboardWritePlan::None
    );
    assert_eq!(
        plan_clipboard_write(&[ClipboardWriteItem::Other]),
        ClipboardWritePlan::None
    );
    assert_eq!(plan_clipboard_write(&[]), ClipboardWritePlan::None);
    assert_eq!(
        plan_clipboard_write(&[ClipboardWriteItem::Other, ClipboardWriteItem::Text("x"),]),
        ClipboardWritePlan::Text("x".into())
    );
}

#[test]
fn clipboard_read_order_and_image_priority_are_frozen() {
    assert_eq!(
        ClipboardRoute::from_flavors(&["text", "files", "png"]),
        ClipboardRoute::Files
    );
    assert_eq!(
        ClipboardRoute::from_flavors(&["png", "text"]),
        ClipboardRoute::Text
    );
    assert_eq!(
        ClipboardRoute::from_flavors(&["png"]),
        ClipboardRoute::Image(ClipboardImageFormat::Png)
    );
    assert_eq!(
        ClipboardRoute::from_flavors(&["gif", "webp"]),
        ClipboardRoute::Image(ClipboardImageFormat::Webp)
    );
    assert_eq!(
        ClipboardRoute::from_flavors(&["html"]),
        ClipboardRoute::None
    );
    assert_eq!(
        ClipboardRoute::from_flavors(&["rtf", "text"]),
        ClipboardRoute::Text
    );
    assert_eq!(ClipboardRoute::from_flavors(&[]), ClipboardRoute::None);
}

#[test]
fn clipboard_route_from_entries_keeps_the_frozen_priority() {
    use ClipboardImageFormat::{Gif, Png, Tiff, Webp};
    assert_eq!(
        ClipboardRoute::from_entries(true, true, &[Png]),
        ClipboardRoute::Files
    );
    assert_eq!(
        ClipboardRoute::from_entries(false, true, &[Png]),
        ClipboardRoute::Text
    );
    assert_eq!(
        ClipboardRoute::from_entries(false, false, &[Gif, Webp]),
        ClipboardRoute::Image(Webp)
    );
    assert_eq!(
        ClipboardRoute::from_entries(false, false, &[Tiff, Png]),
        ClipboardRoute::Image(Png)
    );
    assert_eq!(
        ClipboardRoute::from_entries(false, false, &[]),
        ClipboardRoute::None
    );
}
