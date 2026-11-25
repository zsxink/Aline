//! V-06 内容侧容量探针：公式与 Mermaid 的源码规模 → 渲染耗时与产物大小。
//!
//! 与 `m4-v03/probe-content` 的分工：那边量“支持什么”，这边量“代价多大”。
//! 两个候选后端都尚未准入（裁决在任务 1.5）；即使准入否决，这些耗时仍描述
//! 了同类工作量的量级，可用于冻结预算，但不能当作生产承诺。
//!
//! 子命令：
//! - `math <terms>...`：渲染 `x_1 + ... + x_n`，报告冷/热耗时与 SVG 字节。
//! - `mermaid <nodes>...`：渲染 n 节点链式流程图，报告冷/热耗时与 SVG 字节。

use std::time::Instant;

use latex_rust::{latex_to_svg, MathFont, SvgOptions};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_default();
    let rest: Vec<String> = arguments.collect();
    match command.as_str() {
        "math" => math(&rest),
        "mermaid" => mermaid(&rest),
        other => println!(
            "{}",
            serde_json::json!({ "error": format!("unknown command: {other}") })
        ),
    }
}

/// 每个用例渲染两次：首次计入字体/内部缓存初始化，第二次是稳定态。
/// 只报一个数会把一次性开销摊到规模上，得出错误的斜率。
fn math(sizes: &[String]) {
    let font = match MathFont::stix_two_math() {
        Ok(font) => font,
        Err(error) => {
            println!("{}", serde_json::json!({ "error": error.to_string() }));
            return;
        }
    };
    let options = SvgOptions::new();
    for size in sizes {
        let terms: usize = match size.parse() {
            Ok(value) => value,
            Err(_) => {
                println!("{}", serde_json::json!({ "terms": size, "error": "not a number" }));
                continue;
            }
        };
        let source = math_source(terms);
        let first = Instant::now();
        let first_result = latex_to_svg(&source, &font, &options);
        let first_ms = first.elapsed().as_secs_f64() * 1000.0;
        let second = Instant::now();
        let second_result = latex_to_svg(&source, &font, &options);
        let second_ms = second.elapsed().as_secs_f64() * 1000.0;
        report("terms", size, &source, first_ms, second_ms, first_result, second_result);
    }
}

/// 线性增长的 TeX：n 个下标项相加。不用 `\sum` 之类的定长记号，
/// 否则“规模”只增长源码长度而不增长工作量。
fn math_source(terms: usize) -> String {
    let mut source = String::new();
    for index in 1..=terms {
        if index > 1 {
            source.push('+');
        }
        source.push_str(&format!("x_{{{index}}}"));
    }
    source
}

fn mermaid(sizes: &[String]) {
    for size in sizes {
        let nodes: usize = match size.parse() {
            Ok(value) => value,
            Err(_) => {
                println!("{}", serde_json::json!({ "nodes": size, "error": "not a number" }));
                continue;
            }
        };
        let source = mermaid_source(nodes);
        let first = Instant::now();
        let first_result = mermaid_rs_renderer::render(&source);
        let first_ms = first.elapsed().as_secs_f64() * 1000.0;
        let second = Instant::now();
        let second_result = mermaid_rs_renderer::render(&source);
        let second_ms = second.elapsed().as_secs_f64() * 1000.0;
        report("nodes", size, &source, first_ms, second_ms, first_result, second_result);
    }
}

/// 链式流程图：n 个节点、n-1 条边。
fn mermaid_source(nodes: usize) -> String {
    let mut source = String::from("flowchart TD\n");
    for index in 0..nodes {
        if index > 0 {
            source.push_str(&format!("  N{} --> N{}\n", index - 1, index));
        }
        source.push_str(&format!("  N{index}[节点{index}]\n"));
    }
    source
}

fn report<E: ToString>(
    label: &str,
    size: &str,
    source: &str,
    first_ms: f64,
    second_ms: f64,
    first: Result<String, E>,
    second: Result<String, E>,
) {
    let (ok, output_bytes, diagnostic) = match (&first, &second) {
        (Ok(_), Ok(svg)) => (true, Some(svg.len()), None),
        (Err(error), _) | (_, Err(error)) => (false, None, Some(error.to_string())),
    };
    println!(
        "{}",
        serde_json::json!({
            (label): size,
            "ok": ok,
            "source_bytes": source.len(),
            "cold_ms": first_ms,
            "warm_ms": second_ms,
            "output_bytes": output_bytes,
            "diagnostic": diagnostic,
        })
    );
}
