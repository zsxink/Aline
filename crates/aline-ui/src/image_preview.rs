//! 图片预览的纯视口模型：适应、1:1、缩放、平移与百分比。
//!
//! 只处理数值，不接触 GPUI、文件系统或插件门控；窗口层负责传入当前视口
//! 尺寸并把结果映射为布局。缩放按整数百分比显示，步进 1.25 倍，范围
//! 10%–800%（UI §9、PREVIEW-02）。

use std::path::{Path, PathBuf};
use std::time::SystemTime;

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

/// 预览资源的文件版本：长度与修改时间。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileVersion {
    /// 文件长度（字节）。
    pub length: u64,
    /// 修改时间；平台不提供时为 `None`。
    pub modified: Option<SystemTime>,
}

impl FileVersion {
    /// 构造文件版本快照。
    pub const fn new(length: u64, modified: Option<SystemTime>) -> Self {
        Self { length, modified }
    }
}

/// 读取文件版本；文件不可读时返回 `None`。
pub fn file_version(path: &Path) -> Option<FileVersion> {
    let metadata = std::fs::metadata(path).ok()?;
    Some(FileVersion::new(metadata.len(), metadata.modified().ok()))
}

/// 预览失败类别；窗口层负责映射为用户可见说明。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewFailure {
    /// 未注入图片预算。
    NoBudget,
    /// 签名、头或载荷不是受支持图片。
    Unsupported,
    /// 超出已配置预算。
    TooLarge,
    /// 文件读取失败。
    Io,
    /// 再启用时资源已变化。
    Changed,
}

/// 预览槽的当前阶段。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewPhase {
    /// 没有预览。
    Closed,
    /// 正在读取/校验图片。
    Loading,
    /// 图片可显示。
    Ready,
    /// 加载失败；保留路径。
    Failed,
    /// 插件关闭；保留路径与视口。
    Disabled,
}

/// 插件重新启用后的结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PreviewEnable {
    /// 没有可恢复的预览。
    Idle,
    /// 已开始重新加载；携带请求编号。
    Loading(u64),
    /// 恢复为失败/资源变化状态；保留路径，等待重试。
    Failed(PreviewFailure),
}

struct PreviewLoad {
    request: u64,
    path: PathBuf,
    version: FileVersion,
    generation: u64,
    viewport: PreviewViewport,
    restore: Option<PreviewView>,
}

struct PreviewReady {
    path: PathBuf,
    version: FileVersion,
    view: PreviewView,
}

struct PreviewFailed {
    path: PathBuf,
    failure: PreviewFailure,
    version: Option<FileVersion>,
}

struct PreviewDisabled {
    path: PathBuf,
    version: Option<FileVersion>,
    view: Option<PreviewView>,
    failure: Option<PreviewFailure>,
}

enum PreviewState {
    Closed,
    Loading(PreviewLoad),
    Ready(PreviewReady),
    Failed(PreviewFailed),
    Disabled(PreviewDisabled),
}

/// 单槽图片预览：路径复用、加载结果校验、插件开关与再启用恢复。
///
/// 槽只保存状态与视图，不读文件也不解码；窗口层在后台完成读取后把
/// 结果连同当前插件代次交回，由槽拒绝旧请求与已变化的资源。
pub struct PreviewSlot {
    state: PreviewState,
    next_request: u64,
}

impl PreviewSlot {
    /// 空槽。
    pub fn new() -> Self {
        Self {
            state: PreviewState::Closed,
            next_request: 1,
        }
    }

    /// 当前阶段。
    pub fn phase(&self) -> PreviewPhase {
        match &self.state {
            PreviewState::Closed => PreviewPhase::Closed,
            PreviewState::Loading(_) => PreviewPhase::Loading,
            PreviewState::Ready(_) => PreviewPhase::Ready,
            PreviewState::Failed(_) => PreviewPhase::Failed,
            PreviewState::Disabled(_) => PreviewPhase::Disabled,
        }
    }

    /// 当前图片路径；关闭后为 `None`。
    pub fn path(&self) -> Option<&Path> {
        match &self.state {
            PreviewState::Closed => None,
            PreviewState::Loading(load) => Some(load.path.as_path()),
            PreviewState::Ready(ready) => Some(ready.path.as_path()),
            PreviewState::Failed(failed) => Some(failed.path.as_path()),
            PreviewState::Disabled(disabled) => Some(disabled.path.as_path()),
        }
    }

