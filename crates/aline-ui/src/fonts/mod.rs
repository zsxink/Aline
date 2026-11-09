//! 原生字体适配边界：注册静态字体、发布有效字体代次并报告失败。
//!
//! 字体只在启动或显式实验时注册；渲染路径只读取已发布的字体快照，不读字体
//! 文件。代次表示“有效字体环境”，只在字族确实切换后推进，布局缓存键据此
//! 失效；注册失败或缺字族不冒充激活成功，也不改变当前有效字族。

use std::borrow::Cow;

use gpui::{Font, FontFallbacks, SharedString, TextSystem};

/// 最近一次字体动作的结果；失败保留此前的有效字体环境。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum FontStatus {
    /// 使用平台默认字体。
    System,
    /// 指定字族已注册且可见。
    Active,
    /// 字体数据无法注册；有效字族与代次不变。
    RegisterFailed,
    /// 字体数据已注册但字族不可见；有效字族与代次不变。
    FamilyMissing,
}

/// 字体注册后端；测试注入替身，生产实现转发 `TextSystem`。
pub(super) trait FontBackend {
    /// 注册静态字体数据；失败不改变文本系统。
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<(), ()>;
    /// 字族是否对平台文本系统可见。
    fn has_family(&self, family: &str) -> bool;
}

impl FontBackend for TextSystem {
    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<(), ()> {
        TextSystem::add_fonts(self, fonts).map_err(|_| ())
    }

    fn has_family(&self, family: &str) -> bool {
        self.all_font_names().iter().any(|name| name == family)
    }
}

/// 有效字体环境；`font()` 是渲染唯一可读的字体快照。
pub(super) struct FontEnvironment {
    generation: u64,
    status: FontStatus,
    family: SharedString,
    fallbacks: Option<FontFallbacks>,
}

impl Default for FontEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

impl FontEnvironment {
    /// 平台默认字体，代次 0。
    pub(super) fn new() -> Self {
        Self {
            generation: 0,
            status: FontStatus::System,
            family: Font::default().family,
            fallbacks: None,
        }
    }

    /// 注册并切换字族；只有字族确实可见才推进代次并替换快照。
    pub(super) fn activate<B: FontBackend + ?Sized>(
        &mut self,
        backend: &B,
        fonts: Vec<Cow<'static, [u8]>>,
        family: SharedString,
        fallbacks: Vec<String>,
    ) -> Result<(), FontStatus> {
        if backend.add_fonts(fonts).is_err() {
            self.status = FontStatus::RegisterFailed;
            return Err(FontStatus::RegisterFailed);
        }
        if !backend.has_family(&family) {
            self.status = FontStatus::FamilyMissing;
            return Err(FontStatus::FamilyMissing);
        }
        self.generation = self.generation.saturating_add(1);
        self.family = family;
        self.fallbacks = (!fallbacks.is_empty()).then(|| FontFallbacks::from_fonts(fallbacks));
        self.status = FontStatus::Active;
        Ok(())
    }

    /// 启动时注册内置字体；M1 不随包分发字体，列表为空时保持平台字体。
    pub(super) fn activate_bundled<B: FontBackend + ?Sized>(
        &mut self,
        backend: &B,
        fonts: &[(&'static str, &'static [u8])],
    ) {
        for (family, bytes) in fonts {
            let _ = self.activate(
                backend,
                vec![Cow::Borrowed(*bytes)],
                SharedString::from(*family),
                Vec::new(),
            );
        }
    }

    /// 当前有效字体代次；进入布局缓存键。
    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    /// 最近一次字体动作的结果；失败必须在界面可见。
    pub(super) fn status(&self) -> FontStatus {
        self.status
    }

