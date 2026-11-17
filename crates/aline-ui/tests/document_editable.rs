//! M2 无窗口编辑夹具，从真实解析范围和当前源码身份驱动文档所有者。

mod common;

use aline_buffer::{SelectionSpec, SrcSelection};
use aline_markdown::{ParseBudget, ParseConfig, parse};
use aline_ui::{DocCore, DocError, EditorMode};
use common::{TEXT, assert_snapshot, capture, collapsed, limits};

type TestResult = Result<(), Box<dyn std::error::Error>>;

#[test]
fn editable_document_switches_before_opening_and_preserves_directional_selection() -> TestResult {
    let mut doc = DocCore::new_editable("甲乙\r\n\r\n尾", SelectionSpec::new(6, 3), limits())?;
    let version = doc.version();
    let original = doc.current_input_identity()?;
    assert!(!doc.has_open_block());
    doc.switch_mode(EditorMode::Source, 0)?;
    doc.switch_mode(EditorMode::Visual, 1)?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(6, 3));
    assert_eq!(doc.version(), version);
    assert_eq!(doc.history_node_count(), 0);
    assert!(matches!(
        doc.accept_document_result(&original),
        Err(DocError::StaleResult)
    ));
    assert_snapshot(&mut doc, "甲乙\r\n\r\n尾")?;
    Ok(())
}

#[test]
fn editable_visual_block_keeps_block_extent_across_groups_and_excludes_separators() -> TestResult {
    let mut doc = DocCore::new_editable("甲乙\r\n\r\n尾", collapsed(0), limits())?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(
        snapshot.text().src_offset(3)?,
        snapshot.text().src_offset(3)?,
    )?;
    let input = doc.current_input_identity()?;
    let block = parsed.blocks().first().ok_or("正文夹具应有块")?;
    assert_eq!(block.range().byte_range(), 0..6);
    doc.open_visual_block(&input, &parsed, block.range(), &selection)?;
    assert!(matches!(
        doc.open_visual_block(&input, &parsed, block.range(), &selection),
        Err(DocError::BlockAlreadyOpen)
    ));
    doc.type_text(3, "中", 0)?;
    doc.type_text(6, "文", 901)?;
    assert_eq!(doc.undo_depth(), 1);
    assert!(matches!(
        doc.type_text(13, "外", 902),
        Err(DocError::OutsideOpenBlock)
    ));
    assert_snapshot(&mut doc, "甲中文乙\r\n\r\n尾")?;
    doc.escape(903)?;
    assert!(!doc.has_open_block());
    assert_eq!(doc.buffer_text()?, "甲中文乙\r\n\r\n尾");
    assert_eq!(doc.undo_depth(), 2);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲中乙\r\n\r\n尾");
    doc.undo()?;
    assert_eq!(doc.current_selection(), collapsed(3));
    assert_eq!(doc.buffer_text()?, "甲乙\r\n\r\n尾");
    doc.redo()?;
    doc.redo()?;
    assert_eq!(doc.current_selection(), collapsed(9));
    Ok(())
}

