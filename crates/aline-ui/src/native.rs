//! M1 首个 macOS 原生窗口；内容保持最小只读空状态。

use aline_app_core::{Locale, Translator};
use gpui::{App, Application, QuitMode, SharedString};

use crate::native_window::{NativeWindowCopy, open_initial_window};

pub(super) fn run(locale: Locale, fallback_used: bool) -> Result<(), String> {
    let translator = Translator::embedded(locale).map_err(|error| error.to_string())?;
    let window_failed = translator
        .t("startup.window_failed")
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
    Application::with_platform(std::rc::Rc::new(gpui_macos::MacPlatform::new(false)))
        .with_quit_mode(QuitMode::LastWindowClosed)
        .run(move |cx: &mut App| {
            if let Err(diagnostic) = open_initial_window(cx, copy, locale_fallback) {
                report_startup_failure(&window_failed, &diagnostic);
            }
            cx.activate(true);
        });
    Ok(())
}

fn report_startup_failure(window_failed: &str, diagnostic: &str) -> ! {
    eprintln!("{window_failed}: {diagnostic}");
    std::process::exit(1)
}
