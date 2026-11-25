//! Markdown 命令的原生入口和显式差异确认。

use aline_app_core::Translator;
use aline_buffer::{SrcSelection, TextBudget};
use aline_edit::{
    BlockCommand, EditBudget, EditCommand, EditContext, EditRefusal, InlineFormat, ParagraphStyle,
    PlanOutcome, plan_block_command, plan_change, plan_markdown_format,
};
use aline_markdown::Parsed;
use gpui::{Context, MenuItem, MouseButton, Role, SharedString, Window, div, prelude::*, px, rgb};

use super::{NativeDocumentWindow, tokens};
use crate::{CommitOutcome, DocError, document::PreparedCommand};

macro_rules! format_actions {
    ($( $action:ident, $handler:ident => ($label:literal, $command:expr) ),+ $(,)?) => {
        gpui::actions!(m3_document, [$($action),+]);

        pub(super) fn menu_items(copy: &MarkdownCommandCopy, enabled: bool, table_enabled: bool) -> Vec<MenuItem> {
            let mut items = Vec::new();
            let mut labels = copy.actions.iter();
            $(
                if let Some(label) = labels.next() {
                    items.push(MenuItem::action(label.clone(), $action).disabled(!enabled));
                }
            )+
            items.push(super::table_commands::table_menu_item(copy.table.clone(), table_enabled));
            items
        }

        impl NativeDocumentWindow {
            $(
                pub(super) fn $handler(&mut self, _: &$action, window: &mut Window, cx: &mut Context<Self>) {
                    self.run_markdown_command($command, window, cx);
                }
            )+
        }

        const ACTION_LABELS: &[&str] = &[$($label),+];
    };
}

format_actions! {
    Bold, on_bold => ("action.bold", MarkdownCommand::Inline(InlineFormat::Strong)),
    Italic, on_italic => ("action.italic", MarkdownCommand::Inline(InlineFormat::Emphasis)),
    Strike, on_strike => ("action.strike", MarkdownCommand::Inline(InlineFormat::Strike)),
    InlineCode, on_inline_code => ("action.inline_code", MarkdownCommand::Inline(InlineFormat::Code)),
    Link, on_link => ("action.link", MarkdownCommand::Inline(InlineFormat::Link)),
    Paragraph, on_paragraph => ("action.paragraph", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Paragraph))),
    HeadingOne, on_heading_one => ("action.heading_one", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Heading(1)))),
    HeadingTwo, on_heading_two => ("action.heading_two", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Heading(2)))),
    HeadingThree, on_heading_three => ("action.heading_three", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Heading(3)))),
    HeadingFour, on_heading_four => ("action.heading_four", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Heading(4)))),
    HeadingFive, on_heading_five => ("action.heading_five", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Heading(5)))),
    HeadingSix, on_heading_six => ("action.heading_six", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Heading(6)))),
    Quote, on_quote => ("action.quote", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::Quote))),
    OrderedList, on_ordered_list => ("action.ordered_list", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::OrderedList))),
    UnorderedList, on_unordered_list => ("action.unordered_list", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::UnorderedList))),
    TaskList, on_task_list => ("action.task_list", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::TaskList))),
    CodeBlock, on_code_block => ("action.code_block", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::CodeBlock))),
    MathBlock, on_math_block => ("action.math_block", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::MathBlock))),
    ThematicBreak, on_thematic_break => ("action.thematic_break", MarkdownCommand::Block(BlockCommand::Convert(ParagraphStyle::ThematicBreak))),
}

gpui::actions!(m3_document, [FormatMarkdown]);

pub(super) struct FormatPreview {
    command: PreparedCommand,
    diff: SharedString,
}

pub(super) struct MarkdownCommandCopy {
    pub(super) menu: SharedString,
    pub(super) format: SharedString,
    table: SharedString,
    apply: SharedString,
    cancel: SharedString,
    actions: Vec<SharedString>,
}

impl MarkdownCommandCopy {
    pub(super) fn new(translator: &Translator) -> Result<Self, String> {
        let text = |key| {
            translator
                .t(key)
                .map(SharedString::from)
                .map_err(|error| error.to_string())
        };
        Ok(Self {
            menu: text("menu.format")?,
            format: text("action.format_markdown")?,
            table: text("action.table_operations")?,
            apply: text("action.apply_markdown_format")?,
            cancel: text("action.cancel_markdown_format")?,
            actions: ACTION_LABELS
                .iter()
                .map(|key| text(key))
                .collect::<Result<_, _>>()?,
        })
    }
}

#[derive(Clone, Copy)]
pub(super) enum MarkdownCommand {
    Inline(InlineFormat),
    Block(BlockCommand),
    TaskMarker(usize),
}

