//! 存储字节到编辑源码的受检解码；只处理内存视图，不写盘、不修改缓冲。
//!
//! 解码先检查注入预算，再识别 UTF-16 BOM 并验证 UTF-8，只分离一个头部
//! UTF-8 BOM；正文 U+FEFF、CR、LF 与文件末尾状态原样保留。存储元数据记录
//! 原始 CRLF 对数与未配对 LF 数，供后续编码选择多数派行尾策略。

use std::fmt;

use crate::baseline::SourceBaseline;
use crate::storage::{StorageBudget, StorageSnapshot};

/// 由原始字节多数派决定的重编码行尾策略；平票（含均为零）为 LF。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineEnding {
    /// 编码时以单个 LF 表示一个行尾单元。
    Lf,
    /// 编码时以 CRLF 表示一个行尾单元。
    Crlf,
}

/// 解码得到的只读存储事实，不替代源码内容相等或补丁历史判断。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageMetadata {
    has_bom: bool,
    crlf_pairs: usize,
    unpaired_lf: usize,
    line_ending: LineEnding,
}

impl StorageMetadata {
    /// 返回原始字节是否以 UTF-8 BOM 开头；该 BOM 已从编辑源码分离。
    pub const fn has_bom(self) -> bool {
        self.has_bom
    }

    /// 返回原始字节中完整 CRLF 对的数量。
    pub const fn crlf_pairs(self) -> usize {
        self.crlf_pairs
    }

    /// 返回不属于 CRLF 对的 LF 数量；孤立 CR 不计入。
    pub const fn unpaired_lf(self) -> usize {
        self.unpaired_lf
    }

    /// 返回重编码时应使用的多数派行尾策略。
    pub const fn line_ending(self) -> LineEnding {
        self.line_ending
    }
}

/// 解码失败；预算与编码类别可区分，不返回有损替换后的伪源码。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodeError {
    /// 输入字节数超过注入预算。
    BudgetExceeded {
        /// 输入字节数。
        needed: usize,
        /// 预算上限。
        limit: usize,
    },
    /// 输入以 UTF-16 BOM 开头；仅 BOM 前缀可区别，无 BOM 的 UTF-16 按 UTF-8 验证结果处理。
    Utf16Bom {
        /// 是否为小端 UTF-16 BOM。
        little_endian: bool,
    },
    /// 输入不是有效 UTF-8；偏移相对原始存储字节。
    InvalidUtf8 {
        /// 第一个无法构成有效 UTF-8 的字节位置；截断时指向不完整序列的起点。
        valid_up_to: usize,
        /// 非法序列长度；`None` 表示输入在序列中途截断。
        error_len: Option<usize>,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for DecodeError {}

/// 解码成功的编辑源码、存储元数据与原始存储基线身份。
#[derive(Clone, Debug)]
pub struct DecodedSource {
    baseline: SourceBaseline,
}

impl DecodedSource {
    /// 返回解码所依据的只读存储视图；其身份与传入快照相同。
    pub fn storage(&self) -> &StorageSnapshot {
        self.baseline.storage()
    }

    /// 返回编辑源码：已分离一个头部 UTF-8 BOM，其余 CR/LF 原样保留。
    pub fn source(&self) -> &str {
        self.baseline.source()
    }

    /// 返回原始字节的 BOM 与行尾统计。
    pub const fn metadata(&self) -> StorageMetadata {
        self.baseline.metadata()
    }

    /// 返回本次解码建立的只读存储基线。
    pub fn baseline(&self) -> &SourceBaseline {
        &self.baseline
    }

    /// 取出本次解码建立的只读存储基线。
    pub fn into_baseline(self) -> SourceBaseline {
        self.baseline
    }
}

/// 先验预算，再验证编码并解码存储视图。
///
/// 失败时不修改输入快照，也不返回部分源码。成功结果携带与输入相同的
/// 存储基线身份，供后续编码绑定内容相等证据。
pub fn decode(
    storage: &StorageSnapshot,
    budget: StorageBudget,
) -> Result<DecodedSource, DecodeError> {
    let bytes = storage.bytes();
    let needed = bytes.len();
    let limit = budget.max_bytes();
    if needed > limit {
        return Err(DecodeError::BudgetExceeded { needed, limit });
    }
    if bytes.starts_with(&[0xff, 0xfe]) {
        return Err(DecodeError::Utf16Bom {
            little_endian: true,
        });
    }
    if bytes.starts_with(&[0xfe, 0xff]) {
        return Err(DecodeError::Utf16Bom {
            little_endian: false,
        });
    }
    let text = std::str::from_utf8(bytes).map_err(|error| DecodeError::InvalidUtf8 {
        valid_up_to: error.valid_up_to(),
        error_len: error.error_len(),
    })?;
    let (has_bom, body) = match text.strip_prefix('\u{feff}') {
        Some(body) => (true, body),
        None => (false, text),
    };
    let (crlf_pairs, unpaired_lf) = scan_line_endings(body.as_bytes());
    let line_ending = if crlf_pairs > unpaired_lf {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    };
    Ok(DecodedSource {
        baseline: SourceBaseline::new(
            storage.clone(),
            body.to_owned(),
            StorageMetadata {
                has_bom,
                crlf_pairs,
                unpaired_lf,
                line_ending,
            },
        ),
    })
}

fn scan_line_endings(bytes: &[u8]) -> (usize, usize) {
    let mut crlf_pairs = 0usize;
    let mut unpaired_lf = 0usize;
    let mut index = 0usize;
    while let Some(byte) = bytes.get(index) {
        match byte {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                crlf_pairs += 1;
                index += 2;
            }
            b'\n' => {
                unpaired_lf += 1;
                index += 1;
            }
            _ => index += 1,
        }
    }
    (crlf_pairs, unpaired_lf)
}
