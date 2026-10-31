//! 组 7：所见模式对 front matter / 目录保护的拒绝，以及 Source 模式放行。

use std::error::Error;
use std::io;
use std::ops::Range;

use aline_buffer::{PatchBudget, SrcSelection, TextBudget, TextSnapshot};
use aline_edit::{
    ChangePlan, EditBudget, EditCommand, EditContext, EditMode, EditRefusal, PlanOutcome,
    SourceEdit, plan_change,
};
use aline_markdown::{ParseBudget, ParseConfig, Parsed, ProtectionKind, parse};

const TEXT_LIMIT: usize = 1 << 20;
const PATCH_LIMIT: usize = 64;

type TestResult = Result<(), Box<dyn Error>>;

fn invalid(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

struct Env {
    snapshot: TextSnapshot,
    parsed: Parsed,
}

impl Env {
    fn new(source: &str) -> Result<Self, Box<dyn Error>> {
        let snapshot = TextSnapshot::new(source.to_owned(), TextBudget::new(TEXT_LIMIT))?;
        let parsed = parse(&snapshot, ParseConfig::m0(), ParseBudget::m0())?;
        Ok(Self { snapshot, parsed })
    }

    fn selection(&self, anchor: usize, head: usize) -> Result<SrcSelection, Box<dyn Error>> {
        Ok(SrcSelection::new(
            self.snapshot.src_offset(anchor)?,
            self.snapshot.src_offset(head)?,
        )?)
    }

    fn protected(&self, kind: ProtectionKind) -> Result<Range<usize>, Box<dyn Error>> {
        self.parsed
            .protected_ranges()
            .iter()
            .find(|entry| entry.kind() == kind)
            .map(|entry| entry.range().byte_range())
            .ok_or_else(|| invalid(format!("missing protection {kind:?}")))
    }
}

fn budget() -> EditBudget {
    EditBudget::new(
        PatchBudget::new(PATCH_LIMIT, TEXT_LIMIT, TEXT_LIMIT),
        TextBudget::new(TEXT_LIMIT),
        ParseBudget::m0(),
    )
}

fn replace(range: Range<usize>, expected: &str, replacement: &str) -> EditCommand {
    EditCommand::replace(vec![SourceEdit::new(
        range,
        expected.to_owned(),
        replacement.to_owned(),
    )])
}

fn plan(
    env: &Env,
    selection: &SrcSelection,
    mode: EditMode,
    command: &EditCommand,
) -> Result<PlanOutcome, EditRefusal> {
    plan_change(
        &env.snapshot,
        &env.parsed,
        selection,
        &EditContext::new(mode),
        command,
        budget(),
    )
}

fn assert_protected(
    result: Result<PlanOutcome, EditRefusal>,
    kind: ProtectionKind,
    range: Range<usize>,
) -> TestResult {
    match result {
        Err(EditRefusal::ProtectedRange {
            kind: actual,
            range: actual_range,
        }) => {
            assert_eq!(actual, kind);
            assert_eq!(actual_range, range);
            Ok(())
        }
        Ok(_) => Err(invalid("expected ProtectedRange, got a plan")),
        Err(other) => Err(invalid(format!(
            "expected ProtectedRange, got {}",
            other.as_str()
        ))),
    }
}

fn changed(outcome: PlanOutcome) -> Result<Box<ChangePlan>, Box<dyn Error>> {
    match outcome {
        PlanOutcome::Changed(plan) => Ok(plan),
        PlanOutcome::Unchanged(reason) => Err(invalid(format!(
            "expected Changed, got Unchanged({})",
            reason.as_str()
        ))),
    }
}

#[test]
fn front_matter_visual_refused_source_allowed() -> TestResult {
    let env = Env::new("---\nt: x\n---\n\n[toc]\n")?;
    let front = env.protected(ProtectionKind::FrontMatter)?;
    assert_eq!(front, 0..12);
    let selection = env.selection(4, 8)?;
    let command = replace(4..8, "t: x", "title: x");
    assert_protected(
        plan(&env, &selection, EditMode::Visual, &command),
        ProtectionKind::FrontMatter,
        front,
    )?;

    let changed = changed(plan(&env, &selection, EditMode::Source, &command)?)?;
    assert_eq!(changed.group_reason().as_str(), "SourceReplace");
    assert_eq!(changed.batch().patches(), &[(4..8, "title: x".to_owned())]);
    assert_eq!(changed.target_selection().anchor(), 12);
    assert_eq!(changed.target_selection().head(), 12);
    Ok(())
}

#[test]
fn toc_visual_refused_source_allowed() -> TestResult {
    let env = Env::new("---\nt: x\n---\n\n[toc]\n")?;
    let toc = env.protected(ProtectionKind::Toc)?;
    assert_eq!(toc, 14..19);
    let selection = env.selection(14, 19)?;
    let command = replace(14..19, "[toc]", "[TOC]");
    assert_protected(
        plan(&env, &selection, EditMode::Visual, &command),
        ProtectionKind::Toc,
        toc,
    )?;

    let changed = changed(plan(&env, &selection, EditMode::Source, &command)?)?;
    assert_eq!(changed.group_reason().as_str(), "SourceReplace");
    assert_eq!(changed.target_selection().anchor(), 19);
    assert_eq!(changed.preview().drawn_text(), "[TOC]");
    Ok(())
}

#[test]
fn visual_body_beside_front_matter_allowed() -> TestResult {
    let env = Env::new("---\nt: x\n---\n\nbody\n")?;
    assert_eq!(env.protected(ProtectionKind::FrontMatter)?, 0..12);
    let selection = env.selection(14, 18)?;
    let command = replace(14..18, "body", "text");
    let changed = changed(plan(&env, &selection, EditMode::Visual, &command)?)?;
    assert_eq!(changed.group_reason().as_str(), "VisibleReplace");
    assert_eq!(changed.target_selection().anchor(), 18);
    assert_eq!(changed.preview().drawn_text(), "text");
    Ok(())
}

#[test]
fn visual_cross_region_replacement_refused_source_allowed() -> TestResult {
    let env = Env::new("before [^1] after\n\n[^1]: note\n")?;
    let note = env.protected(ProtectionKind::FootnoteReference)?;
    assert_eq!(note, 7..11);
    let selection = env.selection(0, 11)?;
    // 跨普通正文与脚注引用的删除+替换（粘贴同理）；只读区不能被跨越。
    let command = replace(0..11, "before [^1]", "BEFORE");
    assert_protected(
        plan(&env, &selection, EditMode::Visual, &command),
        ProtectionKind::FootnoteReference,
        note,
    )?;

    let changed = changed(plan(&env, &selection, EditMode::Source, &command)?)?;
    assert_eq!(changed.group_reason().as_str(), "SourceReplace");
    assert_eq!(changed.target_selection().anchor(), 6);
    assert_eq!(changed.preview().drawn_text(), "BEFORE afternote");
    Ok(())
}
