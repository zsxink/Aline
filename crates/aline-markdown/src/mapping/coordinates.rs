//! 投影单位的受检值对象；构造与消费只在所属映射域进行。

use std::fmt;
use std::ops::Range;
use std::sync::Arc;

use super::projection::MappingError;

/// 当前展示投影实例身份；相同可见文字也不复用身份。
#[derive(Clone)]
pub struct ProjectionId(pub(super) Arc<()>);

impl PartialEq for ProjectionId {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ProjectionId {}
impl fmt::Debug for ProjectionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ProjectionId(..)")
    }
}

/// 绑定投影的 UTF-8 绘制字节偏移，只由受检投影构造。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrawnOffset {
    pub(super) projection: ProjectionId,
    pub(super) byte: usize,
}

impl DrawnOffset {
    /// 返回绘制字节值，不作为跨源码坐标转换的捷径。
    pub const fn byte_offset(&self) -> usize {
        self.byte
    }
    /// 返回所属投影身份。
    pub fn projection_id(&self) -> &ProjectionId {
        &self.projection
    }
}

/// 绑定投影的 UTF-16 code unit 偏移，不允许处于代理对中间。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf16Offset {
    pub(super) projection: ProjectionId,
    pub(super) unit: usize,
}

impl Utf16Offset {
    /// 返回原生适配边界所需的 UTF-16 单位值。
    pub const fn code_units(&self) -> usize {
        self.unit
    }
    /// 返回所属投影身份。
    pub fn projection_id(&self) -> &ProjectionId {
        &self.projection
    }
}

/// 绑定同一投影、两端 UTF-8 对齐的绘制半开范围。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DrawnRange {
    pub(super) projection: ProjectionId,
    bytes: Range<usize>,
}

impl DrawnRange {
    /// 从受检端点构造范围，错投影或反向范围拒绝。
    pub fn new(start: DrawnOffset, end: DrawnOffset) -> Result<Self, MappingError> {
        if start.projection != end.projection {
            return Err(MappingError::ProjectionMismatch);
        }
        if start.byte > end.byte {
            return Err(MappingError::ReversedRange);
        }
        Ok(Self {
            projection: start.projection,
            bytes: start.byte..end.byte,
        })
    }
    /// 返回绘制字节区间。
    pub fn byte_range(&self) -> Range<usize> {
        self.bytes.clone()
    }
    /// 返回所属投影身份。
    pub fn projection_id(&self) -> &ProjectionId {
        &self.projection
    }
}

/// 绑定同一投影、两端合法的 UTF-16 code unit 范围。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Utf16Range {
    pub(super) projection: ProjectionId,
    units: Range<usize>,
}

impl Utf16Range {
    /// 从受检端点构造范围，错投影或反向范围拒绝。
    pub fn new(start: Utf16Offset, end: Utf16Offset) -> Result<Self, MappingError> {
        if start.projection != end.projection {
            return Err(MappingError::ProjectionMismatch);
        }
        if start.unit > end.unit {
            return Err(MappingError::ReversedRange);
        }
        Ok(Self {
            projection: start.projection,
            units: start.unit..end.unit,
        })
    }
    /// 返回 UTF-16 单位区间。
    pub fn code_unit_range(&self) -> Range<usize> {
        self.units.clone()
    }
    /// 返回所属投影身份。
    pub fn projection_id(&self) -> &ProjectionId {
        &self.projection
    }
}
