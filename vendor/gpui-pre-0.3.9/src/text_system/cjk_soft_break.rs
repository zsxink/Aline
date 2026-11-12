//! 中文自动软折行的合法断点纯规则（候选来源：锁定 gpui-pre 0.3.9）。
//!
//! 职责：只判断两个相邻字符之间能否作为自动软折行边界；不持有状态、不做 I/O、
//! 不决定行内像素几何，也不改写原文。三个消费点（shaped-line wrapping、
//! `LineWrapper::wrap_line`、`LineWrapper::truncate_wrapped_line`）共用同一集合。
//!
//! 边界：本规则不替代真实字形簇信息。调用方必须以 shaping/字素结果给出
//! `next_starts_cluster`；按字符迭代的 `LineWrapper` 只能给出 UTF-8 字符边界，
//! 不能声称与 shaped-line 路径产生完全相同的字节断点。
#![forbid(unsafe_code)]

/// 不得出现在自动软折行行首的闭标点（W2 已复现样本与决定 §28）。
pub(crate) const FORBIDDEN_LINE_START: [char; 12] = [
    '，', '。', '、', '！', '？', '；', '：', '）', '】', '》', '」', '』',
];

/// 不得出现在自动软折行行尾的开标点（W2 已复现样本与决定 §28）。
pub(crate) const FORBIDDEN_LINE_END: [char; 5] = ['（', '【', '《', '「', '『'];

/// 该字符是否禁止成为自动软折行后的行首。
pub(crate) fn forbids_line_start(character: char) -> bool {
    FORBIDDEN_LINE_START.contains(&character)
}

/// 该字符是否禁止成为自动软折行前的行尾。
pub(crate) fn forbids_line_end(character: char) -> bool {
    FORBIDDEN_LINE_END.contains(&character)
}

/// 判断相邻字符之间能否自动软折行。
///
/// `previous` 是断点前一个字符，`next` 是断点后第一个字符；
/// `next_starts_cluster` 必须来自实际 shaping/字素结果，false 表示断点落在
/// 同一字形簇内部，直接拒绝。
pub(crate) fn is_legal_soft_break_between(
    previous: char,
    next: char,
    next_starts_cluster: bool,
) -> bool {
    if !next_starts_cluster {
        return false;
    }
    if forbids_line_start(next) {
        return false;
    }
    if forbids_line_end(previous) {
        return false;
    }
    true
}

/// 带 UTF-8 端点校验的入口；供可直接访问原文的调用方使用。
///
/// 返回 false 的情形：偏移越界、不是 UTF-8 字符边界、断点在文本首尾、
/// 位于同一字形簇内，或违反行首/行尾禁则。无 panic 路径。
pub(crate) fn is_legal_soft_break_at(text: &str, offset: usize, next_starts_cluster: bool) -> bool {
    if offset == 0 || offset >= text.len() {
        return false;
    }
    if !text.is_char_boundary(offset) {
        return false;
    }
    let Some(before) = text.get(..offset) else {
        return false;
    };
    let Some(after) = text.get(offset..) else {
        return false;
    };
    let Some(previous) = before.chars().next_back() else {
        return false;
    };
    let Some(next) = after.chars().next() else {
        return false;
    };
    is_legal_soft_break_between(previous, next, next_starts_cluster)
}
