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
                // 带 `{count}` 插值的键由下方插值测试单独覆盖。
                "status.pending",
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

#[test]
fn status_counts_interpolate_the_single_count_placeholder() {
    for locale in [Locale::ZhCn, Locale::En] {
        let translator = Translator::embedded(locale);
        assert!(
            translator.is_ok(),
            "catalog must be embedded for {locale:?}"
        );
        if let Ok(translator) = translator {
            for key in ["status.characters", "status.lines"] {
                let translated = translator.t_with(key, &[("count", "12")]);
                assert!(
                    matches!(translated, Ok(value) if value.contains("12") && !value.contains('{')),
                    "missing count interpolation for {key} in {locale:?}"
                );
                assert!(
                    translator.t_with(key, &[]).is_err(),
                    "count placeholder must be required for {key} in {locale:?}"
                );
            }
            let pending = translator.t("status.pending");
            assert!(
                matches!(pending, Ok(value) if !value.contains('{') && !value.trim().is_empty()),
                "status.pending must be a literal in {locale:?}"
            );
        }
    }
}
