//! 全局设置的纯模型：默认值、分区覆盖、有效值解析与内部持久化格式。
//!
//! 本模块零 GPUI、无线程、无全局状态；文件读写与发布由应用宿主负责。
//! 只保存用户显式覆盖，读取失败按字段回退并给出类型化诊断。

mod model;
mod validate;

pub use model::{
    AppearanceMode, AppearanceOverrides, DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES,
    EditorOverrides, EffectiveAppearance, EffectiveEditor, EffectiveGeneral, EffectiveHighlight,
    EffectiveImages, EffectivePlugins, EffectiveSettings, GeneralOverrides, HighlightModeChoice,
    HighlightOverrides, HighlightThemeChoice, ImageOverrides, LARGE_SOURCE_REASON_KEY,
    LanguagePreference, PluginOverrides, Section, Settings, SettingsField, ThemeFamily,
};
pub use validate::{SettingsIssue, SettingsIssueReason, SettingsLoad};
