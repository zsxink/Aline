//! 使用实际窗口输入意图消费文件会话，平台 callback 由锁定 handler 的 trait 方法接线。

use super::*;
use crate::document::parse_call_count;
use crate::{
    CommitOutcome, DocLimits, ModeSwitchOutcome, ReadonlyFileAccess, ReadonlyFileInput,
    ReadonlyOpenLimits, ReadonlySession,
};
use aline_buffer::{HistoryBudget, LineIndexBudget, SelectionSpec, TextBudget};
use aline_markdown::{ParseBudget, ParseConfig};
use aline_source::StorageBudget;
use std::{error::Error, io::Cursor, path::Path};

const TEXT: TextBudget = TextBudget::new(4096);
type TestResult = Result<(), Box<dyn Error>>;

const MULTILINE_BLOCK: &str = concat!(
    "line01\nline02\nline03\nline04\nline05\nline06\nline07\nline08\nline09\nline10\n",
    "line11\nline12\nline13\nline14\nline15\nline16\nline17\nline18\nline19\nline20\n",
    "line21\nline22\nline23\nline24\nline25\nline26\nline27\nline28\nline29\nline30\n",
    "line31\nline32\nline33\nline34\nline35\nline36\nline37\nline38\nline39\nline40",
);

struct File(&'static str);
impl ReadonlyFileAccess for File {
    type Reader = Cursor<&'static [u8]>;

    fn open(&self, _: &Path) -> std::io::Result<ReadonlyFileInput<Self::Reader>> {
        Ok(ReadonlyFileInput::new(
            Cursor::new(self.0.as_bytes()),
            self.0.len() as u64,
            true,
        ))
    }
}

pub(in crate::native_window) fn session(
    raw: &'static str,
    history: HistoryBudget,
    mode: EditorMode,
) -> Result<ReadonlySession, Box<dyn Error>> {
    let mut state = ReadonlySession::new(
        ReadonlyOpenLimits::new(
            StorageBudget::new(4096),
            DocLimits::new(TEXT, LineIndexBudget::new(128), history),
            ParseConfig::m0(),
            ParseBudget::m0(),
        )
        .with_editing(),
    );
    let request = state.request_open()?;
    let job = state
        .accept_selection(&request, Some(vec!["test.md".into()]))?
        .ok_or("没有工作项")?;
    state.accept_open(job.read_with(&File(raw)));
    let doc = state.document_mut().ok_or("没有文档")?;
    if mode == EditorMode::Visual {
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        assert!(doc.open_visual_at(&identity, current.selection())?);
    } else {
        doc.switch_mode(mode)?;
    }
    Ok(state)
}

fn view(doc: &mut ReadonlyDocument) -> Result<InputView, Box<dyn Error>> {
    let current = doc.current_snapshot()?;
    let len = current.len_bytes();
    let site = InputSite {
        mode: doc.core().mode(),
        source: 0..len,
        document_len: len,
        reader_row: current.text().src_range(0..len)?,
        cell: None,
        blank: false,
    };
    InputView::new(doc, site).ok_or_else(|| "没有正文输入投影".into())
}

fn apply(doc: &mut ReadonlyDocument, site: &InputView, edit: InputIntent) -> Result<(), DocError> {
    let current = doc.current_snapshot()?;
    edit.apply(site, &current, doc, 1)
}

#[test]
fn blank_line_input_rebinds_after_commit_before_reader_refresh() -> TestResult {
    let mut state = session("- 父\r\n", HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let document = state.document_mut().ok_or("没有文档")?;
    let current = document.current_snapshot()?;
    let caret = current.text().src_offset(current.len_bytes())?;
    let selection = SrcSelection::new(caret.clone(), caret)?;
    let identity = document.input_identity()?;
    assert!(document.open_visual_at(&identity, &selection)?);
    let reader_row = selection.range();
    let site = InputSite::for_row(document, reader_row.clone(), None).ok_or("没有空行输入")?;
    let view = InputView::new(document, site).ok_or("没有空行投影")?;
    let frame = InputFrame(Rc::new(RefCell::new(Some(view.clone()))));
    apply(document, &view, InputIntent::Replace(None, "正".to_owned()))?;
    assert!(frame.rebind(document));
    document.edit_core()?.commit_open_block(2)?;
    assert!(!document.is_parse_current());
    assert!(frame.rebind(document));
    let retained = frame.source_site().ok_or("没有保留的空行投影")?;
    let site = InputSite::for_row(document, reader_row, Some(&retained)).ok_or("未重绑空行")?;
    let view = InputView::new(document, site).ok_or("没有重绑后的投影")?;
    apply(document, &view, InputIntent::Replace(None, "文".to_owned()))?;
    document.edit_core()?.commit_open_block(3)?;
    assert_eq!(document.core().buffer_text()?, "- 父\r\n\r\n正文");
    assert_eq!(document.core().history_node_count(), 2);
    Ok(())
}

const SHORT_TABLE: &str = "| A | B | C |\n| --- | --- | --- |\n| a |";

fn short_table_site(
    document: &mut ReadonlyDocument,
    column: usize,
) -> Result<InputSite, Box<dyn Error>> {
    let current = document.current_snapshot()?;
    let table = document
        .parsed()
        .presentation()
        .first()
        .ok_or("没有表格")?
        .source_range()
        .clone();
    let target = aline_edit::TableTarget::new(table.clone(), 1, column);
    InputSite::for_table(document.parsed(), &current, &target, table, TEXT)
        .ok_or_else(|| "没有单元格输入投影".into())
}

fn select_site(document: &mut ReadonlyDocument, site: &InputSite) -> Result<(), DocError> {
    let current = document.current_snapshot()?;
    let identity = document.input_identity()?;
    let offset = current.text().src_offset(site.source.start)?;
    let selection = SrcSelection::new(offset.clone(), offset)?;
    document.edit_core()?.select(&identity, &selection)
}

#[test]
fn table_input_keeps_distinct_missing_cells_at_shared_anchor() -> TestResult {
    let mut state = session(SHORT_TABLE, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let document = state.document_mut().ok_or("没有文档")?;
    let first = short_table_site(document, 1)?;
    let second = short_table_site(document, 2)?;
    assert_eq!(first.source, second.source);
    select_site(document, &second)?;
    let version = document.core().version();
    let view = InputView::new(document, second.clone()).ok_or("缺失格投影为空")?;
    let binding = SourceInputSite {
        identity: view.identity.clone(),
        site: second.clone(),
    };
    let row = second.reader_row.clone();
    let rendered = InputSite::for_row(document, row, Some(&binding)).ok_or("缺失格未渲染")?;
    assert_eq!(rendered.cell(), Some((1, 2)));
    assert_eq!(document.core().version(), version);
    assert_eq!(document.core().history_node_count(), 0);
    apply(document, &view, InputIntent::Replace(None, "中".to_owned()))?;
    assert_eq!(
        document.current_snapshot()?.full_text(TEXT)?,
        format!("{SHORT_TABLE}  | 中 |")
    );
    assert_eq!(document.core().history_node_count(), 1);
    document.checked_undo()?;
    assert_eq!(document.current_snapshot()?.full_text(TEXT)?, SHORT_TABLE);
    Ok(())
}

#[test]
fn table_input_materializes_missing_cell_only_on_ime_confirmation() -> TestResult {
    let mut state = session(SHORT_TABLE, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let document = state.document_mut().ok_or("没有文档")?;
    let site = short_table_site(document, 2)?;
    select_site(document, &site)?;
    let view = InputView::new(document, site).ok_or("缺失格投影为空")?;
    apply(
        document,
        &view,
        InputIntent::Mark(None, "ni".to_owned(), Some(2..2)),
    )?;
    assert_eq!(document.core().buffer_text()?, SHORT_TABLE);
    assert_eq!(document.core().history_node_count(), 0);
    let site = view.site.rebound(document).ok_or("组合后单元格丢失")?;
    let view = InputView::new(document, site).ok_or("组合投影为空")?;
    let current = document.current_snapshot()?;
    let intent = InputIntent::Replace(Some(0..2), "你".to_owned());
    let InputStep::Prepared(job) = view.step(&intent, &view.identity, &current, document, 2) else {
        return Err("组合确认未冻结".into());
    };
    document.edit_core()?.publish_native_input(job.run()?)?;
    assert_eq!(
        document.current_snapshot()?.full_text(TEXT)?,
        format!("{SHORT_TABLE}  | 你 |")
    );
    assert_eq!(document.core().history_node_count(), 1);
    assert!(document.core().composition().is_none());
    assert!(document.core().has_open_block());
    let rebound = view.site.rebound(document).ok_or("提交后单元格丢失")?;
    assert_eq!(rebound.cell(), Some((1, 2)));
    assert!(rebound.missing_cell().is_none());
    Ok(())
}

#[test]
fn table_input_nested_projection_mounts_only_its_cell() -> TestResult {
    const RAW: &str = "> | A | B |\n> | --- | --- |\n> | x | y |";
    let mut state = session(RAW, HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let document = state.document_mut().ok_or("没有文档")?;
    let current = document.current_snapshot()?;
    let offset = current
        .text()
        .src_offset(RAW.find('x').ok_or("没有目标格")?)?;
    let target =
        crate::native_window::table_commands::table_target_at(document.parsed(), &offset, TEXT)
            .ok_or("引用内表格未定位")?;
    let row = target.table_range().clone();
    let identity = document.input_identity()?;
    let selection = SrcSelection::new(offset.clone(), offset)?;
    document.edit_core()?.select(&identity, &selection)?;
    assert_ne!(row.byte_range().start, 0);
    let site = InputSite::for_row(document, row, None).ok_or("引用内单元格没有输入帧")?;
    let view = InputView::new(document, site).ok_or("引用内单元格投影为空")?;
    assert_eq!(
        view.bound(&view.identity).ok_or("投影失效")?.drawn_text(),
        " x "
    );
    apply(document, &view, InputIntent::Replace(None, "中".to_owned()))?;
    document.edit_core()?.escape(2)?;
    assert_eq!(document.core().buffer_text()?, RAW.replace(" x ", " 中x "));
    assert_eq!(document.core().history_node_count(), 1);
    Ok(())
}

#[test]
fn multiline_visual_input_geometry_keeps_a_mid_block_source_anchor() -> TestResult {
    let mut state = session(
        MULTILINE_BLOCK,
        HistoryBudget::new(8, 4096),
        EditorMode::Source,
    )?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    // 独立预期：前 29 行各 7 字节，第 30 行第三个 ASCII 字符后的端点是 206。
    let selection = SrcSelection::new(
        current.text().src_offset(206)?,
        current.text().src_offset(206)?,
    )?;
    doc.edit_core()?.select(&identity, &selection)?;
    doc.switch_mode(EditorMode::Visual)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    assert!(doc.open_visual_at(&identity, current.selection())?);
    let row = doc
        .parsed()
        .snapshot()
        .src_range(0..MULTILINE_BLOCK.len())?;
    let input_site = InputSite::for_row(doc, row, None).ok_or("没有多行输入站点")?;
    let view = InputView::new(doc, input_site).ok_or("没有多行输入投影")?;
    let bound = view.bound(&view.identity).ok_or("没有受检投影")?;
    assert_eq!(bound.drawn_text(), MULTILINE_BLOCK);
    let mapped = bound
        .projection()
        .drawn_at(current.selection().head(), aline_buffer::Affinity::After)?;
    assert_eq!(mapped.value().byte_offset(), 206);
    assert_eq!(
        bound
            .projection()
            .source_at(mapped.value(), aline_buffer::Affinity::After)?
            .value(),
        current.selection().head()
    );
    assert!(input_geometry_matches(&view.identity, doc));
    assert_eq!(doc.core().history_node_count(), 0);
    assert_eq!(doc.core().buffer_text()?, MULTILINE_BLOCK);

    doc.switch_mode(EditorMode::Source)?;
    let source_row = doc.parsed().snapshot().src_range(203..209)?;
    let source_site =
        InputSite::for_row(doc, source_row, None).ok_or("反切源码后应有第30行站点")?;
    let source_view = InputView::new(doc, source_site).ok_or("反切源码后应有输入投影")?;
    let source_bound = source_view
        .bound(&source_view.identity)
        .ok_or("没有源码输入绑定")?;
    assert_eq!(source_bound.drawn_text(), "line30");
    let current = doc.current_snapshot()?;
    assert_eq!(
        source_bound
            .projection()
            .drawn_at(current.selection().head(), aline_buffer::Affinity::After)?
            .value()
            .byte_offset(),
        3
    );
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(206, 206));
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn input_geometry_identity_rejects_input_config_mode_and_foreign_document_changes() -> TestResult {
    for scenario in ["input", "config", "request", "mode", "document"] {
        let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
        let doc = state.document_mut().ok_or("没有文档")?;
        let view = view(doc)?;
        assert!(input_geometry_matches(&view.identity, doc));
        match scenario {
            "input" => apply(doc, &view, InputIntent::Replace(None, "X".to_owned()))?,
            "config" => doc.edit_core()?.advance_parse_config()?,
            "request" => {
                doc.edit_core()?.begin_document_request()?;
            }
            "mode" => {
                doc.switch_mode(EditorMode::Visual)?;
            }
            _ => {}
        }
        if scenario == "document" {
            let other = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
            assert!(!input_geometry_matches(
                &view.identity,
                other.document().ok_or("没有文档")?
            ));
        } else {
            assert!(!input_geometry_matches(&view.identity, doc), "{scenario}");
        }
    }
    Ok(())
}

#[test]
fn native_input_reports_source_entities_and_rejects_half_surrogates_and_graphemes() -> TestResult {
    let mut state = session(
        "👩‍💻 &amp; e\u{301}",
        HistoryBudget::new(8, 4096),
        EditorMode::Visual,
    )?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let site = view(doc)?;
    let bound = site.bound(&site.identity).ok_or("没有绑定")?;
    assert_eq!(bound.drawn_text(), "👩‍💻 &amp; e\u{301}");
    let entity = bound.source_from_utf16(&bound.utf16_range(6..11)?)?;
    assert_eq!(entity.byte_range(), 12..17);
    let before = doc.current_snapshot()?;
    for range in [1..1, 2..2, 5..7] {
        assert!(
            apply(
                doc,
                &site,
                InputIntent::Replace(Some(range), "x".to_owned())
            )
            .is_err()
        );
    }
    let after = doc.current_snapshot()?;
    assert_eq!(before.version(), after.version());
    assert_eq!(before.selection(), after.selection());
    assert_eq!(before.full_text(TEXT)?, after.full_text(TEXT)?);
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn marked_update_replaces_candidate_and_commits_platform_confirmation_once() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(0..1))?;
    apply(
        doc,
        &initial,
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)),
    )?;
    let current = view(doc)?;
    assert!(matches!(
        apply(doc, &initial, InputIntent::Replace(None, "过期".to_owned())),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.core().composition().ok_or("没有候选")?.candidate(), "n");
    apply(
        doc,
        &current,
        InputIntent::Mark(Some(0..1), "👋你".to_owned(), Some(2..3)),
    )?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(4, 7));
    assert_eq!(doc.core().history_node_count(), 0);
    let updated = view(doc)?;
    apply(
        doc,
        &updated,
        InputIntent::Replace(Some(0..3), "你好".to_owned()),
    )?;
    assert!(doc.core().composition().is_none());
    assert_eq!(doc.core().buffer_text()?, "你好b");
    assert_eq!(doc.core().history_node_count(), 1);
    assert!(matches!(
        apply(doc, &updated, InputIntent::Replace(None, "你好".to_owned())),
        Err(DocError::StaleResult)
    ));
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(0, 1));
    Ok(())
}

#[test]
fn marked_cancel_restores_selection_and_failed_confirmation_keeps_mark() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(0, 0), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let site = view(doc)?;
    apply(doc, &site, InputIntent::Select(0..1))?;
    assert!(
        apply(
            doc,
            &site,
            InputIntent::Mark(None, "👋".to_owned(), Some(1..2))
        )
        .is_err()
    );
    assert!(doc.core().composition().is_none());
    assert!(!doc.core().has_open_block());
    apply(
        doc,
        &site,
        InputIntent::Mark(None, "ni".to_owned(), Some(2..2)),
    )?;
    let updated = view(doc)?;
    let before = doc.current_snapshot()?;
    assert!(matches!(
        apply(doc, &updated, InputIntent::Replace(None, "你".to_owned())),
        Err(DocError::Buffer(BufferError::HistoryBudgetExceeded { .. }))
    ));
    let after = doc.current_snapshot()?;
    assert_eq!(before.version(), after.version());
    assert_eq!(before.selection(), after.selection());
    assert_eq!(before.full_text(TEXT)?, after.full_text(TEXT)?);
    assert_eq!(
        doc.core().composition().ok_or("没有候选")?.candidate(),
        "ni"
    );
    apply(
        doc,
        &updated,
        InputIntent::Mark(Some(0..2), String::new(), Some(0..0)),
    )?;
    assert!(doc.core().composition().is_none());
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(0, 1));
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn input_recovery_rejects_stale_platform_coordinates_and_rebinds_commands() {
    // 携带平台 UTF-16 坐标的意图：旧帧不可消费时必须拒收，不能换投影解释。
    let platform = [
        InputIntent::Replace(Some(0..1), "x".to_owned()),
        InputIntent::Mark(None, "n".to_owned(), None),
        InputIntent::Mark(Some(0..1), "n".to_owned(), Some(1..1)),
        InputIntent::Select(0..1),
    ];
    for composing in [false, true] {
        for intent in &platform {
            assert_eq!(
                input_recovery(true, composing, intent),
                InputRecovery::Proceed,
                "可消费帧上的平台意图应直接提交"
            );
            assert_eq!(
                input_recovery(false, composing, intent),
                InputRecovery::Reject,
                "过期帧上的平台坐标意图必须静默拒收"
            );
        }
        // 组合期无范围 Replace 目标是组合范围，同按平台坐标拒收。
        let plain = InputIntent::Replace(None, "x".to_owned());
        assert_eq!(
            input_recovery(false, true, &plain),
            InputRecovery::Reject,
            "组合期无范围替换必须保全组合目标"
        );
        // 非组合无范围 Replace 只表达当前选区键入：入口与排水同口径重绑。
        assert_eq!(
            input_recovery(false, false, &plain),
            InputRecovery::Rebind,
            "非组合普通键入与排水路径同口径按语义重绑"
        );
    }
    // 只携带语义的命令：过期后按最新状态重建，排队输入继续按序应用。
    let commands = [
        InputIntent::LineBreak,
        InputIntent::Delete {
            forward: true,
            count: 1,
        },
        InputIntent::Delete {
            forward: false,
            count: 2,
        },
        InputIntent::Move {
            forward: true,
            extend: false,
        },
        InputIntent::Move {
            forward: false,
            extend: true,
        },
        InputIntent::Paste("p".to_owned()),
        InputIntent::Escape,
    ];
    for intent in &commands {
        assert_eq!(
            input_recovery(true, false, intent),
            InputRecovery::Proceed,
            "可消费帧上的命令应直接提交"
        );
        assert_eq!(
            input_recovery(false, false, intent),
            InputRecovery::Rebind,
            "过期帧上的命令应重建后继续"
        );
    }
}

