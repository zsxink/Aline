//! M3 段落与列表独立样例，源码预期先于生产命令编写。

use std::error::Error;

use aline_buffer::{PatchBudget, SrcSelection, TextBudget, TextSnapshot};
use aline_edit::{
    BlockCommand, EditBudget, EditContext, EditMode, ParagraphStyle, PlanOutcome,
    plan_block_command, validate_change,
};
use aline_markdown::{ParseBudget, ParseConfig, PresentationKind, parse};

const TEXT: TextBudget = TextBudget::new(8192);
const EDIT: EditBudget = EditBudget::new(PatchBudget::new(64, 8192, 8192), TEXT, ParseBudget::m0());
const VISUAL: EditContext = EditContext::new(EditMode::Visual);
type TestResult = Result<(), Box<dyn Error>>;

fn changed(
    source: &str,
    anchor: usize,
    head: usize,
    command: BlockCommand,
    expected: &str,
) -> TestResult {
    let snapshot = TextSnapshot::new(source.into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let selection = SrcSelection::new(snapshot.src_offset(anchor)?, snapshot.src_offset(head)?)?;
    let result = plan_block_command(&parsed, &selection, &VISUAL, command, EDIT)
        .map_err(|error| format!("{command:?} {source:?}: {error}"))?;
    let PlanOutcome::Changed(plan) = result else {
        return Err("expected changed plan".into());
    };
    let checked = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
    let mut output = source.to_owned();
    for (range, replacement) in checked.batch().patches().iter().rev() {
        output.replace_range(range.clone(), replacement);
    }
    assert_eq!(output, expected, "command={command:?}, source={source:?}");
    assert_eq!(
        &*snapshot.slice(&snapshot.src_range(0..source.len())?, TEXT)?,
        source
    );
    Ok(())
}

#[test]
fn block_convert_preserves_content_and_neighbours() -> TestResult {
    for (source, anchor, head, style, expected) in [
        (
            "前\n\n甲乙\n\n后",
            5,
            11,
            ParagraphStyle::Heading(2),
            "前\n\n## 甲乙\n\n后",
        ),
        ("甲\n乙", 0, 7, ParagraphStyle::Quote, "> 甲\n> 乙"),
        (
            "甲\n乙",
            0,
            7,
            ParagraphStyle::TaskList,
            "- [ ] 甲\n- [ ] 乙",
        ),
        ("甲\n乙", 0, 7, ParagraphStyle::OrderedList, "1. 甲\n2. 乙"),
        ("甲\n乙", 0, 7, ParagraphStyle::UnorderedList, "- 甲\n- 乙"),
        ("### 甲乙", 4, 10, ParagraphStyle::Paragraph, "甲乙"),
        ("词 #", 0, 5, ParagraphStyle::Heading(1), "# 词 # #"),
        ("# 词 # #", 2, 7, ParagraphStyle::Heading(2), "## 词 # #"),
        ("> 甲\n> 乙", 2, 11, ParagraphStyle::Paragraph, "甲\n乙"),
        (
            "    x\n    y\n\n邻接",
            0,
            11,
            ParagraphStyle::CodeBlock,
            "```\nx\ny\n```\n\n邻接",
        ),
        ("甲", 0, 3, ParagraphStyle::MathBlock, "$$\n甲\n$$"),
        ("甲", 0, 3, ParagraphStyle::ThematicBreak, "---\n\n甲"),
    ] {
        changed(source, anchor, head, BlockCommand::Convert(style), expected)?;
    }
    Ok(())
}

#[test]
fn block_convert_supports_all_atx_heading_levels() -> TestResult {
    for level in 1..=6 {
        let expected = format!("前\r\n\r\n{} 甲乙\r\n\r\n后", "#".repeat(level));
        changed(
            "前\r\n\r\n甲乙\r\n\r\n后",
            7,
            13,
            BlockCommand::Convert(ParagraphStyle::Heading(u8::try_from(level)?)),
            &expected,
        )?;
    }
    Ok(())
}

#[test]
fn block_convert_keeps_fenced_code_info_and_trailing_hash_content() -> TestResult {
    for source in ["```rust\nlet x = 1;\n```", "  ~~~~rust extra\nx\n~~~~"] {
        let snapshot = TextSnapshot::new(source.into(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let caret = snapshot.src_offset(source.len() - 4)?;
        let selection = SrcSelection::new(caret.clone(), caret)?;
        assert!(matches!(
            plan_block_command(
                &parsed,
                &selection,
                &VISUAL,
                BlockCommand::Convert(ParagraphStyle::CodeBlock),
                EDIT,
            )?,
            PlanOutcome::Unchanged(_)
        ));
    }
    let original = TextSnapshot::new("词 #".into(), TEXT)?;
    let converted = TextSnapshot::new("# 词 # #".into(), TEXT)?;
    let before = parse(&original, ParseConfig::m0(), ParseBudget::m0())?;
    let after = parse(&converted, ParseConfig::m0(), ParseBudget::m0())?;
    assert_eq!(
        before.projection().drawn_text(),
        after.projection().drawn_text()
    );
    Ok(())
}

#[test]
fn lists_preserve_delimiters_task_markers_and_nested_context() -> TestResult {
    for (source, caret, command, expected) in [
        ("7. 甲", 6, BlockCommand::ListEnter, "7. 甲\n8. "),
        ("7) 甲", 6, BlockCommand::ListEnter, "7) 甲\n8) "),
        ("- [X] 甲", 9, BlockCommand::ListEnter, "- [X] 甲\n- [ ] "),
        (
            "- 父\n\n  7) 子\n\n相邻",
            15,
            BlockCommand::ListEnter,
            "- 父\n\n  7) 子\n  8) \n\n相邻",
        ),
        (
            "- 父\n  - 子\n  - ",
            18,
            BlockCommand::ListEnter,
            "- 父\n  - 子\n- ",
        ),
        ("- 父\n- ", 8, BlockCommand::ListEnter, "- 父\n"),
        ("- 甲\n- 乙", 11, BlockCommand::IndentList, "- 甲\n  - 乙"),
        (
            "-    parent\n- child",
            19,
            BlockCommand::IndentList,
            "-    parent\n     - child",
        ),
        (
            "- [x] 父\n- 子",
            15,
            BlockCommand::IndentList,
            "- [x] 父\n  - 子",
        ),
        (
            "- parent\n- child\n  - leaf\n- next",
            16,
            BlockCommand::IndentList,
            "- parent\n  - child\n    - leaf\n- next",
        ),
        (
            "7) 甲\n1) 乙",
            13,
            BlockCommand::IndentList,
            "7) 甲\n   1) 乙",
        ),
        ("- 父\n  - 子", 13, BlockCommand::OutdentList, "- 父\n- 子"),
        (
            "- [ ] 甲\n- [X] 乙",
            2,
            BlockCommand::ToggleTask,
            "- [x] 甲\n- [X] 乙",
        ),
        ("- [X] 甲", 2, BlockCommand::ToggleTask, "- [ ] 甲"),
    ] {
        changed(source, caret, caret, command, expected)?;
    }
    Ok(())
}

#[test]
fn list_enter_continues_multiline_items_from_the_parsed_marker() -> TestResult {
    for (source, caret, expected) in [
        ("- 甲\n  续行", 14, "- 甲\n  续行\n- "),
        ("- 甲\n\n  次段", 15, "- 甲\n\n  次段\n- "),
        ("- 甲\n  续行\n- 乙", 14, "- 甲\n  续行\n- \n- 乙"),
        ("- 父\n  - 子\n    子续", 24, "- 父\n  - 子\n    子续\n  - "),
        (
            "> 7) 甲\r\n>    续行\r\n> 9) 乙",
            21,
            "> 7) 甲\r\n>    续行\r\n> 8) \r\n> 9) 乙",
        ),
        (
            "- [X] 甲\n  续行\n\n相邻",
            18,
            "- [X] 甲\n  续行\n- [ ] \n\n相邻",
        ),
    ] {
        changed(source, caret, caret, BlockCommand::ListEnter, expected)?;
    }
    Ok(())
}

#[test]
fn list_enter_returns_continuation_body_to_ordinary_input() -> TestResult {
    for (source, caret) in [
        ("- 甲\n  续行", 11),
        ("- 甲\n  续行\n  后行", 14),
        ("- 甲\n  > quote\n  > body", 24),
    ] {
        let snapshot = TextSnapshot::new(source.into(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let caret = snapshot.src_offset(caret)?;
        let selection = SrcSelection::new(caret.clone(), caret)?;
        assert!(
            matches!(
                plan_block_command(&parsed, &selection, &VISUAL, BlockCommand::ListEnter, EDIT),
                Err(aline_edit::EditRefusal::ListCommandOutsideItem)
            ),
            "{source:?}"
        );
    }
    Ok(())
}

#[test]
fn indent_ordered_item_preserves_ordinal_and_parent_marker_width() -> TestResult {
    for (source, caret, expected) in [
        ("7) 甲\n8) 乙", 13, "7) 甲\n\n   8) 乙"),
        (
            "7) 甲\r\n8) 乙\r\n9) 丙",
            14,
            "7) 甲\r\n\r\n   8) 乙\r\n9) 丙",
        ),
        ("7) 甲\n\n   8) 乙", 17, "7) 甲\n\n8) 乙"),
    ] {
        let command = if source.contains("   8)") {
            BlockCommand::OutdentList
        } else {
            BlockCommand::IndentList
        };
        changed(source, caret, caret, command, expected)?;
    }
    Ok(())
}

#[test]
fn exiting_nested_empty_item_puts_caret_at_the_outdented_item_end() -> TestResult {
    let mut source = "- 父\n  - 子\n  - ".to_owned();
    for expected_text in ["- 父\n  - 子\n- ", "- 父\n  - 子\n"] {
        let snapshot = TextSnapshot::new(source.clone(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let caret = snapshot.src_offset(source.len())?;
        let selection = SrcSelection::new(caret.clone(), caret)?;
        let result =
            plan_block_command(&parsed, &selection, &VISUAL, BlockCommand::ListEnter, EDIT)?;
        let PlanOutcome::Changed(plan) = result else {
            return Err("空项应退出一级".into());
        };
        let checked = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
        let [(range, replacement)] = checked.batch().patches() else {
            return Err("空项退出应产生单个源码补丁".into());
        };
        assert_eq!((range.clone(), replacement.as_str()), (14..16, ""));
        source.replace_range(range.clone(), replacement);
        assert_eq!(source, expected_text);
        assert_eq!(
            (
                plan.target_selection().anchor(),
                plan.target_selection().head()
            ),
            (expected_text.len(), expected_text.len())
        );
    }
    Ok(())
}

#[test]
fn list_enter_does_not_treat_setext_underline_as_empty_child_item() -> TestResult {
    let source = "- 父\n  - ";
    let snapshot = TextSnapshot::new(source.into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let node = &parsed.presentation()[0].children()[0].children()[0];
    assert_eq!(node.kind(), &PresentationKind::Heading { level: 2 });
    let caret = snapshot.src_offset(source.len())?;
    let selection = SrcSelection::new(caret.clone(), caret)?;
    assert!(
        plan_block_command(&parsed, &selection, &VISUAL, BlockCommand::ListEnter, EDIT).is_err()
    );
    Ok(())
}

#[test]
fn structure_rejections_leave_inputs_untouched() -> TestResult {
    for (source, anchor, head, command) in [
        (
            "甲\n\n乙",
            0,
            8,
            BlockCommand::Convert(ParagraphStyle::Quote),
        ),
        ("- 第一", 8, 8, BlockCommand::IndentList),
        ("- 第一", 8, 8, BlockCommand::OutdentList),
        ("- \n  - 子", 2, 2, BlockCommand::ListEnter),
        (
            "- parent\n  - alpha\n  - beta\n- next",
            18,
            18,
            BlockCommand::OutdentList,
        ),
        (
            "- parent\n  - alpha\n\n  Parent paragraph\n- next",
            18,
            18,
            BlockCommand::OutdentList,
        ),
        (
            "<b>甲</b>",
            0,
            10,
            BlockCommand::Convert(ParagraphStyle::Heading(1)),
        ),
        (
            "甲[^a]\n\n[^a]: 注",
            0,
            7,
            BlockCommand::Convert(ParagraphStyle::Quote),
        ),
        (
            "甲",
            0,
            3,
            BlockCommand::Convert(ParagraphStyle::Heading(7)),
        ),
    ] {
        let snapshot = TextSnapshot::new(source.into(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let selection =
            SrcSelection::new(snapshot.src_offset(anchor)?, snapshot.src_offset(head)?)?;
        assert!(plan_block_command(&parsed, &selection, &VISUAL, command, EDIT).is_err());
        assert_eq!(
            &*snapshot.slice(&snapshot.src_range(0..source.len())?, TEXT)?,
            source
        );
        assert_eq!(selection.anchor().byte_offset(), anchor);
        assert_eq!(selection.head().byte_offset(), head);
    }
    Ok(())
}

#[test]
fn list_key_distinguishes_ordinary_paragraphs_from_refused_list_edits() -> TestResult {
    for (source, outside_list) in [("正文", true), ("> 正文", true), ("999999999. 甲", false)]
    {
        let snapshot = TextSnapshot::new(source.into(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let caret = snapshot.src_offset(source.len())?;
        let selection = SrcSelection::new(caret.clone(), caret)?;
        let result =
            plan_block_command(&parsed, &selection, &VISUAL, BlockCommand::ListEnter, EDIT);
        assert!(result.is_err(), "source={source:?}");
        assert_eq!(
            matches!(result, Err(aline_edit::EditRefusal::ListCommandOutsideItem)),
            outside_list,
            "source={source:?}"
        );
    }
    Ok(())
}
