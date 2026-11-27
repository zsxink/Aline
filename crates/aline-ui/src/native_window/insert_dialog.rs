//! 链接与图片的插入弹窗：统一 Ctrl/Cmd+K 与右键入口。
//!
//! 弹窗只收集文字与目标，确认时把选区替换为一条 Markdown 构件编辑；取消、
//! 拒绝与文档无关，确认走与其他源码编辑相同的历史节点。

use std::ops::Range;

use aline_app_core::Translator;
use aline_edit::SourceEdit;
use aline_markdown::{InlineMarkupKind, InsertRejection};
use gpui::{
    AnyElement, Context, Entity, FocusHandle, MouseButton, SharedString, Subscription, Window, div,
    prelude::*, px, rgb,
};

use super::text_field::{TextField, TextFieldEvent};
use super::{
    Copy, Cut, NativeDocumentWindow, Paste, PastePlain, Redo, SelectAll, Undo, edit_error_key,
    tokens,
};
use crate::{DocError, InputIdentity};

/// 弹窗种类。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum InsertKind {
    /// 链接 `[文字](目标)`。
    Link,
    /// 图片 `![替代文字](目标)`。
    Image,
}

impl InsertKind {
    /// 对应 `aline-markdown` 的构件种类。
    pub(super) const fn markup_kind(self) -> InlineMarkupKind {
        match self {
            Self::Link => InlineMarkupKind::Link,
            Self::Image => InlineMarkupKind::Image,
        }
    }
}

/// 确认时被替换的源码锚点：选区范围与其在快照中的原文。
#[derive(Clone, Debug)]
pub(super) struct InsertAnchor {
    range: Range<usize>,
    expected: String,
}

impl InsertAnchor {
    /// 记录锚点；`expected` 必须是该范围当前的源码，用于提交前重读校验。
    pub(super) fn new(range: Range<usize>, expected: impl Into<String>) -> Self {
        Self {
            range,
            expected: expected.into(),
        }
    }

    /// 返回锚点原文。
    pub(super) fn expected(&self) -> &str {
        &self.expected
    }
}

/// 把弹窗内容转换为一条源码替换意图。
///
/// 只生成意图，不修改文档；空目标、未许可链接协议与换行按
/// [`InsertRejection`] 拒绝，文案键见 [`InsertRejection::message_key`]。
pub(super) fn insert_edit(
    kind: InsertKind,
    text: &str,
    destination: &str,
    anchor: &InsertAnchor,
) -> Result<SourceEdit, InsertRejection> {
    let markup = aline_markdown::inline_markup(kind.markup_kind(), text, destination)?;
    Ok(SourceEdit::new(
        anchor.range.clone(),
        anchor.expected.clone(),
        markup,
    ))
}

/// 弹窗文案；两种弹窗共用确认/取消，标题与文字字段标签按种类取。
pub(super) struct InsertCopy {
    link_title: SharedString,
    image_title: SharedString,
    text_label: SharedString,
    alt_label: SharedString,
    target_label: SharedString,
    link_text_placeholder: SharedString,
    image_text_placeholder: SharedString,
    link_target_placeholder: SharedString,
    image_target_placeholder: SharedString,
    pub(super) confirm: SharedString,
    pub(super) cancel: SharedString,
}

impl InsertCopy {
    /// 从当前语言目录取全部弹窗文案；缺键即失败。
    pub(super) fn new(translator: &Translator) -> Result<Self, String> {
        let text = |key| {
            translator
                .t(key)
                .map(SharedString::from)
                .map_err(|error| error.to_string())
        };
        Ok(Self {
            link_title: text("insert.link_title")?,
            image_title: text("insert.image_title")?,
            text_label: text("insert.text_label")?,
            alt_label: text("insert.alt_label")?,
            target_label: text("insert.target_label")?,
            link_text_placeholder: text("insert.link_text_placeholder")?,
            image_text_placeholder: text("insert.image_text_placeholder")?,
            link_target_placeholder: text("insert.link_target_placeholder")?,
            image_target_placeholder: text("insert.image_target_placeholder")?,
            confirm: text("insert.confirm")?,
            cancel: text("insert.cancel")?,
        })
    }