#[test]
fn input_settlement_keeps_save_and_close_feedback_until_user_acts() {
    // 输入类提示随下一次输入消失，保存/冲突/关闭类提示必须保留。
    for key in [
        "document.edit_refused",
        "document.input_dropped",
        "document.composition_pending",
        "document.save_saved",
        "document.save_cancelled",
    ] {
        assert!(input_feedback_cleared(key), "输入类反馈应被清除：{key}");
    }
    for key in [
        "document.save_failed",
        "document.save_conflict",
        "document.save_uncertain",
        "document.external_conflict",
        "document.external_missing",
        "document.close_pending_changes",
        "document.close_save_failed",
        "document.close_save_conflict",
        "document.close_save_uncertain",
    ] {
        assert!(
            !input_feedback_cleared(key),
            "保存/关闭类反馈不得清除：{key}"
        );
    }
}

#[test]
fn same_version_source_retarget_invalidates_queries_and_writes_for_old_row() -> TestResult {
    let mut state = session("ab\ncd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(0..2)?;
    let site = InputSite::for_row(doc, row, None).ok_or("没有当前输入行")?;
    let old = InputView::new(doc, site).ok_or("没有正文输入投影")?;
    let caret = current.text().src_offset(4)?;
    let selection = SrcSelection::new(caret.clone(), caret)?;
    doc.edit_core()?.select(&old.identity, &selection)?;
    let newer = doc.current_snapshot()?;
    let identity = doc.input_identity()?;
    assert_eq!(old.identity, identity);
    assert!(old.active(&identity, &newer, EditorMode::Source).is_none());
    assert!(matches!(
        apply(doc, &old, InputIntent::Select(1..1)),
        Err(DocError::StaleResult)
    ));
    assert!(matches!(
        apply(doc, &old, InputIntent::Replace(None, "旧输入".to_owned())),
        Err(DocError::StaleResult)
    ));
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(4, 4));
    assert_eq!(doc.core().buffer_text()?, "ab\ncd");
    Ok(())
}

