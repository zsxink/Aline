//! 无头树 dump：读取一个 UTF-8 文件并打印稳定文本结果。

use std::process::ExitCode;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{ParseBudget, ParseConfig, parse};

fn main() -> ExitCode {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: dump_tree <path>");
        return ExitCode::FAILURE;
    };
    let source = match std::fs::read_to_string(&path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("read {path}: {error}");
            return ExitCode::FAILURE;
        }
    };
    let snapshot = match TextSnapshot::new(source, TextBudget::new(16 * 1024 * 1024)) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            eprintln!("snapshot: {error:?}");
            return ExitCode::FAILURE;
        }
    };
    match parse(&snapshot, ParseConfig::m0(), ParseBudget::m0()) {
        Ok(parsed) => {
            print!("{}", parsed.dump_tree());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("parse: {error}");
            ExitCode::FAILURE
        }
    }
}
