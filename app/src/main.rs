//! M0 薄入口；只建立编译边界，不创建应用窗口或平台服务。

use std::process::ExitCode;

use aline_app_core::Locale;

fn main() -> ExitCode {
    match aline_ui::application_name(Locale::En) {
        Ok(_application_name) => ExitCode::SUCCESS,
        Err(_catalog_error) => ExitCode::FAILURE,
    }
}
