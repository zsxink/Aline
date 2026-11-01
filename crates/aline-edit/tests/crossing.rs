//! F-11 独立删除、粘贴与 HTML 跨界金样。

use std::error::Error;
use std::io;
use std::ops::Range;

use aline_buffer::{
    ApplyOutcome, CheckedBatch, HistoryBudget, LineIndexBudget, OperationSource, PatchBudget,
    SelectionSpec, SrcSelection, StringBuffer, TextBudget, TextSnapshot,
};
use aline_edit::{
    ChangePlan, EditBudget, EditCommand, EditContext, EditMode, EditRefusal, InlineFormat,
    PlanOutcome, SourceEdit, plan_change,
};
use aline_markdown::{ParseBudget, ParseConfig, Parsed, ProtectionKind, parse};

const TEXT_LIMIT: usize = 1 << 20;
const PATCH_LIMIT: usize = 64;
type TestResult = Result<(), Box<dyn Error>>;

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

fn decode(raw: &str) -> Result<String, Box<dyn Error>> {
    let inner = raw
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .ok_or_else(|| invalid(format!("not a quoted TSV string: {raw}")))?;
    let mut result = String::new();
    let mut chars = inner.chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('t') => result.push('\t'),
            Some('\\') => result.push('\\'),
            Some('"') => result.push('"'),
            other => return Err(invalid(format!("bad TSV escape: {other:?}"))),
        }
    }
    Ok(result)
}

fn string(row: &[&str], index: usize) -> Result<String, Box<dyn Error>> {
    decode(row[index])
}

fn number(row: &[&str], index: usize) -> Result<usize, Box<dyn Error>> {
    Ok(row[index].parse()?)
}

fn range(raw: &str) -> Result<Range<usize>, Box<dyn Error>> {
    let (start, end) = raw
        .split_once("..")
        .ok_or_else(|| invalid(format!("not a source range: {raw}")))?;
    Ok(start.parse()?..end.parse()?)
}

fn budget() -> EditBudget {
    EditBudget::new(
        PatchBudget::new(PATCH_LIMIT, TEXT_LIMIT, TEXT_LIMIT),
        TextBudget::new(TEXT_LIMIT),
        ParseBudget::m0(),
    )
}

fn parse_buffer(buffer: &StringBuffer) -> Result<Parsed, Box<dyn Error>> {
    Ok(parse(
        buffer.snapshot(),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )?)
}

fn make_buffer(source: &str, anchor: usize, head: usize) -> Result<StringBuffer, Box<dyn Error>> {
    Ok(StringBuffer::new(
        source.to_owned(),
        SelectionSpec::new(anchor, head),
        TextBudget::new(TEXT_LIMIT),
        LineIndexBudget::new(4096),
        HistoryBudget::new(128, TEXT_LIMIT),
    )?)
}

fn text_of(buffer: &StringBuffer) -> Result<String, Box<dyn Error>> {
    Ok(buffer.text(TextBudget::new(TEXT_LIMIT))?.into_owned())
}

#[derive(Debug, PartialEq)]
struct State {
    text: String,
    anchor: usize,
    head: usize,
    revision: aline_buffer::DocumentRevision,
    undo_depth: usize,
    redo_depth: usize,
    nodes: usize,
}

fn capture(buffer: &StringBuffer) -> Result<State, Box<dyn Error>> {
    Ok(State {
        text: text_of(buffer)?,
        anchor: buffer.selection().anchor().byte_offset(),
        head: buffer.selection().head().byte_offset(),
        revision: buffer.revision(),
        undo_depth: buffer.undo_depth(),
        redo_depth: buffer.redo_depth(),
        nodes: buffer.history_node_count(),
    })
}

fn seed_redo(buffer: &mut StringBuffer, selection: SelectionSpec) -> TestResult {
    let end = buffer.len_bytes();
    let batch = CheckedBatch::new(
        buffer.snapshot(),
        vec![(end..end, "Q".to_owned())],
        PatchBudget::new(PATCH_LIMIT, TEXT_LIMIT, TEXT_LIMIT),
    )?;
    let outcome = buffer.apply(
        &batch,
        selection,
        OperationSource::Replace,
        0,
        TextBudget::new(TEXT_LIMIT),
    )?;
    if !matches!(outcome, ApplyOutcome::Changed(_)) {
        return Err(invalid("redo fixture did not create a change"));
    }
    buffer.undo(TextBudget::new(TEXT_LIMIT))?;
    if buffer.redo_depth() != 1 {
        return Err(invalid("redo fixture did not retain exactly one branch"));
    }
    Ok(())
}