#[test]
fn backspace_and_delete_remove_a_grapheme_and_restore_directional_selection_on_undo() -> TestResult
{
    let mut state = session(
        "a👩‍💻e\u{301}b",
        HistoryBudget::new(8, 4096),
        EditorMode::Source,
    )?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(
        doc,
        &initial,
        InputIntent::Move {
            forward: true,
            extend: false,
        },
    )?;
    apply(
        doc,
        &initial,
        InputIntent::Move {
            forward: true,
            extend: true,
        },
    )?;
    apply(
        doc,
        &initial,
        InputIntent::Delete {
            forward: false,
            count: 1,
        },
    )?;
    assert_eq!(doc.core().buffer_text()?, "ae\u{301}b");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 12));
    let restored = view(doc)?;
    apply(
        doc,
        &restored,
        InputIntent::Move {
            forward: true,
            extend: false,
        },
    )?;
    apply(
        doc,
        &restored,
        InputIntent::Delete {
            forward: true,
            count: 1,
        },
    )?;
    assert_eq!(doc.core().buffer_text()?, "a👩‍💻b");
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(12, 12));
    Ok(())
}

#[test]
fn backspace_inside_pending_typing_stays_in_the_same_group() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "👩‍💻".to_owned()))?;
    let inserted = view(doc)?;
    apply(
        doc,
        &inserted,
        InputIntent::Delete {
            forward: false,
            count: 1,
        },
    )?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "ab");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn composing_projection_can_process_natural_end_without_current_reader_row() -> TestResult {
    let mut state = session(
        "ab\ncd\nef",
        HistoryBudget::new(8, 4096),
        EditorMode::Source,
    )?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(0..2)?;
    let hidden_row = current.text().src_range(6..8)?;
    let site = InputSite::for_row(doc, row.clone(), None).ok_or("没有当前输入行")?;
    let view = InputView::new(doc, site).ok_or("没有正文输入投影")?;
    apply(
        doc,
        &view,
        InputIntent::Mark(None, "ni".to_owned(), Some(2..2)),
    )?;
    let site = view.site.rebound(doc).ok_or("组合投影失效")?;
    let view = InputView::new(doc, site.clone()).ok_or("组合投影为空")?;
    let continuation = SourceInputSite {
        identity: view.identity.clone(),
        site,
    };
    assert!(InputSite::for_row(doc, hidden_row, Some(&continuation)).is_none());
    assert!(InputSite::for_row(doc, row.clone(), None).is_none());
    assert!(InputSite::for_row(doc, row, Some(&continuation)).is_some());
    apply(
        doc,
        &view,
        InputIntent::Mark(Some(0..2), String::new(), Some(0..0)),
    )?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "ab\ncd\nef");
    assert_eq!(doc.core().history_node_count(), 0);
    assert!(doc.core().composition().is_none());
    Ok(())
}

#[test]
fn escape_commits_pending_text_and_preserves_a_live_composition() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Replace(None, "x".to_owned()))?;
    assert_eq!(doc.core().history_node_count(), 0);
    doc.edit_core()?.escape(1)?;
    assert!(!doc.core().has_open_block());
    assert_eq!(doc.core().buffer_text()?, "xab");
    assert_eq!(doc.core().history_node_count(), 1);

    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Visual)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(
        doc,
        &initial,
        InputIntent::Mark(None, "ni".to_owned(), Some(2..2)),
    )?;
    assert!(matches!(
        doc.edit_core()?.escape(2),
        Err(DocError::CompositionActive)
    ));
    assert_eq!(
        doc.core().composition().ok_or("没有候选")?.candidate(),
        "ni"
    );
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn identical_ime_confirmation_keeps_final_selection_after_mode_switch() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(0..1))?;
    apply(
        doc,
        &initial,
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)),
    )?;
    let marked = view(doc)?;
    apply(doc, &marked, InputIntent::Replace(None, "a".to_owned()))?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.core().buffer_selection(), SelectionSpec::new(1, 1));
    doc.switch_mode(EditorMode::Visual)?;
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn line_break_and_following_input_share_the_source_site_until_commit() -> TestResult {
    let mut state = session("ab\ncd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(0..2)?;
    let site = InputSite::for_row(doc, row.clone(), None).ok_or("没有当前行")?;
    let initial = InputView::new(doc, site).ok_or("没有输入投影")?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::LineBreak)?;
    let site = initial.site.rebound(doc).ok_or("输入行失效")?;
    let continued = InputView::new(doc, site.clone()).ok_or("没有续接投影")?;
    let continuation = SourceInputSite {
        identity: continued.identity.clone(),
        site,
    };
    assert!(InputSite::for_row(doc, row, Some(&continuation)).is_some());
    apply(doc, &continued, InputIntent::Replace(None, "中".to_owned()))?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "a\n中b\ncd");
    assert_eq!(doc.core().history_node_count(), 0);
    doc.edit_core()?.escape(2)?;
    assert_eq!(doc.core().buffer_text()?, "a\n中b\ncd");
    assert_eq!(doc.core().history_node_count(), 1);
    Ok(())
}

