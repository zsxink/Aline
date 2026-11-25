//! 真实开块输入的源码范围、权限与候选重读边界。

use std::error::Error;

use aline_buffer::{PatchBudget, SrcSelection, TextBudget, TextSnapshot};
use aline_edit::{
    EditBudget, EditContext, EditMode, EditRefusal, GroupReason, PlanOutcome, RawBlockEdit,
    plan_raw_block_input, validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, ProtectionKind, parse};

const TEXT: TextBudget = TextBudget::new(4096);
const EDIT: EditBudget = EditBudget::new(PatchBudget::new(1, 4096, 4096), TEXT, ParseBudget::m0());
const VISUAL: EditContext = EditContext::new(EditMode::Visual);

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn raw_input_accepts_blank_line_anchor_but_rejects_block_and_crlf_interior() -> TestResult {
    let snapshot = TextSnapshot::new("- a\r\n".into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    for (start, expected) in [(1, false), (3, false), (4, false), (5, true)] {
        let anchor = snapshot.src_range(start..start)?;
        let source = snapshot.src_offset(start)?;
        let selection = SrcSelection::new(source.clone(), source)?;
        let result = plan_raw_block_input(
            &parsed,
            &selection,
            &VISUAL,
            &anchor,
            RawBlockEdit::new(&anchor, "", "\r\ntext"),
            EDIT,
        );
        if expected {
            let PlanOutcome::Changed(plan) = result? else {
                return Err("空行应接受受检插入".into());
            };
            let change = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
            assert_eq!(change.candidate_blocks().len(), 2);
        } else {
            assert!(matches!(result, Err(EditRefusal::InvalidRawBlock)));
        }
    }
    Ok(())
}

#[test]
fn raw_input_changes_markdown_markers_and_entities_inside_open_block() -> TestResult {
    for (source, range, from, to, drawn) in [
        ("**字**", 0..2, "**", "", "字**"),
        ("a &amp; b", 2..7, "&amp;", "&lt;", "a < b"),
        ("a b", 2..3, "b", "**b**", "a b"),
        ("a b", 3..3, "", "*", "a b*"),
    ] {
        let snapshot = TextSnapshot::new(source.into(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let block = snapshot.src_range(0..source.len())?;
        let selection = SrcSelection::new(
            snapshot.src_offset(range.start)?,
            snapshot.src_offset(range.end)?,
        )?;
        let edit = snapshot.src_range(range.clone())?;
        let plan = plan_raw_block_input(
            &parsed,
            &selection,
            &VISUAL,
            &block,
            RawBlockEdit::new(&edit, from, to),
            EDIT,
        )?;
        let PlanOutcome::Changed(plan) = plan else {
            return Err("expected a raw source patch".into());
        };
        let checked = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
        assert_eq!(checked.group_reason(), GroupReason::RawBlockInput);
        assert_eq!(checked.batch().patches(), &[(range, to.to_owned())]);
        assert_eq!(checked.preview().drawn_text(), drawn);
    }
    Ok(())
}

#[test]
fn raw_input_validation_exposes_candidate_blocks_after_a_split() -> TestResult {
    let snapshot = TextSnapshot::new("alpha beta".into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let block = snapshot.src_range(0..10)?;
    let range = snapshot.src_range(5..6)?;
    let selection = SrcSelection::new(snapshot.src_offset(5)?, snapshot.src_offset(6)?)?;
    let plan = plan_raw_block_input(
        &parsed,
        &selection,
        &VISUAL,
        &block,
        RawBlockEdit::new(&range, " ", "\n\n"),
        EDIT,
    )?;
    let PlanOutcome::Changed(plan) = plan else {
        return Err("expected a raw source patch".into());
    };
    let change = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
    let blocks = change.candidate_blocks();
    let [first, second] = blocks else {
        return Err("expected two candidate blocks".into());
    };
    assert_eq!(first.range().byte_range(), 0..5);
    assert_eq!(second.range().byte_range(), 7..11);
    assert_eq!(first.range().snapshot_id(), second.range().snapshot_id());
    assert_ne!(first.range().snapshot_id(), snapshot.id());
    assert_eq!(parsed.blocks().len(), 1);
    Ok(())
}

#[test]
fn raw_input_validation_exposes_protection_from_the_same_candidate() -> TestResult {
    let snapshot = TextSnapshot::new("word".into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let block = snapshot.src_range(0..4)?;
    let selection = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(4)?)?;
    let plan = plan_raw_block_input(
        &parsed,
        &selection,
        &VISUAL,
        &block,
        RawBlockEdit::new(&block, "word", "<b>bold</b> [^n]\n\n[^n]: note"),
        EDIT,
    )?;
    let PlanOutcome::Changed(plan) = plan else {
        return Err("expected a raw source patch".into());
    };
    let change = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
    let candidate_id = change
        .candidate_blocks()
        .first()
        .ok_or("missing candidate block")?
        .range()
        .snapshot_id();
    assert!(change.candidate_protected_ranges().iter().any(|entry| {
        entry.kind() == ProtectionKind::FootnoteReference && entry.range().byte_range() == (12..16)
    }));
    assert!(
        change
            .candidate_protected_ranges()
            .iter()
            .all(|entry| entry.range().snapshot_id() == candidate_id)
    );
    assert!(
        change
            .candidate_styled_ranges()
            .iter()
            .any(|entry| entry.range().byte_range() == (3..7))
    );
    assert!(
        change
            .candidate_styled_ranges()
            .iter()
            .all(|entry| entry.range().snapshot_id() == candidate_id)
    );
    assert_ne!(candidate_id, snapshot.id());
    Ok(())
}

#[test]
fn raw_input_rejects_cross_block_foreign_and_false_preimages() -> TestResult {
    let original = "a\n\nb";
    let snapshot = TextSnapshot::new(original.into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(0)?)?;
    let block = snapshot.src_range(0..1)?;
    let whole = snapshot.src_range(0..4)?;
    let cross = snapshot.src_range(1..3)?;
    let end = snapshot.src_range(4..4)?;
    let other = TextSnapshot::new(original.into(), TEXT)?;
    let other_edit = other.src_range(0..1)?;
    for (block, range, expected, failure) in [
        (&whole, &block, "a", "InvalidRawBlock"),
        (&block, &cross, "\n\n", "RawInputOutsideBlock"),
        (&block, &end, "", "RawInputOutsideBlock"),
        (&block, &other_edit, "a", "WrongSnapshot"),
        (&other_edit, &block, "a", "WrongSnapshot"),
        (&block, &block, "wrong", "ExpectedFragmentMismatch"),
    ] {
        let result = plan_raw_block_input(
            &parsed,
            &selection,
            &VISUAL,
            block,
            RawBlockEdit::new(range, expected, "x"),
            EDIT,
        );
        assert!(matches!(result, Err(error) if error.as_str() == failure));
    }
    assert_eq!(&*snapshot.slice(&whole, TEXT)?, original);
    assert_eq!(selection.head().byte_offset(), 0);
    Ok(())
}

#[test]
fn raw_input_rejects_md11_endpoints_and_html_boundary() -> TestResult {
    let snapshot = TextSnapshot::new("word[^x]\n\n[^x]: note\n".into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let block = snapshot.src_range(0..8)?;
    let endpoint = snapshot.src_range(4..4)?;
    let selection = SrcSelection::new(snapshot.src_offset(4)?, snapshot.src_offset(4)?)?;
    assert!(matches!(
        plan_raw_block_input(
            &parsed,
            &selection,
            &VISUAL,
            &block,
            RawBlockEdit::new(&endpoint, "", "x"),
            EDIT
        ),
        Err(EditRefusal::ProtectedRange {
            kind: ProtectionKind::FootnoteReference,
            ..
        })
    ));

    let snapshot = TextSnapshot::new("<b>x</b>".into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let block = snapshot.src_range(0..8)?;
    let edit = snapshot.src_range(3..4)?;
    let selection = SrcSelection::new(snapshot.src_offset(3)?, snapshot.src_offset(4)?)?;
    assert!(matches!(
        plan_raw_block_input(
            &parsed,
            &selection,
            &VISUAL,
            &block,
            RawBlockEdit::new(&edit, "x", "y"),
            EDIT
        ),
        Err(EditRefusal::HtmlBoundary { .. })
    ));
    assert!(matches!(
        plan_raw_block_input(
            &parsed,
            &selection,
            &EditContext::new(EditMode::Source),
            &block,
            RawBlockEdit::new(&edit, "x", "y"),
            EDIT
        ),
        Err(EditRefusal::WrongMode { .. })
    ));
    Ok(())
}
