//! V-04/V-06 容量探针：在真实 `Application` 里量解码、字体注册与 SVG 渲染的
//! 时间与位图占用。
//!
//! 为什么用真实 `Application`：文档图片走 `ImageAssetLoader::load`，字体走
//! `TextSystem::add_fonts`，SVG 走 `SvgRenderer`——它们都依赖应用上下文。
//! 脱离 App 只调 `image` crate 或 font-kit 量到的是库的行为，不是应用的行为。
//!
//! 三个子命令，各自只写 stdout 的 JSON 行，由 `scripts/probe_m4_v04_v06.py`
//! 解析并比对规则层契约：
//!
//! - `images <file>...`：解码耗时、像素尺寸、帧数、位图占用（宽×高×4×帧）。
//! - `svg <file>...`：渲染耗时与不透明像素数。
//! - `fonts <file>...`：逐文件注册耗时、错误，以及注册后新增的家族名。
//! - `shape <family> <files...>`：先注册，再报告该字族的 'm' advance。
//! - `resolve <family>...`：只解析不注册，单独暴露字体解析缓存边界。
//!
//! `shape` 存在的理由：`all_font_names()` 只证明“字族出现在列表里”，不证明
//! 渲染会用到它。`TextSystem::resolve_font` 在字族缺失时沿内置回退栈静默选
//! 下一个，所以“注册成功”和“实际生效”是两件事，必须分别观测。advance 变化
//! 是生效的直接证据；advance 不变即静默回退。
//!
//! 不测进程 RSS：`getrusage` 需要 unsafe，探针不引入 unsafe；位图占用是
//! 解码结果的确定性大小，也是预算真正要约束的量。

use std::borrow::Cow;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use gpui::{Asset, AssetLogger, ImageAssetLoader, Resource};

/// 解码结果的位图占用：GPUI 的 `RenderImage` 每像素 4 字节 BGRA，逐帧存放。
///
/// 这是解码本身的确定性开销；进程 RSS 还含解码器的临时缓冲，不在此列。
/// `DevicePixels` 是 `i32`，负值只可能来自未解码的占位图，按 0 计。
fn bitmap_bytes(width: i32, height: i32, frames: usize) -> u64 {
    let pixels = u64::from(width.max(0) as u32) * u64::from(height.max(0) as u32);
    pixels * 4 * frames as u64
}

fn describe_image(image: &gpui::RenderImage, elapsed_ms: f64, source_bytes: u64) -> String {
    let size = image.size(0);
    let frames = image.frame_count();
    serde_json::json!({
        "ok": true,
        "elapsed_ms": elapsed_ms,
        "source_bytes": source_bytes,
        "width": size.width.0,
        "height": size.height.0,
        "frames": frames,
        "bitmap_bytes": bitmap_bytes(size.width.0, size.height.0, frames),
    })
    .to_string()
}

fn probe_document_image(cx: &mut gpui::App, file: &str) -> String {
    let source_bytes = std::fs::metadata(file).map(|meta| meta.len()).unwrap_or(0);
    let resource = Resource::Path(Arc::from(Path::new(file)));
    let load = AssetLogger::<ImageAssetLoader>::load(resource, cx);
    let started = Instant::now();
    let decoded = cx.foreground_executor().block_on(load);
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    match decoded {
        Ok(image) => describe_image(&image, elapsed_ms, source_bytes),
        Err(error) => serde_json::json!({
            "ok": false,
            "elapsed_ms": elapsed_ms,
            "source_bytes": source_bytes,
            "error": error.to_string(),
        })
        .to_string(),
    }
}

fn probe_svg(cx: &mut gpui::App, file: &str) -> String {
    let source_bytes = std::fs::metadata(file).map(|meta| meta.len()).unwrap_or(0);
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let svg = cx.svg_renderer();
    let started = Instant::now();
    let rendered = svg.render_single_frame(&bytes, 1.0);
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    match rendered {
        Ok(image) => {
            let size = image.size(0);
            serde_json::json!({
                "ok": true,
                "elapsed_ms": elapsed_ms,
                "source_bytes": source_bytes,
                "width": size.width.0,
                "height": size.height.0,
                "bitmap_bytes": bitmap_bytes(size.width.0, size.height.0, image.frame_count()),
                "opaque_pixels": opaque_pixel_count(&image),
            })
            .to_string()
        }
        Err(error) => serde_json::json!({
            "ok": false,
            "elapsed_ms": elapsed_ms,
            "source_bytes": source_bytes,
            "error": error.to_string(),
        })
        .to_string(),
    }
}

fn opaque_pixel_count(image: &gpui::RenderImage) -> Option<usize> {
    let bytes = image.as_bytes(0)?;
    let pixels = bytes.as_chunks::<4>().0;
    Some(pixels.iter().filter(|pixel| pixel[3] != 0).count())
}

