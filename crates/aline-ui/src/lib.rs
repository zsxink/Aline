//! 文档状态、只读生命周期与原生单窗口装配边界。

mod document;

pub use document::{
    CommitOutcome, CompositionView, CurrentSnapshot, DerivedRequestId, DocCore, DocError,
    DocLimits, DocumentId, DraftGeneration, EditorMode, IdentityError, InputIdentity,
    ModeSwitchOutcome, OpenDisposition, OpenError, OpenRequestId, ParseConfigGeneration,
    PublicationKind, PublicationOutcome, PublicationResult, ReadonlyDocument, ReadonlyFileAccess,
    ReadonlyFileInput, ReadonlyOpenCompletion, ReadonlyOpenJob, ReadonlyOpenLimits,
    ReadonlyOpenPhase, ReadonlySession, ResourceReference, WaitHandle, WaitStatus, WaitView,
    WeakResourceReference, WorkingVersion,
};

use aline_app_core::{CatalogError, Locale, Translator};

#[cfg(target_os = "macos")]
mod native;
#[cfg(target_os = "macos")]
mod native_window;

/// 取得当前应用名称并转换为 GPUI 可消费的共享字符串。
pub fn application_name(locale: Locale) -> Result<gpui::SharedString, CatalogError> {
    let translator = Translator::embedded(locale)?;
    translator.t("app.name").map(gpui::SharedString::from)
}

/// 在已接入的平台上运行单窗口应用；其它平台返回当前 catalog 的未启用说明。
///
/// macOS 调用方必须从进程的 OS 主线程调用；应用入口 `main` 满足该约束。
/// 初始窗口构造失败时会输出本地化错误及诊断，并以非零状态立即退出。
pub fn run_application(locale: Locale, fallback_used: bool) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        native::run(locale, fallback_used)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let translator = Translator::embedded(locale).map_err(|error| error.to_string())?;
        Err(translator
            .t("app.platform_unavailable")
            .map_err(|error| error.to_string())?)
    }
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
