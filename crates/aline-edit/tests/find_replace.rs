//! 源码查找金样、查找替换计划与原子历史消费。

use std::error::Error;

use aline_buffer::{
    ApplyOutcome, HistoryBudget, LineIndexBudget, OperationSource, PatchBudget, SelectionSpec,
    SrcSelection, StringBuffer, TextBudget, TextSnapshot,
};
use aline_edit::{
    BudgetKind, EditBudget, EditContext, EditMode, EditRefusal, FindBudget, FindCase,
    FindReplacement, FindVersion, GroupReason, PlanOutcome, RereadFailure, find_matches,
    plan_find_replace, validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, ProtectionKind, parse};

const TEXT: TextBudget = TextBudget::new(4096);
const FIND: FindBudget = FindBudget::new(TEXT, 64);

type TestResult = Result<(), Box<dyn Error>>;

fn edit_budget() -> EditBudget {
    EditBudget::new(PatchBudget::new(64, 4096, 4096), TEXT, ParseBudget::m0())
}

#[test]
fn find_uses_current_source_including_hidden_and_cross_block_text() -> TestResult {
    let base = TextSnapshot::new("**a**\n\n<!--b--> [c](https://a)".into(), TEXT)?;
    let current = TextSnapshot::with_overlay(
        &base,
        base.src_range(11..12)?,
        std::sync::Arc::new("B".to_owned()),
        TEXT,
    )?;
    let version = FindVersion::new(1, (0, 1), 5);
    for (query, expected) in [
        ("**", 0..2),
        ("**\n\n<!--B", 3..12),
        ("<!--B-->", 7..15),
        ("https://a", 20..29),
    ] {
        let found = find_matches(&current, version, query, FindCase::Sensitive, FIND)?;
        let hit = found.matches().first().ok_or("missing hit")?;
        assert_eq!(hit.range().byte_range(), expected, "{query}");
        assert_eq!(hit.text(), query);
        assert_eq!(hit.range().snapshot_id(), current.id());
    }
    Ok(())
}

#[test]
fn find_unicode_insensitive_matches_complete_source_scalars_only() -> TestResult {
    let source = TextSnapshot::new("éÉ İ i\u{307} i Σσ ς e\u{301} 😀".into(), TEXT)?;
    let version = FindVersion::new(1, 0, 0);
    for (query, expected) in [
        ("É", vec!["é", "É"]),
        ("i\u{307}", vec!["İ", "i\u{307}"]),
        ("i", vec!["i", "i"]),
        ("σ", vec!["Σ", "σ", "ς"]),
        ("😀", vec!["😀"]),
    ] {
        let found = find_matches(&source, version, query, FindCase::Insensitive, FIND)?;
        assert_eq!(
            found
                .matches()
                .iter()
                .map(|hit| hit.text())
                .collect::<Vec<_>>(),
            expected,
            "{query}"
        );
    }
    Ok(())
}

#[test]
fn find_rejects_expanded_scan_workspace_before_allocating() -> TestResult {
    let source = TextSnapshot::new("İ".into(), TEXT)?;
    let version = FindVersion::new(1, 0, 0);
    // İ 小写展开为 i 和组合点。对照两项实际工作区元素的字节数。
    let expanded_bytes = 2 * (size_of::<char>() + size_of::<usize>() + size_of::<Option<usize>>());
    let limit = expanded_bytes - 1;
    assert!(matches!(
        find_matches(
            &source,
            version,
            "İ",
            FindCase::Insensitive,
            FindBudget::new(TextBudget::new(limit), 1),
        ),
        Err(EditRefusal::BudgetExceeded {
            kind: BudgetKind::FindWorkspace,
            needed: Some(needed),
            limit: Some(actual_limit),
        }) if needed == expanded_bytes && actual_limit == limit
    ));
    let matches = find_matches(
        &source,
        version,
        "İ",
        FindCase::Insensitive,
        FindBudget::new(TextBudget::new(expanded_bytes), 1),
    )?;
    assert_eq!(matches.matches().len(), 1);
    Ok(())
}

