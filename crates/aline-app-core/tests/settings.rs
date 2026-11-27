//! 全局设置模型：PRD §5.13 默认值、损坏/旧值回退、分区重置与双语诊断键。

use aline_app_core::{
    AppearanceMode, CatalogError, DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES, EditorOverrides,
    EffectiveHighlight, EffectiveSettings, HighlightModeChoice, HighlightOverrides,
    HighlightThemeChoice, LARGE_SOURCE_REASON_KEY, LanguagePreference, Locale, PluginOverrides,
    Section, Settings, SettingsField, SettingsIssue, SettingsIssueReason, ThemeFamily, Translator,
};

const FULL_OVERRIDES: &str = r#"
[general]
language = "zh-CN"
autosave = false

[appearance]
theme = "vue"
mode = "sepia"

[editor]
body_wrap = false
source_wrap = false
code_wrap = true
large_document_source_threshold_bytes = 8388608

[images]
directory = "../assets images"

[plugins]
mermaid = false
export = false

[highlight]
code_theme = "feishu"
code_mode = "dark"
"#;

#[test]
fn defaults_follow_frozen_product_values() {
    let settings = Settings::default();
    let effective = settings.effective();
    assert_eq!(effective, EffectiveSettings::default());
    assert_eq!(effective.general.language, LanguagePreference::FollowSystem);
    assert!(effective.general.autosave);
    assert_eq!(effective.appearance.theme, ThemeFamily::GitHub);
    assert_eq!(effective.appearance.mode, AppearanceMode::FollowSystem);
    assert!(effective.editor.body_wrap);
    assert!(effective.editor.source_wrap);
    assert!(!effective.editor.code_wrap);
    assert_eq!(
        effective.editor.large_document_source_threshold_bytes,
        4 * 1024 * 1024
    );
    assert_eq!(
        effective.editor.large_document_source_threshold_bytes,
        DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES
    );
    assert_eq!(effective.images.directory, "./assets");
    assert!(effective.plugins.mermaid);
    assert!(effective.plugins.table);
    assert!(effective.plugins.export);
    assert!(effective.plugins.math);
    assert!(effective.plugins.highlight);
    assert!(effective.plugins.image_preview);
    assert_eq!(LARGE_SOURCE_REASON_KEY, "reading.large_source");
    for section in Section::ALL {
        for field in section.fields() {
            assert!(!settings.is_overridden(*field), "{field:?}");
            assert_eq!(field.section(), section);
        }
    }
    assert_eq!(settings.to_toml_string(), "");
}

#[test]
fn highlight_section_is_the_last_plugin_specific_section() {
    assert_eq!(
        Section::ALL,
        [
            Section::General,
            Section::Appearance,
            Section::Editor,
            Section::Images,
            Section::Plugins,
            Section::Highlight,
        ]
    );
    assert_eq!(Section::Highlight.name(), "highlight");
    assert_eq!(
        Section::Highlight.fields(),
        [
            SettingsField::HighlightCodeTheme,
            SettingsField::HighlightCodeMode
        ]
    );
    assert_eq!(SettingsField::HighlightCodeTheme.key(), "code_theme");
    assert_eq!(SettingsField::HighlightCodeMode.key(), "code_mode");
    assert_eq!(
        SettingsField::HighlightCodeTheme.section(),
        Section::Highlight
    );
    assert_eq!(
        SettingsField::HighlightCodeMode.section(),
        Section::Highlight
    );
}

#[test]
fn highlight_choices_default_to_follow_app_and_resolve_against_current_appearance() {
    let effective = Settings::default().effective();
    assert_eq!(effective.highlight, EffectiveHighlight::default());
    assert_eq!(
        effective.highlight.code_theme,
        HighlightThemeChoice::FollowApp
    );
    assert_eq!(
        effective.highlight.code_mode,
        HighlightModeChoice::FollowApp
    );
    assert_eq!(
        effective.highlight.resolve_theme(ThemeFamily::Vue),
        ThemeFamily::Vue
    );
    assert_eq!(
        effective.highlight.resolve_mode(AppearanceMode::Dark),
        AppearanceMode::Dark
    );
}

