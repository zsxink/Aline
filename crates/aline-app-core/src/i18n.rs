//! 显式语言实例与嵌入 catalog 的查询和校验。

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "locales/"]
struct CatalogAssets;

/// 当前翻译器实例使用的语言。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Locale {
    /// 简体中文。
    ZhCn,
    /// 英语。
    En,
}

impl Locale {
    fn resource_name(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN.toml",
            Self::En => "en.toml",
        }
    }
}

/// catalog 载入、校验或查找失败。
#[derive(Debug)]
pub enum CatalogError {
    /// 嵌入资源缺失。
    MissingResource(String),
    /// Catalog 资源不是有效 UTF-8。
    InvalidUtf8(std::str::Utf8Error),
    /// TOML 文档无法解析。
    InvalidToml(toml::de::Error),
    /// catalog 没有任何翻译键。
    EmptyCatalog,
    /// catalog 的层级键名非法。
    InvalidKey(String),
    /// catalog 叶值不是字符串。
    InvalidValueType(String),
    /// 两种语言的键集合不一致。
    KeySetMismatch,
    /// 两种语言的占位符集合不一致。
    PlaceholderMismatch(String),
    /// 占位符格式无效。
    InvalidPlaceholder(String),
    /// 查找键不存在。
    MissingKey(String),
    /// 翻译文本需要插值参数或实际参数与需求不一致。
    InterpolationMismatch(String),
    /// 同一插值参数被重复传入。
    DuplicateArgument(String),
}

impl fmt::Display for CatalogError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for CatalogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidUtf8(error) => Some(error),
            Self::InvalidToml(error) => Some(error),
            _ => None,
        }
    }
}

/// 双语嵌入 catalog 的实例级查询器。
pub struct Translator {
    locale: Locale,
    chinese: BTreeMap<String, String>,
    english: BTreeMap<String, String>,
}

impl Translator {
    /// 从编译进二进制的双语资源构造并校验翻译器。
    ///
    /// 两种语言的资源都必须存在且通过 TOML、schema、键集合与占位符校验。
    pub fn embedded(locale: Locale) -> Result<Self, CatalogError> {
        let chinese = Self::embedded_resource(Locale::ZhCn)
            .ok_or_else(|| CatalogError::MissingResource(String::from("zh-CN.toml")))?;
        let english = Self::embedded_resource(Locale::En)
            .ok_or_else(|| CatalogError::MissingResource(String::from("en.toml")))?;
        let chinese = parse_catalog(&chinese)?;
        let english = parse_catalog(&english)?;
        Self::from_catalogs(locale, english, chinese)
    }

    /// 返回此翻译器当前使用的语言。
    pub fn locale(&self) -> Locale {
        self.locale
    }

    /// 更改此翻译器实例的语言。
    pub fn set_locale(&mut self, locale: Locale) {
        self.locale = locale;
    }

    /// 查询不含插值的翻译文本；键不存在或文本含占位符时返回错误。
    pub fn t(&self, key: &str) -> Result<String, CatalogError> {
        self.t_with(key, &[])
    }

    /// 查询翻译文本，并按名称替换全部 ASCII `{name}` 占位符。
    ///
    /// 参数必须与模板占位符集合精确相等，重复参数、缺失参数或额外参数均返回错误。
    pub fn t_with(&self, key: &str, arguments: &[(&str, &str)]) -> Result<String, CatalogError> {
        let catalog = match self.locale {
            Locale::ZhCn => &self.chinese,
            Locale::En => &self.english,
        };
        let template = catalog
            .get(key)
            .ok_or_else(|| CatalogError::MissingKey(String::from(key)))?;
        let placeholders = placeholders(template, key)?;
        let mut values = BTreeMap::new();
        for (name, value) in arguments {
            if values.insert(*name, *value).is_some() {
                return Err(CatalogError::DuplicateArgument(String::from(*name)));
            }
        }
        let names: BTreeSet<&str> = placeholders.iter().copied().collect();
        if names != values.keys().copied().collect() {
            return Err(CatalogError::InterpolationMismatch(String::from(key)));
        }
        let mut output = String::with_capacity(template.len());
        let mut remainder: &str = template;
        while let Some(open) = remainder.find('{') {
            output.push_str(&remainder[..open]);
            let after_open = &remainder[open + 1..];
            let close = after_open
                .find('}')
                .ok_or_else(|| CatalogError::InvalidPlaceholder(String::from(key)))?;
            let name = &after_open[..close];
            let value = values
                .get(name)
                .ok_or_else(|| CatalogError::InterpolationMismatch(String::from(key)))?;
            output.push_str(value);
            remainder = &after_open[close + 1..];
        }
        output.push_str(remainder);
        Ok(output)
    }