#[test]
fn editable_source_cut_paste_and_undo_share_selection_and_history() -> TestResult {
    let mut doc = DocCore::new_editable("甲\r\n\r\n尾", collapsed(0), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let input = doc.current_input_identity()?;
    doc.select_all(&input)?;
    assert_eq!(doc.copy_selection(TEXT)?, "甲\r\n\r\n尾");
    assert_eq!(doc.cut_selection(&input, TEXT, 1)?, "甲\r\n\r\n尾");
    assert_eq!(doc.buffer_text()?, "");
    assert!(matches!(
        doc.replace_selection(&input, "stale", 2),
        Err(DocError::StaleResult)
    ));
    let input = doc.current_input_identity()?;
    doc.replace_selection(&input, "新\r\n", 3)?;
    assert_eq!(doc.buffer_text()?, "新\r\n");
    doc.switch_mode(EditorMode::Visual, 4)?;
    doc.undo()?;
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲\r\n\r\n尾");
    assert_eq!(doc.current_selection(), SelectionSpec::new(0, 10));
    doc.redo()?;
    doc.redo()?;
    assert_eq!(doc.current_selection(), collapsed(5));
    Ok(())
}

#[test]
fn editable_source_selects_and_copies_latest_draft_without_sealing_or_reidentifying_it()
-> TestResult {
    let mut doc = DocCore::new_editable("甲乙", collapsed(3), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let input = doc.current_input_identity()?;
    doc.type_selection(&input, "中", 1)?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let input = doc.current_input_identity()?;
    let selection = SrcSelection::new(
        snapshot.text().src_offset(6)?,
        snapshot.text().src_offset(3)?,
    )?;
    doc.select(&input, &selection)?;
    assert_eq!(doc.current_input_identity()?, input);
    assert_eq!(doc.current_snapshot(TEXT)?.selection(), &selection);
    assert_eq!(doc.copy_selection(TEXT)?, "中");
    let before = capture(&mut doc)?;
    assert_eq!(
        doc.replace_selection(&input, "中", 2)?,
        aline_ui::CommitOutcome::Unchanged
    );
    assert_eq!(capture(&mut doc)?, before);
    doc.undo()?;
    assert_eq!(doc.current_selection(), collapsed(3));
    doc.redo()?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(6, 3));
    assert_snapshot(&mut doc, "甲中乙")?;
    Ok(())
}

#[test]
fn editable_source_mode_switch_commits_input_and_does_not_leave_a_visual_editor() -> TestResult {
    let mut doc = DocCore::new_editable("甲乙", collapsed(3), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let input = doc.current_input_identity()?;
    doc.type_selection(&input, "中", 1)?;
    doc.switch_mode(EditorMode::Visual, 2)?;
    assert_eq!(doc.buffer_text()?, "甲中乙");
    assert_eq!(doc.current_selection(), collapsed(6));
    assert_eq!(doc.undo_depth(), 1);
    assert!(!doc.has_open_block());
    let identity = doc.current_input_identity()?;
    assert!(matches!(
        doc.type_selection(&identity, "新", 3),
        Err(DocError::NoOpenBlock)
    ));
    assert_eq!(doc.undo_depth(), 1);
    Ok(())
}

#[test]
fn editable_visual_refuses_protected_or_stale_block_before_opening() -> TestResult {
    let raw = "---\ntitle: example\n---\n\n正文";
    let mut doc = DocCore::new_editable(raw, collapsed(0), limits())?;
    let current = doc.current_snapshot(TEXT)?;
    let parsed = parse(current.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let input = doc.current_input_identity()?;
    let first = parsed.blocks().first().ok_or("夹具应有 Front matter")?;
    let caret = SrcSelection::new(current.text().src_offset(4)?, current.text().src_offset(4)?)?;
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.open_visual_block(&input, &parsed, first.range(), &caret),
        Err(DocError::Edit(aline_edit::EditRefusal::ProtectedRange {
            kind: aline_markdown::ProtectionKind::FrontMatter,
            ..
        }))
    ));
    assert_eq!(capture(&mut doc)?, before);
    let mut replacement = DocCore::new_editable(raw, collapsed(0), limits())?;
    let replacement_before = capture(&mut replacement)?;
    assert!(matches!(
        replacement.open_visual_block(&input, &parsed, first.range(), &caret),
        Err(DocError::WrongDocument)
    ));
    assert_eq!(capture(&mut replacement)?, replacement_before);
    Ok(())
}

#[test]
fn editable_visual_open_block_accepts_raw_markdown_and_structural_newlines() -> TestResult {
    let mut doc = DocCore::new_editable("首尾\r\n\r\n下", collapsed(3), limits())?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let block = parsed.blocks().first().ok_or("正文夹具应有块")?;
    let input = doc.current_input_identity()?;
    doc.open_visual_block(&input, &parsed, block.range(), snapshot.selection())?;
    doc.type_selection(&input, "**中** &amp;\r\n\r\n", 1)?;
    doc.escape(2)?;
    assert_eq!(doc.buffer_text()?, "首**中** &amp;\r\n\r\n尾\r\n\r\n下");
    assert_eq!(doc.undo_depth(), 1);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "首尾\r\n\r\n下");
    Ok(())
}