impl MarkdownCommand {
    fn plan(
        self,
        parsed: &Parsed,
        selection: &SrcSelection,
        context: &EditContext,
        budget: EditBudget,
    ) -> Result<PlanOutcome, EditRefusal> {
        match self {
            Self::Inline(format) => plan_change(
                parsed.snapshot(),
                parsed,
                selection,
                context,
                &EditCommand::toggle_inline_format(selection.range().byte_range(), format),
                budget,
            ),
            Self::Block(command) => plan_block_command(parsed, selection, context, command, budget),
            Self::TaskMarker(source_byte) => {
                let target = parsed
                    .snapshot()
                    .src_offset(source_byte)
                    .map_err(|_| EditRefusal::InvalidPlan)?;
                aline_edit::plan_task_toggle(parsed, selection, context, &target, budget)
            }
        }
    }
}

impl NativeDocumentWindow {
    pub(super) fn run_markdown_command(
        &mut self,
        command: MarkdownCommand,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.find_contains_focus(window, cx) {
            return;
        }
        self.execute_markdown_command(command, window, cx);
    }

    pub(super) fn execute_markdown_command(
        &mut self,
        command: MarkdownCommand,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.has_unsettled_input() {
            return false;
        }
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let outcome = (|| {
            let document = self
                .state
                .session
                .document_mut()
                .ok_or(DocError::ClosedTarget)?;
            let core = document.edit_core()?;
            core.prepare_command(|parsed, selection, context, budget| {
                command.plan(parsed, selection, context, budget)
            })?
            .map(|prepared| core.publish_command(&prepared, now))
            .transpose()
        })();
        if matches!(
            outcome,
            Err(DocError::Edit(EditRefusal::ListCommandOutsideItem))
        ) && matches!(
            command,
            MarkdownCommand::Block(
                BlockCommand::ListEnter | BlockCommand::IndentList | BlockCommand::OutdentList
            )
        ) {
            return false;
        }
        self.finish_markdown_command(outcome, window, cx);
        true
    }

    pub(super) fn markdown_key(
        &mut self,
        key: &str,
        shift: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let command = match (key, shift) {
            ("enter", false) => BlockCommand::ListEnter,
            ("tab", false) => BlockCommand::IndentList,
            ("tab", true) => BlockCommand::OutdentList,
            _ => return false,
        };
        let Some(document) = self.state.session.document() else {
            return false;
        };
        if !document.is_editable()
            || document.core().mode() != crate::EditorMode::Visual
            || document.core().composition().is_some()
            || !document.core().has_open_block()
        {
            return false;
        }
        if key == "tab" && self.table_tab(shift, window, cx) {
            return true;
        }
        if !self
            .state
            .session
            .document()
            .is_some_and(may_have_list_command)
        {
            return false;
        }
        self.execute_markdown_command(MarkdownCommand::Block(command), window, cx)
    }

    pub(super) fn format_markdown(
        &mut self,
        _: &FormatMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_unsettled_input() || self.find_contains_focus(window, cx) {
            return;
        }
        let prepared = (|| {
            let document = self
                .state
                .session
                .document_mut()
                .ok_or(DocError::ClosedTarget)?;
            let budget = document.text_budget();
            let command =
                document
                    .edit_core()?
                    .prepare_command(|parsed, selection, context, budget| {
                        plan_markdown_format(parsed.snapshot(), parsed, selection, context, budget)
                    })?;
            command
                .map(|command| {
                    patch_diff(command.parsed(), command.plan().batch().patches(), budget)
                        .map(|diff| (command, diff))
                })
                .transpose()
        })();
        let (command, diff) = match prepared {
            Ok(Some(value)) => value,
            Ok(None) => {
                self.format_preview = None;
                self.state.feedback = Some("document.format_unchanged");
                cx.notify();
                return;
            }
            Err(error) => {
                self.finish_markdown_command(Err(error), window, cx);
                return;
            }
        };
        self.format_preview = Some(FormatPreview {
            command,
            diff: diff.into(),
        });
        self.state.feedback = None;
        cx.notify();
    }

