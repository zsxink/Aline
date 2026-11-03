//! M1 空窗口展示与目标平台反馈使用的最小双语键集合。

use aline_app_core::{Locale, Translator};

#[test]
fn native_window_copy_exists_in_both_catalogs() {
    for locale in [Locale::ZhCn, Locale::En] {
        let translator = Translator::embedded(locale);
        assert!(
            translator.is_ok(),
            "catalog must be embedded for {locale:?}"
        );
        if let Ok(translator) = translator {
            for key in [
                "window.empty",
                "window.selecting",
                "window.loading",
                "window.readonly",
                "action.open",
                "action.close_document",
                "action.visual_view",
                "action.source_view",
                "menu.file",
                "open_error.selection",
                "open_error.extension",
                "open_error.not_regular",
                "open_error.budget",
                "open_error.io",
                "open_error.decode",
                "open_error.internal",
                "open_error.dialog",
                "app.platform_unavailable",
                "startup.window_failed",
                "startup.language_fallback",
                "reading.large_source",
                "reading.layout_budget",
                "action.cancel_navigation",
            ] {
                let translated = translator.t(key);
                assert!(
                    matches!(translated, Ok(value) if !value.trim().is_empty()),
                    "missing or empty {key} in {locale:?}"
                );
            }
        }
    }
}