#[test]
fn editable_visual_input_rollover_rebinds_to_newly_parsed_block() -> TestResult {
    let mut doc = DocCore::new_editable("甲乙", collapsed(3), limits())?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let block = parsed.blocks().first().ok_or("夹具应有正文")?;
    let identity = doc.current_input_identity()?;
    doc.open_visual_block(&identity, &parsed, block.range(), snapshot.selection())?;
    doc.type_selection(&identity, "\r\n\r\n", 0)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "中", 901)?;
    assert_eq!(doc.buffer_text()?, "甲\r\n\r\n乙");
    assert_eq!(doc.undo_depth(), 1);
    doc.escape(902)?;
    assert_eq!(doc.buffer_text()?, "甲\r\n\r\n中乙");
    assert_eq!(doc.undo_depth(), 2);
    Ok(())
}

#[test]
fn editable_visual_input_rollover_in_gap_leaves_pending_group_intact() -> TestResult {
    let mut doc = DocCore::new_editable("甲", collapsed(3), limits())?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let block = parsed.blocks().first().ok_or("夹具应有正文")?;
    let identity = doc.current_input_identity()?;
    doc.open_visual_block(&identity, &parsed, block.range(), snapshot.selection())?;
    doc.type_selection(&identity, "\r\n\r\n", 0)?;
    let identity = doc.current_input_identity()?;
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.type_selection(&identity, "中", 901),
        Err(DocError::InvalidVisualBlock)
    ));
    assert_eq!(capture(&mut doc)?, before);
    doc.commit_open_block(902)?;
    assert_eq!(doc.buffer_text()?, "甲\r\n\r\n");
    assert!(!doc.has_open_block());
    assert_eq!(doc.current_selection(), collapsed(7));
    assert_eq!(doc.undo_depth(), 1);
    Ok(())
}

#[test]
fn editable_visual_cuts_plain_cross_block_selection_in_one_step() -> TestResult {
    let mut doc = DocCore::new_editable("甲乙\r\n\r\n丙丁", collapsed(0), limits())?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let selection = SrcSelection::new(
        snapshot.text().src_offset(13)?,
        snapshot.text().src_offset(3)?,
    )?;
    let identity = doc.current_input_identity()?;
    doc.select(&identity, &selection)?;
    assert_eq!(doc.copy_selection(TEXT)?, "乙\r\n\r\n丙");
    assert_eq!(doc.cut_selection(&identity, TEXT, 1)?, "乙\r\n\r\n丙");
    assert_eq!(doc.buffer_text()?, "甲丁");
    assert_eq!(doc.undo_depth(), 1);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲乙\r\n\r\n丙丁");
    assert_eq!(doc.current_selection(), SelectionSpec::new(13, 3));
    Ok(())
}

