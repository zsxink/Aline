//! 图片右键操作的纯能力模型。
//!
//! 只按引用类型、本地资源状态与平台能力给出每项操作的启用状态和不可用
//! 原因；不读磁盘、不写剪贴板、不打开 Finder。远程引用一律不下载。

use crate::media::SourceKind;

/// 图片右键菜单中的操作，顺序即呈现顺序。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageAction {
    /// 以原始编码复制图片。
    Copy,
    /// 把原始字节另存为文件。
    SaveAs,
    /// 复制本地绝对路径或远程 URL。
    CopyPath,
    /// 在文件管理器中显示本地文件。
    Reveal,
}

impl ImageAction {
    /// 菜单顺序。
    pub const ALL: [Self; 4] = [Self::Copy, Self::SaveAs, Self::CopyPath, Self::Reveal];

    /// 菜单项文案键。
    pub const fn label_key(self) -> &'static str {
        match self {
            Self::Copy => "image.action.copy",
            Self::SaveAs => "image.action.save_as",
            Self::CopyPath => "image.action.copy_path",
            Self::Reveal => "image.action.reveal",
        }
    }
}

/// 本地引用的资源状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LocalState {
    /// 普通文件可读且未超过读取上限。
    Available,
    /// 路径不存在或不是普通文件。
    Missing,
    /// 文件存在但超过读取上限。
    TooLarge,
}

/// 平台对各项操作的支持；M4 的 macOS 后端全部支持。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlatformImageCapabilities {
    /// 可写入图片剪贴板。
    pub copy_image: bool,
    /// 可弹出另存为面板并写文件。
    pub save_as: bool,
    /// 可写文本剪贴板。
    pub copy_path: bool,
    /// 可在文件管理器中显示。
    pub reveal: bool,
}

impl PlatformImageCapabilities {
    /// macOS 锁定的 GPUI 后端能力。
    pub const fn macos() -> Self {
        Self {
            copy_image: true,
            save_as: true,
            copy_path: true,
            reveal: true,
        }
    }
}

/// 单项操作的启用状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageActionState {
    action: ImageAction,
    reason: Option<&'static str>,
}

impl ImageActionState {
    /// 对应的操作。
    pub fn action(&self) -> ImageAction {
        self.action
    }

    /// 是否可执行。
    pub fn is_enabled(&self) -> bool {
        self.reason.is_none()
    }

    /// 不可用原因文案键；可用时为 `None`。
    pub fn reason(&self) -> Option<&'static str> {
        self.reason
    }
}

