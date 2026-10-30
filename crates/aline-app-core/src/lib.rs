//! 显式配置与 catalog 的纯模型，不持有 GPUI 或全局语言状态。

mod i18n;

pub use i18n::{CatalogError, Locale, Translator};
