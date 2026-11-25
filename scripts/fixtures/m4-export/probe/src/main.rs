// V-02 候选后端探针：用真实样本生成 PDF 与 DOCX，再由外部脚本回读。
//
// 本程序只做“按契约生成产物”这一件事；它**不是生产实现**，也不接入任何
// 工作区成员。输入由 `scripts/probe_m4_export.py` 通过 argv 给出：
//
//   m4-export-probe pdf  <out.pdf>  <font.ttf>  <image.png>  <cells.json>
//   m4-export-probe docx <out.docx> <font.ttf>  <image.png>  <docx.json>
//
// `cells.json` 描述 PDF 纸型矩阵的每一格；`docx.json` 描述样式组合。
// 失败一律非零退出，不产出半成品文件。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn fail(message: &str) -> ExitCode {
    eprintln!("PROBE_FAIL {message}");
    ExitCode::FAILURE
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return fail("缺少子命令 pdf|docx|syntax");
    }
    let result = match args[1].as_str() {
        "pdf" => pdf_main(&args[2..]),
        "docx" => docx_main(&args[2..]),
        "syntax" => syntax_main(&args[2..]),
        other => return fail(&format!("未知子命令 {other}")),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => fail(&message),
    }
}

// ---------------------------------------------------------------- JSON 读取

/// 只支持探针需要的极小子集：对象、数组、字符串、数字、布尔、null。
/// 手写解析避免为探针再引入 serde（探针不属于工作区，但仍保持零额外依赖）。
mod tinyjson {
    #[derive(Debug, Clone, PartialEq)]
    pub enum Value {
        Null,
        Bool(bool),
        Number(f64),
        String(String),
        Array(Vec<Value>),
        Object(Vec<(String, Value)>),
    }

    impl Value {
        pub fn get(&self, key: &str) -> Option<&Value> {
            match self {
                Value::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
                _ => None,
            }
        }

        pub fn as_str(&self) -> Option<&str> {
            match self {
                Value::String(s) => Some(s),
                _ => None,
            }
        }

        pub fn as_f64(&self) -> Option<f64> {
            match self {
                Value::Number(n) => Some(*n),
                _ => None,
            }
        }

        pub fn as_array(&self) -> Option<&[Value]> {
            match self {
                Value::Array(items) => Some(items),
                _ => None,
            }
        }
    }

    pub fn parse(text: &str) -> Result<Value, String> {
        let mut parser = Parser {
            bytes: text.as_bytes(),
            pos: 0,
        };
        let value = parser.value()?;
        parser.skip_ws();
        if parser.pos != parser.bytes.len() {
            return Err(format!("JSON 尾部有多余内容，位置 {}", parser.pos));
        }
        Ok(value)
    }