#[test]
fn editable_visual_extent_is_bound_to_current_snapshot_and_includes_the_whole_block() -> TestResult
{
    let mut doc = DocCore::new_editable("甲乙\r\n\r\n尾", collapsed(3), limits())?;
    let initial = doc.current_snapshot(TEXT)?;
    let parsed = parse(initial.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let block = parsed.blocks().first().ok_or("夹具应有正文")?;
    assert!(doc.visual_block_range(&initial)?.is_none());
    let identity = doc.current_input_identity()?;
    doc.open_visual_block(&identity, &parsed, block.range(), initial.selection())?;
    assert_eq!(
        doc.visual_block_range(&initial)?.as_ref(),
        Some(block.range())
    );
    doc.type_selection(&identity, "中", 0)?;
    assert!(matches!(
        doc.visual_block_range(&initial),
        Err(DocError::StaleResult)
    ));
    let current = doc.current_snapshot(TEXT)?;
    let extent = doc.visual_block_range(&current)?.ok_or("应保留打开块")?;
    assert_eq!(current.text().validate_range(&extent)?, 0..9);
    assert_eq!(current.text().slice(&extent, TEXT)?, "甲中乙");
    assert_eq!(doc.open_block_range(), Some(3..3));
    let mut other = DocCore::new_editable("甲中乙\r\n\r\n尾", collapsed(6), limits())?;
    assert!(matches!(
        doc.visual_block_range(&other.current_snapshot(TEXT)?),
        Err(DocError::WrongDocument)
    ));
    Ok(())
}

fn open_first_visual_block(doc: &mut DocCore) -> TestResult {
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let block = parsed.blocks().first().ok_or("夹具应有正文")?;
    let identity = doc.current_input_identity()?;
    doc.open_visual_block(&identity, &parsed, block.range(), snapshot.selection())?;
    Ok(())
}

fn select_bytes(doc: &mut DocCore, anchor: usize, head: usize) -> TestResult {
    let snapshot = doc.current_snapshot(TEXT)?;
    let selection = SrcSelection::new(
        snapshot.text().src_offset(anchor)?,
        snapshot.text().src_offset(head)?,
    )?;
    let identity = doc.current_input_identity()?;
    doc.select(&identity, &selection)?;
    Ok(())
}

#[test]
fn editable_visual_replaces_nonempty_selection_in_typing_group_and_keeps_block_open() -> TestResult
{
    let mut doc = DocCore::new_editable("左右", SelectionSpec::new(6, 3), limits())?;
    open_first_visual_block(&mut doc)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "**新**", 0)?;
    assert_eq!(doc.buffer_text()?, "左右");
    assert_eq!(doc.undo_depth(), 0);
    assert!(doc.has_open_block());
    assert_snapshot(&mut doc, "左**新**")?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "!", 900)?;
    doc.commit_open_block(901)?;
    assert!(doc.has_open_block());
    assert_eq!(doc.buffer_text()?, "左**新**!");
    assert_eq!(doc.undo_depth(), 1);
    doc.undo()?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(6, 3));
    assert_eq!(doc.buffer_text()?, "左右");
    Ok(())
}

#[test]
fn editable_visual_protection_refuses_typing_and_composition_before_discarding_redo() -> TestResult
{
    let raw = "前 [^ref] <b>styled</b> 后\n\n[^ref]: note";
    let mut doc = DocCore::new_editable(raw, collapsed(0), limits())?;
    let identity = doc.current_input_identity()?;
    doc.replace_selection(&identity, "再", 0)?;
    doc.undo()?;
    open_first_visual_block(&mut doc)?;
    assert_eq!(doc.redo_depth(), 1);
    let footnote_src_off = raw.find("[^ref]").ok_or("夹具应有脚注引用")? + 3;
    let html_src_off = raw.find("<b>").ok_or("夹具应有 HTML 标签")? + 1;
    let styled_src_off = raw.find("styled").ok_or("夹具应有 HTML 样式文字")? + 2;
    for (src_off, expects_footnote) in [
        (footnote_src_off, true),
        (html_src_off, false),
        (styled_src_off, false),
    ] {
        select_bytes(&mut doc, src_off, src_off)?;
        let identity = doc.current_input_identity()?;
        let before = capture(&mut doc)?;
        let typed = doc.type_selection(&identity, "新", 10);
        let ime = doc.composition_update("候选", 11);
        for result in [typed, ime] {
            if expects_footnote {
                assert!(matches!(
                    result,
                    Err(DocError::Edit(aline_edit::EditRefusal::ProtectedRange {
                        kind: aline_markdown::ProtectionKind::FootnoteReference,
                        ..
                    }))
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(DocError::Edit(aline_edit::EditRefusal::HtmlBoundary { .. }))
                ));
            }
        }
        assert_eq!(capture(&mut doc)?, before);
    }
    select_bytes(&mut doc, footnote_src_off + 1, footnote_src_off)?;
    let identity = doc.current_input_identity()?;
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.type_selection(&identity, "新", 12),
        Err(DocError::Edit(
            aline_edit::EditRefusal::ProtectedRange { .. }
        ))
    ));
    assert_eq!(capture(&mut doc)?, before);
    Ok(())
}