    pub(super) fn render_format_preview(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        let preview = self.format_preview.as_ref()?;
        let copy = &self.copy.markdown_commands;
        Some(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .p_2()
                .border_b_1()
                .border_color(rgb(tokens::BORDER))
                .child(div().text_sm().child(copy.format.clone()))
                .child(
                    div()
                        .id("format-markdown-diff")
                        .max_h(px(180.))
                        .overflow_y_scroll()
                        .p_2()
                        .bg(rgb(tokens::CODE_SURFACE))
                        .text_xs()
                        .child(preview.diff.clone()),
                )
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Self::format_preview_button(
                            "apply-markdown-format",
                            copy.apply.clone(),
                            Self::apply_format_preview,
                            cx,
                        ))
                        .child(Self::format_preview_button(
                            "cancel-markdown-format",
                            copy.cancel.clone(),
                            |this, window, cx| {
                                this.dismiss_format_preview(window, cx);
                            },
                            cx,
                        )),
                )
                .into_any_element(),
        )
    }

    fn format_preview_button(
        id: &'static str,
        label: SharedString,
        action: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.clone())
            .focusable()
            .tab_stop(true)
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(tokens::BORDER))
            .focus(|style| style.border_color(rgb(tokens::ACCENT)))
            .hover(|style| style.bg(rgb(tokens::CODE_SURFACE)))
            .cursor_pointer()
            .text_xs()
            .capture_any_mouse_down(|event, window, _| {
                if event.button == MouseButton::Left {
                    window.prevent_default();
                }
            })
            .on_click(cx.listener(move |this, _, window, cx| action(this, window, cx)))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        action(this, window, cx);
                        this.focus.focus(window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(label)
            .into_any_element()
    }

    pub(super) fn dismiss_format_preview(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.format_preview.take().is_none() {
            return false;
        }
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    fn apply_format_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.has_unsettled_input() {
            self.finish_markdown_command(Err(DocError::PendingInput), window, cx);
            return;
        }
        let Some(preview) = self.format_preview.take() else {
            return;
        };
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let outcome = (|| {
            let document = self
                .state
                .session
                .document_mut()
                .ok_or(DocError::ClosedTarget)?;
            if preview.command.document_id() != document.core().document_id() {
                return Err(DocError::StaleResult);
            }
            document
                .edit_core()?
                .publish_command(&preview.command, now)
                .map(Some)
        })();
        if matches!(
            outcome,
            Err(DocError::StaleResult
                | DocError::WrongDocument
                | DocError::ClosedTarget
                | DocError::Edit(EditRefusal::SelectionMismatch))
        ) {
            self.state.feedback = Some("document.format_preview_expired");
            cx.notify();
        } else {
            self.finish_markdown_command(outcome, window, cx);
        }
    }

    pub(super) fn finish_markdown_command(
        &mut self,
        outcome: Result<Option<CommitOutcome>, DocError>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        match outcome {
            Ok(committed) => {
                self.state.feedback = None;
                if matches!(committed, Some(CommitOutcome::Committed { .. })) {
                    self.input_frame = None;
                    self.source_input_site = None;
                    self.visual_input_restore = self
                        .state
                        .session
                        .document()
                        .filter(|doc| doc.core().mode() == crate::EditorMode::Visual)
                        .map(super::edit_actions::VisualInputRestore::new);
                    self.refresh_after_edit(window, cx);
                    self.note_document_activity(window, cx);
                    self.refresh_find(cx);
                }
            }
            Err(error) => self.show_edit_refusal(error),
        }
        cx.notify();
    }
}

// 普通正文的 Enter/Tab 直接留给输入路径。结构候选再由命令规划完整复验。
fn may_have_list_command(document: &crate::ReadonlyDocument) -> bool {
    let core = document.core();
    let Some(range) = core.visual_baseline_extent() else {
        return false;
    };
    if core.open_block_baseline() != Some(document.parsed_identity().working_version().revision()) {
        return false;
    }
    let caret = core.open_block_range().map_or_else(
        || core.current_selection().head(),
        |draft_range| draft_range.start,
    );
    let parsed = document.parsed();
    let blocks = parsed.blocks();
    let block = blocks
        .binary_search_by_key(&range.start, |block| block.range().byte_range().start)
        .ok()
        .and_then(|index| blocks.get(index));
    let cached_list = match block.map(aline_markdown::Block::kind) {
        Some(aline_markdown::BlockKind::List) => true,
        Some(aline_markdown::BlockKind::BlockQuote) => parsed
            .presentation()
            .iter()
            .find(|node| node.source_range().byte_range().start == range.start)
            .is_some_and(|node| list_item_contains(node.children(), caret)),
        _ => false,
    };
    cached_list || core.open_block_draft().is_some_and(could_start_list)
}

fn list_item_contains(nodes: &[aline_markdown::PresentationNode], source_byte: usize) -> bool {
    nodes.iter().any(|node| {
        let range = node.source_range().byte_range();
        range.start <= source_byte
            && source_byte <= range.end
            && (matches!(node.kind(), aline_markdown::PresentationKind::Item)
                || list_item_contains(node.children(), source_byte))
    })
}

fn could_start_list(draft: &str) -> bool {
    draft.lines().any(|line| {
        let line = line.trim_start_matches([' ', '\t', '>']);
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        match line.as_bytes().get(digits..) {
            Some([b'-' | b'+' | b'*'] | [b'-' | b'+' | b'*', b' ' | b'\t', ..]) => digits == 0,
            Some([b'.' | b')'] | [b'.' | b')', b' ' | b'\t', ..]) => (1..=9).contains(&digits),
            _ => false,
        }
    })
}

