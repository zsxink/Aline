//! 正文图片与剪贴板的纯策略层。
//!
//! 本模块只做分类、嗅探、入口判维、预算判定与安全 SVG 解析，不读磁盘、
//! 不发网络请求、不启动 GPUI。正文图片预算尚未由 V-06 冻结，因此
//! [`MediaBudget`] 必须由调用方显式注入，生产代码不得依赖隐式默认值。
//!
//! 图片格式以字节签名为准，扩展名不参与判定；远程来源一律拒绝，绝不
//! 交给 GPUI 的 `Resource::Uri` 路径。SVG 只允许 `data:` 内联引用，脚本、
//! `foreignObject` 与外部实体由 usvg 解析阶段丢弃或忽略。

use std::fmt;

/// SVG 在锁定 GPUI 中的固定放大倍数，与 V-03 冻结值一致。
pub const SVG_RENDER_SCALE: u32 = 2;

/// 正文图片目标的来源分类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SourceKind {
    /// `data:` 内联字节。
    DataUrl,
    /// 本地路径或 `file:` URI。
    Local,
    /// `http(s):` 或协议相对 `//host`；正文不得请求。
    Remote,
    /// 空目标、未知协议或其他不可用来源。
    Unsupported,
}

/// 按目标字符串分类来源；不做文件系统或网络访问。
pub fn classify_source(destination: &str) -> SourceKind {
    let trimmed = destination.trim();
    if trimmed.is_empty() {
        return SourceKind::Unsupported;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("data:") {
        return SourceKind::DataUrl;
    }
    if lower.starts_with("http://") || lower.starts_with("https://") || trimmed.starts_with("//") {
        return SourceKind::Remote;
    }
    if lower.starts_with("file:") {
        return SourceKind::Local;
    }
    if let Some(colon) = lower.find(':') {
        let scheme = &lower[..colon];
        let valid_scheme = !scheme.is_empty()
            && scheme.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '+' | '-' | '.')
            });
        if valid_scheme {
            // Windows 盘符 `C:` 是本地路径，不是协议。
            if scheme.len() == 1 && scheme.as_bytes()[0].is_ascii_alphabetic() {
                return SourceKind::Local;
            }
            return SourceKind::Unsupported;
        }
    }
    SourceKind::Local
}

/// 正文支持解码的图片格式；不含剪贴板才可能出现的 TIFF/ICO/PNM。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaFormat {
    /// PNG；APNG 只取首帧。
    Png,
    /// JPEG。
    Jpeg,
    /// GIF，逐帧播放。
    Gif,
    /// WebP；动画 WebP 在文档路径逐帧播放。
    Webp,
    /// BMP。
    Bmp,
    /// SVG，经安全解析后光栅化。
    Svg,
}

/// 剪贴板图片格式，顺序即读取优先级。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardImageFormat {
    /// PNG。
    Png,
    /// JPEG。
    Jpeg,
    /// WebP。
    Webp,
    /// GIF。
    Gif,
    /// SVG。
    Svg,
    /// BMP。
    Bmp,
    /// TIFF。
    Tiff,
    /// ICO。
    Ico,
    /// PNM。
    Pnm,
}

impl ClipboardImageFormat {
    const PRIORITY: [Self; 9] = [
        Self::Png,
        Self::Jpeg,
        Self::Webp,
        Self::Gif,
        Self::Svg,
        Self::Bmp,
        Self::Tiff,
        Self::Ico,
        Self::Pnm,
    ];

    /// 读取优先级，数值越小越先命中。
    pub fn priority(self) -> usize {
        Self::PRIORITY
            .iter()
            .position(|candidate| *candidate == self)
            .unwrap_or(Self::PRIORITY.len())
    }

    /// 从剪贴板 flavor 名解析格式；大小写不敏感。
    pub fn from_flavor(flavor: &str) -> Option<Self> {
        match flavor.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "jpg" | "jpeg" => Some(Self::Jpeg),
            "webp" => Some(Self::Webp),
            "gif" => Some(Self::Gif),
            "svg" => Some(Self::Svg),
            "bmp" => Some(Self::Bmp),
            "tif" | "tiff" => Some(Self::Tiff),
            "ico" => Some(Self::Ico),
            "pnm" => Some(Self::Pnm),
            _ => None,
        }
    }
}

