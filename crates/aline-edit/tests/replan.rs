//! 封口待定输入后的等价候选重规划。所有真实状态由调用方持有。

use std::error::Error;
use std::sync::Arc;

use aline_buffer::{PatchBudget, SrcSelection, TextBudget, TextSnapshot};
use aline_edit::{
    EditBudget, EditCommand, EditContext, EditMode, EditRefusal, FindBudget, FindCase,
    FindReplacement, FindVersion, InlineFormat, PlanOutcome, RawBlockEdit, SourceEdit,
    find_matches, plan_change, plan_find_replace, plan_raw_block_input, validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, ProtectionKind, parse};

const TEXT: TextBudget = TextBudget::new(4096);
const EDIT: EditBudget = EditBudget::new(PatchBudget::new(16, 4096, 4096), TEXT, ParseBudget::m0());
const FIND: FindBudget = FindBudget::new(TEXT, 16);
const VISUAL: EditContext = EditContext::new(EditMode::Visual);
const SOURCE: EditContext = EditContext::new(EditMode::Source);
type TestResult = Result<(), Box<dyn Error>>;

fn selection(
    snapshot: &TextSnapshot,
    anchor: usize,
    head: usize,
) -> Result<SrcSelection, aline_buffer::BufferError> {
    SrcSelection::new(snapshot.src_offset(anchor)?, snapshot.src_offset(head)?)
}

#[test]
fn replan_direct_replace_rebinds_segmented_logical_input_and_checks_old_identity() -> TestResult {
    let base = TextSnapshot::new("abc".into(), TEXT)?;
    let original =
        TextSnapshot::with_overlay(&base, base.src_range(1..1)?, Arc::new("d".into()), TEXT)?;
    let before = parse(&original, ParseConfig::m0(), ParseBudget::m0())?;
    let before_selection = selection(&original, 4, 2)?;
    let candidate = TextSnapshot::new("adbc".into(), TEXT)?;
    let after_input = parse(&candidate, ParseConfig::m0(), ParseBudget::m0())?;
    let after_input_selection = selection(&candidate, 4, 2)?;
    let planned = plan_change(
        &original,
        &before,
        &before_selection,
        &SOURCE,
        &EditCommand::replace(vec![SourceEdit::new(2..4, "bc", "X")]),
        EDIT,
    )?;
    let PlanOutcome::Changed(plan) = planned else {
        return Err("expected direct plan".into());
    };
    assert!(matches!(
        validate_change(
            &plan,
            &candidate,
            &after_input,
            &after_input_selection,
            &SOURCE
        ),
        Err(EditRefusal::WrongSnapshot)
    ));
    let new_plan = plan.replan_on_equivalent_candidate(
        &before,
        &before_selection,
        &after_input,
        &after_input_selection,
        &SOURCE,
    )?;
    let PlanOutcome::Changed(new_plan) = new_plan else {
        return Err("expected replanned direct command".into());
    };
    let checked = validate_change(
        &new_plan,
        &candidate,
        &after_input,
        &after_input_selection,
        &SOURCE,
    )?;
    assert_eq!(checked.batch().patches(), &[(2..4, "X".into())]);
    assert_eq!(checked.preview().drawn_text(), "adX");
    assert_eq!(checked.batch().before_id(), candidate.id());
    assert_eq!(plan.batch().before_id(), original.id());

    // 命中片段仍是 bc，候选全文的差异也必须拒绝。
    let foreign = TextSnapshot::new("axbc".into(), TEXT)?;
    let foreign_parsed = parse(&foreign, ParseConfig::m0(), ParseBudget::m0())?;
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &before,
            &before_selection,
            &foreign_parsed,
            &selection(&foreign, 4, 2)?,
            &SOURCE,
        ),
        Err(EditRefusal::CandidateContentMismatch)
    ));
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &before,
            &selection(&original, 2, 4)?,
            &after_input,
            &after_input_selection,
            &SOURCE,
        ),
        Err(EditRefusal::SelectionMismatch)
    ));
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &before,
            &before_selection,
            &after_input,
            &selection(&candidate, 2, 4)?,
            &SOURCE,
        ),
        Err(EditRefusal::SelectionMismatch)
    ));
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &after_input,
            &after_input_selection,
            &after_input,
            &after_input_selection,
            &SOURCE,
        ),
        Err(EditRefusal::WrongSnapshot)
    ));
    let reparse = parse(&original, ParseConfig::m0(), ParseBudget::m0())?;
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &reparse,
            &before_selection,
            &after_input,
            &after_input_selection,
            &SOURCE,
        ),
        Err(EditRefusal::ProjectionMismatch)
    ));
    assert_eq!(checked.preview().drawn_text(), "adX");
    Ok(())
}

