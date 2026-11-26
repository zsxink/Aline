//! 图片预览的纯视口模型：适应、1:1、缩放、平移与百分比。
//!
//! 只处理数值，不接触 GPUI、文件系统或插件门控；窗口层负责传入当前视口
//! 尺寸并把结果映射为布局。缩放按整数百分比显示，步进 1.25 倍，范围
//! 10%–800%（UI §9、PREVIEW-02）。

use std::path::Path;

/// 缩放百分比下限（PREVIEW-02）。
pub const MIN_PERCENT: u32 = 10;
/// 缩放百分比上限（PREVIEW-02）。
pub const MAX_PERCENT: u32 = 800;

const ZOOM_NUMERATOR: u32 = 5;
const ZOOM_DENOMINATOR: u32 = 4;

/// 预览视口尺寸（逻辑点）；非有限或小于 1 的值按 1 处理。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewViewport {
    width: f32,
    height: f32,
}

impl PreviewViewport {
    /// 由窗口内容区尺寸构造；非法尺寸收敛为 1 点，避免除零与负平移边界。
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width: sanitize_extent(width),
            height: sanitize_extent(height),
        }
    }

    /// 视口宽度（点）。
    pub fn width(&self) -> f32 {
        self.width
    }

    /// 视口高度（点）。
    pub fn height(&self) -> f32 {
        self.height
    }
}

fn sanitize_extent(value: f32) -> f32 {
    if value.is_finite() && value >= 1.0 {
        value
    } else {
        1.0
    }
}

/// 图片像素尺寸；零尺寸不构成可预览图片。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImageSize {
    width: u32,
    height: u32,
}

impl ImageSize {
    /// 仅接受宽高均非零的尺寸。
    pub fn new(width: u32, height: u32) -> Option<Self> {
        (width > 0 && height > 0).then_some(Self { width, height })
    }

    /// 图片宽度（像素）。
    pub fn width(&self) -> u32 {
        self.width
    }

    /// 图片高度（像素）。
    pub fn height(&self) -> u32 {
        self.height
    }
}

/// 显示模式：适应视口或手动百分比。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PreviewMode {
    /// 按视口计算比例，且不放大原图。
    Fit,
    /// 按整数百分比显示。
    Manual,
}

/// 平移偏移（点，相对视口中心）。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewPan {
    x: f32,
    y: f32,
}

impl PreviewPan {
    /// 由水平/垂直偏移构造。
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    /// 水平偏移。
    pub fn x(&self) -> f32 {
        self.x
    }

    /// 垂直偏移。
    pub fn y(&self) -> f32 {
        self.y
    }
}

impl Default for PreviewPan {
    fn default() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

/// 缩放命令结果：是否变化、是否已在边界。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoomOutcome {
    /// 百分比或模式是否发生变化。
    pub changed: bool,
    /// 是否因到达 10%/800% 边界而未变化；界面据此说明不能继续。
    pub at_limit: bool,
}

/// 单张图片的视口状态：模式、百分比与平移。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PreviewView {
    image: ImageSize,
    viewport: PreviewViewport,
    mode: PreviewMode,
    percent: u32,
    pan: PreviewPan,
}

impl PreviewView {
    /// 初始为适应模式且平移归零。
    pub fn new(image: ImageSize, viewport: PreviewViewport) -> Self {
        Self {
            image,
            viewport,
            mode: PreviewMode::Fit,
            percent: 100,
            pan: PreviewPan::default(),
        }
    }

    /// 当前显示模式。
    pub fn mode(&self) -> PreviewMode {
        self.mode
    }

    /// 图片像素尺寸。
    pub fn image(&self) -> ImageSize {
        self.image
    }

    /// 当前视口尺寸。
    pub fn viewport(&self) -> PreviewViewport {
        self.viewport
    }

    /// 当前平移偏移。
    pub fn pan(&self) -> PreviewPan {
        self.pan
    }