/// 剪贴板读取路由；优先级为文件、纯文本、图片。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardRoute {
    /// 文件列表。
    Files,
    /// 纯文本。
    Text,
    /// 图片；按 [`ClipboardImageFormat::priority`] 选择。
    Image(ClipboardImageFormat),
    /// 无可用路由；HTML/RTF 单独出现时属于此类。
    None,
}

impl ClipboardRoute {
    /// 按冻结优先级从 flavor 集合推导路由；不做实际读取。
    pub fn from_flavors(flavors: &[&str]) -> Self {
        let has_files = flavors
            .iter()
            .any(|flavor| flavor.eq_ignore_ascii_case("files"));
        let has_text = flavors
            .iter()
            .any(|flavor| flavor.eq_ignore_ascii_case("text"));
        let images = flavors
            .iter()
            .filter_map(|flavor| ClipboardImageFormat::from_flavor(flavor))
            .collect::<Vec<_>>();
        Self::from_entries(has_files, has_text, &images)
    }

    /// 按冻结优先级从已分类的条目推导路由。
    ///
    /// 文件优先于文本，文本优先于图片；多张图片按
    /// [`ClipboardImageFormat::priority`] 取最靠前者。
    pub fn from_entries(has_files: bool, has_text: bool, images: &[ClipboardImageFormat]) -> Self {
        if has_files {
            return Self::Files;
        }
        if has_text {
            return Self::Text;
        }
        images
            .iter()
            .copied()
            .min_by_key(|format| format.priority())
            .map_or(Self::None, Self::Image)
    }
}

/// 剪贴板写入项；`Other` 覆盖外部路径与未知类型。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardWriteItem<'a> {
    /// 纯文本项。
    Text(&'a str),
    /// 图片项。
    Image(ClipboardImageFormat),
    /// 外部路径或其他不可直接写入的项。
    Other,
}

/// 剪贴板写入计划；对应 V-03 冻结的三类写入契约。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ClipboardWritePlan {
    /// 写入纯文本；多项时只拼接字符串项。
    Text(String),
    /// 写入单张图片，格式按声明保持。
    Image(ClipboardImageFormat),
    /// 没有可写项。
    None,
}

/// 按 V-03 冻结规则推导剪贴板写入计划。
///
/// 单项纯文本或图片直接写入；多项只拼接字符串项，图片与外部路径被丢弃；
/// 多项且没有字符串项时不写入。
pub fn plan_clipboard_write(items: &[ClipboardWriteItem<'_>]) -> ClipboardWritePlan {
    if let [item] = items {
        return match item {
            ClipboardWriteItem::Text(text) => ClipboardWritePlan::Text((*text).to_owned()),
            ClipboardWriteItem::Image(format) => ClipboardWritePlan::Image(*format),
            ClipboardWriteItem::Other => ClipboardWritePlan::None,
        };
    }
    let joined = items
        .iter()
        .filter_map(|item| match item {
            ClipboardWriteItem::Text(text) => Some(*text),
            ClipboardWriteItem::Image(_) | ClipboardWriteItem::Other => None,
        })
        .collect::<String>();
    if joined.is_empty() {
        ClipboardWritePlan::None
    } else {
        ClipboardWritePlan::Text(joined)
    }
}

/// 图片固有尺寸，单位为像素。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageDimensions {
    width: u32,
    height: u32,
}

impl ImageDimensions {
    /// 构造尺寸；零尺寸由各格式解析器拒绝。
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    /// 宽度。
    pub const fn width(self) -> u32 {
        self.width
    }

    /// 高度。
    pub const fn height(self) -> u32 {
        self.height
    }

    /// 像素总数；溢出返回 `None`。
    pub fn pixels(self) -> Option<u64> {
        u64::from(self.width).checked_mul(u64::from(self.height))
    }
}

/// 入口判维结果；尚未解码。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageInfo {
    /// 按签名判定的格式。
    pub format: MediaFormat,
    /// 固有尺寸。
    pub dimensions: ImageDimensions,
    /// 预计解码帧数；APNG 固定为 1。
    pub frames: u32,
}