    /// 当前视图；失败后为 `None`，关闭期间保留原视图。
    pub fn view(&self) -> Option<&PreviewView> {
        match &self.state {
            PreviewState::Ready(ready) => Some(&ready.view),
            PreviewState::Disabled(disabled) => disabled.view.as_ref(),
            _ => None,
        }
    }

    /// 当前失败类别；非失败状态为 `None`。
    pub fn failure(&self) -> Option<PreviewFailure> {
        match &self.state {
            PreviewState::Failed(failed) => Some(failed.failure),
            PreviewState::Disabled(disabled) => disabled.failure,
            _ => None,
        }
    }

    /// 打开图片：同路径复用现有状态；不同路径开始新加载并返回请求编号。
    pub fn open(
        &mut self,
        path: PathBuf,
        version: FileVersion,
        generation: u64,
        viewport: PreviewViewport,
    ) -> Option<u64> {
        if self.path().is_some_and(|current| current == path) {
            return None;
        }
        let request = self.take_request();
        self.state = PreviewState::Loading(PreviewLoad {
            request,
            path,
            version,
            generation,
            viewport,
            restore: None,
        });
        Some(request)
    }

    /// 采纳加载结果；旧请求、旧插件代次或资源变化都返回 `false`。
    pub fn accept_loaded(
        &mut self,
        request: u64,
        generation: u64,
        image: ImageSize,
        version: FileVersion,
    ) -> bool {
        let state = std::mem::replace(&mut self.state, PreviewState::Closed);
        match state {
            PreviewState::Loading(load) => {
                if load.request != request || load.generation != generation {
                    self.state = PreviewState::Loading(load);
                    return false;
                }
                if load.version != version {
                    self.state = PreviewState::Failed(PreviewFailed {
                        path: load.path,
                        failure: PreviewFailure::Changed,
                        version: Some(version),
                    });
                    return false;
                }
                let view = match load.restore {
                    Some(mut saved) if saved.image() == image => {
                        saved.set_viewport(load.viewport);
                        saved
                    }
                    _ => PreviewView::new(image, load.viewport),
                };
                self.state = PreviewState::Ready(PreviewReady {
                    path: load.path,
                    version,
                    view,
                });
                true
            }
            other => {
                self.state = other;
                false
            }
        }
    }

    /// 采纳失败结果；只有当前请求生效。
    pub fn accept_failed(&mut self, request: u64, failure: PreviewFailure) -> bool {
        let state = std::mem::replace(&mut self.state, PreviewState::Closed);
        match state {
            PreviewState::Loading(load) if load.request == request => {
                self.state = PreviewState::Failed(PreviewFailed {
                    path: load.path,
                    failure,
                    version: Some(load.version),
                });
                true
            }
            other => {
                self.state = other;
                false
            }
        }
    }

    /// 插件关闭：保留路径与视图（含失败类别），丢弃加载中的请求。
    pub fn disable(&mut self) {
        let state = std::mem::replace(&mut self.state, PreviewState::Closed);
        self.state = match state {
            PreviewState::Closed => PreviewState::Closed,
            PreviewState::Loading(load) => PreviewState::Disabled(PreviewDisabled {
                path: load.path,
                version: Some(load.version),
                view: load.restore,
                failure: None,
            }),
            PreviewState::Ready(ready) => PreviewState::Disabled(PreviewDisabled {
                path: ready.path,
                version: Some(ready.version),
                view: Some(ready.view),
                failure: None,
            }),
            PreviewState::Failed(failed) => PreviewState::Disabled(PreviewDisabled {
                path: failed.path,
                version: failed.version,
                view: None,
                failure: Some(failed.failure),
            }),
            PreviewState::Disabled(disabled) => PreviewState::Disabled(disabled),
        };
    }

    /// 插件重新启用：按当前文件版本决定重新加载或报告资源变化。
    pub fn enable(
        &mut self,
        version: Option<FileVersion>,
        generation: u64,
        viewport: PreviewViewport,
    ) -> PreviewEnable {
        let state = std::mem::replace(&mut self.state, PreviewState::Closed);
        let PreviewState::Disabled(disabled) = state else {
            self.state = state;
            return PreviewEnable::Idle;
        };
        let unchanged = match (disabled.version, version) {
            (Some(expected), Some(current)) => expected == current,
            (None, None) => true,
            _ => false,
        };
        if !unchanged {
            self.state = PreviewState::Failed(PreviewFailed {
                path: disabled.path,
                failure: PreviewFailure::Changed,
                version,
            });
            return PreviewEnable::Failed(PreviewFailure::Changed);
        }
        if let Some(failure) = disabled.failure {
            self.state = PreviewState::Failed(PreviewFailed {
                path: disabled.path,
                failure,
                version,
            });
            return PreviewEnable::Failed(failure);
        }
        let Some(version) = version else {
            self.state = PreviewState::Failed(PreviewFailed {
                path: disabled.path,
                failure: PreviewFailure::Io,
                version: None,
            });
            return PreviewEnable::Failed(PreviewFailure::Io);
        };
        let request = self.take_request();
        let restore = disabled.view.map(|mut view| {
            view.set_viewport(viewport);
            view
        });
        self.state = PreviewState::Loading(PreviewLoad {
            request,
            path: disabled.path,
            version,
            generation,
            viewport,
            restore,
        });
        PreviewEnable::Loading(request)
    }

