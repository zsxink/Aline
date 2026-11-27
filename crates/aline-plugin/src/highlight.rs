//! 代码高亮：锁定语言集上的语言识别与语法类别跨度（PLUGIN-05 / PL-02）。
//!
//! 输入是围栏 info 与代码块绘制文本，输出是相对整段绘制文本的字节跨度；
//! 未覆盖的字节（空白、未分类 scope）由消费方按 `codeBlock.color` 绘制。
//! 本模块只做纯计算，不持有 GPUI 实体、不读取配置、不做 I/O；语言集在
//! 首次使用时一次性加载并常驻。
//!
//! 语言识别口径（2026-11-27 用户裁决）：围栏 info 取首个空白分隔记号，
//! 大小写不敏感地匹配锁定默认集的语法名与定义名别名；不按文件扩展名
//! 猜测。`Plain Text` 是加载器附带的兜底定义，不计入支持集。

use std::ops::Range;
use std::sync::OnceLock;

use syntect::parsing::{ParseState, Scope, ScopeStack, SyntaxSet};

/// 主题包 `syntax.*` 的 14 个有限语义类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyntaxCategory {
    /// 注释。
    Comment,
    /// 关键字。
    Keyword,
    /// 字符串。
    String,
    /// 数字。
    Number,
    /// 函数名。
    Function,
    /// 类型名。
    Type,
    /// 运算符。
    Operator,
    /// 标点。
    Punctuation,
    /// 标签名。
    Tag,
    /// 属性名。
    Attribute,
    /// 变量。
    Variable,
    /// 宏。
    Macro,
    /// 插入内容（diff 等）。
    Inserted,
    /// 删除内容（diff 等）。
    Deleted,
}

/// 一个语法类别跨度，`range` 是相对整段代码绘制文本的字节偏移。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSpan {
    /// 字节区间，保证落在 UTF-8 字符边界上且不含行终止符。
    pub range: Range<usize>,
    /// 该跨度的语义类别。
    pub category: SyntaxCategory,
}

/// 高亮成功的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedCode {
    /// 规范语法名（锁定默认集的 `SyntaxReference.name`）。
    ///
    /// 仅用于测试与诊断；围栏原始 info 由渲染层另行显示，不得用本字段
    /// 替换标签。
    pub language: &'static str,
    /// 按偏移升序、相邻同类别已合并、互不重叠的跨度。
    pub spans: Vec<CodeSpan>,
}

/// 高亮计算结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HighlightOutcome {
    /// 语言已识别且着色完成；空文本返回空跨度。
    Highlighted(HighlightedCode),
    /// 空 info、未知语言或 `Plain Text` 兜底定义：按普通代码回退。
    Unsupported,
    /// 已识别但语法状态机报错：按普通代码回退。
    Failed,
}

/// 加载锁定默认集（一次性）并预热类别表。
pub fn preload_languages() {
    let _ = syntax_set();
    let _ = scope_categories();
}

/// 返回支持集的规范语法名，按字母序排列，不含 `Plain Text`。
pub fn supported_languages() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = syntax_set()
        .syntaxes()
        .iter()
        .map(|syntax| syntax.name.as_str())
        .filter(|name| *name != PLAIN_TEXT)
        .collect();
    names.sort_unstable();
    names
}

/// 把围栏 info 解析为规范语法名。
///
/// 先按完整 info 匹配（支持含空格的定义名），再取首个空白分隔记号匹配，
/// 均大小写不敏感；先匹配语法名，再匹配冻结定义名别名；空 info、未知
/// 语言与 `Plain Text` 返回 `None`。
pub fn resolve_language(info: &str) -> Option<&'static str> {
    let trimmed = info.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(language) = match_token(trimmed) {
        return Some(language);
    }
    let token = trimmed.split_whitespace().next()?;
    if token == trimmed {
        return None;
    }
    match_token(token)
}