#[test]
fn editable_visual_rollover_rejects_newly_protected_target_before_finishing_current_group()
-> TestResult {
    let mut doc = DocCore::new_editable("前 后\n\n[^ref]: note", collapsed(4), limits())?;
    open_first_visual_block(&mut doc)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "[^ref] ", 0)?;
    select_bytes(&mut doc, 7, 7)?;
    let identity = doc.current_input_identity()?;
    let before = capture(&mut doc)?;
    for result in [
        doc.type_selection(&identity, "新", 901),
        doc.composition_start("候选", 902),
        doc.replace_selection(&identity, "粘贴", 903).map(|_| ()),
    ] {
        assert!(matches!(
            result,
            Err(DocError::Edit(aline_edit::EditRefusal::ProtectedRange {
                kind: aline_markdown::ProtectionKind::FootnoteReference,
                ..
            }))
        ));
        assert_eq!(capture(&mut doc)?, before);
    }
    Ok(())
}

#[test]
fn editable_cut_after_typing_publishes_two_sealed_steps_with_directional_selections() -> TestResult
{
    let mut doc = DocCore::new_editable("甲乙", collapsed(3), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "中", 1)?;
    select_bytes(&mut doc, 6, 3)?;
    let identity = doc.current_input_identity()?;
    assert_eq!(doc.cut_selection(&identity, TEXT, 2)?, "中");
    assert_eq!(doc.buffer_text()?, "甲乙");
    assert_eq!(doc.undo_depth(), 2);
    assert_eq!(doc.current_selection(), collapsed(3));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲中乙");
    assert_eq!(doc.current_selection(), SelectionSpec::new(6, 3));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲乙");
    assert_eq!(doc.current_selection(), collapsed(3));
    doc.redo()?;
    assert_eq!(doc.current_selection(), SelectionSpec::new(6, 3));
    doc.redo()?;
    assert_eq!(doc.current_selection(), collapsed(3));
    Ok(())
}

#[test]
fn editable_staged_replacement_refuses_whole_pair_when_history_cannot_keep_both_steps() -> TestResult
{
    let tight = aline_ui::DocLimits::new(
        TEXT,
        common::INDEX,
        aline_buffer::HistoryBudget::new(1, 1024),
    );
    let mut doc = DocCore::new_editable("甲乙", collapsed(3), tight)?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "中", 1)?;
    select_bytes(&mut doc, 6, 3)?;
    let identity = doc.current_input_identity()?;
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.replace_selection(&identity, "新", 2),
        Err(DocError::Buffer(
            aline_buffer::BufferError::HistoryBudgetExceeded { .. }
        ))
    ));
    assert_eq!(capture(&mut doc)?, before);
    Ok(())
}

#[test]
fn editable_staged_replacement_after_net_unchanged_typing_records_only_the_command() -> TestResult {
    let mut doc = DocCore::new_editable("甲乙", SelectionSpec::new(6, 0), limits())?;
    open_first_visual_block(&mut doc)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "甲", 1)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "乙", 2)?;
    assert_eq!(doc.open_block_draft(), Some("甲乙"));
    select_bytes(&mut doc, 6, 3)?;
    let identity = doc.current_input_identity()?;
    doc.replace_selection(&identity, "丁", 3)?;
    assert_eq!(doc.buffer_text()?, "甲丁");
    assert_eq!(doc.version().revision().value(), 1);
    assert_eq!(doc.undo_depth(), 1);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "甲乙");
    assert_eq!(doc.current_selection(), SelectionSpec::new(6, 3));
    Ok(())
}

