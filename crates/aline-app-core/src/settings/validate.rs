//! 设置的内部 TOML 表示：解析、逐字段校验与序列化。
//!
//! 持久化格式是内部实现细节，只写显式覆盖而不写默认值，升级时可以调整
//! 默认值。读取时任何损坏、类型错误或未知枚举值都按字段回退到默认值并
//! 给出诊断，绝不阻止启动；未知键（旧版本遗留或更新版本写入）静默忽略。

use std::fmt::Write as _;

use toml::{Table, Value};

use super::model::{
    AppearanceMode, AppearanceOverrides, EditorOverrides, GeneralOverrides, HighlightModeChoice,
    HighlightOverrides, HighlightThemeChoice, ImageOverrides, LanguagePreference, PluginOverrides,
    Section, Settings, SettingsField, ThemeFamily,
};

/// 设置读取诊断原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettingsIssueReason {
    /// 整个设置文档无法解析。
    Unreadable,
    /// 单个字段的值类型或取值非法。
    InvalidValue,
}

impl SettingsIssueReason {
    /// 面向用户的 catalog 键；缺失或占位符不一致是门禁失败。
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::Unreadable => "settings.issue.unreadable",
            Self::InvalidValue => "settings.issue.invalid_value",
        }
    }
}

/// 单条设置诊断；`field` 为 `None` 表示文档级失败。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SettingsIssue {
    /// 受影响字段；文档级失败为 `None`。
    pub field: Option<SettingsField>,
    /// 诊断原因。
    pub reason: SettingsIssueReason,
}

/// 设置读取结果：可用实例与逐字段诊断。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettingsLoad {
    /// 回退后的覆盖集合，可直接用于 `effective()`。
    pub settings: Settings,
    /// 读取过程中发现的问题；空表示全部值有效。
    pub issues: Vec<SettingsIssue>,
}

impl Settings {
    /// 从内部 TOML 表示读取覆盖；损坏或非法值按字段回退，不返回错误。
    pub fn load(source: &str) -> SettingsLoad {
        let root: Table = match source.parse() {
            Ok(root) => root,
            Err(_) => {
                return SettingsLoad {
                    settings: Self::default(),
                    issues: vec![SettingsIssue {
                        field: None,
                        reason: SettingsIssueReason::Unreadable,
                    }],
                };
            }
        };
        let mut settings = Self::default();
        let mut issues = Vec::new();
        if let Some(table) = section(&root, Section::General, &mut issues) {
            settings.general = GeneralOverrides {
                language: read_enum(
                    table,
                    SettingsField::Language,
                    LanguagePreference::parse,
                    &mut issues,
                ),
                autosave: read_bool(table, SettingsField::Autosave, &mut issues),
            };
        }
        if let Some(table) = section(&root, Section::Appearance, &mut issues) {
            settings.appearance = AppearanceOverrides {
                theme: read_enum(table, SettingsField::Theme, ThemeFamily::parse, &mut issues),
                mode: read_enum(
                    table,
                    SettingsField::Mode,
                    AppearanceMode::parse,
                    &mut issues,
                ),
            };
        }
        if let Some(table) = section(&root, Section::Editor, &mut issues) {
            settings.editor = EditorOverrides {
                body_wrap: read_bool(table, SettingsField::BodyWrap, &mut issues),
                source_wrap: read_bool(table, SettingsField::SourceWrap, &mut issues),
                code_wrap: read_bool(table, SettingsField::CodeWrap, &mut issues),
                large_document_source_threshold_bytes: read_threshold(table, &mut issues),
            };
        }
        if let Some(table) = section(&root, Section::Images, &mut issues) {
            settings.images = ImageOverrides {
                directory: read_directory(table, &mut issues),
            };
        }
        if let Some(table) = section(&root, Section::Plugins, &mut issues) {
            settings.plugins = PluginOverrides {
                mermaid: read_bool(table, SettingsField::MermaidEnabled, &mut issues),
                table: read_bool(table, SettingsField::TableEnabled, &mut issues),
                export: read_bool(table, SettingsField::ExportEnabled, &mut issues),
                math: read_bool(table, SettingsField::MathEnabled, &mut issues),
                highlight: read_bool(table, SettingsField::HighlightEnabled, &mut issues),
                image_preview: read_bool(table, SettingsField::ImagePreviewEnabled, &mut issues),
            };
        }
        if let Some(table) = section(&root, Section::Highlight, &mut issues) {
            settings.highlight = HighlightOverrides {
                code_theme: read_enum(
                    table,
                    SettingsField::HighlightCodeTheme,
                    HighlightThemeChoice::parse,
                    &mut issues,
                ),
                code_mode: read_enum(
                    table,
                    SettingsField::HighlightCodeMode,
                    HighlightModeChoice::parse,
                    &mut issues,
                ),
            };
        }
        SettingsLoad { settings, issues }
    }