#[test]
fn fixed_highlight_dimensions_ignore_later_app_appearance_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let load = Settings::load("[highlight]\ncode_theme = \"notion\"\ncode_mode = \"sepia\"\n");
    assert!(load.issues.is_empty(), "{:?}", load.issues);
    assert!(
        load.settings
            .is_overridden(SettingsField::HighlightCodeTheme)
    );
    assert!(
        load.settings
            .is_overridden(SettingsField::HighlightCodeMode)
    );
    assert_eq!(
        load.settings.highlight.code_theme,
        Some(HighlightThemeChoice::Fixed(ThemeFamily::Notion))
    );
    assert_eq!(
        load.settings.highlight.code_mode,
        Some(HighlightModeChoice::Sepia)
    );
    let effective = load.settings.effective();
    assert_eq!(
        effective.highlight.resolve_theme(ThemeFamily::Vue),
        ThemeFamily::Notion
    );
    assert_eq!(
        effective.highlight.resolve_mode(AppearanceMode::Light),
        AppearanceMode::Sepia
    );
    Ok(())
}

#[test]
fn explicit_follow_app_tokens_are_recorded_as_overrides() -> Result<(), Box<dyn std::error::Error>>
{
    let load =
        Settings::load("[highlight]\ncode_theme = \"follow-app\"\ncode_mode = \"follow-app\"\n");
    assert!(load.issues.is_empty(), "{:?}", load.issues);
    assert!(
        load.settings
            .is_overridden(SettingsField::HighlightCodeTheme)
    );
    assert!(
        load.settings
            .is_overridden(SettingsField::HighlightCodeMode)
    );
    assert_eq!(
        load.settings.highlight.code_theme,
        Some(HighlightThemeChoice::FollowApp)
    );
    assert_eq!(
        load.settings.highlight.code_mode,
        Some(HighlightModeChoice::FollowApp)
    );
    assert_eq!(
        load.settings.to_toml_string(),
        "[highlight]\ncode_theme = \"follow-app\"\ncode_mode = \"follow-app\"\n"
    );
    Ok(())
}

#[test]
fn invalid_highlight_values_fall_back_per_field() {
    let load = Settings::load("[highlight]\ncode_theme = \"system\"\ncode_mode = \"auto\"\n");
    assert_eq!(load.settings, Settings::default());
    assert_eq!(
        load.issues,
        vec![
            SettingsIssue {
                field: Some(SettingsField::HighlightCodeTheme),
                reason: SettingsIssueReason::InvalidValue,
            },
            SettingsIssue {
                field: Some(SettingsField::HighlightCodeMode),
                reason: SettingsIssueReason::InvalidValue,
            },
        ]
    );
    let typed = Settings::load("[highlight]\ncode_theme = 1\n");
    assert_eq!(
        typed.issues,
        vec![SettingsIssue {
            field: Some(SettingsField::HighlightCodeTheme),
            reason: SettingsIssueReason::InvalidValue,
        }]
    );
}

#[test]
fn wrong_highlight_section_shape_reports_each_field() {
    let load = Settings::load("highlight = 1\n");
    assert_eq!(load.settings, Settings::default());
    assert_eq!(
        load.issues,
        vec![
            SettingsIssue {
                field: Some(SettingsField::HighlightCodeTheme),
                reason: SettingsIssueReason::InvalidValue,
            },
            SettingsIssue {
                field: Some(SettingsField::HighlightCodeMode),
                reason: SettingsIssueReason::InvalidValue,
            },
        ]
    );
}