    /// 失败后重试：按当前版本重新加载。
    pub fn retry(
        &mut self,
        version: FileVersion,
        generation: u64,
        viewport: PreviewViewport,
    ) -> Option<u64> {
        let state = std::mem::replace(&mut self.state, PreviewState::Closed);
        match state {
            PreviewState::Failed(failed) => {
                let request = self.take_request();
                self.state = PreviewState::Loading(PreviewLoad {
                    request,
                    path: failed.path,
                    version,
                    generation,
                    viewport,
                    restore: None,
                });
                Some(request)
            }
            other => {
                self.state = other;
                None
            }
        }
    }

    /// 关闭预览；迟到的加载结果随之作废。
    pub fn close(&mut self) {
        self.state = PreviewState::Closed;
    }

    /// 视口变化：就绪时重算适应/夹取平移，加载中记录最新视口。
    pub fn set_viewport(&mut self, viewport: PreviewViewport) {
        match &mut self.state {
            PreviewState::Ready(ready) => ready.view.set_viewport(viewport),
            PreviewState::Loading(load) => load.viewport = viewport,
            _ => {}
        }
    }

    /// 放大一步；非就绪状态返回 `None`。
    pub fn zoom_in(&mut self) -> Option<ZoomOutcome> {
        match &mut self.state {
            PreviewState::Ready(ready) => Some(ready.view.zoom_in()),
            _ => None,
        }
    }

    /// 缩小一步；非就绪状态返回 `None`。
    pub fn zoom_out(&mut self) -> Option<ZoomOutcome> {
        match &mut self.state {
            PreviewState::Ready(ready) => Some(ready.view.zoom_out()),
            _ => None,
        }
    }

    /// 回到适应模式。
    pub fn fit(&mut self) {
        if let PreviewState::Ready(ready) = &mut self.state {
            ready.view.fit();
        }
    }

    /// 切到 1:1。
    pub fn actual_size(&mut self) {
        if let PreviewState::Ready(ready) = &mut self.state {
            ready.view.actual_size();
        }
    }

    /// 平移给定偏移；返回是否实际变化。
    pub fn pan_by(&mut self, dx: f32, dy: f32) -> bool {
        match &mut self.state {
            PreviewState::Ready(ready) => ready.view.pan_by(dx, dy),
            _ => false,
        }
    }

    fn take_request(&mut self) -> u64 {
        let request = self.next_request;
        self.next_request = self.next_request.saturating_add(1);
        request
    }
}

impl Default for PreviewSlot {
    fn default() -> Self {
        Self::new()
    }
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

    // ---- 预览槽：加载/复用/关闭/插件开关/资源变化/旧结果 ----

    fn version(length: u64) -> FileVersion {
        FileVersion::new(length, None)
    }

    fn viewport() -> PreviewViewport {
        PreviewViewport::new(800.0, 600.0)
    }

    fn size(width: u32, height: u32) -> Result<ImageSize, Box<dyn Error>> {
        Ok(ImageSize::new(width, height).ok_or("测试图片尺寸必须非零")?)
    }

    fn loaded_slot(width: u32, height: u32) -> Result<PreviewSlot, Box<dyn Error>> {
        let mut slot = PreviewSlot::new();
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("首次打开应开始加载")?;
        assert!(slot.accept_loaded(request, 7, size(width, height)?, version(1)));
        Ok(slot)
    }

