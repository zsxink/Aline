//! UI 侧插件门控与任务采纳：六开关映射、执行处拒绝、开→关→开、
//! 旧工作版本、关闭目标及主题/资源代次交错；拒绝不得改变文档状态。

use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;

use aline_app_core::{EffectivePlugins, SettingsField};
use aline_markdown::ParseFeatures;
use aline_plugin::{
    CapabilityDeclaration, CapabilityId, EnabledCapabilities, HostRequest, OutputIntent, Provider,
    ProviderDescriptor, ProviderDiagnostic, ProviderId, ProviderSource, RequestInput,
};
use aline_ui::{
    BUILT_IN_CAPABILITIES, BUILT_IN_SETTINGS, DocCore, PluginGate, PluginGateError,
    capability_for_setting, enabled_capabilities, setting_for_capability,
};

mod common;
use common::{capture, collapsed, limits, new_doc};

/// 测试提供者声明六个内置能力并原样回显文本；调用计数证明拒绝路径不派发。
struct GateProvider {
    descriptor: ProviderDescriptor,
    calls: Rc<Cell<usize>>,
}

impl GateProvider {
    fn new(capabilities: &[&'static str]) -> Self {
        let declarations = capabilities
            .iter()
            .map(|capability| {
                CapabilityDeclaration::new(
                    CapabilityId::from_static(capability),
                    ParseFeatures::EMPTY,
                )
            })
            .collect();
        Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::from_static("test.gate"),
                ProviderSource::Test,
                declarations,
            ),
            calls: Rc::new(Cell::new(0)),
        }
    }

    fn calls(&self) -> Rc<Cell<usize>> {
        self.calls.clone()
    }
}

impl Provider for GateProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn dispatch(&self, request: &HostRequest) -> Result<Vec<OutputIntent>, ProviderDiagnostic> {
        self.calls.set(self.calls.get() + 1);
        match &request.input {
            RequestInput::Text(text) => Ok(vec![OutputIntent::Text(text.clone())]),
            RequestInput::None => Ok(Vec::new()),
        }
    }
}

fn all_plugins() -> EffectivePlugins {
    EffectivePlugins {
        mermaid: true,
        table: true,
        export: true,
        math: true,
        highlight: true,
        image_preview: true,
    }
}

fn all_enabled() -> EnabledCapabilities {
    enabled_capabilities(all_plugins())
}

type GateFixture = (PluginGate, Rc<Cell<usize>>);

fn gate() -> Result<GateFixture, Box<dyn Error>> {
    let mut gate = PluginGate::new();
    let provider = GateProvider::new(&[
        "mermaid",
        "table",
        "export",
        "math",
        "highlight",
        "image_preview",
    ]);
    let calls = provider.calls();
    gate.register(Box::new(provider))?;
    Ok((gate, calls))
}

fn set_plugin(plugins: &mut EffectivePlugins, field: SettingsField, value: bool) -> bool {
    match field {
        SettingsField::MermaidEnabled => plugins.mermaid = value,
        SettingsField::TableEnabled => plugins.table = value,
        SettingsField::ExportEnabled => plugins.export = value,
        SettingsField::MathEnabled => plugins.math = value,
        SettingsField::HighlightEnabled => plugins.highlight = value,
        SettingsField::ImagePreviewEnabled => plugins.image_preview = value,
        _ => return false,
    }
    true
}

#[test]
fn six_plugin_settings_map_bijectively_to_six_capabilities() -> Result<(), Box<dyn Error>> {
    assert_eq!(BUILT_IN_CAPABILITIES.len(), 6);
    assert_eq!(BUILT_IN_SETTINGS.len(), 6);

    let mut mapped = Vec::new();
    for field in BUILT_IN_SETTINGS {
        let capability = capability_for_setting(field).ok_or("插件设置字段未映射能力")?;
        assert_eq!(setting_for_capability(capability), Some(field));
        assert!(
            !mapped.contains(&capability),
            "{field:?} 与既有字段映射到同一能力"
        );
        mapped.push(capability);
    }
    assert_eq!(mapped, BUILT_IN_CAPABILITIES.to_vec());

    for capability in BUILT_IN_CAPABILITIES {
        let field = setting_for_capability(capability).ok_or("内置能力未映射设置字段")?;
        assert_eq!(capability_for_setting(field), Some(capability));
    }

    assert_eq!(capability_for_setting(SettingsField::Autosave), None);
    assert_eq!(
        setting_for_capability(CapabilityId::from_static("test.custom")),
        None
    );
    Ok(())
}