/// 可注入的正文图片预算；没有生产默认值。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MediaBudget {
    max_source_bytes: u64,
    max_pixels: u64,
    max_decoded_bytes: u64,
    max_frames: u32,
}

impl MediaBudget {
    /// 显式构造预算；所有界限都由调用方决定。
    pub const fn new(
        max_source_bytes: u64,
        max_pixels: u64,
        max_decoded_bytes: u64,
        max_frames: u32,
    ) -> Self {
        Self {
            max_source_bytes,
            max_pixels,
            max_decoded_bytes,
            max_frames,
        }
    }

    /// 单张图片源字节上限。
    pub const fn max_source_bytes(self) -> u64 {
        self.max_source_bytes
    }

    /// 解码后像素总数上限。
    pub const fn max_pixels(self) -> u64 {
        self.max_pixels
    }

    /// 解码后位图字节上限（宽×高×4×帧）。
    pub const fn max_decoded_bytes(self) -> u64 {
        self.max_decoded_bytes
    }

    /// 解码帧数上限。
    pub const fn max_frames(self) -> u32 {
        self.max_frames
    }
}

/// 图片策略失败；全部在解码或光栅化之前产生。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaError {
    /// 目标或数据为空。
    EmptySource,
    /// 签名不在正文支持矩阵内。
    UnsupportedFormat,
    /// 头部截断、尺寸为零或结构非法。
    InvalidHeader,
    /// 源字节超过预算。
    SourceTooLarge {
        /// 预算上限。
        limit: u64,
        /// 实际字节数。
        actual: u64,
    },
    /// 像素总数超过预算。
    PixelLimit {
        /// 解码宽度。
        width: u32,
        /// 解码高度。
        height: u32,
        /// 预算上限。
        limit: u64,
    },
    /// 帧数超过预算。
    FrameLimit {
        /// 实际帧数。
        frames: u32,
        /// 预算上限。
        limit: u32,
    },
    /// 解码位图字节超过预算。
    DecodedLimit {
        /// 实际字节数。
        bytes: u64,
        /// 预算上限。
        limit: u64,
    },
    /// `data:` URL 结构或编码非法。
    InvalidDataUrl,
    /// SVG 解析失败。
    InvalidSvg,
}

impl fmt::Display for MediaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for MediaError {}

/// 按字节签名判定格式；扩展名不参与。
pub fn sniff_format(bytes: &[u8]) -> Option<MediaFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(MediaFormat::Png);
    }
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some(MediaFormat::Jpeg);
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return Some(MediaFormat::Gif);
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(MediaFormat::Webp);
    }
    if bytes.len() >= 26 && bytes.starts_with(b"BM") {
        return Some(MediaFormat::Bmp);
    }
    if svg_candidate(bytes) {
        return Some(MediaFormat::Svg);
    }
    None
}

fn svg_candidate(bytes: &[u8]) -> bool {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let start = bytes
        .iter()
        .position(|byte| !byte.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    let rest = &bytes[start..];
    starts_with_ascii_case_insensitive(rest, b"<svg")
        || starts_with_ascii_case_insensitive(rest, b"<?xml")
}

fn starts_with_ascii_case_insensitive(bytes: &[u8], prefix: &[u8]) -> bool {
    bytes.len() >= prefix.len()
        && bytes[..prefix.len()]
            .iter()
            .zip(prefix)
            .all(|(left, right)| left.eq_ignore_ascii_case(right))
}

/// 读取头部尺寸、帧数并在解码前执行预算判定。
pub fn probe_image(bytes: &[u8], budget: MediaBudget) -> Result<ImageInfo, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::EmptySource);
    }
    check_source(bytes.len(), budget)?;
    let format = sniff_format(bytes).ok_or(MediaError::UnsupportedFormat)?;
    let (dimensions, frames, scale) = match format {
        MediaFormat::Png => (png_dimensions(bytes)?, 1, 1),
        MediaFormat::Jpeg => (jpeg_dimensions(bytes)?, 1, 1),
        MediaFormat::Gif => {
            let (dimensions, frames) = gif_dimensions(bytes)?;
            (dimensions, frames, 1)
        }
        MediaFormat::Webp => {
            let (dimensions, frames) = webp_dimensions(bytes)?;
            (dimensions, frames, 1)
        }
        MediaFormat::Bmp => (bmp_dimensions(bytes)?, 1, 1),
        MediaFormat::Svg => (svg_dimensions(bytes)?, 1, SVG_RENDER_SCALE),
    };
    check_budget(dimensions, frames, scale, budget)?;
    Ok(ImageInfo {
        format,
        dimensions,
        frames,
    })
}

