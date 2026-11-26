//! 公共插件契约：测试提供者证明注册、查询、启停、派发与解析边界，
//! 并覆盖重复身份、能力冲突和无效声明的拒绝路径。

use std::cell::Cell;
use std::error::Error;
use std::rc::Rc;

use aline_markdown::ParseFeatures;
use aline_plugin::{
    CapabilityDeclaration, CapabilityId, DiagnosticKind, DispatchError, DispatchOutcome,
    EnabledCapabilities, HostRequest, Lifecycle, OutputIntent, Provider, ProviderDescriptor,
    ProviderDiagnostic, ProviderId, ProviderSource, RegistrationError, RegistrationErrorKind,
    Registry, RequestId, RequestInput, dispatch,
};

/// 标识符长度上限（64 字节）的边界值，注册应接受。
const LONGEST_ID: &str = "test.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";
/// 超过长度上限（65 字节），注册应拒绝。
const TOO_LONG_ID: &str = "test.xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

/// 提供者被调用次数的共享计数；测试保留副本以证明门控失败时未调用。
#[derive(Clone, Default)]
struct CallCounter(Rc<Cell<usize>>);

impl CallCounter {
    fn count(&self) -> usize {
        self.0.get()
    }

    fn record(&self) {
        self.0.set(self.0.get() + 1);
    }
}

struct TestProvider {
    descriptor: ProviderDescriptor,
    failure: Option<DiagnosticKind>,
    calls: CallCounter,
}

impl TestProvider {
    fn new(id: &'static str, capabilities: Vec<CapabilityDeclaration>) -> Self {
        Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::from_static(id),
                ProviderSource::Test,
                capabilities,
            ),
            failure: None,
            calls: CallCounter::default(),
        }
    }

    fn failing(mut self, kind: DiagnosticKind) -> Self {
        self.failure = Some(kind);
        self
    }

    fn calls(&self) -> CallCounter {
        self.calls.clone()
    }
}

impl Provider for TestProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn dispatch(&self, request: &HostRequest) -> Result<Vec<OutputIntent>, ProviderDiagnostic> {
        self.calls.record();
        if let Some(kind) = self.failure {
            return Err(ProviderDiagnostic::new(kind));
        }
        match &request.input {
            RequestInput::Text(text) => Ok(vec![OutputIntent::Text(text.clone())]),
            RequestInput::None => Ok(Vec::new()),
        }
    }
}

/// 返回二进制资产的提供者，用于验证 `OutputIntent::Asset` 原样传递。
struct AssetProvider {
    descriptor: ProviderDescriptor,
}

impl AssetProvider {
    fn new(id: &'static str, capability: &'static str) -> Self {
        Self {
            descriptor: ProviderDescriptor::new(
                ProviderId::from_static(id),
                ProviderSource::Test,
                vec![declaration(capability)],
            ),
        }
    }
}

impl Provider for AssetProvider {
    fn descriptor(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    fn dispatch(&self, _request: &HostRequest) -> Result<Vec<OutputIntent>, ProviderDiagnostic> {
        Ok(vec![OutputIntent::Asset {
            media_type: String::from("image/svg+xml"),
            bytes: vec![1, 2, 3],
        }])
    }
}

fn declaration(capability: &'static str) -> CapabilityDeclaration {
    CapabilityDeclaration::new(CapabilityId::from_static(capability), ParseFeatures::EMPTY)
}

fn parse_declaration(capability: &'static str, features: ParseFeatures) -> CapabilityDeclaration {
    CapabilityDeclaration::new(CapabilityId::from_static(capability), features)
}

fn request(capability: &'static str, generation: u64, text: &str) -> HostRequest {
    HostRequest::new(
        RequestId::new(1),
        CapabilityId::from_static(capability),
        generation,
        RequestInput::Text(String::from(text)),
    )
}

fn registration_error(
    result: Result<(), RegistrationError>,
    kind: RegistrationErrorKind,
) -> Result<RegistrationError, Box<dyn Error>> {
    match result {
        Err(error) => {
            assert_eq!(error.kind, kind);
            Ok(error)
        }
        Ok(()) => Err(format!("期望注册失败 {kind:?}，实际注册成功").into()),
    }
}

fn dispatch_error(
    result: Result<DispatchOutcome, DispatchError>,
) -> Result<DispatchError, Box<dyn Error>> {
    match result {
        Err(error) => Ok(error),
        Ok(outcome) => Err(format!("期望派发失败，实际返回 {outcome:?}").into()),
    }
}

#[test]
fn test_provider_registers_and_is_queryable_by_capability() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.echo",
        vec![declaration("test.echo.render")],
    )))?;
    let query = registry
        .capability(CapabilityId::from_static("test.echo.render"))
        .ok_or("能力未注册")?;
    assert_eq!(query.descriptor().id.as_str(), "test.echo");
    assert_eq!(query.descriptor().source, ProviderSource::Test);
    assert_eq!(registry.descriptors().count(), 1);
    assert_eq!(registry.len(), 1);
    assert!(!registry.is_empty());
    assert!(
        registry
            .capability(CapabilityId::from_static("test.absent"))
            .is_none()
    );
    Ok(())
}

