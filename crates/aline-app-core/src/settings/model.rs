//! 全局设置的类型、PRD §5.13 冻结默认值与分区归属。
//!
//! 这里只保存**用户显式覆盖**：`None` 表示沿用默认值，因此升级时可以调整
//! 默认值而不覆盖用户选择。窗口布局是各窗口独立状态，不属于全局设置。

/// 大文档自动进入源码视图的默认阈值：4 MiB。
pub const DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES: u64 = 4 * 1024 * 1024;

/// 大文档自动进入源码视图的原因提示键，由 UI 经 `t("key")` 本地化。
pub const LARGE_SOURCE_REASON_KEY: &str = "reading.large_source";

/// 界面语言偏好；`FollowSystem` 表示随系统首选语言。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LanguagePreference {
    /// 随系统首选语言（zh 映射 zh-CN，其余回退 en）。
    FollowSystem,
    /// 简体中文。
    ZhCn,
    /// 英语。
    En,
}

impl LanguagePreference {
    /// 内部持久化标识。
    pub const fn token(self) -> &'static str {
        match self {
            Self::FollowSystem => "system",
            Self::ZhCn => "zh-CN",
            Self::En => "en",
        }
    }

    /// 从持久化标识解析；未知标识返回 `None`，由调用方回退默认值。
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "system" => Some(Self::FollowSystem),
            "zh-CN" => Some(Self::ZhCn),
            "en" => Some(Self::En),
            _ => None,
        }
    }
}

/// 内置内容主题家族。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThemeFamily {
    /// GitHub。
    GitHub,
    /// Notion。
    Notion,
    /// Vue。
    Vue,
    /// 飞书。
    Feishu,
}

impl ThemeFamily {
    /// 内部持久化标识。
    pub const fn token(self) -> &'static str {
        match self {
            Self::GitHub => "github",
            Self::Notion => "notion",
            Self::Vue => "vue",
            Self::Feishu => "feishu",
        }
    }

    /// 从持久化标识解析；未知标识返回 `None`。
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "github" => Some(Self::GitHub),
            "notion" => Some(Self::Notion),
            "vue" => Some(Self::Vue),
            "feishu" => Some(Self::Feishu),
            _ => None,
        }
    }
}

/// 外观模式；`FollowSystem` 只在白天/夜间之间跟随，护眼必须显式选择。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AppearanceMode {
    /// 跟随系统明暗。
    FollowSystem,
    /// 白天。
    Light,
    /// 夜间。
    Dark,
    /// 护眼。
    Sepia,
}

impl AppearanceMode {
    /// 内部持久化标识。
    pub const fn token(self) -> &'static str {
        match self {
            Self::FollowSystem => "system",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Sepia => "sepia",
        }
    }

    /// 从持久化标识解析；未知标识返回 `None`。
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "system" => Some(Self::FollowSystem),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "sepia" => Some(Self::Sepia),
            _ => None,
        }
    }
}

/// 代码主题维度：随应用主题家族，或固定某一内置家族。
///
/// 固定维度不被之后的应用主题变化覆盖；只有 `FollowApp` 才读取应用当前
/// 家族。主题包身份与用户导入主题归 5.1，这里只覆盖四个内置家族。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightThemeChoice {
    /// 随应用主题家族。
    FollowApp,
    /// 固定主题家族。
    Fixed(ThemeFamily),
}

impl HighlightThemeChoice {
    /// 内部持久化标识。
    pub const fn token(self) -> &'static str {
        match self {
            Self::FollowApp => "follow-app",
            Self::Fixed(family) => family.token(),
        }
    }

    /// 从持久化标识解析；未知标识返回 `None`。
    pub fn parse(token: &str) -> Option<Self> {
        if token == "follow-app" {
            return Some(Self::FollowApp);
        }
        ThemeFamily::parse(token).map(Self::Fixed)
    }

    /// 解析为实际主题家族；`FollowApp` 取应用当前家族。
    pub const fn resolve(self, app: ThemeFamily) -> ThemeFamily {
        match self {
            Self::FollowApp => app,
            Self::Fixed(family) => family,
        }
    }
}