#[test]
fn source_rebound_relocates_to_the_same_line_after_an_earlier_length_change() -> TestResult {
    let mut state = session("a\nb\nc", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    // 在 c 行首打开块，并把该行绑定为输入站点。
    let len = doc.current_snapshot()?.len_bytes();
    doc.edit_core()?
        .open_block(0..len, SelectionSpec::new(4, 4))?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(4..5)?;
    let site = InputSite::for_row(doc, row, None).ok_or("没有当前输入行")?;
    let initial = InputView::new(doc, site).ok_or("没有正文输入投影")?;

    // 显式编辑：在 c 行之前插入一个字符并提交，长度 +1、行数不变。
    doc.edit_core()?.type_text(0, "x", 0)?;
    assert!(matches!(
        doc.edit_core()?.commit_open_block(1)?,
        CommitOutcome::Committed { .. }
    ));
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "xa\nb\nc");
    // 光标随内容平移到新 c 行首（5），再重建输入站点。
    let identity = doc.input_identity()?;
    let head = doc.current_snapshot()?.text().src_offset(5)?;
    doc.edit_core()?
        .select(&identity, &SrcSelection::new(head.clone(), head)?)?;

    let rebound = initial.site.rebound(doc).ok_or("应能重建到同一逻辑行")?;
    assert_eq!(rebound.source, 5..6, "重建必须指向 c 行而不是旧偏移");
    let resumed = InputView::new(doc, rebound).ok_or("没有重建后的投影")?;
    apply(doc, &resumed, InputIntent::Replace(None, "X".to_owned()))?;
    assert_eq!(
        doc.current_snapshot()?.full_text(TEXT)?,
        "xa\nb\nXc",
        "排队按键必须写入同一逻辑行"
    );
    Ok(())
}

#[test]
fn source_rebound_refuses_a_line_merged_into_the_previous_one() -> TestResult {
    let mut state = session("a\nb\nc", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let len = doc.current_snapshot()?.len_bytes();
    doc.edit_core()?
        .open_block(0..len, SelectionSpec::new(4, 4))?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(4..5)?;
    let site = InputSite::for_row(doc, row, None).ok_or("没有当前输入行")?;
    let initial = InputView::new(doc, site).ok_or("没有正文输入投影")?;

    // 显式编辑：删除 b 与 c 之间的换行，c 行被并入 b 行，光标留在合并点。
    let identity = doc.input_identity()?;
    let anchor = current.text().src_offset(3)?;
    let head = current.text().src_offset(4)?;
    doc.edit_core()?
        .select(&identity, &SrcSelection::new(anchor, head)?)?;
    doc.edit_core()?.replace_selection(&identity, "", 1)?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "a\nbc");

    assert!(
        initial.site.rebound(doc).is_none(),
        "原输入行已被并入更早的行，无法证明时不得把排队输入写到新偏移"
    );
    Ok(())
}

#[test]
fn source_rebound_refuses_a_selection_outside_the_logical_line() -> TestResult {
    let mut state = session("a\nb\nc", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let len = doc.current_snapshot()?.len_bytes();
    doc.edit_core()?
        .open_block(0..len, SelectionSpec::new(4, 4))?;
    let current = doc.current_snapshot()?;
    let row = current.text().src_range(4..5)?;
    let site = InputSite::for_row(doc, row, None).ok_or("没有当前输入行")?;
    let initial = InputView::new(doc, site).ok_or("没有正文输入投影")?;

    // 边界仍成立，但选区跨出该行；单一逻辑行不能被证明为输入站点。
    let identity = doc.input_identity()?;
    let anchor = current.text().src_offset(1)?;
    let head = current.text().src_offset(4)?;
    doc.edit_core()?
        .select(&identity, &SrcSelection::new(anchor, head)?)?;
    assert!(
        initial.site.rebound(doc).is_none(),
        "选区跨行时不得把该行继续当作输入站点"
    );
    Ok(())
}

#[test]
fn deleting_all_of_a_replacement_draft_keeps_its_removal_at_commit() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(0..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "x".to_owned()))?;
    let after = view(doc)?;
    apply(
        doc,
        &after,
        InputIntent::Delete {
            forward: false,
            count: 1,
        },
    )?;
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "b");
    doc.edit_core()?.escape(2)?;
    assert_eq!(doc.core().buffer_text()?, "b");
    assert_eq!(doc.core().history_node_count(), 1);
    doc.edit_core()?.undo()?;
    assert_eq!(doc.core().buffer_text()?, "ab");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(0, 1));
    Ok(())
}

#[test]
fn native_delete_prepare_and_publish_do_not_parse_on_main_thread() -> TestResult {
    let mut state = session("abcd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let site = view(doc)?;
    apply(doc, &site, InputIntent::Select(1..1))?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let baseline = parse_call_count();
    let InputStep::Prepared(job) = site.step(
        &InputIntent::Delete {
            forward: false,
            count: 1,
        },
        &identity,
        &current,
        doc,
        3,
    ) else {
        return Err("独立删除应准备后台 job".into());
    };
    assert_eq!(parse_call_count(), baseline, "删除准备段不得解析");
    let prepared = job.run()?;
    let background = parse_call_count();
    assert!(background > baseline, "后台 job 必须解析源码");
    doc.edit_core()?.publish_native_input(prepared)?;
    assert_eq!(parse_call_count(), background, "发布段不得解析");
    assert_eq!(doc.core().buffer_text()?, "bcd");
    Ok(())
}

#[test]
fn typing_inside_the_window_does_not_parse_and_the_boundary_only_freezes_a_job() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    let baseline = parse_call_count();
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    assert!(matches!(
        initial.step(
            &InputIntent::Replace(None, "x".to_owned()),
            &identity,
            &current,
            doc,
            10,
        ),
        InputStep::Applied { changed: true, .. }
    ));
    assert_eq!(parse_call_count(), baseline, "组内普通输入不得解析");

    let continued = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let InputStep::Sealed(job) = continued.step(
        &InputIntent::Replace(None, "y".to_owned()),
        &identity,
        &current,
        doc,
        911,
    ) else {
        return Err("900ms 边界后应冻结封口 job".into());
    };
    assert_eq!(parse_call_count(), baseline, "封口准备段不得解析");
    // 边界按键只冻结 prepared 封口：旧草稿、历史与块状态在发布前均未同步提交。
    assert_eq!(doc.core().buffer_text()?, "ab", "封口准备不得同步发布草稿");
    assert_eq!(doc.core().history_node_count(), 0, "封口准备不得同步写历史");
    assert_eq!(doc.core().open_block_draft(), Some("x"));
    let prepared = job.run()?;
    let background = parse_call_count();
    assert!(background > baseline, "封口后台 job 必须解析源码");
    doc.edit_core()?.publish_native_input(prepared)?;
    assert_eq!(parse_call_count(), background, "封口发布段不得解析");
    assert_eq!(doc.core().buffer_text()?, "axb");
    assert_eq!(doc.core().history_node_count(), 1);
    assert!(doc.core().open_block_draft().is_none());
    Ok(())
}

#[test]
fn draft_local_backspace_completes_without_a_background_job() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "xy".to_owned()))?;
    let typed = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let baseline = parse_call_count();
    let step = typed.step(
        &InputIntent::Delete {
            forward: false,
            count: 1,
        },
        &identity,
        &current,
        doc,
        2,
    );
    assert!(
        matches!(step, InputStep::Applied { changed: true, .. }),
        "组内删除应同步完成"
    );
    assert_eq!(parse_call_count(), baseline, "组内删除不得解析");
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "axb");
    Ok(())
}

