//! M1 首个 macOS 原生窗口；内容保持最小只读空状态。

use std::rc::Rc;
use std::time::Duration;

use aline_app_core::{Locale, Translator};
use gpui::{App, Application, Platform, QuitMode, SharedString};

use crate::native_window::{NativeWindowCopy, open_initial_window};

/// 显示复检延迟。本轮选定值，不是平台常量：显示器睡眠、Spaces 切换或远程会话
/// 建立初期，显示枚举可能瞬时为空，立即判定会把「暂时看不到」误判成「服务不可用」。
const DISPLAY_RECHECK_DELAY: Duration = Duration::from_secs(2);

pub(super) fn run(locale: Locale, fallback_used: bool) -> Result<(), String> {
    let translator = Translator::embedded(locale).map_err(|error| error.to_string())?;
    let window_failed = translator
        .t("startup.window_failed")
        .map_err(|error| error.to_string())?;
    let no_display = translator
        .t("startup.no_display")
        .map_err(|error| error.to_string())?;
    let locale_fallback = if fallback_used {
        Some(SharedString::from(
            translator
                .t("startup.language_fallback")
                .map_err(|error| error.to_string())?,
        ))
    } else {
        None
    };
    let copy = NativeWindowCopy::new(translator)?;
    let platform: Rc<dyn Platform> = Rc::new(gpui_macos::MacPlatform::new(false));
    // 检测必须在 `Application::run` 之前：窗口服务不可达时的可观察事实是
    // 「无窗口、无 stderr、进程不退」，无法区分启动回调是否执行过，因此按可观察
    // 行为选择更早的检测点，而不假设回调不会被调用。
    let first = platform.displays().len();
    if StartupDisplay::of(first) == StartupDisplay::Recheck {
        std::thread::sleep(DISPLAY_RECHECK_DELAY);
        if startup_failure(first, platform.displays().len()) {
            report_startup_failure(&no_display);
        }
    }
    Application::with_platform(platform)
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| {
            // 后端诊断只用于判断成败，不拼进用户可见文案（铁律 7）。
            if open_initial_window(cx, copy, locale_fallback).is_err() {
                report_startup_failure(&window_failed);
            }
            cx.activate(true);
        });
    Ok(())
}

/// 启动期显示判定结果。纯规则，便于不启动窗口地验证两条分支。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StartupDisplay {
    /// 至少有一个活跃显示，正常进入建窗路径。
    Ready,
    /// 没有活跃显示，需要延迟复检。
    Recheck,
}

impl StartupDisplay {
    fn of(active_display_count: usize) -> Self {
        if active_display_count == 0 {
            Self::Recheck
        } else {
            Self::Ready
        }
    }
}

/// 首查与复检两次计数 → 是否判定启动失败。只有「首查为空且复检仍为空」才是失败。
fn startup_failure(first_count: usize, recheck_count: usize) -> bool {
    first_count == 0 && recheck_count == 0
}

/// 失败出口只输出本地化文案：AGENTS 铁律 7 要求用户可见字符串走 `t("key")`，
/// 因此不再拼接英文技术诊断（2026-11-12 裁决）。
fn report_startup_failure(message: &str) -> ! {
    eprintln!("{message}");
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    use super::{StartupDisplay, startup_failure};

    #[test]
    fn startup_display_with_active_display_skips_recheck() {
        assert_eq!(StartupDisplay::of(1), StartupDisplay::Ready);
        assert_eq!(StartupDisplay::of(3), StartupDisplay::Ready);
    }

    #[test]
    fn startup_display_without_active_display_requests_recheck() {
        assert_eq!(StartupDisplay::of(0), StartupDisplay::Recheck);
    }

    #[test]
    fn recheck_without_active_display_is_failure() {
        assert!(startup_failure(0, 0));
    }

    #[test]
    fn recheck_with_active_display_is_not_failure() {
        assert!(!startup_failure(0, 1));
        assert!(!startup_failure(0, 2));
    }

    /// 编排断言：首查非空时不会进入延迟分支，也就不可能判失败。
    #[test]
    fn active_first_check_never_fails_even_if_recheck_is_empty() {
        assert!(!startup_failure(1, 0));
        assert!(!startup_failure(2, 0));
    }
}
