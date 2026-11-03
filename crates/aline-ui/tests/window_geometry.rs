//! 主窗几何的独立无窗口预期；几何模块不依赖 GPUI，可直接以 `#[path]` 引入。

#[path = "../src/native_window/geometry.rs"]
mod geometry;

use geometry::{
    BODY_FONT_SIZE, BODY_LINE_HEIGHT, COMPACT_BREAKPOINT, CONTENT_MAX_WIDTH, NARROW_BREAKPOINT,
    SCROLLBAR_WIDTH, STATUSBAR_HEIGHT, TOOLBAR_HEIGHT, content_metrics,
};

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
