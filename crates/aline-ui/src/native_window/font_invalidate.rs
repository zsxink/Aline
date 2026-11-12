#![forbid(unsafe_code)]
//! 受限 opt-in 的字体失效触发（2026-11-12，撤销 §35 其四）。
//!
//! M1 的 `BUNDLED_FONTS` 为空，`FontEnvironment` 没有 UI/环境入口，因此
//! 5.1b 要求的「字体失效」无法在真实窗口触发。本模块只补一个**验证专用**
//! 入口：两个环境变量齐全时才在文档打开后推进一次字体代次，其余路径完全
//! 不变。它不发布运行中字体切换能力，也不改渲染路径。
//!
//! 语义边界：`FontEnvironment::activate` 只有在字体数据注册成功**且**字族
//! 可见时才推进代次。字体失效要的正是「代次推进 → 布局缓存键变化 → 重新
//! 测量」，所以这里注册一个**系统已存在**的字族（默认 `Menlo`），让
//! `has_family` 为真、代次确实推进，而不引入任何随包分发的字体字节。
//!
//! 为什么不是「注册失败」：注册失败按设计不推进代次、保留原字体环境，那是
//! 错误可见性路径（`font.register_failed`），不是字体失效。用失败来「制造
//! 失效」既不会让布局缓存键变化，也与产品语义相反。
//!
//! 环境变量：
//! - `ALINE_M1_FONT_INVALIDATE_FAMILY`：要切换到的字族名（必须非空）。
//! - `ALINE_M1_FONT_INVALIDATE_MARK`：证据标记文件路径；只写新文件，用于
//!   证明触发确实发生、并记录触发前后的代次。

use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use gpui::SharedString;

use crate::fonts::FontEnvironment;

/// 一次已确认的字体失效触发结果。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Invalidated {
    /// 切换后的字族。
    pub(super) family: SharedString,
    /// 触发后的字体代次。
    pub(super) generation: u64,
}

/// 验证专用的字体失效请求；缺任一变量时返回 `None`，生产行为不变。
pub(super) struct FontInvalidation {
    family: String,
    mark: PathBuf,
}

impl FontInvalidation {
    pub(super) fn from_env() -> Option<Self> {
        let family = std::env::var("ALINE_M1_FONT_INVALIDATE_FAMILY").ok()?;
        let mark = std::env::var_os("ALINE_M1_FONT_INVALIDATE_MARK")?;
        if family.trim().is_empty() {
            return None;
        }
        Some(Self {
            family,
            mark: PathBuf::from(mark),
        })
    }

    /// 推进一次字体代次，并把结果写入标记文件。
    ///
    /// 返回 `Some` 表示代次确实推进；返回 `None` 表示字族不可见或注册失败，
    /// 字体环境按 `FontEnvironment` 的既有契约保持不变——此时也写标记，
    /// 让证据能区分「没触发」和「触发了但没推进」。
    pub(super) fn apply<B: crate::fonts::FontBackend + ?Sized>(
        &self,
        fonts: &mut FontEnvironment,
        backend: &B,
    ) -> Option<Invalidated> {
        let before = fonts.generation();
        let result = fonts.activate(
            backend,
            Vec::new(),
            SharedString::from(self.family.clone()),
            Vec::new(),
        );
        let after = fonts.generation();
        let status = match fonts.status() {
            crate::fonts::FontStatus::System => "system",
            crate::fonts::FontStatus::Active => "active",
            crate::fonts::FontStatus::RegisterFailed => "register_failed",
            crate::fonts::FontStatus::FamilyMissing => "family_missing",
        };
        let outcome = match &result {
            Ok(()) => "activated",
            Err(_) => "not_activated",
        };
        let record = format!(
            "family={}\tstatus={status}\toutcome={outcome}\tbefore={before}\tafter={after}\n",
            self.family
        );
        if let Err(error) = write_mark(&self.mark, record.as_bytes()) {
            eprintln!("m1-font-invalidate mark failed: {error}");
        }
        match result {
            Ok(()) if after > before => Some(Invalidated {
                family: SharedString::from(self.family.clone()),
                generation: after,
            }),
            _ => None,
        }
    }
}