    /// 当前缩放比例；1.0 为 100%。
    pub fn scale(&self) -> f32 {
        match self.mode {
            PreviewMode::Fit => self.fit_scale(),
            PreviewMode::Manual => {
                f32::from(u16::try_from(self.percent).unwrap_or(u16::MAX)) / 100.0
            }
        }
    }

    /// 当前显示百分比；适应模式按实际比例四舍五入。
    pub fn percent(&self) -> u32 {
        match self.mode {
            PreviewMode::Fit => percent_from_scale(self.fit_scale()),
            PreviewMode::Manual => self.percent,
        }
    }

    /// 视口变化：适应模式自然重算，手动模式按新边界夹取平移。
    pub fn set_viewport(&mut self, viewport: PreviewViewport) {
        self.viewport = viewport;
        self.clamp_pan();
    }

    /// 回到适应模式并重置平移。
    pub fn fit(&mut self) {
        self.mode = PreviewMode::Fit;
        self.pan = PreviewPan::default();
    }

    /// 切到 1:1（手动 100%）并重置平移。
    pub fn actual_size(&mut self) {
        self.mode = PreviewMode::Manual;
        self.percent = 100;
        self.pan = PreviewPan::default();
    }

    /// 放大一步（×1.25），返回是否变化与是否到达上限。
    pub fn zoom_in(&mut self) -> ZoomOutcome {
        self.zoom(ZOOM_NUMERATOR, ZOOM_DENOMINATOR)
    }

    /// 缩小一步（÷1.25），返回是否变化与是否到达下限。
    pub fn zoom_out(&mut self) -> ZoomOutcome {
        self.zoom(ZOOM_DENOMINATOR, ZOOM_NUMERATOR)
    }

    /// 平移给定偏移（点）；返回是否实际发生变化。
    pub fn pan_by(&mut self, dx: f32, dy: f32) -> bool {
        if !dx.is_finite() || !dy.is_finite() {
            return false;
        }
        let before = self.pan;
        self.pan.x += dx;
        self.pan.y += dy;
        self.clamp_pan();
        self.pan != before
    }

    fn fit_scale(&self) -> f32 {
        let width = self.image.width as f32;
        let height = self.image.height as f32;
        (self.viewport.width / width)
            .min(self.viewport.height / height)
            .min(1.0)
    }

    fn zoom(&mut self, numerator: u32, denominator: u32) -> ZoomOutcome {
        let current = self.percent();
        if numerator < denominator && current <= MIN_PERCENT {
            return ZoomOutcome {
                changed: false,
                at_limit: true,
            };
        }
        let stepped = u64::from(current) * u64::from(numerator) / u64::from(denominator);
        let next = u32::try_from(stepped)
            .unwrap_or(MAX_PERCENT)
            .clamp(MIN_PERCENT, MAX_PERCENT);
        if next == current {
            return ZoomOutcome {
                changed: false,
                at_limit: true,
            };
        }
        self.mode = PreviewMode::Manual;
        self.percent = next;
        self.clamp_pan();
        ZoomOutcome {
            changed: true,
            at_limit: false,
        }
    }

    fn clamp_pan(&mut self) {
        let (max_x, max_y) = self.max_pan();
        self.pan.x = self.pan.x.clamp(-max_x, max_x);
        self.pan.y = self.pan.y.clamp(-max_y, max_y);
    }

    fn max_pan(&self) -> (f32, f32) {
        let scale = self.scale();
        let width = self.image.width as f32 * scale;
        let height = self.image.height as f32 * scale;
        (
            ((width - self.viewport.width) / 2.0).max(0.0),
            ((height - self.viewport.height) / 2.0).max(0.0),
        )
    }
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "比例来自有限正数，先夹到 u16 上界再取整"
)]
fn percent_from_scale(scale: f32) -> u32 {
    let percent = (scale * 100.0).round();
    if !percent.is_finite() || percent < 1.0 {
        return 1;
    }
    percent.min(f32::from(u16::MAX)) as u32
}

