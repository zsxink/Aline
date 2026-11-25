//! M3 HTML 块原文编辑与可视化命令保护的独立预期。

use std::error::Error;

use aline_buffer::{PatchBudget, SrcSelection, TextBudget, TextSnapshot};
use aline_edit::{
    EditBudget, EditCommand, EditContext, EditMode, EditRefusal, InlineFormat, PlanOutcome,
    RawBlockEdit, SourceEdit, plan_change, plan_raw_block_input, validate_change,
};
use aline_markdown::{BlockKind, ParseBudget, ParseConfig, parse};

const TEXT: TextBudget = TextBudget::new(4096);
const EDIT: EditBudget = EditBudget::new(PatchBudget::new(8, 4096, 4096), TEXT, ParseBudget::m0());
const VISUAL: EditContext = EditContext::new(EditMode::Visual);

type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn html_block_raw_input_preserves_comments_entities_and_neighboring_source() -> TestResult {
    let source = "<div>甲 &amp; <!--保留-->乙</div>\r\n\r\n尾";
    let snapshot = TextSnapshot::new(source.into(), TEXT)?;
    let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
    let block = parsed.blocks().first().ok_or("missing HTML block")?;
    assert_eq!(block.kind(), BlockKind::HtmlBlock);
    let start = source.find("乙").ok_or("missing text")?;
    let range = snapshot.src_range(start..start + 3)?;
    let selection =
        SrcSelection::new(snapshot.src_offset(start)?, snapshot.src_offset(start + 3)?)?;
    let outcome = plan_raw_block_input(
        &parsed,
        &selection,
        &VISUAL,
        block.range(),
        RawBlockEdit::new(&range, "乙", "丙"),
        EDIT,
    )?;
    let PlanOutcome::Changed(plan) = outcome else {
        return Err("expected HTML raw plan".into());
    };
    let checked = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
    assert_eq!(
        checked.batch().patches(),
        &[(start..start + 3, "丙".to_owned())]
    );
    assert!(checked.preview().drawn_text().contains("甲 & 丙"));
    assert!(!checked.preview().drawn_text().contains("保留"));
    assert!(checked.preview().drawn_text().ends_with("尾"));
    Ok(())
}

#[test]
fn html_block_raw_input_can_edit_hidden_comment_and_filtered_tag_source() -> TestResult {
    for (source, expected, replacement) in [
        ("<div><!--旧-->&amp;</div>", "旧", "新"),
        ("<script>alert(1)</script>", "1", "2"),
    ] {
        let snapshot = TextSnapshot::new(source.into(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let block = parsed.blocks().first().ok_or("missing block")?;
        assert_eq!(block.kind(), BlockKind::HtmlBlock);
        let start = source.find(expected).ok_or("missing preimage")?;
        let range = snapshot.src_range(start..start + expected.len())?;
        let selection =
            SrcSelection::new(snapshot.src_offset(start)?, snapshot.src_offset(start)?)?;
        let PlanOutcome::Changed(plan) = plan_raw_block_input(
            &parsed,
            &selection,
            &VISUAL,
            block.range(),
            RawBlockEdit::new(&range, expected, replacement),
            EDIT,
        )?
        else {
            return Err("expected HTML raw plan".into());
        };
        let checked = validate_change(&plan, &snapshot, &parsed, &selection, &VISUAL)?;
        assert_eq!(
            checked.batch().patches(),
            &[(start..start + expected.len(), replacement.to_owned())]
        );
    }
    Ok(())
}

#[test]
fn html_raw_authorization_does_not_allow_inline_html_or_cross_block_edit() -> TestResult {
    for source in ["甲 <b>乙</b> 丙", "<div>甲</div>\n\n尾"] {
        let snapshot = TextSnapshot::new(source.into(), TEXT)?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        let block = parsed.blocks().first().ok_or("missing block")?;
        let selected = snapshot.src_range(0..source.len())?;
        let selection =
            SrcSelection::new(snapshot.src_offset(0)?, snapshot.src_offset(source.len())?)?;
        assert!(
            plan_raw_block_input(
                &parsed,
                &selection,
                &VISUAL,
                block.range(),
                RawBlockEdit::new(&selected, source, "x"),
                EDIT,
            )
            .is_err()
        );
        for command in [
            EditCommand::replace(vec![SourceEdit::new(0..source.len(), source, "x")]),
            EditCommand::toggle_inline_format(0..source.len(), InlineFormat::Strong),
        ] {
            assert!(matches!(
                plan_change(&snapshot, &parsed, &selection, &VISUAL, &command, EDIT),
                Err(EditRefusal::HtmlBoundary { .. })
            ));
        }
    }
    Ok(())
}
