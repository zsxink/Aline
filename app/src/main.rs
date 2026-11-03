//! M1 应用入口；系统语言检测留在 app，原生窗口装配交给 aline-ui。

use std::process::ExitCode;

use aline_app_core::Locale;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct LocaleSelection {
    locale: Locale,
    fallback_used: bool,
}

fn system_locale() -> LocaleSelection {
    select_locale(sys_locale::get_locales())
}

fn select_locale<T: AsRef<str>>(tags: impl IntoIterator<Item = T>) -> LocaleSelection {
    let locale = tags
        .into_iter()
        .next()
        .and_then(|tag| locale_from_tag(tag.as_ref()));
    LocaleSelection {
        locale: locale.unwrap_or(Locale::En),
        fallback_used: locale.is_none(),
    }
}

fn locale_from_tag(tag: &str) -> Option<Locale> {
    let language = tag.split(['-', '_']).next()?;
    if language.eq_ignore_ascii_case("zh") {
        Some(Locale::ZhCn)
    } else if language.eq_ignore_ascii_case("en") {
        Some(Locale::En)
    } else {
        None
    }
}

fn main() -> ExitCode {
    let selection = system_locale();
    match aline_ui::run_application(selection.locale, selection.fallback_used) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod locale_tests {
    use super::{LocaleSelection, locale_from_tag, select_locale, system_locale};
    use aline_app_core::Locale;

    #[test]
    fn language_tags_map_to_the_supported_catalogs() {
        assert_eq!(locale_from_tag("zh-Hans-CN"), Some(Locale::ZhCn));
        assert_eq!(locale_from_tag("zh_CN"), Some(Locale::ZhCn));
        assert_eq!(locale_from_tag("en-US"), Some(Locale::En));
        assert_eq!(locale_from_tag("fr-FR"), None);
        assert_eq!(locale_from_tag(""), None);
    }

    #[test]
    fn unsupported_first_preference_uses_visible_english_fallback() {
        assert_eq!(
            select_locale(["fr-FR", "zh-Hans-CN", "en-US"]),
            LocaleSelection {
                locale: Locale::En,
                fallback_used: true,
            }
        );
        assert_eq!(
            select_locale(["zh-Hans-CN", "en-US"]),
            LocaleSelection {
                locale: Locale::ZhCn,
                fallback_used: false,
            }
        );
        assert_eq!(
            select_locale(std::iter::empty::<&str>()),
            LocaleSelection {
                locale: Locale::En,
                fallback_used: true,
            }
        );
    }

    #[test]
    fn default_locale_reads_the_system_provider() {
        let tags = sys_locale::get_locales().collect::<Vec<_>>();
        let expected = tags.first().and_then(|tag| locale_from_tag(tag));
        assert_eq!(
            system_locale(),
            LocaleSelection {
                locale: expected.unwrap_or(Locale::En),
                fallback_used: expected.is_none(),
            }
        );
    }
}