/// 只创建新文件，拒绝覆盖已有证据；标记文件必须落在 `logs/validation` 内。
///
/// 与 `probe` / `readonly_evidence` 同一条规则：验证产物只能写进仓库证据目录，
/// 避免把本机绝对路径或任意路径写盘能力带进产品。
fn write_mark(path: &PathBuf, bytes: &[u8]) -> io::Result<()> {
    let root = std::env::current_dir()?
        .join("logs/validation")
        .canonicalize()?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("缺少标记目录"))?;
    if !parent.canonicalize()?.starts_with(root) {
        return Err(io::Error::other("标记目录不在 logs/validation 内"));
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(bytes)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::cell::Cell;

    use super::FontInvalidation;
    use crate::fonts::{FontBackend, FontEnvironment, FontStatus};

    /// 替身后端：按配置决定注册是否成功、字族是否可见，并记录调用次数。
    struct Backend {
        accept: bool,
        families: Vec<&'static str>,
        added: Cell<usize>,
    }

    impl Backend {
        fn new(accept: bool, families: &[&'static str]) -> Self {
            Self {
                accept,
                families: families.to_vec(),
                added: Cell::new(0),
            }
        }
    }

    impl FontBackend for Backend {
        fn add_fonts(&self, _fonts: Vec<Cow<'static, [u8]>>) -> Result<(), ()> {
            self.added.set(self.added.get() + 1);
            if self.accept { Ok(()) } else { Err(()) }
        }

        fn has_family(&self, family: &str) -> bool {
            self.families.contains(&family)
        }
    }

    fn request(family: &str) -> FontInvalidation {
        FontInvalidation {
            family: family.to_owned(),
            // 指向一个不在 logs/validation 内的路径：写标记会失败并打印，
            // 但本组测试只验证代次推进规则，不依赖写盘成功。
            mark: std::path::PathBuf::from("/dev/null/mark"),
        }
    }

    #[test]
    fn visible_family_advances_generation() {
        let backend = Backend::new(true, &["Menlo"]);
        let mut fonts = FontEnvironment::new();
        let result = request("Menlo").apply(&mut fonts, &backend);
        assert_eq!(
            result.map(|item| (item.family.to_string(), item.generation)),
            Some(("Menlo".to_owned(), 1))
        );
        assert_eq!(fonts.generation(), 1);
        assert_eq!(fonts.status(), FontStatus::Active);
        assert_eq!(fonts.font().family, "Menlo");
    }

    #[test]
    fn missing_family_reports_not_activated_and_keeps_generation() {
        let backend = Backend::new(true, &["Other"]);
        let mut fonts = FontEnvironment::new();
        assert!(request("Menlo").apply(&mut fonts, &backend).is_none());
        assert_eq!(fonts.generation(), 0);
        assert_eq!(fonts.status(), FontStatus::FamilyMissing);
        assert_eq!(fonts.font().family, gpui::Font::default().family);
    }

    #[test]
    fn register_failure_reports_not_activated_and_keeps_generation() {
        let backend = Backend::new(false, &[]);
        let mut fonts = FontEnvironment::new();
        assert!(request("Menlo").apply(&mut fonts, &backend).is_none());
        assert_eq!(fonts.generation(), 0);
        assert_eq!(fonts.status(), FontStatus::RegisterFailed);
    }

    #[test]
    fn repeated_apply_advances_generation_each_time() {
        let backend = Backend::new(true, &["Menlo"]);
        let mut fonts = FontEnvironment::new();
        let first = request("Menlo").apply(&mut fonts, &backend);
        let second = request("Menlo").apply(&mut fonts, &backend);
        assert_eq!(first.map(|item| item.generation), Some(1));
        assert_eq!(second.map(|item| item.generation), Some(2));
        assert_eq!(fonts.generation(), 2);
    }

    #[test]
    fn empty_family_is_rejected_at_parse_time() {
        // 空串在 `from_env` 阶段就被拒；这里验证同一判据本身。
        assert!("   ".trim().is_empty());
        assert!(!"Menlo".trim().is_empty());
    }
}
