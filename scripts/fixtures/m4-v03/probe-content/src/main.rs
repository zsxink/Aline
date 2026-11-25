//! V-03 内容侧探针：公式语法与 Mermaid 图类型/主题。
//!
//! 这两个候选后端（`latex-rust`、`mermaid-rs-renderer`）都**尚未准入**：
//! 前者拉 `ttf-parser`（RUSTSEC-2026-0192 提示 unmaintained），后者直接依赖
//! 仓库禁库 `regex` 且把 resvg/usvg 0.47 带进树里。本探针只量“支持到什么
//! 程度”，准入裁决在任务 1.5。若准入否决，本探针的结论仍然有效——它描述的
//! 是候选能力，不是生产承诺。
//!
//! 子命令：
//! - `math <case>...`：逐条渲染 TeX，报告成功/诊断与产物长度。
//! - `mermaid <type>...`：逐类渲染最小图，报告 SVG 长度与是否含文字节点。
//! - `theme <file>...`：读 JSON 主题，报告候选后端能否反序列化、以及产物里
//!   注入的字符串是否被原样写进 SVG（用于证明“后端不校验，必须我们自己校验”）。

use latex_rust::{latex_to_svg, MathFont, SvgOptions};
use mermaid_rs_renderer::{RenderOptions, Theme};

fn main() {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_default();
    let rest: Vec<String> = arguments.collect();
    match command.as_str() {
        "math" => math(&rest),
        "mermaid" => mermaid(&rest),
        "theme" => theme(&rest),
        other => println!(
            "{}",
            serde_json::json!({ "error": format!("unknown command: {other}") })
        ),
    }
}

/// 逐条渲染公式。`case` 是驱动给出的稳定 id，`source` 是 TeX 原文（经 stdin
/// 传入以免 shell 转义破坏反斜杠）。这里从环境变量读原文，一行一条。
fn math(cases: &[String]) {
    let font = match MathFont::stix_two_math() {
        Ok(font) => font,
        Err(error) => {
            println!("{}", serde_json::json!({ "error": error.to_string() }));
            return;
        }
    };
    for case in cases {
        let Some(source) = std::env::var(format!("M4V03_TEX_{case}")).ok() else {
            println!(
                "{}",
                serde_json::json!({ "case": case, "error": "missing source" })
            );
            continue;
        };
        match latex_to_svg(&source, &font, &SvgOptions::new()) {
            Ok(svg) => println!(
                "{}",
                serde_json::json!({
                    "case": case,
                    "ok": true,
                    "svg_bytes": svg.len(),
                    "path_count": svg.matches("<path").count(),
                })
            ),
            Err(error) => println!(
                "{}",
                serde_json::json!({
                    "case": case,
                    "ok": false,
                    "diagnostic": error.to_string(),
                })
            ),
        }
    }
}

