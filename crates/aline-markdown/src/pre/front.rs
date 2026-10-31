//! 文首 front matter 识别；只接受 BOM 之后、文首成对的 `---`/`+++`。

use std::ops::Range;

/// front matter 分隔符种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrontMatterKind {
    /// YAML 风格 `---`。
    Yaml,
    /// TOML 风格 `+++`。
    Plus,
}

/// 已识别的成对 front matter 源码范围；范围包含 BOM（若有）和两侧标记行。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrontMatter {
    range: Range<usize>,
    kind: FrontMatterKind,
}

impl FrontMatter {
    /// 返回包含标记行的源码范围，不含闭合行后的换行。
    pub fn range(&self) -> Range<usize> {
        self.range.clone()
    }

    /// 返回分隔符种类。
    pub const fn kind(&self) -> FrontMatterKind {
        self.kind
    }
}

/// 识别文首成对 front matter；未闭合、空首行或首行不是标记时返回 None。
pub fn detect(source: &str) -> Option<FrontMatter> {
    let bom_len = if source.starts_with('\u{feff}') { 3 } else { 0 };
    let first_end = line_content_end(source, bom_len)?;
    let first = source.get(bom_len..first_end)?;
    let marker = first.trim_end_matches([' ', '\t']);
    let kind = match marker {
        "---" => FrontMatterKind::Yaml,
        "+++" => FrontMatterKind::Plus,
        _ => return None,
    };
    let mut line_start = first_end.checked_add(line_ending_len(source, first_end))?;
    while line_start <= source.len() {
        let line_end = line_content_end(source, line_start)?;
        let line = source.get(line_start..line_end)?;
        if line.trim_end_matches([' ', '\t']) == marker {
            return Some(FrontMatter {
                range: 0..line_end,
                kind,
            });
        }
        let next = line_end.checked_add(line_ending_len(source, line_end))?;
        if next == line_start {
            return None;
        }
        line_start = next;
    }
    None
}

fn line_content_end(source: &str, start: usize) -> Option<usize> {
    if start > source.len() {
        return None;
    }
    let rest = source.get(start..)?;
    let end = rest
        .find('\n')
        .map_or(source.len(), |offset| start + offset);
    let end = if end > start && source.as_bytes().get(end - 1) == Some(&b'\r') {
        end - 1
    } else {
        end
    };
    Some(end)
}

fn line_ending_len(source: &str, content_end: usize) -> usize {
    match source.as_bytes().get(content_end) {
        Some(b'\r') if source.as_bytes().get(content_end + 1) == Some(&b'\n') => 2,
        Some(b'\n') => 1,
        _ => 0,
    }
}
