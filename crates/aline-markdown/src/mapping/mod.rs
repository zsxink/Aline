//! 投影单位与转换的聚合出口，具体状态由 projection 叶模块持有。

mod coordinates;
mod grapheme;
mod projection;

pub use coordinates::{DrawnOffset, DrawnRange, ProjectionId, Utf16Offset, Utf16Range};
pub use grapheme::GraphemeError;
pub use projection::{
    Mapped, MappingBudget, MappingError, MappingPrecision, MappingSegment, Projection,
};
