//! 主窗口区域几何的唯一来源；纯数值，不依赖 GPUI、窗口或任何可变状态。
//!
//! 布局结构与以下数值来自 `docs/prototype/legacy/` 的 CSS（`#app` 网格行高
//! 44/36/1fr/28、`.editor` 的 `max-width:880px` 与 `padding:40px 56px 100px`、
//! 正文 `16px/1.75`，以及 900px/600px 两档媒体查询），按 UI §3.3 与 §4.1 作为
//! **实验候选**：本模块只提供取数起点，冻结与否由 V-09 实测裁决。
//!
//! `SCROLLBAR_WIDTH`、`MIN_TEXT_WIDTH` 与窄窗收缩规则是本实现的实验候选：原型只用
//! `scrollbar-width:thin` 等相对表述，没有给出这些像素值。
//!
//! 盒模型：原型的 `box-sizing:border-box` 生效，因此内容列的 880px **含**
//! 水平内边距，正文宽度是列宽减去两侧内边距。

/// 工具栏行高度（原型 44px）。
pub(super) const TOOLBAR_HEIGHT: f32 = 44.0;

/// 状态栏行高度（原型 28px）。
pub(super) const STATUSBAR_HEIGHT: f32 = 28.0;

/// 内容列最大宽度（原型 880px，含水平内边距）。
pub(super) const CONTENT_MAX_WIDTH: f32 = 880.0;

/// 正文最小宽度；窗口窄到放不下时先收内边距，再退到此宽度。
pub(super) const MIN_TEXT_WIDTH: f32 = 64.0;

/// 窄窗档位断点（原型 `@media(max-width:900px)`）。
pub(super) const NARROW_BREAKPOINT: f32 = 900.0;

/// 紧凑档位断点（原型 `@media(max-width:600px)`）。
pub(super) const COMPACT_BREAKPOINT: f32 = 600.0;

/// 正文字号（原型 16px）。
pub(super) const BODY_FONT_SIZE: f32 = 16.0;

/// 正文行高（原型 16px × 1.75）。
pub(super) const BODY_LINE_HEIGHT: f32 = 28.0;

/// 内容区右侧滚动条槽位宽度。
pub(super) const SCROLLBAR_WIDTH: f32 = 12.0;

/// 一份内容列的排版度量。
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ContentMetrics {
    /// 内容列宽度（含水平内边距）。
    pub(super) column_width: f32,
    /// 正文可用宽度。
    pub(super) text_width: f32,
    /// 水平内边距。
    pub(super) padding_x: f32,
    /// 首行上内边距。
    pub(super) padding_top: f32,
    /// 末行下内边距。
    pub(super) padding_bottom: f32,
}

/// 按窗口宽度选择留白档位；断点与原型的媒体查询一致。
fn padding_tier(window_width: f32) -> (f32, f32, f32) {
    if window_width > NARROW_BREAKPOINT {
        (56.0, 40.0, 100.0)
    } else if window_width > COMPACT_BREAKPOINT {
        (28.0, 30.0, 80.0)
    } else {
        (20.0, 24.0, 60.0)
    }
}

