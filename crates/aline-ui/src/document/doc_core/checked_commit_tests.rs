//! R04 真实文档所有者的权限、输入发布和命令拒绝回归。
use super::*;

fn limits() -> DocLimits {
    DocLimits::new(
        TextBudget::new(1 << 20),
        LineIndexBudget::new(4096),
        aline_buffer::HistoryBudget::new(32, 1 << 20),
    )
}

#[derive(Debug, PartialEq)]
struct State {
    buffer: String,
    buffer_id: aline_buffer::SnapshotId,
    version: WorkingVersion,
    open: Option<OpenBlock>,
    wait: Option<WaitState>,
    cache: Option<(WorkingVersion, aline_buffer::SnapshotId)>,
    next_wait: u64,
    config: ParseConfigGeneration,
    request: DerivedRequestId,
    mode: EditorMode,
    document: DocumentId,
    resources: Arc<[ResourceReference]>,
    limits: DocLimits,
}
fn state(doc: &DocCore) -> State {
    State {
        // 冻结全部历史节点的只读步骤、历史光标、正逆补丁及行索引；不增加可写访问器。
        buffer: format!(
            "{:?}",
            (
                doc.buffer.text(doc.limits.text),
                doc.buffer.selection(),
                doc.buffer.line_index(),
                doc.undo_depth(),
                doc.redo_depth(),
                doc.history_node_count(),
                (0..doc.history_node_count())
                    .filter_map(|index| doc.buffer.history_step(index))
                    .map(|step| format!("{:?}", step.get()))
                    .collect::<Vec<_>>()
            )
        ),
        buffer_id: doc.buffer.snapshot().id().clone(),
        version: doc.version,
        open: doc.open.clone(),
        wait: doc.wait,
        cache: doc
            .snapshot_cache
            .as_ref()
            .map(|cache| (cache.version, cache.snapshot.snapshot_id().clone())),
        next_wait: doc.next_wait_id,
        config: doc.parse_config,
        request: doc.derived_request,
        mode: doc.mode,
        document: doc.document.clone(),
        resources: doc.resources.clone(),
        limits: doc.limits,
    }
}

#[test]
fn visual_typing_protection_rejection_preserves_complete_state() -> Result<(), DocError> {
    let mut doc = DocCore::new("---\nt: x\n---\n", SelectionSpec::new(7, 7), limits())?;
    doc.type_text(7, "Y", 0)?;
    doc.current_snapshot(limits().text())?;
    let before = state(&doc);
    let result = doc.commit_open_block(1);
    assert!(
        result.is_err(),
        "protected Visual input was committed: {result:?}"
    );
    assert_eq!(state(&doc), before);
    Ok(())
}

#[test]
fn visual_ime_protection_rejection_preserves_complete_state() -> Result<(), DocError> {
    let mut doc = DocCore::new("---\nt: x\n---\n", SelectionSpec::new(7, 7), limits())?;
    doc.composition_start("Y", 0)?;
    doc.composition_update("Z", 1)?;
    doc.begin_publication_wait(PublicationKind::Save)?;
    doc.current_snapshot(limits().text())?;
    let before = state(&doc);
    let result = doc.composition_commit(2);
    assert!(
        result.is_err(),
        "protected IME input was committed: {result:?}"
    );
    assert_eq!(state(&doc), before);
    Ok(())
}

use aline_buffer::{Affinity, PatchBudget};
use aline_edit::{
    ChangePlan, EditBudget, EditCommand, EditContext, EditMode, EditRefusal, PlanOutcome,
    SourceEdit, plan_change,
};
use aline_markdown::{ParseBudget, ParseConfig, Parsed, ProtectionKind, parse};
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

fn planned(
    doc: &mut DocCore,
    edits: Vec<SourceEdit>,
) -> Result<(InputIdentity, Parsed, Box<ChangePlan>), Box<dyn Error>> {
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(limits().text())?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let selection = bind_selection(snapshot.text(), doc.current_selection())?;
    let mode = if doc.mode() == EditorMode::Visual {
        EditMode::Visual
    } else {
        EditMode::Source
    };
    let outcome = plan_change(
        snapshot.text(),
        &parsed,
        &selection,
        &EditContext::new(mode),
        &EditCommand::replace(edits),
        EditBudget::new(
            PatchBudget::new(8, 1 << 20, 1 << 20),
            limits().text(),
            ParseBudget::m0(),
        ),
    )?;
    match outcome {
        PlanOutcome::Changed(plan) => Ok((identity, parsed, plan)),
        _ => Err(std::io::Error::other("测试命令必须产生计划").into()),
    }
}

