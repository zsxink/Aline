//! 解析前处理：文首 front matter 识别与解析副本重写。

pub mod front;
pub mod rewrite;

pub use front::{FrontMatter, FrontMatterKind, detect as detect_front_matter};
pub use rewrite::{
    Rewrite, RewriteEdit, RewriteError, RewriteMap, RewriteSegment, RewriteSource,
    rewrite_front_matter, rewrite_front_matter_with_cancel,
};