#[test]
fn editable_clipboard_replacement_refuses_active_composition_and_preserves_input() -> TestResult {
    let mut doc = DocCore::new_editable("甲乙", collapsed(3), limits())?;
    open_first_visual_block(&mut doc)?;
    doc.composition_start("候选", 0)?;
    let identity = doc.current_input_identity()?;
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.replace_selection(&identity, "粘贴", 1),
        Err(DocError::CompositionActive)
    ));
    assert_eq!(capture(&mut doc)?, before);
    Ok(())
}

#[test]
fn editable_typing_from_selection_merges_continuously_in_both_modes() -> TestResult {
    for mode in [EditorMode::Visual, EditorMode::Source] {
        let mut doc = DocCore::new_editable("ac", SelectionSpec::new(1, 0), limits())?;
        if mode == EditorMode::Visual {
            open_first_visual_block(&mut doc)?;
        } else {
            doc.switch_mode(mode, 0)?;
        }
        let identity = doc.current_input_identity()?;
        doc.type_selection(&identity, "X", 10)?;
        let identity = doc.current_input_identity()?;
        doc.type_selection(&identity, "Y", 110)?;
        assert_eq!(doc.buffer_text()?, "ac");
        assert_snapshot(&mut doc, "XYc")?;
        doc.commit_open_block(111)?;
        assert_eq!(doc.undo_depth(), 1);
        doc.undo()?;
        assert_eq!(doc.buffer_text()?, "ac");
        assert_eq!(doc.current_selection(), SelectionSpec::new(1, 0));
    }
    Ok(())
}

#[test]
fn editable_typing_identical_selection_collapses_caret_in_both_modes() -> TestResult {
    for mode in [EditorMode::Visual, EditorMode::Source] {
        let mut doc = DocCore::new_editable("ac", SelectionSpec::new(1, 0), limits())?;
        if mode == EditorMode::Visual {
            open_first_visual_block(&mut doc)?;
        } else {
            doc.switch_mode(mode, 0)?;
        }
        let identity = doc.current_input_identity()?;
        let version = doc.version();
        doc.type_selection(&identity, "a", 10)?;
        assert_eq!(doc.version(), version);
        assert_eq!(doc.history_node_count(), 0);
        assert_eq!(doc.current_selection(), collapsed(1));
        let identity = doc.current_input_identity()?;
        doc.type_selection(&identity, "Y", 110)?;
        doc.commit_open_block(111)?;
        assert_eq!(doc.buffer_text()?, "aYc");
        assert_eq!(doc.undo_depth(), 1);
        doc.undo()?;
        assert_eq!(doc.current_selection(), collapsed(1));
    }
    Ok(())
}

#[test]
fn editable_visual_typing_requires_open_block_and_refuses_cross_block_selection() -> TestResult {
    let mut closed = DocCore::new_editable("a", SelectionSpec::new(0, 1), limits())?;
    let identity = closed.current_input_identity()?;
    let before = capture(&mut closed)?;
    assert!(matches!(
        closed.type_selection(&identity, "a", 0),
        Err(DocError::NoOpenBlock)
    ));
    assert_eq!(capture(&mut closed)?, before);

    let mut open = DocCore::new_editable("a\n\nc", collapsed(0), limits())?;
    open_first_visual_block(&mut open)?;
    select_bytes(&mut open, 4, 0)?;
    let identity = open.current_input_identity()?;
    let before = capture(&mut open)?;
    assert!(matches!(
        open.type_selection(&identity, "X", 0),
        Err(DocError::OutsideOpenBlock)
    ));
    assert_eq!(capture(&mut open)?, before);
    Ok(())
}