/// 每类图的最小合法样本。刻意保持最小：只证明“这类图能被解析并产出 SVG”，
/// 不追求覆盖该类型的全部语法——语法覆盖属 4.2 的实现验收。
fn mermaid_sample(kind: &str) -> Option<&'static str> {
    Some(match kind {
        "flowchart" => "flowchart TD\n  A[开始] --> B{判断}\n  B -->|是| C[结束]\n",
        "sequenceDiagram" => "sequenceDiagram\n  participant A as 甲\n  participant B as 乙\n  A->>B: 你好\n  B-->>A: 回应\n",
        "classDiagram" => "classDiagram\n  class Animal {\n    +String name\n    +eat()\n  }\n  Animal <|-- Dog\n",
        "stateDiagram-v2" => "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running: start\n",
        "erDiagram" => "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n",
        "pie" => "pie title 份额\n  \"甲\" : 40\n  \"乙\" : 60\n",
        "gantt" => "gantt\n  title 计划\n  dateFormat YYYY-MM-DD\n  section 阶段\n  任务一 :a1, 2026-01-01, 30d\n",
        "journey" => "journey\n  title 旅程\n  section 开始\n    打开: 5: 我\n",
        "timeline" => "timeline\n  title 时间线\n  2026 : 事件一 : 事件二\n",
        "mindmap" => "mindmap\n  root((中心))\n    分支一\n      叶子\n",
        "gitGraph" => "gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n  checkout main\n  merge dev\n",
        "xychart-beta" => "xychart-beta\n  x-axis [甲, 乙]\n  y-axis 0 --> 10\n  bar [3, 7]\n",
        "quadrantChart" => "quadrantChart\n  x-axis 低 --> 高\n  y-axis 低 --> 高\n  quadrant-1 甲区\n  A: [0.3, 0.6]\n",
        "sankey-beta" => "sankey-beta\n\nA,B,10\nB,C,5\n",
        "kanban" => "kanban\n  todo[待办]\n    task1[任务一]\n",
        "C4Context" => "C4Context\n  title 系统\n  Person(u, \"用户\")\n  System(s, \"系统\")\n  Rel(u, s, \"使用\")\n",
        "block-beta" => "block-beta\n  columns 2\n  a[\"甲\"] b[\"乙\"]\n",
        "architecture-beta" => "architecture-beta\n  group api(cloud)[接口]\n  service db(database)[库] in api\n",
        "requirementDiagram" => "requirementDiagram\n  requirement r1 {\n    id: 1\n    text: 需求\n    risk: high\n    verifymethod: test\n  }\n",
        "zenuml" => "zenuml\n  title 演示\n  A->B: 调用\n",
        "packet-beta" => "packet-beta\n  0-7: \"源\"\n  8-15: \"目标\"\n",
        "radar-beta" => "radar-beta\n  axis a[\"甲\"], b[\"乙\"]\n  curve c[\"曲线\"]{1, 2}\n",
        "treemap" => "treemap\n  \"根\"\n    \"甲\" 10\n    \"乙\" 20\n",
        _ => return None,
    })
}

fn mermaid(kinds: &[String]) {
    for kind in kinds {
        let Some(source) = mermaid_sample(kind) else {
            println!(
                "{}",
                serde_json::json!({ "type": kind, "error": "no sample" })
            );
            continue;
        };
        match mermaid_rs_renderer::render(source) {
            Ok(svg) => println!(
                "{}",
                serde_json::json!({
                    "type": kind,
                    "ok": true,
                    "svg_bytes": svg.len(),
                    "has_text": svg.contains("<text"),
                    "has_cjk": svg.contains('甲') || svg.contains('开'),
                })
            ),
            Err(error) => println!(
                "{}",
                serde_json::json!({
                    "type": kind,
                    "ok": false,
                    "diagnostic": error.to_string(),
                })
            ),
        }
    }
}

/// 主题：把每个字段分别替换成注入串，报告 (a) 后端是否接受、(b) 注入串是否
/// 原样进入 SVG。两者都为“是”时，说明**后端不做任何校验**，生产必须前置校验。
fn theme(files: &[String]) {
    for file in files {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) => {
                println!(
                    "{}",
                    serde_json::json!({ "file": file, "error": error.to_string() })
                );
                continue;
            }
        };
        match serde_json::from_str::<Theme>(&text) {
            Ok(theme) => {
                // 记录反序列化接受了多少字段（未知字段被静默忽略是后端缺陷）。
                let rendered = mermaid_rs_renderer::render_with_options(
                    "flowchart TD\n  A[甲] --> B[乙]\n",
                    RenderOptions {
                        theme,
                        ..Default::default()
                    },
                );
                match rendered {
                    Ok(svg) => println!(
                        "{}",
                        serde_json::json!({
                            "file": file,
                            "accepted": true,
                            "svg_bytes": svg.len(),
                            "svg_has_script": svg.contains("<script"),
                            "svg_has_onload": svg.contains("onload="),
                            "svg_has_javascript": svg.contains("javascript:"),
                            "svg_has_url": svg.contains("url("),
                        })
                    ),
                    Err(error) => println!(
                        "{}",
                        serde_json::json!({
                            "file": file,
                            "accepted": true,
                            "render_error": error.to_string(),
                        })
                    ),
                }
            }
            Err(error) => println!(
                "{}",
                serde_json::json!({
                    "file": file,
                    "accepted": false,
                    "diagnostic": error.to_string(),
                })
            ),
        }
    }
}