/// 按窗口宽度与内容区宽度计算内容列的排版度量。
///
/// `window_width` 用于选择留白档位（原型的媒体查询作用于视口宽度）；`area_width`
/// 是内容区自身宽度，M1 无侧栏时等于窗口宽度，M5 出现侧栏列后由实际区域宽度
/// 传入，列宽上限与正文宽度随之收缩。
pub(super) fn content_metrics(window_width: f32, area_width: f32) -> ContentMetrics {
    let area_width = area_width.max(0.0);
    // 不变量：内容区不小于滚动条槽位时，列宽与右侧滚动条槽位之和不得超过内容区
    // 宽度。内容区窄于滚动条宽属于不可用退化尺寸，列宽退化为 0，由平台窗口尺寸
    // 下限兜底。
    let available = (area_width - SCROLLBAR_WIDTH).max(0.0);
    let (tier_padding_x, padding_top, padding_bottom) = padding_tier(window_width);
    let column_width = available.min(CONTENT_MAX_WIDTH);
    let mut padding_x = tier_padding_x;
    let mut text_width = column_width - 2.0 * padding_x;
    if text_width < MIN_TEXT_WIDTH {
        // 窗口窄到放不下当前内边距时先收内边距；仍放不下时以不横向溢出为准，
        // 文本宽退到列宽，不强行保留 64px 下限。
        padding_x = ((column_width - MIN_TEXT_WIDTH) / 2.0).clamp(0.0, tier_padding_x);
        text_width = column_width - 2.0 * padding_x;
    }
    let text_width = text_width.max(MIN_TEXT_WIDTH.min(column_width));
    ContentMetrics {
        column_width,
        text_width,
        padding_x,
        padding_top,
        padding_bottom,
    }
}

/// 模块内无头测试：单元测试与生产代码同源编译，无需 `#[path]` 引入。
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wide_window_uses_prototype_column_and_text_width() {
        let metrics = content_metrics(1200.0, 1200.0);

        assert_eq!(TOOLBAR_HEIGHT, 44.0);
        assert_eq!(STATUSBAR_HEIGHT, 28.0);
        assert_eq!(CONTENT_MAX_WIDTH, 880.0);
        assert_eq!(BODY_FONT_SIZE, 16.0);
        assert_eq!(BODY_LINE_HEIGHT, 28.0);
        assert_eq!(metrics.column_width, 880.0);
        assert_eq!(metrics.padding_x, 56.0);
        assert_eq!(metrics.text_width, 768.0);
        assert_eq!(metrics.padding_top, 40.0);
        assert_eq!(metrics.padding_bottom, 100.0);
    }

    #[test]
    fn breakpoints_match_prototype_media_queries() {
        let compact = content_metrics(COMPACT_BREAKPOINT, COMPACT_BREAKPOINT);
        assert_eq!(compact.padding_x, 20.0);
        assert_eq!(compact.padding_top, 24.0);
        assert_eq!(compact.padding_bottom, 60.0);

        let narrow = content_metrics(NARROW_BREAKPOINT, NARROW_BREAKPOINT);
        assert_eq!(narrow.padding_x, 28.0);
        assert_eq!(narrow.padding_top, 30.0);
        assert_eq!(narrow.padding_bottom, 80.0);

        let wide = content_metrics(NARROW_BREAKPOINT + 0.5, NARROW_BREAKPOINT + 0.5);
        assert_eq!(wide.padding_x, 56.0);
        assert_eq!(wide.padding_top, 40.0);
        assert_eq!(wide.padding_bottom, 100.0);
    }

    #[test]
    fn padding_shrinks_before_text_width_hits_the_minimum() {
        let metrics = content_metrics(500.0, 100.0);

        assert_eq!(metrics.column_width, 88.0);
        assert_eq!(metrics.padding_x, 12.0);
        assert_eq!(metrics.text_width, 64.0);
    }

    #[test]
    fn content_column_and_scrollbar_never_exceed_content_area() {
        for area in [
            SCROLLBAR_WIDTH,
            40.0,
            64.0,
            76.0,
            100.0,
            500.0,
            900.0,
            1200.0,
        ] {
            let metrics = content_metrics(area, area);
            assert!(
                metrics.column_width + SCROLLBAR_WIDTH <= area + f32::EPSILON,
                "area={area}, metrics={metrics:?}"
            );
        }
    }

    #[test]
    fn extreme_narrow_content_area_collapses_padding_and_column() {
        let metrics = content_metrics(200.0, 40.0);

        assert_eq!(metrics.column_width, 28.0);
        assert_eq!(metrics.padding_x, 0.0);
        assert_eq!(metrics.text_width, 28.0);
    }
}