#[test]
fn entry_gate_follows_each_setting_toggle() -> Result<(), Box<dyn Error>> {
    let (mut gate, _calls) = gate()?;

    for (index, field) in BUILT_IN_SETTINGS.into_iter().enumerate() {
        let capability = capability_for_setting(field).ok_or("插件设置字段未映射能力")?;
        let mut plugins = all_plugins();
        assert!(set_plugin(&mut plugins, field, false));
        gate.publish_enabled(enabled_capabilities(plugins));

        match gate.check_capability(capability) {
            Err(PluginGateError::Disabled {
                capability: rejected,
            }) => assert_eq!(rejected, capability),
            other => return Err(format!("期望 Disabled，实际 {other:?}").into()),
        }
        for (other_index, other) in BUILT_IN_CAPABILITIES.into_iter().enumerate() {
            if other_index != index {
                assert!(
                    gate.check_capability(other).is_ok(),
                    "关闭 {field:?} 不应影响 {other:?}"
                );
            }
        }
    }

    match gate.check_capability(CapabilityId::from_static("test.absent")) {
        Err(PluginGateError::UnknownCapability { capability }) => {
            assert_eq!(capability.as_str(), "test.absent");
        }
        other => return Err(format!("期望 UnknownCapability，实际 {other:?}").into()),
    }
    Ok(())
}

#[test]
fn off_on_cycle_rejects_old_task_without_document_change() -> Result<(), Box<dyn Error>> {
    let (mut gate, calls) = gate()?;
    let mut doc = new_doc("x\n", 1)?;
    gate.publish_enabled(all_enabled());
    let task = gate.begin_task(CapabilityId::MERMAID, &doc)?;

    gate.publish_enabled(EnabledCapabilities::none());
    gate.publish_enabled(all_enabled());

    let before = capture(&mut doc)?;
    match gate.adopt(&task, &doc, RequestInput::Text(String::from("旧"))) {
        Err(PluginGateError::StalePluginGeneration { expected, current }) => {
            assert_eq!(expected, task.plugin_generation());
            assert!(current > expected, "off→on 必须推进代次");
        }
        other => return Err(format!("期望 StalePluginGeneration，实际 {other:?}").into()),
    }
    assert_eq!(before, capture(&mut doc)?);
    assert_eq!(calls.get(), 0, "失效任务不得调用提供者");
    Ok(())
}

#[test]
fn old_working_version_is_rejected_without_document_change() -> Result<(), Box<dyn Error>> {
    let (mut gate, calls) = gate()?;
    let mut doc = new_doc("x\n", 1)?;
    gate.publish_enabled(all_enabled());
    let task = gate.begin_task(CapabilityId::MATH, &doc)?;

    doc.composition_update("中", 1)?;

    let before = capture(&mut doc)?;
    match gate.adopt(&task, &doc, RequestInput::Text(String::from("旧"))) {
        Err(PluginGateError::StaleWorkingVersion) => {}
        other => return Err(format!("期望 StaleWorkingVersion，实际 {other:?}").into()),
    }
    assert_eq!(before, capture(&mut doc)?);
    assert_eq!(calls.get(), 0, "旧工作版本不得调用提供者");
    Ok(())
}

#[test]
fn closed_target_is_rejected_without_document_change() -> Result<(), Box<dyn Error>> {
    let (mut gate, calls) = gate()?;
    let mut doc = DocCore::new_readonly("x\n", collapsed(1), limits())?;
    gate.publish_enabled(all_enabled());
    let task = gate.begin_task(CapabilityId::IMAGE_PREVIEW, &doc)?;

    doc.close_readonly()?;

    let before = capture(&mut doc)?;
    match gate.adopt(&task, &doc, RequestInput::Text(String::from("旧"))) {
        Err(PluginGateError::ClosedTarget) => {}
        other => return Err(format!("期望 ClosedTarget，实际 {other:?}").into()),
    }
    assert_eq!(before, capture(&mut doc)?);
    assert_eq!(calls.get(), 0, "关闭目标不得调用提供者");
    Ok(())
}