/// 字族在**实际布局路径**上的表现：`resolve_font` 返回的槽位，以及 'm' 的
/// shaped advance。字族不可解析时 `resolve_font` 会沿内置回退栈选下一个，
/// 所以“能否解析”与“解析到的是不是本字族”必须分开看——后者只能由 advance
/// 与已知字形宽度比对得出，探针只报告数值，判定交给规则层。
///
/// 注意顺序：`resolve_font` 会把结果写进 `TextSystem` 的 `font_ids_by_font`
/// 缓存，而 `add_fonts` 只清 `platform_text_system` 的缓存。因此“先观测再
/// 注册”与“先注册再观测”会得到不同结果，生产顺序是后者。`shape` 子命令
/// 先注册后观测；`resolve` 子命令只观测，用来单独暴露这个缓存边界。
fn probe_shape(cx: &gpui::App, family: &str) -> String {
    let text_system = cx.text_system();
    let requested = gpui::font(family);
    let font_id = text_system.resolve_font(&requested);
    let resolved_family = text_system
        .get_font_for_id(font_id)
        .map(|font| font.family.to_string());
    let advance = text_system
        .advance(font_id, gpui::px(16.0), 'm')
        .ok()
        .map(|size| size.width.as_f32());
    let em = text_system
        .em_advance(font_id, gpui::px(16.0))
        .ok()
        .map(|v| v.as_f32());
    serde_json::json!({
        "ok": true,
        "family": family,
        "font_id": format!("{font_id:?}"),
        "resolved_family": resolved_family,
        "advance_m": advance,
        "em_advance": em,
    })
    .to_string()
}

/// 逐文件注册：`add_fonts` 每次都清缓存并推进代次，所以一次只喂一个文件，
/// 否则量不到单个字体的耗时，也分不清是哪一个出错。
fn probe_font(cx: &gpui::App, file: &str) -> String {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) => {
            return serde_json::json!({ "ok": false, "error": error.to_string() }).to_string()
        }
    };
    let source_bytes = bytes.len() as u64;
    let text_system = cx.text_system();
    let before: std::collections::HashSet<String> =
        text_system.all_font_names().into_iter().collect();
    let started = Instant::now();
    let registered = text_system.add_fonts(vec![Cow::Owned(bytes)]);
    let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
    let mut added: Vec<String> = text_system
        .all_font_names()
        .into_iter()
        .filter(|name| !before.contains(name))
        .collect();
    added.sort_unstable();
    match registered {
        Ok(()) => serde_json::json!({
            "ok": true,
            "elapsed_ms": elapsed_ms,
            "source_bytes": source_bytes,
            "names_added": added,
        })
        .to_string(),
        Err(error) => serde_json::json!({
            "ok": false,
            "elapsed_ms": elapsed_ms,
            "source_bytes": source_bytes,
            "names_added": added,
            "error": error.to_string(),
        })
        .to_string(),
    }
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
            "svg" => {
                for file in &rest {
                    println!(
                        "{}",
                        serde_json::json!({
                            "file": file,
                            "result": serde_json::from_str::<serde_json::Value>(
                                &probe_svg(cx, file)).unwrap_or(serde_json::Value::Null),
                        })
                    );
                }
            }
            "fonts" => {
                for file in &rest {
                    println!(
                        "{}",
                        serde_json::json!({
                            "file": file,
                            "result": serde_json::from_str::<serde_json::Value>(
                                &probe_font(cx, file)).unwrap_or(serde_json::Value::Null),
                        })
                    );
                }
            }
            "shape" => {
                // 生产顺序：先注册，再观测。不先观测——先观测会污染缓存。
                let (family, files) = match rest.split_first() {
                    Some((family, files)) => (family.as_str(), files),
                    None => ("", &[][..]),
                };
                let mut registrations = Vec::new();
                for file in files {
                    registrations.push(serde_json::json!({
                        "file": file,
                        "result": serde_json::from_str::<serde_json::Value>(
                            &probe_font(cx, file)).unwrap_or(serde_json::Value::Null),
                    }));
                }
                let after = probe_shape(cx, family);
                println!(
                    "{}",
                    serde_json::json!({
                        "registrations": registrations,
                        "after": serde_json::from_str::<serde_json::Value>(&after)
                            .unwrap_or(serde_json::Value::Null),
                    })
                );
            }
            "resolve" => {
                // 只观测，不注册；用于单独暴露 TextSystem 的字体解析缓存边界。
                for family in &rest {
                    println!(
                        "{}",
                        serde_json::json!({
                            "family": family,
                            "result": serde_json::from_str::<serde_json::Value>(
                                &probe_shape(cx, family)).unwrap_or(serde_json::Value::Null),
                        })
                    );
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
