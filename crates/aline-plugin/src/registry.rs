//! 提供者登记与能力查询；V1 显式注入内置提供者。
//!
//! 注册是原子的：任一校验失败都不会留下半注册，也不会覆盖已有提供者
//! 或按注册顺序静默取胜。

use std::fmt;

use aline_markdown::ParseFeatures;

use crate::contracts::{CapabilityId, ProviderDescriptor, ProviderId};
use crate::lifecycle::EnabledCapabilities;
use crate::traits::Provider;

/// 注册失败类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistrationErrorKind {
    /// 提供者身份为空、过长或含非法字符。
    InvalidProviderId,
    /// 提供者没有声明任何能力。
    EmptyCapabilities,
    /// 能力身份为空、过长或含非法字符。
    InvalidCapabilityId,
    /// 同一提供者内重复声明同一能力。
    DuplicateCapabilityInDeclaration,
    /// 已有提供者使用同一身份。
    DuplicateProviderId,
    /// 该能力已由其他提供者声明。
    CapabilityConflict,
}

/// 注册失败；携带出错的提供者与（可选）能力。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegistrationError {
    /// 失败类别。
    pub kind: RegistrationErrorKind,
    /// 出错的提供者身份。
    pub provider: ProviderId,
    /// 相关能力；提供者级失败为 `None`。
    pub capability: Option<CapabilityId>,
}

impl fmt::Display for RegistrationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for RegistrationError {}

struct RegisteredProvider {
    descriptor: ProviderDescriptor,
    provider: Box<dyn Provider>,
}

/// 已注册能力的查询结果。
///
/// 对外只暴露声明。提供者实例仅由 [`dispatch`](crate::dispatch) 在通过
/// 代次与启用门控后访问，调用方不能绕过校验直接派发。
pub struct CapabilityQuery<'a> {
    descriptor: &'a ProviderDescriptor,
    provider: &'a dyn Provider,
}

impl<'a> CapabilityQuery<'a> {
    /// 声明该能力的提供者描述。
    pub fn descriptor(&self) -> &'a ProviderDescriptor {
        self.descriptor
    }

    /// 提供者实例；仅供 crate 内派发路径在门控通过后使用。
    pub(crate) fn provider(&self) -> &'a dyn Provider {
        self.provider
    }
}

/// 提供者注册表。
#[derive(Default)]
pub struct Registry {
    providers: Vec<RegisteredProvider>,
}

impl Registry {
    /// 空注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// 校验并登记一个提供者。
    ///
    /// 声明在注册时读取一次；失败不改变注册表。
    pub fn register(&mut self, provider: Box<dyn Provider>) -> Result<(), RegistrationError> {
        let descriptor = provider.descriptor().clone();
        self.validate(&descriptor)?;
        self.providers.push(RegisteredProvider {
            descriptor,
            provider,
        });
        Ok(())
    }

    /// 查询能力所属的提供者；未注册返回 `None`。
    pub fn capability(&self, capability: CapabilityId) -> Option<CapabilityQuery<'_>> {
        self.providers.iter().find_map(|registered| {
            let owns = registered
                .descriptor
                .capabilities
                .iter()
                .any(|declaration| declaration.capability == capability);
            owns.then_some(CapabilityQuery {
                descriptor: &registered.descriptor,
                provider: registered.provider.as_ref(),
            })
        })
    }

    /// 按注册顺序遍历提供者声明。
    pub fn descriptors(&self) -> impl Iterator<Item = &ProviderDescriptor> {
        self.providers
            .iter()
            .map(|registered| &registered.descriptor)
    }

    /// 已登记提供者数量。
    pub fn len(&self) -> usize {
        self.providers.len()
    }

    /// 是否没有提供者。
    pub fn is_empty(&self) -> bool {
        self.providers.is_empty()
    }

    /// 聚合启用能力要求的附加解析语法位；未启用的能力不贡献。
    ///
    /// 返回值只含插件附加位；宿主负责并入核心语法集合并形成
    /// `aline_markdown::ParseConfig` 与其代次。
    pub fn parse_features(&self, enabled: &EnabledCapabilities) -> ParseFeatures {
        self.providers
            .iter()
            .flat_map(|registered| registered.descriptor.capabilities.iter())
            .filter(|declaration| enabled.contains(declaration.capability))
            .fold(ParseFeatures::EMPTY, |features, declaration| {
                features.union(declaration.parse_features)
            })
    }

    fn validate(&self, descriptor: &ProviderDescriptor) -> Result<(), RegistrationError> {
        let error = |kind, capability| RegistrationError {
            kind,
            provider: descriptor.id,
            capability,
        };
        if !descriptor.id.is_valid() {
            return Err(error(RegistrationErrorKind::InvalidProviderId, None));
        }
        if descriptor.capabilities.is_empty() {
            return Err(error(RegistrationErrorKind::EmptyCapabilities, None));
        }
        for (index, declaration) in descriptor.capabilities.iter().enumerate() {
            if !declaration.capability.is_valid() {
                return Err(error(
                    RegistrationErrorKind::InvalidCapabilityId,
                    Some(declaration.capability),
                ));
            }
            let repeated = descriptor
                .capabilities
                .iter()
                .take(index)
                .any(|prior| prior.capability == declaration.capability);
            if repeated {
                return Err(error(
                    RegistrationErrorKind::DuplicateCapabilityInDeclaration,
                    Some(declaration.capability),
                ));
            }
        }
        if self
            .providers
            .iter()
            .any(|registered| registered.descriptor.id == descriptor.id)
        {
            return Err(error(RegistrationErrorKind::DuplicateProviderId, None));
        }
        for declaration in &descriptor.capabilities {
            if self.capability(declaration.capability).is_some() {
                return Err(error(
                    RegistrationErrorKind::CapabilityConflict,
                    Some(declaration.capability),
                ));
            }
        }
        Ok(())
    }
}