/// 解码 `data:` URL；先算长度再分配，超过预算即拒绝。
pub fn decode_data_url(destination: &str, budget: MediaBudget) -> Result<Vec<u8>, MediaError> {
    let trimmed = destination.trim();
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("data:") {
        return Err(MediaError::InvalidDataUrl);
    }
    let rest = &trimmed[5..];
    let comma = rest.find(',').ok_or(MediaError::InvalidDataUrl)?;
    let header = &rest[..comma];
    let payload = &rest[comma + 1..];
    let base64 = header
        .rsplit(';')
        .next()
        .is_some_and(|part| part.eq_ignore_ascii_case("base64"));
    let bytes = if base64 {
        decode_base64(payload, budget.max_source_bytes)?
    } else {
        percent_decode(payload, budget.max_source_bytes)?
    };
    if bytes.is_empty() {
        return Err(MediaError::EmptySource);
    }
    check_source(bytes.len(), budget)?;
    Ok(bytes)
}

/// 安全解析后的 SVG：字节已去掉字符串 href、脚本与 `foreignObject`。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SafeSvg {
    sanitized: Vec<u8>,
    dimensions: ImageDimensions,
    frames: u32,
}

impl SafeSvg {
    /// 可交给锁定 GPUI `SvgRenderer` 的净化字节。
    pub fn bytes(&self) -> &[u8] {
        &self.sanitized
    }

    /// SVG 固有尺寸。
    pub const fn dimensions(&self) -> ImageDimensions {
        self.dimensions
    }

    /// 帧数；SVG 固定为 1。
    pub const fn frames(&self) -> u32 {
        self.frames
    }
}

/// 用只放行 `data:` 的 resolver 解析 SVG，并返回净化后的字节。
///
/// 解析与序列化都不触达文件系统或网络；字符串 href 在 usvg 阶段被丢弃，
/// 脚本与 `foreignObject` 不进入输出。序列化后再交给 GPUI，GPUI 的默认
/// resolver 已没有可解析的外部 href。
pub fn sanitize_svg(bytes: &[u8], budget: MediaBudget) -> Result<SafeSvg, MediaError> {
    if bytes.is_empty() {
        return Err(MediaError::EmptySource);
    }
    check_source(bytes.len(), budget)?;
    let tree = parse_svg_tree(bytes)?;
    let dimensions = tree_dimensions(&tree)?;
    check_budget(dimensions, 1, SVG_RENDER_SCALE, budget)?;
    let sanitized = tree.to_string(&usvg::WriteOptions::default()).into_bytes();
    Ok(SafeSvg {
        sanitized,
        dimensions,
        frames: 1,
    })
}

fn parse_svg_tree(bytes: &[u8]) -> Result<usvg::Tree, MediaError> {
    let options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: usvg::ImageHrefResolver::default_data_resolver(),
            resolve_string: Box::new(|_href: &str, _options: &usvg::Options| None),
        },
        ..Default::default()
    };
    usvg::Tree::from_data(bytes, &options).map_err(|_| MediaError::InvalidSvg)
}

fn svg_dimensions(bytes: &[u8]) -> Result<ImageDimensions, MediaError> {
    let tree = parse_svg_tree(bytes)?;
    tree_dimensions(&tree)
}

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "已先检查有限正数与 u32 上界，再取整转换"
)]
fn tree_dimensions(tree: &usvg::Tree) -> Result<ImageDimensions, MediaError> {
    let size = tree.size();
    let width = size.width();
    let height = size.height();
    if !width.is_finite() || !height.is_finite() || width <= 0.0 || height <= 0.0 {
        return Err(MediaError::InvalidHeader);
    }
    let width = width.ceil();
    let height = height.ceil();
    if width > u32::MAX as f32 || height > u32::MAX as f32 {
        return Err(MediaError::InvalidHeader);
    }
    Ok(ImageDimensions::new(width as u32, height as u32))
}