    /// 正文字体快照；族与回退来自已发布环境。
    pub(super) fn font(&self) -> Font {
        Font {
            family: self.family.clone(),
            fallbacks: self.fallbacks.clone(),
            ..Font::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::{FontBackend, FontEnvironment, FontStatus};

    struct FakeBackend {
        accept: bool,
        families: Vec<&'static str>,
        added: Cell<usize>,
    }

    impl FakeBackend {
        fn new(accept: bool, families: &[&'static str]) -> Self {
            Self {
                accept,
                families: families.to_vec(),
                added: Cell::new(0),
            }
        }
    }

    impl FontBackend for FakeBackend {
        fn add_fonts(&self, _fonts: Vec<std::borrow::Cow<'static, [u8]>>) -> Result<(), ()> {
            self.added.set(self.added.get() + 1);
            if self.accept { Ok(()) } else { Err(()) }
        }

        fn has_family(&self, family: &str) -> bool {
            self.families.contains(&family)
        }
    }

    #[test]
    fn registration_failure_keeps_previous_generation_and_family() {
        let backend = FakeBackend::new(false, &[]);
        let mut fonts = FontEnvironment::new();
        let result = fonts.activate(
            &backend,
            vec![std::borrow::Cow::Borrowed(b"broken".as_slice())],
            "Fixture Sans".into(),
            Vec::new(),
        );
        assert_eq!(result, Err(FontStatus::RegisterFailed));
        assert_eq!(fonts.status(), FontStatus::RegisterFailed);
        assert_eq!(fonts.generation(), 0);
        assert_eq!(fonts.font().family, gpui::Font::default().family);
    }

    #[test]
    fn missing_family_does_not_claim_activation() {
        let backend = FakeBackend::new(true, &["Other Sans"]);
        let mut fonts = FontEnvironment::new();
        let result = fonts.activate(
            &backend,
            vec![std::borrow::Cow::Borrowed(b"font".as_slice())],
            "Fixture Sans".into(),
            Vec::new(),
        );
        assert_eq!(result, Err(FontStatus::FamilyMissing));
        assert_eq!(fonts.status(), FontStatus::FamilyMissing);
        assert_eq!(fonts.generation(), 0);
        assert_eq!(fonts.font().family, gpui::Font::default().family);
        assert!(fonts.font().fallbacks.is_none());
    }

    #[test]
    fn activation_publishes_generation_family_and_fallbacks() {
        let backend = FakeBackend::new(true, &["Fixture Sans"]);
        let mut fonts = FontEnvironment::new();
        let result = fonts.activate(
            &backend,
            vec![std::borrow::Cow::Borrowed(b"font".as_slice())],
            "Fixture Sans".into(),
            vec!["PingFang SC".to_owned()],
        );
        assert_eq!(result, Ok(()));
        assert_eq!(fonts.status(), FontStatus::Active);
        assert_eq!(fonts.generation(), 1);
        let font = fonts.font();
        assert_eq!(font.family, "Fixture Sans");
        assert!(font.fallbacks.is_some());
        if let Some(fallbacks) = font.fallbacks {
            assert_eq!(fallbacks.fallback_list(), ["PingFang SC"]);
        }
    }

    #[test]
    fn repeated_activation_advances_generation_each_time() {
        let backend = FakeBackend::new(true, &["Fixture Sans"]);
        let mut fonts = FontEnvironment::new();
        for _ in 0..2 {
            assert_eq!(
                fonts.activate(
                    &backend,
                    vec![std::borrow::Cow::Borrowed(b"font".as_slice())],
                    "Fixture Sans".into(),
                    Vec::new(),
                ),
                Ok(())
            );
        }
        assert_eq!(fonts.generation(), 2);
        assert_eq!(backend.added.get(), 2);
    }

    #[test]
    fn bundled_list_is_explicit_and_optional() {
        let backend = FakeBackend::new(true, &[]);
        let mut fonts = FontEnvironment::new();
        fonts.activate_bundled(&backend, &[]);
        assert_eq!(fonts.generation(), 0);
        assert_eq!(fonts.status(), FontStatus::System);
        assert_eq!(backend.added.get(), 0);
    }
}