fn with_redo() -> Result<DocCore, DocError> {
    let mut doc = DocCore::new("abc", SelectionSpec::new(1, 1), limits())?;
    doc.type_text(1, "X", 0)?;
    doc.commit_open_block(1)?;
    doc.type_text(2, "Z", 2)?;
    doc.commit_open_block(3)?;
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "aXbc");
    assert_eq!(
        (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
        (1, 1, 2)
    );
    Ok(doc)
}

#[test]
fn visual_html_input_rejection_uses_parsed_boundaries_and_keeps_state() -> TestResult {
    let source = "<b>x</b>";
    let snapshot = TextSnapshot::new(source.to_owned(), limits().text())?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let mut offsets = parsed
        .protected_ranges()
        .iter()
        .filter(|item| item.kind() == ProtectionKind::Html)
        .map(|item| item.range().byte_range().start)
        .collect::<Vec<_>>();
    offsets.extend(
        parsed
            .styled_ranges()
            .iter()
            .map(|item| item.range().byte_range().start),
    );
    assert!(!offsets.is_empty());
    for at in offsets {
        for ime in [false, true] {
            let mut doc = DocCore::new(source, SelectionSpec::new(at, at), limits())?;
            if ime {
                doc.composition_start("Y", 0)?;
            } else {
                doc.type_text(at, "Y", 0)?;
            }
            doc.current_snapshot(limits().text())?;
            let before = state(&doc);
            let result = if ime {
                doc.composition_commit(1)
            } else {
                doc.commit_open_block(1)
            };
            assert!(matches!(
                result,
                Err(DocError::Edit(EditRefusal::HtmlBoundary { .. }))
            ));
            assert_eq!(state(&doc), before);
        }
    }
    Ok(())
}