    /// 弹窗标题。
    pub(super) fn title(&self, kind: InsertKind) -> &SharedString {
        match kind {
            InsertKind::Link => &self.link_title,
            InsertKind::Image => &self.image_title,
        }
    }

    /// 文字字段标签：链接用「文字」，图片用「替代文字」。
    pub(super) fn text_label(&self, kind: InsertKind) -> &SharedString {
        match kind {
            InsertKind::Link => &self.text_label,
            InsertKind::Image => &self.alt_label,
        }
    }

    /// 文字字段占位提示。
    pub(super) fn text_placeholder(&self, kind: InsertKind) -> &SharedString {
        match kind {
            InsertKind::Link => &self.link_text_placeholder,
            InsertKind::Image => &self.image_text_placeholder,
        }
    }

    /// 目标字段占位提示。
    pub(super) fn target_placeholder(&self, kind: InsertKind) -> &SharedString {
        match kind {
            InsertKind::Link => &self.link_target_placeholder,
            InsertKind::Image => &self.image_target_placeholder,
        }
    }

    /// 目标字段标签。
    pub(super) fn target_label(&self) -> &SharedString {
        &self.target_label
    }
}

/// 把选区原文预填进文字字段。
///
/// 只接受单行、非空且不超过字段上限的文字；多行、空白或超长选区留空，
/// 避免把块级内容塞进单行标签。
pub(super) fn prefill_text(source: &str) -> String {
    let trimmed = source.trim();
    if trimmed.is_empty()
        || trimmed.len() > super::text_field::MAX_FIELD_BYTES
        || trimmed.contains(['\r', '\n', '\0'])
    {
        return String::new();
    }
    trimmed.to_owned()
}

/// 字段宽度；面板留出内边距。
const FIELD_WIDTH: f32 = 320.0;

/// 打开中的链接/图片弹窗。
///
/// 锚点记录打开瞬间的源码区间与原文，确认时再由受检编辑路径复验；
/// 弹窗自身不改文档，取消只丢弃状态。
pub(super) struct InsertDialog {
    kind: InsertKind,
    anchor: InsertAnchor,
    identity: InputIdentity,
    text_field: Entity<TextField>,
    target_field: Entity<TextField>,
    confirm_focus: FocusHandle,
    cancel_focus: FocusHandle,
    error: Option<&'static str>,
    /// 字段事件订阅；随弹窗一起释放。
    _subscriptions: Vec<Subscription>,
}

impl NativeDocumentWindow {
    /// 打开弹窗；锚点取当前选区，文字字段预填单行选区。
    pub(super) fn open_insert_dialog(
        &mut self,
        kind: InsertKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.insert_dialog.is_some() || self.find_contains_focus(window, cx) {
            return;
        }
        if self.has_unsettled_input() {
            self.state.feedback = Some("document.edit_refused");
            cx.notify();
            return;
        }
        let prepared = (|| {
            let document = self
                .state
                .session
                .document_mut()
                .ok_or(DocError::ClosedTarget)?;
            if !document.is_editable() {
                return Err(DocError::ReadOnly);
            }
            if document.core().composition().is_some() {
                return Err(DocError::CompositionActive);
            }
            let identity = document.input_identity()?;
            let budget = document.text_budget();
            let snapshot = document.current_snapshot()?;
            let selection = snapshot.selection().range();
            let expected = snapshot.text().slice(&selection, budget)?.into_owned();
            Ok::<_, DocError>((
                identity,
                InsertAnchor::new(selection.byte_range(), expected),
            ))
        })();
        let (identity, anchor) = match prepared {
            Ok(value) => value,
            Err(error) => {
                self.state.feedback = Some(edit_error_key(error));
                cx.notify();
                return;
            }
        };
        let text_placeholder = self.copy.insert.text_placeholder(kind).clone();
        let target_placeholder = self.copy.insert.target_placeholder(kind).clone();
        let prefill = SharedString::from(prefill_text(anchor.expected()));
        let text_field = cx.new(|cx| {
            TextField::new(
                "insert-text",
                text_placeholder,
                px(FIELD_WIDTH),
                prefill,
                cx,
            )
        });
        let target_field = cx.new(|cx| {
            TextField::new(
                "insert-target",
                target_placeholder,
                px(FIELD_WIDTH),
                SharedString::default(),
                cx,
            )
        });
        let subscriptions = vec![
            cx.subscribe_in(&text_field, window, Self::handle_insert_field_event),
            cx.subscribe_in(&target_field, window, Self::handle_insert_field_event),
        ];
        self.insert_dialog = Some(InsertDialog {
            kind,
            anchor,
            identity,
            text_field: text_field.clone(),
            target_field,
            confirm_focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            error: None,
            _subscriptions: subscriptions,
        });
        self.state.feedback = None;
        text_field.update(cx, |field, cx| field.focus.focus(window, cx));
        cx.notify();
    }

