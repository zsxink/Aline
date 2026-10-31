//! 所见命令的保护/HTML 边界检查；只在计划与校验阶段读取解析结果。

use std::ops::Range;

use aline_markdown::{Parsed, ProtectionKind};

use crate::refusal::EditRefusal;

/// 检查所见修改是否触及保护或 HTML 区间。
///
/// 边界按闭区间相交处理：选区端点正好落在标签/保护边界上也算触及。
pub(crate) fn check_visual_permissions(
    parsed: &Parsed,
    range: &Range<usize>,
) -> Result<(), EditRefusal> {
    for protected in parsed.protected_ranges() {
        let protected_range = protected.range().byte_range();
        if touches(range, &protected_range) {
            return match protected.kind() {
                ProtectionKind::Html => Err(EditRefusal::HtmlBoundary {
                    range: protected_range,
                }),
                kind => Err(EditRefusal::ProtectedRange {
                    kind,
                    range: protected_range,
                }),
            };
        }
    }
    for styled in parsed.styled_ranges() {
        let styled_range = styled.range().byte_range();
        if touches(range, &styled_range) {
            return Err(EditRefusal::HtmlBoundary {
                range: styled_range,
            });
        }
    }
    Ok(())
}

fn touches(left: &Range<usize>, right: &Range<usize>) -> bool {
    left.start <= right.end && right.start <= left.end
}