fn selection(snapshot: &TextSnapshot, row: &[&str]) -> Result<SrcSelection, Box<dyn Error>> {
    Ok(SrcSelection::new(
        snapshot.src_offset(number(row, 4)?)?,
        snapshot.src_offset(number(row, 5)?)?,
    )?)
}

fn command(row: &[&str]) -> Result<EditCommand, Box<dyn Error>> {
    match row[6] {
        "Replace" => Ok(EditCommand::replace(vec![SourceEdit::new(
            range(row[7])?,
            string(row, 8)?,
            string(row, 9)?,
        )])),
        "ToggleInlineFormat" => {
            let format = match row[10] {
                "Strong" => InlineFormat::Strong,
                "Strike" => InlineFormat::Strike,
                other => return Err(invalid(format!("unknown inline format {other}"))),
            };
            Ok(EditCommand::toggle_inline_format(range(row[7])?, format))
        }
        other => Err(invalid(format!("unknown command {other}"))),
    }
}

fn actual_protection(parsed: &Parsed, category: &str) -> Result<Vec<Range<usize>>, Box<dyn Error>> {
    let kind = match category {
        "delete" | "paste" => ProtectionKind::FootnoteReference,
        "html" => ProtectionKind::Html,
        other => return Err(invalid(format!("unknown category {other}"))),
    };
    let mut ranges = parsed
        .protected_ranges()
        .iter()
        .filter(|entry| entry.kind() == kind)
        .map(|entry| entry.range().byte_range())
        .collect::<Vec<_>>();
    if category == "html" {
        ranges.extend(
            parsed
                .styled_ranges()
                .iter()
                .map(|entry| entry.range().byte_range()),
        );
    }
    ranges.sort_by_key(|value| (value.start, value.end));
    Ok(ranges)
}

fn changed(outcome: PlanOutcome, case: &str) -> Result<Box<ChangePlan>, Box<dyn Error>> {
    match outcome {
        PlanOutcome::Changed(plan) => Ok(plan),
        PlanOutcome::Unchanged(reason) => Err(invalid(format!(
            "{case}: expected Changed, got Unchanged({})",
            reason.as_str()
        ))),
    }
}

fn expect_refusal(
    result: Result<PlanOutcome, EditRefusal>,
    row: &[&str],
    case: &str,
) -> TestResult {
    let refusal = match result {
        Err(refusal) => refusal,
        Ok(_) => return Err(invalid(format!("{case}: expected refusal, got a plan"))),
    };
    match (row[12], refusal) {
        (
            "ProtectedRange",
            EditRefusal::ProtectedRange {
                kind,
                range: actual,
            },
        ) => {
            assert_eq!(kind.as_str(), row[13], "{case}");
            assert_eq!(actual, range(row[14])?, "{case}");
        }
        ("HtmlBoundary", EditRefusal::HtmlBoundary { range: actual }) => {
            assert_eq!(actual, range(row[14])?, "{case}");
        }
        (expected, other) => {
            return Err(invalid(format!(
                "{case}: expected {}, got {}",
                expected,
                other.as_str()
            )));
        }
    }
    Ok(())
}

