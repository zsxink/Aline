//! 冻结编辑源码到完整待写 bytes 的受检编码；不写盘、不改 Buffer/History。
//!
//! 冻结文本与基线源码逐字相等时走原 bytes 通道；否则重放头部 BOM 与基线的
//! 多数派行尾。CRLF 对和未配对 LF 各为一个行尾单元，孤立 CR 逐字节保留；
//! LF 策略下紧邻孤立 CR 的 CRLF 单元保留 CRLF，避免重开时丢失未配对 CR。
//! 基线无 BOM 而冻结源码以 U+FEFF 开头时补写一个头部 BOM，使该正文字符在
//! 再解码时不被当成头部 BOM 删除（2026-10-31 裁决）。
//!
//! 编码不接受调用方自报的“未修改”布尔值，相等证据由 source 在同一基线对象上
//! 直接比较冻结文本得出；因此 3.3 列出的“基线或相等证据不匹配”错误按构造
//! 不可达。预算失败对应 [`EncodeError::BudgetExceeded`]，长度溢出对应防御性的
//! [`EncodeError::LengthOverflow`]，不返回部分写盘结果。

use std::fmt;

use crate::baseline::SourceBaseline;
use crate::decode::LineEnding;
use crate::storage::{BaselineId, StorageBudget};

const BOM: &[u8] = &[0xef, 0xbb, 0xbf];

/// 编码失败；预算与长度溢出可区别，不返回部分写盘结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EncodeError {
    /// 完整待写 bytes 超过注入预算。
    BudgetExceeded {
        /// 完整待写字节数。
        needed: usize,
        /// 预算上限。
        limit: usize,
    },
    /// 待写字节数无法用 `usize` 表示；不进行静默截断。
    ///
    /// 防御性分支：CRLF 策略下正文最坏约为源码两倍，正常可分配输入不会触发。
    LengthOverflow,
}

impl fmt::Display for EncodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for EncodeError {}

/// 编码得到的完整待写 bytes 及其来源基线身份。
#[derive(Clone)]
pub struct EncodedBytes {
    bytes: Vec<u8>,
    baseline: BaselineId,
    reused_original: bool,
}

impl fmt::Debug for EncodedBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedBytes")
            .field("bytes_len", &self.bytes.len())
            .field("reused_original", &self.reused_original)
            .finish()
    }
}

impl EncodedBytes {
    /// 返回完整待写 bytes；未命中原 bytes 通道时已重放 BOM 与多数派行尾。
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// 返回这次编码所依据的存储基线身份。
    pub fn baseline_id(&self) -> &BaselineId {
        &self.baseline
    }

    /// 返回是否因冻结文本与基线源码逐字相等而原样复用了输入 bytes。
    ///
    /// 该值由 source 在同一基线对象上比较文本得出；即使变更后的编码结果
    /// 恰好与输入 bytes 相同，这里也为 `false`。
    pub const fn reused_original(&self) -> bool {
        self.reused_original
    }
}

/// 把冻结编辑源码编码为完整待写 bytes。
///
/// 相等时返回基线原 bytes（含 BOM、混合行尾、孤立 CR 与末尾状态）；不等时
/// 重放 BOM 和 [`SourceBaseline`] 记录的行尾策略，平票为 LF。基线无 BOM 而
/// 冻结源码以 U+FEFF 开头时，补写一个头部 BOM 保护该正文字符，预算计入。
/// 编码不修改基线或输入文本，不隐式整理行尾，也不增删末尾换行。
pub fn encode(
    frozen_source: &str,
    baseline: &SourceBaseline,
    budget: StorageBudget,
) -> Result<EncodedBytes, EncodeError> {
    let limit = budget.max_bytes();
    if frozen_source == baseline.source() {
        let needed = baseline.storage().bytes().len();
        check_budget(needed, limit)?;
        return Ok(EncodedBytes {
            bytes: baseline.storage().bytes().to_vec(),
            baseline: baseline.id().clone(),
            reused_original: true,
        });
    }

    let policy = baseline.metadata().line_ending();
    let emit_bom = emits_header_bom(frozen_source, baseline);
    let mut body_len = 0usize;
    walk_body(frozen_source, policy, |chunk| {
        body_len = body_len
            .checked_add(chunk.len())
            .ok_or(EncodeError::LengthOverflow)?;
        Ok(())
    })?;
    let bom_len = if emit_bom { BOM.len() } else { 0 };
    let needed = body_len
        .checked_add(bom_len)
        .ok_or(EncodeError::LengthOverflow)?;
    check_budget(needed, limit)?;

    let mut bytes = Vec::with_capacity(needed);
    if emit_bom {
        bytes.extend_from_slice(BOM);
    }
    walk_body(frozen_source, policy, |chunk| {
        bytes.extend_from_slice(chunk);
        Ok(())
    })?;
    Ok(EncodedBytes {
        bytes,
        baseline: baseline.id().clone(),
        reused_original: false,
    })
}

/// 判断改后编码是否写出头部 BOM。
///
/// 基线原有 BOM 时重放；基线无 BOM 但冻结源码以 U+FEFF 开头时也写出一个，
/// 否则再解码会把该正文首字符当成头部 BOM 删除。后一情形不改写源码，也不
/// 声称基线原本有 BOM；成功写盘后的新基线由 4.3/4.4 按实际字节记为有 BOM。
fn emits_header_bom(frozen_source: &str, baseline: &SourceBaseline) -> bool {
    baseline.metadata().has_bom() || frozen_source.starts_with('\u{feff}')
}

fn check_budget(needed: usize, limit: usize) -> Result<(), EncodeError> {
    if needed > limit {
        return Err(EncodeError::BudgetExceeded { needed, limit });
    }
    Ok(())
}

/// 按行尾单元扫描冻结源码，把输出片段交给 `emit`。
///
/// CRLF 单元在 LF 策略下输出 LF；若它紧跟在孤立 CR 之后，则保留 CRLF，
/// 否则重开时前一个未配对 CR 会与 LF 拼成新的 CRLF 而丢失。未配对 LF 在
/// CRLF 策略下输出 CRLF；孤立 CR 与其余字节原样输出。
fn walk_body(
    source: &str,
    policy: LineEnding,
    mut emit: impl FnMut(&[u8]) -> Result<(), EncodeError>,
) -> Result<(), EncodeError> {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    let mut run_start = 0usize;
    while let Some(byte) = bytes.get(index) {
        match byte {
            b'\r' if bytes.get(index + 1) == Some(&b'\n') => {
                if run_start < index {
                    emit(&bytes[run_start..index])?;
                }
                let preserves_isolated_cr =
                    policy == LineEnding::Lf && index > 0 && bytes[index - 1] == b'\r';
                if policy == LineEnding::Crlf || preserves_isolated_cr {
                    emit(b"\r\n")?;
                } else {
                    emit(b"\n")?;
                }
                index += 2;
                run_start = index;
            }
            b'\n' => {
                if run_start < index {
                    emit(&bytes[run_start..index])?;
                }
                match policy {
                    LineEnding::Lf => emit(b"\n")?,
                    LineEnding::Crlf => emit(b"\r\n")?,
                }
                index += 1;
                run_start = index;
            }
            _ => index += 1,
        }
    }
    if run_start < bytes.len() {
        emit(&bytes[run_start..])?;
    }
    Ok(())
}
