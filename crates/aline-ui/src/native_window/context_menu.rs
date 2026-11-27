//! 正文右键菜单：新增链接、插入图片与图片资源操作。
//!
//! 菜单项按引用类型、本地资源状态与平台能力生成；图片操作只读取已通过
//! 入口校验的本地字节或 `data:` 内联字节，远程引用永不下载。

use std::fmt;
use std::path::{Path, PathBuf};

use gpui::{
    AnyElement, ClipboardItem, Context, FocusHandle, Image, MouseButton, Pixels, Point,
    SharedString, Window, div, prelude::*, px, rgb,
};

use super::body_image_cache::image_format;
use super::insert_dialog::InsertKind;
use super::readonly_reader::ContextTarget;
use super::{NativeDocumentWindow, tokens};
use crate::body_image::{BodyImageSource, local_destination_path, resolve_body_image};
use crate::document::MAX_RESOURCE_BYTES;
use crate::image_actions::{
    ImageAction, LocalState, PlatformImageCapabilities, image_action_states,
};
use crate::media::{MediaBudget, MediaFormat, SourceKind, classify_source};
use crate::{DocumentId, WorkingVersion};

/// 菜单命令；文字菜单与图片菜单共用一个执行入口。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ContextCommand {
    /// 打开链接弹窗。
    AddLink,
    /// 打开图片弹窗。
    InsertImage,
    /// 图片资源操作。
    Image(ImageAction),
}

/// 一个菜单项。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct ContextMenuItem {
    label_key: &'static str,
    command: ContextCommand,
    enabled: bool,
    reason_key: Option<&'static str>,
}

impl ContextMenuItem {
    /// 菜单项文案键。
    pub(super) const fn label_key(&self) -> &'static str {
        self.label_key
    }

    /// 对应命令。
    pub(super) const fn command(&self) -> ContextCommand {
        self.command
    }

    /// 是否可执行。
    pub(super) const fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// 不可用原因文案键。
    pub(super) const fn reason_key(&self) -> Option<&'static str> {
        self.reason_key
    }
}

/// 文字右键菜单：新增链接与插入图片。
pub(super) fn text_menu_items() -> Vec<ContextMenuItem> {
    [
        ("context.add_link", ContextCommand::AddLink),
        ("context.insert_image", ContextCommand::InsertImage),
    ]
    .into_iter()
    .map(|(label_key, command)| ContextMenuItem {
        label_key,
        command,
        enabled: true,
        reason_key: None,
    })
    .collect()
}

/// 图片右键菜单：按引用类型与本地资源状态生成四项操作。
pub(super) fn image_menu_items(
    destination: &str,
    document_dir: &Path,
    max_bytes: u64,
) -> Vec<ContextMenuItem> {
    let kind = classify_source(destination);
    let local = if kind == SourceKind::Local {
        local_resource_state(destination, document_dir, max_bytes)
    } else {
        LocalState::Available
    };
    image_action_states(kind, local, PlatformImageCapabilities::macos())
        .into_iter()
        .map(|state| ContextMenuItem {
            label_key: state.action().label_key(),
            command: ContextCommand::Image(state.action()),
            enabled: state.is_enabled(),
            reason_key: state.reason(),
        })
        .collect()
}

/// 本地引用的资源状态；只读元数据，不解码。
pub(super) fn local_resource_state(
    destination: &str,
    document_dir: &Path,
    max_bytes: u64,
) -> LocalState {
    let Ok(path) = local_destination_path(destination, document_dir) else {
        return LocalState::Missing;
    };
    match std::fs::metadata(&path) {
        Ok(metadata) if metadata.is_file() => {
            if metadata.len() > max_bytes {
                LocalState::TooLarge
            } else {
                LocalState::Available
            }
        }
        _ => LocalState::Missing,
    }
}

/// 键盘移动：跳过不可用项，到头回绕；没有可用项时返回 `None`。
pub(super) fn step(items: &[ContextMenuItem], current: usize, backward: bool) -> Option<usize> {
    let count = items.len();
    if count == 0 {
        return None;
    }
    (1..=count).find_map(|offset| {
        let index = if backward {
            (current + count - offset) % count
        } else {
            (current + offset) % count
        };
        items.get(index).filter(|item| item.enabled).map(|_| index)
    })
}

