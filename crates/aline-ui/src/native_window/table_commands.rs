//! 单元格目标绑定、表格结构入口及仅移动选区的导航。

use aline_buffer::{SrcOffset, SrcSelection, TextBudget};
use aline_edit::{
    TableCommand, TableNavigation, TableOperation, TableTarget, plan_table_command,
    table_navigation,
};
use aline_markdown::{
    Parsed, PresentationKind, PresentationNode, ProjectionId, TableAlignment, table_source,
};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    ClipboardItem, Context, FocusHandle, InteractiveElement, IntoElement, MenuItem, MouseButton,
    ParentElement, Pixels, Point, ScrollHandle, SharedString, StatefulInteractiveElement, Styled,
    Window, div, point, px, rgb,
};

use super::NativeDocumentWindow;
use super::editable_input::{InputFrame, SourceInputSite};
use super::readonly_layout::LayoutToken;
use super::tokens;
use crate::{CurrentSnapshot, DocError, EditorMode, InputIdentity, ReadonlyDocument};

gpui::actions!(m3_document, [TableOperations]);

pub(super) struct TableMenu {
    binding: TableMenuBinding,
    position: Point<Pixels>,
    items: Vec<(&'static str, MenuCommand)>,
    focus: FocusHandle,
    active: usize,
    scroll: ScrollHandle,
}

struct TableMenuBinding {
    target: TableTarget,
    identity: InputIdentity,
    projection: ProjectionId,
    layout: Option<LayoutToken>,
}

impl TableMenuBinding {
    fn validate(
        &self,
        document: &mut crate::ReadonlyDocument,
        layout: Option<LayoutToken>,
    ) -> Result<(), DocError> {
        if document.core().composition().is_some() {
            return Err(DocError::CompositionActive);
        }
        self.identity
            .validate(&document.input_identity()?)
            .map_err(|_| DocError::StaleResult)?;
        if &self.projection != document.parsed().projection().id() || self.layout != layout {
            return Err(DocError::StaleResult);
        }
        Ok(())
    }

    fn copy_markdown(&self, document: &mut crate::ReadonlyDocument) -> Result<String, DocError> {
        let budget = document.text_budget();
        document.with_working_parse(|parsed, _, _| {
            let table = table_source(parsed, self.target.table_range(), budget)
                .map_err(|_| DocError::StaleResult)?;
            let rows = table.rows();
            let mut text = String::new();
            for row in rows
                .iter()
                .take(1)
                .chain([table.separator()])
                .chain(rows.iter().skip(1))
            {
                text.push_str(&parsed.snapshot().slice(row.source_range(), budget)?);
                text.push_str(row.line_ending());
            }
            Ok(text)
        })
    }
}

#[derive(Clone, Copy)]
enum MenuCommand {
    Copy,
    Edit(TableOperation),
}

impl NativeDocumentWindow {
    pub(super) fn has_table_menu_target(&mut self) -> bool {
        if self.has_unsettled_input() {
            return false;
        }
        let Some(document) = self
            .state
            .session
            .document_mut()
            .filter(|document| table_editable(document))
        else {
            return false;
        };
        let Ok(current) = document.current_snapshot() else {
            return false;
        };
        let Ok(identity) = document.input_identity() else {
            return false;
        };
        navigation_target(
            document,
            &current,
            &identity,
            self.source_input_site.as_ref(),
        )
        .is_some()
    }

    fn selected_table_menu_target(&mut self) -> Option<TableTarget> {
        if self.has_unsettled_input() {
            return None;
        }
        let document = self.state.session.document_mut()?;
        if !table_editable(document) {
            return None;
        }
        let active = self.source_input_site.as_ref();
        let budget = document.text_budget();
        document
            .with_working_parse(|parsed, current, identity| {
                Ok(current_table_target(
                    parsed, current, identity, active, budget,
                ))
            })
            .ok()
            .flatten()
    }

    pub(super) fn show_table_operations(
        &mut self,
        _: &TableOperations,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(target) = self.selected_table_menu_target() {
            self.open_table_menu(target, point(px(16.0), px(48.0)), window, cx);
        } else {
            self.state.feedback = Some("document.edit_refused");
            cx.notify();
        }
    }

    pub(super) fn open_table_menu(
        &mut self,
        target: TableTarget,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_bound_table_menu(target, point, window, cx);
    }