#[test]
fn explicit_overrides_resolve_and_round_trip() -> Result<(), Box<dyn std::error::Error>> {
    let load = Settings::load(FULL_OVERRIDES);
    assert!(load.issues.is_empty(), "{:?}", load.issues);
    let effective = load.settings.effective();
    assert_eq!(effective.general.language, LanguagePreference::ZhCn);
    assert!(!effective.general.autosave);
    assert_eq!(effective.appearance.theme, ThemeFamily::Vue);
    assert_eq!(effective.appearance.mode, AppearanceMode::Sepia);
    assert!(!effective.editor.body_wrap);
    assert!(!effective.editor.source_wrap);
    assert!(effective.editor.code_wrap);
    assert_eq!(
        effective.editor.large_document_source_threshold_bytes,
        8 * 1024 * 1024
    );
    assert_eq!(effective.images.directory, "../assets images");
    assert!(!effective.plugins.mermaid);
    assert!(!effective.plugins.export);
    assert!(effective.plugins.table);
    assert_eq!(
        effective.highlight.code_theme,
        HighlightThemeChoice::Fixed(ThemeFamily::Feishu)
    );
    assert_eq!(effective.highlight.code_mode, HighlightModeChoice::Dark);
    assert_eq!(
        effective
            .highlight
            .resolve_theme(effective.appearance.theme),
        ThemeFamily::Feishu
    );
    assert_eq!(
        effective.highlight.resolve_mode(effective.appearance.mode),
        AppearanceMode::Dark
    );
    let written = load.settings.to_toml_string();
    let reloaded = Settings::load(&written);
    assert!(reloaded.issues.is_empty(), "{:?}", reloaded.issues);
    assert_eq!(reloaded.settings, load.settings);
    Ok(())
}

#[test]
fn corrupted_document_is_unreadable_but_startup_stays_usable() {
    let load = Settings::load("[general\nlanguage = \"en\"\n");
    assert_eq!(load.settings, Settings::default());
    assert_eq!(
        load.issues,
        vec![SettingsIssue {
            field: None,
            reason: SettingsIssueReason::Unreadable,
        }]
    );
    assert_eq!(load.settings.effective(), EffectiveSettings::default());
}

#[test]
fn wrong_section_shape_reports_each_field_and_keeps_defaults() {
    let load = Settings::load("general = 1\n");
    assert_eq!(load.settings, Settings::default());
    assert_eq!(
        load.issues,
        vec![
            SettingsIssue {
                field: Some(SettingsField::Language),
                reason: SettingsIssueReason::InvalidValue,
            },
            SettingsIssue {
                field: Some(SettingsField::Autosave),
                reason: SettingsIssueReason::InvalidValue,
            },
        ]
    );
}

#[test]
fn invalid_and_old_values_fall_back_per_field_without_blocking_startup() {
    let source = r#"
[general]
language = "zh_CN"
autosave = false
legacy_theme = "solarized"

[appearance]
mode = "auto"
theme = "notion"

[editor]
large_document_source_threshold_bytes = -1
"#;
    let load = Settings::load(source);
    assert_eq!(load.settings.general.language, None);
    assert_eq!(load.settings.general.autosave, Some(false));
    assert_eq!(load.settings.appearance.mode, None);
    assert_eq!(load.settings.appearance.theme, Some(ThemeFamily::Notion));
    assert_eq!(
        load.settings.editor.large_document_source_threshold_bytes,
        None
    );
    assert_eq!(
        load.issues,
        vec![
            SettingsIssue {
                field: Some(SettingsField::Language),
                reason: SettingsIssueReason::InvalidValue,
            },
            SettingsIssue {
                field: Some(SettingsField::Mode),
                reason: SettingsIssueReason::InvalidValue,
            },
            SettingsIssue {
                field: Some(SettingsField::LargeDocumentSourceThreshold),
                reason: SettingsIssueReason::InvalidValue,
            },
        ]
    );
    let effective = load.settings.effective();
    assert_eq!(effective.general.language, LanguagePreference::FollowSystem);
    assert_eq!(effective.appearance.mode, AppearanceMode::FollowSystem);
    assert_eq!(effective.appearance.theme, ThemeFamily::Notion);
    assert_eq!(
        effective.editor.large_document_source_threshold_bytes,
        DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES
    );
}

