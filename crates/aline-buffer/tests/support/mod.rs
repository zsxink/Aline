//! W3 专用测试驱动：本地 RNG、配置和失败上下文；四 crate 测试消费，不进入生产代码。
//! 只接收合成测试文本；错误含完整轨迹，纯输入性质可删除最小化，不执行文件 I/O。

use std::error::Error;
use std::fmt::Debug;
use std::io;

pub type TestResult<T = ()> = Result<T, Box<dyn Error>>;

pub fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    io::Error::new(io::ErrorKind::InvalidData, message.into()).into()
}

pub struct Config {
    pub seed: u64,
    pub seeds: usize,
    pub steps: usize,
    pub trace: bool,
    pub inject: bool,
}

fn number(name: &str, fallback: u64, maximum: u64) -> TestResult<u64> {
    let value = match std::env::var(name) {
        Ok(value) => value.parse::<u64>()?,
        Err(std::env::VarError::NotPresent) => fallback,
        Err(error) => return Err(error.into()),
    };
    if value == 0 || value > maximum {
        return Err(invalid(format!("{name} 必须在 1..={maximum}")));
    }
    Ok(value)
}

impl Config {
    pub fn read() -> TestResult<Self> {
        let trace = match std::env::var("ALINE_FUZZ_TRACE") {
            Ok(value) if value == "1" => true,
            Ok(value) if value == "0" => false,
            Err(std::env::VarError::NotPresent) => false,
            _ => return Err(invalid("ALINE_FUZZ_TRACE 必须为 0 或 1")),
        };
        let inject = match std::env::var("ALINE_FUZZ_INJECT") {
            Ok(value) if value == "byte-loss" => true,
            Err(std::env::VarError::NotPresent) => false,
            _ => return Err(invalid("ALINE_FUZZ_INJECT 仅接受 byte-loss")),
        };
        Ok(Self {
            seed: number("ALINE_FUZZ_SEED", 0x5a17_2026, u64::MAX - 1024)?,
            seeds: usize::try_from(number("ALINE_FUZZ_SEEDS", 16, 1024)?)?,
            steps: usize::try_from(number("ALINE_FUZZ_STEPS", 100, 100_000)?)?,
            trace,
            inject,
        })
    }
}

pub struct Run {
    seed: u64,
    state: u64,
    step: usize,
    trace: Vec<String>,
    verbose: bool,
}

impl Run {
    pub fn new(seed: u64, verbose: bool) -> Self {
        Self {
            seed,
            state: seed,
            step: 0,
            trace: Vec::new(),
            verbose,
        }
    }

    pub fn below(&mut self, limit: usize) -> TestResult<usize> {
        if limit == 0 {
            return Err(invalid("RNG 上界不能为零"));
        }
        self.state ^= self.state >> 12;
        self.state ^= self.state << 25;
        self.state ^= self.state >> 27;
        let value = self.state.wrapping_mul(0x2545_f491_4f6c_dd1d);
        Ok(usize::try_from(value % u64::try_from(limit)?)?)
    }

    pub fn record(&mut self, step: usize, operation: impl Into<String>) {
        self.step = step;
        self.trace.push(operation.into());
        if self.verbose {
            eprintln!("seed={} step={step} {:?}", self.seed, self.trace.last());
        }
    }

    pub fn check(&self, condition: bool, property: &str, input: &[u8]) -> TestResult {
        if condition {
            return Ok(());
        }
        Err(self.failure(property, input))
    }

    pub fn equal<T: Debug + PartialEq>(
        &self,
        actual: &T,
        expected: &T,
        property: &str,
        input: &[u8],
    ) -> TestResult {
        self.check(
            actual == expected,
            &format!("{property}: actual={actual:?}, expected={expected:?}"),
            input,
        )
    }

    pub fn failure(&self, property: &str, input: &[u8]) -> Box<dyn Error> {
        invalid(format!(
            "seed={} step={} property={property}\noperations={:?}\ninput={input:?}\n复跑: ALINE_FUZZ_SEED={} ALINE_FUZZ_SEEDS=1 ALINE_FUZZ_STEPS={}",
            self.seed,
            self.step,
            self.trace,
            self.seed,
            self.step + 1
        ))
    }
}

pub fn boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(src_off, _)| src_off)
        .chain(std::iter::once(text.len()))
        .collect()
}

/// 删除至单字节删除也不能继续缩短；只保留仍触发同一失败性质的候选。
pub fn minimize(mut input: Vec<u8>, fails: impl Fn(&[u8]) -> bool) -> Vec<u8> {
    let mut width = input.len().max(1);
    while width > 0 {
        let mut start = 0;
        while start < input.len() {
            let end = start.saturating_add(width).min(input.len());
            let mut candidate = input.clone();
            candidate.drain(start..end);
            if fails(&candidate) {
                input = candidate;
                start = 0;
            } else {
                start = end;
            }
        }
        width /= 2;
    }
    input
}

#[test]
fn fuzz_minimizer_retains_injected_byte_loss() {
    let minimized = minimize(b"abc!xyz".to_vec(), |bytes| bytes.contains(&b'!'));
    assert_eq!(minimized, b"!");
}

#[test]
fn fuzz_minimizer_rechecks_earlier_deletions_after_shrinking() {
    let minimized = minimize(b"ab!".to_vec(), |bytes| {
        bytes == b"ab!" || bytes == b"ab" || bytes == b"b"
    });
    assert_eq!(minimized, b"b");
}
