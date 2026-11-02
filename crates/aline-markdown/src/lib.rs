//! 从不可变源码派生语义与映射，不持有显示或插件状态。

mod budget;
mod build;
mod config;
mod diagnostic;
mod mapping;
mod mdast;
mod navigation;
mod parse;
mod pre;
mod protection;
mod reuse;
mod style;
mod tree;

pub use budget::{CancelCheck, CancelToken, ParseBudget, ParseStats};
pub use config::{ParseConfig, ParseFeatures};
pub use diagnostic::{Diagnostic, DiagnosticKind};
pub use mapping::{
    DrawnOffset, DrawnRange, GraphemeError, Mapped, MappingBudget, MappingError, MappingPrecision,
    MappingSegment, Projection, ProjectionId, Utf16Offset, Utf16Range,
};
pub use mdast::{Block, BlockKind, Gap};
pub use navigation::{NavigationEntry, NavigationIndex, NavigationKind};
pub use parse::{ParseError, Parsed, parse, parse_with_cancel};
pub use pre::{
    FrontMatter, FrontMatterKind, Rewrite, RewriteEdit, RewriteError, RewriteMap, RewriteSegment,
    RewriteSource, detect_front_matter, rewrite_front_matter, rewrite_front_matter_with_cancel,
};
pub use protection::{ProtectedRange, ProtectionKind};
pub use reuse::{ReuseOutcome, ReuseRefusal, ReuseTranslateError, ReusedBlock, reuse};
pub use style::{StyledRange, TextStyle};
