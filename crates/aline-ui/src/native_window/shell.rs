//! 主窗三段骨架的元素装配：工具栏行、内容区、状态栏行与表面配色。
//!
//! 几何一律取自 `geometry.rs`，这里不持有第二套数值，也不持有窗口状态；
//! 交互监听由 `native_window.rs` 装配。配色使用 UI §3.1 已确认的 GitHub 白天
//! 语义色：画布、正文、次级文字、边框与强调色。原型状态栏的米白表面不在已确认
//! 色值内，因此状态栏沿用画布色并以边框和次级文字区分，具体表面关系待 V-09。

use std::sync::Arc;

use gpui::{Div, Image, ImageFormat, ObjectFit, SharedString, div, img, prelude::*, px, rgb};

use super::geometry::{STATUSBAR_HEIGHT, TOOLBAR_HEIGHT};
use super::tokens;

/// 应用内品牌图标；与 `docs/assets/app-icon/aline.png` 同一文件，按目标尺寸缩放。
const BRAND_ICON: &[u8] = include_bytes!("../../../../docs/assets/app-icon/aline.png");

/// 构造应用内品牌图标；只在窗口创建时调用一次，渲染路径复用同一实例。
pub(super) fn brand_icon() -> Arc<Image> {
    Arc::new(Image::from_bytes(ImageFormat::Png, BRAND_ICON.to_vec()))
}

/// 工具栏行：固定高度、左右内边距、下边框。
pub(super) fn toolbar_row() -> Div {
    div()
        .h(px(TOOLBAR_HEIGHT))
        .w_full()
        .flex()
        .items_center()
        .gap_2()
        .px_3()
        .bg(rgb(tokens::SURFACE))
        .border_b_1()
        .border_color(rgb(tokens::BORDER))
        .overflow_hidden()
}

/// 内容区：占据剩余高度，内部保留一排容器以便后续插入侧栏列。
pub(super) fn content_row() -> Div {
    div()
        .flex_1()
        .min_h_0()
        .w_full()
        .flex()
        .flex_row()
        .overflow_hidden()
}

/// 状态栏行：固定高度、左右内边距、上边框、次级文字。
pub(super) fn status_row() -> Div {
    div()
        .h(px(STATUSBAR_HEIGHT))
        .w_full()
        .flex()
        .items_center()
        .gap_4()
        .px_3()
        .bg(rgb(tokens::SURFACE))
        .border_t_1()
        .border_color(rgb(tokens::BORDER))
        .text_xs()
        .text_color(rgb(tokens::SECONDARY))
        .overflow_hidden()
}

/// 工具栏左侧应用品牌区：透明 PNG 图标与产品名，不二次加白底或圆角。
pub(super) fn brand(icon: Arc<Image>, name: SharedString) -> Div {
    div()
        .flex()
        .items_center()
        .gap_2()
        .pr_3()
        .child(
            img(icon)
                .w(px(18.0))
                .h(px(18.0))
                .object_fit(ObjectFit::Contain),
        )
        .child(
            div()
                .text_size(px(15.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(rgb(tokens::TEXT))
                .child(name),
        )
}