    pub(super) fn open_active_table_menu(
        &mut self,
        frame: &InputFrame,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        let Some(site) = frame.source_site() else {
            return;
        };
        if !self
            .input_frame
            .as_ref()
            .is_some_and(|current| current.is_same(frame))
        {
            return;
        }
        let budget = document.text_budget();
        let target = document.with_working_parse(|parsed, current, identity| {
            Ok(site
                .active_table_target(identity, current)
                .and_then(|target| rebind_table_target(parsed, target, budget)))
        });
        if let Ok(Some(target)) = target {
            self.open_bound_table_menu(target, point, window, cx);
        }
    }

    fn open_bound_table_menu(
        &mut self,
        target: TableTarget,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.has_unsettled_input() {
            return;
        }
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        if !document.is_editable()
            || document.core().mode() != EditorMode::Visual
            || document.core().composition().is_some()
        {
            return;
        }
        let budget = document.text_budget();
        let Ok((rows, columns, complete, identity)) =
            document.with_working_parse(|parsed, _, identity| {
                let table = table_source(parsed, target.table_range(), budget)
                    .map_err(|_| DocError::StaleResult)?;
                Ok((
                    table.rows().len(),
                    table.alignments().len(),
                    table
                        .rows()
                        .iter()
                        .all(|row| row.cells().iter().all(|cell| cell.is_present())),
                    identity.clone(),
                ))
            })
        else {
            self.state.feedback = Some("document.edit_refused");
            cx.notify();
            return;
        };
        let row = target.row();
        let column = target.column();
        if row >= rows || column >= columns {
            return;
        }
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.table_menu = Some(TableMenu {
            binding: TableMenuBinding {
                target,
                identity,
                projection: document.parsed().projection().id().clone(),
                layout: self
                    .reader
                    .as_ref()
                    .and_then(|reader| reader.layout_token()),
            },
            position: point,
            items: table_items(row, column, rows, columns, complete),
            focus,
            active: 0,
            scroll: ScrollHandle::new(),
        });
        cx.notify();
    }

    pub(super) fn dismiss_table_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.table_menu.take().is_none() {
            return false;
        }
        self.focus.focus(window, cx);
        cx.notify();
        true
    }

    fn visible_table_menu(&self) -> Option<&TableMenu> {
        let menu = self.table_menu.as_ref()?;
        let document = self.state.session.document()?;
        if menu.binding.identity.document_id() != document.core().document_id()
            || menu.binding.identity.working_version() != document.core().version()
            || &menu.binding.projection != document.parsed().projection().id()
            || menu.binding.layout
                != self
                    .reader
                    .as_ref()
                    .and_then(|reader| reader.layout_token())
            || document.core().mode() != EditorMode::Visual
        {
            return None;
        }
        Some(menu)
    }

