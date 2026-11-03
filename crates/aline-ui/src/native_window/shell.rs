//! 主窗三段骨架的元素装配：工具栏行、内容区、状态栏行与表面配色。
//!
//! 几何一律取自 `geometry.rs`，这里不持有第二套数值，也不持有窗口状态；
//! 交互监听由 `native_window.rs` 装配。配色使用 UI §3.1 已确认的 GitHub 白天
//! 语义色：画布、正文、次级文字、边框与强调色。原型状态栏的米白表面不在已确认
//! 色值内，因此状态栏沿用画布色并以边框和次级文字区分，具体表面关系待 V-09。

use std::sync::Arc;

use gpui::{Div, Image, ImageFormat, ObjectFit, SharedString, div, img, prelude::*, px, rgb};

use super::geometry::{STATUSBAR_HEIGHT, TOOLBAR_HEIGHT};

/// 已确认的画布色（UI §3.1）。
pub(super) const SURFACE: u32 = 0xFFFFFF;

/// 已确认的正文色（UI §3.1）。
pub(super) const TEXT: u32 = 0x1F2328;

/// 已确认的次级文字色（UI §3.1）。
pub(super) const SECONDARY: u32 = 0x59636E;

/// 已确认的边框色（UI §3.1）。
pub(super) const BORDER: u32 = 0xD1D9E0;

/// 已确认的强调色（UI §3.1）。
pub(super) const ACCENT: u32 = 0x0969DA;

/// 强调色的弱背景，对应原型 `.icon-btn.active` 的 `--accent-soft`（10% 强调色）。
pub(super) const ACCENT_SOFT: u32 = 0x0969DA1A;

/// 中性悬停背景，对应原型 `--hover`（5% 黑色）。
pub(super) const HOVER: u32 = 0x0000000D;

/// 中性提示表面，沿用原型中滚动条槽位已有的浅灰。
pub(super) const NOTICE_SURFACE: u32 = 0xF6F8FA;

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
        .bg(rgb(SURFACE))
        .border_b_1()
        .border_color(rgb(BORDER))
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
        .bg(rgb(SURFACE))
        .border_t_1()
        .border_color(rgb(BORDER))
        .text_xs()
        .text_color(rgb(SECONDARY))
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
                .text_color(rgb(TEXT))
                .child(name),
        )
}
