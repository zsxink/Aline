//! 能力身份：开放标识而非封闭枚举，公共路由不写死六个内置能力。

/// 能力身份。
///
/// 注册表按该标识查询与派发；六个内置常量只是与设置键对齐的产品锚点，
/// 注册、启停和路由都不匹配这些常量，测试提供者可使用任意合法标识。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CapabilityId(&'static str);

impl CapabilityId {
    /// 内置能力：Mermaid 图表。
    pub const MERMAID: Self = Self("mermaid");
    /// 内置能力：表格增强。
    pub const TABLE: Self = Self("table");
    /// 内置能力：正文导出。
    pub const EXPORT: Self = Self("export");
    /// 内置能力：公式。
    pub const MATH: Self = Self("math");
    /// 内置能力：代码语言高亮。
    pub const HIGHLIGHT: Self = Self("highlight");
    /// 内置能力：图片预览。
    pub const IMAGE_PREVIEW: Self = Self("image_preview");

    /// 由静态标识构造。
    ///
    /// 标识须非空、不超过 64 字节，且只含 ASCII 字母、数字与 `-`、`_`、`.`；
    /// 合法性在注册时统一校验。
    pub const fn from_static(value: &'static str) -> Self {
        Self(value)
    }

    /// 返回标识文本。
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    pub(crate) fn is_valid(self) -> bool {
        super::valid_identifier(self.0)
    }
}