    struct Parser<'a> {
        bytes: &'a [u8],
        pos: usize,
    }

    impl<'a> Parser<'a> {
        fn skip_ws(&mut self) {
            while self.pos < self.bytes.len()
                && matches!(self.bytes[self.pos], b' ' | b'\t' | b'\n' | b'\r')
            {
                self.pos += 1;
            }
        }

        fn peek(&self) -> Option<u8> {
            self.bytes.get(self.pos).copied()
        }

        fn expect(&mut self, byte: u8) -> Result<(), String> {
            if self.peek() == Some(byte) {
                self.pos += 1;
                Ok(())
            } else {
                Err(format!("期望 {:?}，位置 {}", byte as char, self.pos))
            }
        }

        fn value(&mut self) -> Result<Value, String> {
            self.skip_ws();
            match self.peek() {
                Some(b'{') => self.object(),
                Some(b'[') => self.array(),
                Some(b'"') => Ok(Value::String(self.string()?)),
                Some(b't') => self.literal("true", Value::Bool(true)),
                Some(b'f') => self.literal("false", Value::Bool(false)),
                Some(b'n') => self.literal("null", Value::Null),
                Some(byte) if byte == b'-' || byte.is_ascii_digit() => self.number(),
                other => Err(format!(
                    "无法识别的 JSON 值起点 {other:?}，位置 {}",
                    self.pos
                )),
            }
        }

        fn literal(&mut self, text: &str, value: Value) -> Result<Value, String> {
            if self.bytes[self.pos..].starts_with(text.as_bytes()) {
                self.pos += text.len();
                Ok(value)
            } else {
                Err(format!("无法识别的字面量，位置 {}", self.pos))
            }
        }

        fn object(&mut self) -> Result<Value, String> {
            self.expect(b'{')?;
            let mut entries = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b'}') {
                self.pos += 1;
                return Ok(Value::Object(entries));
            }
            loop {
                self.skip_ws();
                let key = self.string()?;
                self.skip_ws();
                self.expect(b':')?;
                let value = self.value()?;
                entries.push((key, value));
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                    }
                    Some(b'}') => {
                        self.pos += 1;
                        return Ok(Value::Object(entries));
                    }
                    other => {
                        return Err(format!(
                            "对象缺少 , 或 }}，位置 {}，读到 {other:?}",
                            self.pos
                        ))
                    }
                }
            }
        }

        fn array(&mut self) -> Result<Value, String> {
            self.expect(b'[')?;
            let mut items = Vec::new();
            self.skip_ws();
            if self.peek() == Some(b']') {
                self.pos += 1;
                return Ok(Value::Array(items));
            }
            loop {
                items.push(self.value()?);
                self.skip_ws();
                match self.peek() {
                    Some(b',') => {
                        self.pos += 1;
                    }
                    Some(b']') => {
                        self.pos += 1;
                        return Ok(Value::Array(items));
                    }
                    other => {
                        return Err(format!(
                            "数组缺少 , 或 ]，位置 {}，读到 {other:?}",
                            self.pos
                        ))
                    }
                }
            }
        }

        fn string(&mut self) -> Result<String, String> {
            self.expect(b'"')?;
            let mut out = String::new();
            loop {
                let byte = self.peek().ok_or_else(|| "字符串未结束".to_string())?;
                self.pos += 1;
                match byte {
                    b'"' => return Ok(out),
                    b'\\' => {
                        let escape = self.peek().ok_or_else(|| "转义未结束".to_string())?;
                        self.pos += 1;
                        match escape {
                            b'"' => out.push('"'),
                            b'\\' => out.push('\\'),
                            b'/' => out.push('/'),
                            b'n' => out.push('\n'),
                            b't' => out.push('\t'),
                            b'r' => out.push('\r'),
                            b'u' => {
                                let hex = self
                                    .bytes
                                    .get(self.pos..self.pos + 4)
                                    .ok_or_else(|| "\\u 转义不完整".to_string())?;
                                let text = std::str::from_utf8(hex)
                                    .map_err(|_| "\\u 非 UTF-8".to_string())?;
                                let code = u32::from_str_radix(text, 16)
                                    .map_err(|_| format!("\\u 非法十六进制 {text}"))?;
                                self.pos += 4;
                                out.push(
                                    char::from_u32(code)
                                        .ok_or_else(|| "\\u 非标量值".to_string())?,
                                );
                            }
                            other => return Err(format!("未知转义 \\{}", other as char)),
                        }
                    }
                    _ => {
                        // 多字节 UTF-8：回退一个字节，按字符解码。
                        self.pos -= 1;
                        let rest = std::str::from_utf8(&self.bytes[self.pos..])
                            .map_err(|_| "字符串不是合法 UTF-8".to_string())?;
                        let ch = rest
                            .chars()
                            .next()
                            .ok_or_else(|| "字符串解码失败".to_string())?;
                        self.pos += ch.len_utf8();
                        out.push(ch);
                    }
                }
            }
        }

        fn number(&mut self) -> Result<Value, String> {
            let start = self.pos;
            if self.peek() == Some(b'-') {
                self.pos += 1;
            }
            while let Some(byte) = self.peek() {
                if byte.is_ascii_digit() || matches!(byte, b'.' | b'e' | b'E' | b'+' | b'-') {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            let text = std::str::from_utf8(&self.bytes[start..self.pos])
                .map_err(|_| "数字不是 ASCII".to_string())?;
            text.parse::<f64>()
                .map(Value::Number)
                .map_err(|_| format!("非法数字 {text}"))
        }
    }
}

