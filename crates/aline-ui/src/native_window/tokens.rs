//! M1 GitHub 白天表面的私有语义色；不是 M4 主题包或已冻结的完整调色板。
//!
//! 原始值只在 `palette` 定义；组件按用途消费外层常量。提取时保留现有数值与
//! RGB/RGBA 转换，未将原型候选色提升为产品决定。

mod palette {
    // UI §3.1 / PRD §6.2 已确认的 GitHub 白天画布、正文、次级、边框、强调、代码色。
    pub(super) const WHITE: u32 = 0xFFFFFF;
    pub(super) const GRAY_900: u32 = 0x1F2328;
    pub(super) const GRAY_600: u32 = 0x59636E;
    pub(super) const GRAY_300: u32 = 0xD1D9E0;
    pub(super) const BLUE_500: u32 = 0x0969DA;
    pub(super) const GRAY_050: u32 = 0xF6F8FA;
    // 原型 light.css 的 --accent-soft / --hover；保留既有 8 位 RGBA 量化值。
    pub(super) const BLUE_500_ALPHA_10: u32 = 0x0969DA1A;
    pub(super) const BLACK_ALPHA_05: u32 = 0x0000000D;
    // 原型 .editor mark 和五类 Alert；当前 M1 实验值，不冻结正式主题字段。
    pub(super) const YELLOW_100: u32 = 0xFFF8C5;
    pub(super) const GREEN_600: u32 = 0x1A7F37;
    pub(super) const PURPLE_500: u32 = 0x8250DF;
    pub(super) const YELLOW_700: u32 = 0x9A6700;
    pub(super) const RED_500: u32 = 0xCF222E;
    // 阅读器既有滚动条滑块候选色；V-09 未冻结该值。
    pub(super) const GRAY_500: u32 = 0x8C959F;
    // 窗口既有 RGBA 全透明边框，保留控件边框几何。
    pub(super) const TRANSPARENT: u32 = 0x00000000;
}

pub(super) const SURFACE: u32 = palette::WHITE;
pub(super) const TEXT: u32 = palette::GRAY_900;
pub(super) const SECONDARY: u32 = palette::GRAY_600;
pub(super) const BORDER: u32 = palette::GRAY_300;
pub(super) const ACCENT: u32 = palette::BLUE_500;
pub(super) const ACCENT_SOFT: u32 = palette::BLUE_500_ALPHA_10;
pub(super) const HOVER: u32 = palette::BLACK_ALPHA_05;
pub(super) const NOTICE_SURFACE: u32 = palette::GRAY_050;
pub(super) const CODE_SURFACE: u32 = palette::GRAY_050;
pub(super) const MARK_SURFACE: u32 = palette::YELLOW_100;
pub(super) const ALERT_NOTE: u32 = palette::BLUE_500;
pub(super) const ALERT_TIP: u32 = palette::GREEN_600;
pub(super) const ALERT_IMPORTANT: u32 = palette::PURPLE_500;
pub(super) const ALERT_WARNING: u32 = palette::YELLOW_700;
pub(super) const ALERT_CAUTION: u32 = palette::RED_500;
pub(super) const LINK: u32 = palette::BLUE_500;
pub(super) const SCROLLBAR_RAIL: u32 = palette::GRAY_050;
pub(super) const SCROLLBAR_THUMB: u32 = palette::GRAY_500;
pub(super) const TRANSPARENT_BORDER: u32 = palette::TRANSPARENT;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_values_preserve_m1_pre_refactor_palette() {
        // 独立预期来自提取前的 shell/reader/window 常量，不从 palette 派生。
        assert_eq!(
            [
                SURFACE,
                TEXT,
                SECONDARY,
                BORDER,
                ACCENT,
                ACCENT_SOFT,
                HOVER,
                NOTICE_SURFACE,
                CODE_SURFACE,
                MARK_SURFACE,
                ALERT_NOTE,
                ALERT_TIP,
                ALERT_IMPORTANT,
                ALERT_WARNING,
                ALERT_CAUTION,
                LINK,
                SCROLLBAR_RAIL,
                SCROLLBAR_THUMB,
                TRANSPARENT_BORDER,
            ],
            [
                0xFFFFFF, 0x1F2328, 0x59636E, 0xD1D9E0, 0x0969DA, 0x0969DA1A, 0x0000000D, 0xF6F8FA,
                0xF6F8FA, 0xFFF8C5, 0x0969DA, 0x1A7F37, 0x8250DF, 0x9A6700, 0xCF222E, 0x0969DA,
                0xF6F8FA, 0x8C959F, 0x00000000,
            ]
        );
    }
}
