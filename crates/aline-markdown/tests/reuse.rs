//! 组 6.7：头尾配对复用过滤、重定位、差分与安全回退。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{Affinity, BufferError, SrcOffset, TextBudget, TextSnapshot};
use aline_markdown::{
    BlockKind, MappingPrecision, ParseBudget, ParseConfig, Parsed, ProtectionKind, ReuseOutcome,
    ReuseRefusal, ReuseTranslateError, ReusedBlock, TextStyle, parse, reuse,
};

const BUDGET: usize = 1024 * 1024;

fn parse_with(
    text: &str,
    config: ParseConfig,
    budget: ParseBudget,
) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    Ok(parse(&snapshot, config, budget)?)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    parse_with(text, ParseConfig::m0(), ParseBudget::m0())
}

#[test]
fn f04_translate_must_reject_half_char_old_offset() -> Result<(), Box<dyn Error>> {
    let old = TextSnapshot::new("😀\n".to_string(), TextBudget::new(BUDGET))?;
    let new = TextSnapshot::new("x\n\n😀\n".to_string(), TextBudget::new(BUDGET))?;
    let a = parse(&old, ParseConfig::m0(), ParseBudget::m0())?;
    let b = parse(&new, ParseConfig::m0(), ParseBudget::m0())?;
    let outcome = reuse(&a, &b);
    let pair = outcome.reused().first().ok_or("missing reused pair")?;
    assert!(matches!(
        old.src_offset(1),
        Err(BufferError::NotCharBoundary { offset: 1 })
    ));
    // 旧端点在旧快照里就是半字符，旧快照本身已拒绝构造它。
    assert!(old.src_offset(1).is_err(), "端点 1 在旧快照不可构造");
    // 块起点必须是合法正例，说明配对本身可用。
    let start = old.src_offset(pair.old_range().byte_range().start)?;
    let translated = pair.translate(&new, &start)?;
    assert_eq!(
        translated.byte_offset(),
        pair.new_range().byte_range().start
    );
    // 旧 API 会把端点 1 静默算成新端点 4；受检入口没有这个通道：
    // 端点 1 根本无法作为 SrcOffset 存在，因此不存在可传入的非法端点。
    assert!(
        old.src_offset(1).is_err(),
        "受检入口不得存在接受半字符端点的路径"
    );
    assert!(matches!(
        new.src_offset(4),
        Err(BufferError::NotCharBoundary { offset: 4 })
    ));
    Ok(())
}

/// 用旧快照的块起点构造端点；`src_offset` 越界或半字符会返回错误。
fn old_start(snapshot: &TextSnapshot, pair: &ReusedBlock) -> Result<SrcOffset, Box<dyn Error>> {
    Ok(snapshot.src_offset(pair.old_range().byte_range().start)?)
}

#[test]
fn translate_rejects_wrong_old_snapshot() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n\nC\n")?;
    let new = parse_text("X\n\nA\n\nB\n\nC\n")?;
    let other = parse_text("Z\n\nY\n")?;
    let outcome = reuse(&old, &new);
    let pair = outcome.reused().first().ok_or("missing pair")?;
    // 合法正例先确认，确保拒绝来自快照身份而非端点数值。
    let ok = pair.translate(new.snapshot(), &old_start(old.snapshot(), pair)?)?;
    assert_eq!(ok.byte_offset(), pair.new_range().byte_range().start);
    // 用另一快照的同值端点喂进去，必须报错旧快照。
    let foreign = other
        .snapshot()
        .src_offset(pair.old_range().byte_range().start)?;
    assert!(matches!(
        pair.translate(new.snapshot(), &foreign),
        Err(ReuseTranslateError::WrongOldSnapshot)
    ));
    Ok(())
}