/// 预览入口的扩展名判定；格式白名单与 V-03 冻结的支持矩阵一致。
pub fn has_preview_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;
    use std::path::Path;

    type TestResult = Result<(), Box<dyn Error>>;

    fn view(width: u32, height: u32, viewport: (f32, f32)) -> Result<PreviewView, Box<dyn Error>> {
        let image = ImageSize::new(width, height).ok_or("测试图片尺寸必须非零")?;
        Ok(PreviewView::new(
            image,
            PreviewViewport::new(viewport.0, viewport.1),
        ))
    }

    fn assert_close(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.01,
            "actual={actual} expected={expected}"
        );
    }

    #[test]
    fn fit_does_not_upscale_small_image() -> TestResult {
        let view = view(100, 50, (800.0, 600.0))?;
        assert_eq!(view.mode(), PreviewMode::Fit);
        assert_eq!(view.percent(), 100);
        assert_close(view.scale(), 1.0);
        Ok(())
    }

    #[test]
    fn fit_scales_large_image_into_viewport() -> TestResult {
        let view = view(1600, 1200, (800.0, 600.0))?;
        assert_eq!(view.percent(), 50);
        assert_close(view.scale(), 0.5);
        Ok(())
    }

    #[test]
    fn fit_recomputes_after_viewport_change() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.set_viewport(PreviewViewport::new(400.0, 300.0));
        assert_eq!(view.mode(), PreviewMode::Fit);
        assert_eq!(view.percent(), 25);
        assert_close(view.scale(), 0.25);
        Ok(())
    }

    #[test]
    fn actual_size_switches_to_manual_hundred_percent_and_resets_pan() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        assert!(view.pan_by(100.0, 100.0));
        view.actual_size();
        assert_eq!(view.mode(), PreviewMode::Manual);
        assert_eq!(view.percent(), 100);
        assert_close(view.scale(), 1.0);
        assert_eq!(view.pan(), PreviewPan::default());
        Ok(())
    }

    #[test]
    fn fit_returns_to_fit_mode_and_resets_pan() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        assert!(view.pan_by(100.0, 100.0));
        view.fit();
        assert_eq!(view.mode(), PreviewMode::Fit);
        assert_eq!(view.percent(), 50);
        assert_eq!(view.pan(), PreviewPan::default());
        Ok(())
    }

    #[test]
    fn zoom_in_steps_by_one_and_a_quarter_from_manual_percent() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        assert!(view.zoom_in().changed);
        assert_eq!(view.percent(), 125);
        assert!(view.zoom_in().changed);
        assert_eq!(view.percent(), 156);
        assert!(view.zoom_in().changed);
        assert_eq!(view.percent(), 195);
        Ok(())
    }

    #[test]
    fn zoom_out_steps_by_one_and_a_quarter_from_manual_percent() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        assert!(view.zoom_out().changed);
        assert_eq!(view.percent(), 80);
        assert!(view.zoom_out().changed);
        assert_eq!(view.percent(), 64);
        Ok(())
    }

    #[test]
    fn zoom_in_from_fit_starts_at_fit_percent() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        assert!(view.zoom_in().changed);
        assert_eq!(view.mode(), PreviewMode::Manual);
        assert_eq!(view.percent(), 62);
        Ok(())
    }

    #[test]
    fn zoom_out_from_fit_starts_at_fit_percent() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        assert!(view.zoom_out().changed);
        assert_eq!(view.mode(), PreviewMode::Manual);
        assert_eq!(view.percent(), 40);
        Ok(())
    }

    #[test]
    fn zoom_stops_at_upper_bound_and_reports_limit() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        while view.percent() < MAX_PERCENT {
            assert!(view.zoom_in().changed);
        }
        let outcome = view.zoom_in();
        assert!(!outcome.changed);
        assert!(outcome.at_limit);
        assert_eq!(view.percent(), MAX_PERCENT);
        Ok(())
    }

    #[test]
    fn zoom_stops_at_lower_bound_and_reports_limit() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        while view.percent() > MIN_PERCENT {
            assert!(view.zoom_out().changed);
        }
        let outcome = view.zoom_out();
        assert!(!outcome.changed);
        assert!(outcome.at_limit);
        assert_eq!(view.percent(), MIN_PERCENT);
        Ok(())
    }

    #[test]
    fn zoom_out_from_fit_below_minimum_reports_limit_without_enlarging() -> TestResult {
        let mut view = view(20_000, 20_000, (800.0, 600.0))?;
        assert_eq!(view.percent(), 3);
        let outcome = view.zoom_out();
        assert!(!outcome.changed);
        assert!(outcome.at_limit);
        assert_eq!(view.mode(), PreviewMode::Fit);
        assert_eq!(view.percent(), 3);
        Ok(())
    }

    #[test]
    fn zoom_in_from_fit_below_minimum_clamps_to_minimum() -> TestResult {
        let mut view = view(20_000, 20_000, (800.0, 600.0))?;
        let outcome = view.zoom_in();
        assert!(outcome.changed);
        assert_eq!(view.mode(), PreviewMode::Manual);
        assert_eq!(view.percent(), MIN_PERCENT);
        Ok(())
    }

    #[test]
    fn pan_clamps_to_scaled_overflow() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        assert!(view.pan_by(1000.0, 1000.0));
        assert_close(view.pan().x(), 400.0);
        assert_close(view.pan().y(), 300.0);
        assert!(view.pan_by(-10_000.0, -10_000.0));
        assert_close(view.pan().x(), -400.0);
        assert_close(view.pan().y(), -300.0);
        Ok(())
    }

    #[test]
    fn pan_does_nothing_when_image_fits() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        assert!(!view.pan_by(100.0, 100.0));
        assert_eq!(view.pan(), PreviewPan::default());
        Ok(())
    }

    #[test]
    fn pan_clamps_when_zooming_out() -> TestResult {
        let mut view = view(1600, 1200, (800.0, 600.0))?;
        view.actual_size();
        assert!(view.pan_by(400.0, 300.0));
        assert!(view.zoom_out().changed);
        assert_close(view.pan().x(), 240.0);
        assert_close(view.pan().y(), 180.0);
        Ok(())
    }

    #[test]
    fn viewport_change_recomputes_fit_and_clamps_manual_pan() -> TestResult {
        let mut fitted = view(1600, 1200, (800.0, 600.0))?;
        fitted.set_viewport(PreviewViewport::new(1600.0, 1200.0));
        assert_eq!(fitted.percent(), 100);

        let mut manual = view(1600, 1200, (800.0, 600.0))?;
        manual.actual_size();
        assert!(manual.pan_by(400.0, 300.0));
        manual.set_viewport(PreviewViewport::new(1600.0, 1200.0));
        assert_eq!(manual.mode(), PreviewMode::Manual);
        assert_eq!(manual.percent(), 100);
        assert_eq!(manual.pan(), PreviewPan::default());
        Ok(())
    }

    #[test]
    fn viewport_size_is_at_least_one_point() {
        let viewport = PreviewViewport::new(0.0, -5.0);
        assert_eq!(viewport.width(), 1.0);
        assert_eq!(viewport.height(), 1.0);
    }

    #[test]
    fn image_size_rejects_zero_dimension() {
        assert!(ImageSize::new(0, 10).is_none());
        assert!(ImageSize::new(10, 0).is_none());
        assert!(ImageSize::new(10, 10).is_some());
    }

    #[test]
    fn supported_image_extensions_are_case_insensitive() {
        for name in [
            "a.PNG", "b.jpg", "c.JPEG", "d.gif", "e.WebP", "f.bmp", "g.SVG",
        ] {
            assert!(has_preview_extension(Path::new(name)), "{name}");
        }
        for name in ["a.md", "b.png.txt", "c", "d.tiff", "e.avif"] {
            assert!(!has_preview_extension(Path::new(name)), "{name}");
        }
    }
}