    pub(super) fn render_table_menu(
        &self,
        window: &Window,
        cx: &Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let menu = self.visible_table_menu()?;
        let panel_width = px(206.);
        let width = window.viewport_size().width;
        let height = window.viewport_size().height;
        let top = menu.position.y.min((height - px(220.)).max(px(0.)));
        let panel = div()
            .id("table-operations")
            .track_focus(&menu.focus)
            .absolute()
            .left(menu.position.x.min((width - panel_width).max(px(0.))))
            .top(top)
            .w(panel_width)
            .max_h((height - top).max(px(0.)))
            .overflow_y_scroll()
            .track_scroll(&menu.scroll)
            .occlude()
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(rgb(tokens::BORDER))
            .bg(rgb(tokens::SURFACE))
            .p_1()
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                this.table_menu_key(event, window, cx);
            }))
            .children(
                menu.items
                    .iter()
                    .enumerate()
                    .filter_map(|(index, (key, command))| {
                        let label = self.copy.translator.t(key).ok()?;
                        let command = *command;
                        let listener = cx.listener(move |this, _, window, cx| {
                            this.run_table_menu_command(command, window, cx);
                            cx.stop_propagation();
                        });
                        Some(
                            div()
                                .id(*key)
                                .px_2()
                                .py_1()
                                .when(menu.active == index, |item| {
                                    item.bg(rgb(tokens::CODE_SURFACE))
                                })
                                .hover(|style| style.bg(rgb(tokens::CODE_SURFACE)))
                                .child(SharedString::from(label))
                                .on_click(listener),
                        )
                    }),
            );
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
                                this.dismiss_table_menu(window, cx);
                                cx.stop_propagation();
                            }),
                        )
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(|this, _, window, cx| {
                                this.dismiss_table_menu(window, cx);
                                cx.stop_propagation();
                            }),
                        ),
                )
                .child(panel)
                .into_any_element(),
        )
    }

    pub(super) fn move_table_menu_focus(
        &mut self,
        backward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self
            .table_menu
            .as_ref()
            .is_some_and(|menu| menu.focus.is_focused(window))
        {
            return false;
        }
        if self.visible_table_menu().is_none() {
            return self.dismiss_table_menu(window, cx);
        }
        let Some(menu) = self.table_menu.as_mut() else {
            return false;
        };
        let count = menu.items.len();
        menu.active = if backward {
            (menu.active + count - 1) % count
        } else {
            (menu.active + 1) % count
        };
        menu.scroll.scroll_to_item(menu.active);
        cx.notify();
        true
    }

    fn table_menu_key(
        &mut self,
        event: &gpui::KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.table_menu.as_mut() else {
            return;
        };
        match event.keystroke.key.as_str() {
            "up" | "down" | "tab" => {
                let backward = event.keystroke.key == "up"
                    || event.keystroke.key == "tab" && event.keystroke.modifiers.shift;
                self.move_table_menu_focus(backward, window, cx);
            }
            "enter" | "space" => {
                if let Some((_, command)) = menu.items.get(menu.active).copied() {
                    self.run_table_menu_command(command, window, cx);
                }
            }
            "escape" => {
                self.dismiss_table_menu(window, cx);
            }
            _ => return,
        }
        window.prevent_default();
        cx.stop_propagation();
    }

    fn run_table_menu_command(
        &mut self,
        operation: MenuCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = self.table_menu.take() else {
            return;
        };
        self.focus.focus(window, cx);
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let layout = self
            .reader
            .as_ref()
            .and_then(|reader| reader.layout_token());
        let outcome = (|| {
            let document = self
                .state
                .session
                .document_mut()
                .ok_or(DocError::ClosedTarget)?;
            menu.binding.validate(document, layout)?;
            match operation {
                MenuCommand::Copy => {
                    let text = menu.binding.copy_markdown(document)?;
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    Ok(None)
                }
                MenuCommand::Edit(operation) => {
                    let core = document.edit_core()?;
                    let command = TableCommand::new(menu.binding.target, operation);
                    core.prepare_command(|parsed, selection, context, budget| {
                        plan_table_command(parsed, selection, context, &command, budget)
                    })?
                    .map(|prepared| core.publish_command(&prepared, now))
                    .transpose()
                }
            }
        })();
        self.finish_markdown_command(outcome, window, cx);
    }

    pub(super) fn table_tab(
        &mut self,
        backward: bool,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.has_unsettled_input() {
            return false;
        }
        let Some(document) = self.state.session.document_mut() else {
            return false;
        };
        let core = document.core();
        if core.mode() != EditorMode::Visual || core.composition().is_some() {
            return false;
        }
        if !document.is_parse_current()
            && !self
                .source_input_site
                .as_ref()
                .is_some_and(SourceInputSite::is_table)
        {
            return false;
        }
        let result = plan_tab(document, self.source_input_site.as_ref(), backward);
        if matches!(result, Ok(None)) {
            return false;
        }
        let now = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let mut next_site = None;
        let outcome = (|| match result?.ok_or(DocError::StaleResult)? {
            PreparedTableTab::Boundary => Ok(None),
            PreparedTableTab::Cell { selection, site } => {
                let identity = document.input_identity()?;
                document.edit_core()?.select(&identity, &selection)?;
                next_site = Some(site);
                Ok(None)
            }
            PreparedTableTab::AppendRow(command) => {
                let core = document.edit_core()?;
                core.prepare_command(|parsed, selection, context, budget| {
                    let target = rebind_table_target(parsed, command.target(), budget.text())
                        .ok_or(aline_edit::EditRefusal::InvalidPlan)?;
                    let command = TableCommand::new(target, command.operation());
                    plan_table_command(parsed, selection, context, &command, budget)
                })?
                .map(|prepared| core.publish_command(&prepared, now))
                .transpose()
            }
        })();
        if let Some(site) = next_site {
            self.replace_table_site(site);
            self.update_menu(cx);
        }
        self.finish_markdown_command(outcome, window, cx);
        true
    }

    pub(super) fn replace_table_site(&mut self, site: SourceInputSite) {
        if let Some(frame) = self.input_frame.take() {
            frame.clear();
        }
        self.source_input_site = Some(site);
    }
}

enum PreparedTableTab {
    Cell {
        selection: SrcSelection,
        site: SourceInputSite,
    },
    AppendRow(TableCommand),
    Boundary,
}

