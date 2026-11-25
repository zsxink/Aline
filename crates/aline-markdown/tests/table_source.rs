//! 表格源码元数据的独立范围金样，先于实现冻结。

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    ParseBudget, ParseConfig, PresentationKind, PresentationNode, parse, table_source,
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

#[test]
fn source_cells_keep_escaped_pipes_empty_cells_and_missing_tail() -> Result<(), Box<dyn Error>> {
    let source = "| a\\|b | `x` | |\n| :--- | ---: | --- |\n| c |";
    let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(4096))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let node = table(parsed.presentation()).ok_or("missing table")?;
    let source_table = table_source(&parsed, node.source_range(), TextBudget::new(4096))?;
    assert_eq!(source_table.rows().len(), 2);
    assert_eq!(source_table.rows()[0].cells().len(), 3);
    assert_eq!(source_table.rows()[1].cells().len(), 3);
    let expected = [[" a\\|b ", " `x` ", " "], [" c ", "", ""]];
    for (row, expected) in source_table.rows().iter().zip(expected) {
        for (cell, expected) in row.cells().iter().zip(expected) {
            assert_eq!(
                snapshot
                    .slice(cell.source_range(), TextBudget::new(4096))?
                    .as_ref(),
                expected
            );
        }
    }
    assert_eq!(source_table.separator().cells().len(), 3);
    Ok(())
}

#[test]
fn nested_source_rows_preserve_container_prefixes_and_crlf() -> Result<(), Box<dyn Error>> {
    for source in [
        "> | a | b |\r\n> | --- | --- |\r\n> | c | d |",
        "- | a | b |\r\n  | --- | --- |\r\n  | c | d |",
    ] {
        let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(4096))?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let node = table(parsed.presentation()).ok_or("missing nested table")?;
        let source_table = table_source(&parsed, node.source_range(), TextBudget::new(4096))?;
        assert_eq!(source_table.rows().len(), 2);
        assert_eq!(source_table.rows()[0].line_ending(), "\r\n");
        assert_eq!(source_table.rows()[1].line_ending(), "");
        assert_eq!(
            snapshot
                .slice(source_table.rows()[1].prefix_range(), TextBudget::new(4096))?
                .len(),
            2
        );
    }
    Ok(())
}

#[test]
fn escaped_terminal_pipe_is_cell_content_only() -> Result<(), Box<dyn Error>> {
    let budget = TextBudget::new(4096);
    let snapshot = TextSnapshot::new("A|B|C\n---|---|---\na\\|\t \nb\\| |\t ".into(), budget)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let node = table(parsed.presentation()).ok_or("missing table")?;
    let source = table_source(&parsed, node.source_range(), budget)?;
    let [_, escaped, delimited] = source.rows() else {
        return Err("expected a header and two short rows".into());
    };
    assert!(!escaped.has_outer_end());
    assert!(delimited.has_outer_end());
    for (row, expected) in [(escaped, "a\\|\t "), (delimited, "b\\| ")] {
        let first = row.cells().first().ok_or("missing cell")?;
        assert_eq!(snapshot.slice(first.source_range(), budget)?, expected);
        assert!(!row.cells()[1].is_present());
    }
    Ok(())
}

#[test]
fn table_source_rejects_foreign_snapshot_target() -> Result<(), Box<dyn Error>> {
    let source = "a|b\n-| -\nc|d";
    let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(4096))?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let foreign = TextSnapshot::new(source.to_owned(), TextBudget::new(4096))?;
    let target = foreign.src_range(0..source.len())?;
    assert!(table_source(&parsed, &target, TextBudget::new(4096)).is_err());
    Ok(())
}