fn check_source(actual: usize, budget: MediaBudget) -> Result<(), MediaError> {
    let actual = u64::try_from(actual).map_err(|_| MediaError::SourceTooLarge {
        limit: budget.max_source_bytes,
        actual: u64::MAX,
    })?;
    if actual > budget.max_source_bytes {
        return Err(MediaError::SourceTooLarge {
            limit: budget.max_source_bytes,
            actual,
        });
    }
    Ok(())
}

fn check_budget(
    dimensions: ImageDimensions,
    frames: u32,
    scale: u32,
    budget: MediaBudget,
) -> Result<(), MediaError> {
    let width = dimensions
        .width()
        .checked_mul(scale)
        .ok_or(MediaError::InvalidHeader)?;
    let height = dimensions
        .height()
        .checked_mul(scale)
        .ok_or(MediaError::InvalidHeader)?;
    let pixels = u64::from(width)
        .checked_mul(u64::from(height))
        .ok_or(MediaError::InvalidHeader)?;
    if pixels > budget.max_pixels {
        return Err(MediaError::PixelLimit {
            width,
            height,
            limit: budget.max_pixels,
        });
    }
    if frames > budget.max_frames {
        return Err(MediaError::FrameLimit {
            frames,
            limit: budget.max_frames,
        });
    }
    let bytes = pixels
        .checked_mul(4)
        .and_then(|value| value.checked_mul(u64::from(frames)))
        .ok_or(MediaError::InvalidHeader)?;
    if bytes > budget.max_decoded_bytes {
        return Err(MediaError::DecodedLimit {
            bytes,
            limit: budget.max_decoded_bytes,
        });
    }
    Ok(())
}

fn png_dimensions(bytes: &[u8]) -> Result<ImageDimensions, MediaError> {
    if bytes.get(12..16) != Some(&b"IHDR"[..]) {
        return Err(MediaError::InvalidHeader);
    }
    let width = be_u32(bytes, 16).ok_or(MediaError::InvalidHeader)?;
    let height = be_u32(bytes, 20).ok_or(MediaError::InvalidHeader)?;
    if width == 0 || height == 0 {
        return Err(MediaError::InvalidHeader);
    }
    Ok(ImageDimensions::new(width, height))
}

fn jpeg_dimensions(bytes: &[u8]) -> Result<ImageDimensions, MediaError> {
    let mut index = 2usize;
    while index + 1 < bytes.len() {
        if bytes[index] != 0xFF {
            index += 1;
            continue;
        }
        let marker = bytes[index + 1];
        index += 2;
        if marker == 0xD8 || marker == 0xD9 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if marker == 0xDA {
            break;
        }
        let length = usize::from(be_u16(bytes, index).ok_or(MediaError::InvalidHeader)?);
        if length < 2 {
            return Err(MediaError::InvalidHeader);
        }
        let end = index.checked_add(length).ok_or(MediaError::InvalidHeader)?;
        if end > bytes.len() {
            return Err(MediaError::InvalidHeader);
        }
        if is_sof_marker(marker) {
            if length < 7 {
                return Err(MediaError::InvalidHeader);
            }
            let height = be_u16(bytes, index + 3).ok_or(MediaError::InvalidHeader)?;
            let width = be_u16(bytes, index + 5).ok_or(MediaError::InvalidHeader)?;
            if width == 0 || height == 0 {
                return Err(MediaError::InvalidHeader);
            }
            return Ok(ImageDimensions::new(u32::from(width), u32::from(height)));
        }
        index = end;
    }
    Err(MediaError::InvalidHeader)
}

