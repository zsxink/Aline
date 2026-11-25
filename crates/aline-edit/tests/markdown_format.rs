//! 手动格式化的独立输入输出、保护、预算及过期金样。

use std::error::Error;

use aline_buffer::{PatchBudget, SrcSelection, TextBudget, TextSnapshot};
use aline_edit::{
    EditBudget, EditContext, EditMode, EditRefusal, GroupReason, PlanOutcome, plan_markdown_format,
    validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, parse};

const TEXT: TextBudget = TextBudget::new(65536);
const EDIT: EditBudget =
    EditBudget::new(PatchBudget::new(128, 65536, 65536), TEXT, ParseBudget::m0());
const VISUAL: EditContext = EditContext::new(EditMode::Visual);
type TestResult = Result<(), Box<dyn Error>>;

fn formatted(source: &str, context: &EditContext) -> Result<String, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(source.to_owned(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(0)?)?;
    let outcome = plan_markdown_format(&snapshot, &parsed, &selection, context, EDIT)?;
    let PlanOutcome::Changed(plan) = outcome else {
        return Ok(source.to_owned());
    };
    let checked = validate_change(&plan, &snapshot, &parsed, &selection, context)?;
    assert_eq!(checked.group_reason(), GroupReason::MarkdownFormat);
    let mut output = String::new();
    let mut src_off = 0;
    for (range, replacement) in checked.batch().patches() {
        output.push_str(
            source
                .get(src_off..range.start)
                .ok_or("invalid patch start")?,
        );
        output.push_str(replacement);
        src_off = range.end;
    }
    output.push_str(source.get(src_off..).ok_or("invalid patch end")?);
    Ok(output)
}

#[test]
fn markdown_format_normalizes_atx_and_preserves_mixed_line_endings() -> TestResult {
    let source = "#   甲 ###\r\n\n##\t乙\n\n尾\r\n";
    let expected = "# 甲\r\n\n## 乙\n\n尾\r\n";
    assert_eq!(formatted(source, &VISUAL)?, expected);
    assert_eq!(formatted(expected, &VISUAL)?, expected);
    assert_eq!(formatted("#   甲 ###", &VISUAL)?, "# 甲");
    Ok(())
}

#[test]
fn markdown_format_preserves_literal_unknown_and_protected_source() -> TestResult {
    let protected = "---\nkey: value\n---\n\n<div>  &amp; <!-- 注 --> </div>\n\n[toc]\n\n[^a]:  注\n\n```md\n#   字 ###\n```\n\n$$\nx + y\n$$\n\n:::unknown  属性\n原文\n:::\n\n> #   引用 ###\n\n- #   列表 ###\n\nSetext\n======\n";
    let source = format!("{protected}\n#   标题 ###");
    let expected = format!("{protected}\n# 标题");
    assert_eq!(formatted(&source, &VISUAL)?, expected);
    assert_eq!(
        formatted(&source, &EditContext::new(EditMode::Source))?,
        expected
    );
    Ok(())
}

#[test]
fn markdown_format_tidies_tables_without_rewriting_cell_source() -> TestResult {
    let source = "前\n\na|b\r\n:---|---:\n x\\|y | `a\\|b`\r\n| 短 |\n\n后";
    let expected = "前\n\n| a | b |\r\n| :--- | ---: |\n| x\\|y | `a\\|b` |\r\n| 短 |  |\n\n后";
    assert_eq!(formatted(source, &VISUAL)?, expected);
    assert_eq!(formatted(expected, &VISUAL)?, expected);
    Ok(())
}

