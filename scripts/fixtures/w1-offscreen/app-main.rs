#![forbid(unsafe_code)]
//! W1 独立进程入口；必须在候选准入后才构建和运行。

use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let Some(output) = std::env::args_os().nth(1).map(PathBuf::from) else {
        eprintln!("缺少 logs/validation 内的独立输出目录");
        return ExitCode::FAILURE;
    };
    match aline_ui::run_w1_spike(&output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("离屏能力实验失败：{error}");
            ExitCode::FAILURE
        }
    }
}
