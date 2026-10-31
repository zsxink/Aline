//! 快照内锚点经正补丁的受检变换；只处理字节坐标，不解析或写盘。

use crate::{Affinity, BufferError, CheckedBatch, SrcOffset, SrcSelection, TextSnapshot};

/// 锚点经过一个批次后的有效位置或失效结论。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnchorTransform {
    /// 锚点仍有效并映射到 after 坐标。
    Valid {
        /// 映射后的 UTF-8 字节偏移。
        byte: usize,
    },
    /// 锚点严格位于被替换的非空范围内部，没有唯一映射。
    Invalidated,
}

/// 两端均有效时的一组 after 原始字节选区。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SelectionTransform {
    anchor: usize,
    head: usize,
}

impl SelectionTransform {
    /// 返回映射后的锚点字节位置。
    pub const fn anchor(self) -> usize {
        self.anchor
    }

    /// 返回映射后的活动端点字节位置。
    pub const fn head(self) -> usize {
        self.head
    }

    /// 判断映射后的选区方向是否反转。
    pub const fn is_reversed(self) -> bool {
        self.anchor > self.head
    }
}

/// 将一个 before 坐标锚点按补丁与方向偏好变换为 after 坐标。
///
/// 非空替换内部的位置失效；替换起点按 `affinity` 决定停留在前文还是后文，
/// 替换终点始终映射到替换文本之后。批次会先对当前快照复验身份。
pub fn transform_anchor(
    before: &TextSnapshot,
    anchor: &SrcOffset,
    batch: &CheckedBatch,
    affinity: Affinity,
) -> Result<AnchorTransform, BufferError> {
    batch.validate(before)?;
    let position = before.validate_offset(anchor)?;
    transform_position(position, batch.patches(), affinity)
}

/// 变换选区两端；任一端失效时返回 `None`，不把半有效选区贴到 after。
pub fn transform_selection(
    before: &TextSnapshot,
    selection: &SrcSelection,
    batch: &CheckedBatch,
    affinity: Affinity,
) -> Result<Option<SelectionTransform>, BufferError> {
    batch.validate(before)?;
    before.validate_selection(selection)?;
    let anchor = transform_position(selection.anchor().byte_offset(), batch.patches(), affinity)?;
    let head = transform_position(selection.head().byte_offset(), batch.patches(), affinity)?;
    match (anchor, head) {
        (AnchorTransform::Valid { byte: anchor }, AnchorTransform::Valid { byte: head }) => {
            Ok(Some(SelectionTransform { anchor, head }))
        }
        _ => Ok(None),
    }
}

fn transform_position(
    position: usize,
    patches: &[(std::ops::Range<usize>, String)],
    affinity: Affinity,
) -> Result<AnchorTransform, BufferError> {
    let mut mapped = position;
    for (range, replacement) in patches {
        if position < range.start {
            break;
        }
        if range.is_empty() {
            if position == range.start {
                if affinity == Affinity::After {
                    mapped = mapped
                        .checked_add(replacement.len())
                        .ok_or(BufferError::LengthOverflow)?;
                }
                continue;
            }
            mapped = mapped
                .checked_add(replacement.len())
                .ok_or(BufferError::LengthOverflow)?;
            continue;
        }
        if position == range.start {
            if affinity == Affinity::After {
                mapped = mapped
                    .checked_add(replacement.len())
                    .ok_or(BufferError::LengthOverflow)?;
            }
            return Ok(AnchorTransform::Valid { byte: mapped });
        }
        if position < range.end {
            return Ok(AnchorTransform::Invalidated);
        }
        mapped = mapped
            .checked_sub(range.len())
            .and_then(|value| value.checked_add(replacement.len()))
            .ok_or(BufferError::LengthOverflow)?;
    }
    Ok(AnchorTransform::Valid { byte: mapped })
}