/// 把单个记号或完整 info 与语法名/冻结别名匹配。
fn match_token(token: &str) -> Option<&'static str> {
    let token = normalize(token);
    if token.is_empty() {
        return None;
    }
    let set = syntax_set();
    if let Some(syntax) = set
        .syntaxes()
        .iter()
        .find(|syntax| syntax.name != PLAIN_TEXT && normalize(&syntax.name) == token)
    {
        return Some(syntax.name.as_str());
    }
    ALIASES
        .iter()
        .find(|(alias, _)| normalize(alias) == token)
        .and_then(|(_, canonical)| {
            set.syntaxes()
                .iter()
                .find(|syntax| syntax.name == *canonical)
        })
        .map(|syntax| syntax.name.as_str())
}

/// 对整段代码绘制文本着色。
///
/// `text` 是代码块正文（含行间换行，不含围栏行），返回的跨度相对
/// `text` 起点；行终止符不进入跨度。
pub fn highlight_code(info: &str, text: &str) -> HighlightOutcome {
    let Some(language) = resolve_language(info) else {
        return HighlightOutcome::Unsupported;
    };
    let set = syntax_set();
    let Some(syntax) = set.syntaxes().iter().find(|syntax| syntax.name == language) else {
        return HighlightOutcome::Unsupported;
    };
    let mut state = ParseState::new(syntax);
    let mut stack = ScopeStack::new();
    let mut spans: Vec<CodeSpan> = Vec::new();
    let mut line_start = 0usize;
    for line in text.split_inclusive('\n') {
        let mut content_len = line.len();
        if line.ends_with('\n') {
            content_len -= 1;
            if line[..content_len].ends_with('\r') {
                content_len -= 1;
            }
        }
        let Ok(ops) = state.parse_line(line, set) else {
            return HighlightOutcome::Failed;
        };
        let mut last = 0usize;
        for (index, op) in ops {
            if index > last
                && let Some(category) = category_for(&stack)
            {
                push_span(
                    &mut spans,
                    text,
                    line_start + last,
                    (line_start + index).min(line_start + content_len),
                    category,
                );
            }
            if stack.apply(&op).is_err() {
                return HighlightOutcome::Failed;
            }
            last = last.max(index);
        }
        if last < content_len
            && let Some(category) = category_for(&stack)
        {
            push_span(
                &mut spans,
                text,
                line_start + last,
                line_start + content_len,
                category,
            );
        }
        line_start += line.len();
    }
    HighlightOutcome::Highlighted(HighlightedCode { language, spans })
}

/// 加载器附带的兜底定义，不属于冻结的 74 项支持集。
const PLAIN_TEXT: &str = "Plain Text";

/// 冻结定义名与运行期语法名的差异别名（2026-11-27 支持清单逐项核对）。
///
/// 键是围栏记号（`包/名称` 中的名称），值是该定义的 `SyntaxReference.name`。
/// 其余 67 项按名称规范化后即可命中，不在此表重复。
const ALIASES: [(&str, &str); 7] = [
    ("bash", "Bourne Again Shell (bash)"),
    ("build", "NAnt Build File"),
    ("cargo", "Cargo Build Results"),
    ("dot", "Graphviz (DOT)"),
    ("html-asp", "HTML (ASP)"),
    ("java server pages (jsp)", "Java Server Page (JSP)"),
    ("regexp", "Regular Expression"),
];

/// 记号与语法名的比较形式：去首尾空白、转小写、去掉空格/连字符/下划线。
fn normalize(token: &str) -> String {
    token.trim().to_lowercase().replace([' ', '-', '_'], "")
}

/// 从内向外取第一个可分类 scope 的语义类别。
fn category_for(stack: &ScopeStack) -> Option<SyntaxCategory> {
    for scope in stack.as_slice().iter().rev() {
        for (prefix, category) in scope_categories() {
            if prefix.is_prefix_of(*scope) {
                return Some(*category);
            }
        }
    }
    None
}