fn is_sof_marker(marker: u8) -> bool {
    matches!(
        marker,
        0xC0 | 0xC1 | 0xC2 | 0xC3 | 0xC5 | 0xC6 | 0xC7 | 0xC9 | 0xCA | 0xCB | 0xCD | 0xCE | 0xCF
    )
}

fn gif_dimensions(bytes: &[u8]) -> Result<(ImageDimensions, u32), MediaError> {
    if bytes.len() < 13 {
        return Err(MediaError::InvalidHeader);
    }
    let width = le_u16(bytes, 6).ok_or(MediaError::InvalidHeader)?;
    let height = le_u16(bytes, 8).ok_or(MediaError::InvalidHeader)?;
    if width == 0 || height == 0 {
        return Err(MediaError::InvalidHeader);
    }
    let packed = bytes[10];
    let mut index = 13usize;
    if packed & 0x80 != 0 {
        let table_entries = 1usize
            .checked_shl(u32::from(packed & 0x07) + 1)
            .ok_or(MediaError::InvalidHeader)?;
        let table_bytes = table_entries
            .checked_mul(3)
            .ok_or(MediaError::InvalidHeader)?;
        index = index
            .checked_add(table_bytes)
            .ok_or(MediaError::InvalidHeader)?;
    }
    let mut frames = 0u32;
    loop {
        let block = *bytes.get(index).ok_or(MediaError::InvalidHeader)?;
        index += 1;
        match block {
            0x3B => break,
            0x21 => {
                let _label = *bytes.get(index).ok_or(MediaError::InvalidHeader)?;
                index += 1;
                index = skip_gif_sub_blocks(bytes, index)?;
            }
            0x2C => {
                if index + 9 > bytes.len() {
                    return Err(MediaError::InvalidHeader);
                }
                let local = bytes[index + 8];
                index += 9;
                if local & 0x80 != 0 {
                    let table_entries = 1usize
                        .checked_shl(u32::from(local & 0x07) + 1)
                        .ok_or(MediaError::InvalidHeader)?;
                    let table_bytes = table_entries
                        .checked_mul(3)
                        .ok_or(MediaError::InvalidHeader)?;
                    index = index
                        .checked_add(table_bytes)
                        .ok_or(MediaError::InvalidHeader)?;
                }
                let _lzw_min = *bytes.get(index).ok_or(MediaError::InvalidHeader)?;
                index += 1;
                index = skip_gif_sub_blocks(bytes, index)?;
                frames = frames.checked_add(1).ok_or(MediaError::InvalidHeader)?;
            }
            _ => return Err(MediaError::InvalidHeader),
        }
    }
    if frames == 0 {
        return Err(MediaError::InvalidHeader);
    }
    Ok((
        ImageDimensions::new(u32::from(width), u32::from(height)),
        frames,
    ))
}

fn skip_gif_sub_blocks(bytes: &[u8], mut index: usize) -> Result<usize, MediaError> {
    loop {
        let size = usize::from(*bytes.get(index).ok_or(MediaError::InvalidHeader)?);
        index += 1;
        if size == 0 {
            return Ok(index);
        }
        index = index.checked_add(size).ok_or(MediaError::InvalidHeader)?;
        if index > bytes.len() {
            return Err(MediaError::InvalidHeader);
        }
    }
}

