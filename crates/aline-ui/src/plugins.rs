//! UI 侧插件门控与任务采纳；平台无关，不调度具体插件计算。
//!
//! 设置服务在 5.4 装配前，启用集合、主题代次与资源代次由测试或宿主直接
//! 发布；本模块只保存类型化快照并做采纳校验，不读取设置文件，也不实现
//! 具体插件视图。门控顺序固定为：文档身份/存活/工作版本、插件代次、
//! 能力存在与启用、主题代次、资源代次，最后才调用提供者。

use std::fmt;

use aline_app_core::{EffectivePlugins, SettingsField};
use aline_plugin::{
    CapabilityId, DispatchError, DispatchOutcome, EnabledCapabilities, HostRequest, Lifecycle,
    Provider, RegistrationError, Registry, RequestId, RequestInput, dispatch,
};

use crate::document::{DocCore, DocumentId, WorkingVersion};

/// 六个内置能力，顺序与 [`BUILT_IN_SETTINGS`] 一致。
pub const BUILT_IN_CAPABILITIES: [CapabilityId; 6] = [
    CapabilityId::MERMAID,
    CapabilityId::TABLE,
    CapabilityId::EXPORT,
    CapabilityId::MATH,
    CapabilityId::HIGHLIGHT,
    CapabilityId::IMAGE_PREVIEW,
];

/// 六个插件开关字段，顺序与 [`BUILT_IN_CAPABILITIES`] 一致。
pub const BUILT_IN_SETTINGS: [SettingsField; 6] = [
    SettingsField::MermaidEnabled,
    SettingsField::TableEnabled,
    SettingsField::ExportEnabled,
    SettingsField::MathEnabled,
    SettingsField::HighlightEnabled,
    SettingsField::ImagePreviewEnabled,
];

/// 返回插件开关对应的能力；非插件设置字段返回 `None`。
pub const fn capability_for_setting(field: SettingsField) -> Option<CapabilityId> {
    match field {
        SettingsField::MermaidEnabled => Some(CapabilityId::MERMAID),
        SettingsField::TableEnabled => Some(CapabilityId::TABLE),
        SettingsField::ExportEnabled => Some(CapabilityId::EXPORT),
        SettingsField::MathEnabled => Some(CapabilityId::MATH),
        SettingsField::HighlightEnabled => Some(CapabilityId::HIGHLIGHT),
        SettingsField::ImagePreviewEnabled => Some(CapabilityId::IMAGE_PREVIEW),
        _ => None,
    }
}

/// 返回内置能力对应的插件开关字段；非内置能力返回 `None`。
pub fn setting_for_capability(capability: CapabilityId) -> Option<SettingsField> {
    match capability.as_str() {
        "mermaid" => Some(SettingsField::MermaidEnabled),
        "table" => Some(SettingsField::TableEnabled),
        "export" => Some(SettingsField::ExportEnabled),
        "math" => Some(SettingsField::MathEnabled),
        "highlight" => Some(SettingsField::HighlightEnabled),
        "image_preview" => Some(SettingsField::ImagePreviewEnabled),
        _ => None,
    }
}

/// 把有效设置中的六个开关转换为启用集合；未启用项不进入集合。
pub fn enabled_capabilities(plugins: EffectivePlugins) -> EnabledCapabilities {
    let mut ids = Vec::new();
    if plugins.mermaid {
        ids.push(CapabilityId::MERMAID);
    }
    if plugins.table {
        ids.push(CapabilityId::TABLE);
    }
    if plugins.export {
        ids.push(CapabilityId::EXPORT);
    }
    if plugins.math {
        ids.push(CapabilityId::MATH);
    }
    if plugins.highlight {
        ids.push(CapabilityId::HIGHLIGHT);
    }
    if plugins.image_preview {
        ids.push(CapabilityId::IMAGE_PREVIEW);
    }
    EnabledCapabilities::from_ids(ids)
}

/// 一次插件任务发起时冻结的采纳资格。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginTaskKey {
    request: RequestId,
    capability: CapabilityId,
    plugin_generation: u64,
    document: DocumentId,
    version: WorkingVersion,
    theme_generation: u64,
    resource_generation: u64,
}

impl PluginTaskKey {
    /// 任务请求身份；采纳结果必须对应同一身份。
    pub const fn request_id(&self) -> RequestId {
        self.request
    }

    /// 任务目标能力。
    pub const fn capability(&self) -> CapabilityId {
        self.capability
    }

    /// 发起时的插件代次。
    pub const fn plugin_generation(&self) -> u64 {
        self.plugin_generation
    }

    /// 发起时的主题来源代次。
    pub const fn theme_generation(&self) -> u64 {
        self.theme_generation
    }

    /// 发起时的资源来源代次。
    pub const fn resource_generation(&self) -> u64 {
        self.resource_generation
    }
}