/// 代码高亮模式维度：随应用外观模式，或固定亮、暗、护眼。
///
/// 与 `AppearanceMode` 分开，是因为“随应用”与应用自身的“随系统”不是同一
/// 记号：代码维度跟随的是应用外观**已解析**的具体模式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HighlightModeChoice {
    /// 随应用外观模式。
    FollowApp,
    /// 固定亮色。
    Light,
    /// 固定暗色。
    Dark,
    /// 固定护眼。
    Sepia,
}

impl HighlightModeChoice {
    /// 内部持久化标识。
    pub const fn token(self) -> &'static str {
        match self {
            Self::FollowApp => "follow-app",
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Sepia => "sepia",
        }
    }

    /// 从持久化标识解析；未知标识返回 `None`。
    pub fn parse(token: &str) -> Option<Self> {
        match token {
            "follow-app" => Some(Self::FollowApp),
            "light" => Some(Self::Light),
            "dark" => Some(Self::Dark),
            "sepia" => Some(Self::Sepia),
            _ => None,
        }
    }

    /// 解析为实际外观模式；`FollowApp` 取应用已解析的具体模式。
    pub const fn resolve(self, app: AppearanceMode) -> AppearanceMode {
        match self {
            Self::FollowApp => app,
            Self::Light => AppearanceMode::Light,
            Self::Dark => AppearanceMode::Dark,
            Self::Sepia => AppearanceMode::Sepia,
        }
    }
}

/// 可独立重置的全局设置分区。
///
/// UI 的“高级”分区在 V1 没有设置项，因此没有对应变体；不能为了分区存在
/// 就填入占位项。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Section {
    /// 通用。
    General,
    /// 外观。
    Appearance,
    /// 编辑器。
    Editor,
    /// 图片。
    Images,
    /// 插件开关（插件专属设置另立分区，见 `Highlight`）。
    Plugins,
    /// 代码语言高亮（插件专属分区，位于设置页底部）。
    Highlight,
}

impl Section {
    /// 全部分区，按设置页顺序。
    pub const ALL: [Self; 6] = [
        Self::General,
        Self::Appearance,
        Self::Editor,
        Self::Images,
        Self::Plugins,
        Self::Highlight,
    ];

    /// 内部持久化表名。
    pub const fn name(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Appearance => "appearance",
            Self::Editor => "editor",
            Self::Images => "images",
            Self::Plugins => "plugins",
            Self::Highlight => "highlight",
        }
    }

    /// 分区内的字段，按持久化顺序。
    pub const fn fields(self) -> &'static [SettingsField] {
        match self {
            Self::General => &[SettingsField::Language, SettingsField::Autosave],
            Self::Appearance => &[SettingsField::Theme, SettingsField::Mode],
            Self::Editor => &[
                SettingsField::BodyWrap,
                SettingsField::SourceWrap,
                SettingsField::CodeWrap,
                SettingsField::LargeDocumentSourceThreshold,
            ],
            Self::Images => &[SettingsField::ImageDirectory],
            Self::Plugins => &[
                SettingsField::MermaidEnabled,
                SettingsField::TableEnabled,
                SettingsField::ExportEnabled,
                SettingsField::MathEnabled,
                SettingsField::HighlightEnabled,
                SettingsField::ImagePreviewEnabled,
            ],
            Self::Highlight => &[
                SettingsField::HighlightCodeTheme,
                SettingsField::HighlightCodeMode,
            ],
        }
    }
}

/// 设置字段身份；用于覆盖查询、重置与逐字段降级诊断。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsField {
    /// 界面语言。
    Language,
    /// 自动保存开关。
    Autosave,
    /// 内容主题家族。
    Theme,
    /// 外观模式。
    Mode,
    /// 正文自动换行。
    BodyWrap,
    /// 源码视图自动换行。
    SourceWrap,
    /// 代码块自动换行。
    CodeWrap,
    /// 大文档自动进入源码视图的阈值（字节）。
    LargeDocumentSourceThreshold,
    /// 图片目录。
    ImageDirectory,
    /// Mermaid 开关。
    MermaidEnabled,
    /// 表格增强开关。
    TableEnabled,
    /// 导出开关。
    ExportEnabled,
    /// 公式开关。
    MathEnabled,
    /// 代码语言高亮开关。
    HighlightEnabled,
    /// 图片预览开关。
    ImagePreviewEnabled,
    /// 代码高亮的代码主题维度。
    HighlightCodeTheme,
    /// 代码高亮的高亮模式维度。
    HighlightCodeMode,
}

