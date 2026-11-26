//! 正文图片入口：把 Markdown 目标转换为可渲染来源，并在解码前执行预算。
//!
//! 本模块只读候选本地文件与 `data:` 字节，不发网络请求、不创建 GPUI 对象。
//! 本地栅格图保留路径交给宿主资源加载器（以保留 GIF/WebP 动画）；SVG 与
//! `data:` 目标返回已验证字节。路径与文件内容可能在入口校验后被替换，
//! 宿主仍须把后续读取失败当作可恢复错误；这不是解码预算的替代。

use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::media::{
    ImageDimensions, MediaBudget, MediaError, MediaFormat, SourceKind, classify_source,
    decode_data_url, probe_image, sanitize_svg, sniff_format,
};

/// 入口判维读取的头部字节上限；正文栅格头远小于此值。
const HEADER_BYTES: u64 = 64 * 1024;

/// 通过入口校验的图片可渲染来源。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BodyImageSource {
    /// 已验证的本地栅格文件；由宿主资源加载器读取并保留动图。
    Local(PathBuf),
    /// 已验证并解码/净化的字节；SVG 与 `data:` 目标走这里。
    Bytes(Vec<u8>),
}

/// 通过入口校验的正文图片。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BodyImage {
    source: BodyImageSource,
    format: MediaFormat,
    dimensions: ImageDimensions,
    frames: u32,
}

impl BodyImage {
    /// 返回可渲染来源。
    pub fn source(&self) -> &BodyImageSource {
        &self.source
    }

    /// 返回按签名判定的格式。
    pub const fn format(&self) -> MediaFormat {
        self.format
    }

    /// 返回入口读出的固有尺寸。
    pub const fn dimensions(&self) -> ImageDimensions {
        self.dimensions
    }

    /// 返回解码帧数；动画 GIF/WebP 大于 1。
    pub const fn frames(&self) -> u32 {
        self.frames
    }
}

/// 正文图片被拒绝的原因；全部在解码或光栅化之前产生。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BodyImageError {
    /// 未注入预算，禁止解码。
    NoBudget,
    /// 远程目标；正文不发起网络请求。
    Remote,
    /// 非本地、非 `data:` 的目标。
    UnsupportedScheme,
    /// 本地文件读取失败。
    Io,
    /// 入口判定失败（格式、体积、像素、帧或解码上限）。
    Media(MediaError),
}

impl fmt::Display for BodyImageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for BodyImageError {}

/// 解析正文图片目标。
///
/// 未注入预算时立即拒绝，不读磁盘也不解码。相对路径以 `document_dir` 为基准；
/// `file:` URL 只接受空 authority 或 `localhost`。
pub fn resolve_body_image(
    destination: &str,
    document_dir: &Path,
    budget: Option<MediaBudget>,
) -> Result<BodyImage, BodyImageError> {
    let Some(budget) = budget else {
        return Err(BodyImageError::NoBudget);
    };
    match classify_source(destination) {
        SourceKind::Remote => Err(BodyImageError::Remote),
        SourceKind::Unsupported => Err(BodyImageError::UnsupportedScheme),
        SourceKind::DataUrl => {
            let bytes = decode_data_url(destination, budget).map_err(BodyImageError::Media)?;
            from_bytes(bytes, budget)
        }
        SourceKind::Local => {
            let path = local_path(destination, document_dir)?;
            from_local(path, budget)
        }
    }
}

fn from_bytes(bytes: Vec<u8>, budget: MediaBudget) -> Result<BodyImage, BodyImageError> {
    let Some(format) = sniff_format(&bytes) else {
        return Err(BodyImageError::Media(MediaError::UnsupportedFormat));
    };
    if format == MediaFormat::Svg {
        let safe = sanitize_svg(&bytes, budget).map_err(BodyImageError::Media)?;
        return Ok(BodyImage {
            source: BodyImageSource::Bytes(safe.bytes().to_vec()),
            format,
            dimensions: safe.dimensions(),
            frames: safe.frames(),
        });
    }
    let info = probe_image(&bytes, budget).map_err(BodyImageError::Media)?;
    Ok(BodyImage {
        source: BodyImageSource::Bytes(bytes),
        format: info.format,
        dimensions: info.dimensions,
        frames: info.frames,
    })
}

fn from_local(path: PathBuf, budget: MediaBudget) -> Result<BodyImage, BodyImageError> {
    let metadata = std::fs::metadata(&path).map_err(|_| BodyImageError::Io)?;
    let length = metadata.len();
    if length > budget.max_source_bytes() {
        return Err(BodyImageError::Media(MediaError::SourceTooLarge {
            limit: budget.max_source_bytes(),
            actual: length,
        }));
    }
    let head = read_head(&path, length)?;
    let Some(format) = sniff_format(&head) else {
        return Err(BodyImageError::Media(MediaError::UnsupportedFormat));
    };
    if format == MediaFormat::Svg {
        // SVG 必须先读完整字节再做安全解析；体积已按预算预检。
        let bytes = std::fs::read(&path).map_err(|_| BodyImageError::Io)?;
        let safe = sanitize_svg(&bytes, budget).map_err(BodyImageError::Media)?;
        return Ok(BodyImage {
            source: BodyImageSource::Bytes(safe.bytes().to_vec()),
            format,
            dimensions: safe.dimensions(),
            frames: safe.frames(),
        });
    }
    let info = probe_image(&head, budget).map_err(BodyImageError::Media)?;
    Ok(BodyImage {
        source: BodyImageSource::Local(path),
        format: info.format,
        dimensions: info.dimensions,
        frames: info.frames,
    })
}

fn read_head(path: &Path, length: u64) -> Result<Vec<u8>, BodyImageError> {
    let file = File::open(path).map_err(|_| BodyImageError::Io)?;
    let mut head = Vec::new();
    file.take(length.min(HEADER_BYTES))
        .read_to_end(&mut head)
        .map_err(|_| BodyImageError::Io)?;
    Ok(head)
}

fn local_path(destination: &str, document_dir: &Path) -> Result<PathBuf, BodyImageError> {
    let trimmed = destination.trim();
    let lower = trimmed.to_ascii_lowercase();
    let raw = if lower.starts_with("file:") {
        file_url_path(&trimmed["file:".len()..])?
    } else {
        trimmed.to_owned()
    };
    let decoded = percent_decode(&raw).ok_or(BodyImageError::UnsupportedScheme)?;
    let path = Path::new(&decoded);
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        document_dir.join(path)
    })
}

/// `file:` 之后的路径：只接受空 authority 或 `localhost`，不接受远程主机。
fn file_url_path(rest: &str) -> Result<String, BodyImageError> {
    let Some(after) = rest.strip_prefix("//") else {
        return Ok(rest.to_owned());
    };
    let (authority, path) = match after.find('/') {
        Some(slash) => (&after[..slash], &after[slash..]),
        None => (after, ""),
    };
    if !authority.is_empty() && !authority.eq_ignore_ascii_case("localhost") {
        return Err(BodyImageError::UnsupportedScheme);
    }
    Ok(path.to_owned())
}

/// 解码 Markdown 目标里的百分号转义；非法转义或非 UTF-8 返回 `None`。
fn percent_decode(input: &str) -> Option<String> {
    if !input.contains('%') {
        return Some(input.to_owned());
    }
    let bytes = input.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = hex(*bytes.get(index + 1)?)?;
            let low = hex(*bytes.get(index + 2)?)?;
            output.push((high << 4) | low);
            index += 3;
        } else {
            output.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(output).ok()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