#[test]
fn translate_rejects_wrong_new_snapshot() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n\nC\n")?;
    let new = parse_text("X\n\nA\n\nB\n\nC\n")?;
    let other = parse_text("Z\n\nY\n")?;
    let outcome = reuse(&old, &new);
    let pair = outcome.reused().first().ok_or("missing pair")?;
    let start = old_start(old.snapshot(), pair)?;
    assert!(matches!(
        pair.translate(other.snapshot(), &start),
        Err(ReuseTranslateError::WrongNewSnapshot)
    ));
    Ok(())
}

#[test]
fn translate_rejects_outside_old_block() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n\nC\n")?;
    let new = parse_text("X\n\nA\n\nB\n\nC\n")?;
    let outcome = reuse(&old, &new);
    let pair = outcome.reused().first().ok_or("missing pair")?;
    let bytes = pair.old_range().byte_range();
    // 块起点 0，故没有 start - 1 可测；右端点 +1 与 usize::MAX 都必须被拒。
    if let Some(before) = bytes.start.checked_sub(1) {
        let outside = old.snapshot().src_offset(before)?;
        assert!(matches!(
            pair.translate(new.snapshot(), &outside),
            Err(ReuseTranslateError::OutsideOldBlock)
        ));
    }
    let after = old.snapshot().src_offset(bytes.end.saturating_add(1))?;
    assert!(matches!(
        pair.translate(new.snapshot(), &after),
        Err(ReuseTranslateError::OutsideOldBlock)
    ));
    // usize::MAX 先被旧快照的越界检查拒绝，到不了块外判定；两者都必须失败。
    assert!(old.snapshot().src_offset(usize::MAX).is_err());
    Ok(())
}

#[test]
fn translate_accepts_in_block_endpoints_and_is_byte_exact() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n\nC\n")?;
    let new = parse_text("X\n\nA\n\nB\n\nC\n")?;
    let outcome = reuse(&old, &new);
    let pair = outcome.reused().first().ok_or("missing pair")?;
    let old_bytes = pair.old_range().byte_range();
    let new_bytes = pair.new_range().byte_range();
    assert_eq!(old_bytes.len(), new_bytes.len());
    // 闭区间右端点必须保留：既有 assert_all_reused_pairs_match 依赖它。
    for offset in old_bytes.start..=old_bytes.end {
        let old_offset = old.snapshot().src_offset(offset)?;
        let translated = pair.translate(new.snapshot(), &old_offset)?;
        let expected = new_bytes
            .start
            .checked_add(offset.saturating_sub(old_bytes.start))
            .ok_or("offset overflow")?;
        assert_eq!(
            translated.byte_offset(),
            expected,
            "块内偏移 {offset} 必须逐字节对应"
        );
    }
    Ok(())
}

#[test]
fn insertion_before_rebases_matching_suffix_blocks() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n\nC\n")?;
    let new = parse_text("X\n\nA\n\nB\n\nC\n")?;
    let outcome = reuse(&old, &new);
    assert_eq!(outcome.refusal(), None);
    assert_eq!(outcome.reused().len(), 3, "suffix blocks must be reused");
    let expected = [
        (0usize, 1usize, 0..1, 3..4),
        (1, 2, 3..4, 6..7),
        (2, 3, 6..7, 9..10),
    ];
    for (pair, (old_index, new_index, old_range, new_range)) in
        outcome.reused().iter().zip(expected)
    {
        assert_eq!(pair.old_index(), old_index);
        assert_eq!(pair.new_index(), new_index);
        assert_eq!(pair.old_range().byte_range(), old_range);
        assert_eq!(pair.new_range().byte_range(), new_range);
        assert_eq!(
            pair.translate(new.snapshot(), &old.snapshot().src_offset(old_range.start)?)?
                .byte_offset(),
            new_range.start
        );
        assert_eq!(
            pair.translate(new.snapshot(), &old.snapshot().src_offset(old_range.end)?)?
                .byte_offset(),
            new_range.end
        );
    }
    let reused_old: Vec<_> = outcome
        .reused()
        .iter()
        .map(|pair| pair.old_index())
        .collect();
    assert_eq!(reused_old, vec![0, 1, 2]);
    Ok(())
}