#[test]
fn queued_text_after_in_flight_backspace_publishes_delete_before_text() -> TestResult {
    let mut state = session("abcd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(2..2))?;

    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let delete = ScheduledInput::new(
        InputIntent::Delete {
            forward: false,
            count: 1,
        },
        1,
    );
    let InputStep::Prepared(job) =
        initial.step(&delete.intent, &identity, &current, doc, delete.now_millis)
    else {
        return Err("提交删除应准备后台 job".into());
    };

    let document = doc.core().document_id().clone();
    let mut scheduler = InputScheduler::new();
    let InputSubmission::Now(delete) = scheduler.submit(&document, delete) else {
        return Err("空闲调度应立即执行删除".into());
    };
    scheduler.start_direct(delete);
    let queued =
        ScheduledInput::capture(InputIntent::Replace(None, "x".to_owned()), 2, &initial, doc);
    assert!(matches!(
        scheduler.submit(&document, queued),
        InputSubmission::Queued
    ));
    assert!(scheduler.is_active());
    assert_eq!(scheduler.pending_len(), 1, "在途删除期间的文本必须排队");

    let Some(active) = scheduler.take_active() else {
        return Err("删除 job 缺少在途记录".into());
    };
    assert!(!active.is_seal());
    let prepared = job.run()?;
    doc.edit_core()?.publish_native_input(prepared)?;
    assert_eq!(doc.core().buffer_text()?, "acd", "删除必须先发布");
    assert_eq!(scheduler.pending_len(), 1, "发布后文本仍在等待队列");

    let Some(queued) = scheduler.take_pending() else {
        return Err("等待的文本输入缺失".into());
    };
    let InputIntent::Replace(None, text) = &queued.intent else {
        return Err("等待意图不是文本替换".into());
    };
    assert_eq!(text, "x");
    let after = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    assert!(matches!(
        queued.step(&after, &identity, &current, doc),
        InputStep::Applied { changed: true, .. }
    ));
    assert_eq!(
        doc.current_snapshot()?.full_text(TEXT)?,
        "axcd",
        "文本必须在删除之后应用"
    );
    Ok(())
}

#[test]
fn queued_platform_selection_after_backspace_does_not_select_a_different_character() -> TestResult {
    let mut state = session("abcd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(2..2))?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let delete = ScheduledInput::new(
        InputIntent::Delete {
            forward: false,
            count: 1,
        },
        1,
    );
    let InputStep::Prepared(job) =
        initial.step(&delete.intent, &identity, &current, doc, delete.now_millis)
    else {
        return Err("删除必须先冻结为在途 job".into());
    };
    let document = doc.core().document_id().clone();
    let mut scheduler = InputScheduler::new();
    let InputSubmission::Now(delete) = scheduler.submit(&document, delete) else {
        return Err("首条删除必须直接执行".into());
    };
    scheduler.start_direct(delete);
    // 接收时 2..3 选择 c；删除 b 后，同一数值却会选择 d。
    let queued = ScheduledInput::capture(InputIntent::Select(2..3), 2, &initial, doc);
    assert!(matches!(
        scheduler.submit(&document, queued),
        InputSubmission::Queued
    ));
    let following =
        ScheduledInput::capture(InputIntent::Replace(None, "x".to_owned()), 3, &initial, doc);
    assert!(matches!(
        scheduler.submit(&document, following),
        InputSubmission::Queued
    ));
    doc.edit_core()?.publish_native_input(job.run()?)?;
    assert!(scheduler.take_active().is_some());
    let queued = scheduler
        .take_pending()
        .ok_or("平台选择回调必须保持排队次序")?;
    assert_eq!(doc.core().buffer_text()?, "acd");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    let after = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let step = queued.step(&after, &identity, &current, doc);
    assert!(matches!(step, InputStep::StalePlatformOrigin));
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
    assert_eq!(doc.core().buffer_text()?, "acd");
    assert_eq!(doc.core().undo_depth(), 1);
    assert_eq!(doc.core().redo_depth(), 0);
    assert!(doc.core().composition().is_none());
    assert!(doc.core().open_block_draft().is_none());
    assert_eq!(scheduler.pending_len(), 1, "过期选择不得清空后续语义输入");
    let following = scheduler.take_pending().ok_or("后续语义按键缺失")?;
    let current = doc.current_snapshot()?;
    assert!(matches!(
        following.step(&after, &identity, &current, doc),
        InputStep::Applied { changed: true, .. }
    ));
    assert_eq!(
        doc.current_snapshot()?.full_text(TEXT)?,
        "axcd",
        "不能因旧范围错选 d 而覆盖它"
    );
    assert!(scheduler.take_pending().is_none());
    Ok(())
}

#[test]
fn queued_platform_replace_and_mark_after_backspace_preserve_the_published_state() -> TestResult {
    let callbacks = [
        InputIntent::Replace(Some(2..2), "x".to_owned()),
        InputIntent::Mark(Some(2..2), "n".to_owned(), Some(1..1)),
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)),
    ];
    for intent in callbacks {
        let mut state = session("abcd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
        let doc = state.document_mut().ok_or("没有文档")?;
        let initial = view(doc)?;
        apply(doc, &initial, InputIntent::Select(2..2))?;
        let queued = ScheduledInput::capture(intent, 2, &initial, doc);
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        let InputStep::Prepared(job) = initial.step(
            &InputIntent::Delete {
                forward: false,
                count: 1,
            },
            &identity,
            &current,
            doc,
            1,
        ) else {
            return Err("删除应冻结为后台 job".into());
        };
        doc.edit_core()?.publish_native_input(job.run()?)?;
        let version = doc.core().version();
        let snapshot = doc.current_snapshot()?.snapshot_id().clone();
        let after = view(doc)?;
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        assert!(matches!(
            queued.step(&after, &identity, &current, doc),
            InputStep::StalePlatformOrigin
        ));
        assert!(
            !queued.platform_is_current(doc),
            "旧平台目标不得获得重试资格"
        );
        assert_eq!(doc.core().buffer_text()?, "acd");
        assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "acd");
        assert_eq!(doc.core().version(), version);
        assert_eq!(doc.current_snapshot()?.snapshot_id(), &snapshot);
        assert_eq!(doc.core().current_selection(), SelectionSpec::new(1, 1));
        assert_eq!(doc.core().undo_depth(), 1);
        assert_eq!(doc.core().redo_depth(), 0);
        assert_eq!(doc.core().history_node_count(), 1);
        assert!(doc.core().composition().is_none());
        assert!(doc.core().open_block_draft().is_none());
    }
    Ok(())
}

#[test]
fn platform_replacement_does_not_follow_a_changed_selection_in_the_same_snapshot() -> TestResult {
    let mut state = session("abcd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..2))?;
    let queued =
        ScheduledInput::capture(InputIntent::Replace(None, "x".to_owned()), 2, &initial, doc);
    // 无范围普通输入是语义键入，当前选区仍由调用顺序决定。
    assert!(queued.platform.is_none());
    let bound = ScheduledInput::capture(
        InputIntent::Replace(Some(1..2), "x".to_owned()),
        2,
        &initial,
        doc,
    );
    let before = doc.core().version();
    apply(doc, &initial, InputIntent::Select(2..3))?;
    assert_eq!(doc.core().version(), before, "选区移动不推进内容版本");
    let after = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    assert!(matches!(
        bound.step(&after, &identity, &current, doc),
        InputStep::StalePlatformOrigin
    ));
    assert!(!bound.platform_is_current(doc));
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(2, 3));
    assert_eq!(doc.core().buffer_text()?, "abcd");
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn platform_selection_uses_its_original_mapping_when_only_the_frame_was_rebuilt() -> TestResult {
    let mut state = session("abcd", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let version = doc.core().version();
    let initial = view(doc)?;
    let queued = ScheduledInput::capture(InputIntent::Select(2..3), 2, &initial, doc);
    // 同一源码快照重新绘制会创建不同投影；平台数值仍由被捕获的原投影解释。
    let after = view(doc)?;
    assert_ne!(
        initial.coordinates.projection_id(),
        after.coordinates.projection_id()
    );
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    assert!(matches!(
        queued.step(&after, &identity, &current, doc),
        InputStep::Applied { .. }
    ));
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(2, 3));
    assert_eq!(doc.core().buffer_text()?, "abcd");
    assert_eq!(doc.core().version(), version);
    assert_eq!(doc.core().history_node_count(), 0);
    Ok(())
}

#[test]
fn composition_start_after_own_seal_continues_only_the_trigger_callback() -> TestResult {
    for (mode, range) in [
        (EditorMode::Source, None),
        (EditorMode::Source, Some(2..2)),
        (EditorMode::Visual, None),
        (EditorMode::Visual, Some(2..2)),
    ] {
        let mut state = session("ab", HistoryBudget::new(8, 4096), mode)?;
        let doc = state.document_mut().ok_or("没有文档")?;
        let initial = view(doc)?;
        apply(doc, &initial, InputIntent::Select(1..1))?;
        apply(doc, &initial, InputIntent::Replace(None, "x".to_owned()))?;
        let drafted = view(doc)?;
        let mut trigger = ScheduledInput::capture(
            InputIntent::Mark(range, "n".to_owned(), Some(1..1)),
            2,
            &drafted,
            doc,
        );
        let queued = ScheduledInput::capture(InputIntent::Select(0..1), 3, &drafted, doc);
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        let InputStep::Sealed(job) = trigger.step(&drafted, &identity, &current, doc) else {
            return Err("组合起始必须先封口普通输入组".into());
        };
        doc.edit_core()?.publish_native_input(job.run()?)?;
        assert_eq!(doc.core().buffer_text()?, "axb");
        assert_eq!(doc.core().current_selection(), SelectionSpec::new(2, 2));
        assert_eq!(doc.core().undo_depth(), 1);
        assert!(!trigger.platform_is_current(doc));
        assert!(!queued.platform_is_current(doc));
        let frame = InputFrame(Rc::new(RefCell::new(Some(view(doc)?))));
        assert!(trigger.after_own_seal(Some(&frame), doc));
        assert!(trigger.platform_is_current(doc));
        assert!(!queued.platform_is_current(doc), "封口只更新触发回调的资格");
        let after = view(doc)?;
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        assert!(matches!(
            trigger.step(&after, &identity, &current, doc),
            InputStep::Applied { changed: true, .. }
        ));
        let composition = doc.core().composition().ok_or("触发回调应开启组合")?;
        assert_eq!(composition.range(), 2..2);
        assert_eq!(composition.candidate(), "n");
        let cancelled = view(doc)?;
        apply(
            doc,
            &cancelled,
            InputIntent::Mark(None, String::new(), Some(0..0)),
        )?;
        assert!(doc.core().composition().is_none());
        assert_eq!(doc.core().buffer_text()?, "axb");
        assert_eq!(doc.core().undo_depth(), 1);
        assert_eq!(doc.core().redo_depth(), 0);
    }
    Ok(())
}

#[test]
fn late_composition_callbacks_do_not_target_the_next_round_after_confirmation() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(
        doc,
        &initial,
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)),
    )?;
    let composing = view(doc)?;
    let confirm = ScheduledInput::capture(
        InputIntent::Replace(None, "你".to_owned()),
        2,
        &composing,
        doc,
    );
    let late = [
        ScheduledInput::capture(
            InputIntent::Replace(None, "好".to_owned()),
            3,
            &composing,
            doc,
        ),
        ScheduledInput::capture(
            InputIntent::Mark(None, "h".to_owned(), Some(1..1)),
            3,
            &composing,
            doc,
        ),
    ];
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let InputStep::Prepared(job) = confirm.step(&composing, &identity, &current, doc) else {
        return Err("组合确认必须准备后台 job".into());
    };
    doc.edit_core()?.publish_native_input(job.run()?)?;
    let version = doc.core().version();
    assert_eq!(doc.core().buffer_text()?, "你ab");
    for input in late {
        let after = view(doc)?;
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        assert!(matches!(
            input.step(&after, &identity, &current, doc),
            InputStep::StalePlatformOrigin
        ));
        assert!(!input.platform_is_current(doc), "旧组合目标不允许后台重试");
        assert_eq!(doc.core().buffer_text()?, "你ab");
        assert_eq!(doc.core().version(), version);
        assert_eq!(doc.core().current_selection(), SelectionSpec::new(3, 3));
        assert_eq!(doc.core().undo_depth(), 1);
        assert!(doc.core().composition().is_none());
    }
    // 发布后新捕获的一轮组合可以正常开始，不能被旧来源拒收清空。
    let after = view(doc)?;
    let next = ScheduledInput::capture(
        InputIntent::Mark(None, "h".to_owned(), Some(1..1)),
        4,
        &after,
        doc,
    );
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    assert!(matches!(
        next.step(&after, &identity, &current, doc),
        InputStep::Applied { changed: true, .. }
    ));
    let composition = doc.core().composition().ok_or("新一轮组合应仍有效")?;
    assert_eq!(composition.range(), 3..3);
    assert_eq!(composition.candidate(), "h");
    let cancelled = view(doc)?;
    apply(
        doc,
        &cancelled,
        InputIntent::Mark(None, String::new(), Some(0..0)),
    )?;
    assert_eq!(doc.core().buffer_text()?, "你ab");
    assert_eq!(doc.core().undo_depth(), 1);
    Ok(())
}

