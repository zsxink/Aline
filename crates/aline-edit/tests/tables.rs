//! M3 表格独立源码金样、逐格导航和写前复读回归。

use aline_buffer::{PatchBudget, SelectionSpec, SrcSelection, TextBudget, TextSnapshot};
use aline_edit::{
    EditBudget, EditContext, EditMode, PlanOutcome, RawBlockEdit, TableCommand, TableNavigation,
    TableOperation, TableTarget, plan_raw_block_input, plan_table_command,
    plan_table_missing_input, table_navigation, validate_change,
};
use aline_markdown::{
    ParseBudget, ParseConfig, Parsed, PresentationKind, PresentationNode, TableAlignment, parse,
};
use std::error::Error;

fn table(nodes: &[PresentationNode]) -> Option<&PresentationNode> {
    nodes.iter().find_map(|node| {
        if matches!(node.kind(), PresentationKind::Table { .. }) {
            Some(node)
        } else {
            table(node.children())
        }
    })
}
fn fixture(source: &str) -> Result<(Parsed, SrcSelection, TableTarget), Box<dyn Error>> {
    let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(1 << 20))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let range = table(parsed.presentation())
        .ok_or("missing table")?
        .source_range()
        .clone();
    let selection = SrcSelection::new(
        snapshot.src_offset(range.byte_range().start)?,
        snapshot.src_offset(range.byte_range().start)?,
    )?;
    Ok((parsed, selection, TableTarget::new(range, 1, 0)))
}
fn budget() -> EditBudget {
    EditBudget::new(
        PatchBudget::new(100, 1 << 20, 1 << 20),
        TextBudget::new(1 << 20),
        ParseBudget::m0(),
    )
}
fn apply(
    source: &str,
    row: usize,
    column: usize,
    operation: TableOperation,
) -> Result<(String, SelectionSpec), Box<dyn Error>> {
    let (parsed, selection, target) = fixture(source)?;
    let target = TableTarget::new(target.table_range().clone(), row, column);
    let context = EditContext::new(EditMode::Visual);
    let outcome = plan_table_command(
        &parsed,
        &selection,
        &context,
        &TableCommand::new(target, operation),
        budget(),
    )?;
    let PlanOutcome::Changed(plan) = outcome else {
        return Ok((source.to_owned(), SelectionSpec::new(0, 0)));
    };
    validate_change(&plan, parsed.snapshot(), &parsed, &selection, &context)?;
    let mut actual = source.to_owned();
    for (range, replacement) in plan.batch().patches().iter().rev() {
        actual.replace_range(range.clone(), replacement);
    }
    Ok((actual, plan.target_selection()))
}

#[test]
fn table_header_swap_keeps_separator_alignment_and_tail_bytes() -> Result<(), Box<dyn Error>> {
    let before = "前\n\n| 名 | 年龄 |\n| :--- | ---: |\n| 甲 | 8 |\n| 乙 | 9 |\n\n后";
    let expected = "前\n\n| 乙 | 9 |\n| :--- | ---: |\n| 甲 | 8 |\n| 名 | 年龄 |\n\n后";
    assert_eq!(
        apply(before, 2, 1, TableOperation::PromoteHeader)?.0,
        expected
    );
    Ok(())
}
#[test]
fn table_column_move_keeps_escaped_pipe_code_and_corresponding_alignment()
-> Result<(), Box<dyn Error>> {
    let before = "| A | B |\n| :--- | ---: |\n| a\\|b | `x` |";
    let expected = "| B | A |\n| ---: | :--- |\n| `x` | a\\|b |";
    assert_eq!(
        apply(before, 1, 1, TableOperation::MoveColumn { to: 0 })?.0,
        expected
    );
    Ok(())
}
#[test]
fn table_tidy_pads_short_rows_and_preserves_mixed_endings() -> Result<(), Box<dyn Error>> {
    let before = "A|B\r\n:---|---:\n x\\|y | `a\\|b`\r\n短";
    let expected = "| A | B |\r\n| :--- | ---: |\n| x\\|y | `a\\|b` |\r\n| 短 |  |";
    assert_eq!(apply(before, 1, 0, TableOperation::Tidy)?.0, expected);
    Ok(())
}
#[test]
fn table_existing_cell_navigation_has_no_text_plan_and_last_cell_appends()
-> Result<(), Box<dyn Error>> {
    let before = "| A | B |\n| --- | --- |\n| 甲 | 1 |";
    let (parsed, _, target) = fixture(before)?;
    let TableNavigation::Cell { selection, .. } =
        table_navigation(&parsed, &target, false, budget().text())?
    else {
        return Err("expected navigation".into());
    };
    assert_eq!(selection, SelectionSpec::new(32, 32));
    let last = TableTarget::new(target.table_range().clone(), 1, 1);
    assert!(matches!(
        table_navigation(&parsed, &last, false, budget().text())?,
        TableNavigation::AppendRow(_)
    ));
    let (after, selection) = apply(before, 1, 1, TableOperation::InsertRow { after: true })?;
    assert_eq!(after, "| A | B |\n| --- | --- |\n| 甲 | 1 |\n|  |  |");
    assert_eq!(selection, SelectionSpec::new(39, 39));
    Ok(())
}

