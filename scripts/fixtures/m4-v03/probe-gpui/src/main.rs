//! V-03 GPUI 侧探针：在真实 `Application` 里量图片解码、SVG 安全与剪贴板。
//!
//! 为什么必须起 `Application`：文档图片走 `ImageAssetLoader::load`，它需要
//! `&mut App` 才能拿到 SVG renderer、asset source 与 http client。脱离 App
//! 直接调解码函数只能证明 `image` crate 能解，不能证明**应用**里能解——而
//! V-03 要的正是后者。
//!
//! 三个子命令，各自只写 stdout 的 JSON 行，由驱动 `scripts/probe_m4_v03.py`
//! 解析、比对规则层契约并落证据：
//!
//! - `images <file>...`：文档路径解码（格式、像素尺寸、帧数、首帧延时、首像素）。
//! - `svg <case>...`：SVG 安全边界。每个 case 是驱动生成到临时目录的 SVG 与其
//!   目标探针文件；探针只报告“渲染出的图像节点指向哪份字节”，不判断对错。
//! - `clipboard <flavor>`：读剪贴板并按读取优先级报告命中项与解码结果。

use std::path::Path;
use std::sync::Arc;

use gpui::{
    Asset, AssetLogger, ClipboardEntry, Image, ImageAssetLoader, ImageFormat, Resource, SvgRenderer,
};

/// 首像素的十六进制，便于驱动比对通道顺序而不必传二进制。
fn first_pixel(image: &gpui::RenderImage, frame: usize) -> Option<String> {
    let bytes = image.as_bytes(frame)?;
    let head: Vec<String> = bytes.iter().take(4).map(|b| format!("{b:02x}")).collect();
    Some(head.join(""))
}

fn describe_image(image: &gpui::RenderImage) -> String {
    let size = image.size(0);
    let (numer, denom) = image.delay(0).numer_denom_ms();
    serde_json::json!({
        "width": size.width.0,
        "height": size.height.0,
        "frames": image.frame_count(),
        "delay_numer_ms": numer,
        "delay_denom": denom,
        "first_pixel_bgra": first_pixel(image, 0),
        "bytes": image.as_bytes(0).map(|bytes| bytes.len()),
    })
    .to_string()
}

/// 文档路径：`ImageAssetLoader::load` 是应用里 `<img>` 元素真正走的解码入口。
fn probe_document_image(cx: &mut gpui::App, file: &str) -> String {
    let resource = Resource::Path(Arc::from(Path::new(file)));
    let load = AssetLogger::<ImageAssetLoader>::load(resource, cx);
    match cx.foreground_executor().block_on(load) {
        Ok(image) => describe_image(&image),
        Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
    }
}

/// 剪贴板路径：`Image::to_image_data`，与文档路径**不是**同一段代码。
fn probe_clipboard_image(image: &Image, svg: SvgRenderer) -> String {
    match image.to_image_data(svg) {
        Ok(rendered) => describe_image(&rendered),
        Err(error) => serde_json::json!({ "error": error.to_string() }).to_string(),
    }
}

fn describe_clipboard(item: Option<gpui::ClipboardItem>, svg: SvgRenderer) -> String {
    let Some(item) = item else {
        return serde_json::json!({ "route": "none" }).to_string();
    };
    let entries: Vec<serde_json::Value> = item
        .entries
        .iter()
        .map(|entry| match entry {
            ClipboardEntry::String(string) => serde_json::json!({
                "kind": "string",
                "text": string.text,
                "metadata": string.metadata,
            }),
            ClipboardEntry::Image(image) => {
                let decoded: serde_json::Value =
                    serde_json::from_str(&probe_clipboard_image(image, svg.clone()))
                        .unwrap_or(serde_json::Value::Null);
                serde_json::json!({
                    "kind": "image",
                    "format": format!("{:?}", image.format()),
                    "declared_bytes": image.bytes().len(),
                    "decoded": decoded,
                })
            }
            ClipboardEntry::ExternalPaths(paths) => serde_json::json!({
                "kind": "paths",
                "count": paths.0.len(),
                "first": paths.0.first().map(|p| p.to_string_lossy().into_owned()),
            }),
        })
        .collect();
    serde_json::json!({ "route": "hit", "entries": entries }).to_string()
}

/// SVG 安全：用**默认** resolver 解析，报告每个 `<image>` 节点实际拿到了什么。
///
/// 这是“锁定 GPUI 当前会怎么做”的事实，不是“我们想要它怎么做”。生产要求
/// 用自定义 resolver 阻断字符串 href；`deny` 子命令量的是阻断后的结果。
fn probe_svg(cx: &mut gpui::App, file: &str, deny_strings: bool) -> String {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) => return serde_json::json!({ "error": error.to_string() }).to_string(),
    };
    let svg = cx.svg_renderer();
    if !deny_strings {
        // 默认路径：走 SvgRenderer 自己的 usvg 选项（含默认 string resolver）。
        return match svg.render_single_frame(&bytes, 1.0) {
            Ok(image) => {
                let size = image.size(0);
                serde_json::json!({
                    "mode": "default",
                    "width": size.width.0,
                    "height": size.height.0,
                    "frames": image.frame_count(),
                    "first_pixel_bgra": first_pixel(&image, 0),
                    "pixels": opaque_pixel_count(&image),
                })
                .to_string()
            }
            Err(error) => {
                serde_json::json!({ "mode": "default", "error": error.to_string() }).to_string()
            }
        };
    }

    // 自定义路径：只放行 data:，拒绝一切字符串 href（含相对/绝对/../ 与 file:）。
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_href: &str, _options: &usvg::Options| None),
        },
        ..Default::default()
    };
    let tree = match usvg::Tree::from_data(&bytes, &options) {
        Ok(tree) => tree,
        Err(error) => {
            return serde_json::json!({ "mode": "deny", "error": error.to_string() }).to_string()
        }
    };
    let images = image_nodes(&tree);
    serde_json::json!({
        "mode": "deny",
        "image_nodes": images.len(),
        "kinds": images,
    })
    .to_string()
}