#[test]
fn edit_inside_one_paragraph_keeps_only_outer_blocks() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n\nC\n")?;
    let new = parse_text("A\n\nB2\n\nC\n")?;
    let outcome = reuse(&old, &new);
    assert_eq!(outcome.refusal(), None);
    assert_eq!(outcome.reused().len(), 2);
    assert_eq!(
        outcome
            .reused()
            .iter()
            .map(|pair| pair.old_index())
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    let suffix = outcome.reused().get(1).ok_or("missing suffix")?;
    assert_eq!(suffix.old_range().byte_range(), 6..7);
    assert_eq!(suffix.new_range().byte_range(), 7..8);
    Ok(())
}

#[test]
fn unrelated_documents_reuse_nothing() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n")?;
    let new = parse_text("C\n\nD\n")?;
    let outcome = reuse(&old, &new);
    assert_eq!(outcome.refusal(), None);
    assert!(outcome.is_empty());
    Ok(())
}

#[test]
fn global_dependencies_refuse_reuse() -> Result<(), Box<dyn Error>> {
    let plain = parse_text("A\n\nB\n")?;
    let cases: [(&str, ReuseRefusal); 6] = [
        ("---\nt: x\n---\n\nA\n\nB\n", ReuseRefusal::FrontMatter),
        ("a[^1]\n\n[^1]: x\n", ReuseRefusal::Footnote),
        ("[toc]\n\nA\n", ReuseRefusal::Toc),
        ("```\nx\n```\n", ReuseRefusal::Fence),
        ("- a\n- b\n", ReuseRefusal::List),
        ("see [x]\n\n[x]: /u\n", ReuseRefusal::ReferenceDefinition),
    ];
    for (source, expected) in cases {
        let changed = parse_text(source)?;
        assert_eq!(
            reuse(&plain, &changed).refusal(),
            Some(expected),
            "source {source}"
        );
        assert_eq!(
            reuse(&changed, &plain).refusal(),
            Some(expected),
            "reversed source {source}"
        );
    }
    let html = parse_text("<div>x</div>\n")?;
    assert_eq!(reuse(&plain, &html).refusal(), Some(ReuseRefusal::Html));
    Ok(())
}

#[test]
fn config_change_and_raw_fallback_refuse_reuse() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n")?;
    let changed = parse_with(
        "A\n\nB\n",
        ParseConfig::new(2, aline_markdown::ParseFeatures::M0),
        ParseBudget::m0(),
    )?;
    assert_eq!(
        reuse(&old, &changed).refusal(),
        Some(ReuseRefusal::ConfigChanged)
    );
    let fallback = parse_with(
        "A\n\nB\n",
        ParseConfig::m0(),
        ParseBudget::m0().with_max_segments(0),
    )?;
    assert_eq!(
        reuse(&old, &fallback).refusal(),
        Some(ReuseRefusal::RawFallback)
    );
    assert_eq!(
        reuse(&fallback, &old).refusal(),
        Some(ReuseRefusal::RawFallback)
    );
    Ok(())
}

#[test]
fn old_absolute_ranges_fail_against_new_snapshot() -> Result<(), Box<dyn Error>> {
    let old = parse_text("A\n\nB\n\nC\n")?;
    let new = parse_text("X\n\nA\n\nB\n\nC\n")?;
    let old_block = old.blocks().first().ok_or("missing old block")?;
    assert!(matches!(
        new.validate_range(old_block.range()),
        Err(aline_markdown::ParseError::Source(
            BufferError::SnapshotMismatch
        ))
    ));
    let outcome = reuse(&old, &new);
    let pair = outcome.reused().first().ok_or("missing pair")?;
    assert_eq!(
        pair.translate(new.snapshot(), &old.snapshot().src_offset(0)?)?
            .byte_offset(),
        3
    );
    // 块范围 0..1，故 2 已在块外；usize::MAX 连旧快照都构造不出。
    assert!(matches!(
        pair.translate(new.snapshot(), &old.snapshot().src_offset(2)?),
        Err(ReuseTranslateError::OutsideOldBlock)
    ));
    assert!(old.snapshot().src_offset(usize::MAX).is_err());
    Ok(())
}