    fn embedded_resource(locale: Locale) -> Option<Cow<'static, [u8]>> {
        CatalogAssets::get(locale.resource_name()).map(|resource| resource.data)
    }

    fn from_catalogs(
        locale: Locale,
        english: BTreeMap<String, String>,
        chinese: BTreeMap<String, String>,
    ) -> Result<Self, CatalogError> {
        if english.keys().ne(chinese.keys()) {
            return Err(CatalogError::KeySetMismatch);
        }
        for (key, english_text) in &english {
            let english_names: BTreeSet<&str> =
                placeholders(english_text, key)?.into_iter().collect();
            let chinese_names: BTreeSet<&str> =
                placeholders(&chinese[key], key)?.into_iter().collect();
            if english_names != chinese_names {
                return Err(CatalogError::PlaceholderMismatch(key.clone()));
            }
        }
        Ok(Self {
            locale,
            chinese,
            english,
        })
    }
}

fn parse_catalog(source: &[u8]) -> Result<BTreeMap<String, String>, CatalogError> {
    let source = std::str::from_utf8(source).map_err(CatalogError::InvalidUtf8)?;
    let value: toml::Value = toml::from_str(source).map_err(CatalogError::InvalidToml)?;
    let mut entries = BTreeMap::new();
    flatten_table("", value, &mut entries)?;
    if entries.is_empty() {
        return Err(CatalogError::EmptyCatalog);
    }
    for (key, text) in &entries {
        placeholders(text, key)?;
    }
    Ok(entries)
}

fn flatten_table(
    prefix: &str,
    value: toml::Value,
    entries: &mut BTreeMap<String, String>,
) -> Result<(), CatalogError> {
    match value {
        toml::Value::Table(table) => {
            for (name, child) in table {
                if name.is_empty() || name.contains('.') {
                    return Err(CatalogError::InvalidKey(name));
                }
                let key = if prefix.is_empty() {
                    name
                } else {
                    format!("{prefix}.{name}")
                };
                flatten_table(&key, child, entries)?;
            }
        }
        toml::Value::String(text) if !prefix.is_empty() => {
            entries.insert(prefix.to_owned(), text);
        }
        _ => return Err(CatalogError::InvalidValueType(prefix.to_owned())),
    }
    Ok(())
}

fn placeholders<'a>(template: &'a str, key: &str) -> Result<Vec<&'a str>, CatalogError> {
    let mut names = Vec::new();
    let mut remainder = template;
    while let Some(open) = remainder.find(['{', '}']) {
        if remainder.as_bytes()[open] == b'}' {
            return Err(CatalogError::InvalidPlaceholder(String::from(key)));
        }
        let after_open = &remainder[open + 1..];
        let close = after_open
            .find('}')
            .ok_or_else(|| CatalogError::InvalidPlaceholder(String::from(key)))?;
        let name = &after_open[..close];
        let mut bytes = name.bytes();
        let valid_first = bytes
            .next()
            .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_');
        if !valid_first || !bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_') {
            return Err(CatalogError::InvalidPlaceholder(String::from(key)));
        }
        names.push(name);
        remainder = &after_open[close + 1..];
    }
    Ok(names)
}

#[cfg(test)]
mod i18n_tests {
    use std::borrow::Cow;
    use std::collections::BTreeMap;

    use super::{CatalogError, Locale, Translator, parse_catalog};

    fn catalog(values: &[(&str, &str)]) -> BTreeMap<String, String> {
        values
            .iter()
            .map(|(key, value)| (String::from(*key), String::from(*value)))
            .collect()
    }

    #[test]
    fn embedded_catalog_is_borrowed_and_translates_app_name() {
        let resource = Translator::embedded_resource(Locale::En);
        assert!(matches!(resource, Some(Cow::Borrowed(_))));
        let translator = Translator::embedded(Locale::En);
        assert!(
            matches!(translator, Ok(value) if matches!(value.t("app.name"), Ok(name) if name == "Aline"))
        );
    }

    #[test]
    fn locale_is_owned_by_each_translator_instance() -> Result<(), CatalogError> {
        let mut english = Translator::from_catalogs(
            Locale::En,
            catalog(&[("app.name", "Aline")]),
            catalog(&[("app.name", "阿莱恩")]),
        )?;
        let mut chinese = Translator::from_catalogs(
            Locale::ZhCn,
            catalog(&[("app.name", "Aline")]),
            catalog(&[("app.name", "阿莱恩")]),
        )?;
        assert_eq!(english.t("app.name")?, "Aline");
        assert_eq!(chinese.t("app.name")?, "阿莱恩");
        chinese.set_locale(Locale::En);
        assert_eq!(chinese.t("app.name")?, "Aline");
        assert_eq!(english.t("app.name")?, "Aline");
        english.set_locale(Locale::ZhCn);
        assert_eq!(english.locale(), Locale::ZhCn);
        assert_eq!(english.t("app.name")?, "阿莱恩");
        assert_eq!(chinese.t("app.name")?, "Aline");
        Ok(())
    }