#[test]
fn source_typing_and_ime_allow_protected_and_hidden_source() -> TestResult {
    for (source, at, input, expected) in [
        ("---\nt: x\n---\n", 7, "Y", "---\nt: Yx\n---\n"),
        ("abc", 0, "**hi**", "**hi**abc"),
        ("<b>x</b>", 3, "Y", "<b>Yx</b>"),
    ] {
        for ime in [false, true] {
            let mut doc = DocCore::new(source, SelectionSpec::new(at, at), limits())?;
            doc.switch_mode(EditorMode::Source, 0)?;
            if ime {
                doc.composition_start(input, 1)?;
                doc.composition_commit(2)?;
            } else {
                doc.type_text(at, input, 1)?;
                doc.commit_open_block(2)?;
            }
            assert_eq!(doc.buffer_text()?, expected);
            assert_eq!(
                (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
                (1, 0, 1)
            );
            doc.switch_mode(EditorMode::Visual, 3)?;
            assert_eq!(doc.history_node_count(), 1);
            doc.undo()?;
            assert_eq!(doc.buffer_text()?, source);
        }
    }
    Ok(())
}

#[test]
fn visual_typing_and_ime_use_exact_mapped_visible_insertion() -> TestResult {
    for ime in [false, true] {
        let source = "**hi**";
        let snapshot = TextSnapshot::new(source.to_owned(), limits().text())?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let at = parsed
            .projection()
            .source_at(&parsed.projection().drawn_offset(2)?, Affinity::Before)?
            .into_value()
            .byte_offset();
        let mut doc = DocCore::new(source, SelectionSpec::new(at, at), limits())?;
        if ime {
            doc.composition_start("ab", 0)?;
            doc.composition_update("abc", 1)?;
            doc.composition_commit(2)?;
        } else {
            doc.type_text(at, "a", 0)?;
            doc.type_text(at + 1, "b", 1)?;
            doc.type_text(at + 2, "c", 2)?;
            doc.commit_open_block(3)?;
        }
        assert_eq!(doc.buffer_text()?, "**hiabc**");
        assert_eq!(doc.history_node_count(), 1);
        let step = doc
            .buffer
            .history_step(0)
            .ok_or_else(|| std::io::Error::other("缺少历史"))?;
        assert_eq!(step.get().forward_patches(), &[(at..at, "abc".to_owned())]);
        assert_eq!(
            step.get().source(),
            if ime {
                OperationSource::ImeCommit
            } else {
                OperationSource::Typing
            }
        );
        doc.undo()?;
        assert_eq!(doc.buffer_text()?, source);
    }
    Ok(())
}

#[test]
fn visual_hidden_marker_creation_retains_existing_replace_refusal() -> TestResult {
    for ime in [false, true] {
        let mut doc = DocCore::new("abc", SelectionSpec::new(0, 0), limits())?;
        if ime {
            doc.composition_start("**hi**", 0)?;
        } else {
            doc.type_text(0, "**hi**", 0)?;
        }
        let before = state(&doc);
        let result = if ime {
            doc.composition_commit(1)
        } else {
            doc.commit_open_block(1)
        };
        assert!(matches!(
            result,
            Err(DocError::Edit(EditRefusal::RereadFailed {
                reason: aline_edit::RereadFailure::HiddenSourceChanged,
                ..
            }))
        ));
        assert_eq!(state(&doc), before);
    }
    Ok(())
}

#[test]
fn protected_pending_group_cannot_bypass_checked_commit_at_any_boundary() -> TestResult {
    for boundary in [0, 1, 2, 3] {
        let mut doc = DocCore::new("---\nt: x\n---\n", SelectionSpec::new(7, 7), limits())?;
        doc.type_text(7, "Y", 0)?;
        doc.current_snapshot(limits().text())?;
        let before = state(&doc);
        let result = match boundary {
            0 => doc.type_text(8, "Z", 901),
            1 => doc.composition_start("中", 1),
            2 => doc.leave_block(1),
            _ => doc.switch_mode(EditorMode::Source, 1).map(|_| ()),
        };
        assert!(matches!(
            result,
            Err(DocError::Edit(EditRefusal::ProtectedRange { .. }))
        ));
        assert_eq!(state(&doc), before);
    }
    Ok(())
}

#[test]
fn command_precheck_refuses_real_pending_or_composition_with_redo_untouched() -> TestResult {
    for ime in [false, true] {
        let mut doc = with_redo()?;
        let (identity, parsed, plan) = planned(&mut doc, vec![SourceEdit::new(2..2, "", "Q")])?;
        if ime {
            doc.composition_start("中", 5)?;
            doc.begin_publication_wait(PublicationKind::Save)?;
        } else {
            doc.type_text(2, "P", 5)?;
        }
        doc.current_snapshot(limits().text())?;
        let before = state(&doc);
        let result = doc.apply_change(&identity, &parsed, &plan, 6);
        assert!(if ime {
            matches!(result, Err(DocError::CompositionActive))
        } else {
            matches!(result, Err(DocError::PendingInput))
        });
        assert_eq!(state(&doc), before);
    }
    Ok(())
}

#[test]
fn command_success_is_one_atomic_step_and_invalidates_old_redo() -> TestResult {
    let mut doc = with_redo()?;
    let (identity, parsed, plan) = planned(&mut doc, vec![SourceEdit::new(2..2, "", "Q")])?;
    let result = doc.apply_change(&identity, &parsed, &plan, 4)?;
    assert!(matches!(result, CommitOutcome::Committed { .. }));
    assert_eq!(doc.buffer_text()?, "aXQbc");
    assert_eq!(
        (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
        (2, 0, 2)
    );
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "aXbc");
    doc.redo()?;
    assert_eq!(doc.buffer_text()?, "aXQbc");
    Ok(())
}

#[test]
fn command_source_multi_patch_is_not_restricted_by_input_patch_budget() -> TestResult {
    let mut doc = DocCore::new("abcd", SelectionSpec::new(0, 0), limits())?;
    doc.switch_mode(EditorMode::Source, 0)?;
    let (identity, parsed, plan) = planned(
        &mut doc,
        vec![
            SourceEdit::new(0..1, "a", "XX"),
            SourceEdit::new(3..4, "d", "Y"),
        ],
    )?;
    doc.apply_change(&identity, &parsed, &plan, 1)?;
    assert_eq!(doc.buffer_text()?, "XXbcY");
    assert_eq!(doc.history_node_count(), 1);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "abcd");
    Ok(())
}

#[test]
fn command_refuses_stale_identity_selection_or_projection_without_state_change() -> TestResult {
    for mismatch in 0..8 {
        let mut doc = with_redo()?;
        let (mut identity, mut parsed, plan) =
            planned(&mut doc, vec![SourceEdit::new(2..2, "", "Q")])?;
        match mismatch {
            0 => doc.advance_parse_config()?,
            1 => {
                doc.begin_derived_request()?;
            }
            2 => doc.set_selection(SelectionSpec::new(0, 0))?,
            3 => {
                parsed = parse(doc.buffer.snapshot(), ParseConfig::m0(), ParseBudget::m0())?;
            }
            4 => {
                let mut other = DocCore::new("aXbc", SelectionSpec::new(2, 2), limits())?;
                identity = other.current_input_identity()?;
            }
            5 => {
                doc.type_text(2, "R", 5)?;
                doc.commit_open_block(6)?;
            }
            6 => {
                doc.composition_start("Q", 5)?;
                doc.composition_cancel(6)?;
            }
            _ => {
                parsed = parse(
                    doc.buffer.snapshot(),
                    ParseConfig::new(2, ParseConfig::m0().features()),
                    ParseBudget::m0(),
                )?;
            }
        }
        let before = state(&doc);
        assert!(doc.apply_change(&identity, &parsed, &plan, 7).is_err());
        assert_eq!(state(&doc), before);
    }
    Ok(())
}