#[test]
fn block_quote_reference_definition_changes_refuse_reuse() -> Result<(), Box<dyn Error>> {
    let definitions = [
        "> [x]: /u\n\n[x]\n",
        ">   [x]: /u\n\n[x]\n",
        ">    [x]: /u\n\n[x]\n",
        ">> [x]: /u\n\n[x]\n",
        "  > [x]: /u\n\n[x]\n",
        "> - [x]: /u\n\n[x]\n",
        "> -    [x]: /u\n\n[x]\n",
        "> 1. [x]: /u\n\n[x]\n",
        ">\t[x]: /u\n\n[x]\n",
        "> - \t[x]: /u\n\n[x]\n",
    ];
    let deleted = parse_text("[x]\n")?;
    let replaced = parse_text("> [z]: /v\n\n[x]\n")?;
    for source in definitions {
        let old = parse_text(source)?;
        let changed = parse_text(&source.replace("/u", "/v"))?;
        let cases = [
            (&old, &changed, "target changed"),
            (&changed, &old, "target changed reversed"),
            (&old, &deleted, "definition deleted"),
            (&deleted, &old, "definition deleted reversed"),
            (&old, &replaced, "definition block replaced"),
            (&replaced, &old, "definition block replaced reversed"),
        ];
        for (left, right, label) in cases {
            let outcome = reuse(left, right);
            assert_eq!(
                outcome.refusal(),
                Some(ReuseRefusal::ReferenceDefinition),
                "{label} for {source:?}"
            );
            assert!(
                outcome.is_empty(),
                "{label} for {source:?} must not silently reuse blocks"
            );
        }
    }
    Ok(())
}

#[test]
fn multiline_reference_definitions_refuse_reuse() -> Result<(), Box<dyn Error>> {
    let plain = parse_text("A\n\nB\n")?;
    let definitions = [
        "[foo\nbar]: /u\n\n[foo bar]\n",
        "> [foo\n> bar]: /u\n\n[foo bar]\n",
        ">> [foo\n>> bar]: /u\n\n[foo bar]\n",
        ">\t[foo\n>\tbar]: /u\n\n[foo bar]\n",
    ];
    for source in definitions {
        let parsed = parse_text(source)?;
        assert_eq!(
            reuse(&plain, &parsed).refusal(),
            Some(ReuseRefusal::ReferenceDefinition),
            "multiline definition present in {source:?}"
        );
        assert_eq!(
            reuse(&parsed, &plain).refusal(),
            Some(ReuseRefusal::ReferenceDefinition),
            "multiline definition present reversed in {source:?}"
        );
    }
    Ok(())
}

#[test]
fn deleting_multiline_definition_refuses_reuse() -> Result<(), Box<dyn Error>> {
    let old = parse_text("[foo\nbar]: /u\n\n[foo bar]\n")?;
    let new = parse_text("[foo bar]\n")?;
    for (left, right, label) in [(&old, &new, "forward"), (&new, &old, "reversed")] {
        let outcome = reuse(left, right);
        assert_eq!(
            outcome.refusal(),
            Some(ReuseRefusal::ReferenceDefinition),
            "definition deleted {label}"
        );
        assert!(
            outcome.is_empty(),
            "definition deleted {label} must not silently reuse blocks"
        );
    }
    Ok(())
}