fn load_json(path: &Path) -> Result<tinyjson::Value, String> {
    let text =
        fs::read_to_string(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    tinyjson::parse(&text).map_err(|e| format!("解析 {} 失败: {e}", path.display()))
}

// ---------------------------------------------------------------- PDF

/// 与 `scripts/m4_export_rules.py` 保持同一纸型表；探针只做生成，
/// 契约校验与回读比较在 Python 侧完成。
const PT_PER_MM: f64 = 72.0 / 25.4;

fn mm(value: f64) -> f32 {
    (value * PT_PER_MM) as f32
}

struct PdfCell {
    paper: String,
    orientation: String,
    margins: (f64, f64, f64, f64),
    expected_width_pt: f64,
    expected_height_pt: f64,
    body_lines: Vec<String>,
}

fn read_cells(value: &tinyjson::Value) -> Result<Vec<PdfCell>, String> {
    let items = value
        .as_array()
        .ok_or_else(|| "cells.json 顶层必须是数组".to_string())?;
    let mut cells = Vec::new();
    for (index, item) in items.iter().enumerate() {
        let paper = item
            .get("paper")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("第 {index} 格缺少 paper"))?;
        let orientation = item
            .get("orientation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| format!("第 {index} 格缺少 orientation"))?;
        let margins = item
            .get("margins")
            .ok_or_else(|| format!("第 {index} 格缺少 margins"))?;
        let side = |name: &str| -> Result<f64, String> {
            margins
                .get(name)
                .and_then(|v| v.as_f64())
                .ok_or_else(|| format!("第 {index} 格 margins.{name} 缺失或非数值"))
        };
        let expected_width_pt = item
            .get("expected_width_pt")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("第 {index} 格缺少 expected_width_pt"))?;
        let expected_height_pt = item
            .get("expected_height_pt")
            .and_then(|v| v.as_f64())
            .ok_or_else(|| format!("第 {index} 格缺少 expected_height_pt"))?;
        let lines = item
            .get("body_lines")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("第 {index} 格缺少 body_lines"))?
            .iter()
            .map(|line| {
                line.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| format!("第 {index} 格 body_lines 含非字符串"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        cells.push(PdfCell {
            paper: paper.to_string(),
            orientation: orientation.to_string(),
            margins: (side("top")?, side("right")?, side("bottom")?, side("left")?),
            expected_width_pt,
            expected_height_pt,
            body_lines: lines,
        });
    }
    if cells.is_empty() {
        return Err("cells.json 为空".to_string());
    }
    Ok(cells)
}

fn pdf_main(args: &[String]) -> Result<(), String> {
    if args.len() != 4 {
        return Err("用法: pdf <out.pdf> <font.ttf> <image.png> <cells.json>".to_string());
    }
    let out = PathBuf::from(&args[0]);
    let font_bytes = fs::read(&args[1]).map_err(|e| format!("读取字体失败: {e}"))?;
    let image_bytes = fs::read(&args[2]).map_err(|e| format!("读取图片失败: {e}"))?;
    let cells = read_cells(&load_json(Path::new(&args[3]))?)?;

    let font = krilla::text::Font::new(std::sync::Arc::new(font_bytes).into(), 0)
        .ok_or_else(|| "字体无法解析".to_string())?;
    let image = krilla::image::Image::from_png(image_bytes.into(), false)
        .map_err(|e| format!("图片无法解码: {e}"))?;

    let mut document = krilla::Document::new();
    for (index, cell) in cells.iter().enumerate() {
        let width = mm(cell.expected_width_pt / PT_PER_MM);
        let height = mm(cell.expected_height_pt / PT_PER_MM);
        let (top, right, bottom, left) = cell.margins;
        let mut page = document.start_page_with(
            krilla::page::PageSettings::from_wh(width, height)
                .ok_or_else(|| format!("第 {index} 格页面尺寸非正"))?,
        );
        let mut surface = page.surface();

        // krilla 的绘制坐标原点在左上、y 向下（见 surface 模块文档），
        // 与 PDF 用户空间（原点左下）相反；这里按左上角排版，
        // 回读时由 PDFKit 给出左下坐标，比较在 Python 侧换算。
        let left_pt = mm(left);
        let top_pt = mm(top);
        let bottom_pt = mm(bottom);
        let right_pt = mm(right);
        let content_width = width - left_pt - right_pt;
        let content_height = height - top_pt - bottom_pt;
        if content_width <= 0.0 || content_height <= 0.0 {
            return Err(format!("第 {index} 格边距挤占后内容区非正"));
        }

        // 用一条极细边框画出内容区，使“四边边距是否真的生效”成为可回读的
        // 几何事实，而不是只依赖文本墨迹位置（文本墨迹受字形侧边距影响）。
        let mut builder = krilla::geom::PathBuilder::new();
        builder.move_to(left_pt, top_pt);
        builder.line_to(left_pt + content_width, top_pt);
        builder.line_to(left_pt + content_width, top_pt + content_height);
        builder.line_to(left_pt, top_pt + content_height);
        builder.close();
        let frame = builder
            .finish()
            .ok_or_else(|| format!("第 {index} 格边框路径无效"))?;
        surface.set_fill(None);
        surface.set_stroke(Some(krilla::paint::Stroke {
            paint: krilla::color::rgb::Color::new(220, 20, 60).into(),
            width: 0.5,
            ..Default::default()
        }));
        surface.draw_path(&frame);
        surface.set_stroke(None);

        // 逐行文本：字号 12pt，行距 18pt，自上而下全部落在内容区内。
        let mut baseline = top_pt + 16.0;
        for line in &cell.body_lines {
            surface.draw_text(
                krilla::geom::Point::from_xy(left_pt + 4.0, baseline),
                font.clone(),
                12.0,
                line,
                false,
                krilla::text::TextDirection::Auto,
            );
            baseline += 18.0;
        }

        // 首格额外嵌入样本 PNG，证明位图资源路径可用（EXPORT-04 图片策略的 PDF 侧）。
        if index == 0 {
            let size = krilla::geom::Size::from_wh(96.0, 48.0)
                .ok_or_else(|| "图片尺寸非正".to_string())?;
            surface.push_transform(&krilla::geom::Transform::from_translate(
                left_pt + 4.0,
                top_pt + content_height - 52.0,
            ));
            surface.draw_image(image.clone(), size);
            surface.pop();
        }

        surface.finish();
        page.finish();
    }

    let bytes = document
        .finish()
        .map_err(|e| format!("PDF 生成失败: {e:?}"))?;
    fs::write(&out, &bytes).map_err(|e| format!("写入 {} 失败: {e}", out.display()))?;
    let labels: Vec<String> = cells
        .iter()
        .map(|cell| format!("{}/{}", cell.paper, cell.orientation))
        .collect();
    println!(
        "PDF_OK {} cells={} bytes={} matrix={}",
        out.display(),
        cells.len(),
        bytes.len(),
        labels.join(",")
    );
    Ok(())
}

// ---------------------------------------------------------------- DOCX

struct DocxPlan {
    heading_body: String,
    code_style: String,
    heading_sizes_half_pt: Vec<usize>,
    body_size_half_pt: usize,
    code_font: String,
    code_shaded: bool,
    code_fill: Option<String>,
    code_color: Option<String>,
    east_asia_font: String,
    ascii_font: String,
}

fn read_plan(value: &tinyjson::Value) -> Result<DocxPlan, String> {
    let heading_body = value
        .get("heading_body")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "docx.json 缺少 heading_body".to_string())?
        .to_string();
    let code_style = value
        .get("code_style")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "docx.json 缺少 code_style".to_string())?
        .to_string();
    let heading_sizes = value
        .get("heading_sizes_half_pt")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "docx.json 缺少 heading_sizes_half_pt".to_string())?
        .iter()
        .map(|item| {
            item.as_f64()
                .filter(|n| *n > 0.0)
                .map(|n| n as usize)
                .ok_or_else(|| "heading_sizes_half_pt 含非法值".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let body_size = value
        .get("body_size_half_pt")
        .and_then(|v| v.as_f64())
        .filter(|n| *n > 0.0)
        .map(|n| n as usize)
        .ok_or_else(|| "docx.json 缺少 body_size_half_pt".to_string())?;
    let code = value
        .get("code")
        .ok_or_else(|| "docx.json 缺少 code".to_string())?;
    let code_font = code
        .get("font")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "docx.json code.font 缺失".to_string())?
        .to_string();
    let code_shaded = match code.get("shaded") {
        Some(tinyjson::Value::Bool(flag)) => *flag,
        _ => return Err("docx.json code.shaded 缺失或非布尔".to_string()),
    };
    let code_fill = match code.get("fill") {
        Some(tinyjson::Value::String(text)) => Some(text.clone()),
        _ => None,
    };
    let code_color = match code.get("color") {
        Some(tinyjson::Value::String(text)) if text != "auto" => Some(text.clone()),
        _ => None,
    };
    if code_shaded && code_fill.is_none() {
        return Err("docx.json 要求底色但未给出 code.fill".to_string());
    }
    let east_asia_font = value
        .get("east_asia_font")
        .and_then(|v| v.as_str())
        .unwrap_or("PingFang SC")
        .to_string();
    let ascii_font = value
        .get("ascii_font")
        .and_then(|v| v.as_str())
        .unwrap_or("Helvetica")
        .to_string();
    if heading_sizes.len() != 3 {
        return Err("heading_sizes_half_pt 必须恰好 3 档".to_string());
    }
    Ok(DocxPlan {
        heading_body,
        code_style,
        heading_sizes_half_pt: heading_sizes,
        body_size_half_pt: body_size,
        code_font,
        code_shaded,
        code_fill,
        code_color,
        east_asia_font,
        ascii_font,
    })
}

fn docx_main(args: &[String]) -> Result<(), String> {
    use docx_rs::*;

    if args.len() != 4 {
        return Err("用法: docx <out.docx> <font.ttf> <image.png> <docx.json>".to_string());
    }
    let out = PathBuf::from(&args[0]);
    let image_bytes = fs::read(&args[2]).map_err(|e| format!("读取图片失败: {e}"))?;
    let plan = read_plan(&load_json(Path::new(&args[3]))?)?;

    let fonts = || {
        RunFonts::new()
            .ascii(plan.ascii_font.clone())
            .east_asia(plan.east_asia_font.clone())
    };
    let code_fonts = || {
        RunFonts::new()
            .ascii(plan.code_font.clone())
            .east_asia(plan.code_font.clone())
    };

    let mut docx = Docx::new();
    for (level, size) in plan.heading_sizes_half_pt.iter().enumerate() {
        let id = format!("Heading{}", level + 1);
        docx = docx.add_style(
            Style::new(id.clone(), StyleType::Paragraph)
                .name(format!("Heading {}", level + 1))
                .size(*size)
                .fonts(fonts()),
        );
    }
    let mut code_style = Style::new("Code", StyleType::Paragraph)
        .name("Code")
        .size(20)
        .fonts(code_fonts());
    if let Some(color) = &plan.code_color {
        code_style = code_style.color(color.clone());
    }
    if plan.code_shaded {
        // 底色必须落在**段落**属性（w:pPr/w:shd）：w:rPr 的 shd 只覆盖字形
        // 墨迹范围，行内空白与行尾都不着色，Word 里渲染成一条条断开的灰带，
        // 不是代码块。文字色仍留在 rPr。
        let fill = plan
            .code_fill
            .clone()
            .unwrap_or_else(|| "F3F4F6".to_string());
        code_style.paragraph_property = code_style
            .paragraph_property
            .shading(Shading::new().shd_type(ShdType::Clear).fill(fill));
    }
    docx = docx.add_style(code_style);
    // 引用样式：只要左边框。docx-rs 的 ParagraphBorders 默认四边都有边框，
    // 因此从 `with_empty()` 起再只加左边，否则引用块渲染成带框段落。
    let mut quote_style = Style::new("Quote", StyleType::Paragraph)
        .name("Quote")
        .fonts(fonts())
        .size(plan.body_size_half_pt)
        .indent(Some(480), None, None, None);
    quote_style.paragraph_property = quote_style.paragraph_property.set_borders(
        ParagraphBorders::with_empty().set(
            ParagraphBorder::new(ParagraphBorderPosition::Left)
                .size(12)
                .space(8)
                .color("D0D7DE"),
        ),
    );
    docx = docx.add_style(quote_style);
    // 分隔线样式：空段落 + 下边框。Word 里水平线不是图形，是段落边框。
    let mut rule_style = Style::new("Rule", StyleType::Paragraph)
        .name("Rule")
        .fonts(fonts());
    rule_style.paragraph_property = rule_style.paragraph_property.set_borders(
        ParagraphBorders::with_empty().set(
            ParagraphBorder::new(ParagraphBorderPosition::Bottom)
                .size(6)
                .space(1)
                .color("888888"),
        ),
    );
    docx = docx.add_style(rule_style);
    // 无序列表的 bullet 编号定义：默认编号 1 是十进制有序列表，
    // 无序项必须用另一份定义，否则 Word 会显示 "4. 无序项"。
    docx = docx.add_abstract_numbering(
        AbstractNumbering::new(2).add_level(
            Level::new(
                0,
                Start::new(1),
                NumberFormat::new("bullet"),
                LevelText::new("•"),
                LevelJc::new("left"),
            )
            .indent(Some(420), Some(SpecialIndentType::Hanging(420)), None, None),
        ),
    );
    docx = docx.add_numbering(Numbering::new(2, 2));

    // 结构：与 `scripts/m4_export_rules.py` 的 SYNTAX_MATRIX 同源。
    // 每个语法块在 DOCX 里落成真结构（真标题、真编号、真表格、真脚注），
    // 回读侧逐项断言这些结构存在且计数正确。
    docx = docx.add_paragraph(
        Paragraph::new()
            .style(&format!("Heading{}", 1))
            .add_run(Run::new().add_text("中文标题 Heading 1").fonts(fonts())),
    );
    // 行内标记：同一段落里混排普通、加粗、斜体与行内代码。
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text("正文段落 body text，含行内 ")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            )
            .add_run(
                Run::new()
                    .add_text("code")
                    .fonts(code_fonts())
                    .size(plan.body_size_half_pt),
            )
            .add_run(
                Run::new()
                    .add_text("、")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            )
            .add_run(
                Run::new()
                    .add_text("加粗")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt)
                    .bold(),
            )
            .add_run(
                Run::new()
                    .add_text("、")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            )
            .add_run(
                Run::new()
                    .add_text("斜体")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt)
                    .italic(),
            )
            .add_run(
                Run::new()
                    .add_text("。")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            ),
    );
    // 链接：外部超链接，关系写入 document.xml.rels。
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text("含")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            )
            .add_hyperlink(
                Hyperlink::new("https://example.com", HyperlinkType::External).add_run(
                    Run::new()
                        .add_text("链接")
                        .fonts(fonts())
                        .size(plan.body_size_half_pt)
                        .color("0563C1")
                        .underline("single"),
                ),
            )
            .add_run(
                Run::new()
                    .add_text("的段落。")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            ),
    );
    docx = docx.add_paragraph(
        Paragraph::new()
            .style("Heading2")
            .add_run(Run::new().add_text("二级标题 Section").fonts(fonts())),
    );
    docx = docx.add_paragraph(
        Paragraph::new()
            .style("Heading3")
            .add_run(Run::new().add_text("三级标题 Sub").fonts(fonts())),
    );
    // 编号由 OOXML 编号定义生成（LevelText "%1."），文本里不得再写序号，
    // 否则渲染成 "1. 1. 第一项"。
    for item in [
        "第一项 first item",
        "第二项 second item",
        "第三项 third item",
    ] {
        docx = docx.add_paragraph(
            Paragraph::new()
                .numbering(NumberingId::new(1), IndentLevel::new(0))
                .add_run(Run::new().add_text(item).fonts(fonts())),
        );
    }
    // 无序列表用 bullet 编号定义（编号 2），不是手写圆点。
    for item in ["无序项 bullet one", "无序项 bullet two"] {
        docx = docx.add_paragraph(
            Paragraph::new()
                .numbering(NumberingId::new(2), IndentLevel::new(0))
                .add_run(Run::new().add_text(item).fonts(fonts())),
        );
    }
    // 引用块：左边框由 Quote 样式给出，段落自身只带内容。
    docx = docx.add_paragraph(
        Paragraph::new().style("Quote").add_run(
            Run::new()
                .add_text("引用块 quote text")
                .fonts(fonts())
                .color("57606A"),
        ),
    );
    // 显式列宽：docx-rs 不会从单元格推导 <w:tblGrid>，不设就是空网格，
    // 真实 Word 打开会按默认等分，样式回读也失去“列数”这一事实。
    docx = docx.add_table(
        Table::new(vec![
            TableRow::new(vec![
                TableCell::new().add_paragraph(
                    Paragraph::new().add_run(Run::new().add_text("列 A").fonts(fonts())),
                ),
                TableCell::new().add_paragraph(
                    Paragraph::new().add_run(Run::new().add_text("列 B").fonts(fonts())),
                ),
            ]),
            TableRow::new(vec![
                TableCell::new().add_paragraph(
                    Paragraph::new().add_run(Run::new().add_text("甲").fonts(fonts())),
                ),
                TableCell::new().add_paragraph(
                    Paragraph::new().add_run(Run::new().add_text("乙").fonts(fonts())),
                ),
            ]),
        ])
        .set_grid(vec![2400, 2400]),
    );
    for line in ["fn main() {", "    println!(\"中文 code\");", "}"] {
        let mut run = Run::new().add_text(line).fonts(code_fonts());
        if let Some(color) = &plan.code_color {
            run = run.color(color.clone());
        }
        docx = docx.add_paragraph(Paragraph::new().style("Code").add_run(run));
    }
    docx =
        docx.add_paragraph(Paragraph::new().add_run(Run::new().add_image(Pic::new(&image_bytes))));
    // 分隔线：空段落 + Rule 样式的下边框，不是表格线。
    docx = docx.add_paragraph(Paragraph::new().style("Rule"));
    // 公式与 Mermaid：本轮只落源文本，不做数学/图表排版（明确的可编辑性边界）。
    for text in ["行内公式 E = mc^2 与后文。", "$$", "a^2 + b^2 = c^2", "$$"] {
        docx = docx.add_paragraph(
            Paragraph::new().add_run(
                Run::new()
                    .add_text(text)
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            ),
        );
    }
    for text in ["```mermaid", "graph TD", "  A[开始] --> B[结束]", "```"] {
        docx = docx.add_paragraph(
            Paragraph::new()
                .style("Code")
                .add_run(Run::new().add_text(text).fonts(code_fonts())),
        );
    }
    // 脚注：正文里的引用标记与文末脚注内容都由 docx-rs 生成真脚注部件。
    let footnote = Footnote::new().add_content(
        Paragraph::new().add_run(Run::new().add_text("脚注内容 footnote body").fonts(fonts())),
    );
    docx = docx.add_paragraph(
        Paragraph::new()
            .add_run(
                Run::new()
                    .add_text("末尾段落 end paragraph")
                    .fonts(fonts())
                    .size(plan.body_size_half_pt),
            )
            .add_run(Run::new().add_footnote_reference(footnote)),
    );

    let file = fs::File::create(&out).map_err(|e| format!("创建 {} 失败: {e}", out.display()))?;
    let mut writer = std::io::BufWriter::new(file);
    docx.build()
        .pack(&mut writer)
        .map_err(|e| format!("写入 DOCX 失败: {e}"))?;
    let bytes = fs::metadata(&out).map(|meta| meta.len()).unwrap_or(0);
    println!(
        "DOCX_OK {} heading_body={} code_style={} bytes={}",
        out.display(),
        plan.heading_body,
        plan.code_style,
        bytes
    );
    Ok(())
}