#[test]
fn foreign_document_is_rejected_without_target_change() -> Result<(), Box<dyn Error>> {
    let (mut gate, calls) = gate()?;
    let doc = new_doc("x\n", 1)?;
    let mut other = new_doc("y\n", 1)?;
    gate.publish_enabled(all_enabled());
    let task = gate.begin_task(CapabilityId::TABLE, &doc)?;

    let before = capture(&mut other)?;
    match gate.adopt(&task, &other, RequestInput::Text(String::from("旧"))) {
        Err(PluginGateError::WrongDocument) => {}
        other_error => return Err(format!("期望 WrongDocument，实际 {other_error:?}").into()),
    }
    assert_eq!(before, capture(&mut other)?);
    assert_eq!(calls.get(), 0, "外来文档不得调用提供者");
    Ok(())
}

#[test]
fn theme_and_resource_generation_changes_reject_task_without_document_change()
-> Result<(), Box<dyn Error>> {
    let (mut gate, calls) = gate()?;
    let mut doc = new_doc("x\n", 1)?;
    gate.publish_enabled(all_enabled());

    let theme_task = gate.begin_task(CapabilityId::HIGHLIGHT, &doc)?;
    gate.advance_theme_generation();
    let before = capture(&mut doc)?;
    match gate.adopt(&theme_task, &doc, RequestInput::Text(String::from("旧"))) {
        Err(PluginGateError::StaleTheme { expected, current }) => {
            assert_eq!(expected, theme_task.theme_generation());
            assert!(current > expected);
        }
        other => return Err(format!("期望 StaleTheme，实际 {other:?}").into()),
    }
    assert_eq!(before, capture(&mut doc)?);
    assert_eq!(calls.get(), 0, "主题变化后的旧任务不得调用提供者");

    let resource_task = gate.begin_task(CapabilityId::IMAGE_PREVIEW, &doc)?;
    gate.advance_resource_generation();
    let before = capture(&mut doc)?;
    match gate.adopt(&resource_task, &doc, RequestInput::Text(String::from("旧"))) {
        Err(PluginGateError::StaleResource { expected, current }) => {
            assert_eq!(expected, resource_task.resource_generation());
            assert!(current > expected);
        }
        other => return Err(format!("期望 StaleResource，实际 {other:?}").into()),
    }
    assert_eq!(before, capture(&mut doc)?);
    assert_eq!(calls.get(), 0, "资源变化后的旧任务不得调用提供者");
    Ok(())
}

#[test]
fn unrelated_capability_change_invalidates_in_flight_task() -> Result<(), Box<dyn Error>> {
    let (mut gate, calls) = gate()?;
    let doc = new_doc("x\n", 1)?;
    gate.publish_enabled(all_enabled());

    let current = gate.begin_task(CapabilityId::MERMAID, &doc)?;
    let outcome = gate.adopt(&current, &doc, RequestInput::Text(String::from("图")))?;
    assert_eq!(outcome.request_id, current.request_id());
    assert_eq!(outcome.generation, gate.plugin_generation());
    assert_eq!(
        outcome.intents,
        vec![OutputIntent::Text(String::from("图"))]
    );
    assert_eq!(calls.get(), 1);

    let task = gate.begin_task(CapabilityId::MERMAID, &doc)?;
    let mut plugins = all_plugins();
    assert!(set_plugin(&mut plugins, SettingsField::TableEnabled, false));
    gate.publish_enabled(enabled_capabilities(plugins));

    match gate.adopt(&task, &doc, RequestInput::Text(String::from("图"))) {
        Err(PluginGateError::StalePluginGeneration { .. }) => {}
        other => return Err(format!("无关能力变化也应推进全局代次，实际 {other:?}").into()),
    }
    assert_eq!(calls.get(), 1, "失效任务不得再次调用提供者");
    Ok(())
}