#[test]
fn changing_multiline_definition_target_refuses_reuse() -> Result<(), Box<dyn Error>> {
    let cases = [
        (
            "[foo\nbar]: http://x\n\n[foo bar]\n",
            "[foo\nbar]: javascript:x\n\n[foo bar]\n",
            "plain",
        ),
        (
            "> [foo\n> bar]: http://x\n\n[foo bar]\n",
            "> [foo\n> bar]: javascript:x\n\n[foo bar]\n",
            "block quote",
        ),
    ];
    for (old_text, new_text, label) in cases {
        let old = parse_text(old_text)?;
        let new = parse_text(new_text)?;
        assert_eq!(
            reuse(&old, &new).refusal(),
            Some(ReuseRefusal::ReferenceDefinition),
            "multiline target changed ({label})"
        );
        assert_eq!(
            reuse(&new, &old).refusal(),
            Some(ReuseRefusal::ReferenceDefinition),
            "multiline target changed reversed ({label})"
        );
    }
    Ok(())
}

#[test]
fn ordinary_multiline_paragraph_text_still_reuses() -> Result<(), Box<dyn Error>> {
    let old = parse_text("alpha\nbeta\n\nC\n")?;
    let new = parse_text("X\n\nalpha\nbeta\n\nC\n")?;
    let outcome = reuse(&old, &new);
    assert_eq!(outcome.refusal(), None, "ordinary multiline paragraph");
    assert_eq!(
        outcome.reused().len(),
        2,
        "ordinary multiline paragraph must reuse"
    );
    assert_all_reused_pairs_match(&old, &new, &outcome)?;
    Ok(())
}

#[test]
fn multiline_label_without_definition_still_reuses() -> Result<(), Box<dyn Error>> {
    let old = parse_text("[foo\nbar] text\n\nC\n")?;
    let new = parse_text("X\n\n[foo\nbar] text\n\nC\n")?;
    let outcome = reuse(&old, &new);
    assert_eq!(
        outcome.refusal(),
        None,
        "multiline label without colon must not refuse"
    );
    assert_eq!(
        outcome.reused().len(),
        2,
        "multiline label without colon must reuse"
    );
    assert_all_reused_pairs_match(&old, &new, &outcome)?;
    Ok(())
}

#[test]
fn block_quote_fence_changes_refuse_reuse() -> Result<(), Box<dyn Error>> {
    let cases = [
        (
            "> ```\n> x\n> ```\n\nA\n",
            "> ```\n> y\n> ```\n\nA\n",
            "fence content changed",
        ),
        (
            "> ```\n> x\n> ```\n\nA\n",
            "A\n\n> ```\n> x\n> ```\n",
            "fence position changed",
        ),
        (
            "> ~~~\n> x\n> ~~~\n\nA\n",
            "> ~~~\n> y\n> ~~~\n\nA\n",
            "tilde fence content changed",
        ),
        (
            "> - ```\n>   x\n>   ```\n\nA\n",
            "> - ```\n>   y\n>   ```\n\nA\n",
            "fence in list content changed",
        ),
        (
            "> - ```\n>   x\n>   ```\n\nA\n",
            "> - ```\n>   x\n>   ```\n\nB\n",
            "trailing text changed",
        ),
        (
            ">\t```\n>\tx\n>\t```\n\nA\n",
            ">\t```\n>\ty\n>\t```\n\nA\n",
            "tab fence content changed",
        ),
    ];
    for (old_text, new_text, label) in cases {
        let old = parse_text(old_text)?;
        let new = parse_text(new_text)?;
        assert_eq!(
            reuse(&old, &new).refusal(),
            Some(ReuseRefusal::Fence),
            "{label}"
        );
        assert_eq!(
            reuse(&new, &old).refusal(),
            Some(ReuseRefusal::Fence),
            "{label} reversed"
        );
    }
    Ok(())
}

