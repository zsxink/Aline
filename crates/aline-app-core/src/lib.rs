//! 显式配置与 catalog 的纯模型，不持有 GPUI 或全局语言状态。

mod i18n;
mod settings;

pub use i18n::{CatalogError, Locale, Translator};
pub use settings::{
    AppearanceMode, AppearanceOverrides, DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES,
    EditorOverrides, EffectiveAppearance, EffectiveEditor, EffectiveGeneral, EffectiveHighlight,
    EffectiveImages, EffectivePlugins, EffectiveSettings, GeneralOverrides, HighlightModeChoice,
    HighlightOverrides, HighlightThemeChoice, ImageOverrides, LARGE_SOURCE_REASON_KEY,
    LanguagePreference, PluginOverrides, Section, Settings, SettingsField, SettingsIssue,
    SettingsIssueReason, SettingsLoad, ThemeFamily,
};
