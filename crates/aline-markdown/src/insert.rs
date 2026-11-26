//! 弹窗插入的 Markdown 构件：转义、空文字回退与目标校验。
//!
//! 构件只生成源码文本，不判断文档状态；链接插入只拒绝空目标与未许可
//! scheme，相对路径和裸路径照常写入。链接**打开**的协议判定见
//! [`link_verdict`](crate::link_verdict)，两者语义不同。

use crate::{LinkVerdict, link_verdict};

/// 插入构件的种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InlineMarkupKind {
    /// 链接 `[文字](目标)`。
    Link,
    /// 图片 `![替代文字](目标)`。
    Image,
}

/// 插入目标被拒绝的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InsertRejection {
    /// 目标为空或纯空白。
    EmptyDestination,
    /// 链接目标带未许可 scheme。
    UnsupportedScheme,
    /// 目标包含换行或 NUL。
    LineBreak,
}

impl InsertRejection {
    /// 冻结的界面反馈文案键。
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::EmptyDestination => "insert.rejected.empty",
            Self::UnsupportedScheme => "insert.rejected.scheme",
            Self::LineBreak => "insert.rejected.line_break",
        }
    }
}

/// 生成插入用的 Markdown 源码。
///
/// 目标先做首尾裁剪；空文字回退为目标本身。文字中的 `\`、`[`、`]` 转义，
/// 换行折叠为单个空格。目标含空格、括号、尖括号或反斜杠时改用尖括号形式，
/// 并转义其中的 `\`、`<`、`>`。图片目标不做协议判定，由正文图片入口和
/// 右键能力矩阵表达可用性。
pub fn inline_markup(
    kind: InlineMarkupKind,
    text: &str,
    destination: &str,
) -> Result<String, InsertRejection> {
    let destination = destination.trim();
    if destination.is_empty() {
        return Err(InsertRejection::EmptyDestination);
    }
    if destination.contains(['\r', '\n', '\0']) {
        return Err(InsertRejection::LineBreak);
    }
    if kind == InlineMarkupKind::Link && link_verdict(destination) == LinkVerdict::UnsupportedScheme
    {
        return Err(InsertRejection::UnsupportedScheme);
    }
    let label = flatten(text);
    let label = if label.is_empty() {
        destination.to_owned()
    } else {
        label
    };
    let mut markup = String::new();
    markup.push_str(if kind == InlineMarkupKind::Link {
        "["
    } else {
        "!["
    });
    escape_label(&label, &mut markup);
    markup.push_str("](");
    format_destination(destination, &mut markup);
    markup.push(')');
    Ok(markup)
}

/// 折叠换行并裁剪；空结果表示回退目标。
fn flatten(text: &str) -> String {
    let mut output = String::new();
    for character in text.trim().chars() {
        if matches!(character, '\r' | '\n') {
            if !output.ends_with(' ') {
                output.push(' ');
            }
        } else {
            output.push(character);
        }
    }
    output.trim().to_owned()
}

fn escape_label(label: &str, output: &mut String) {
    for character in label.chars() {
        if matches!(character, '\\' | '[' | ']') {
            output.push('\\');
        }
        output.push(character);
    }
}

fn format_destination(destination: &str, output: &mut String) {
    if !destination
        .chars()
        .any(|character| matches!(character, ' ' | '\t' | '(' | ')' | '<' | '>' | '\\'))
    {
        output.push_str(destination);
        return;
    }
    output.push('<');
    for character in destination.chars() {
        if matches!(character, '\\' | '<' | '>') {
            output.push('\\');
        }
        output.push(character);
    }
    output.push('>');
}