#[test]
fn editable_visual_permissions_follow_existing_protected_range_after_pending_insertion()
-> TestResult {
    let raw = "前 [^ref] 后\n\n[^ref]: note";
    let mut doc = DocCore::new_editable(raw, collapsed(0), limits())?;
    open_first_visual_block(&mut doc)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "中", 0)?;
    let reference_start = raw.find("[^ref]").ok_or("夹具应有脚注引用")? + "中".len();
    let caret = reference_start + 2;
    select_bytes(&mut doc, caret, caret)?;
    let identity = doc.current_input_identity()?;
    let before = capture(&mut doc)?;
    match doc.type_selection(&identity, "无", 901) {
        Err(DocError::Edit(aline_edit::EditRefusal::ProtectedRange {
            kind: aline_markdown::ProtectionKind::FootnoteReference,
            range,
        })) => {
            assert_eq!(range, reference_start..reference_start + "[^ref]".len());
        }
        other => return Err(format!("未拒绝草稿中的脚注位置：{other:?}").into()),
    }
    assert_eq!(capture(&mut doc)?, before);
    doc.commit_open_block(902)?;
    assert_eq!(doc.buffer_text()?, format!("中{raw}"));
    Ok(())
}

#[test]
fn editable_planned_replace_consumes_current_draft_in_two_steps_and_rejects_selection_change()
-> TestResult {
    use aline_buffer::PatchBudget;
    use aline_edit::{
        EditBudget, EditCommand, EditContext, EditMode, PlanOutcome, SourceEdit, plan_change,
    };

    let mut doc = DocCore::new_editable("ac", collapsed(1), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "B", 1)?;
    select_bytes(&mut doc, 2, 1)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let replacement = EditCommand::replace(vec![SourceEdit::new(1..2, "B", "X")]);
    let PlanOutcome::Changed(plan) = plan_change(
        snapshot.text(),
        &parsed,
        snapshot.selection(),
        &EditContext::new(EditMode::Source),
        &replacement,
        EditBudget::new(PatchBudget::new(1, 16, 16), TEXT, ParseBudget::m0()),
    )?
    else {
        return Err("夹具应生成替换计划".into());
    };
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.apply_find_change(&identity, &parsed, &plan, 0, 2),
        Err(DocError::Edit(aline_edit::EditRefusal::StaleFindResults))
    ));
    assert_eq!(capture(&mut doc)?, before);
    select_bytes(&mut doc, 1, 2)?;
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.apply_change(&identity, &parsed, &plan, 2),
        Err(DocError::Edit(aline_edit::EditRefusal::SelectionMismatch))
    ));
    assert_eq!(capture(&mut doc)?, before);
    select_bytes(&mut doc, 2, 1)?;
    doc.apply_change(&identity, &parsed, &plan, 3)?;
    assert_eq!(doc.buffer_text()?, "aXc");
    assert_eq!(doc.undo_depth(), 2);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "aBc");
    assert_eq!(doc.current_selection(), SelectionSpec::new(2, 1));
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "ac");
    assert_eq!(doc.current_selection(), collapsed(1));
    Ok(())
}

#[test]
fn editable_planned_find_all_uses_current_draft_and_preserves_one_bulk_command_step() -> TestResult
{
    use aline_buffer::PatchBudget;
    use aline_edit::{
        EditBudget, EditContext, EditMode, FindBudget, FindCase, FindReplacement, FindVersion,
        PlanOutcome, find_matches, plan_find_replace,
    };

    let mut doc = DocCore::new_editable("x x", collapsed(1), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, " X", 1)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let version = FindVersion::new(doc.document_id().clone(), doc.version(), 0);
    let found = find_matches(
        snapshot.text(),
        version.clone(),
        "x",
        FindCase::Insensitive,
        FindBudget::new(TEXT, 4),
    )?;
    let PlanOutcome::Changed(plan) = plan_find_replace(
        &found,
        &version,
        &parsed,
        snapshot.selection(),
        &EditContext::new(EditMode::Source).with_find_query_generation(0),
        FindReplacement::All("y"),
        EditBudget::new(PatchBudget::new(4, 16, 16), TEXT, ParseBudget::m0()),
    )?
    else {
        return Err("夹具应有全部替换计划".into());
    };
    let fresh_identity = doc.begin_document_request()?;
    assert_ne!(identity, fresh_identity);
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.apply_change(&fresh_identity, &parsed, &plan, 2),
        Err(DocError::Edit(aline_edit::EditRefusal::StaleFindResults))
    ));
    assert!(matches!(
        doc.apply_find_change(&fresh_identity, &parsed, &plan, 1, 2),
        Err(DocError::Edit(aline_edit::EditRefusal::StaleFindResults))
    ));
    assert_eq!(capture(&mut doc)?, before);
    doc.apply_find_change(&fresh_identity, &parsed, &plan, 0, 2)?;
    assert_eq!(doc.buffer_text()?, "y y y");
    assert_eq!(doc.undo_depth(), 2);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "x X x");
    assert_eq!(doc.undo_depth(), 1);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "x x");
    Ok(())
}