fn table_editable(document: &ReadonlyDocument) -> bool {
    document.is_editable()
        && document.core().mode() == EditorMode::Visual
        && document.core().composition().is_none()
}

fn plan_tab(
    document: &mut ReadonlyDocument,
    active: Option<&SourceInputSite>,
    backward: bool,
) -> Result<Option<PreparedTableTab>, DocError> {
    let current = document.current_snapshot()?;
    let identity = document.input_identity()?;
    let Some(target) = navigation_target(document, &current, &identity, active) else {
        // 输入站点有效但阅读布局正在更换时，不能把表格 Tab 交给外层列表处理。
        if active.is_some_and(|site| site.active_table_target(&identity, &current).is_some()) {
            return Err(DocError::StaleResult);
        }
        return Ok(None);
    };
    let parsed = document.parsed();
    let budget = document.text_budget();
    // 已采用的表结构提供格身份和原范围。草稿仅借 DocCore 的单区间映射绑定目标格。
    let navigation = table_navigation(parsed, &target, backward, budget).map_err(DocError::Edit)?;
    Ok(Some(match navigation {
        TableNavigation::Boundary => PreparedTableTab::Boundary,
        TableNavigation::AppendRow(command) => PreparedTableTab::AppendRow(command),
        TableNavigation::Cell { target, .. } => {
            let reader_row = active
                .filter(|site| site.active_table_target(&identity, &current).is_some())
                .map_or_else(
                    || target.table_range().clone(),
                    |site| site.reader_row().clone(),
                );
            let (selection, site) = SourceInputSite::for_table_navigation(
                document, &current, &identity, &target, reader_row,
            )?;
            PreparedTableTab::Cell { selection, site }
        }
    }))
}

fn navigation_target(
    document: &ReadonlyDocument,
    current: &CurrentSnapshot,
    identity: &InputIdentity,
    active: Option<&SourceInputSite>,
) -> Option<TableTarget> {
    let parsed = document.parsed();
    let budget = document.text_budget();
    if let Some(target) = active.and_then(|site| site.active_table_target(identity, current))
        && target.table_range().snapshot_id() == parsed.snapshot().id()
        && document
            .core()
            .project_visual_source_range(
                current,
                target.table_range(),
                aline_buffer::Affinity::Before,
            )
            .is_ok()
    {
        return Some(target.clone());
    }
    if document.is_parse_current() {
        return table_target_at(parsed, current.selection().head(), budget);
    }
    None
}

fn current_table_target(
    parsed: &Parsed,
    current: &CurrentSnapshot,
    identity: &InputIdentity,
    active: Option<&SourceInputSite>,
    budget: TextBudget,
) -> Option<TableTarget> {
    if let Some(target) = active.and_then(|site| site.active_table_target(identity, current)) {
        return rebind_table_target(parsed, target, budget);
    }
    table_target_at(parsed, current.selection().head(), budget)
}

pub(super) fn rebind_table_target(
    parsed: &Parsed,
    target: &TableTarget,
    budget: TextBudget,
) -> Option<TableTarget> {
    let start = target.table_range().byte_range().start;
    let node = table_at(parsed.presentation(), start)?;
    if node.source_range().byte_range().start != start {
        return None;
    }
    let table = table_source(parsed, node.source_range(), budget).ok()?;
    table
        .rows()
        .get(target.row())?
        .cells()
        .get(target.column())?;
    Some(TableTarget::new(
        node.source_range().clone(),
        target.row(),
        target.column(),
    ))
}

pub(super) fn table_menu_item(label: SharedString, can_edit: bool) -> MenuItem {
    MenuItem::action(label, TableOperations).disabled(!can_edit)
}

pub(super) fn table_target_at(
    parsed: &Parsed,
    source: &SrcOffset,
    budget: TextBudget,
) -> Option<TableTarget> {
    if source.snapshot_id() != parsed.snapshot().id() {
        return None;
    }
    let source = source.byte_offset();
    let node = table_at(parsed.presentation(), source)?;
    let table = table_source(parsed, node.source_range(), budget).ok()?;
    for (row_index, row) in table.rows().iter().enumerate() {
        for (column_index, cell) in row.cells().iter().enumerate() {
            let range = cell.source_range().byte_range();
            if range.start <= source && source <= range.end {
                return Some(TableTarget::new(
                    node.source_range().clone(),
                    row_index,
                    column_index,
                ));
            }
        }
    }
    None
}