#[test]
fn raw_cell_input_preserves_top_level_and_nested_table_structure() -> Result<(), Box<dyn Error>> {
    let context = EditContext::new(EditMode::Visual);
    for source in [
        "前\n\n| A | B |\r\n| --- | --- |\n| 甲 | `x` |\r\n\n后",
        "> | A | B |\n> | --- | --- |\n> | 甲 | `x` |\n\n后",
        "- | A | B |\r\n  | --- | --- |\r\n  | 甲 | `x` |\r\n  \r\n  后",
    ] {
        let (parsed, selection, _) = fixture(source)?;
        let snapshot = parsed.snapshot();
        let start = source.find('甲').ok_or("missing target")?;
        let end = start + '甲'.len_utf8();
        let patch = snapshot.src_range(start..end)?;
        let block = parsed
            .blocks()
            .iter()
            .find(|block| block.range().byte_range().contains(&start))
            .ok_or("missing open block")?;
        for invalid in ["甲|乙", "甲\n乙", "甲\r乙"] {
            assert!(
                plan_raw_block_input(
                    &parsed,
                    &selection,
                    &context,
                    block.range(),
                    RawBlockEdit::new(&patch, "甲", invalid),
                    budget(),
                )
                .is_err(),
                "invalid cell input {invalid:?} in {source:?}"
            );
        }
        let valid = "乙\\|丙";
        let PlanOutcome::Changed(plan) = plan_raw_block_input(
            &parsed,
            &selection,
            &context,
            block.range(),
            RawBlockEdit::new(&patch, "甲", valid),
            budget(),
        )?
        else {
            return Err("expected a raw cell plan".into());
        };
        let checked = validate_change(&plan, snapshot, &parsed, &selection, &context)?;
        assert_eq!(checked.batch().patches(), &[(start..end, valid.to_owned())]);
        assert_eq!(
            checked.target_selection(),
            SelectionSpec::new(start + valid.len(), start + valid.len())
        );
        assert_eq!(snapshot.len_bytes(), source.len());
    }
    Ok(())
}