#[test]
fn editable_find_command_requires_explicit_current_query_generation_without_draft() -> TestResult {
    use aline_buffer::PatchBudget;
    use aline_edit::{
        EditBudget, EditContext, EditMode, EditRefusal, FindBudget, FindCase, FindReplacement,
        FindVersion, PlanOutcome, find_matches, plan_find_replace,
    };

    let mut doc = DocCore::new_editable("a a", collapsed(0), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let version = FindVersion::new(doc.document_id().clone(), doc.version(), 0);
    let found = find_matches(
        snapshot.text(),
        version.clone(),
        "a",
        FindCase::Sensitive,
        FindBudget::new(TEXT, 4),
    )?;
    let PlanOutcome::Changed(plan) = plan_find_replace(
        &found,
        &version,
        &parsed,
        snapshot.selection(),
        &EditContext::new(EditMode::Source).with_find_query_generation(0),
        FindReplacement::All("b"),
        EditBudget::new(PatchBudget::new(4, 16, 16), TEXT, ParseBudget::m0()),
    )?
    else {
        return Err("夹具应有全部替换计划".into());
    };
    let identity = doc.begin_document_request()?;
    let before = capture(&mut doc)?;
    for result in [
        doc.apply_change(&identity, &parsed, &plan, 1),
        doc.apply_find_change(&identity, &parsed, &plan, 1, 1),
    ] {
        assert!(matches!(
            result,
            Err(DocError::Edit(EditRefusal::StaleFindResults))
        ));
        assert_eq!(capture(&mut doc)?, before);
    }
    doc.apply_find_change(&identity, &parsed, &plan, 0, 2)?;
    assert_eq!(doc.buffer_text()?, "b b");
    assert_eq!(doc.undo_depth(), 1);
    Ok(())
}

#[test]
fn editable_staged_visual_command_refuses_source_plan_for_protected_range() -> TestResult {
    use aline_buffer::PatchBudget;
    use aline_edit::{
        EditBudget, EditCommand, EditContext, EditMode, EditRefusal, PlanOutcome, SourceEdit,
        plan_change,
    };

    let raw = "前 [^ref] 后\n\n[^ref]: note";
    let mut doc = DocCore::new_editable(raw, collapsed(0), limits())?;
    open_first_visual_block(&mut doc)?;
    let identity = doc.current_input_identity()?;
    doc.type_selection(&identity, "中", 1)?;
    let reference_start = "中".len() + raw.find("[^ref]").ok_or("夹具应有脚注引用")?;
    let reference_end = reference_start + "[^ref]".len();
    select_bytes(&mut doc, reference_end, reference_start)?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(TEXT)?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let command = EditCommand::replace(vec![SourceEdit::new(
        reference_start..reference_end,
        "[^ref]",
        "新",
    )]);
    let PlanOutcome::Changed(plan) = plan_change(
        snapshot.text(),
        &parsed,
        snapshot.selection(),
        &EditContext::new(EditMode::Source),
        &command,
        EditBudget::new(PatchBudget::new(1, 1024, 1024), TEXT, ParseBudget::m0()),
    )?
    else {
        return Err("应有源码替换计划".into());
    };
    let before = capture(&mut doc)?;
    assert!(matches!(
        doc.apply_change(&identity, &parsed, &plan, 2),
        Err(DocError::Edit(EditRefusal::WrongMode { .. }))
    ));
    assert_eq!(capture(&mut doc)?, before);
    Ok(())
}
