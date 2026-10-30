#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    match aline_app_core_probe::embedded_title() {
        Ok(title) => {
            println!("embedded title={title}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("embedded catalog error: {error:?}");
            ExitCode::FAILURE
        }
    }
}
