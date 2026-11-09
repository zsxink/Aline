#![forbid(unsafe_code)]
//! W1 隔离主线程离屏能力探针；依赖准入前只准备源码，不构建或运行。

use std::{fs, io, io::Write, path::Path, rc::Rc};

use gpui::{
    AppContext, Context, IntoElement, ParentElement, Render, Styled, VisualTestAppContext, Window,
    div, px, rgb, size,
};
use gpui_macos::MacPlatform;

const LOGICAL_WIDTH: u32 = 640;
const LOGICAL_HEIGHT: u32 = 400;

/// 两个色块与中文测试内容是独立场景夹具，不是产品文案。
struct SpikeScene;

impl Render for SpikeScene {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(rgb(0xf4f2ed))
            .p(px(24.0))
            .flex()
            .flex_col()
            .gap(px(16.0))
            .child(div().w(px(180.0)).h(px(64.0)).bg(rgb(0x1d6b53)))
            .child(div().w(px(96.0)).h(px(40.0)).bg(rgb(0xc55b3b)))
            .child("离屏能力测试：中文、标点。Aline 123")
    }
}

fn rejected(message: &str) -> io::Error {
    io::Error::other(message)
}

fn store_new(output: &Path, name: &str, contents: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output.join(name))?;
    file.write_all(contents)
}

fn check_pixels(width: u32, height: u32, scale: f32, rgba: &[u8]) -> io::Result<()> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(rejected("无效显示缩放"));
    }
    let expected_width = f64::from(LOGICAL_WIDTH) * f64::from(scale);
    let expected_height = f64::from(LOGICAL_HEIGHT) * f64::from(scale);
    if (f64::from(width) - expected_width).abs() > 0.5
        || (f64::from(height) - expected_height).abs() > 0.5
    {
        return Err(rejected("纹理尺寸不符合逻辑尺寸及真实缩放"));
    }
    let length = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(|| rejected("像素尺寸溢出"))?;
    if rgba.len() != length || rgba.is_empty() {
        return Err(rejected("RGBA 长度错误或纹理为空"));
    }
    let first = rgba.get(..4).ok_or_else(|| rejected("缺少首像素"))?;
    let pixels = rgba.as_chunks::<4>().0;
    if !pixels.iter().any(|pixel| pixel.as_slice() != first)
        || !pixels.iter().any(|pixel| pixel[3] != 0)
    {
        return Err(rejected("纹理全透明或单色，未证明场景被绘制"));
    }
    Ok(())
}

fn ppm(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut bytes = format!("P6\n{width} {height}\n255\n").into_bytes();
    for pixel in rgba.as_chunks::<4>().0 {
        bytes.extend_from_slice(&pixel[..3]);
    }
    bytes
}

/// 仅由独立二进制 main 的初始线程调用；不走 Rust 测试工作线程。
///
/// 调用方须先完成候选精确准入，在 logs/validation 下预建独立空输出目录。
/// 该函数只证明离屏纹理读取，不声称发生屏幕呈现或真实用户输入。
pub fn run_w1_spike(output: &Path) -> io::Result<()> {
    let allowed = std::env::current_dir()?
        .join("logs/validation")
        .canonicalize()?;
    let output = output.canonicalize()?;
    if !output.starts_with(allowed) || !output.is_dir() {
        return Err(rejected("输出目录必须位于 logs/validation 内"));
    }
    let platform = Rc::new(MacPlatform::new(true));
    let mut cx = VisualTestAppContext::new(platform);
    let window = cx
        .open_offscreen_window(size(px(640.0), px(400.0)), |_, cx| cx.new(|_| SpikeScene))
        .map_err(|error| rejected(&error.to_string()))?;
    cx.run_until_parked();

    let mut previous: Option<Vec<u8>> = None;
    let mut records = String::from(
        "kind=method\tboundary=offscreen-texture-readback\tscreen_present=false\tmain_thread_entry=executable-main\n",
    );
    for sample in 1..=2 {
        let (logical_size, scale) = cx
            .update_window(window.into(), |_, window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
                (window.viewport_size(), window.scale_factor())
            })
            .map_err(|error| rejected(&error.to_string()))?;
        if logical_size != size(px(640.0), px(400.0)) {
            return Err(rejected("离屏窗口逻辑尺寸不符合请求"));
        }
        let image = cx
            .capture_screenshot(window.into())
            .map_err(|error| rejected(&error.to_string()))?;
        let (width, height) = image.dimensions();
        let rgba = image.as_raw();
        check_pixels(width, height, scale, rgba)?;
        store_new(&output, &format!("capture-{sample}.rgba"), rgba)?;
        store_new(
            &output,
            &format!("capture-{sample}.ppm"),
            &ppm(width, height, rgba),
        )?;
        records.push_str(&format!("kind=capture\tsample={sample}\tlogical_width={LOGICAL_WIDTH}\tlogical_height={LOGICAL_HEIGHT}\tscale={scale}\tpixel_width={width}\tpixel_height={height}\tbytes={}\n", rgba.len()));
        if let Some(previous) = previous.as_ref()
            && previous.as_slice() != rgba.as_slice()
        {
            store_new(&output, "capture.tsv", records.as_bytes())?;
            return Err(rejected("连续两次静态场景纹理不同"));
        }
        previous = Some(rgba.clone());
    }
    records.push_str("kind=done\tstable_repeated_capture=true\n");
    store_new(&output, "capture.tsv", records.as_bytes())?;
    cx.update_window(window.into(), |_, window, _| window.remove_window())
        .map_err(|error| rejected(&error.to_string()))?;
    cx.run_until_parked();
    Ok(())
}