#[test]
fn crossing_golden_cases() -> TestResult {
    let mut count = 0usize;
    for (line_number, line) in include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/validation/m0-engine-foundation/remediation/R08/run-1/goldens/edit-goldens.tsv"
    ))
    .lines()
    .enumerate()
    {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let row = line.split('\t').collect::<Vec<_>>();
        if row.len() != 21 {
            return Err(invalid(format!(
                "TSV line {} has {} columns, expected 21",
                line_number + 1,
                row.len()
            )));
        }
        let case = row[0];
        let source = string(&row, 3)?;
        let anchor = number(&row, 4)?;
        let head = number(&row, 5)?;
        let mut buffer = make_buffer(&source, anchor, head)?;
        let needs_redo = row[11] != "Changed";
        if needs_redo {
            seed_redo(&mut buffer, SelectionSpec::new(anchor, head))?;
        }
        let parsed = parse_buffer(&buffer)?;
        if row[14] != "-" && (row[1] == "delete" || row[1] == "paste") {
            let ranges = actual_protection(&parsed, row[1])?;
            let expected = range(row[14])?;
            if !ranges.contains(&expected) {
                return Err(invalid(format!(
                    "{case}: parsed protection ranges {ranges:?} do not include {expected:?}"
                )));
            }
        }
        if row[11] == "Refused" && row[1] == "html" {
            let ranges = actual_protection(&parsed, "html")?;
            let expected = range(row[14])?;
            if !ranges.contains(&expected) {
                return Err(invalid(format!(
                    "{case}: parsed protected/styled ranges {ranges:?} do not include {expected:?}"
                )));
            }
        }
        let selected = selection(buffer.snapshot(), &row)?;
        let before = capture(&buffer)?;
        let result = plan_change(
            buffer.snapshot(),
            &parsed,
            &selected,
            &EditContext::new(match row[2] {
                "Visual" => EditMode::Visual,
                "Source" => EditMode::Source,
                other => return Err(invalid(format!("{case}: unknown mode {other}"))),
            }),
            &command(&row)?,
            budget(),
        );
        match row[11] {
            "Refused" => {
                expect_refusal(result, &row, case)?;
                assert_eq!(capture(&buffer)?, before, "{case}: refusal changed state");
                buffer.redo(TextBudget::new(TEXT_LIMIT))?;
                assert_eq!(text_of(&buffer)?, format!("{source}Q"), "{case}: redo");
                assert_eq!(buffer.redo_depth(), 0, "{case}: redo was not consumed");
                assert_eq!(
                    buffer.selection().anchor().byte_offset(),
                    anchor,
                    "{case}: redo anchor"
                );
                assert_eq!(
                    buffer.selection().head().byte_offset(),
                    head,
                    "{case}: redo head"
                );
            }
            "Unchanged" => {
                match result? {
                    PlanOutcome::Unchanged(reason) => {
                        assert_eq!(reason.as_str(), row[19], "{case}")
                    }
                    PlanOutcome::Changed(_) => {
                        return Err(invalid(format!("{case}: expected Unchanged")));
                    }
                }
                assert_eq!(
                    capture(&buffer)?,
                    before,
                    "{case}: unchanged modified state"
                );
                buffer.redo(TextBudget::new(TEXT_LIMIT))?;
                assert_eq!(text_of(&buffer)?, format!("{source}Q"), "{case}: redo");
                assert_eq!(buffer.redo_depth(), 0, "{case}: redo was not consumed");
                assert_eq!(
                    buffer.selection().anchor().byte_offset(),
                    anchor,
                    "{case}: redo anchor"
                );
                assert_eq!(
                    buffer.selection().head().byte_offset(),
                    head,
                    "{case}: redo head"
                );
            }
            "Changed" => {
                let plan = changed(
                    result.map_err(|error| {
                        invalid(format!("{case}: planning refused with {}", error.as_str()))
                    })?,
                    case,
                )?;
                let expected_patch = (range(row[15])?, string(&row, 16)?);
                assert_eq!(plan.batch().patches(), &[expected_patch], "{case}: patches");
                let target = SelectionSpec::new(number(&row, 17)?, number(&row, 18)?);
                assert_eq!(plan.target_selection(), target, "{case}: target selection");
                assert_eq!(plan.group_reason().as_str(), row[19], "{case}: reason");
                let outcome = buffer.apply(
                    plan.batch(),
                    target,
                    OperationSource::Replace,
                    1,
                    TextBudget::new(TEXT_LIMIT),
                )?;
                if !matches!(outcome, ApplyOutcome::Changed(_)) {
                    return Err(invalid(format!("{case}: expected Changed buffer apply")));
                }
                assert_eq!(text_of(&buffer)?, string(&row, 20)?, "{case}: after text");
                assert_eq!(
                    buffer.selection().anchor().byte_offset(),
                    target.anchor(),
                    "{case}: anchor"
                );
                assert_eq!(
                    buffer.selection().head().byte_offset(),
                    target.head(),
                    "{case}: head"
                );
                assert_eq!(buffer.undo_depth(), 1, "{case}: one history step");
            }
            other => return Err(invalid(format!("{case}: unknown outcome {other}"))),
        }
        count += 1;
    }
    assert_eq!(count, 14);
    Ok(())
}