/// 菜单第一项可用位置；没有可用项时为 `None`。
pub(super) fn first_enabled(items: &[ContextMenuItem]) -> Option<usize> {
    items.iter().position(ContextMenuItem::is_enabled)
}

/// 图片操作所需的原始字节与建议文件名。
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ImagePayload {
    pub(super) bytes: Vec<u8>,
    pub(super) format: MediaFormat,
    pub(super) suggested_name: String,
}

/// 读取图片字节失败的原因。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ImagePayloadError {
    /// 引用类型不支持、文件缺失或超过上限。
    Unavailable,
    /// 读取失败。
    Io,
}

impl fmt::Display for ImagePayloadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for ImagePayloadError {}

/// 按入口校验读取图片原始字节。
///
/// 本地栅格图读取文件字节；SVG 与 `data:` 返回入口已验证/净化的字节。
/// 读取以预算上限截断，文件在预检后变大也不会超限。
pub(super) fn load_image_payload(
    destination: &str,
    document_dir: &Path,
    budget: MediaBudget,
) -> Result<ImagePayload, ImagePayloadError> {
    let image = resolve_body_image(destination, document_dir, Some(budget))
        .map_err(|_| ImagePayloadError::Unavailable)?;
    let format = image.format();
    let bytes = match image.source() {
        BodyImageSource::Local(path) => read_bounded(path, budget.max_source_bytes())?,
        BodyImageSource::Bytes(bytes) => bytes.clone(),
    };
    // 本地 SVG 的复制字节经过净化，名字仍取原文件名；其余按格式给出
    // `image.<ext>`，避免建议名依赖文档目标里的目录结构。
    let suggested_name = local_destination_path(destination, document_dir)
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| format!("image.{}", format.extension()));
    Ok(ImagePayload {
        bytes,
        format,
        suggested_name,
    })
}

/// 以预算上限读取文件；预检后文件变大时按超限拒绝，不无限分配。
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, ImagePayloadError> {
    use std::io::Read;

    let file = std::fs::File::open(path).map_err(|_| ImagePayloadError::Io)?;
    let mut bytes = Vec::new();
    file.take(limit.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| ImagePayloadError::Io)?;
    if bytes.len() as u64 > limit {
        return Err(ImagePayloadError::Unavailable);
    }
    Ok(bytes)
}

/// 图片操作读取预算：只收紧源字节，其余维度交给入口校验与解码器。
pub(super) fn image_action_budget() -> MediaBudget {
    MediaBudget::new(MAX_RESOURCE_BYTES as u64, u64::MAX, u64::MAX, u32::MAX)
}

/// 另存为建议文件名：本地引用取原文件名，其余退回 `image`。
pub(super) fn suggested_image_name(destination: &str, directory: &Path) -> String {
    local_destination_path(destination, directory)
        .ok()
        .and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "image".to_owned())
}

/// 打开中的正文右键菜单。
///
/// 绑定打开时的文档身份与工作版本；任何编辑推进版本后菜单即视为过期，
/// 不再绘制也不再接受命令。
pub(super) struct ContextMenu {
    target: ContextTarget,
    document: DocumentId,
    version: WorkingVersion,
    position: Point<Pixels>,
    items: Vec<ContextMenuItem>,
    focus: FocusHandle,
    active: Option<usize>,
}

impl NativeDocumentWindow {
    /// 打开右键菜单；文字菜单只对可编辑文档开放，图片菜单始终可用。
    pub(super) fn open_context_menu(
        &mut self,
        target: ContextTarget,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.insert_dialog.is_some() || self.find_contains_focus(window, cx) {
            return;
        }
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        if matches!(target, ContextTarget::Text) && !document.is_editable() {
            return;
        }
        let items = match &target {
            ContextTarget::Text => text_menu_items(),
            ContextTarget::Image(destination) => {
                let directory = document.path().parent().unwrap_or(Path::new(""));
                image_menu_items(destination, directory, MAX_RESOURCE_BYTES as u64)
            }
        };
        if items.is_empty() {
            return;
        }
        let document_id = document.core().document_id().clone();
        let version = document.core().version();
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.context_menu = Some(ContextMenu {
            target,
            document: document_id,
            version,
            position,
            active: first_enabled(&items),
            items,
            focus,
        });
        cx.notify();
    }