// ---------------------------------------------------------------- 语法页（PDF）

/// 语法页的一条绘制指令；布局坐标由 Python 侧的 `syntax_pdf_document()`
/// 算好，这里只按坐标执行。分页信息随指令一起给出（`page` 字段），
/// 这样“排到第几页”也是可断言的契约事实，而不是实现细节。
fn syntax_main(args: &[String]) -> Result<(), String> {
    use krilla::geom::{Point, Rect, Size};
    use krilla::paint::{Fill, Stroke};

    if args.len() != 4 {
        return Err("用法: syntax <out.pdf> <font.ttf> <image.png> <syntax.json>".to_string());
    }
    let out = PathBuf::from(&args[0]);
    let font_bytes = fs::read(&args[1]).map_err(|e| format!("读取字体失败: {e}"))?;
    let image_bytes = fs::read(&args[2]).map_err(|e| format!("读取图片失败: {e}"))?;
    let plan = load_json(Path::new(&args[3]))?;

    let page_width_pt = plan
        .get("page_width_pt")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "syntax.json 缺少 page_width_pt".to_string())?;
    let page_height_pt = plan
        .get("page_height_pt")
        .and_then(|v| v.as_f64())
        .ok_or_else(|| "syntax.json 缺少 page_height_pt".to_string())?;
    let pages = plan
        .get("pages")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "syntax.json 缺少 pages".to_string())?;
    if pages.is_empty() {
        return Err("syntax.json 的 pages 为空".to_string());
    }

    let font = krilla::text::Font::new(std::sync::Arc::new(font_bytes).into(), 0)
        .ok_or_else(|| "字体无法解析".to_string())?;
    let image = krilla::image::Image::from_png(image_bytes.into(), false)
        .map_err(|e| format!("图片无法解码: {e}"))?;

    let mut document = krilla::Document::new();
    for (page_index, page) in pages.iter().enumerate() {
        let ops = page
            .get("ops")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("第 {page_index} 页缺少 ops"))?;
        let settings =
            krilla::page::PageSettings::from_wh(page_width_pt as f32, page_height_pt as f32)
                .ok_or_else(|| format!("第 {page_index} 页尺寸非正"))?;
        let mut page_handle = document.start_page_with(settings);
        let mut surface = page_handle.surface();
        // 注释不能与 surface 同时持有 page 的可变借用，先收集，surface 结束后再挂。
        let mut annotations = Vec::new();
        for (op_index, op) in ops.iter().enumerate() {
            let kind = op
                .get("op")
                .and_then(|v| v.as_str())
                .ok_or_else(|| format!("第 {page_index} 页第 {op_index} 条指令缺少 op"))?;
            let number = |key: &str| -> Result<f32, String> {
                op.get(key)
                    .and_then(|v| v.as_f64())
                    .map(|n| n as f32)
                    .ok_or_else(|| format!("第 {page_index} 页第 {op_index} 条 {kind} 缺少 {key}"))
            };
            match kind {
                "text" => {
                    let text = op.get("text").and_then(|v| v.as_str()).ok_or_else(|| {
                        format!("第 {page_index} 页第 {op_index} 条 text 缺少内容")
                    })?;
                    surface.draw_text(
                        Point::from_xy(number("x")?, number("y")?),
                        font.clone(),
                        number("size")?,
                        text,
                        false,
                        krilla::text::TextDirection::Auto,
                    );
                }
                "rect" => {
                    let fill = op.get("fill").and_then(|v| v.as_str()).ok_or_else(|| {
                        format!("第 {page_index} 页第 {op_index} 条 rect 缺少 fill")
                    })?;
                    let (r, g, b) = parse_hex_color(fill)?;
                    let mut builder = krilla::geom::PathBuilder::new();
                    let x = number("x")?;
                    let y = number("y")?;
                    builder.move_to(x, y);
                    builder.line_to(x + number("w")?, y);
                    builder.line_to(x + number("w")?, y + number("h")?);
                    builder.line_to(x, y + number("h")?);
                    builder.close();
                    let path = builder
                        .finish()
                        .ok_or_else(|| "rect 路径无效".to_string())?;
                    surface.set_fill(Some(Fill {
                        paint: krilla::color::rgb::Color::new(r, g, b).into(),
                        ..Default::default()
                    }));
                    surface.draw_path(&path);
                    surface.set_fill(None);
                }
                "line" => {
                    let color = op.get("color").and_then(|v| v.as_str()).ok_or_else(|| {
                        format!("第 {page_index} 页第 {op_index} 条 line 缺少 color")
                    })?;
                    let (r, g, b) = parse_hex_color(color)?;
                    let mut builder = krilla::geom::PathBuilder::new();
                    builder.move_to(number("x1")?, number("y1")?);
                    builder.line_to(number("x2")?, number("y2")?);
                    let path = builder
                        .finish()
                        .ok_or_else(|| "line 路径无效".to_string())?;
                    surface.set_stroke(Some(Stroke {
                        paint: krilla::color::rgb::Color::new(r, g, b).into(),
                        width: number("width")?,
                        ..Default::default()
                    }));
                    surface.draw_path(&path);
                    surface.set_stroke(None);
                }
                "image" => {
                    let size = Size::from_wh(number("w")?, number("h")?)
                        .ok_or_else(|| "image 尺寸非正".to_string())?;
                    surface.push_transform(&krilla::geom::Transform::from_translate(
                        number("x")?,
                        number("y")?,
                    ));
                    surface.draw_image(image.clone(), size);
                    surface.pop();
                }
                "link" => {
                    // 链接注释的矩形同样是左上原点坐标，需换算成 PDF 用户空间
                    // （原点左下）后再交给 krilla。
                    let x = number("x")?;
                    let y = number("y")?;
                    let w = number("w")?;
                    let h = number("h")?;
                    let rect = Rect::from_xywh(x, page_height_pt as f32 - y - h, w, h)
                        .ok_or_else(|| "link 矩形无效".to_string())?;
                    let uri = op
                        .get("uri")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| "link 缺少 uri".to_string())?;
                    annotations.push((rect, uri.to_string()));
                }
                other => return Err(format!("未知绘制指令: {other}")),
            }
        }
        surface.finish();
        for (rect, uri) in annotations {
            let target = krilla::annotation::Target::Action(krilla::action::Action::Link(
                krilla::action::LinkAction::new(uri),
            ));
            page_handle
                .add_annotation(krilla::annotation::LinkAnnotation::new(rect, target).into());
        }
        page_handle.finish();
    }

    let bytes = document
        .finish()
        .map_err(|e| format!("PDF 生成失败: {e:?}"))?;
    fs::write(&out, &bytes).map_err(|e| format!("写入 {} 失败: {e}", out.display()))?;
    println!(
        "SYNTAX_OK {} pages={} bytes={}",
        out.display(),
        pages.len(),
        bytes.len()
    );
    Ok(())
}

/// 解析 `RRGGBB`；非法颜色直接失败，不静默退回黑色。
fn parse_hex_color(text: &str) -> Result<(u8, u8, u8), String> {
    if text.len() != 6 {
        return Err(format!("颜色必须是 RRGGBB: {text}"));
    }
    let parse = |range: std::ops::Range<usize>| -> Result<u8, String> {
        u8::from_str_radix(&text[range], 16).map_err(|_| format!("颜色必须是十六进制: {text}"))
    };
    Ok((parse(0..2)?, parse(2..4)?, parse(4..6)?))
}