#[test]
fn capabilities_are_enabled_and_disabled_independently() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    let provider_a = TestProvider::new("test.a", vec![declaration("test.a.render")]);
    let calls_a = provider_a.calls();
    registry.register(Box::new(provider_a))?;
    let provider_b = TestProvider::new("test.b", vec![declaration("test.b.render")]);
    let calls_b = provider_b.calls();
    registry.register(Box::new(provider_b))?;
    let mut lifecycle = Lifecycle::new();
    let a = CapabilityId::from_static("test.a.render");
    let b = CapabilityId::from_static("test.b.render");
    let generation = lifecycle.apply(EnabledCapabilities::from_ids([a]));
    assert_eq!(generation, lifecycle.generation());

    let outcome = dispatch(
        &registry,
        &lifecycle,
        &request("test.a.render", generation, "甲"),
    )?;
    assert_eq!(
        outcome.intents,
        vec![OutputIntent::Text(String::from("甲"))]
    );
    assert_eq!(calls_a.count(), 1);
    match dispatch_error(dispatch(
        &registry,
        &lifecycle,
        &request("test.b.render", generation, "乙"),
    ))? {
        DispatchError::Disabled { capability, .. } => assert_eq!(capability, b),
        other => return Err(format!("期望 Disabled，实际 {other:?}").into()),
    }
    assert_eq!(calls_b.count(), 0, "未启用的能力不得调用提供者");

    let generation = lifecycle.apply(EnabledCapabilities::from_ids([a, b]));
    assert!(generation > 1);
    let outcome = dispatch(
        &registry,
        &lifecycle,
        &request("test.b.render", generation, "乙"),
    )?;
    assert_eq!(
        outcome.intents,
        vec![OutputIntent::Text(String::from("乙"))]
    );
    assert_eq!(calls_b.count(), 1);

    let generation = lifecycle.apply(EnabledCapabilities::from_ids([b]));
    match dispatch_error(dispatch(
        &registry,
        &lifecycle,
        &request("test.a.render", generation, "甲"),
    ))? {
        DispatchError::Disabled { capability, .. } => assert_eq!(capability, a),
        other => return Err(format!("期望 Disabled，实际 {other:?}").into()),
    }
    assert_eq!(calls_a.count(), 1, "停用后的能力不得调用提供者");
    dispatch(
        &registry,
        &lifecycle,
        &request("test.b.render", generation, "乙"),
    )?;
    assert_eq!(calls_b.count(), 2);
    Ok(())
}

#[test]
fn one_provider_gates_each_declared_capability_independently() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    let provider = TestProvider::new(
        "test.multi",
        vec![declaration("test.multi.a"), declaration("test.multi.b")],
    );
    let calls = provider.calls();
    registry.register(Box::new(provider))?;
    let mut lifecycle = Lifecycle::new();
    let a = CapabilityId::from_static("test.multi.a");
    let b = CapabilityId::from_static("test.multi.b");

    let generation = lifecycle.apply(EnabledCapabilities::from_ids([a]));
    dispatch(
        &registry,
        &lifecycle,
        &request("test.multi.a", generation, "甲"),
    )?;
    assert_eq!(calls.count(), 1);
    match dispatch_error(dispatch(
        &registry,
        &lifecycle,
        &request("test.multi.b", generation, "乙"),
    ))? {
        DispatchError::Disabled { capability, .. } => assert_eq!(capability, b),
        other => return Err(format!("期望 Disabled，实际 {other:?}").into()),
    }
    assert_eq!(calls.count(), 1, "同一提供者的未启用能力不得放行");

    let generation = lifecycle.apply(EnabledCapabilities::from_ids([b]));
    dispatch(
        &registry,
        &lifecycle,
        &request("test.multi.b", generation, "乙"),
    )?;
    assert_eq!(calls.count(), 2);
    match dispatch_error(dispatch(
        &registry,
        &lifecycle,
        &request("test.multi.a", generation, "甲"),
    ))? {
        DispatchError::Disabled { capability, .. } => assert_eq!(capability, a),
        other => return Err(format!("期望 Disabled，实际 {other:?}").into()),
    }
    assert_eq!(calls.count(), 2, "切换后旧能力不得继续放行");
    Ok(())
}

