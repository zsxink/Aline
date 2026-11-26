//! 提供者身份、来源与能力声明。

use aline_markdown::ParseFeatures;

use super::CapabilityId;

/// 提供者稳定身份。
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ProviderId(&'static str);

impl ProviderId {
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

/// 提供者来源。
///
/// V1 只随应用编译内置提供者；`Test` 供公共契约的自动化用例使用，
/// 不代表已经具备外部加载或隔离能力。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProviderSource {
    /// 随应用编译的内置提供者。
    BuiltIn,
    /// 仅用于验证公共契约的测试提供者。
    Test,
}

/// 一项能力声明。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CapabilityDeclaration {
    /// 能力身份；整个注册表内至多一个提供者声明同一能力。
    pub capability: CapabilityId,
    /// 启用该能力时要求解析器附加的语法位；不改变解析语义时用
    /// [`ParseFeatures::EMPTY`]。
    pub parse_features: ParseFeatures,
}

impl CapabilityDeclaration {
    /// 构造一项能力声明。
    pub const fn new(capability: CapabilityId, parse_features: ParseFeatures) -> Self {
        Self {
            capability,
            parse_features,
        }
    }
}

/// 提供者声明：注册时读取一次并校验。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderDescriptor {
    /// 稳定身份。
    pub id: ProviderId,
    /// 来源。
    pub source: ProviderSource,
    /// 声明的能力；不得为空，且同一提供者内不得重复。
    pub capabilities: Vec<CapabilityDeclaration>,
}

impl ProviderDescriptor {
    /// 构造提供者声明。
    pub fn new(
        id: ProviderId,
        source: ProviderSource,
        capabilities: Vec<CapabilityDeclaration>,
    ) -> Self {
        Self {
            id,
            source,
            capabilities,
        }
    }
}