#[test]
fn list_global_dependencies_refuse_reuse() -> Result<(), Box<dyn Error>> {
    for source in [
        "- [x]: /u\n\n[x]\n",
        "  - [x]: /u\n\n[x]\n",
        "1. [x]: /u\n\n[x]\n",
    ] {
        let old = parse_text(source)?;
        let changed = parse_text(&source.replace("/u", "/v"))?;
        assert_eq!(
            reuse(&old, &changed).refusal(),
            Some(ReuseRefusal::List),
            "list definition changed for {source:?}"
        );
        assert_eq!(
            reuse(&changed, &old).refusal(),
            Some(ReuseRefusal::List),
            "list definition changed reversed for {source:?}"
        );
    }
    let old = parse_text("- ```\n  x\n  ```\n")?;
    let new = parse_text("- ```\n  y\n  ```\n")?;
    assert_eq!(reuse(&old, &new).refusal(), Some(ReuseRefusal::List));
    let quoted = [
        (
            "> - [x]: /u\n\n[x]\n",
            "> - [x]: /v\n\n[x]\n",
            ReuseRefusal::ReferenceDefinition,
        ),
        (
            "> -    [x]: /u\n\n[x]\n",
            "> -    [x]: /v\n\n[x]\n",
            ReuseRefusal::ReferenceDefinition,
        ),
        (
            "> 1. [x]: /u\n\n[x]\n",
            "> 1. [x]: /v\n\n[x]\n",
            ReuseRefusal::ReferenceDefinition,
        ),
        (
            "> - ```\n>   x\n>   ```\n\nA\n",
            "> - ```\n>   y\n>   ```\n\nA\n",
            ReuseRefusal::Fence,
        ),
    ];
    for (old_text, new_text, expected) in quoted {
        let old = parse_text(old_text)?;
        let new = parse_text(new_text)?;
        assert_eq!(
            reuse(&old, &new).refusal(),
            Some(expected),
            "quoted list dependency for {old_text:?}"
        );
        assert_eq!(
            reuse(&new, &old).refusal(),
            Some(expected),
            "quoted list dependency reversed for {old_text:?}"
        );
    }
    Ok(())
}

#[test]
fn plain_block_quote_text_still_reuses() -> Result<(), Box<dyn Error>> {
    let old = parse_text("> A\n\nB\n\nC\n")?;
    for new_text in ["> A\n\nB\n\nC\n\nD\n", "X\n\n> A\n\nB\n\nC\n"] {
        let new = parse_text(new_text)?;
        let outcome = reuse(&old, &new);
        assert_eq!(outcome.refusal(), None, "plain quote in {new_text:?}");
        assert_eq!(
            outcome.reused().len(),
            3,
            "plain quote blocks must reuse in {new_text:?}"
        );
        assert_all_reused_pairs_match(&old, &new, &outcome)?;
    }
    Ok(())
}

#[test]
fn html_and_footnote_refuse_reuse_in_both_directions() -> Result<(), Box<dyn Error>> {
    let plain = parse_text("A\n\nB\n")?;
    let cases = [
        ("A <b>x</b>\n", ReuseRefusal::Html),
        ("<div>x</div>\n", ReuseRefusal::Html),
        ("A[^1]\n\n[^1]: x\n", ReuseRefusal::Footnote),
        ("[^1]: x\n", ReuseRefusal::Footnote),
    ];
    for (source, expected) in cases {
        let changed = parse_text(source)?;
        assert_eq!(
            reuse(&plain, &changed).refusal(),
            Some(expected),
            "{source:?} forward"
        );
        assert_eq!(
            reuse(&changed, &plain).refusal(),
            Some(expected),
            "{source:?} reversed"
        );
    }
    Ok(())
}

/// 复用块与全量重算的逐块签名：文本、种类、范围、映射片段（来源/绘制/precision）、
/// 保护范围与样式范围都重定位到新快照坐标后比较。
///
/// M0 的 `Parsed` 不暴露链接目标；标题作为导航目标由 `BlockKind::Heading` 的文本与
/// 范围覆盖，而含 HTML、脚注或引用定义的文档在 `global_dependency` 处整体拒绝复用，
/// 因此链接/脚注目标本身在复用路径上不可比，这里不作伪装比较。
#[derive(Debug, Eq, PartialEq)]
struct BlockSignature {
    kind: BlockKind,
    text: String,
    range: Range<usize>,
    segments: Vec<SegmentSignature>,
    protected: Vec<(Range<usize>, ProtectionKind)>,
    styled: Vec<(Range<usize>, TextStyle)>,
}