#[test]
fn sync_fallback_keeps_composing_frame_only_while_composition_is_live() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let frame = InputFrame(Rc::new(RefCell::new(Some(view(doc)?))));
    let current = doc.current_snapshot()?;
    {
        let active = frame.0.borrow();
        let view = active.as_ref().ok_or("缺少输入投影")?;
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)).apply(view, &current, doc, 1)?;
    }
    assert!(doc.core().composition().is_some(), "同步应用后组合应仍活动");
    assert!(
        composing_frame(Some(&frame), doc.core().composition().is_some()).is_some(),
        "组合活动时同步退化必须保留离屏帧"
    );
    assert!(
        composing_frame(Some(&frame), false).is_none(),
        "组合结束后必须释放离屏帧"
    );
    assert!(composing_frame(None, true).is_none());
    Ok(())
}

#[test]
fn degraded_group_split_uses_synchronous_commit_and_parses() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    let typing = view(doc)?;
    apply(doc, &typing, InputIntent::Replace(None, "x".to_owned()))?;
    let drafted = view(doc)?;
    apply(doc, &drafted, InputIntent::Select(1..2))?;
    let selected = view(doc)?;
    assert!(doc.core().open_block_draft().is_some());

    // 同步退化/显式命令在已有待定组上替换选区时仍走旧同步封口；
    // 该退化路径的代价是主线程解析，正常按键路径不经过它。
    let baseline = parse_call_count();
    apply(
        doc,
        &selected,
        InputIntent::Replace(Some(1..2), "y".to_owned()),
    )?;
    assert!(parse_call_count() > baseline, "旧同步封口必须在主线程解析");
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "ayb");
    assert_eq!(doc.core().history_node_count(), 1);
    Ok(())
}

#[test]
fn composition_confirmation_freezes_a_job_without_parsing() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(0..1))?;
    apply(
        doc,
        &initial,
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)),
    )?;
    let composing = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let baseline = parse_call_count();
    let InputStep::Prepared(job) = composing.step(
        &InputIntent::Replace(None, "你".to_owned()),
        &identity,
        &current,
        doc,
        3,
    ) else {
        return Err("组合确认应准备后台 job".into());
    };
    assert_eq!(parse_call_count(), baseline, "组合确认准备段不得解析");
    let prepared = job.run()?;
    let background = parse_call_count();
    doc.edit_core()?.publish_native_input(prepared)?;
    assert_eq!(parse_call_count(), background, "组合确认发布段不得解析");
    assert_eq!(doc.core().buffer_text()?, "你b");
    Ok(())
}

#[test]
fn scheduler_executes_idle_input_immediately_and_preserves_order() -> TestResult {
    let document = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let first = ScheduledInput::new(InputIntent::LineBreak, 1);
    let InputSubmission::Now(first) = scheduler.submit(&document, first) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_seal(first);
    let second = ScheduledInput::new(InputIntent::Replace(None, "a".to_owned()), 2);
    assert!(matches!(
        scheduler.submit(&document, second),
        InputSubmission::Queued
    ));
    let third = ScheduledInput::new(InputIntent::Replace(None, "b".to_owned()), 3);
    assert!(matches!(
        scheduler.submit(&document, third),
        InputSubmission::Queued
    ));
    assert_eq!(scheduler.pending_len(), 2);
    let Some(ScheduledInput {
        intent: InputIntent::Replace(_, text),
        now_millis: 2,
        retried: false,
        ..
    }) = scheduler.take_pending()
    else {
        return Err("队首输入缺失或顺序错误".into());
    };
    assert_eq!(text, "a");
    let Some(ScheduledInput {
        intent: InputIntent::Replace(_, text),
        now_millis: 3,
        ..
    }) = scheduler.take_pending()
    else {
        return Err("次条输入缺失或顺序错误".into());
    };
    assert_eq!(text, "b");
    Ok(())
}

#[test]
fn consecutive_same_direction_deletes_merge_with_latest_time() -> TestResult {
    let document = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let first = ScheduledInput::new(
        InputIntent::Delete {
            forward: true,
            count: 1,
        },
        2,
    );
    let InputSubmission::Now(first) = scheduler.submit(&document, first) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_direct(first);
    scheduler.submit(
        &document,
        ScheduledInput::new(
            InputIntent::Delete {
                forward: true,
                count: 1,
            },
            3,
        ),
    );
    scheduler.submit(
        &document,
        ScheduledInput::new(
            InputIntent::Delete {
                forward: true,
                count: 2,
            },
            4,
        ),
    );
    scheduler.submit(
        &document,
        ScheduledInput::new(
            InputIntent::Delete {
                forward: false,
                count: 1,
            },
            5,
        ),
    );
    assert_eq!(scheduler.pending_len(), 2);
    let Some(ScheduledInput {
        intent: InputIntent::Delete {
            forward: true,
            count,
        },
        now_millis: 4,
        ..
    }) = scheduler.take_pending()
    else {
        return Err("同向删除未合并或时刻未更新".into());
    };
    assert_eq!(count, 3);
    let Some(ScheduledInput {
        intent: InputIntent::Delete { forward: false, .. },
        ..
    }) = scheduler.take_pending()
    else {
        return Err("反向删除不得并入前一组".into());
    };
    Ok(())
}

#[test]
fn seal_requeue_replays_the_trigger_before_queued_input() -> TestResult {
    let document = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let trigger = ScheduledInput::new(InputIntent::Replace(None, "触".to_owned()), 901);
    let InputSubmission::Now(trigger) = scheduler.submit(&document, trigger) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_seal(trigger);
    scheduler.submit(&document, ScheduledInput::new(InputIntent::LineBreak, 902));
    let Some(active) = scheduler.take_active() else {
        return Err("封口 job 缺少在途记录".into());
    };
    assert!(active.is_seal());
    scheduler.requeue_front(active.into_input());
    assert_eq!(scheduler.pending_len(), 2);
    let Some(ScheduledInput {
        intent: InputIntent::Replace(_, text),
        now_millis: 901,
        ..
    }) = scheduler.take_pending()
    else {
        return Err("触发意图未回到队首".into());
    };
    assert_eq!(text, "触");
    assert!(matches!(
        scheduler.take_pending(),
        Some(ScheduledInput {
            intent: InputIntent::LineBreak,
            ..
        })
    ));
    Ok(())
}