fn webp_dimensions(bytes: &[u8]) -> Result<(ImageDimensions, u32), MediaError> {
    if bytes.len() < 16 || !bytes.starts_with(b"RIFF") || &bytes[8..12] != b"WEBP" {
        return Err(MediaError::InvalidHeader);
    }
    let mut index = 12usize;
    let mut dimensions = None;
    let mut frames = 0u32;
    let mut animated = false;
    while index + 8 <= bytes.len() {
        let fourcc = &bytes[index..index + 4];
        let size = usize::try_from(le_u32(bytes, index + 4).ok_or(MediaError::InvalidHeader)?)
            .map_err(|_| MediaError::InvalidHeader)?;
        let data_start = index + 8;
        let data_end = data_start
            .checked_add(size)
            .ok_or(MediaError::InvalidHeader)?;
        if data_end > bytes.len() {
            break;
        }
        match fourcc {
            b"VP8X" => {
                if size < 10 {
                    return Err(MediaError::InvalidHeader);
                }
                animated = bytes[data_start] & 0x02 != 0;
                let width = u24_le(bytes, data_start + 4).ok_or(MediaError::InvalidHeader)? + 1;
                let height = u24_le(bytes, data_start + 7).ok_or(MediaError::InvalidHeader)? + 1;
                dimensions = Some(ImageDimensions::new(width, height));
            }
            b"VP8 " => {
                if dimensions.is_none() {
                    dimensions = Some(webp_lossy_dimensions(bytes, data_start, size)?);
                }
            }
            b"VP8L" => {
                if dimensions.is_none() {
                    dimensions = Some(webp_lossless_dimensions(bytes, data_start, size)?);
                }
            }
            b"ANMF" => {
                frames = frames.checked_add(1).ok_or(MediaError::InvalidHeader)?;
            }
            _ => {}
        }
        index = data_end
            .checked_add(size & 1)
            .ok_or(MediaError::InvalidHeader)?;
    }
    let dimensions = dimensions.ok_or(MediaError::InvalidHeader)?;
    if !animated {
        frames = 1;
    }
    if frames == 0 {
        return Err(MediaError::InvalidHeader);
    }
    Ok((dimensions, frames))
}

fn webp_lossy_dimensions(
    bytes: &[u8],
    data_start: usize,
    size: usize,
) -> Result<ImageDimensions, MediaError> {
    if size < 10 {
        return Err(MediaError::InvalidHeader);
    }
    let start = data_start.checked_add(6).ok_or(MediaError::InvalidHeader)?;
    if bytes.get(data_start + 3..data_start + 6) != Some(&[0x9D, 0x01, 0x2A][..]) {
        return Err(MediaError::InvalidHeader);
    }
    let width = le_u16(bytes, start).ok_or(MediaError::InvalidHeader)? & 0x3FFF;
    let height = le_u16(bytes, start + 2).ok_or(MediaError::InvalidHeader)? & 0x3FFF;
    if width == 0 || height == 0 {
        return Err(MediaError::InvalidHeader);
    }
    Ok(ImageDimensions::new(u32::from(width), u32::from(height)))
}

fn webp_lossless_dimensions(
    bytes: &[u8],
    data_start: usize,
    size: usize,
) -> Result<ImageDimensions, MediaError> {
    if size < 5 {
        return Err(MediaError::InvalidHeader);
    }
    if bytes.get(data_start) != Some(&0x2F) {
        return Err(MediaError::InvalidHeader);
    }
    let bits = le_u32(bytes, data_start + 1).ok_or(MediaError::InvalidHeader)?;
    let width = (bits & 0x3FFF) + 1;
    let height = ((bits >> 14) & 0x3FFF) + 1;
    Ok(ImageDimensions::new(width, height))
}

fn bmp_dimensions(bytes: &[u8]) -> Result<ImageDimensions, MediaError> {
    if bytes.len() < 26 || !bytes.starts_with(b"BM") {
        return Err(MediaError::InvalidHeader);
    }
    let dib_size = le_u32(bytes, 14).ok_or(MediaError::InvalidHeader)?;
    if dib_size == 12 {
        let width = le_u16(bytes, 18).ok_or(MediaError::InvalidHeader)?;
        let height = le_u16(bytes, 20).ok_or(MediaError::InvalidHeader)?;
        if width == 0 || height == 0 {
            return Err(MediaError::InvalidHeader);
        }
        return Ok(ImageDimensions::new(u32::from(width), u32::from(height)));
    }
    if dib_size < 40 || bytes.len() < 26 {
        return Err(MediaError::InvalidHeader);
    }
    let width = le_i32(bytes, 18).ok_or(MediaError::InvalidHeader)?;
    let height = le_i32(bytes, 22).ok_or(MediaError::InvalidHeader)?;
    if width <= 0 || height == 0 || height == i32::MIN {
        return Err(MediaError::InvalidHeader);
    }
    Ok(ImageDimensions::new(
        u32::try_from(width).map_err(|_| MediaError::InvalidHeader)?,
        height.unsigned_abs(),
    ))
}