    /// 写出只含显式覆盖的内部 TOML 表示；无覆盖时返回空串。
    pub fn to_toml_string(&self) -> String {
        let mut blocks = Vec::new();
        let mut general = String::new();
        if let Some(language) = self.general.language {
            write_string(
                &mut general,
                SettingsField::Language.key(),
                language.token(),
            );
        }
        if let Some(autosave) = self.general.autosave {
            write_bool(&mut general, SettingsField::Autosave.key(), autosave);
        }
        push_block(&mut blocks, Section::General, general);
        let mut appearance = String::new();
        if let Some(theme) = self.appearance.theme {
            write_string(&mut appearance, SettingsField::Theme.key(), theme.token());
        }
        if let Some(mode) = self.appearance.mode {
            write_string(&mut appearance, SettingsField::Mode.key(), mode.token());
        }
        push_block(&mut blocks, Section::Appearance, appearance);
        let mut editor = String::new();
        if let Some(body_wrap) = self.editor.body_wrap {
            write_bool(&mut editor, SettingsField::BodyWrap.key(), body_wrap);
        }
        if let Some(source_wrap) = self.editor.source_wrap {
            write_bool(&mut editor, SettingsField::SourceWrap.key(), source_wrap);
        }
        if let Some(code_wrap) = self.editor.code_wrap {
            write_bool(&mut editor, SettingsField::CodeWrap.key(), code_wrap);
        }
        if let Some(threshold) = self.editor.large_document_source_threshold_bytes {
            write_integer(
                &mut editor,
                SettingsField::LargeDocumentSourceThreshold.key(),
                threshold,
            );
        }
        push_block(&mut blocks, Section::Editor, editor);
        let mut images = String::new();
        if let Some(directory) = &self.images.directory {
            write_string(&mut images, SettingsField::ImageDirectory.key(), directory);
        }
        push_block(&mut blocks, Section::Images, images);
        let mut plugins = String::new();
        for (field, value) in [
            (SettingsField::MermaidEnabled, self.plugins.mermaid),
            (SettingsField::TableEnabled, self.plugins.table),
            (SettingsField::ExportEnabled, self.plugins.export),
            (SettingsField::MathEnabled, self.plugins.math),
            (SettingsField::HighlightEnabled, self.plugins.highlight),
            (
                SettingsField::ImagePreviewEnabled,
                self.plugins.image_preview,
            ),
        ] {
            if let Some(enabled) = value {
                write_bool(&mut plugins, field.key(), enabled);
            }
        }
        push_block(&mut blocks, Section::Plugins, plugins);
        let mut highlight = String::new();
        if let Some(theme) = self.highlight.code_theme {
            write_string(
                &mut highlight,
                SettingsField::HighlightCodeTheme.key(),
                theme.token(),
            );
        }
        if let Some(mode) = self.highlight.code_mode {
            write_string(
                &mut highlight,
                SettingsField::HighlightCodeMode.key(),
                mode.token(),
            );
        }
        push_block(&mut blocks, Section::Highlight, highlight);
        blocks.join("\n")
    }
}

fn push_block(blocks: &mut Vec<String>, section: Section, mut block: String) {
    if block.is_empty() {
        return;
    }
    block.insert_str(0, &format!("[{}]\n", section.name()));
    blocks.push(block);
}

/// 取分区表；存在但不是表时，为该分区每个字段各记一条诊断。
fn section<'a>(
    root: &'a Table,
    section: Section,
    issues: &mut Vec<SettingsIssue>,
) -> Option<&'a Table> {
    match root.get(section.name()) {
        None => None,
        Some(Value::Table(table)) => Some(table),
        Some(_) => {
            issues.extend(section.fields().iter().map(|field| SettingsIssue {
                field: Some(*field),
                reason: SettingsIssueReason::InvalidValue,
            }));
            None
        }
    }
}

fn invalid(field: SettingsField, issues: &mut Vec<SettingsIssue>) {
    issues.push(SettingsIssue {
        field: Some(field),
        reason: SettingsIssueReason::InvalidValue,
    });
}

fn read_bool(table: &Table, field: SettingsField, issues: &mut Vec<SettingsIssue>) -> Option<bool> {
    match table.get(field.key()) {
        None => None,
        Some(Value::Boolean(value)) => Some(*value),
        Some(_) => {
            invalid(field, issues);
            None
        }
    }
}

fn read_enum<T>(
    table: &Table,
    field: SettingsField,
    parse: impl Fn(&str) -> Option<T>,
    issues: &mut Vec<SettingsIssue>,
) -> Option<T> {
    match table.get(field.key()) {
        None => None,
        Some(Value::String(text)) => match parse(text) {
            Some(value) => Some(value),
            None => {
                invalid(field, issues);
                None
            }
        },
        Some(_) => {
            invalid(field, issues);
            None
        }
    }
}

fn read_threshold(table: &Table, issues: &mut Vec<SettingsIssue>) -> Option<u64> {
    let field = SettingsField::LargeDocumentSourceThreshold;
    match table.get(field.key()) {
        None => None,
        Some(Value::Integer(value)) => match u64::try_from(*value) {
            Ok(bytes) => Some(bytes),
            Err(_) => {
                invalid(field, issues);
                None
            }
        },
        Some(_) => {
            invalid(field, issues);
            None
        }
    }
}

fn read_directory(table: &Table, issues: &mut Vec<SettingsIssue>) -> Option<String> {
    let field = SettingsField::ImageDirectory;
    match table.get(field.key()) {
        None => None,
        Some(Value::String(text)) if valid_directory(text) => Some(text.clone()),
        Some(_) => {
            invalid(field, issues);
            None
        }
    }
}

/// 目录允许相对或绝对路径；空串与 NUL 不能成为有效路径。
fn valid_directory(text: &str) -> bool {
    !text.is_empty() && !text.contains('\0')
}

fn write_bool(out: &mut String, key: &str, value: bool) {
    out.push_str(key);
    out.push_str(" = ");
    out.push_str(if value { "true" } else { "false" });
    out.push('\n');
}

fn write_integer(out: &mut String, key: &str, value: u64) {
    let _ = writeln!(out, "{key} = {value}");
}

fn write_string(out: &mut String, key: &str, value: &str) {
    out.push_str(key);
    out.push_str(" = \"");
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control.is_control() => {
                let _ = write!(out, "\\u{:04X}", u32::from(control));
            }
            other => out.push(other),
        }
    }
    out.push_str("\"\n");
}