/// 遍历 usvg 树里的 `<image>` 节点，报告其内联字节类型与大小。
///
/// `deny` 模式下若字符串 href 被阻断，被阻断的 `<image>` 根本不会成为节点；
/// 因此 `kinds` 为空 = 阻断生效，`kinds` 含 PNG(…) = 字节来自 data: 且被接受。
fn image_nodes(tree: &usvg::Tree) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending: Vec<usvg::Node> = tree.root().children().to_vec();
    while let Some(node) = pending.pop() {
        match &node {
            usvg::Node::Group(group) => pending.extend(group.children().iter().cloned()),
            usvg::Node::Image(image) => found.push(match image.kind() {
                usvg::ImageKind::PNG(data) => format!("PNG({})", data.len()),
                usvg::ImageKind::JPEG(data) => format!("JPEG({})", data.len()),
                usvg::ImageKind::GIF(data) => format!("GIF({})", data.len()),
                usvg::ImageKind::WEBP(data) => format!("WEBP({})", data.len()),
                usvg::ImageKind::SVG(_) => "SVG".to_string(),
            }),
            _ => {}
        }
    }
    found
}

/// 不透明像素数：用于区分“渲染了内容”与“整张透明”。
fn opaque_pixel_count(image: &gpui::RenderImage) -> Option<usize> {
    let bytes = image.as_bytes(0)?;
    let pixels = bytes.as_chunks::<4>().0;
    Some(pixels.iter().filter(|pixel| pixel[3] != 0).count())
}

fn main() {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_default();
    let rest: Vec<String> = arguments.collect();

    let platform = gpui_macos::MacPlatform::new(true);
    let app = gpui::Application::new_inaccessible(std::rc::Rc::new(platform));
    let handle = app.run_embedded(move |cx| {
        match command.as_str() {
            "images" => {
                for file in &rest {
                    println!(
                        "{}",
                        serde_json::json!({
                            "file": file,
                            "result": serde_json::from_str::<serde_json::Value>(
                                &probe_document_image(cx, file)).unwrap_or(serde_json::Value::Null),
                        })
                    );
                }
            }
            "svg-default" | "svg-deny" => {
                let deny = command == "svg-deny";
                for file in &rest {
                    println!(
                        "{}",
                        serde_json::json!({
                            "file": file,
                            "result": serde_json::from_str::<serde_json::Value>(
                                &probe_svg(cx, file, deny)).unwrap_or(serde_json::Value::Null),
                        })
                    );
                }
            }
            "clipboard" => {
                let svg = cx.svg_renderer();
                let item = cx.read_from_clipboard();
                println!("{}", describe_clipboard(item, svg));
            }
            "clipboard-write-text" => {
                let text = rest.first().cloned().unwrap_or_default();
                cx.write_to_clipboard(gpui::ClipboardItem {
                    entries: vec![ClipboardEntry::String(gpui::ClipboardString {
                        text,
                        metadata: None,
                    })],
                });
                println!("{}", serde_json::json!({ "wrote": "text" }));
            }
            "clipboard-write-mixed" => {
                // 文本 + 图片一次写入：量的是 `Pasteboard::write` 的多项分支
                // （只拼接 String，Image/ExternalPaths 被丢弃），不是我们挑的。
                let text = rest.first().cloned().unwrap_or_default();
                let file = rest.get(1).cloned().unwrap_or_default();
                let bytes = match std::fs::read(&file) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        println!("{}", serde_json::json!({ "error": error.to_string() }));
                        return;
                    }
                };
                cx.write_to_clipboard(gpui::ClipboardItem {
                    entries: vec![
                        ClipboardEntry::String(gpui::ClipboardString {
                            text,
                            metadata: None,
                        }),
                        ClipboardEntry::Image(Image {
                            format: ImageFormat::Png,
                            bytes,
                            id: 2,
                        }),
                    ],
                });
                println!("{}", serde_json::json!({ "wrote": "mixed" }));
            }
            "clipboard-write-image" => {
                // 第二个参数是格式名：写出时必须声明**正确**的 UTI，
                // 否则读回时 `Image::to_image_data` 会按错误的格式解码。
                let file = rest.first().cloned().unwrap_or_default();
                let format = match rest.get(1).map(String::as_str) {
                    Some("gif") => ImageFormat::Gif,
                    Some("webp") => ImageFormat::Webp,
                    Some("jpeg") => ImageFormat::Jpeg,
                    Some("bmp") => ImageFormat::Bmp,
                    _ => ImageFormat::Png,
                };
                match std::fs::read(&file) {
                    Ok(bytes) => {
                        cx.write_to_clipboard(gpui::ClipboardItem {
                            entries: vec![ClipboardEntry::Image(Image {
                                format,
                                bytes,
                                id: 1,
                            })],
                        });
                        println!(
                            "{}",
                            serde_json::json!({ "wrote": "image",
                                                          "format": format!("{format:?}") })
                        );
                    }
                    Err(error) => {
                        println!("{}", serde_json::json!({ "error": error.to_string() }))
                    }
                }
            }
            other => {
                println!(
                    "{}",
                    serde_json::json!({ "error": format!("unknown command: {other}") })
                )
            }
        }
        cx.quit();
    });
    let _ = handle;
}