    #[test]
    fn interpolation_requires_exact_unique_arguments() {
        let en = catalog(&[("greeting", "Hello {name}")]);
        let zh = catalog(&[("greeting", "你好，{name}")]);
        let translator = Translator::from_catalogs(Locale::En, en, zh);
        assert!(
            matches!(translator, Ok(value) if matches!(value.t_with("greeting", &[("name", "Ada")]), Ok(text) if text == "Hello Ada"))
        );
        let translator = Translator::from_catalogs(
            Locale::En,
            catalog(&[("greeting", "Hello {name}")]),
            catalog(&[("greeting", "你好，{name}")]),
        );
        assert!(
            matches!(translator, Ok(value) if matches!(value.t("greeting"), Err(CatalogError::InterpolationMismatch(_))))
        );
        let translator = Translator::from_catalogs(
            Locale::En,
            catalog(&[("greeting", "Hello {name}")]),
            catalog(&[("greeting", "你好，{name}")]),
        );
        assert!(
            matches!(translator, Ok(value) if matches!(value.t_with("greeting", &[("name", "Ada"), ("name", "Grace")]), Err(CatalogError::DuplicateArgument(_))))
        );
        let translator = Translator::from_catalogs(
            Locale::En,
            catalog(&[("greeting", "{name} says {name}")]),
            catalog(&[("greeting", "{name} 说 {name}")]),
        );
        assert!(
            matches!(translator, Ok(value) if matches!(value.t_with("greeting", &[("name", "你好，Ada")]), Ok(text) if text == "你好，Ada says 你好，Ada"))
        );
    }

    #[test]
    fn catalogs_reject_key_placeholder_and_brace_mismatches() {
        assert!(matches!(
            Translator::from_catalogs(
                Locale::En,
                catalog(&[("app.name", "Aline")]),
                catalog(&[("other", "Aline")])
            ),
            Err(CatalogError::KeySetMismatch)
        ));
        assert!(matches!(
            Translator::from_catalogs(
                Locale::En,
                catalog(&[("greeting", "Hello {name}")]),
                catalog(&[("greeting", "你好 {user}")])
            ),
            Err(CatalogError::PlaceholderMismatch(_))
        ));
        assert!(
            Translator::from_catalogs(
                Locale::En,
                catalog(&[("greeting", "Hello {first} {name}")]),
                catalog(&[("greeting", "你好 {name} {first}")])
            )
            .is_ok()
        );
        assert!(matches!(
            Translator::from_catalogs(
                Locale::En,
                catalog(&[("greeting", "Hello {name")]),
                catalog(&[("greeting", "你好 {name}")])
            ),
            Err(CatalogError::InvalidPlaceholder(_))
        ));
    }

    #[test]
    fn lookup_interpolation_and_catalog_shape_fail_explicitly() {
        let translator = Translator::from_catalogs(
            Locale::En,
            catalog(&[("greeting", "Hello {name}")]),
            catalog(&[("greeting", "你好 {name}")]),
        );
        assert!(
            matches!(translator, Ok(ref value) if matches!(value.t("unknown"), Err(CatalogError::MissingKey(_))))
        );
        assert!(
            matches!(translator, Ok(value) if matches!(value.t_with("greeting", &[("name", "Ada"), ("other", "Grace")]), Err(CatalogError::InterpolationMismatch(_))))
        );
        assert!(matches!(
            parse_catalog(b"[broken"),
            Err(CatalogError::InvalidToml(_))
        ));
        if let Err(error) = parse_catalog(b"[broken") {
            assert!(std::error::Error::source(&error).is_some());
        }
        assert!(matches!(
            parse_catalog(&[0xff]),
            Err(CatalogError::InvalidUtf8(_))
        ));
        if let Err(error) = parse_catalog(&[0xff]) {
            assert!(std::error::Error::source(&error).is_some());
        }
        assert!(matches!(
            parse_catalog(b"[app]\nname = 7\n"),
            Err(CatalogError::InvalidValueType(_))
        ));
        assert!(matches!(
            parse_catalog(b""),
            Err(CatalogError::EmptyCatalog)
        ));
        assert!(matches!(
            parse_catalog(b"[app]\n\"a.b\" = \"name\"\n"),
            Err(CatalogError::InvalidKey(_))
        ));
        assert!(matches!(
            parse_catalog(b"[app]\nname = \"{1bad}\"\n"),
            Err(CatalogError::InvalidPlaceholder(_))
        ));
    }
}