impl SettingsField {
    /// 内部持久化键。
    pub const fn key(self) -> &'static str {
        match self {
            Self::Language => "language",
            Self::Autosave => "autosave",
            Self::Theme => "theme",
            Self::Mode => "mode",
            Self::BodyWrap => "body_wrap",
            Self::SourceWrap => "source_wrap",
            Self::CodeWrap => "code_wrap",
            Self::LargeDocumentSourceThreshold => "large_document_source_threshold_bytes",
            Self::ImageDirectory => "directory",
            Self::MermaidEnabled => "mermaid",
            Self::TableEnabled => "table",
            Self::ExportEnabled => "export",
            Self::MathEnabled => "math",
            Self::HighlightEnabled => "highlight",
            Self::ImagePreviewEnabled => "image_preview",
            Self::HighlightCodeTheme => "code_theme",
            Self::HighlightCodeMode => "code_mode",
        }
    }

    /// 字段所属分区。
    pub const fn section(self) -> Section {
        match self {
            Self::Language | Self::Autosave => Section::General,
            Self::Theme | Self::Mode => Section::Appearance,
            Self::BodyWrap
            | Self::SourceWrap
            | Self::CodeWrap
            | Self::LargeDocumentSourceThreshold => Section::Editor,
            Self::ImageDirectory => Section::Images,
            Self::MermaidEnabled
            | Self::TableEnabled
            | Self::ExportEnabled
            | Self::MathEnabled
            | Self::HighlightEnabled
            | Self::ImagePreviewEnabled => Section::Plugins,
            Self::HighlightCodeTheme | Self::HighlightCodeMode => Section::Highlight,
        }
    }
}

/// 通用分区的显式覆盖。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct GeneralOverrides {
    /// 界面语言覆盖；`None` 表示随系统。
    pub language: Option<LanguagePreference>,
    /// 自动保存覆盖。
    pub autosave: Option<bool>,
}

/// 外观分区的显式覆盖。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AppearanceOverrides {
    /// 内容主题家族覆盖。
    pub theme: Option<ThemeFamily>,
    /// 外观模式覆盖。
    pub mode: Option<AppearanceMode>,
}

/// 编辑器分区的显式覆盖。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct EditorOverrides {
    /// 正文自动换行覆盖。
    pub body_wrap: Option<bool>,
    /// 源码视图自动换行覆盖。
    pub source_wrap: Option<bool>,
    /// 代码块自动换行覆盖。
    pub code_wrap: Option<bool>,
    /// 大文档自动进入源码视图的阈值覆盖（字节）。
    pub large_document_source_threshold_bytes: Option<u64>,
}

/// 图片分区的显式覆盖。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImageOverrides {
    /// 图片目录覆盖；默认 `./assets`，允许相对或绝对路径。
    pub directory: Option<String>,
}

/// 六个内置提供者的启用状态覆盖；默认全部开启。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PluginOverrides {
    /// Mermaid。
    pub mermaid: Option<bool>,
    /// 表格增强。
    pub table: Option<bool>,
    /// 导出。
    pub export: Option<bool>,
    /// 公式。
    pub math: Option<bool>,
    /// 代码语言高亮。
    pub highlight: Option<bool>,
    /// 图片预览。
    pub image_preview: Option<bool>,
}

/// 代码语言高亮分区的显式覆盖；两维分别独立解析。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HighlightOverrides {
    /// 代码主题维度覆盖。
    pub code_theme: Option<HighlightThemeChoice>,
    /// 高亮模式维度覆盖。
    pub code_mode: Option<HighlightModeChoice>,
}

/// 全局设置的显式覆盖集合；默认实例表示全部沿用默认值。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Settings {
    /// 通用分区覆盖。
    pub general: GeneralOverrides,
    /// 外观分区覆盖。
    pub appearance: AppearanceOverrides,
    /// 编辑器分区覆盖。
    pub editor: EditorOverrides,
    /// 图片分区覆盖。
    pub images: ImageOverrides,
    /// 插件开关覆盖。
    pub plugins: PluginOverrides,
    /// 代码语言高亮覆盖。
    pub highlight: HighlightOverrides,
}