#[test]
fn short_row_navigation_and_materialization_use_exact_logical_column() -> Result<(), Box<dyn Error>>
{
    let context = EditContext::new(EditMode::Visual);
    for (source, row, insertion) in [
        (
            "| A | B | C |\n| --- | --- | --- |\n| 甲 |",
            "| 甲 |",
            "  | 乙\\|丙 |",
        ),
        (
            "> A|B|C\r\n> ---|---|---\r\n> 甲\r\n\r\n后",
            "> 甲",
            "|  | 乙\\|丙 |",
        ),
        (
            "- | A | B | C |\n  | --- | --- | --- |\n  | 甲 |  \n\n后",
            "  | 甲 |  ",
            "  | 乙\\|丙 |",
        ),
        ("A|B|C\n---|---|---\n甲\\|\t ", "甲\\|\t ", "|  | 乙\\|丙 |"),
    ] {
        let (parsed, selection, first_cell) = fixture(source)?;
        let snapshot = parsed.snapshot();
        let row_end = source.find(row).ok_or("missing data row")? + row.len();
        let TableNavigation::Cell {
            target: second,
            selection: second_selection,
        } = table_navigation(&parsed, &first_cell, false, budget().text())?
        else {
            return Err("expected navigation to column 1".into());
        };
        let TableNavigation::Cell {
            target: third,
            selection: third_selection,
        } = table_navigation(&parsed, &second, false, budget().text())?
        else {
            return Err("expected navigation to column 2".into());
        };
        assert_eq!(second.column(), 1);
        assert_eq!(third.column(), 2);
        assert_eq!(second_selection, SelectionSpec::new(row_end, row_end));
        assert_eq!(third_selection, second_selection);
        for invalid in ["甲|乙", "  |", "甲\n乙"] {
            assert!(
                plan_table_missing_input(&parsed, &selection, &context, &third, invalid, budget())
                    .is_err()
            );
        }
        let PlanOutcome::Changed(plan) =
            plan_table_missing_input(&parsed, &selection, &context, &third, "乙\\|丙", budget())?
        else {
            return Err("expected a missing-cell plan".into());
        };
        let checked = validate_change(&plan, snapshot, &parsed, &selection, &context)?;
        assert_eq!(
            checked.batch().patches(),
            &[(row_end..row_end, insertion.to_owned())]
        );
        let caret = row_end + insertion.len() - " |".len();
        assert_eq!(checked.target_selection(), SelectionSpec::new(caret, caret));
    }
    Ok(())
}
#[test]
fn table_row_delete_preserves_original_eof_and_header_only_validity() -> Result<(), Box<dyn Error>>
{
    assert_eq!(
        apply(
            "| A | B |\n| --- | --- |\n| 甲 | 1 |",
            1,
            0,
            TableOperation::DeleteRow
        )?
        .0,
        "| A | B |\n| --- | --- |"
    );
    Ok(())
}
#[test]
fn table_alignment_changes_only_separator_marker() -> Result<(), Box<dyn Error>> {
    let before = "| A | B |\n| --- | --- |\n| a  | `x` |  ";
    assert_eq!(
        apply(before, 1, 0, TableOperation::Align(TableAlignment::Center))?.0,
        "| A | B |\n| :---: | --- |\n| a  | `x` |  "
    );
    Ok(())
}
#[test]
fn table_nested_tidy_preserves_quote_and_list_prefixes() -> Result<(), Box<dyn Error>> {
    for (before, expected) in [
        (
            "- A|B\n  -| -\n  x|y",
            "- | A | B |\n  | --- | --- |\n  | x | y |",
        ),
        (
            " > A|B\r\n > -| -\r\n > x|y",
            " > | A | B |\r\n > | --- | --- |\r\n > | x | y |",
        ),
    ] {
        assert_eq!(apply(before, 1, 0, TableOperation::Tidy)?.0, expected);
    }
    Ok(())
}
#[test]
fn table_short_row_move_foreign_target_and_composition_reject() -> Result<(), Box<dyn Error>> {
    let source = "| A | B |\n| --- | --- |\n| a |";
    let (parsed, selection, target) = fixture(source)?;
    for operation in [
        TableOperation::MoveColumn { to: 1 },
        TableOperation::DeleteColumn,
        TableOperation::DeleteRow,
    ] {
        let command = TableCommand::new(
            if operation == TableOperation::DeleteRow {
                TableTarget::new(target.table_range().clone(), 0, 0)
            } else {
                target.clone()
            },
            operation,
        );
        assert!(
            plan_table_command(
                &parsed,
                &selection,
                &EditContext::new(EditMode::Visual),
                &command,
                budget()
            )
            .is_err()
        );
    }
    let command = TableCommand::new(target, TableOperation::Tidy);
    assert!(
        plan_table_command(
            &parsed,
            &selection,
            &EditContext::new(EditMode::Visual).with_composition_active(true),
            &command,
            budget()
        )
        .is_err()
    );
    Ok(())
}

#[test]
fn table_structure_sequence_preserves_cells_alignment_and_neighbour_bytes()
-> Result<(), Box<dyn Error>> {
    const SEED: u64 = 0x3_a17e_2026;
    let mut random = SEED;
    let mut rows = [["甲", "B", "C"], [r"x\|y", "`q`", "一"], ["乙", "2", "二"]];
    let mut alignments = [":---", "---:", ":---:"];
    let render = |rows: [[&str; 3]; 3], alignments: [&str; 3]| {
        format!(
            "前\r\n\r\n| {} |\r\n| {} |\n| {} |\r\n| {} |\n\n后",
            rows[0].join(" | "),
            alignments.join(" | "),
            rows[1].join(" | "),
            rows[2].join(" | ")
        )
    };
    let mut source = render(rows, alignments);
    let mut operations = Vec::new();
    for step in 0..48 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let adjacent = usize::from(random & 1 != 0);
        let (row, column, operation) = match random % 3 {
            0 => {
                rows.swap(1, 2);
                (1, 0, TableOperation::MoveRow { to: 2 })
            }
            1 => {
                for row in &mut rows {
                    row.swap(adjacent, adjacent + 1);
                }
                alignments.swap(adjacent, adjacent + 1);
                (0, adjacent, TableOperation::MoveColumn { to: adjacent + 1 })
            }
            _ => {
                let row = adjacent + 1;
                rows.swap(0, row);
                (row, 0, TableOperation::PromoteHeader)
            }
        };
        operations.push((row, column, operation));
        let (actual, _) = apply(&source, row, column, operation).map_err(|error| {
            format!("seed={SEED} step={step} operations={operations:?} input={source:?}: {error}")
        })?;
        assert_eq!(
            actual,
            render(rows, alignments),
            "seed={SEED} step={step} operations={operations:?} input={source:?}"
        );
        source = actual;
    }
    Ok(())
}