#[derive(Debug, Eq, PartialEq)]
struct SegmentSignature {
    source: Range<usize>,
    drawn: Range<usize>,
    precision: MappingPrecision,
}

fn assert_all_reused_pairs_match(
    old: &Parsed,
    new: &Parsed,
    outcome: &ReuseOutcome,
) -> Result<(), Box<dyn Error>> {
    for pair in outcome.reused() {
        let new_base = pair.new_range().byte_range().start;
        let old_signature = block_signature(
            old,
            pair.old_index(),
            new_base,
            Some((pair, new.snapshot())),
        )?;
        let new_signature = block_signature(new, pair.new_index(), new_base, None)?;
        assert_eq!(
            old_signature,
            new_signature,
            "reused block {} must match full reparse block {}",
            pair.old_index(),
            pair.new_index()
        );
        let old_start = old
            .snapshot()
            .src_offset(pair.old_range().byte_range().start)?;
        let old_end = old
            .snapshot()
            .src_offset(pair.old_range().byte_range().end)?;
        assert_eq!(
            pair.translate(new.snapshot(), &old_start)?.byte_offset(),
            pair.new_range().byte_range().start
        );
        assert_eq!(
            pair.translate(new.snapshot(), &old_end)?.byte_offset(),
            pair.new_range().byte_range().end
        );
    }
    Ok(())
}

fn block_signature(
    parsed: &Parsed,
    block_index: usize,
    base: usize,
    relocation: Option<(&ReusedBlock, &TextSnapshot)>,
) -> Result<BlockSignature, Box<dyn Error>> {
    let block = parsed.blocks().get(block_index).ok_or("missing block")?;
    let block_range = block.range().byte_range();
    let text = parsed
        .snapshot()
        .slice(block.range(), TextBudget::new(BUDGET))?
        .into_owned();
    let drawn_base = parsed
        .projection()
        .drawn_at(
            &parsed.snapshot().src_offset(block_range.start)?,
            Affinity::Before,
        )?
        .value()
        .byte_offset();
    let relocate = |offset: usize| -> Result<usize, Box<dyn Error>> {
        let relocated = match relocation {
            Some((pair, new_snapshot)) => {
                let old_offset = parsed.snapshot().src_offset(offset)?;
                pair.translate(new_snapshot, &old_offset)?.byte_offset()
            }
            None => offset,
        };
        relocated
            .checked_sub(base)
            .ok_or_else(|| -> Box<dyn Error> { "relocated offset before block base".into() })
    };
    let mut segments = Vec::new();
    for segment in parsed.projection().segments() {
        let source = segment.source_range().byte_range();
        let overlaps = match segment.precision() {
            MappingPrecision::Synthetic => {
                source.start >= block_range.start && source.start <= block_range.end
            }
            _ => source.end > block_range.start && source.start < block_range.end,
        };
        if !overlaps {
            continue;
        }
        let start = source.start.max(block_range.start);
        let end = source.end.min(block_range.end);
        let drawn = segment.drawn_range();
        let clipped_drawn = match segment.precision() {
            MappingPrecision::Exact => {
                let drawn_start = drawn
                    .start
                    .checked_add(start - source.start)
                    .ok_or("drawn offset overflow")?;
                let drawn_end = drawn
                    .start
                    .checked_add(end - source.start)
                    .ok_or("drawn offset overflow")?;
                drawn_start..drawn_end
            }
            MappingPrecision::Hidden => drawn.start..drawn.start,
            MappingPrecision::Aligned | MappingPrecision::Synthetic => {
                if start != source.start || end != source.end {
                    return Err("partially clipped non-exact segment".into());
                }
                drawn
            }
        };
        segments.push(SegmentSignature {
            source: relocate(start)?..relocate(end)?,
            drawn: clipped_drawn
                .start
                .checked_sub(drawn_base)
                .ok_or("drawn start before block")?
                ..clipped_drawn
                    .end
                    .checked_sub(drawn_base)
                    .ok_or("drawn end before block")?,
            precision: segment.precision(),
        });
    }
    let mut protected = Vec::new();
    for range in parsed.protected_ranges() {
        let bytes = range.range().byte_range();
        if bytes.start < block_range.start || bytes.end > block_range.end {
            continue;
        }
        protected.push((relocate(bytes.start)?..relocate(bytes.end)?, range.kind()));
    }
    let mut styled = Vec::new();
    for range in parsed.styled_ranges() {
        let bytes = range.range().byte_range();
        if bytes.start < block_range.start || bytes.end > block_range.end {
            continue;
        }
        styled.push((relocate(bytes.start)?..relocate(bytes.end)?, range.style()));
    }
    Ok(BlockSignature {
        kind: block.kind(),
        text,
        range: relocate(block_range.start)?..relocate(block_range.end)?,
        segments,
        protected,
        styled,
    })
}