    fn handle_insert_field_event(
        &mut self,
        _sender: &Entity<TextField>,
        event: &TextFieldEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.insert_dialog.is_none() {
            return;
        }
        match *event {
            TextFieldEvent::Submit { .. } => self.confirm_insert(window, cx),
            TextFieldEvent::Cancel => {
                self.dismiss_insert_dialog(window, cx);
            }
            TextFieldEvent::Changed | TextFieldEvent::Composition => {}
        }
    }

    /// 文档被替换或关闭后丢弃弹窗；返回是否确实丢弃。
    pub(super) fn prune_insert_dialog(&mut self) -> bool {
        let Some(dialog) = self.insert_dialog.as_ref() else {
            return false;
        };
        let stale =
            self.state.session.document().is_none_or(|document| {
                dialog.identity.document_id() != document.core().document_id()
            });
        if stale {
            self.insert_dialog = None;
        }
        stale
    }

    /// 关闭弹窗；返回是否确有弹窗被关闭。
    pub(super) fn dismiss_insert_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.insert_dialog.take().is_none() {
            return false;
        }
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    fn cancel_insert_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.dismiss_insert_dialog(window, cx);
    }

    /// 确认：把锚点替换为 Markdown 构件，并与其他编辑共用历史。
    fn confirm_insert(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.insert_dialog.as_ref() else {
            return;
        };
        let kind = dialog.kind;
        let anchor = dialog.anchor.clone();
        let identity = dialog.identity.clone();
        if dialog.text_field.read(cx).is_composing() || dialog.target_field.read(cx).is_composing()
        {
            return;
        }
        let text = dialog.text_field.read(cx).committed().to_string();
        let destination = dialog.target_field.read(cx).committed().to_string();
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let error = if self.has_unsettled_input() {
            Some("document.edit_refused")
        } else {
            match insert_edit(kind, &text, &destination, &anchor) {
                Err(rejection) => Some(rejection.message_key()),
                Ok(edit) => {
                    let outcome = (|| {
                        let document = self
                            .state
                            .session
                            .document_mut()
                            .ok_or(DocError::ClosedTarget)?;
                        let current = document.input_identity()?;
                        if identity.document_id() != current.document_id() {
                            return Err(DocError::WrongDocument);
                        }
                        document.commit_source_edit(&current, edit, now)
                    })();
                    match outcome {
                        Ok(outcome) => {
                            self.insert_dialog = None;
                            self.focus.focus(window, cx);
                            self.finish_markdown_command(Ok(Some(outcome)), window, cx);
                            return;
                        }
                        Err(error) => Some(edit_error_key(error)),
                    }
                }
            }
        };
        if let Some(dialog) = self.insert_dialog.as_mut() {
            dialog.error = error;
        }
        cx.notify();
    }

    /// Tab/Shift+Tab：文字 → 目标 → 确认 → 取消 循环。
    pub(super) fn move_insert_dialog_focus(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(dialog) = self.insert_dialog.as_ref() else {
            return false;
        };
        let handles = [
            dialog.text_field.read(cx).focus.clone(),
            dialog.target_field.read(cx).focus.clone(),
            dialog.confirm_focus.clone(),
            dialog.cancel_focus.clone(),
        ];
        let Some(current) = handles.iter().position(|handle| handle.is_focused(window)) else {
            return false;
        };
        let count = handles.len();
        let next = if backward {
            (current + count - 1) % count
        } else {
            (current + 1) % count
        };
        handles[next].focus(window, cx);
        cx.notify();
        true
    }

    pub(super) fn render_insert_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.insert_dialog.as_ref()?;
        let copy = &self.copy.insert;
        let label = |text: &SharedString| {
            div()
                .text_xs()
                .text_color(rgb(tokens::SECONDARY))
                .child(text.clone())
        };
        let panel = div()
            .id("insert-dialog")
            .occlude()
            .flex()
            .flex_col()
            .gap_3()
            .w(px(FIELD_WIDTH + 64.0))
            .p_4()
            .rounded_lg()
            .border_1()
            .border_color(rgb(tokens::BORDER))
            .bg(rgb(tokens::SURFACE))
            .shadow_md()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "escape" {
                    this.dismiss_insert_dialog(window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_action(cx.listener(|_, _: &Copy, _, cx| cx.stop_propagation()))
            .on_action(cx.listener(|_, _: &Cut, _, cx| cx.stop_propagation()))
            .on_action(cx.listener(|_, _: &Paste, _, cx| cx.stop_propagation()))
            .on_action(cx.listener(|_, _: &PastePlain, _, cx| cx.stop_propagation()))
            .on_action(cx.listener(|_, _: &SelectAll, _, cx| cx.stop_propagation()))
            .on_action(cx.listener(|_, _: &Undo, _, cx| cx.stop_propagation()))
            .on_action(cx.listener(|_, _: &Redo, _, cx| cx.stop_propagation()))
            .child(div().text_lg().child(copy.title(dialog.kind).clone()))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(copy.text_label(dialog.kind)))
                    .child(dialog.text_field.clone()),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(label(copy.target_label()))
                    .child(dialog.target_field.clone()),
            )
            .when_some(
                dialog
                    .error
                    .and_then(|key| self.copy.errors.get(key).cloned()),
                |panel, message| {
                    panel.child(
                        div()
                            .text_sm()
                            .text_color(rgb(tokens::ALERT_CAUTION))
                            .child(message),
                    )
                },
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap_2()
                    .child(Self::insert_dialog_button(
                        "insert-cancel",
                        copy.cancel.clone(),
                        &dialog.cancel_focus,
                        false,
                        Self::cancel_insert_dialog,
                        cx,
                    ))
                    .child(Self::insert_dialog_button(
                        "insert-confirm",
                        copy.confirm.clone(),
                        &dialog.confirm_focus,
                        true,
                        Self::confirm_insert,
                        cx,
                    )),
            );
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    div()
                        .id("insert-dialog-scrim")
                        .absolute()
                        .inset_0()
                        .occlude()
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, window, cx| {
                                this.dismiss_insert_dialog(window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, _, window, cx| {
                                this.dismiss_insert_dialog(window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(panel),
                )
                .into_any_element(),
        )
    }

    fn insert_dialog_button(
        id: &'static str,
        label: SharedString,
        focus: &FocusHandle,
        primary: bool,
        action: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Button)
            .aria_label(label.clone())
            .track_focus(focus)
            .focusable()
            .tab_stop(true)
            .cursor_pointer()
            .px_3()
            .py_1()
            .rounded_md()
            .border_1()
            .border_color(rgb(tokens::BORDER))
            .when(primary, |button| {
                button
                    .bg(gpui::rgba(tokens::ACCENT_SOFT))
                    .text_color(rgb(tokens::ACCENT))
            })
            .focus(|style| style.border_color(rgb(tokens::ACCENT)))
            .hover(|style| style.bg(rgb(tokens::CODE_SURFACE)))
            .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        action(this, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(label)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aline_buffer::{HistoryBudget, SrcSelection};

    use crate::EditorMode;
    use crate::native_window::editable_input::tests::session;

    fn anchor(range: Range<usize>, expected: &str) -> InsertAnchor {
        InsertAnchor::new(range, expected)
    }

    fn replacement(edit: &SourceEdit) -> &str {
        edit.replacement()
    }

    /// 取出构造成功的编辑；被拒绝即视为用例失败。
    fn accepted(
        result: Result<SourceEdit, InsertRejection>,
    ) -> Result<SourceEdit, Box<dyn std::error::Error>> {
        match result {
            Ok(edit) => Ok(edit),
            Err(rejection) => Err(format!("预期构造成立，实际被拒绝：{rejection:?}").into()),
        }
    }

    /// 取出被拒绝的原因；构造成功即视为用例失败。
    fn refusal(
        result: Result<SourceEdit, InsertRejection>,
    ) -> Result<InsertRejection, Box<dyn std::error::Error>> {
        match result {
            Ok(_) => Err("预期拒绝，但构造成立".into()),
            Err(rejection) => Ok(rejection),
        }
    }

    #[test]
    fn link_edit_replaces_the_anchor_with_escaped_markup() -> Result<(), Box<dyn std::error::Error>>
    {
        let edit = accepted(insert_edit(
            InsertKind::Link,
            "链接",
            "https://example.test",
            &anchor(5..5, ""),
        ))?;
        assert_eq!(edit.range(), &(5..5));
        assert_eq!(edit.expected(), "");
        assert_eq!(replacement(&edit), "[链接](https://example.test)");

        let edit = accepted(insert_edit(
            InsertKind::Link,
            "",
            "docs/a b.md",
            &anchor(0..3, "旧文"),
        ))?;
        assert_eq!(edit.range(), &(0..3));
        assert_eq!(edit.expected(), "旧文");
        assert_eq!(replacement(&edit), "[docs/a b.md](<docs/a b.md>)");
        Ok(())
    }

    #[test]
    fn image_edit_keeps_the_alt_fallback_and_skips_link_scheme_rules()
    -> Result<(), Box<dyn std::error::Error>> {
        let edit = accepted(insert_edit(
            InsertKind::Image,
            "",
            "images/a.png",
            &anchor(2..4, "ab"),
        ))?;
        assert_eq!(edit.expected(), "ab");
        assert_eq!(replacement(&edit), "![images/a.png](images/a.png)");

        let edit = accepted(insert_edit(
            InsertKind::Image,
            "示意",
            "custom:scheme.png",
            &anchor(0..0, ""),
        ))?;
        assert_eq!(InsertKind::Image.markup_kind(), InlineMarkupKind::Image);
        assert_eq!(InsertKind::Link.markup_kind(), InlineMarkupKind::Link);
        assert_eq!(replacement(&edit), "![示意](custom:scheme.png)");
        Ok(())
    }

    #[test]
    fn rejections_expose_the_frozen_message_keys() -> Result<(), Box<dyn std::error::Error>> {
        let empty = refusal(insert_edit(
            InsertKind::Link,
            "文字",
            "  ",
            &anchor(0..0, ""),
        ))?;
        assert_eq!(empty, InsertRejection::EmptyDestination);
        assert_eq!(empty.message_key(), "insert.rejected.empty");

        let scheme = refusal(insert_edit(
            InsertKind::Link,
            "文字",
            "javascript:alert(1)",
            &anchor(0..0, ""),
        ))?;
        assert_eq!(scheme, InsertRejection::UnsupportedScheme);
        assert_eq!(scheme.message_key(), "insert.rejected.scheme");

        let line_break = refusal(insert_edit(
            InsertKind::Link,
            "文字",
            "a\nb",
            &anchor(0..0, ""),
        ))?;
        assert_eq!(line_break, InsertRejection::LineBreak);
        assert_eq!(line_break.message_key(), "insert.rejected.line_break");
        Ok(())
    }

    #[test]
    fn confirmed_insert_shares_undo_history_with_typed_edits()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut state = session("abc def", HistoryBudget::new(8, 4096), EditorMode::Visual)?;
        let document = state.document_mut().ok_or("没有文档")?;
        {
            let identity = document.input_identity()?;
            let text = document.current_snapshot()?.text().clone();
            let core = document.edit_core()?;
            let start = text.src_offset(4)?;
            let end = text.src_offset(7)?;
            let selection = SrcSelection::new(start.clone(), end)?;
            core.select(&identity, &selection)?;
            core.type_selection(&identity, "X", 1)?;
            core.commit_open_block(2)?;
            assert_eq!(core.buffer_text()?, "abc X");
            assert_eq!(core.history_node_count(), 1);
        }

        let edit = accepted(insert_edit(
            InsertKind::Link,
            "链接",
            "https://example.test",
            &InsertAnchor::new(5..5, ""),
        ))?;
        let identity = document.input_identity()?;
        let outcome = document.commit_source_edit(&identity, edit, 3)?;
        assert!(matches!(outcome, crate::CommitOutcome::Committed { .. }));

        let core = document.edit_core()?;
        assert_eq!(core.buffer_text()?, "abc X[链接](https://example.test)");
        assert_eq!(core.history_node_count(), 2);

        core.undo()?;
        assert_eq!(core.buffer_text()?, "abc X");
        core.undo()?;
        assert_eq!(core.buffer_text()?, "abc def");
        Ok(())
    }

    #[test]
    fn rejected_insert_leaves_the_document_and_history_unchanged()
    -> Result<(), Box<dyn std::error::Error>> {
        let mut state = session("abc def", HistoryBudget::new(8, 4096), EditorMode::Visual)?;
        let document = state.document_mut().ok_or("没有文档")?;
        let core = document.edit_core()?;
        let rejection = refusal(insert_edit(
            InsertKind::Link,
            "文字",
            "javascript:alert(1)",
            &InsertAnchor::new(0..3, "abc"),
        ))?;
        assert_eq!(rejection.message_key(), "insert.rejected.scheme");
        assert_eq!(core.buffer_text()?, "abc def");
        assert_eq!(core.history_node_count(), 0);
        Ok(())
    }

    #[test]
    fn insert_copy_resolves_both_catalogs_and_kind_labels() -> Result<(), Box<dyn std::error::Error>>
    {
        for locale in [aline_app_core::Locale::ZhCn, aline_app_core::Locale::En] {
            let translator = Translator::embedded(locale)?;
            let copy = InsertCopy::new(&translator)?;
            assert!(!copy.title(InsertKind::Link).is_empty());
            assert!(!copy.title(InsertKind::Image).is_empty());
            assert_ne!(copy.title(InsertKind::Link), copy.title(InsertKind::Image));
            assert_ne!(
                copy.text_label(InsertKind::Link),
                copy.text_label(InsertKind::Image)
            );
            assert_ne!(
                copy.text_placeholder(InsertKind::Link),
                copy.text_placeholder(InsertKind::Image)
            );
            assert_ne!(
                copy.target_placeholder(InsertKind::Link),
                copy.target_placeholder(InsertKind::Image)
            );
            assert!(!copy.target_label().is_empty());
            assert!(!copy.confirm.is_empty());
            assert!(!copy.cancel.is_empty());
        }
        Ok(())
    }

    #[test]
    fn prefill_keeps_single_line_text_only() {
        assert_eq!(prefill_text("  标题  "), "标题");
        assert_eq!(prefill_text("第一行\n第二行"), "");
        assert_eq!(prefill_text("  \t "), "");
        assert_eq!(
            prefill_text(&"a".repeat(crate::native_window::text_field::MAX_FIELD_BYTES + 1)),
            ""
        );
        assert_eq!(prefill_text("链接 文字"), "链接 文字");
    }
}