impl Settings {
    /// 查询字段是否被显式覆盖。
    pub fn is_overridden(&self, field: SettingsField) -> bool {
        match field {
            SettingsField::Language => self.general.language.is_some(),
            SettingsField::Autosave => self.general.autosave.is_some(),
            SettingsField::Theme => self.appearance.theme.is_some(),
            SettingsField::Mode => self.appearance.mode.is_some(),
            SettingsField::BodyWrap => self.editor.body_wrap.is_some(),
            SettingsField::SourceWrap => self.editor.source_wrap.is_some(),
            SettingsField::CodeWrap => self.editor.code_wrap.is_some(),
            SettingsField::LargeDocumentSourceThreshold => {
                self.editor.large_document_source_threshold_bytes.is_some()
            }
            SettingsField::ImageDirectory => self.images.directory.is_some(),
            SettingsField::MermaidEnabled => self.plugins.mermaid.is_some(),
            SettingsField::TableEnabled => self.plugins.table.is_some(),
            SettingsField::ExportEnabled => self.plugins.export.is_some(),
            SettingsField::MathEnabled => self.plugins.math.is_some(),
            SettingsField::HighlightEnabled => self.plugins.highlight.is_some(),
            SettingsField::ImagePreviewEnabled => self.plugins.image_preview.is_some(),
            SettingsField::HighlightCodeTheme => self.highlight.code_theme.is_some(),
            SettingsField::HighlightCodeMode => self.highlight.code_mode.is_some(),
        }
    }

    /// 只清除指定分区的显式覆盖，其余分区与文档状态不受影响。
    pub fn reset_section(&mut self, section: Section) {
        match section {
            Section::General => self.general = GeneralOverrides::default(),
            Section::Appearance => self.appearance = AppearanceOverrides::default(),
            Section::Editor => self.editor = EditorOverrides::default(),
            Section::Images => self.images = ImageOverrides::default(),
            Section::Plugins => self.plugins = PluginOverrides::default(),
            Section::Highlight => self.highlight = HighlightOverrides::default(),
        }
    }

    /// 解析有效设置：默认值叠加显式覆盖。
    pub fn effective(&self) -> EffectiveSettings {
        let defaults = EffectiveSettings::default();
        EffectiveSettings {
            general: EffectiveGeneral {
                language: self.general.language.unwrap_or(defaults.general.language),
                autosave: self.general.autosave.unwrap_or(defaults.general.autosave),
            },
            appearance: EffectiveAppearance {
                theme: self.appearance.theme.unwrap_or(defaults.appearance.theme),
                mode: self.appearance.mode.unwrap_or(defaults.appearance.mode),
            },
            editor: EffectiveEditor {
                body_wrap: self.editor.body_wrap.unwrap_or(defaults.editor.body_wrap),
                source_wrap: self
                    .editor
                    .source_wrap
                    .unwrap_or(defaults.editor.source_wrap),
                code_wrap: self.editor.code_wrap.unwrap_or(defaults.editor.code_wrap),
                large_document_source_threshold_bytes: self
                    .editor
                    .large_document_source_threshold_bytes
                    .unwrap_or(defaults.editor.large_document_source_threshold_bytes),
            },
            images: EffectiveImages {
                directory: self
                    .images
                    .directory
                    .clone()
                    .unwrap_or(defaults.images.directory),
            },
            plugins: EffectivePlugins {
                mermaid: self.plugins.mermaid.unwrap_or(defaults.plugins.mermaid),
                table: self.plugins.table.unwrap_or(defaults.plugins.table),
                export: self.plugins.export.unwrap_or(defaults.plugins.export),
                math: self.plugins.math.unwrap_or(defaults.plugins.math),
                highlight: self.plugins.highlight.unwrap_or(defaults.plugins.highlight),
                image_preview: self
                    .plugins
                    .image_preview
                    .unwrap_or(defaults.plugins.image_preview),
            },
            highlight: EffectiveHighlight {
                code_theme: self
                    .highlight
                    .code_theme
                    .unwrap_or(defaults.highlight.code_theme),
                code_mode: self
                    .highlight
                    .code_mode
                    .unwrap_or(defaults.highlight.code_mode),
            },
        }
    }
}

