//! 图片目标定位：右键图片菜单按源码偏移找到最内层图片引用。
//!
//! 只沿展示树查找图片节点；偏移来自绘制文字命中或图片元素命中，不重新解析。

use crate::{Parsed, PresentationKind, PresentationNode};

/// 返回包含给定源码偏移的最内层图片目标。
///
/// 图片、普通文字或落在图片范围之外的偏移返回 `None`；嵌套在链接内的图片
/// 返回图片自身的目标，链接目标仍由
/// [`link_destination_at`](crate::link_destination_at) 查询。
pub fn image_destination_at(parsed: &Parsed, source_offset: usize) -> Option<&str> {
    fn find(nodes: &[PresentationNode], offset: usize) -> Option<&str> {
        for node in nodes {
            let range = node.source_range().byte_range();
            if offset < range.start || offset >= range.end {
                continue;
            }
            if let PresentationKind::Image { destination } = node.kind() {
                return Some(destination);
            }
            if let Some(found) = find(node.children(), offset) {
                return Some(found);
            }
        }
        None
    }
    find(parsed.presentation(), source_offset)
}