    /// 文档替换或工作版本推进后丢弃菜单；返回是否确实丢弃。
    pub(super) fn prune_context_menu(&mut self) -> bool {
        let Some(menu) = self.context_menu.as_ref() else {
            return false;
        };
        let stale = self.state.session.document().is_none_or(|document| {
            &menu.document != document.core().document_id()
                || menu.version != document.core().version()
        });
        if stale {
            self.context_menu = None;
        }
        stale
    }

    pub(super) fn dismiss_context_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.context_menu.take().is_none() {
            return false;
        }
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    fn visible_context_menu(&self) -> Option<&ContextMenu> {
        let menu = self.context_menu.as_ref()?;
        let document = self.state.session.document()?;
        if &menu.document != document.core().document_id()
            || menu.version != document.core().version()
        {
            return None;
        }
        Some(menu)
    }

    pub(super) fn render_context_menu(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.visible_context_menu()?;
        let panel_width = px(220.);
        let panel_height = px(menu.items.len() as f32 * 30.0 + 8.0);
        let width = window.viewport_size().width;
        let height = window.viewport_size().height;
        let left = menu.position.x.min((width - panel_width).max(px(0.)));
        let top = menu.position.y.min((height - panel_height).max(px(0.)));
        let panel = div()
            .id("context-menu")
            .track_focus(&menu.focus)
            .absolute()
            .left(left)
            .top(top)
            .w(panel_width)
            .occlude()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(rgb(tokens::BORDER))
            .bg(rgb(tokens::SURFACE))
            .p_1()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                this.context_menu_key(event, window, cx);
            }))
            .children(menu.items.iter().enumerate().filter_map(|(index, item)| {
                let label = self.copy.translator.t(item.label_key()).ok()?;
                let reason = item
                    .reason_key()
                    .and_then(|key| self.copy.translator.t(key).ok());
                let command = item.command();
                let enabled = item.is_enabled();
                let active = menu.active == Some(index);
                let mut entry = div()
                    .id(item.label_key())
                    .role(gpui::Role::MenuItem)
                    .aria_label(label.clone())
                    .flex()
                    .flex_col()
                    .px_2()
                    .py_1()
                    .rounded_sm()
                    .when(active, |entry| entry.bg(rgb(tokens::CODE_SURFACE)))
                    .child(div().child(SharedString::from(label)));
                if let Some(reason) = reason {
                    entry = entry.aria_description(reason.clone()).child(
                        div()
                            .text_xs()
                            .text_color(rgb(tokens::SECONDARY))
                            .child(SharedString::from(reason)),
                    );
                }
                if !enabled {
                    return Some(entry.text_color(rgb(tokens::SECONDARY)).into_any_element());
                }
                Some(
                    entry
                        .cursor_pointer()
                        .hover(|style| style.bg(rgb(tokens::CODE_SURFACE)))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.run_context_menu_command(command, window, cx);
                            cx.stop_propagation();
                        }))
                        .into_any_element(),
                )
            }));
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.dismiss_context_menu(window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, _, window, cx| {
                                this.dismiss_context_menu(window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(panel)
                .into_any_element(),
        )
    }

    pub(super) fn move_context_menu_focus(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .context_menu
            .as_ref()
            .is_some_and(|menu| menu.focus.is_focused(window))
        {
            return false;
        }
        if self.visible_context_menu().is_none() {
            return self.dismiss_context_menu(window, cx);
        }
        let Some(menu) = self.context_menu.as_mut() else {
            return false;
        };
        if let Some(current) = menu.active {
            menu.active = step(&menu.items, current, backward);
        }
        cx.notify();
        true
    }

    fn context_menu_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "up" | "down" | "tab" => {
                let backward = event.keystroke.key == "up"
                    || event.keystroke.key == "tab" && event.keystroke.modifiers.shift;
                self.move_context_menu_focus(backward, window, cx);
            }
            "enter" | "space" => {
                let command = self.context_menu.as_ref().and_then(|menu| {
                    let index = menu.active?;
                    menu.items
                        .get(index)
                        .filter(|item| item.is_enabled())
                        .map(ContextMenuItem::command)
                });
                if let Some(command) = command {
                    self.run_context_menu_command(command, window, cx);
                }
            }
            "escape" => {
                self.dismiss_context_menu(window, cx);
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    pub(super) fn run_context_menu_command(
        &mut self,
        command: ContextCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.context_menu.take() else {
            return;
        };
        self.focus.focus(window, cx);
        match command {
            ContextCommand::AddLink => self.open_insert_dialog(InsertKind::Link, window, cx),
            ContextCommand::InsertImage => {
                self.open_insert_dialog(InsertKind::Image, window, cx);
            }
            ContextCommand::Image(action) => {
                if let ContextTarget::Image(destination) = menu.target {
                    self.run_image_action(destination, action, window, cx);
                }
            }
        }
        cx.notify();
    }

    /// 执行一项图片资源操作；全程不改文档。
    fn run_image_action(
        &mut self,
        destination: SharedString,
        action: ImageAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(directory) = self.state.session.document().map(|document| {
            document
                .path()
                .parent()
                .unwrap_or(Path::new(""))
                .to_path_buf()
        }) else {
            return;
        };
        match action {
            ImageAction::CopyPath => {
                let text = match classify_source(&destination) {
                    SourceKind::Local => local_destination_path(&destination, &directory)
                        .ok()
                        .map(|path| path.to_string_lossy().into_owned()),
                    SourceKind::Remote => Some(destination.to_string()),
                    SourceKind::DataUrl | SourceKind::Unsupported => None,
                };
                match text {
                    Some(text) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(text));
                        self.state.feedback = Some("image.copy_path_done");
                    }
                    None => {
                        self.state.feedback = Some(match classify_source(&destination) {
                            SourceKind::DataUrl => "image.unavailable.inline",
                            _ => "image.unavailable.unsupported",
                        });
                    }
                }
            }
            ImageAction::Reveal => match local_destination_path(&destination, &directory) {
                Ok(path) if path.is_file() => cx.reveal_path(&path),
                _ => self.state.feedback = Some("image.unavailable.missing"),
            },
            ImageAction::Copy | ImageAction::SaveAs => {
                self.start_image_resource_task(destination, action, directory, window, cx);
            }
        }
        cx.notify();
    }

    /// 复制或另存为：后台读取字节，前台写剪贴板或按面板结果写文件。
    fn start_image_resource_task(
        &mut self,
        destination: SharedString,
        action: ImageAction,
        directory: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let budget = image_action_budget();
        match action {
            ImageAction::Copy => {
                if self.image_task.is_some() {
                    return;
                }
                let destination = destination.to_string();
                self.image_task = Some(cx.spawn_in(window, async move |weak, cx| {
                    let loaded = cx
                        .background_executor()
                        .spawn(async move { load_image_payload(&destination, &directory, budget) })
                        .await;
                    let _ = weak.update_in(cx, |this, _window, cx| {
                        this.image_task = None;
                        match loaded {
                            Ok(payload) => {
                                let image =
                                    Image::from_bytes(image_format(payload.format), payload.bytes);
                                cx.write_to_clipboard(ClipboardItem::new_image(&image));
                                this.state.feedback = Some("image.copy_done");
                            }
                            Err(_) => this.state.feedback = Some("image.copy_failed"),
                        }
                        cx.notify();
                    });
                }));
            }
            ImageAction::SaveAs => {
                if self.image_save_task.is_some() {
                    return;
                }
                let suggested = suggested_image_name(&destination, &directory);
                let prompt = cx.prompt_for_new_path(&directory, Some(&suggested));
                self.image_save_task = Some(cx.spawn_in(window, async move |weak, cx| {
                    let selected = match prompt.await {
                        Ok(Ok(selection)) => selection,
                        _ => None,
                    };
                    let Some(path) = selected else {
                        let _ = weak.update_in(cx, |this, _, cx| {
                            this.image_save_task = None;
                            cx.notify();
                        });
                        return;
                    };
                    let destination = destination.to_string();
                    let saved = cx
                        .background_executor()
                        .spawn(async move {
                            match load_image_payload(&destination, &directory, budget) {
                                Ok(payload) => std::fs::write(&path, &payload.bytes).is_ok(),
                                Err(_) => false,
                            }
                        })
                        .await;
                    let _ = weak.update_in(cx, |this, _window, cx| {
                        this.image_save_task = None;
                        this.state.feedback = Some(if saved {
                            "image.save_done"
                        } else {
                            "image.save_failed"
                        });
                        cx.notify();
                    });
                }));
            }
            ImageAction::CopyPath | ImageAction::Reveal => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    /// 1×1 透明 PNG；只用于 `data:` 目标。
    const PIXEL_PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";

    fn samples() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/fixtures/m4-v03/samples")
    }

    fn budget() -> MediaBudget {
        MediaBudget::new(1 << 20, 1 << 24, 1 << 28, 8)
    }

    /// 测试用临时目录；Drop 时删除。
    struct TempDir(PathBuf);

    impl TempDir {
        fn new(prefix: &str) -> Result<Self, Box<dyn std::error::Error>> {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
            for attempt in 0..100u32 {
                let path = std::env::temp_dir().join(format!(
                    "aline-context-{prefix}-{}-{nanos:x}-{attempt}",
                    std::process::id()
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Ok(Self(path)),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => return Err(error.into()),
                }
            }
            Err("无法创建菜单测试目录".into())
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn item(label: &'static str, enabled: bool) -> ContextMenuItem {
        ContextMenuItem {
            label_key: label,
            command: ContextCommand::AddLink,
            enabled,
            reason_key: if enabled {
                None
            } else {
                Some("image.unavailable.missing")
            },
        }
    }

    #[test]
    fn text_menu_offers_new_link_and_image() {
        let items = text_menu_items();
        assert_eq!(items.len(), 2);
        assert_eq!(
            items
                .iter()
                .map(ContextMenuItem::label_key)
                .collect::<Vec<_>>(),
            ["context.add_link", "context.insert_image"]
        );
        assert_eq!(
            items
                .iter()
                .map(ContextMenuItem::command)
                .collect::<Vec<_>>(),
            [ContextCommand::AddLink, ContextCommand::InsertImage]
        );
        assert!(items.iter().all(ContextMenuItem::is_enabled));
        assert!(items.iter().all(|item| item.reason_key().is_none()));
    }

    #[test]
    fn local_image_menu_matches_the_resource_state() -> Result<(), Box<dyn std::error::Error>> {
        let dir = TempDir::new("local")?;
        fs::write(dir.path().join("a.png"), b"png")?;
        let limit = 1 << 20;

        let present = image_menu_items("a.png", dir.path(), limit);
        assert_eq!(
            present
                .iter()
                .map(ContextMenuItem::label_key)
                .collect::<Vec<_>>(),
            [
                "image.action.copy",
                "image.action.save_as",
                "image.action.copy_path",
                "image.action.reveal"
            ]
        );
        assert!(present.iter().all(ContextMenuItem::is_enabled));

        let missing = image_menu_items("missing.png", dir.path(), limit);
        assert_eq!(
            missing
                .iter()
                .map(ContextMenuItem::is_enabled)
                .collect::<Vec<_>>(),
            [false, false, true, false]
        );
        assert_eq!(
            missing
                .iter()
                .map(ContextMenuItem::reason_key)
                .collect::<Vec<_>>(),
            [
                Some("image.unavailable.missing"),
                Some("image.unavailable.missing"),
                None,
                Some("image.unavailable.missing")
            ]
        );

        let oversize = image_menu_items("a.png", dir.path(), 2);
        assert_eq!(
            oversize
                .iter()
                .map(ContextMenuItem::is_enabled)
                .collect::<Vec<_>>(),
            [false, false, true, true]
        );
        assert_eq!(
            oversize
                .iter()
                .map(ContextMenuItem::reason_key)
                .collect::<Vec<_>>(),
            [
                Some("image.unavailable.too_large"),
                Some("image.unavailable.too_large"),
                None,
                None
            ]
        );
        Ok(())
    }

    #[test]
    fn remote_and_inline_menus_explain_what_is_unavailable() {
        let remote = image_menu_items("https://example.test/a.png", Path::new("/docs"), 1 << 20);
        assert_eq!(
            remote
                .iter()
                .map(ContextMenuItem::is_enabled)
                .collect::<Vec<_>>(),
            [false, false, true, false]
        );
        assert_eq!(
            remote
                .iter()
                .map(ContextMenuItem::reason_key)
                .collect::<Vec<_>>(),
            [
                Some("image.unavailable.remote"),
                Some("image.unavailable.remote"),
                None,
                Some("image.unavailable.remote")
            ]
        );

        let inline = image_menu_items(PIXEL_PNG, Path::new("/docs"), 1 << 20);
        assert_eq!(
            inline
                .iter()
                .map(ContextMenuItem::is_enabled)
                .collect::<Vec<_>>(),
            [true, true, false, false]
        );
        assert_eq!(
            inline
                .iter()
                .map(ContextMenuItem::reason_key)
                .collect::<Vec<_>>(),
            [
                None,
                None,
                Some("image.unavailable.inline"),
                Some("image.unavailable.inline")
            ]
        );
    }

    #[test]
    fn navigation_skips_disabled_items_and_wraps() {
        let items = [
            item("a", false),
            item("b", true),
            item("c", false),
            item("d", true),
        ];
        assert_eq!(step(&items, 0, false), Some(1));
        assert_eq!(step(&items, 1, false), Some(3));
        assert_eq!(step(&items, 3, false), Some(1));
        assert_eq!(step(&items, 1, true), Some(3));
        assert_eq!(step(&items, 3, true), Some(1));
        assert_eq!(step(&items, 2, false), Some(3));

        let none = [item("a", false), item("b", false)];
        assert_eq!(step(&none, 0, false), None);
        assert_eq!(step(&[], 0, false), None);

        assert_eq!(first_enabled(&items), Some(1));
        assert_eq!(first_enabled(&none), None);
    }

    #[test]
    fn suggested_names_follow_the_local_file_or_fall_back() {
        let dir = Path::new("/docs");
        assert_eq!(suggested_image_name("a/b/photo.png", dir), "photo.png");
        assert_eq!(suggested_image_name(PIXEL_PNG, dir), "image");
        assert_eq!(
            suggested_image_name("https://example.test/a.png", dir),
            "image"
        );
    }

    #[test]
    fn image_action_budget_only_caps_the_source_bytes() {
        assert_eq!(
            image_action_budget().max_source_bytes(),
            MAX_RESOURCE_BYTES as u64
        );
    }

    #[test]
    fn image_payloads_read_local_files_and_inline_bytes() -> Result<(), Box<dyn std::error::Error>>
    {
        let png = load_image_payload("plain.png", &samples(), budget())?;
        assert_eq!(png.format, MediaFormat::Png);
        assert_eq!(png.suggested_name, "plain.png");
        assert_eq!(png.bytes, fs::read(samples().join("plain.png"))?);

        let svg = load_image_payload("plain.svg", &samples(), budget())?;
        assert_eq!(svg.format, MediaFormat::Svg);
        assert_eq!(svg.suggested_name, "plain.svg");
        assert!(
            svg.bytes.starts_with(b"<svg"),
            "SVG 复制使用入口净化后的字节"
        );

        let inline = load_image_payload(PIXEL_PNG, &samples(), budget())?;
        assert_eq!(inline.format, MediaFormat::Png);
        assert_eq!(inline.suggested_name, "image.png");
        assert!(inline.bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

        assert_eq!(
            load_image_payload("missing.png", &samples(), budget()),
            Err(ImagePayloadError::Unavailable)
        );
        assert_eq!(
            load_image_payload("https://example.test/a.png", &samples(), budget()),
            Err(ImagePayloadError::Unavailable)
        );
        let tight = MediaBudget::new(2, u64::MAX, u64::MAX, u32::MAX);
        assert_eq!(
            load_image_payload("plain.png", &samples(), tight),
            Err(ImagePayloadError::Unavailable)
        );
        Ok(())
    }
}
