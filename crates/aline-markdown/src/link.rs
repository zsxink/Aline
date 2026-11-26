//! 链接目标协议判定。
//!
//! 判定只回答“能否交给宿主打开”，不执行打开动作，也不改写文档。允许的
//! 协议与 V-03 冻结的 allowlist 一致：`http`、`https`、`mailto`，以及以
//! `#` 或 `/` 开头的站内/绝对路径。其余带 scheme 的目标按协议拒绝，裸相对
//! 路径按不支持目标拒绝；拒绝文案键由调用方通过 [`LinkVerdict::message_key`]
//! 取用，不在本模块生成用户字符串。

/// 允许的链接协议。
const ALLOWED_LINK_SCHEMES: [&str; 3] = ["http", "https", "mailto"];
/// 允许的站内/绝对路径前缀。
const ALLOWED_LINK_PREFIXES: [char; 2] = ['#', '/'];
/// 明确拒绝的协议；其余带 scheme 的目标同样按协议拒绝。
const REJECTED_LINK_SCHEMES: [&str; 8] = [
    "javascript",
    "data",
    "vbscript",
    "file",
    "ftp",
    "tel",
    "ssh",
    "smb",
];

/// 链接目标的判定结果。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LinkVerdict {
    /// 允许交给宿主打开。
    Allowed,
    /// 空目标或纯空白。
    Empty,
    /// 带 scheme 但不在 allowlist 内。
    UnsupportedScheme,
    /// 无 scheme 的裸相对路径或其他不支持目标。
    UnsupportedTarget,
}

impl LinkVerdict {
    /// 是否允许打开。
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// 冻结的界面反馈文案键；允许时返回 `link.allowed`。
    pub const fn message_key(self) -> &'static str {
        match self {
            Self::Allowed => "link.allowed",
            Self::Empty => "link.rejected.empty",
            Self::UnsupportedScheme => "link.rejected.scheme",
            Self::UnsupportedTarget => "link.rejected.unsupported",
        }
    }
}

/// 按 V-03 冻结规则判定链接目标。
pub fn link_verdict(destination: &str) -> LinkVerdict {
    let trimmed = destination.trim();
    if trimmed.is_empty() {
        return LinkVerdict::Empty;
    }
    if ALLOWED_LINK_PREFIXES
        .iter()
        .any(|prefix| trimmed.starts_with(*prefix))
    {
        return LinkVerdict::Allowed;
    }
    let lower = trimmed.to_ascii_lowercase();
    if ALLOWED_LINK_SCHEMES
        .iter()
        .any(|scheme| has_scheme(&lower, scheme))
    {
        return LinkVerdict::Allowed;
    }
    if REJECTED_LINK_SCHEMES
        .iter()
        .any(|scheme| has_scheme(&lower, scheme))
    {
        return LinkVerdict::UnsupportedScheme;
    }
    if lower
        .find(':')
        .is_some_and(|colon| !lower[..colon].contains('/'))
    {
        return LinkVerdict::UnsupportedScheme;
    }
    LinkVerdict::UnsupportedTarget
}

/// 是否允许打开该链接目标；等价于 [`link_verdict`] 的 [`LinkVerdict::Allowed`]。
pub fn is_allowed_link(destination: &str) -> bool {
    link_verdict(destination).is_allowed()
}

fn has_scheme(lower: &str, scheme: &str) -> bool {
    lower
        .strip_prefix(scheme)
        .is_some_and(|rest| rest.starts_with(':'))
}