fn table_at(nodes: &[PresentationNode], source: usize) -> Option<&PresentationNode> {
    for node in nodes {
        let range = node.source_range().byte_range();
        if range.start <= source && source <= range.end {
            if matches!(node.kind(), PresentationKind::Table { .. }) {
                return Some(node);
            }
            if let Some(table) = table_at(node.children(), source) {
                return Some(table);
            }
        }
    }
    None
}

fn table_items(
    row: usize,
    column: usize,
    rows: usize,
    columns: usize,
    complete: bool,
) -> Vec<(&'static str, MenuCommand)> {
    use MenuCommand::Edit;
    use TableOperation::{
        Align, DeleteColumn, DeleteRow, DeleteTable, InsertColumn, InsertRow, MoveColumn, MoveRow,
        PromoteHeader, Tidy,
    };
    let mut items = vec![
        (
            "action.table_insert_above",
            Edit(InsertRow { after: false }),
        ),
        ("action.table_insert_below", Edit(InsertRow { after: true })),
        (
            "action.table_insert_left",
            Edit(InsertColumn { after: false }),
        ),
        (
            "action.table_insert_right",
            Edit(InsertColumn { after: true }),
        ),
    ];
    if row > 0 {
        items.extend([
            ("action.table_delete_row", Edit(DeleteRow)),
            ("action.table_promote", Edit(PromoteHeader)),
        ]);
        if complete && row > 1 {
            items.push(("action.table_move_up", Edit(MoveRow { to: row - 1 })));
        }
        if complete && row + 1 < rows {
            items.push(("action.table_move_down", Edit(MoveRow { to: row + 1 })));
        }
    }
    if complete && columns > 1 {
        items.push(("action.table_delete_column", Edit(DeleteColumn)));
    }
    if complete && column > 0 {
        items.push((
            "action.table_move_left",
            Edit(MoveColumn { to: column - 1 }),
        ));
    }
    if complete && column + 1 < columns {
        items.push((
            "action.table_move_right",
            Edit(MoveColumn { to: column + 1 }),
        ));
    }
    items.extend([
        ("action.table_align_left", Edit(Align(TableAlignment::Left))),
        (
            "action.table_align_center",
            Edit(Align(TableAlignment::Center)),
        ),
        (
            "action.table_align_right",
            Edit(Align(TableAlignment::Right)),
        ),
        ("action.table_tidy", Edit(Tidy)),
        ("action.table_copy", MenuCommand::Copy),
        ("action.table_delete", Edit(DeleteTable)),
    ]);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native_window::editable_input::tests::session;
    use aline_buffer::HistoryBudget;
    use std::error::Error;

    #[test]
    fn table_menu_capabilities_follow_target_and_short_rows() {
        let keys = |items: Vec<(&'static str, MenuCommand)>| {
            items.into_iter().map(|(key, _)| key).collect::<Vec<_>>()
        };
        let header = keys(table_items(0, 0, 3, 1, true));
        assert!(!header.contains(&"action.table_delete_row"));
        assert!(!header.contains(&"action.table_promote"));
        assert!(!header.contains(&"action.table_delete_column"));

        let body = keys(table_items(1, 1, 3, 2, true));
        assert!(body.contains(&"action.table_move_down"));
        assert!(body.contains(&"action.table_move_left"));
        assert!(body.contains(&"action.table_delete_column"));

        let short = keys(table_items(1, 1, 3, 2, false));
        assert!(!short.contains(&"action.table_move_down"));
        assert!(!short.contains(&"action.table_move_left"));
        assert!(!short.contains(&"action.table_delete_column"));
        assert!(short.contains(&"action.table_tidy"));
        assert!(short.contains(&"action.table_copy"));
        assert!(short.contains(&"action.table_delete"));
    }

    #[test]
    fn table_menu_copies_bound_table_and_rejects_changed_input() -> Result<(), Box<dyn Error>> {
        const TABLE: &str = "| A | B |\r\n| --- | --- |\r\n| a\\|b | `甲` |\r\n";
        const RAW: &str = "前\r\n\r\n| A | B |\r\n| --- | --- |\r\n| a\\|b | `甲` |\r\n\r\n后";
        let mut state = session(RAW, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
        let document = state.document_mut().ok_or("没有文档")?;
        let click = RAW.find('甲').ok_or("没有右键目标")?;
        let budget = document.text_budget();
        let projection = document.parsed().projection().id().clone();
        let binding = document.with_working_parse(|parsed, _, identity| {
            let target = table_target_at(parsed, &parsed.snapshot().src_offset(click)?, budget)
                .ok_or(DocError::StaleResult)?;
            assert_eq!((target.row(), target.column()), (1, 1));
            Ok(TableMenuBinding {
                target,
                identity: identity.clone(),
                projection,
                layout: None,
            })
        })?;
        let selection = document.core().current_selection();
        binding.validate(document, None)?;
        assert_eq!(binding.copy_markdown(document)?, TABLE);
        assert_eq!(document.core().current_selection(), selection);
        assert_eq!(document.core().history_node_count(), 0);

        document.edit_core()?.type_text(0, "新", 1)?;
        let draft = document.core().current_selection();
        assert!(matches!(
            binding.validate(document, None),
            Err(DocError::StaleResult)
        ));
        assert_eq!(document.core().current_selection(), draft);
        assert_eq!(document.core().open_block_draft(), Some("新"));
        assert_eq!(document.core().history_node_count(), 0);
        assert_eq!(document.core().buffer_text()?, RAW);
        Ok(())
    }

    #[test]
    fn table_menu_copy_removes_each_container_prefix_and_reparses() -> Result<(), Box<dyn Error>> {
        use aline_buffer::TextSnapshot;
        use aline_markdown::{ParseBudget, ParseConfig, parse};

        const TABLE: &str = "| A | B |\r\n| --- | --- |\n| 甲 | 乙 |";
        for source in [
            "> | A | B |\r\n> | --- | --- |\n> | 甲 | 乙 |",
            "- | A | B |\r\n  | --- | --- |\n  | 甲 | 乙 |",
        ] {
            let mut state = session(source, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
            let document = state.document_mut().ok_or("没有文档")?;
            let identity = document.input_identity()?;
            let parsed = document.parsed();
            let src_off = source.find('甲').ok_or("没有目标格")?;
            let budget = document.text_budget();
            let target = table_target_at(parsed, &parsed.snapshot().src_offset(src_off)?, budget)
                .ok_or("没有表格")?;
            let binding = TableMenuBinding {
                target,
                identity,
                projection: parsed.projection().id().clone(),
                layout: None,
            };
            binding.validate(document, None)?;
            let copied = binding.copy_markdown(document)?;
            assert_eq!(copied, TABLE);
            let snapshot = TextSnapshot::new(copied, budget)?;
            let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
            assert!(matches!(
                parsed.presentation()[0].kind(),
                PresentationKind::Table { .. }
            ));
        }
        Ok(())
    }

    #[test]
    fn table_structure_commands_undo_and_redo_one_step() -> Result<(), Box<dyn Error>> {
        const RAW: &str = "前\r\n\r\n| A | B |\r\n| --- | --- |\r\n| 甲 | 1 |\r\n| 乙 | 2 |";
        for (operation, expected) in [
            (
                TableOperation::InsertRow { after: false },
                "前\r\n\r\n| A | B |\r\n| --- | --- |\r\n|  |  |\r\n| 甲 | 1 |\r\n| 乙 | 2 |",
            ),
            (
                TableOperation::DeleteRow,
                "前\r\n\r\n| A | B |\r\n| --- | --- |\r\n| 乙 | 2 |",
            ),
            (
                TableOperation::MoveRow { to: 2 },
                "前\r\n\r\n| A | B |\r\n| --- | --- |\r\n| 乙 | 2 |\r\n| 甲 | 1 |",
            ),
            (
                TableOperation::InsertColumn { after: false },
                "前\r\n\r\n|  | A | B |\r\n| --- | --- | --- |\r\n|  | 甲 | 1 |\r\n|  | 乙 | 2 |",
            ),
            (
                TableOperation::DeleteColumn,
                "前\r\n\r\n| B |\r\n| --- |\r\n| 1 |\r\n| 2 |",
            ),
            (
                TableOperation::MoveColumn { to: 1 },
                "前\r\n\r\n| B | A |\r\n| --- | --- |\r\n| 1 | 甲 |\r\n| 2 | 乙 |",
            ),
            (
                TableOperation::Align(TableAlignment::Center),
                "前\r\n\r\n| A | B |\r\n| :---: | --- |\r\n| 甲 | 1 |\r\n| 乙 | 2 |",
            ),
            (
                TableOperation::PromoteHeader,
                "前\r\n\r\n| 甲 | 1 |\r\n| --- | --- |\r\n| A | B |\r\n| 乙 | 2 |",
            ),
            (
                TableOperation::Tidy,
                "前\r\n\r\n| A | B |\r\n| --- | --- |\r\n| 甲 | 1 |\r\n| 乙 |  |",
            ),
            (TableOperation::DeleteTable, "前\r\n\r\n"),
        ] {
            let source = if operation == TableOperation::Tidy {
                "前\r\n\r\nA|B\r\n| --- | --- |\r\n| 甲 | 1 |\r\n| 乙 |"
            } else {
                RAW
            };
            let mut state = session(source, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
            let document = state.document_mut().ok_or("没有文档")?;
            let snapshot = document.current_snapshot()?;
            let start = source.find('甲').ok_or("没有目标格")?;
            let selection = SrcSelection::new(
                snapshot.text().src_offset(start + '甲'.len_utf8())?,
                snapshot.text().src_offset(start)?,
            )?;
            let identity = document.input_identity()?;
            let core = document.edit_core()?;
            core.select(&identity, &selection)?;
            let before = core.current_selection();
            let prepared = core
                .prepare_command(|parsed, selection, context, budget| {
                    let target = table_target_at(parsed, selection.head(), budget.text())
                        .ok_or(aline_edit::EditRefusal::InvalidPlan)?;
                    let command = TableCommand::new(target, operation);
                    plan_table_command(parsed, selection, context, &command, budget)
                })?
                .ok_or("结构命令没有差异")?;
            core.publish_command(&prepared, 1)?;
            assert_eq!(core.buffer_text()?, expected, "{operation:?}");
            assert_eq!(core.undo_depth(), 1, "{operation:?}");
            let after = core.current_selection();
            document.checked_undo()?;
            assert_eq!(document.core().buffer_text()?, source, "{operation:?}");
            assert_eq!(document.core().current_selection(), before, "{operation:?}");
            document.checked_redo()?;
            assert_eq!(document.core().buffer_text()?, expected, "{operation:?}");
            assert_eq!(document.core().current_selection(), after, "{operation:?}");
        }
        Ok(())
    }

    #[test]
    fn table_tab_between_existing_cells_only_moves_selection() -> Result<(), Box<dyn Error>> {
        const RAW: &str = "| A | B |\n| --- | --- |\n| 甲 | 乙 |";
        let mut state = session(RAW, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
        let document = state.document_mut().ok_or("没有文档")?;
        let initial = RAW.find('甲').ok_or("没有第一格")?;
        let destination = RAW.find('乙').ok_or("没有第二格")?;
        let snapshot = document.current_snapshot()?;
        let caret = snapshot.text().src_offset(initial)?;
        let identity = document.input_identity()?;
        document
            .edit_core()?
            .select(&identity, &SrcSelection::new(caret.clone(), caret)?)?;
        let version = document.core().version();
        let mut active = None;
        for (backward, expected) in [(false, destination), (true, initial)] {
            let next = plan_tab(document, active.as_ref(), backward)?;
            let Some(PreparedTableTab::Cell { selection, site }) = next else {
                return Err("应导航到已有单元格".into());
            };
            let identity = document.input_identity()?;
            document.edit_core()?.select(&identity, &selection)?;
            assert_eq!(
                document.core().current_selection(),
                aline_buffer::SelectionSpec::new(expected, expected)
            );
            assert_eq!(document.core().version(), version);
            assert_eq!(document.core().history_node_count(), 0);
            assert_eq!(document.core().open_block_draft(), None);
            assert_eq!(document.core().buffer_text()?, RAW);
            active = Some(site);
        }
        Ok(())
    }

    #[test]
    fn table_tab_projects_present_cells_on_either_side_of_draft() -> Result<(), Box<dyn Error>> {
        const RAW: &str = "| A | B | C |\n| --- | --- | --- |\n| 甲 | 乙 | 丙 |";
        let mut state = session(RAW, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
        let document = state.document_mut().ok_or("没有文档")?;
        let budget = document.text_budget();
        let parsed = document.parsed();
        let start = RAW.find('乙').ok_or("没有目标格")?;
        let caret = parsed.snapshot().src_offset(start + '乙'.len_utf8())?;
        let target = table_target_at(parsed, &caret, budget).ok_or("没有目标格")?;
        let selection = SrcSelection::new(caret.clone(), caret)?;
        let reader_row = target.table_range().clone();
        let identity = document.input_identity()?;
        let core = document.edit_core()?;
        core.select(&identity, &selection)?;
        core.type_selection(&identity, "草", 1)?;
        let current = document.current_snapshot()?;
        let identity = document.input_identity()?;
        let (_, site) = SourceInputSite::for_table_navigation(
            document, &current, &identity, &target, reader_row,
        )?;
        for (backward, expected) in [
            (true, RAW.find('甲').ok_or("没有上一格")?),
            (false, RAW.find('丙').ok_or("没有下一格")? + '草'.len_utf8()),
        ] {
            let next = plan_tab(document, Some(&site), backward)?;
            let Some(PreparedTableTab::Cell { selection, .. }) = next else {
                return Err("Tab 未生成单元格导航".into());
            };
            assert_eq!(selection.head().byte_offset(), expected);
        }
        assert_eq!(document.core().open_block_draft(), Some("草"));
        assert_eq!(document.core().history_node_count(), 0);
        Ok(())
    }

    #[test]
    fn table_tab_keeps_live_draft_and_distinguishes_missing_cells() -> Result<(), Box<dyn Error>> {
        const RAW: &str = "| A | B | C |\n| --- | --- | --- |\n| x |";
        let mut state = session(RAW, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
        let document = state.document_mut().ok_or("没有文档")?;
        let before = document.current_snapshot()?;
        let source = before
            .text()
            .src_offset(RAW.find('x').ok_or("没有目标格")?)?;
        let selection = SrcSelection::new(source.clone(), source.clone())?;
        let identity = document.input_identity()?;
        let core = document.edit_core()?;
        core.select(&identity, &selection)?;
        core.type_selection(&identity, "中", 1)?;
        let version = document.core().version();
        let selection = document.core().current_selection();
        let budget = document.text_budget();
        let row = document.parsed().presentation()[0].source_range().clone();
        assert!(!document.is_parse_current());
        let target = table_target_at(document.parsed(), &source, budget).ok_or("没有基线目标格")?;
        let current = document.current_snapshot()?;
        let identity = document.input_identity()?;
        let (_, mut site) =
            SourceInputSite::for_table_navigation(document, &current, &identity, &target, row)?;
        let mut previous = selection.head();
        for column in [1, 2] {
            let next = plan_tab(document, Some(&site), false)?;
            let Some(PreparedTableTab::Cell {
                selection,
                site: next_site,
            }) = next
            else {
                return Err("Tab 未进入缺失格".into());
            };
            let identity = document.input_identity()?;
            document.edit_core()?.select(&identity, &selection)?;
            let current = document.current_snapshot()?;
            let target = next_site
                .active_table_target(&identity, &current)
                .ok_or("Tab 后目标身份失效")?;
            assert_eq!((target.row(), target.column()), (1, column));
            let next_source = selection.head().byte_offset();
            assert_eq!(next_source, RAW.len() + '中'.len_utf8());
            if column == 2 {
                assert_eq!(next_source, previous);
            }
            previous = next_source;
            assert_eq!(document.core().version(), version);
            assert_eq!(document.core().open_block_draft(), Some("中"));
            assert_eq!(document.core().history_node_count(), 0);
            assert_eq!(document.core().buffer_text()?, RAW);
            site = next_site;
        }
        let before_append = document.core().current_selection();
        let append = plan_tab(document, Some(&site), false)?;
        let Some(PreparedTableTab::AppendRow(command)) = append else {
            return Err("末格 Tab 未生成追加命令".into());
        };
        let core = document.edit_core()?;
        let command = core
            .prepare_command(|parsed, selection, context, budget| {
                let target = rebind_table_target(parsed, command.target(), budget.text())
                    .ok_or(aline_edit::EditRefusal::InvalidPlan)?;
                let command = TableCommand::new(target, command.operation());
                plan_table_command(parsed, selection, context, &command, budget)
            })?
            .ok_or("追加命令没有差异")?;
        core.publish_command(&command, 2)?;
        assert_eq!(core.history_node_count(), 2);
        let appended = core.buffer_text()?;
        let appended_selection = core.current_selection();
        assert_eq!(
            appended,
            "| A | B | C |\n| --- | --- | --- |\n| 中x |\n|  |  |  |"
        );
        document.checked_undo()?;
        assert_eq!(document.core().current_selection(), before_append);
        assert_eq!(document.core().buffer_text()?, RAW.replace(" x ", " 中x "));
        document.checked_undo()?;
        assert_eq!(document.core().buffer_text()?, RAW);
        document.checked_redo()?;
        document.checked_redo()?;
        assert_eq!(document.core().buffer_text()?, appended);
        assert_eq!(document.core().current_selection(), appended_selection);
        Ok(())
    }
}