#[test]
fn section_reset_clears_only_that_section() {
    let mut settings = Settings::load(FULL_OVERRIDES).settings;
    settings.reset_section(Section::Editor);
    assert_eq!(settings.editor, EditorOverrides::default());
    assert_eq!(settings.general.language, Some(LanguagePreference::ZhCn));
    assert_eq!(settings.appearance.theme, Some(ThemeFamily::Vue));
    assert_eq!(
        settings.images.directory.as_deref(),
        Some("../assets images")
    );
    assert_eq!(settings.plugins.mermaid, Some(false));
    let effective = settings.effective();
    assert_eq!(
        effective.editor.large_document_source_threshold_bytes,
        DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES
    );
    assert!(effective.editor.body_wrap);
    assert!(!effective.general.autosave);
    settings.reset_section(Section::Plugins);
    assert_eq!(settings.plugins, PluginOverrides::default());
    assert_eq!(settings.general.language, Some(LanguagePreference::ZhCn));
    assert_eq!(
        settings.highlight.code_theme,
        Some(HighlightThemeChoice::Fixed(ThemeFamily::Feishu))
    );
    settings.reset_section(Section::Highlight);
    assert_eq!(settings.highlight, HighlightOverrides::default());
    assert_eq!(settings.general.language, Some(LanguagePreference::ZhCn));
    settings.reset_section(Section::General);
    settings.reset_section(Section::Appearance);
    settings.reset_section(Section::Images);
    assert_eq!(settings, Settings::default());
}

#[test]
fn large_document_threshold_is_configurable_and_keeps_reason_key() {
    let load = Settings::load("[editor]\nlarge_document_source_threshold_bytes = 1048576\n");
    assert!(load.issues.is_empty(), "{:?}", load.issues);
    assert_eq!(
        load.settings
            .effective()
            .editor
            .large_document_source_threshold_bytes,
        1024 * 1024
    );
    let zero = Settings::load("[editor]\nlarge_document_source_threshold_bytes = 0\n");
    assert!(zero.issues.is_empty(), "{:?}", zero.issues);
    assert_eq!(
        zero.settings
            .effective()
            .editor
            .large_document_source_threshold_bytes,
        0
    );
    assert_eq!(LARGE_SOURCE_REASON_KEY, "reading.large_source");
}

#[test]
fn serialization_writes_only_explicit_overrides() {
    let mut settings = Settings::default();
    settings.plugins.export = Some(false);
    assert_eq!(settings.to_toml_string(), "[plugins]\nexport = false\n");
    settings.images.directory = Some(String::from("C:\\图 片\\\"assets\""));
    settings.highlight.code_theme = Some(HighlightThemeChoice::Fixed(ThemeFamily::Feishu));
    settings.highlight.code_mode = Some(HighlightModeChoice::Dark);
    let text = settings.to_toml_string();
    assert!(
        text.ends_with("[highlight]\ncode_theme = \"feishu\"\ncode_mode = \"dark\"\n"),
        "{text}"
    );
    let reloaded = Settings::load(&text);
    assert!(reloaded.issues.is_empty(), "{:?}", reloaded.issues);
    assert_eq!(reloaded.settings, settings);
}

#[test]
fn settings_issue_keys_are_bilingual_and_placeholders_interpolate()
-> Result<(), Box<dyn std::error::Error>> {
    for locale in [Locale::ZhCn, Locale::En] {
        let translator = Translator::embedded(locale)?;
        assert!(
            !translator
                .t(SettingsIssueReason::Unreadable.message_key())?
                .trim()
                .is_empty()
        );
        let key = SettingsIssueReason::InvalidValue.message_key();
        assert!(matches!(
            translator.t(key),
            Err(CatalogError::InterpolationMismatch(_))
        ));
        let label = match locale {
            Locale::ZhCn => "语言",
            Locale::En => "Language",
        };
        let rendered = translator.t_with(key, &[("field", label)])?;
        assert!(rendered.contains(label), "{locale:?}: {rendered}");
    }
    Ok(())
}