#[test]
fn stale_trigger_retries_once_with_the_original_key_time() -> TestResult {
    let document = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let trigger = ScheduledInput::new(InputIntent::Replace(None, "x".to_owned()), 900);
    let InputSubmission::Now(trigger) = scheduler.submit(&document, trigger) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_seal(trigger);
    let Some(mut active) = scheduler.take_active() else {
        return Err("封口 job 缺少在途记录".into());
    };
    active.mark_retried();
    let retry = active.into_input();
    scheduler.start_seal(retry);
    let Some(active) = scheduler.take_active() else {
        return Err("重试输入缺少在途记录".into());
    };
    let ScheduledInput {
        now_millis,
        retried,
        ..
    } = active.into_input();
    assert_eq!(now_millis, 900, "重放必须保留按键时刻");
    assert!(retried, "重试必须带标记，避免无限重试");
    Ok(())
}

#[test]
fn failure_retries_once_then_seal_falls_back_with_the_queue_intact() -> TestResult {
    let document = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let trigger = ScheduledInput::new(InputIntent::Replace(None, "触".to_owned()), 901);
    let InputSubmission::Now(trigger) = scheduler.submit(&document, trigger) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_seal(trigger);
    scheduler.submit(&document, ScheduledInput::new(InputIntent::LineBreak, 902));
    let Some(mut active) = scheduler.take_active() else {
        return Err("封口 job 缺少在途记录".into());
    };
    assert!(matches!(
        scheduler.on_failure(&mut active, InputFailure::OpenDocument, true),
        Recovery::Retry
    ));
    assert!(active.input().retried, "首次失败必须先重试一次");
    assert_eq!(scheduler.pending_len(), 1, "重试期间等待队列不得丢弃");
    assert!(matches!(
        scheduler.on_failure(&mut active, InputFailure::OpenDocument, true),
        Recovery::SyncFallback
    ));
    assert_eq!(scheduler.pending_len(), 1, "同步退化必须保留队列");
    Ok(())
}

#[test]
fn failure_twice_clears_a_direct_input_and_the_queue() -> TestResult {
    let document = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let trigger = ScheduledInput::new(
        InputIntent::Delete {
            forward: true,
            count: 1,
        },
        3,
    );
    let InputSubmission::Now(trigger) = scheduler.submit(&document, trigger) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_direct(trigger);
    scheduler.submit(&document, ScheduledInput::new(InputIntent::LineBreak, 4));
    let Some(mut active) = scheduler.take_active() else {
        return Err("独立 job 缺少在途记录".into());
    };
    assert!(matches!(
        scheduler.on_failure(&mut active, InputFailure::OpenDocument, true),
        Recovery::Retry
    ));
    assert_eq!(scheduler.pending_len(), 1);
    assert!(matches!(
        scheduler.on_failure(&mut active, InputFailure::OpenDocument, true),
        Recovery::Clear
    ));
    assert_eq!(scheduler.pending_len(), 0, "清空必须同时丢弃等待队列");
    Ok(())
}

#[test]
fn document_change_discards_active_and_pending_input() -> TestResult {
    let first = DocumentId::new();
    let second = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let input = ScheduledInput::new(InputIntent::LineBreak, 1);
    let InputSubmission::Now(input) = scheduler.submit(&first, input) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_direct(input);
    scheduler.submit(&first, ScheduledInput::new(InputIntent::LineBreak, 2));
    assert!(scheduler.is_active());
    assert_eq!(scheduler.pending_len(), 1);
    let next = ScheduledInput::new(InputIntent::LineBreak, 3);
    assert!(matches!(
        scheduler.submit(&second, next),
        InputSubmission::Now(_)
    ));
    assert!(!scheduler.is_active());
    assert_eq!(scheduler.pending_len(), 0);
    assert_eq!(scheduler.document(), Some(&second));
    Ok(())
}

#[test]
fn explicit_undo_during_in_flight_seal_rebinds_and_replays_input_in_order() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;

    // 组内输入进入待定草稿；900ms 边界冻结封口 job，此时旧组尚未发布。
    let typing = view(doc)?;
    apply(doc, &typing, InputIntent::Replace(None, "x".to_owned()))?;
    let drafted = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let trigger = ScheduledInput::new(InputIntent::Replace(None, "y".to_owned()), 902);
    let InputStep::Sealed(job) = drafted.step(
        &trigger.intent,
        &identity,
        &current,
        doc,
        trigger.now_millis,
    ) else {
        return Err("900ms 边界应冻结封口 job".into());
    };
    let frame = InputFrame(Rc::new(RefCell::new(Some(drafted))));
    let document = doc.core().document_id().clone();
    let mut scheduler = InputScheduler::new();
    let InputSubmission::Now(trigger) = scheduler.submit(&document, trigger) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_seal(trigger);
    let queued = ScheduledInput::new(InputIntent::Replace(None, "z".to_owned()), 903);
    assert!(
        matches!(scheduler.submit(&document, queued), InputSubmission::Queued),
        "在途封口期间的新按键必须排队"
    );

    // 菜单 Undo 不经过 InputScheduler：撤销待定组并推进工作版本。
    doc.edit_core()?.undo()?;
    assert!(
        doc.core().open_block_draft().is_none(),
        "撤销应清除待定输入组"
    );
    let Some(mut active) = scheduler.take_active() else {
        return Err("封口 job 缺少在途记录".into());
    };
    assert!(active.is_seal(), "在途记录应保持封口类别");
    let prepared = job.run()?;
    assert!(
        matches!(
            doc.edit_core()?.publish_native_input(prepared),
            Err(DocError::StaleResult)
        ),
        "显式命令推进版本后，在途封口结果必须判为过期"
    );
    assert!(
        matches!(
            scheduler.on_failure(&mut active, InputFailure::OpenDocument, true),
            Recovery::Retry
        ),
        "在途封口首次失败应先重试一次"
    );
    let trigger = active.into_input();

    // I-1 复现前提：旧帧在命令后的新版本上不可消费。
    let refused = {
        let active = frame.0.borrow();
        let view = active.as_ref().ok_or("缺少输入投影")?;
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        view.step(
            &trigger.intent,
            &identity,
            &current,
            doc,
            trigger.now_millis,
        )
    };
    assert!(
        matches!(refused, InputStep::Inactive),
        "命令推进版本后旧输入帧必须失效"
    );

    // 与 retry_or_fallback/replay_pending_sync 一致：取帧前先按最新状态重建。
    assert!(frame.rebind(doc), "命令之后必须能按最新文档状态重建输入帧");
    let applied = {
        let active = frame.0.borrow();
        let view = active.as_ref().ok_or("重建后缺少输入投影")?;
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        view.step(
            &trigger.intent,
            &identity,
            &current,
            doc,
            trigger.now_millis,
        )
    };
    assert!(
        matches!(applied, InputStep::Applied { changed: true, .. }),
        "命令之后触发键必须重新应用"
    );
    assert_eq!(
        doc.core().open_block_draft(),
        Some("y"),
        "触发键应在命令之后进入新草稿"
    );

    let Some(queued) = scheduler.take_pending() else {
        return Err("排队输入缺失".into());
    };
    // 触发键推进版本后，排队输入前同样要重建帧（drain_inputs 的取帧路径）。
    assert!(frame.rebind(doc), "触发键应用后必须能按新版本重建输入帧");
    let applied = {
        let active = frame.0.borrow();
        let view = active.as_ref().ok_or("重建后缺少输入投影")?;
        let identity = doc.input_identity()?;
        let current = doc.current_snapshot()?;
        view.step(&queued.intent, &identity, &current, doc, queued.now_millis)
    };
    assert!(
        matches!(applied, InputStep::Applied { changed: true, .. }),
        "命令前已排队的按键必须按序应用"
    );
    assert_eq!(scheduler.pending_len(), 0, "等待队列必须消费完毕");

    // 文本与历史可解释：撤销只清除未提交组；重放的两键组成单个新历史节点。
    assert!(
        matches!(
            doc.edit_core()?.commit_open_block(904)?,
            CommitOutcome::Committed { .. }
        ),
        "重放输入应作为单个历史节点提交"
    );
    assert_eq!(doc.core().buffer_text()?, "ayzb");
    assert_eq!(doc.core().current_selection(), SelectionSpec::new(3, 3));
    assert_eq!(doc.core().history_node_count(), 1, "重放不得双提交旧组");
    assert_eq!(doc.core().undo_depth(), 1);
    assert_eq!(doc.core().redo_depth(), 0);
    Ok(())
}

