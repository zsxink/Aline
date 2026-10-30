//! 从不可变源码派生语义与映射，不持有显示或插件状态。

mod mapping;

pub use mapping::{
    DrawnOffset, DrawnRange, Mapped, MappingBudget, MappingError, MappingPrecision, Projection,
    ProjectionId, Utf16Offset, Utf16Range,
};