fn decode_base64(payload: &str, limit: u64) -> Result<Vec<u8>, MediaError> {
    let bytes = payload.as_bytes();
    let mut padding = 0usize;
    let mut length = bytes.len();
    while length > 0 && bytes[length - 1] == b'=' {
        padding += 1;
        length -= 1;
        if padding > 2 {
            return Err(MediaError::InvalidDataUrl);
        }
    }
    if padding > 0 && !(length + padding).is_multiple_of(4) {
        return Err(MediaError::InvalidDataUrl);
    }
    let decoded_len = match length % 4 {
        0 => length / 4 * 3,
        2 => length / 4 * 3 + 1,
        3 => length / 4 * 3 + 2,
        _ => return Err(MediaError::InvalidDataUrl),
    };
    let actual = u64::try_from(decoded_len).map_err(|_| MediaError::SourceTooLarge {
        limit,
        actual: u64::MAX,
    })?;
    if actual > limit {
        return Err(MediaError::SourceTooLarge { limit, actual });
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(decoded_len)
        .map_err(|_| MediaError::InvalidDataUrl)?;
    let mut index = 0usize;
    while index + 4 <= length {
        let a = base64_value(bytes[index]).ok_or(MediaError::InvalidDataUrl)?;
        let b = base64_value(bytes[index + 1]).ok_or(MediaError::InvalidDataUrl)?;
        let c = base64_value(bytes[index + 2]).ok_or(MediaError::InvalidDataUrl)?;
        let d = base64_value(bytes[index + 3]).ok_or(MediaError::InvalidDataUrl)?;
        output.push((a << 2) | (b >> 4));
        output.push((b << 4) | (c >> 2));
        output.push((c << 6) | d);
        index += 4;
    }
    match length - index {
        0 => {}
        2 => {
            let a = base64_value(bytes[index]).ok_or(MediaError::InvalidDataUrl)?;
            let b = base64_value(bytes[index + 1]).ok_or(MediaError::InvalidDataUrl)?;
            output.push((a << 2) | (b >> 4));
        }
        3 => {
            let a = base64_value(bytes[index]).ok_or(MediaError::InvalidDataUrl)?;
            let b = base64_value(bytes[index + 1]).ok_or(MediaError::InvalidDataUrl)?;
            let c = base64_value(bytes[index + 2]).ok_or(MediaError::InvalidDataUrl)?;
            output.push((a << 2) | (b >> 4));
            output.push((b << 4) | (c >> 2));
        }
        _ => return Err(MediaError::InvalidDataUrl),
    }
    Ok(output)
}

fn base64_value(byte: u8) -> Option<u8> {
    match byte {
        b'A'..=b'Z' => Some(byte - b'A'),
        b'a'..=b'z' => Some(byte - b'a' + 26),
        b'0'..=b'9' => Some(byte - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    }
}

fn percent_decode(payload: &str, limit: u64) -> Result<Vec<u8>, MediaError> {
    let bytes = payload.as_bytes();
    let actual = u64::try_from(bytes.len()).map_err(|_| MediaError::SourceTooLarge {
        limit,
        actual: u64::MAX,
    })?;
    if actual > limit {
        return Err(MediaError::SourceTooLarge { limit, actual });
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(bytes.len())
        .map_err(|_| MediaError::InvalidDataUrl)?;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes
                .get(index + 1)
                .and_then(|byte| hex_value(*byte))
                .ok_or(MediaError::InvalidDataUrl)?;
            let low = bytes
                .get(index + 2)
                .and_then(|byte| hex_value(*byte))
                .ok_or(MediaError::InvalidDataUrl)?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    Ok(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn be_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_be_bytes([slice[0], slice[1]]))
}

fn be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn le_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let slice = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

fn le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn le_i32(bytes: &[u8], offset: usize) -> Option<i32> {
    let slice = bytes.get(offset..offset.checked_add(4)?)?;
    Some(i32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u24_le(bytes: &[u8], offset: usize) -> Option<u32> {
    let slice = bytes.get(offset..offset.checked_add(3)?)?;
    Some(u32::from(slice[0]) | (u32::from(slice[1]) << 8) | (u32::from(slice[2]) << 16))
}
