#![forbid(unsafe_code)]

// 审计探针：编译期嵌入中文 TOML，并在运行期解析其中的字符串。
use rust_embed::RustEmbed;

#[derive(Debug)]
pub enum ProbeError {
    MissingAsset,
    InvalidUtf8(std::str::Utf8Error),
    InvalidToml(toml::de::Error),
    MissingField(&'static str),
    InvalidFieldType(&'static str),
}

#[derive(RustEmbed)]
#[folder = "assets/"]
struct Catalog;

pub fn embedded_title() -> Result<String, ProbeError> {
    let file = Catalog::get("zh-CN.toml").ok_or(ProbeError::MissingAsset)?;
    let text = std::str::from_utf8(file.data.as_ref()).map_err(ProbeError::InvalidUtf8)?;
    let document: toml::Table = toml::from_str(text).map_err(ProbeError::InvalidToml)?;
    let app = document
        .get("app")
        .ok_or(ProbeError::MissingField("app"))?;
    let title = app
        .get("title")
        .ok_or(ProbeError::MissingField("app.title"))?
        .as_str()
        .ok_or(ProbeError::InvalidFieldType("app.title"))?;
    Ok(title.to_owned())
}