#[test]
fn replan_format_runs_the_original_format_intent_on_the_candidate() -> TestResult {
    let source = TextSnapshot::new("one two".into(), TEXT)?;
    let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
    let selection_before = selection(&source, 7, 4)?;
    let plan = plan_change(
        &source,
        &parsed,
        &selection_before,
        &VISUAL,
        &EditCommand::toggle_inline_format(4..7, InlineFormat::Strike),
        EDIT,
    )?;
    let PlanOutcome::Changed(plan) = plan else {
        return Err("expected format plan".into());
    };
    let candidate = TextSnapshot::new("one two".into(), TEXT)?;
    let candidate_parsed = parse(&candidate, ParseConfig::m0(), ParseBudget::m0())?;
    let candidate_selection = selection(&candidate, 7, 4)?;
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &parsed,
            &selection_before,
            &candidate_parsed,
            &candidate_selection,
            &VISUAL.with_find_query_generation(0),
        ),
        Err(EditRefusal::StaleFindResults)
    ));
    let new_plan = plan.replan_on_equivalent_candidate(
        &parsed,
        &selection_before,
        &candidate_parsed,
        &candidate_selection,
        &VISUAL,
    )?;
    let PlanOutcome::Changed(new_plan) = new_plan else {
        return Err("expected replanned format".into());
    };
    assert_eq!(new_plan.group_reason(), plan.group_reason());
    assert_eq!(
        new_plan.batch().patches(),
        &[(4..4, "~~".into()), (7..7, "~~".into())]
    );
    assert_eq!(new_plan.target_selection().anchor(), 9);
    assert_eq!(new_plan.target_selection().head(), 6);
    assert!(
        validate_change(
            &new_plan,
            &candidate,
            &candidate_parsed,
            &candidate_selection,
            &VISUAL
        )
        .is_ok()
    );
    Ok(())
}

#[test]
fn replan_find_uses_the_original_search_and_checks_visual_protection_again() -> TestResult {
    let source = TextSnapshot::new("WORD\n\nword".into(), TEXT)?;
    let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
    let before_selection = selection(&source, 0, 0)?;
    let version = FindVersion::new(1, (0, 1), 0);
    let visual_find = VISUAL.with_find_query_generation(0);
    let hits = find_matches(&source, version, "wOrD", FindCase::Insensitive, FIND)?;
    let planned = plan_find_replace(
        &hits,
        &version,
        &parsed,
        &before_selection,
        &VISUAL,
        FindReplacement::All("z"),
        EDIT,
    )?;
    let PlanOutcome::Changed(plan) = planned else {
        return Err("expected find plan".into());
    };
    let candidate = TextSnapshot::new("WORD\n\nword".into(), TEXT)?;
    let candidate_parsed = parse(&candidate, ParseConfig::m0(), ParseBudget::m0())?;
    let candidate_selection = selection(&candidate, 0, 0)?;
    for context in [VISUAL, VISUAL.with_find_query_generation(1)] {
        assert!(matches!(
            plan.replan_on_equivalent_candidate(
                &parsed,
                &before_selection,
                &candidate_parsed,
                &candidate_selection,
                &context,
            ),
            Err(EditRefusal::StaleFindResults)
        ));
    }
    let new_plan = plan.replan_on_equivalent_candidate(
        &parsed,
        &before_selection,
        &candidate_parsed,
        &candidate_selection,
        &visual_find,
    )?;
    let PlanOutcome::Changed(new_plan) = new_plan else {
        return Err("expected replanned find".into());
    };
    assert_eq!(
        new_plan.batch().patches(),
        &[(0..4, "z".into()), (6..10, "z".into())]
    );
    assert_eq!(new_plan.batch().before_id(), candidate.id());
    assert!(matches!(
        validate_change(
            &new_plan,
            &candidate,
            &candidate_parsed,
            &candidate_selection,
            &VISUAL.with_find_query_generation(1),
        ),
        Err(EditRefusal::StaleFindResults)
    ));
    validate_change(
        &new_plan,
        &candidate,
        &candidate_parsed,
        &candidate_selection,
        &visual_find,
    )?;
    let current = plan_find_replace(
        &hits,
        &version,
        &parsed,
        &before_selection,
        &VISUAL,
        FindReplacement::Current {
            match_index: 1,
            text: "z",
        },
        EDIT,
    )?;
    let PlanOutcome::Changed(current) = current else {
        return Err("expected a single-match find plan".into());
    };
    let current = current.replan_on_equivalent_candidate(
        &parsed,
        &before_selection,
        &candidate_parsed,
        &candidate_selection,
        &visual_find,
    )?;
    let PlanOutcome::Changed(current) = current else {
        return Err("expected a replanned single-match find command".into());
    };
    assert_eq!(current.batch().patches(), &[(6..10, "z".into())]);
    let fallback = parse(
        &candidate,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_nodes(1),
    )?;
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &parsed,
            &before_selection,
            &fallback,
            &candidate_selection,
            &visual_find,
        ),
        Err(EditRefusal::ProtectedRange {
            kind: ProtectionKind::RawFallback,
            ..
        })
    ));
    Ok(())
}

#[test]
fn replan_rejects_raw_input_as_the_following_command() -> TestResult {
    let source = TextSnapshot::new("one".into(), TEXT)?;
    let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
    let before_selection = selection(&source, 0, 0)?;
    let planned = plan_raw_block_input(
        &parsed,
        &before_selection,
        &VISUAL,
        &source.src_range(0..3)?,
        RawBlockEdit::new(&source.src_range(0..0)?, "", "*"),
        EDIT,
    )?;
    let PlanOutcome::Changed(plan) = planned else {
        return Err("expected input plan".into());
    };
    let candidate = TextSnapshot::new("one".into(), TEXT)?;
    let candidate_parsed = parse(&candidate, ParseConfig::m0(), ParseBudget::m0())?;
    assert!(matches!(
        plan.replan_on_equivalent_candidate(
            &parsed,
            &before_selection,
            &candidate_parsed,
            &selection(&candidate, 0, 0)?,
            &VISUAL,
        ),
        Err(EditRefusal::UnsupportedReplan)
    ));
    Ok(())
}