#[test]
fn dispatch_returns_versioned_output_intent() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.echo",
        vec![declaration("test.echo.render")],
    )))?;
    let mut lifecycle = Lifecycle::new();
    let generation = lifecycle.apply(EnabledCapabilities::from_ids([CapabilityId::from_static(
        "test.echo.render",
    )]));
    let outcome = dispatch(
        &registry,
        &lifecycle,
        &request("test.echo.render", generation, "意图"),
    )?;
    assert_eq!(outcome.request_id, RequestId::new(1));
    assert_eq!(outcome.generation, generation);
    assert_eq!(
        outcome.intents,
        vec![OutputIntent::Text(String::from("意图"))]
    );
    Ok(())
}

#[test]
fn empty_input_yields_no_intents_and_asset_intents_round_trip() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.echo",
        vec![declaration("test.echo.render")],
    )))?;
    registry.register(Box::new(AssetProvider::new(
        "test.asset",
        "test.asset.render",
    )))?;
    let mut lifecycle = Lifecycle::new();
    let generation = lifecycle.apply(EnabledCapabilities::from_ids([
        CapabilityId::from_static("test.echo.render"),
        CapabilityId::from_static("test.asset.render"),
    ]));

    let empty = dispatch(
        &registry,
        &lifecycle,
        &HostRequest::new(
            RequestId::new(2),
            CapabilityId::from_static("test.echo.render"),
            generation,
            RequestInput::None,
        ),
    )?;
    assert!(empty.intents.is_empty());

    let asset = dispatch(
        &registry,
        &lifecycle,
        &request("test.asset.render", generation, "忽略"),
    )?;
    assert_eq!(
        asset.intents,
        vec![OutputIntent::Asset {
            media_type: String::from("image/svg+xml"),
            bytes: vec![1, 2, 3],
        }]
    );
    Ok(())
}

#[test]
fn duplicate_provider_id_is_rejected_and_first_provider_stays_usable() -> Result<(), Box<dyn Error>>
{
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.dup",
        vec![declaration("test.dup.first")],
    )))?;
    let error = registration_error(
        registry.register(Box::new(TestProvider::new(
            "test.dup",
            vec![declaration("test.dup.second")],
        ))),
        RegistrationErrorKind::DuplicateProviderId,
    )?;
    assert_eq!(error.provider.as_str(), "test.dup");
    assert_eq!(error.capability, None);
    assert!(
        registry
            .capability(CapabilityId::from_static("test.dup.second"))
            .is_none()
    );
    let query = registry
        .capability(CapabilityId::from_static("test.dup.first"))
        .ok_or("首个提供者应保留")?;
    assert_eq!(query.descriptor().id.as_str(), "test.dup");
    Ok(())
}

#[test]
fn conflicting_capability_is_rejected_atomically() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.first",
        vec![declaration("test.shared")],
    )))?;
    let error = registration_error(
        registry.register(Box::new(TestProvider::new(
            "test.second",
            vec![declaration("test.shared"), declaration("test.second.other")],
        ))),
        RegistrationErrorKind::CapabilityConflict,
    )?;
    assert_eq!(error.provider.as_str(), "test.second");
    assert_eq!(
        error.capability,
        Some(CapabilityId::from_static("test.shared"))
    );
    assert!(
        registry
            .capability(CapabilityId::from_static("test.second.other"))
            .is_none()
    );
    let query = registry
        .capability(CapabilityId::from_static("test.shared"))
        .ok_or("冲突后首个提供者应保留")?;
    assert_eq!(query.descriptor().id.as_str(), "test.first");
    Ok(())
}