#[test]
fn reused_blocks_match_full_reparse_mapping_protection_and_navigation() -> Result<(), Box<dyn Error>>
{
    let cases: [(&str, &str, usize); 4] = [
        ("# H\n\nA\n\nB\n\nC\n", "X\n\n# H\n\nA\n\nB\n\nC\n", 4),
        ("# H\n\nA\n\nB\n\nC\n", "# H\n\nA2\n\nB\n\nC\n", 3),
        ("# H\n\nA\n\nB\n\nC\n", "# H\n\nA\n\nB\n\nC\n\nD\n", 4),
        ("> Q\n\nA\n\nB\n", "X\n\n> Q\n\nA\n\nB\n", 3),
    ];
    for (old_text, new_text, expected) in cases {
        let old = parse_text(old_text)?;
        let new = parse_text(new_text)?;
        let outcome = reuse(&old, &new);
        assert_eq!(outcome.refusal(), None, "old {old_text:?} new {new_text:?}");
        assert_eq!(
            outcome.reused().len(),
            expected,
            "old {old_text:?} new {new_text:?}"
        );
        assert_all_reused_pairs_match(&old, &new, &outcome)?;
    }
    Ok(())
}

#[test]
fn reused_text_matches_full_reparse_block_by_block() -> Result<(), Box<dyn Error>> {
    let old = parse_text("# H\n\nA\n\nB\n\nC\n")?;
    let new = parse_text("# H\n\nA\n\nB\n\nC\n\nD\n")?;
    let outcome = reuse(&old, &new);
    assert_eq!(outcome.refusal(), None);
    assert_eq!(outcome.reused().len(), 4);
    for pair in outcome.reused() {
        let old_block = old
            .blocks()
            .get(pair.old_index())
            .ok_or("missing old block")?;
        let new_block = new
            .blocks()
            .get(pair.new_index())
            .ok_or("missing new block")?;
        assert_eq!(old_block.kind(), new_block.kind());
        let old_range = old_block.range().byte_range();
        let new_range = new_block.range().byte_range();
        let old_text = old
            .snapshot()
            .slice(old_block.range(), TextBudget::new(BUDGET))?;
        let new_text = new
            .snapshot()
            .slice(new_block.range(), TextBudget::new(BUDGET))?;
        assert_eq!(old_text.as_ref(), new_text.as_ref());
        assert_eq!(pair.old_range().byte_range(), old_range);
        assert_eq!(pair.new_range().byte_range(), new_range);
        assert_eq!(
            pair.translate(new.snapshot(), &old.snapshot().src_offset(old_range.start)?)?
                .byte_offset(),
            new_range.start
        );
    }
    assert!(new.blocks().len() > outcome.reused().len());
    Ok(())
}