#[test]
fn mode_switch_during_in_flight_seal_cannot_rebind_and_clears_the_frame() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    let typing = view(doc)?;
    apply(doc, &typing, InputIntent::Replace(None, "x".to_owned()))?;
    let drafted = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let trigger = ScheduledInput::new(InputIntent::Replace(None, "y".to_owned()), 902);
    let InputStep::Sealed(job) = drafted.step(
        &trigger.intent,
        &identity,
        &current,
        doc,
        trigger.now_millis,
    ) else {
        return Err("900ms 边界应冻结封口 job".into());
    };
    let frame = InputFrame(Rc::new(RefCell::new(Some(drafted))));

    // 显式模式切换不经过调度：先提交待定组并改变模式，旧帧因此不可重建。
    assert_eq!(
        doc.switch_mode(EditorMode::Visual)?,
        ModeSwitchOutcome::Ready,
        "非组合期的模式切换应直接就绪"
    );
    let prepared = job.run()?;
    assert!(
        matches!(
            doc.edit_core()?.publish_native_input(prepared),
            Err(DocError::StaleResult)
        ),
        "模式切换后，在途封口结果必须判为过期"
    );

    assert!(
        !frame.rebind(doc),
        "跨模式无法重建投影，窗口必须走明确反馈而不是重放旧帧"
    );
    assert!(
        frame.0.borrow().is_none(),
        "重建失败必须清空帧，避免后续按键继续消费失效投影"
    );
    // 丢弃按键不改变已提交文本与历史：模式切换只提交一次待定组。
    assert_eq!(doc.core().buffer_text()?, "axb");
    assert_eq!(doc.core().history_node_count(), 1);
    assert_eq!(doc.core().open_block_draft(), None);
    Ok(())
}

#[test]
fn composition_end_classifies_platform_outcome() {
    let empty_mark = InputIntent::Mark(None, String::new(), None);
    let filled_mark = InputIntent::Mark(None, "候".to_owned(), None);
    let confirmation = InputIntent::Replace(None, "候".to_owned());
    assert_eq!(
        composition_end(&empty_mark, true, false),
        Some(PublicationOutcome::PlatformCancelled),
        "空候选 Mark 走 composition_cancel，应判定为平台取消"
    );
    assert_eq!(
        composition_end(&filled_mark, true, false),
        Some(PublicationOutcome::PlatformConfirmed),
        "非空 Mark 结束组合应判定为平台确认"
    );
    assert_eq!(
        composition_end(&confirmation, true, false),
        Some(PublicationOutcome::PlatformConfirmed),
        "IME 替换确认应判定为平台确认"
    );
    assert_eq!(
        composition_end(&filled_mark, true, true),
        None,
        "组合仍活动时不得产生结束结果"
    );
    assert_eq!(
        composition_end(&filled_mark, false, false),
        None,
        "结算前无组合时不得产生结束结果"
    );
}

#[test]
fn failure_recovery_drops_stale_platform_trigger_and_keeps_semantic_queue() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "x".to_owned()))?;
    let drafted = view(doc)?;
    let trigger = ScheduledInput::capture(
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)),
        2,
        &drafted,
        doc,
    );
    let queued_select = ScheduledInput::capture(InputIntent::Select(0..1), 3, &drafted, doc);
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let InputStep::Sealed(job) = trigger.step(&drafted, &identity, &current, doc) else {
        return Err("组合起始必须先封口普通输入组".into());
    };
    doc.edit_core()?.publish_native_input(job.run()?)?;
    assert!(!trigger.platform_is_current(doc), "封口后触发来源已失效");
    assert!(!queued_select.platform_is_current(doc));

    let mut scheduler = InputScheduler::new();
    let document = doc.core().document_id().clone();
    let InputSubmission::Now(seed) = scheduler.submit(&document, trigger) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_seal(seed);
    scheduler.submit(&document, queued_select);
    scheduler.submit(
        &document,
        ScheduledInput::new(InputIntent::Replace(None, "y".to_owned()), 4),
    );
    let active = scheduler.take_active().ok_or("缺少在途封口")?;
    let platform_current = active.input().platform_is_current(doc);
    let action =
        scheduler.plan_failure_recovery(active, InputFailure::OpenDocument, platform_current);
    assert!(matches!(action, FailureAction::DropStaleOrigin));
    assert_eq!(scheduler.pending_len(), 2, "普通排水仍须收到两条等待输入");
    assert!(!scheduler.is_active());

    let current_view = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let stale = scheduler.take_pending().ok_or("排水缺少平台选区动作")?;
    assert!(matches!(
        stale.step(&current_view, &identity, &current, doc),
        InputStep::StalePlatformOrigin
    ));
    let queued = scheduler.take_pending().ok_or("排水缺少后续语义输入")?;
    assert!(matches!(
        queued.step(&current_view, &identity, &current, doc),
        InputStep::Applied { changed: true, .. }
    ));
    assert_eq!(doc.current_snapshot()?.full_text(TEXT)?, "axyb");
    assert_eq!(scheduler.pending_len(), 0);
    Ok(())
}

#[test]
fn failure_recovery_retry_then_sync_fallback_and_clear_are_unchanged() -> TestResult {
    let document = DocumentId::new();
    let mut scheduler = InputScheduler::new();
    let trigger = ScheduledInput::new(InputIntent::Replace(None, "触".to_owned()), 901);
    let InputSubmission::Now(trigger) = scheduler.submit(&document, trigger) else {
        return Err("空闲调度应立即可执行".into());
    };
    scheduler.start_seal(trigger);
    scheduler.submit(&document, ScheduledInput::new(InputIntent::LineBreak, 902));
    let active = scheduler.take_active().ok_or("缺少在途封口")?;
    let action = scheduler.plan_failure_recovery(active, InputFailure::OpenDocument, true);
    assert!(matches!(action, FailureAction::Retry(_)));
    // 已重试的封口再失败 → 同步退化；已重试的直接输入再失败 → 清空。
    let mut retried = ScheduledInput::new(InputIntent::LineBreak, 903);
    retried.retried = true;
    let mut seal = ActiveInput::Seal(retried.clone());
    assert!(matches!(
        scheduler.on_failure(&mut seal, InputFailure::OpenDocument, true),
        Recovery::SyncFallback
    ));
    let mut direct = ActiveInput::Direct(retried.clone());
    scheduler.submit(&document, ScheduledInput::new(InputIntent::LineBreak, 904));
    assert!(matches!(
        scheduler.on_failure(&mut direct, InputFailure::OpenDocument, true),
        Recovery::Clear
    ));
    assert_eq!(scheduler.pending_len(), 0, "直接输入再失败清空等待队列");
    // 文档已不存在：无论来源与否都清空且不重试。
    let mut gone = ActiveInput::Direct(retried);
    gone.mark_retried();
    let mut fresh = ActiveInput::Seal(ScheduledInput::new(InputIntent::LineBreak, 905));
    assert!(matches!(
        scheduler.on_failure(&mut fresh, InputFailure::DocumentGone, true),
        Recovery::Clear
    ));
    Ok(())
}

#[test]
fn after_own_seal_refuses_when_selection_or_site_changed() -> TestResult {
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "x".to_owned()))?;
    let drafted = view(doc)?;
    let mut trigger = ScheduledInput::capture(
        InputIntent::Mark(None, "n".to_owned(), Some(1..1)),
        2,
        &drafted,
        doc,
    );
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    let InputStep::Sealed(job) = trigger.step(&drafted, &identity, &current, doc) else {
        return Err("组合起始必须先封口普通输入组".into());
    };
    doc.edit_core()?.publish_native_input(job.run()?)?;
    // 封口发布后、重放前选区被移动：触发来源不得续用。
    let moved = view(doc)?;
    apply(doc, &moved, InputIntent::Select(0..0))?;
    let frame = InputFrame(Rc::new(RefCell::new(Some(view(doc)?))));
    assert!(
        !trigger.after_own_seal(Some(&frame), doc),
        "选区变化后不得续用触发回调"
    );
    assert!(!trigger.platform_is_current(doc));
    // 帧缺失同样拒绝续用。
    assert!(!trigger.after_own_seal(None, doc));
    Ok(())
}

#[test]
fn core_stale_result_rejection_keeps_edit_refused_feedback() -> TestResult {
    // 来源复验通过但核心受检入口拒绝（StaleResult）时，不得被平台来源
    // 静默分支吞掉：该错误仍映射为编辑拒绝反馈。
    assert_eq!(
        edit_error_key(DocError::StaleResult),
        "document.edit_refused"
    );
    let mut state = session("ab", HistoryBudget::new(8, 4096), EditorMode::Source)?;
    let doc = state.document_mut().ok_or("没有文档")?;
    let initial = view(doc)?;
    apply(doc, &initial, InputIntent::Select(1..1))?;
    apply(doc, &initial, InputIntent::Replace(None, "x".to_owned()))?;
    let drafted = view(doc)?;
    let input = ScheduledInput::capture(InputIntent::Select(0..1), 2, &drafted, doc);
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    // 来源有效：Select 正常应用，不产生 StalePlatformOrigin。
    assert!(matches!(
        input.step(&drafted, &identity, &current, doc),
        InputStep::Applied { .. }
    ));
    // 来源失效（显式命令推进版本后选区移动）：只产生来源专用拒收。
    doc.edit_core()?.type_text(1, "q", 5)?;
    let stale = ScheduledInput::capture(InputIntent::Select(0..1), 3, &drafted, doc);
    let rebuilt = view(doc)?;
    let identity = doc.input_identity()?;
    let current = doc.current_snapshot()?;
    assert!(matches!(
        stale.step(&rebuilt, &identity, &current, doc),
        InputStep::StalePlatformOrigin
    ));
    Ok(())
}