#[test]
fn find_distinguishes_empty_no_match_and_budget_failure() -> TestResult {
    let source = TextSnapshot::new("aaaaa".into(), TEXT)?;
    let version = FindVersion::new(1, 0, 0);
    let empty = find_matches(&source, version, "", FindCase::Sensitive, FIND)?;
    let missing = find_matches(&source, version, "x", FindCase::Sensitive, FIND)?;
    assert!(empty.is_empty_query());
    assert!(!missing.is_empty_query());
    assert!(empty.matches().is_empty() && missing.matches().is_empty());
    for case in [FindCase::Sensitive, FindCase::Insensitive] {
        let found = find_matches(&source, version, "aa", case, FIND)?;
        assert_eq!(
            found
                .matches()
                .iter()
                .map(|hit| hit.range().byte_range())
                .collect::<Vec<_>>(),
            vec![0..2, 2..4]
        );
        assert!(matches!(
            find_matches(&source, version, "aa", case, FindBudget::new(TEXT, 1)),
            Err(EditRefusal::BudgetExceeded {
                kind: BudgetKind::FindMatches,
                needed: Some(2),
                limit: Some(1),
            })
        ));
    }
    Ok(())
}

#[test]
fn find_replace_all_visual_is_one_undo_for_multiple_cross_block_hits() -> TestResult {
    let original = "a\n\nb\n\na\n\nb";
    let mut buffer = StringBuffer::new(
        original.to_owned(),
        SelectionSpec::new(3, 0),
        TEXT,
        LineIndexBudget::new(16),
        HistoryBudget::new(16, 4096),
    )?;
    let snapshot = buffer.snapshot();
    let parsed = parse(snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let version = FindVersion::new(1, 0, 0);
    let results = find_matches(snapshot, version, "a\n\nb", FindCase::Sensitive, FIND)?;
    let context = EditContext::new(EditMode::Visual).with_find_query_generation(0);
    let plan = plan_find_replace(
        &results,
        &version,
        &parsed,
        buffer.selection(),
        &context,
        FindReplacement::All("c"),
        edit_budget(),
    )?;
    let PlanOutcome::Changed(plan) = plan else {
        return Err("expected changed plan".into());
    };
    assert_eq!(plan.group_reason(), GroupReason::BulkReplace);
    let validated = validate_change(&plan, snapshot, &parsed, buffer.selection(), &context)?;
    assert_eq!(
        validated.batch().patches(),
        &[(0..4, "c".into()), (6..10, "c".into())]
    );
    assert!(matches!(
        buffer.apply(
            validated.batch(),
            validated.target_selection(),
            OperationSource::Command,
            0,
            TEXT
        )?,
        ApplyOutcome::Changed(_)
    ));
    assert_eq!(buffer.text(TEXT)?.as_ref(), "c\n\nc");
    assert_eq!(buffer.undo_depth(), 1);
    buffer.undo(TEXT)?;
    assert_eq!(buffer.text(TEXT)?.as_ref(), original);
    assert_eq!(buffer.selection().anchor().byte_offset(), 3);
    assert_eq!(buffer.selection().head().byte_offset(), 0);
    Ok(())
}

#[test]
fn find_replace_visual_refuses_deleting_an_invisible_paragraph_gap() -> TestResult {
    let original = "first\n\nsecond";
    let mut buffer = StringBuffer::new(
        original.to_owned(),
        SelectionSpec::new(1, 1),
        TEXT,
        LineIndexBudget::new(16),
        HistoryBudget::new(16, 4096),
    )?;
    let snapshot = buffer.snapshot();
    let parsed = parse(snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let version = FindVersion::new(1, 0, 0);
    let results = find_matches(snapshot, version, "\n\n", FindCase::Sensitive, FIND)?;
    let replacement = FindReplacement::Current {
        match_index: 0,
        text: "",
    };
    assert!(matches!(
        plan_find_replace(
            &results,
            &version,
            &parsed,
            buffer.selection(),
            &EditContext::new(EditMode::Visual),
            replacement,
            edit_budget()
        ),
        Err(EditRefusal::RereadFailed {
            reason: RereadFailure::Projection,
            ..
        })
    ));
    assert_eq!(buffer.text(TEXT)?.as_ref(), original);
    assert_eq!(buffer.selection().head().byte_offset(), 1);
    assert_eq!(buffer.undo_depth(), 0);
    assert_eq!(buffer.redo_depth(), 0);

    let context = EditContext::new(EditMode::Source).with_find_query_generation(0);
    let plan = plan_find_replace(
        &results,
        &version,
        &parsed,
        buffer.selection(),
        &context,
        replacement,
        edit_budget(),
    )?;
    let PlanOutcome::Changed(plan) = plan else {
        return Err("expected a Source plan".into());
    };
    let validated = validate_change(&plan, snapshot, &parsed, buffer.selection(), &context)?;
    buffer.apply(
        validated.batch(),
        validated.target_selection(),
        OperationSource::Command,
        0,
        TEXT,
    )?;
    assert_eq!(buffer.text(TEXT)?.as_ref(), "firstsecond");
    Ok(())
}

#[test]
fn find_replace_visual_requires_a_complete_aligned_entity_or_exact_text() -> TestResult {
    let original = "a &amp; z";
    let buffer = StringBuffer::new(
        original.to_owned(),
        SelectionSpec::new(1, 0),
        TEXT,
        LineIndexBudget::new(16),
        HistoryBudget::new(16, 4096),
    )?;
    let snapshot = buffer.snapshot();
    let revision = snapshot.revision();
    let parsed = parse(snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let version = FindVersion::new(1, 0, 3);
    let visual = EditContext::new(EditMode::Visual).with_find_query_generation(3);

    for query in ["amp", "a &am", "amp; z"] {
        let partial = find_matches(snapshot, version, query, FindCase::Sensitive, FIND)?;
        assert!(
            matches!(
                plan_find_replace(
                    &partial,
                    &version,
                    &parsed,
                    buffer.selection(),
                    &visual,
                    FindReplacement::Current {
                        match_index: 0,
                        text: "lt",
                    },
                    edit_budget()
                ),
                Err(EditRefusal::RereadFailed {
                    reason: RereadFailure::Projection,
                    ..
                })
            ),
            "{query}"
        );
    }
    assert_eq!(buffer.text(TEXT)?.as_ref(), original);
    assert_eq!(buffer.selection().anchor().byte_offset(), 1);
    assert_eq!(buffer.selection().head().byte_offset(), 0);
    assert_eq!(buffer.snapshot().revision(), revision);
    assert_eq!(buffer.undo_depth(), 0);
    assert_eq!(buffer.redo_depth(), 0);

    for (query, replacement, expected) in [
        ("&amp;", "&lt;", "a < z"),
        ("z", "x", "a & x"),
        ("a &amp;", "b &lt;", "b < z"),
    ] {
        let hits = find_matches(snapshot, version, query, FindCase::Sensitive, FIND)?;
        let outcome = plan_find_replace(
            &hits,
            &version,
            &parsed,
            buffer.selection(),
            &visual,
            FindReplacement::Current {
                match_index: 0,
                text: replacement,
            },
            edit_budget(),
        )?;
        let PlanOutcome::Changed(plan) = outcome else {
            return Err("expected a visible replacement".into());
        };
        let change = validate_change(&plan, snapshot, &parsed, buffer.selection(), &visual)?;
        assert_eq!(change.preview().drawn_text(), expected);
    }
    Ok(())
}

#[test]
fn find_replace_requires_the_current_query_generation_when_consumed() -> TestResult {
    let snapshot = TextSnapshot::new("word word".into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(0)?)?;
    let source = EditContext::new(EditMode::Source);
    for generation in [0, 13] {
        let version = FindVersion::new(1, 0, generation);
        let results = find_matches(&snapshot, version, "word", FindCase::Sensitive, FIND)?;
        let outcome = plan_find_replace(
            &results,
            &version,
            &parsed,
            &selection,
            &source,
            FindReplacement::All("new"),
            edit_budget(),
        )?;
        let PlanOutcome::Changed(plan) = outcome else {
            return Err("expected a change for both matches".into());
        };
        for context in [source, source.with_find_query_generation(generation + 1)] {
            assert!(matches!(
                validate_change(&plan, &snapshot, &parsed, &selection, &context),
                Err(EditRefusal::StaleFindResults)
            ));
        }
        validate_change(
            &plan,
            &snapshot,
            &parsed,
            &selection,
            &source.with_find_query_generation(generation),
        )?;
    }
    Ok(())
}

#[test]
fn find_replace_visual_refuses_matches_with_hidden_bytes_or_open_separator_edges() -> TestResult {
    let snapshot = TextSnapshot::new("left\n\nright **bold** next\n\nlast".into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(0)?)?;
    let version = FindVersion::new(1, 0, 0);
    let context = EditContext::new(EditMode::Visual).with_find_query_generation(0);
    for query in [
        "left\n",
        "\nright",
        "right **bold",
        "bold** next",
        "next\n\n",
    ] {
        let hits = find_matches(&snapshot, version, query, FindCase::Sensitive, FIND)?;
        assert!(
            matches!(
                plan_find_replace(
                    &hits,
                    &version,
                    &parsed,
                    &selection,
                    &context,
                    FindReplacement::All("replacement"),
                    edit_budget()
                ),
                Err(EditRefusal::RereadFailed {
                    reason: RereadFailure::Projection,
                    ..
                })
            ),
            "{query:?}"
        );
    }
    Ok(())
}

#[test]
fn find_replace_all_visual_refuses_mixed_visible_and_invisible_hits_as_one_batch() -> TestResult {
    let original = "```\na\n\nb\n```\n\nleft\n\nright";
    let buffer = StringBuffer::new(
        original.to_owned(),
        SelectionSpec::new(2, 1),
        TEXT,
        LineIndexBudget::new(16),
        HistoryBudget::new(16, 4096),
    )?;
    let snapshot = buffer.snapshot();
    let revision = snapshot.revision();
    let parsed = parse(snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let version = FindVersion::new(1, 0, 0);
    let results = find_matches(snapshot, version, "\n\n", FindCase::Sensitive, FIND)?;
    let context = EditContext::new(EditMode::Visual);
    assert_eq!(results.matches().len(), 3);
    let first = plan_find_replace(
        &results,
        &version,
        &parsed,
        buffer.selection(),
        &context,
        FindReplacement::Current {
            match_index: 0,
            text: "",
        },
        edit_budget(),
    )?;
    assert!(matches!(first, PlanOutcome::Changed(_)));
    assert!(matches!(
        plan_find_replace(
            &results,
            &version,
            &parsed,
            buffer.selection(),
            &context,
            FindReplacement::All(""),
            edit_budget()
        ),
        Err(EditRefusal::RereadFailed {
            reason: RereadFailure::Projection,
            ..
        })
    ));
    assert_eq!(buffer.text(TEXT)?.as_ref(), original);
    assert_eq!(buffer.selection().anchor().byte_offset(), 2);
    assert_eq!(buffer.selection().head().byte_offset(), 1);
    assert_eq!(buffer.snapshot().revision(), revision);
    assert_eq!(buffer.undo_depth(), 0);
    assert_eq!(buffer.redo_depth(), 0);
    Ok(())
}

#[test]
fn find_replace_refuses_stale_document_working_version_query_and_snapshot() -> TestResult {
    let source = TextSnapshot::new("x x".into(), TEXT)?;
    let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(source.src_offset(0)?, source.src_offset(0)?)?;
    let version = FindVersion::new(1, (0, 0), 0);
    let results = find_matches(&source, version, "x", FindCase::Sensitive, FIND)?;
    for stale in [
        FindVersion::new(2, (0, 0), 0),
        FindVersion::new(1, (0, 1), 0),
        FindVersion::new(1, (0, 0), 1),
    ] {
        assert!(matches!(
            plan_find_replace(
                &results,
                &stale,
                &parsed,
                &selection,
                &EditContext::new(EditMode::Source),
                FindReplacement::Current {
                    match_index: 0,
                    text: "y"
                },
                edit_budget()
            ),
            Err(EditRefusal::StaleFindResults)
        ));
    }
    let same_text = TextSnapshot::new("x x".into(), TEXT)?;
    assert!(matches!(
        results.validate(&same_text, &version),
        Err(EditRefusal::WrongSnapshot)
    ));
    Ok(())
}

#[test]
fn find_replace_distinguishes_empty_missing_and_identical_replacements() -> TestResult {
    let source = TextSnapshot::new("x x".into(), TEXT)?;
    let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(source.src_offset(0)?, source.src_offset(0)?)?;
    let version = FindVersion::new(1, 0, 0);
    let context = EditContext::new(EditMode::Source);
    for (query, expected) in [("", "EmptyFindQuery"), ("y", "FindMatchUnavailable")] {
        let results = find_matches(&source, version, query, FindCase::Sensitive, FIND)?;
        assert!(matches!(
            plan_find_replace(&results, &version, &parsed, &selection, &context,
                FindReplacement::All("z"), edit_budget()),
            Err(error) if error.as_str() == expected
        ));
    }
    let results = find_matches(&source, version, "x", FindCase::Sensitive, FIND)?;
    assert!(matches!(
        plan_find_replace(
            &results,
            &version,
            &parsed,
            &selection,
            &context,
            FindReplacement::All("x"),
            edit_budget()
        )?,
        PlanOutcome::Unchanged(_)
    ));
    assert!(matches!(
        plan_find_replace(
            &results,
            &version,
            &parsed,
            &selection,
            &context,
            FindReplacement::Current {
                match_index: usize::MAX,
                text: ""
            },
            edit_budget()
        ),
        Err(EditRefusal::FindMatchUnavailable)
    ));
    let plan = plan_find_replace(
        &results,
        &version,
        &parsed,
        &selection,
        &context,
        FindReplacement::Current {
            match_index: 1,
            text: "z",
        },
        edit_budget(),
    )?;
    let PlanOutcome::Changed(plan) = plan else {
        return Err("expected changed plan".into());
    };
    assert_eq!(plan.batch().patches(), &[(2..3, "z".into())]);
    Ok(())
}

#[test]
fn find_replace_refuses_protected_or_hidden_visual_changes_before_any_patch() -> TestResult {
    let source = TextSnapshot::new(
        "---\nt: name\n---\n\nname [label](https://old)\n".into(),
        TEXT,
    )?;
    let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(source.src_offset(0)?, source.src_offset(0)?)?;
    let version = FindVersion::new(1, 0, 0);
    let names = find_matches(&source, version, "name", FindCase::Sensitive, FIND)?;
    let visual = EditContext::new(EditMode::Visual);
    assert!(matches!(
        plan_find_replace(
            &names,
            &version,
            &parsed,
            &selection,
            &visual,
            FindReplacement::All("new"),
            edit_budget()
        ),
        Err(EditRefusal::ProtectedRange {
            kind: ProtectionKind::FrontMatter,
            ..
        })
    ));
    let links = find_matches(&source, version, "https://old", FindCase::Sensitive, FIND)?;
    assert!(matches!(
        plan_find_replace(
            &links,
            &version,
            &parsed,
            &selection,
            &visual,
            FindReplacement::Current {
                match_index: 0,
                text: "https://new"
            },
            edit_budget()
        ),
        Err(EditRefusal::RereadFailed {
            reason: RereadFailure::Projection,
            ..
        })
    ));
    let mixed = find_matches(
        &source,
        version,
        "[label](https://old)",
        FindCase::Sensitive,
        FIND,
    )?;
    assert!(matches!(
        plan_find_replace(
            &mixed,
            &version,
            &parsed,
            &selection,
            &visual,
            FindReplacement::Current {
                match_index: 0,
                text: "[label](https://new)"
            },
            edit_budget()
        ),
        Err(EditRefusal::RereadFailed {
            reason: RereadFailure::Projection,
            ..
        })
    ));
    let label = find_matches(&source, version, "label", FindCase::Sensitive, FIND)?;
    assert!(matches!(
        plan_find_replace(
            &label,
            &version,
            &parsed,
            &selection,
            &visual,
            FindReplacement::Current {
                match_index: 0,
                text: "*label*"
            },
            edit_budget()
        ),
        Err(EditRefusal::RereadFailed {
            reason: RereadFailure::HiddenSourceChanged,
            ..
        })
    ));
    let plan = plan_find_replace(
        &links,
        &version,
        &parsed,
        &selection,
        &EditContext::new(EditMode::Source),
        FindReplacement::Current {
            match_index: 0,
            text: "https://new",
        },
        edit_budget(),
    )?;
    assert!(matches!(plan, PlanOutcome::Changed(_)));
    Ok(())
}

#[test]
fn find_replace_all_refuses_when_a_later_match_touches_md11_protection() -> TestResult {
    for (text, query, kind) in [
        ("toc page\n\n[toc]\n", "toc", ProtectionKind::Toc),
        (
            "label paragraph [^label]\n\n[^label]: text\n",
            "label",
            ProtectionKind::FootnoteReference,
        ),
        (
            "text paragraph\n\n[^label]: text\n",
            "text",
            ProtectionKind::FootnoteDefinition,
        ),
    ] {
        let source = TextSnapshot::new(text.to_owned(), TEXT)?;
        let parsed = parse(&source, ParseConfig::m0(), ParseBudget::m0())?;
        let selection = SrcSelection::new(source.src_offset(0)?, source.src_offset(0)?)?;
        let version = FindVersion::new(1, 0, 0);
        let results = find_matches(&source, version, query, FindCase::Sensitive, FIND)?;
        assert!(results.matches().len() > 1);
        assert!(matches!(
            plan_find_replace(&results, &version, &parsed, &selection,
                &EditContext::new(EditMode::Visual), FindReplacement::All("new"), edit_budget()),
            Err(EditRefusal::ProtectedRange { kind: actual, .. }) if actual == kind
        ));
        assert!(matches!(
            plan_find_replace(
                &results,
                &version,
                &parsed,
                &selection,
                &EditContext::new(EditMode::Source),
                FindReplacement::All("new"),
                edit_budget()
            )?,
            PlanOutcome::Changed(_)
        ));
    }
    Ok(())
}