    #[test]
    fn open_starts_loading_and_completion_makes_it_ready() -> TestResult {
        let mut slot = PreviewSlot::new();
        assert_eq!(slot.phase(), PreviewPhase::Closed);
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("首次打开应开始加载")?;
        assert_eq!(slot.phase(), PreviewPhase::Loading);
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));

        assert!(slot.accept_loaded(request, 7, size(100, 50)?, version(1)));
        assert_eq!(slot.phase(), PreviewPhase::Ready);
        assert_eq!(slot.view().ok_or("缺少视图")?.percent(), 100);
        Ok(())
    }

    #[test]
    fn same_path_reuses_the_ready_preview_and_keeps_the_view() -> TestResult {
        let mut slot = loaded_slot(1600, 1200)?;
        slot.actual_size();
        assert!(slot.zoom_in().ok_or("缩放")?.changed);
        assert_eq!(slot.view().ok_or("视图")?.percent(), 125);

        assert!(
            slot.open(PathBuf::from("/tmp/a.png"), version(9), 8, viewport())
                .is_none(),
            "同路径必须复用，不重新加载"
        );
        assert_eq!(slot.phase(), PreviewPhase::Ready);
        assert_eq!(slot.view().ok_or("视图")?.percent(), 125);
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));
        Ok(())
    }

    #[test]
    fn different_path_replaces_the_slot_and_rejects_the_old_result() -> TestResult {
        let mut slot = PreviewSlot::new();
        let first = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("首次加载")?;
        let second = slot
            .open(PathBuf::from("/tmp/b.png"), version(1), 7, viewport())
            .ok_or("第二次加载")?;
        assert_ne!(first, second);

        assert!(!slot.accept_loaded(first, 7, size(10, 10)?, version(1)));
        assert_eq!(slot.phase(), PreviewPhase::Loading);
        assert_eq!(slot.path(), Some(Path::new("/tmp/b.png")));

        assert!(slot.accept_loaded(second, 7, size(10, 10)?, version(1)));
        assert_eq!(slot.phase(), PreviewPhase::Ready);
        Ok(())
    }

    #[test]
    fn close_forgets_the_path_and_rejects_in_flight_results() -> TestResult {
        let mut slot = PreviewSlot::new();
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("加载")?;
        slot.close();
        assert_eq!(slot.phase(), PreviewPhase::Closed);
        assert_eq!(slot.path(), None);
        assert!(!slot.accept_loaded(request, 7, size(10, 10)?, version(1)));
        assert!(!slot.accept_failed(request, PreviewFailure::Io));
        Ok(())
    }

    #[test]
    fn stale_plugin_generation_is_rejected() -> TestResult {
        let mut slot = PreviewSlot::new();
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("加载")?;
        assert!(!slot.accept_loaded(request, 8, size(10, 10)?, version(1)));
        assert_eq!(slot.phase(), PreviewPhase::Loading);
        Ok(())
    }

    #[test]
    fn resource_change_at_completion_fails_and_keeps_the_path() -> TestResult {
        let mut slot = PreviewSlot::new();
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("加载")?;
        assert!(!slot.accept_loaded(request, 7, size(10, 10)?, version(2)));
        assert_eq!(slot.phase(), PreviewPhase::Failed);
        assert_eq!(slot.failure(), Some(PreviewFailure::Changed));
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));
        Ok(())
    }

    #[test]
    fn disable_keeps_path_and_manual_view_and_enable_restores_it() -> TestResult {
        let mut slot = loaded_slot(1600, 1200)?;
        slot.actual_size();
        assert!(slot.zoom_in().ok_or("缩放")?.changed);
        assert!(slot.pan_by(100.0, 100.0));

        slot.disable();
        assert_eq!(slot.phase(), PreviewPhase::Disabled);
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));
        let saved = slot.view().ok_or("关闭期间应保留视图")?;
        assert_eq!(saved.mode(), PreviewMode::Manual);
        assert_eq!(saved.percent(), 125);

        let request = match slot.enable(Some(version(1)), 9, PreviewViewport::new(400.0, 300.0)) {
            PreviewEnable::Loading(request) => request,
            other => return Err(format!("期望重新加载，实际 {other:?}").into()),
        };
        assert!(slot.accept_loaded(request, 9, size(1600, 1200)?, version(1)));
        let view = slot.view().ok_or("视图")?;
        assert_eq!(view.mode(), PreviewMode::Manual);
        assert_eq!(view.percent(), 125);
        assert_close(view.pan().x(), 100.0);
        assert_close(view.pan().y(), 100.0);
        Ok(())
    }

    #[test]
    fn enable_recomputes_fit_for_the_current_viewport() -> TestResult {
        let mut slot = loaded_slot(1600, 1200)?;
        assert_eq!(slot.view().ok_or("视图")?.percent(), 50);
        slot.disable();

        let request = match slot.enable(Some(version(1)), 9, PreviewViewport::new(400.0, 300.0)) {
            PreviewEnable::Loading(request) => request,
            other => return Err(format!("期望重新加载，实际 {other:?}").into()),
        };
        assert!(slot.accept_loaded(request, 9, size(1600, 1200)?, version(1)));
        assert_eq!(slot.view().ok_or("视图")?.mode(), PreviewMode::Fit);
        assert_eq!(slot.view().ok_or("视图")?.percent(), 25);
        Ok(())
    }

    #[test]
    fn enable_after_resource_change_reports_changed_and_keeps_the_path() -> TestResult {
        let mut slot = loaded_slot(1600, 1200)?;
        slot.disable();
        assert_eq!(
            slot.enable(Some(version(2)), 9, viewport()),
            PreviewEnable::Failed(PreviewFailure::Changed)
        );
        assert_eq!(slot.phase(), PreviewPhase::Failed);
        assert_eq!(slot.failure(), Some(PreviewFailure::Changed));
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));
        Ok(())
    }

    #[test]
    fn failure_keeps_the_path_and_retry_loads_again() -> TestResult {
        let mut slot = PreviewSlot::new();
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("加载")?;
        assert!(slot.accept_failed(request, PreviewFailure::Unsupported));
        assert_eq!(slot.phase(), PreviewPhase::Failed);
        assert_eq!(slot.failure(), Some(PreviewFailure::Unsupported));
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));

        let retry = slot
            .retry(version(3), 7, viewport())
            .ok_or("失败后应可重试")?;
        assert_eq!(slot.phase(), PreviewPhase::Loading);
        assert!(slot.accept_loaded(retry, 7, size(10, 10)?, version(3)));
        assert_eq!(slot.phase(), PreviewPhase::Ready);
        Ok(())
    }

    #[test]
    fn disable_while_loading_cancels_and_enable_starts_a_fresh_load() -> TestResult {
        let mut slot = PreviewSlot::new();
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("加载")?;
        slot.disable();
        assert_eq!(slot.phase(), PreviewPhase::Disabled);
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));
        assert!(slot.view().is_none());

        let fresh = match slot.enable(Some(version(1)), 9, viewport()) {
            PreviewEnable::Loading(request) => request,
            other => return Err(format!("期望重新加载，实际 {other:?}").into()),
        };
        assert_ne!(request, fresh);
        assert!(!slot.accept_loaded(request, 7, size(10, 10)?, version(1)));
        assert!(slot.accept_loaded(fresh, 9, size(10, 10)?, version(1)));
        Ok(())
    }

    #[test]
    fn failed_state_survives_re_enable_with_an_unchanged_version() -> TestResult {
        let mut slot = PreviewSlot::new();
        let request = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("加载")?;
        assert!(slot.accept_failed(request, PreviewFailure::Io));
        slot.disable();
        assert_eq!(
            slot.enable(Some(version(1)), 9, viewport()),
            PreviewEnable::Failed(PreviewFailure::Io)
        );
        assert_eq!(slot.phase(), PreviewPhase::Failed);
        assert_eq!(slot.path(), Some(Path::new("/tmp/a.png")));
        Ok(())
    }

    #[test]
    fn viewport_changes_apply_while_ready() -> TestResult {
        let mut slot = loaded_slot(1600, 1200)?;
        assert_eq!(slot.view().ok_or("视图")?.percent(), 50);
        slot.set_viewport(PreviewViewport::new(400.0, 300.0));
        assert_eq!(slot.view().ok_or("视图")?.percent(), 25);
        Ok(())
    }

    #[test]
    fn zoom_and_pan_require_a_ready_preview() -> TestResult {
        let mut slot = PreviewSlot::new();
        let _ = slot
            .open(PathBuf::from("/tmp/a.png"), version(1), 7, viewport())
            .ok_or("加载")?;
        assert!(slot.zoom_in().is_none());
        assert!(slot.zoom_out().is_none());
        assert!(!slot.pan_by(10.0, 10.0));
        Ok(())
    }

    #[test]
    fn file_version_tracks_length_changes() -> TestResult {
        let directory = std::env::temp_dir().join(format!(
            "aline-preview-version-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)?
                .as_nanos()
        ));
        std::fs::create_dir(&directory)?;
        let path = directory.join("a.png");
        std::fs::write(&path, b"a")?;
        let first = file_version(&path).ok_or("应有文件版本")?;
        std::fs::write(&path, b"ab")?;
        let second = file_version(&path).ok_or("应有文件版本")?;
        assert_ne!(first, second);
        std::fs::remove_dir_all(&directory)?;
        Ok(())
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
