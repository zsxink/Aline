//! 文档状态与跨层事务的所有者；M0 仅实现无窗口状态协议。

mod document;

pub use document::{
    CommitOutcome, CompositionView, CurrentSnapshot, DerivedRequestId, DocCore, DocError,
    DocLimits, DocumentId, DraftGeneration, EditorMode, IdentityError, InputIdentity,
    ModeSwitchOutcome, ParseConfigGeneration, PublicationKind, PublicationOutcome,
    PublicationResult, ResourceReference, WaitHandle, WaitStatus, WaitView, WeakResourceReference,
    WorkingVersion,
};

use aline_app_core::{CatalogError, Locale, Translator};

/// 取得当前应用名称并转换为 GPUI 可消费的共享字符串。
pub fn application_name(locale: Locale) -> Result<gpui::SharedString, CatalogError> {
    let translator = Translator::embedded(locale)?;
    translator.t("app.name").map(gpui::SharedString::from)
}

#[cfg(test)]
mod catalog_tests {
    use aline_app_core::Locale;

    #[test]
    fn application_name_consumes_embedded_catalog() {
        let name = super::application_name(Locale::En);
        assert!(matches!(name, Ok(value) if value.as_ref() == "Aline"));
    }
}