fn patch_diff(
    parsed: &Parsed,
    patches: &[(std::ops::Range<usize>, String)],
    budget: TextBudget,
) -> Result<String, DocError> {
    let snapshot = parsed.snapshot();
    let source = snapshot.slice(&snapshot.src_range(0..snapshot.len_bytes())?, budget)?;
    let mut diff = String::new();
    let mut remaining = patches.iter().peekable();
    while let Some((range, replacement)) = remaining.next() {
        let prefix = source.get(..range.start).ok_or(DocError::StaleResult)?;
        let start = prefix.rfind('\n').map_or(0, |offset| offset + 1);
        let mut end = preview_line_end(&source, range.end)?;
        let mut next = range.end;
        let mut changed = String::new();
        changed.push_str(
            source
                .get(start..range.start)
                .ok_or(DocError::StaleResult)?,
        );
        changed.push_str(replacement);
        // 同一源码行的首尾补丁在差异中合并，预览仍使用原受检补丁提交。
        while let Some((range, replacement)) = remaining.next_if(|(range, _)| range.start <= end) {
            changed.push_str(source.get(next..range.start).ok_or(DocError::StaleResult)?);
            changed.push_str(replacement);
            next = range.end;
            end = preview_line_end(&source, next)?;
        }
        changed.push_str(source.get(next..end).ok_or(DocError::StaleResult)?);
        let original = source.get(start..end).ok_or(DocError::StaleResult)?;
        for (prefix, fragment) in [("- ", original), ("+ ", changed.as_str())] {
            for line in fragment.split('\n') {
                diff.push_str(prefix);
                diff.push_str(line.strip_suffix('\r').unwrap_or(line));
                diff.push('\n');
            }
        }
    }
    Ok(diff)
}

fn preview_line_end(source: &str, offset: usize) -> Result<usize, DocError> {
    let trailing = source.get(offset..).ok_or(DocError::StaleResult)?;
    Ok(trailing
        .find('\n')
        .map_or(source.len(), |remaining| offset + remaining))
}

/// 合成符号与解析器任务标记必须对应同一个源范围。
pub(super) fn is_task_marker(
    nodes: &[aline_markdown::PresentationNode],
    range: &aline_buffer::SrcRange,
) -> bool {
    nodes.iter().any(|node| {
        (matches!(
            node.kind(),
            aline_markdown::PresentationKind::TaskMarker { .. }
        ) && node.source_range().byte_range().start == range.byte_range().start)
            || is_task_marker(node.children(), range)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aline_buffer::TextSnapshot;
    use aline_markdown::{ParseBudget, ParseConfig, parse};

    #[test]
    fn list_key_gate_resolves_the_block_from_an_interior_caret()
    -> Result<(), Box<dyn std::error::Error>> {
        use crate::native_window::editable_input::tests::session;
        use aline_buffer::HistoryBudget;

        for bare in ["-", "1.", "7)"] {
            assert!(could_start_list(bare));
        }
        for (source, needle, expected) in [
            ("段落正文", "正文", false),
            ("7) 第一项", "第一项", true),
            ("> - 子项", "子项", true),
            ("> 前句\n>\n> - 子项", "前句", false),
        ] {
            let mut session = session(
                source,
                HistoryBudget::new(8, 4096),
                crate::EditorMode::Visual,
            )?;
            let document = session.document_mut().ok_or("应存在文档")?;
            let current = document.current_snapshot()?;
            let caret_byte = source.find(needle).ok_or("缺少定位文字")? + needle.len();
            let caret = current.text().src_offset(caret_byte)?;
            let selection = SrcSelection::new(caret.clone(), caret)?;
            let identity = document.input_identity()?;
            assert!(document.open_visual_at(&identity, &selection)?);
            assert_eq!(may_have_list_command(document), expected, "{source}");
        }
        Ok(())
    }

    #[test]
    fn format_menu_resolves_both_catalogs() -> Result<(), Box<dyn std::error::Error>> {
        for locale in [aline_app_core::Locale::ZhCn, aline_app_core::Locale::En] {
            let translator = Translator::embedded(locale)?;
            let copy = MarkdownCommandCopy::new(&translator)?;
            assert_eq!(copy.actions.len(), ACTION_LABELS.len());
        }
        Ok(())
    }

    #[test]
    fn preview_groups_split_heading_patches_and_preserves_other_lines()
    -> Result<(), Box<dyn std::error::Error>> {
        let budget = TextBudget::new(1024);
        let source = TextSnapshot::new("##  标题  ##\r\n\r\n正文".to_owned(), budget)?;
        let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
        let diff = patch_diff(
            &parsed,
            &[(0..4, "## ".to_owned()), (10..14, String::new())],
            budget,
        )?;
        assert_eq!(diff, "- ##  标题  ##\n+ ## 标题\n");
        Ok(())
    }
}