#[test]
fn command_history_budget_failure_keeps_whole_owner_state() -> TestResult {
    let limits = DocLimits::new(
        limits().text(),
        limits().index(),
        aline_buffer::HistoryBudget::new(0, 0),
    );
    let mut doc = DocCore::new("abc", SelectionSpec::new(1, 1), limits)?;
    let (identity, parsed, plan) = planned(&mut doc, vec![SourceEdit::new(1..1, "", "Q")])?;
    let before = state(&doc);
    assert!(matches!(
        doc.apply_change(&identity, &parsed, &plan, 0),
        Err(DocError::Buffer(BufferError::HistoryBudgetExceeded { .. }))
    ));
    assert_eq!(state(&doc), before);
    Ok(())
}

#[test]
fn no_change_or_cancelled_composition_preserves_redo_and_history_steps() -> TestResult {
    for cancel in [false, true] {
        let mut doc = with_redo()?;
        doc.set_selection(SelectionSpec::new(1, 2))?;
        let history_before = state(&doc).buffer;
        doc.composition_start("X", 4)?;
        if cancel {
            doc.composition_cancel(5)?;
        } else {
            assert_eq!(doc.composition_commit(5)?, CommitOutcome::Unchanged);
        }
        assert_eq!(state(&doc).buffer, history_before);
        assert_eq!(
            (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
            (1, 1, 2)
        );
        assert!(doc.composition().is_none());
    }
    Ok(())
}

#[test]
fn ordinary_input_and_ime_clear_redo_at_their_contract_boundaries() -> TestResult {
    for ime in [false, true] {
        let mut doc = with_redo()?;
        if ime {
            doc.composition_start("Q", 4)?;
            // IME 仍是临时候选；取消必须保留旧 redo 分支。
            assert_eq!(doc.redo_depth(), 1);
            doc.composition_cancel(5)?;
            assert_eq!(doc.redo_depth(), 1);
            doc.composition_start("Q", 6)?;
        } else {
            doc.type_text(2, "Q", 4)?;
            // 普通输入成为有效待定草稿时立即截断旧分支。
            assert_eq!(doc.redo_depth(), 0);
        }
        if ime {
            doc.composition_commit(5)?;
        } else {
            doc.commit_open_block(5)?;
        }
        assert_eq!(doc.buffer_text()?, "aXQbc");
        assert_eq!(
            (doc.undo_depth(), doc.redo_depth(), doc.history_node_count()),
            (2, 0, 2)
        );
    }
    Ok(())
}

#[test]
fn command_inline_format_consumes_same_projection_and_records_one_step() -> TestResult {
    let mut doc = DocCore::new("abc", SelectionSpec::new(1, 2), limits())?;
    let identity = doc.current_input_identity()?;
    let snapshot = doc.current_snapshot(limits().text())?;
    let parsed = parse(snapshot.text(), ParseConfig::m0(), ParseBudget::m0())?;
    let selection = bind_selection(snapshot.text(), doc.current_selection())?;
    let outcome = plan_change(
        snapshot.text(),
        &parsed,
        &selection,
        &EditContext::new(EditMode::Visual),
        &EditCommand::toggle_inline_format(1..2, aline_edit::InlineFormat::Strong),
        EditBudget::new(
            PatchBudget::new(2, 1 << 20, 1 << 20),
            limits().text(),
            ParseBudget::m0(),
        ),
    )?;
    let PlanOutcome::Changed(plan) = outcome else {
        return Err(std::io::Error::other("格式计划缺失").into());
    };
    doc.apply_change(&identity, &parsed, &plan, 0)?;
    assert_eq!(doc.buffer_text()?, "a**b**c");
    assert_eq!(doc.history_node_count(), 1);
    doc.undo()?;
    assert_eq!(doc.buffer_text()?, "abc");
    Ok(())
}

#[test]
fn command_wrong_mode_rejects_before_publish_and_preserves_redo() -> TestResult {
    let mut doc = with_redo()?;
    let (_, parsed, plan) = planned(&mut doc, vec![SourceEdit::new(2..2, "", "Q")])?;
    doc.switch_mode(EditorMode::Source, 4)?;
    let identity = doc.current_input_identity()?;
    let before = state(&doc);
    assert!(matches!(
        doc.apply_change(&identity, &parsed, &plan, 5),
        Err(DocError::Edit(EditRefusal::WrongMode { .. }))
    ));
    assert_eq!(state(&doc), before);
    Ok(())
}