#[test]
fn invalid_declarations_are_rejected() -> Result<(), Box<dyn Error>> {
    let cases: [(
        &'static str,
        Vec<CapabilityDeclaration>,
        RegistrationErrorKind,
        Option<&'static str>,
    ); 8] = [
        (
            "",
            vec![declaration("test.ok")],
            RegistrationErrorKind::InvalidProviderId,
            None,
        ),
        (
            "bad id",
            vec![declaration("test.ok")],
            RegistrationErrorKind::InvalidProviderId,
            None,
        ),
        (
            TOO_LONG_ID,
            vec![declaration("test.ok")],
            RegistrationErrorKind::InvalidProviderId,
            None,
        ),
        (
            "test.empty",
            Vec::new(),
            RegistrationErrorKind::EmptyCapabilities,
            None,
        ),
        (
            "test.badcap.empty",
            vec![declaration("")],
            RegistrationErrorKind::InvalidCapabilityId,
            Some(""),
        ),
        (
            "test.badcap.space",
            vec![declaration("bad cap")],
            RegistrationErrorKind::InvalidCapabilityId,
            Some("bad cap"),
        ),
        (
            "test.badcap.long",
            vec![declaration(TOO_LONG_ID)],
            RegistrationErrorKind::InvalidCapabilityId,
            Some(TOO_LONG_ID),
        ),
        (
            "test.dupe",
            vec![declaration("test.dupe.same"), declaration("test.dupe.same")],
            RegistrationErrorKind::DuplicateCapabilityInDeclaration,
            Some("test.dupe.same"),
        ),
    ];
    for (id, capabilities, kind, capability) in cases {
        let mut registry = Registry::new();
        let error = registration_error(
            registry.register(Box::new(TestProvider::new(id, capabilities))),
            kind,
        )?;
        assert_eq!(error.provider.as_str(), id, "{id}: 错误应归属出错的提供者");
        assert_eq!(
            error.capability.map(CapabilityId::as_str),
            capability,
            "{id}: 错误应归属相关能力"
        );
        assert!(registry.is_empty(), "{id}: 无效声明不得留下半注册");
    }
    Ok(())
}

#[test]
fn longest_valid_identifiers_are_accepted() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        LONGEST_ID,
        vec![declaration(LONGEST_ID)],
    )))?;
    let query = registry
        .capability(CapabilityId::from_static(LONGEST_ID))
        .ok_or("边界长度的能力应注册成功")?;
    assert_eq!(query.descriptor().id.as_str(), LONGEST_ID);
    Ok(())
}

#[test]
fn rejected_registration_leaves_existing_providers_usable() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.keep",
        vec![declaration("test.keep.render")],
    )))?;
    let error = registration_error(
        registry.register(Box::new(TestProvider::new("test.broken", Vec::new()))),
        RegistrationErrorKind::EmptyCapabilities,
    )?;
    assert_eq!(error.provider.as_str(), "test.broken");
    assert_eq!(registry.len(), 1);
    let query = registry
        .capability(CapabilityId::from_static("test.keep.render"))
        .ok_or("既有提供者应保持可用")?;
    assert_eq!(query.descriptor().id.as_str(), "test.keep");
    Ok(())
}

#[test]
fn stale_generation_is_rejected_even_after_reenable() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    let provider = TestProvider::new(
        "test.echo",
        vec![
            declaration("test.echo.render"),
            declaration("test.echo.extra"),
        ],
    );
    let calls = provider.calls();
    registry.register(Box::new(provider))?;
    let mut lifecycle = Lifecycle::new();
    let capability = CapabilityId::from_static("test.echo.render");
    let extra = CapabilityId::from_static("test.echo.extra");
    let generation = lifecycle.apply(EnabledCapabilities::from_ids([capability]));
    let stale = request("test.echo.render", generation, "旧");
    let current = lifecycle.apply(EnabledCapabilities::from_ids([capability, extra]));
    assert!(current > generation);
    assert_eq!(
        lifecycle.apply(EnabledCapabilities::from_ids([extra, capability])),
        current,
        "集合未变化时不得推进代次"
    );
    match dispatch_error(dispatch(&registry, &lifecycle, &stale))? {
        DispatchError::StaleGeneration {
            expected,
            current: actual,
            ..
        } => {
            assert_eq!(expected, generation);
            assert_eq!(actual, current);
        }
        other => return Err(format!("期望 StaleGeneration，实际 {other:?}").into()),
    }
    assert_eq!(calls.count(), 0, "失效代次不得调用提供者");

    // 在启用期间发起请求，再经历关闭与重新启用；旧请求仍须失效。
    let generation = lifecycle.apply(EnabledCapabilities::from_ids([capability]));
    let before_off_on = request("test.echo.render", generation, "关闭前");
    lifecycle.apply(EnabledCapabilities::none());
    lifecycle.apply(EnabledCapabilities::from_ids([capability]));
    match dispatch_error(dispatch(&registry, &lifecycle, &before_off_on))? {
        DispatchError::StaleGeneration { .. } => {}
        other => return Err(format!("off→on 后旧请求应失效，实际 {other:?}").into()),
    }
    assert_eq!(calls.count(), 0, "off→on 后的旧请求不得调用提供者");
    Ok(())
}