/// 追加跨度：裁剪到文本与字符边界、去掉空跨度、合并相邻同类别。
fn push_span(
    spans: &mut Vec<CodeSpan>,
    text: &str,
    mut start: usize,
    mut end: usize,
    category: SyntaxCategory,
) {
    start = start.min(text.len());
    end = end.min(text.len());
    while start < end && !text.is_char_boundary(start) {
        start += 1;
    }
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }
    if start >= end {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.category == category
        && last.range.end == start
    {
        last.range.end = end;
        return;
    }
    spans.push(CodeSpan {
        range: start..end,
        category,
    });
}

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn scope_categories() -> &'static [(Scope, SyntaxCategory)] {
    static TABLE: OnceLock<Vec<(Scope, SyntaxCategory)>> = OnceLock::new();
    TABLE.get_or_init(|| {
        const PREFIXES: [(&str, SyntaxCategory); 31] = [
            ("punctuation.definition.comment", SyntaxCategory::Comment),
            ("comment", SyntaxCategory::Comment),
            ("punctuation.definition.string", SyntaxCategory::String),
            ("string", SyntaxCategory::String),
            ("constant.character.escape", SyntaxCategory::String),
            ("constant.numeric", SyntaxCategory::Number),
            ("constant.language", SyntaxCategory::Keyword),
            ("keyword.operator", SyntaxCategory::Operator),
            ("keyword", SyntaxCategory::Keyword),
            ("storage.type", SyntaxCategory::Keyword),
            ("storage.modifier", SyntaxCategory::Keyword),
            ("entity.name.function.macro", SyntaxCategory::Macro),
            ("support.macro", SyntaxCategory::Macro),
            ("entity.name.function", SyntaxCategory::Function),
            ("entity.name.method", SyntaxCategory::Function),
            ("support.function", SyntaxCategory::Function),
            ("entity.name.type", SyntaxCategory::Type),
            ("entity.name.class", SyntaxCategory::Type),
            ("entity.name.struct", SyntaxCategory::Type),
            ("entity.name.enum", SyntaxCategory::Type),
            ("entity.name.trait", SyntaxCategory::Type),
            ("entity.name.interface", SyntaxCategory::Type),
            ("support.type", SyntaxCategory::Type),
            ("support.class", SyntaxCategory::Type),
            ("entity.name.tag", SyntaxCategory::Tag),
            ("entity.other.attribute-name", SyntaxCategory::Attribute),
            ("variable", SyntaxCategory::Variable),
            ("support.variable", SyntaxCategory::Variable),
            ("markup.inserted", SyntaxCategory::Inserted),
            ("markup.deleted", SyntaxCategory::Deleted),
            ("punctuation", SyntaxCategory::Punctuation),
        ];
        PREFIXES
            .iter()
            .filter_map(|(prefix, category)| {
                Scope::new(prefix).ok().map(|scope| (scope, *category))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlighted(outcome: HighlightOutcome) -> Result<HighlightedCode, String> {
        match outcome {
            HighlightOutcome::Highlighted(code) => Ok(code),
            other => Err(format!("期望 Highlighted，实际 {other:?}")),
        }
    }

    fn category_of(text: &str, code: &HighlightedCode, needle: &str) -> Option<SyntaxCategory> {
        code.spans
            .iter()
            .find(|span| &text[span.range.clone()] == needle)
            .map(|span| span.category)
    }

    const RUST_SAMPLE: &str = "fn main() {\n    let s = \"hi\";\n    let n = 42;\n    // note\n}\n";

    #[test]
    fn supported_languages_are_the_frozen_74_sorted_without_plain_text() -> Result<(), String> {
        let languages = supported_languages();
        assert_eq!(languages.len(), 74);
        let mut sorted = languages.clone();
        sorted.sort_unstable();
        assert_eq!(languages, sorted);
        assert!(languages.contains(&"Rust"));
        assert!(languages.contains(&"Python"));
        assert!(languages.contains(&"Bourne Again Shell (bash)"));
        assert!(!languages.contains(&"Plain Text"));
        Ok(())
    }

    #[test]
    fn resolve_language_trims_folds_case_and_takes_the_first_token() -> Result<(), String> {
        assert_eq!(resolve_language("Rust"), Some("Rust"));
        assert_eq!(resolve_language("  PyThOn  "), Some("Python"));
        assert_eq!(resolve_language("rust linenos"), Some("Rust"));
        assert_eq!(resolve_language(""), None);
        assert_eq!(resolve_language("   "), None);
        assert_eq!(resolve_language("brainfuck"), None);
        Ok(())
    }

    #[test]
    fn resolve_language_matches_frozen_definition_aliases() -> Result<(), String> {
        assert_eq!(resolve_language("bash"), Some("Bourne Again Shell (bash)"));
        assert_eq!(resolve_language("cargo"), Some("Cargo Build Results"));
        assert_eq!(resolve_language("dot"), Some("Graphviz (DOT)"));
        assert_eq!(resolve_language("regexp"), Some("Regular Expression"));
        assert_eq!(resolve_language("html-asp"), Some("HTML (ASP)"));
        assert_eq!(resolve_language("build"), Some("NAnt Build File"));
        assert_eq!(
            resolve_language("java server pages (jsp)"),
            Some("Java Server Page (JSP)")
        );
        Ok(())
    }

    #[test]
    fn resolve_language_does_not_guess_by_file_extension() -> Result<(), String> {
        assert_eq!(resolve_language("rs"), None);
        assert_eq!(resolve_language("py"), None);
        assert_eq!(resolve_language("cpp"), None);
        assert_eq!(resolve_language("js"), None);
        assert_eq!(resolve_language("sh"), None);
        assert_eq!(resolve_language("plain text"), None);
        Ok(())
    }

    #[test]
    fn rust_sample_yields_keyword_function_string_number_operator_comment() -> Result<(), String> {
        let code = highlighted(highlight_code("rust", RUST_SAMPLE))?;
        assert_eq!(code.language, "Rust");
        assert_eq!(
            category_of(RUST_SAMPLE, &code, "fn"),
            Some(SyntaxCategory::Keyword)
        );
        assert_eq!(
            category_of(RUST_SAMPLE, &code, "let"),
            Some(SyntaxCategory::Keyword)
        );
        assert_eq!(
            category_of(RUST_SAMPLE, &code, "main"),
            Some(SyntaxCategory::Function)
        );
        assert_eq!(
            category_of(RUST_SAMPLE, &code, "\"hi\""),
            Some(SyntaxCategory::String)
        );
        assert_eq!(
            category_of(RUST_SAMPLE, &code, "42"),
            Some(SyntaxCategory::Number)
        );
        assert_eq!(
            category_of(RUST_SAMPLE, &code, "="),
            Some(SyntaxCategory::Operator)
        );
        assert_eq!(
            category_of(RUST_SAMPLE, &code, "// note"),
            Some(SyntaxCategory::Comment)
        );
        Ok(())
    }

    #[test]
    fn json_sample_yields_strings_numbers_and_keywords() -> Result<(), String> {
        let text = "{\n  \"key\": \"value\",\n  \"n\": 42,\n  \"b\": true\n}\n";
        let code = highlighted(highlight_code("json", text))?;
        assert_eq!(code.language, "JSON");
        assert_eq!(
            category_of(text, &code, "\"value\""),
            Some(SyntaxCategory::String)
        );
        assert_eq!(
            category_of(text, &code, "\"n\""),
            Some(SyntaxCategory::String)
        );
        assert_eq!(category_of(text, &code, "42"), Some(SyntaxCategory::Number));
        assert_eq!(
            category_of(text, &code, "true"),
            Some(SyntaxCategory::Keyword)
        );
        Ok(())
    }

    #[test]
    fn adjacent_same_category_spans_are_merged() -> Result<(), String> {
        let text = "{\"key\": 1}\n";
        let code = highlighted(highlight_code("json", text))?;
        let key_spans: Vec<_> = code
            .spans
            .iter()
            .filter(|span| span.range.start <= 1 && span.range.end >= 6)
            .collect();
        assert_eq!(key_spans.len(), 1);
        assert_eq!(&text[key_spans[0].range.clone()], "\"key\"");
        assert_eq!(key_spans[0].category, SyntaxCategory::String);
        Ok(())
    }

    #[test]
    fn python_sample_yields_keyword_function_variable_and_string() -> Result<(), String> {
        let text = "def f(x):\n    return \"s\"\n";
        let code = highlighted(highlight_code("python", text))?;
        assert_eq!(
            category_of(text, &code, "def"),
            Some(SyntaxCategory::Keyword)
        );
        assert_eq!(
            category_of(text, &code, "f"),
            Some(SyntaxCategory::Function)
        );
        assert_eq!(
            category_of(text, &code, "x"),
            Some(SyntaxCategory::Variable)
        );
        assert_eq!(
            category_of(text, &code, "return"),
            Some(SyntaxCategory::Keyword)
        );
        assert_eq!(
            category_of(text, &code, "\"s\""),
            Some(SyntaxCategory::String)
        );
        Ok(())
    }

    #[test]
    fn html_sample_yields_tag_attribute_and_string() -> Result<(), String> {
        let text = "<div class=\"x\">hi</div>\n";
        let code = highlighted(highlight_code("html", text))?;
        assert_eq!(category_of(text, &code, "div"), Some(SyntaxCategory::Tag));
        assert_eq!(
            category_of(text, &code, "class"),
            Some(SyntaxCategory::Attribute)
        );
        assert_eq!(
            category_of(text, &code, "\"x\""),
            Some(SyntaxCategory::String)
        );
        Ok(())
    }

    #[test]
    fn non_ascii_content_keeps_spans_on_char_boundaries() -> Result<(), String> {
        let text = "let s = \"中文\";\n";
        let code = highlighted(highlight_code("rust", text))?;
        assert!(!code.spans.is_empty());
        for span in &code.spans {
            assert!(text.is_char_boundary(span.range.start));
            assert!(text.is_char_boundary(span.range.end));
        }
        assert_eq!(
            category_of(text, &code, "\"中文\""),
            Some(SyntaxCategory::String)
        );
        Ok(())
    }

    #[test]
    fn crlf_terminators_never_enter_spans() -> Result<(), String> {
        let text = "// note\r\nlet x = 1;\r\n";
        let code = highlighted(highlight_code("rust", text))?;
        for span in &code.spans {
            let slice = &text[span.range.clone()];
            assert!(!slice.contains('\n'), "跨度包含换行: {slice:?}");
            assert!(!slice.contains('\r'), "跨度包含回车: {slice:?}");
        }
        assert_eq!(
            category_of(text, &code, "// note"),
            Some(SyntaxCategory::Comment)
        );
        Ok(())
    }

    #[test]
    fn empty_text_with_known_language_is_highlighted_without_spans() -> Result<(), String> {
        assert_eq!(
            highlight_code("rust", ""),
            HighlightOutcome::Highlighted(HighlightedCode {
                language: "Rust",
                spans: Vec::new(),
            })
        );
        Ok(())
    }

    #[test]
    fn unknown_or_empty_info_falls_back_to_plain_code() -> Result<(), String> {
        assert_eq!(
            highlight_code("", "fn main() {}\n"),
            HighlightOutcome::Unsupported
        );
        assert_eq!(
            highlight_code("brainfuck", "fn main() {}\n"),
            HighlightOutcome::Unsupported
        );
        assert_eq!(
            highlight_code("plain text", "fn main() {}\n"),
            HighlightOutcome::Unsupported
        );
        Ok(())
    }

    #[test]
    fn preload_is_idempotent_and_leaves_the_set_ready() -> Result<(), String> {
        preload_languages();
        preload_languages();
        assert_eq!(supported_languages().len(), 74);
        Ok(())
    }
}