#[test]
fn markdown_format_keeps_reverse_selection_and_rejects_stale_binding() -> TestResult {
    let snapshot = TextSnapshot::new("#   标题 ###\n\n尾".to_owned(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(
        snapshot.src_offset(snapshot.len_bytes())?,
        snapshot.src_offset(4)?,
    )?;
    let PlanOutcome::Changed(plan) =
        plan_markdown_format(&snapshot, &parsed, &selection, &VISUAL, EDIT)?
    else {
        return Err("expected format plan".into());
    };
    assert_eq!(plan.target_selection().anchor(), 13);
    assert_eq!(plan.target_selection().head(), 2);
    let other = TextSnapshot::new("#   标题 ###\n\n尾".to_owned(), TEXT)?;
    let other_parsed = parse(&other, ParseConfig::m0(), ParseBudget::m0())?;
    let other_selection =
        SrcSelection::new(other.src_offset(other.len_bytes())?, other.src_offset(4)?)?;
    assert!(matches!(
        validate_change(&plan, &other, &other_parsed, &other_selection, &VISUAL),
        Err(EditRefusal::WrongSnapshot)
    ));
    Ok(())
}

#[test]
fn markdown_format_rejects_budget_and_composition_before_any_change() -> TestResult {
    let source = "#   标题\n\n##   后";
    let snapshot = TextSnapshot::new(source.to_owned(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(0)?)?;
    let budget = EditBudget::new(PatchBudget::new(1, 65536, 65536), TEXT, ParseBudget::m0());
    assert!(matches!(
        plan_markdown_format(&snapshot, &parsed, &selection, &VISUAL, budget),
        Err(EditRefusal::BudgetExceeded { .. })
    ));
    let active = VISUAL.with_composition_active(true);
    assert!(matches!(
        plan_markdown_format(&snapshot, &parsed, &selection, &active, EDIT),
        Err(EditRefusal::CompositionActive)
    ));
    let pending = VISUAL.with_pending_input(true);
    assert!(matches!(
        plan_markdown_format(&snapshot, &parsed, &selection, &pending, EDIT),
        Err(EditRefusal::PendingInput)
    ));
    assert_eq!(
        snapshot
            .slice(&snapshot.src_range(0..source.len())?, TEXT)?
            .as_ref(),
        source
    );
    Ok(())
}

#[test]
fn markdown_format_preserves_escaped_hashes_and_leaves_canonical_empty_headings_unchanged()
-> TestResult {
    for (source, expected) in [
        ("#   ", "#"),
        ("##  ###", "##"),
        ("   ###\t字 ###  ", "   ### 字"),
        ("#   字 \\###", "# 字 \\###"),
        ("#   字###", "# 字###"),
        ("#######  原文 ###", "#######  原文 ###"),
        ("#字", "#字"),
    ] {
        assert_eq!(formatted(source, &VISUAL)?, expected, "source={source:?}");
        assert_eq!(
            formatted(expected, &VISUAL)?,
            expected,
            "expected={expected:?}"
        );
    }
    Ok(())
}

#[test]
fn markdown_format_refuses_parser_fallback_without_publishing_a_partial_plan() -> TestResult {
    let source = "#   字\n\n段落";
    let snapshot = TextSnapshot::new(source.to_owned(), TEXT)?;
    let parsed = parse(
        &snapshot,
        ParseConfig::m0(),
        ParseBudget::m0().with_max_nodes(0),
    )?;
    let selection = SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(0)?)?;
    assert!(matches!(
        plan_markdown_format(&snapshot, &parsed, &selection, &VISUAL, EDIT),
        Err(EditRefusal::RereadFailed { .. })
    ));
    Ok(())
}

#[test]
fn markdown_format_preserves_hash_prefixed_paragraph_and_formats_adjacent_heading() -> TestResult {
    let source = "###标题 ###\r\n\r\n###  标题 ###\r\n\r\n| A |B|\r\n| --- | --- |\r\n| x |\r\n\r\n<!-- 保留  注释 -->\n";
    let expected = "###标题 ###\r\n\r\n### 标题\r\n\r\n| A | B |\r\n| --- | --- |\r\n| x |  |\r\n\r\n<!-- 保留  注释 -->\n";
    assert_eq!(
        formatted("###标题 ###\r\n普通\n", &VISUAL)?,
        "###标题 ###\r\n普通\n"
    );
    assert_eq!(formatted(source, &VISUAL)?, expected);
    assert_eq!(formatted(expected, &VISUAL)?, expected);
    Ok(())
}