#[test]
fn unknown_capability_dispatch_is_rejected() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    let provider = TestProvider::new("test.present", vec![declaration("test.present.render")]);
    let calls = provider.calls();
    registry.register(Box::new(provider))?;
    let mut lifecycle = Lifecycle::new();
    let generation = lifecycle.apply(EnabledCapabilities::from_ids([CapabilityId::from_static(
        "test.absent",
    )]));
    match dispatch_error(dispatch(
        &registry,
        &lifecycle,
        &request("test.absent", generation, "无"),
    ))? {
        DispatchError::UnknownCapability { capability, .. } => {
            assert_eq!(capability, CapabilityId::from_static("test.absent"));
        }
        other => return Err(format!("期望 UnknownCapability，实际 {other:?}").into()),
    }
    assert_eq!(calls.count(), 0, "未知能力不得调用任何提供者");
    Ok(())
}

#[test]
fn provider_diagnostic_is_propagated_without_output() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(
        TestProvider::new("test.fail", vec![declaration("test.fail.render")])
            .failing(DiagnosticKind::Unsupported),
    ))?;
    let mut lifecycle = Lifecycle::new();
    let generation = lifecycle.apply(EnabledCapabilities::from_ids([CapabilityId::from_static(
        "test.fail.render",
    )]));
    match dispatch_error(dispatch(
        &registry,
        &lifecycle,
        &request("test.fail.render", generation, "失败"),
    ))? {
        DispatchError::Provider {
            request,
            capability,
            diagnostic,
        } => {
            assert_eq!(request, RequestId::new(1));
            assert_eq!(
                capability,
                CapabilityId::from_static("test.fail.render"),
                "诊断应归属请求的能力"
            );
            assert_eq!(diagnostic.kind, DiagnosticKind::Unsupported);
        }
        other => return Err(format!("期望 Provider 诊断，实际 {other:?}").into()),
    }
    Ok(())
}

#[test]
fn parse_features_follow_enabled_semantic_extensions_only() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.math",
        vec![parse_declaration("test.math.render", ParseFeatures::MATH)],
    )))?;
    registry.register(Box::new(TestProvider::new(
        "test.preview",
        vec![declaration("test.preview.render")],
    )))?;
    let mut lifecycle = Lifecycle::new();
    assert_eq!(
        registry.parse_features(lifecycle.enabled()),
        ParseFeatures::EMPTY
    );

    let preview = CapabilityId::from_static("test.preview.render");
    let math = CapabilityId::from_static("test.math.render");
    lifecycle.apply(EnabledCapabilities::from_ids([preview]));
    assert_eq!(
        registry.parse_features(lifecycle.enabled()),
        ParseFeatures::EMPTY
    );

    lifecycle.apply(EnabledCapabilities::from_ids([preview, math]));
    assert!(
        registry
            .parse_features(lifecycle.enabled())
            .contains(ParseFeatures::MATH)
    );

    lifecycle.apply(EnabledCapabilities::from_ids([preview]));
    assert_eq!(
        registry.parse_features(lifecycle.enabled()),
        ParseFeatures::EMPTY
    );
    Ok(())
}

#[test]
fn duplicate_parse_contribution_is_idempotent() -> Result<(), Box<dyn Error>> {
    let mut registry = Registry::new();
    registry.register(Box::new(TestProvider::new(
        "test.math.one",
        vec![parse_declaration(
            "test.math.one.render",
            ParseFeatures::MATH,
        )],
    )))?;
    registry.register(Box::new(TestProvider::new(
        "test.math.two",
        vec![parse_declaration(
            "test.math.two.render",
            ParseFeatures::MATH,
        )],
    )))?;
    let mut lifecycle = Lifecycle::new();
    lifecycle.apply(EnabledCapabilities::from_ids([
        CapabilityId::from_static("test.math.one.render"),
        CapabilityId::from_static("test.math.two.render"),
    ]));
    assert_eq!(
        registry.parse_features(lifecycle.enabled()),
        ParseFeatures::MATH
    );
    Ok(())
}
