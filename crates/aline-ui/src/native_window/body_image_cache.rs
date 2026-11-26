//! 正文图片渲染缓存：入口解析只做一次，`data:` 字节只解码一次。
//!
//! 未注入预算时缓存恒为空，渲染层只能显示替代文字。远程目标与读取失败按
//! “不可渲染”缓存，避免每帧重复分类；缓存按固定容量淘汰最早条目。

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Image, ImageFormat};

use crate::body_image::{BodyImage, BodyImageSource, resolve_body_image};
use crate::media::{ImageDimensions, MediaBudget, MediaFormat};

/// 渲染缓存容量；只保存入口结果与已解码的 `data:` 图片。
const CACHE_CAPACITY: usize = 64;

/// 渲染层可直接使用的来源。
pub(super) enum RenderableSource {
    /// 本地栅格文件；交给 GPUI 资源加载器，保留动图。
    Local(PathBuf),
    /// 已解码的 `data:` 图片或净化 SVG；同一实例跨帧复用。
    Data(Arc<Image>),
}

/// 通过入口校验、可直接交给 `img()` 的图片。
pub(super) struct RenderableImage {
    source: RenderableSource,
    dimensions: ImageDimensions,
}

impl RenderableImage {
    fn new(image: BodyImage) -> Self {
        let dimensions = image.dimensions();
        let source = match image.source() {
            BodyImageSource::Local(path) => RenderableSource::Local(path.clone()),
            BodyImageSource::Bytes(bytes) => RenderableSource::Data(Arc::new(Image::from_bytes(
                image_format(image.format()),
                bytes.clone(),
            ))),
        };
        Self { source, dimensions }
    }

    pub(super) fn source(&self) -> &RenderableSource {
        &self.source
    }

    pub(super) const fn dimensions(&self) -> ImageDimensions {
        self.dimensions
    }
}

/// 按目标字符串缓存入口结果；`None` 表示该目标不可渲染。
pub(super) struct BodyImageCache {
    budget: Option<MediaBudget>,
    entries: HashMap<String, Option<Rc<RenderableImage>>>,
    order: VecDeque<String>,
}

impl BodyImageCache {
    pub(super) fn new(budget: Option<MediaBudget>) -> Self {
        Self {
            budget,
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    /// 解析一个正文图片目标；未注入预算或入口拒绝时返回 `None`。
    pub(super) fn resolve(
        &mut self,
        destination: &str,
        document_dir: &Path,
    ) -> Option<Rc<RenderableImage>> {
        self.budget?;
        if let Some(entry) = self.entries.get(destination) {
            return entry.clone();
        }
        let entry = resolve_body_image(destination, document_dir, self.budget)
            .ok()
            .map(|image| Rc::new(RenderableImage::new(image)));
        self.insert(destination.to_owned(), entry.clone());
        entry
    }

    fn insert(&mut self, key: String, value: Option<Rc<RenderableImage>>) {
        if self.entries.len() >= CACHE_CAPACITY
            && let Some(oldest) = self.order.pop_front()
        {
            self.entries.remove(&oldest);
        }
        self.order.push_back(key.clone());
        self.entries.insert(key, value);
    }
}

/// 把图片固有尺寸等比缩进正文可用宽度；小于列宽时保持原尺寸，不放大。
pub(super) fn fitted_size(dimensions: ImageDimensions, max_width: f32) -> (f32, f32) {
    let width = dimensions.width() as f32;
    let height = dimensions.height() as f32;
    let max_width = max_width.max(1.0);
    if width <= max_width {
        return (width, height);
    }
    (max_width, height * (max_width / width))
}

fn image_format(format: MediaFormat) -> ImageFormat {
    match format {
        MediaFormat::Png => ImageFormat::Png,
        MediaFormat::Jpeg => ImageFormat::Jpeg,
        MediaFormat::Gif => ImageFormat::Gif,
        MediaFormat::Webp => ImageFormat::Webp,
        MediaFormat::Bmp => ImageFormat::Bmp,
        MediaFormat::Svg => ImageFormat::Svg,
    }
}

#[cfg(test)]
mod tests {
    use std::error::Error;
    use std::path::{Path, PathBuf};

    use super::{BodyImageCache, RenderableSource};
    use crate::media::{ImageDimensions, MediaBudget};

    fn samples() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fixtures/m4-v03/samples")
    }

    fn budget() -> MediaBudget {
        MediaBudget::new(1 << 20, 1 << 24, 1 << 28, 8)
    }

    #[test]
    fn cache_without_budget_never_resolves() {
        let mut cache = BodyImageCache::new(None);
        assert!(cache.resolve("plain.png", &samples()).is_none());
    }

    #[test]
    fn remote_and_missing_targets_resolve_to_none() {
        let mut cache = BodyImageCache::new(Some(budget()));
        assert!(
            cache
                .resolve("https://example.test/a.png", &samples())
                .is_none()
        );
        assert!(cache.resolve("//example.test/a.png", &samples()).is_none());
        assert!(cache.resolve("missing.png", &samples()).is_none());
    }

    #[test]
    fn images_fit_the_text_column_without_upscaling() {
        let (width, height) = super::fitted_size(ImageDimensions::new(1000, 500), 400.0);
        assert!((width - 400.0).abs() < 0.01, "{width}");
        assert!((height - 200.0).abs() < 0.01, "{height}");

        let (width, height) = super::fitted_size(ImageDimensions::new(2, 2), 400.0);
        assert!((width - 2.0).abs() < 0.01, "{width}");
        assert!((height - 2.0).abs() < 0.01, "{height}");

        let (width, height) = super::fitted_size(ImageDimensions::new(300, 600), 600.0);
        assert!((width - 300.0).abs() < 0.01, "{width}");
        assert!((height - 600.0).abs() < 0.01, "{height}");
    }

    #[test]
    fn local_raster_keeps_its_path_and_data_images_decode_once() -> Result<(), Box<dyn Error>> {
        let mut cache = BodyImageCache::new(Some(budget()));
        let raster = cache.resolve("plain.png", &samples()).ok_or("png")?;
        assert_eq!(raster.dimensions(), ImageDimensions::new(2, 2));
        assert!(matches!(raster.source(), RenderableSource::Local(_)));

        let again = cache.resolve("plain.png", &samples()).ok_or("png 缓存")?;
        assert!(std::rc::Rc::ptr_eq(&raster, &again));

        let svg = cache.resolve("plain.svg", &samples()).ok_or("svg")?;
        assert!(matches!(svg.source(), RenderableSource::Data(_)));
        Ok(())
    }
}
