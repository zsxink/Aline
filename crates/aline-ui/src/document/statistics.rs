//! 文档统一统计的纯计算；不看窗口、不做 I/O、不持有可变状态。
//!
//! 口径是**候选口径**（UI §5.4）：字符数取源码中非空白 Unicode 标量值的个数，
//! 行数按 LF 分行，与 `aline_buffer::LineIndex::line_count()` 同口径——孤立 CR
//! 不是换行，CRLF 不额外计行，末尾 LF 产生一个空行。
//!
//! 与原型的口径关系：原型当前有两套统计脚本——`prototype.js` 对渲染后的正文
//! 计数，`interactions.js` 的 `stats()` 对 `doc.source` 计数。本模块采用与后者
//! 同方向的**源文本**口径，使两种只读视图得到同一数值；它不是逐字符等价：
//! JavaScript `\s` 与 Rust `char::is_whitespace` 在 U+FEFF、U+0085 等字符上边界
//! 不同，相关样例归 V-07 核对，此处不写成已冻结口径。

/// 一份源码的字符数与行数。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DocumentStatistics {
    characters: usize,
    lines: usize,
}

impl DocumentStatistics {
    /// 返回非空白字符数。
    pub(crate) const fn characters(&self) -> usize {
        self.characters
    }

    /// 返回按 LF 分行的行数。
    pub(crate) const fn lines(&self) -> usize {
        self.lines
    }

    /// 按候选口径统计一份源码的字符数与行数。
    ///
    /// 空文本得到 0 字符与 1 行，与行索引的行数口径一致。
    pub(crate) fn compute(source: &str) -> Self {
        let characters = source
            .chars()
            .filter(|value| !value.is_whitespace())
            .count();
        let lines = source
            .bytes()
            .filter(|byte| *byte == b'\n')
            .count()
            .saturating_add(1);
        Self { characters, lines }
    }
}