/// 按引用事实给出四项操作的状态。
///
/// `local` 只对 [`SourceKind::Local`] 有意义。复制与另存为需要可读取的
/// 原始字节；打开所在位置只要求文件存在；复制路径只需能确定路径本身。
pub fn image_action_states(
    kind: SourceKind,
    local: LocalState,
    capabilities: PlatformImageCapabilities,
) -> [ImageActionState; 4] {
    let state =
        |action: ImageAction, rejected: Option<&'static str>, supported: bool| ImageActionState {
            action,
            reason: rejected.or(if supported {
                None
            } else {
                Some("image.unavailable.platform")
            }),
        };
    let (copy, save_as, copy_path, reveal) = match kind {
        SourceKind::Local => {
            let bytes = match local {
                LocalState::Available => None,
                LocalState::Missing => Some("image.unavailable.missing"),
                LocalState::TooLarge => Some("image.unavailable.too_large"),
            };
            let exists = match local {
                LocalState::Available | LocalState::TooLarge => None,
                LocalState::Missing => Some("image.unavailable.missing"),
            };
            (bytes, bytes, None, exists)
        }
        SourceKind::DataUrl => (
            None,
            None,
            Some("image.unavailable.inline"),
            Some("image.unavailable.inline"),
        ),
        SourceKind::Remote => (
            Some("image.unavailable.remote"),
            Some("image.unavailable.remote"),
            None,
            Some("image.unavailable.remote"),
        ),
        SourceKind::Unsupported => {
            let rejected = Some("image.unavailable.unsupported");
            (rejected, rejected, rejected, rejected)
        }
    };
    [
        state(ImageAction::Copy, copy, capabilities.copy_image),
        state(ImageAction::SaveAs, save_as, capabilities.save_as),
        state(ImageAction::CopyPath, copy_path, capabilities.copy_path),
        state(ImageAction::Reveal, reveal, capabilities.reveal),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled(states: [ImageActionState; 4]) -> [bool; 4] {
        states.map(|state| state.is_enabled())
    }

    fn reasons(states: [ImageActionState; 4]) -> [Option<&'static str>; 4] {
        states.map(|state| state.reason())
    }

    #[test]
    fn local_available_file_enables_every_action() {
        let states = image_action_states(
            SourceKind::Local,
            LocalState::Available,
            PlatformImageCapabilities::macos(),
        );
        assert_eq!(enabled(states), [true, true, true, true]);
        assert_eq!(reasons(states), [None, None, None, None]);
        assert_eq!(
            ImageAction::ALL.map(ImageAction::label_key),
            [
                "image.action.copy",
                "image.action.save_as",
                "image.action.copy_path",
                "image.action.reveal"
            ]
        );
    }

    #[test]
    fn missing_local_file_keeps_only_copy_path() {
        let states = image_action_states(
            SourceKind::Local,
            LocalState::Missing,
            PlatformImageCapabilities::macos(),
        );
        assert_eq!(enabled(states), [false, false, true, false]);
        assert_eq!(
            reasons(states),
            [
                Some("image.unavailable.missing"),
                Some("image.unavailable.missing"),
                None,
                Some("image.unavailable.missing")
            ]
        );
    }

    #[test]
    fn oversize_local_file_still_reveals_but_cannot_be_copied() {
        let states = image_action_states(
            SourceKind::Local,
            LocalState::TooLarge,
            PlatformImageCapabilities::macos(),
        );
        assert_eq!(enabled(states), [false, false, true, true]);
        assert_eq!(
            reasons(states),
            [
                Some("image.unavailable.too_large"),
                Some("image.unavailable.too_large"),
                None,
                None
            ]
        );
    }

    #[test]
    fn data_url_copies_and_saves_but_has_no_path_or_reveal() {
        let states = image_action_states(
            SourceKind::DataUrl,
            LocalState::Missing,
            PlatformImageCapabilities::macos(),
        );
        assert_eq!(enabled(states), [true, true, false, false]);
        assert_eq!(
            reasons(states)[2..],
            [
                Some("image.unavailable.inline"),
                Some("image.unavailable.inline")
            ]
        );
    }

    #[test]
    fn remote_reference_only_copies_its_url_and_never_downloads() {
        let states = image_action_states(
            SourceKind::Remote,
            LocalState::Available,
            PlatformImageCapabilities::macos(),
        );
        assert_eq!(enabled(states), [false, false, true, false]);
        assert_eq!(
            reasons(states),
            [
                Some("image.unavailable.remote"),
                Some("image.unavailable.remote"),
                None,
                Some("image.unavailable.remote")
            ]
        );
    }

    #[test]
    fn unsupported_reference_disables_every_action() {
        let states = image_action_states(
            SourceKind::Unsupported,
            LocalState::Available,
            PlatformImageCapabilities::macos(),
        );
        assert_eq!(enabled(states), [false, false, false, false]);
        for reason in reasons(states) {
            assert_eq!(reason, Some("image.unavailable.unsupported"));
        }
    }

    #[test]
    fn platform_gaps_disable_supported_actions_with_a_platform_reason() {
        let capabilities = PlatformImageCapabilities {
            copy_image: false,
            save_as: false,
            copy_path: false,
            reveal: false,
        };
        let states = image_action_states(SourceKind::Local, LocalState::Available, capabilities);
        assert_eq!(enabled(states), [false, false, false, false]);
        for reason in reasons(states) {
            assert_eq!(reason, Some("image.unavailable.platform"));
        }
    }
}