/// 解析后的通用分区。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveGeneral {
    /// 有效界面语言偏好。
    pub language: LanguagePreference,
    /// 有效自动保存开关。
    pub autosave: bool,
}

/// 解析后的外观分区。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveAppearance {
    /// 有效内容主题家族。
    pub theme: ThemeFamily,
    /// 有效外观模式。
    pub mode: AppearanceMode,
}

/// 解析后的编辑器分区。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveEditor {
    /// 有效正文自动换行。
    pub body_wrap: bool,
    /// 有效源码视图自动换行。
    pub source_wrap: bool,
    /// 有效代码块自动换行。
    pub code_wrap: bool,
    /// 有效大文档阈值（字节）。
    pub large_document_source_threshold_bytes: u64,
}

/// 解析后的图片分区。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveImages {
    /// 有效图片目录。
    pub directory: String,
}

/// 解析后的插件开关。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectivePlugins {
    /// Mermaid 是否启用。
    pub mermaid: bool,
    /// 表格增强是否启用。
    pub table: bool,
    /// 导出是否启用。
    pub export: bool,
    /// 公式是否启用。
    pub math: bool,
    /// 代码语言高亮是否启用。
    pub highlight: bool,
    /// 图片预览是否启用。
    pub image_preview: bool,
}

/// 解析后的代码语言高亮配置；两维分别独立解析。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EffectiveHighlight {
    /// 有效代码主题维度。
    pub code_theme: HighlightThemeChoice,
    /// 有效高亮模式维度。
    pub code_mode: HighlightModeChoice,
}

impl EffectiveHighlight {
    /// 解析代码主题家族；`FollowApp` 取应用当前家族。
    pub const fn resolve_theme(self, app: ThemeFamily) -> ThemeFamily {
        self.code_theme.resolve(app)
    }

    /// 解析代码高亮模式；`FollowApp` 取应用已解析的具体模式。
    ///
    /// `app` 必须是具体模式（亮/暗/护眼）；应用自身“随系统”的解析由外观
    /// 层完成，本方法不重复推断。
    pub const fn resolve_mode(self, app: AppearanceMode) -> AppearanceMode {
        self.code_mode.resolve(app)
    }
}

impl Default for EffectiveHighlight {
    /// PRD §5.13：代码主题与高亮模式初始都随应用。
    fn default() -> Self {
        Self {
            code_theme: HighlightThemeChoice::FollowApp,
            code_mode: HighlightModeChoice::FollowApp,
        }
    }
}

/// 解析后的有效设置；UI 与插件只消费这一层，不自行合并覆盖。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectiveSettings {
    /// 通用分区。
    pub general: EffectiveGeneral,
    /// 外观分区。
    pub appearance: EffectiveAppearance,
    /// 编辑器分区。
    pub editor: EffectiveEditor,
    /// 图片分区。
    pub images: EffectiveImages,
    /// 插件开关。
    pub plugins: EffectivePlugins,
    /// 代码语言高亮配置。
    pub highlight: EffectiveHighlight,
}

impl Default for EffectiveSettings {
    /// PRD §5.13 冻结的首次启动默认值。
    fn default() -> Self {
        Self {
            general: EffectiveGeneral {
                language: LanguagePreference::FollowSystem,
                autosave: true,
            },
            appearance: EffectiveAppearance {
                theme: ThemeFamily::GitHub,
                mode: AppearanceMode::FollowSystem,
            },
            editor: EffectiveEditor {
                body_wrap: true,
                source_wrap: true,
                code_wrap: false,
                large_document_source_threshold_bytes:
                    DEFAULT_LARGE_DOCUMENT_SOURCE_THRESHOLD_BYTES,
            },
            images: EffectiveImages {
                directory: String::from("./assets"),
            },
            plugins: EffectivePlugins {
                mermaid: true,
                table: true,
                export: true,
                math: true,
                highlight: true,
                image_preview: true,
            },
            highlight: EffectiveHighlight::default(),
        }
    }
}