/// 门控或采纳失败。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PluginGateError {
    /// 文档目标已关闭。
    ClosedTarget,
    /// 结果属于另一个文档身份。
    WrongDocument,
    /// 工作版本已推进（提交或草稿代次变化）。
    StaleWorkingVersion,
    /// 插件启用集合已变化；off→on 后旧任务也不复活。
    StalePluginGeneration {
        /// 任务发起时的代次。
        expected: u64,
        /// 当前代次。
        current: u64,
    },
    /// 没有提供者声明该能力。
    UnknownCapability {
        /// 目标能力。
        capability: CapabilityId,
    },
    /// 能力已注册但当前未启用。
    Disabled {
        /// 目标能力。
        capability: CapabilityId,
    },
    /// 主题来源已变化。
    StaleTheme {
        /// 任务发起时的代次。
        expected: u64,
        /// 当前代次。
        current: u64,
    },
    /// 资源来源已变化。
    StaleResource {
        /// 任务发起时的代次。
        expected: u64,
        /// 当前代次。
        current: u64,
    },
    /// 提供者派发失败；门控已通过。
    Dispatch(DispatchError),
}

impl fmt::Display for PluginGateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for PluginGateError {}

/// UI 侧插件门控：持有注册表、启用代次与主题/资源来源代次。
///
/// 提供者计算仍由 `aline-plugin` 的注册/派发契约承担；本类型只决定任务
/// 是否可发起、结果是否仍可采纳，并在采纳处再次执行能力校验。
pub struct PluginGate {
    registry: Registry,
    lifecycle: Lifecycle,
    theme_generation: u64,
    resource_generation: u64,
    next_request: u64,
}

impl PluginGate {
    /// 空注册表、插件代次 1、能力全关、主题/资源代次 1。
    pub fn new() -> Self {
        Self {
            registry: Registry::new(),
            lifecycle: Lifecycle::new(),
            theme_generation: 1,
            resource_generation: 1,
            next_request: 1,
        }
    }

    /// 登记提供者；校验与拒绝语义由 [`Registry::register`] 决定。
    pub fn register(&mut self, provider: Box<dyn Provider>) -> Result<(), RegistrationError> {
        self.registry.register(provider)
    }

    /// 发布有效启用集合；集合变化才推进插件代次。
    pub fn publish_enabled(&mut self, enabled: EnabledCapabilities) -> u64 {
        self.lifecycle.apply(enabled)
    }

    /// 当前插件代次。
    pub fn plugin_generation(&self) -> u64 {
        self.lifecycle.generation()
    }

    /// 推进主题来源代次；真实主题服务在 5.x 接入后调用。
    pub fn advance_theme_generation(&mut self) -> u64 {
        self.theme_generation = self.theme_generation.saturating_add(1);
        self.theme_generation
    }

    /// 推进资源来源代次；真实资源服务在 3.x 接入后调用。
    pub fn advance_resource_generation(&mut self) -> u64 {
        self.resource_generation = self.resource_generation.saturating_add(1);
        self.resource_generation
    }

    /// 执行处校验能力是否可用；菜单即使仍持有旧动作也须经过本检查。
    pub fn check_capability(&self, capability: CapabilityId) -> Result<(), PluginGateError> {
        if self.registry.capability(capability).is_none() {
            return Err(PluginGateError::UnknownCapability { capability });
        }
        if !self.lifecycle.enabled().contains(capability) {
            return Err(PluginGateError::Disabled { capability });
        }
        Ok(())
    }

    /// 为已启用能力发放任务身份并冻结当前文档、插件、主题与资源代次。
    pub fn begin_task(
        &mut self,
        capability: CapabilityId,
        document: &DocCore,
    ) -> Result<PluginTaskKey, PluginGateError> {
        if !document.is_document_alive() {
            return Err(PluginGateError::ClosedTarget);
        }
        self.check_capability(capability)?;
        let request = RequestId::new(self.next_request);
        self.next_request = self.next_request.saturating_add(1);
        Ok(PluginTaskKey {
            request,
            capability,
            plugin_generation: self.lifecycle.generation(),
            document: document.document_id().clone(),
            version: document.version(),
            theme_generation: self.theme_generation,
            resource_generation: self.resource_generation,
        })
    }

    /// 采纳已完成结果：按固定顺序复验身份与代次，全部通过才派发提供者。
    ///
    /// 拒绝是纯校验，不修改文档、选区、历史或既有目标。
    pub fn adopt(
        &self,
        task: &PluginTaskKey,
        document: &DocCore,
        input: RequestInput,
    ) -> Result<DispatchOutcome, PluginGateError> {
        if document.document_id() != &task.document {
            return Err(PluginGateError::WrongDocument);
        }
        if !document.is_document_alive() {
            return Err(PluginGateError::ClosedTarget);
        }
        if document.version() != task.version {
            return Err(PluginGateError::StaleWorkingVersion);
        }
        let current_generation = self.lifecycle.generation();
        if current_generation != task.plugin_generation {
            return Err(PluginGateError::StalePluginGeneration {
                expected: task.plugin_generation,
                current: current_generation,
            });
        }
        self.check_capability(task.capability)?;
        if self.theme_generation != task.theme_generation {
            return Err(PluginGateError::StaleTheme {
                expected: task.theme_generation,
                current: self.theme_generation,
            });
        }
        if self.resource_generation != task.resource_generation {
            return Err(PluginGateError::StaleResource {
                expected: task.resource_generation,
                current: self.resource_generation,
            });
        }
        let request =
            HostRequest::new(task.request, task.capability, task.plugin_generation, input);
        dispatch(&self.registry, &self.lifecycle, &request).map_err(PluginGateError::Dispatch)
    }
}

impl Default for PluginGate {
    fn default() -> Self {
        Self::new()
    }
}
