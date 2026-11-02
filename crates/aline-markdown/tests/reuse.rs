//! 组 6.7：头尾配对复用过滤、重定位、差分与安全回退。

use std::error::Error;
use std::ops::Range;

use aline_buffer::{Affinity, BufferError, SrcOffset, TextBudget, TextSnapshot};
use aline_markdown::{
    BlockKind, MappingPrecision, NavigationEntry, NavigationKind, ParseBudget, ParseConfig, Parsed,
    ProtectionKind, ReuseOutcome, ReuseRefusal, ReuseTranslateError, ReusedBlock, TextStyle, parse,
    reuse,
};

const BUDGET: usize = 1024 * 1024;

fn parse_with(
    text: &str,
    config: ParseConfig,
    budget: ParseBudget,
) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = TextSnapshot::new(text.to_string(), TextBudget::new(BUDGET))?;
    let parsed = parse(&snapshot, config, budget)?;
    assert_tree_partition(&parsed)?;
    Ok(parsed)
}

fn parse_text(text: &str) -> Result<Parsed, Box<dyn Error>> {
    parse_with(text, ParseConfig::m0(), ParseBudget::m0())
}

fn assert_reuse_refused(old: &Parsed, new: &Parsed, expected: ReuseRefusal, label: &str) {
    let outcome = reuse(old, new);
    assert_eq!(outcome.refusal(), Some(expected), "{label}");
    assert!(
        outcome.is_empty(),
        "{label}: refusal must carry no reuse candidates"
    );
}

fn assert_protected_projection_diff(
    old: &Parsed,
    new: &Parsed,
    expected: &[(Range<usize>, ProtectionKind)],
) -> Result<(), Box<dyn Error>> {
    let protected = |parsed: &Parsed| {
        parsed
            .protected_ranges()
            .iter()
            .map(|range| (range.range().byte_range(), range.kind()))
            .collect::<Vec<_>>()
    };
    assert_eq!(protected(old), expected);
    assert_eq!(protected(new), expected);
    let mapping = |parsed: &Parsed| -> Vec<(Range<usize>, Range<usize>, MappingPrecision)> {
        parsed
            .projection()
            .segments()
            .iter()
            .map(|segment| {
                (
                    segment.source_range().byte_range(),
                    segment.drawn_range(),
                    segment.precision(),
                )
            })
            .collect()
    };
    assert_eq!(old.projection().drawn_text(), "a[^1]b[^2]conetwo");
    assert_eq!(new.projection().drawn_text(), "a[^1]b[^2]cunotwo");
    assert_eq!(
        mapping(old),
        mapping(new),
        "fallback full-parse mapping ranges"
    );
    assert_eq!(
        old.projection().drawn_text().get(11..14),
        Some("one"),
        "old protected definition maps its original text"
    );
    assert_eq!(
        new.projection().drawn_text().get(11..14),
        Some("uno"),
        "changed protected definition maps its updated text"
    );
    Ok(())
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
    assert_tree_partition(&old)?;
    assert_tree_partition(&new)?;
    let old_gaps: Vec<_> = old
        .gaps()
        .iter()
        .map(|gap| gap.range().byte_range())
        .collect();
    let new_gaps: Vec<_> = new
        .gaps()
        .iter()
        .map(|gap| gap.range().byte_range())
        .collect();
    assert_eq!(old_gaps, vec![1..3, 4..6, 7..8]);
    assert_eq!(new_gaps, vec![1..3, 4..6, 7..9, 10..11]);
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
        assert_reuse_refused(&plain, &changed, expected, source);
        assert_reuse_refused(&changed, &plain, expected, "reversed source");
    }
    let html = parse_text("<div>x</div>\n")?;
    assert_reuse_refused(&plain, &html, ReuseRefusal::Html, "HTML dependency");

    let footnote_before = parse_text("a[^1]b[^2]c\n\n[^1]: one\n[^2]: two\n")?;
    let footnote_after = parse_text("a[^1]b[^2]c\n\n[^1]: uno\n[^2]: two\n")?;
    assert_reuse_refused(
        &footnote_before,
        &footnote_after,
        ReuseRefusal::Footnote,
        "footnote definition changed",
    );
    assert_protected_projection_diff(
        &footnote_before,
        &footnote_after,
        &[
            (1..5, ProtectionKind::FootnoteReference),
            (6..10, ProtectionKind::FootnoteReference),
            (13..22, ProtectionKind::FootnoteDefinition),
            (23..32, ProtectionKind::FootnoteDefinition),
        ],
    )?;
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
    assert_reuse_refused(
        &old,
        &changed,
        ReuseRefusal::ConfigChanged,
        "config changed",
    );
    let fallback = parse_with(
        "A\n\nB\n",
        ParseConfig::m0(),
        ParseBudget::m0().with_max_segments(0),
    )?;
    assert_reuse_refused(&old, &fallback, ReuseRefusal::RawFallback, "raw fallback");
    assert_reuse_refused(
        &fallback,
        &old,
        ReuseRefusal::RawFallback,
        "raw fallback reversed",
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
fn old_navigation_ranges_fail_against_new_snapshot() -> Result<(), Box<dyn Error>> {
    let old = parse_text("# H\n\nA\n")?;
    let new = parse_text("X\n\n# H\n\nA\n")?;
    let old_entry = old
        .navigation()
        .entries()
        .first()
        .ok_or("missing old navigation entry")?;
    assert_eq!(old_entry.kind(), NavigationKind::Heading);
    assert!(matches!(
        new.validate_range(old_entry.source()),
        Err(aline_markdown::ParseError::Source(
            BufferError::SnapshotMismatch
        ))
    ));
    let new_entry = new
        .navigation()
        .entries()
        .first()
        .ok_or("missing new navigation entry")?;
    let old_start = old
        .snapshot()
        .src_offset(old_entry.source().byte_range().start)?;
    let old_end = old
        .snapshot()
        .src_offset(old_entry.source().byte_range().end)?;
    let outcome = reuse(&old, &new);
    let pair = outcome
        .reused()
        .iter()
        .find(|pair| pair.old_index() == 0)
        .ok_or("missing heading reuse pair")?;
    assert_eq!(
        pair.translate(new.snapshot(), &old_start)?.byte_offset(),
        new_entry.source().byte_range().start
    );
    assert_eq!(
        pair.translate(new.snapshot(), &old_end)?.byte_offset(),
        new_entry.source().byte_range().end
    );
    Ok(())
}

#[test]
fn old_navigation_targets_fail_against_new_snapshot() -> Result<(), Box<dyn Error>> {
    let old = parse_text("# H\n\n[toc]\n\nText[^a].\n\n[^a]: note\n")?;
    let new = parse_text("X\n\n# H\n\n[toc]\n\nText[^a].\n\n[^a]: note\n")?;
    let toc = old
        .navigation()
        .entries()
        .iter()
        .find(|entry| entry.kind() == NavigationKind::Toc)
        .ok_or("missing old toc entry")?;
    assert!(!toc.targets().is_empty());
    for target in toc.targets() {
        assert!(matches!(
            new.validate_range(target),
            Err(aline_markdown::ParseError::Source(
                BufferError::SnapshotMismatch
            ))
        ));
    }
    let reference = old
        .navigation()
        .entries()
        .iter()
        .find(|entry| entry.kind() == NavigationKind::FootnoteReference)
        .ok_or("missing old footnote reference entry")?;
    assert!(!reference.targets().is_empty());
    for target in reference.targets() {
        assert!(matches!(
            new.validate_range(target),
            Err(aline_markdown::ParseError::Source(
                BufferError::SnapshotMismatch
            ))
        ));
    }
    Ok(())
}

#[test]
fn nested_toc_block_reuses_without_toc_navigation() -> Result<(), Box<dyn Error>> {
    let old = parse_text("> [toc]\n\nA\n")?;
    let new = parse_text("X\n\n> [toc]\n\nA\n")?;
    assert!(old.navigation().is_empty());
    assert!(new.navigation().is_empty());
    assert!(
        old.protected_ranges()
            .iter()
            .all(|range| range.kind() != ProtectionKind::Toc)
    );
    assert!(
        new.protected_ranges()
            .iter()
            .all(|range| range.kind() != ProtectionKind::Toc)
    );
    let outcome = reuse(&old, &new);
    assert_eq!(outcome.refusal(), None);
    assert!(!outcome.is_empty(), "nested toc sample must reuse");
    let quote_pair = outcome
        .reused()
        .iter()
        .find(|pair| old.blocks()[pair.old_index()].kind() == BlockKind::BlockQuote)
        .ok_or("nested toc container must itself be reused, not just the tail paragraph")?;
    assert_eq!(
        new.blocks()[quote_pair.new_index()].kind(),
        BlockKind::BlockQuote
    );
    assert_all_reused_pairs_match(&old, &new, &outcome)?;
    assert_heading_source_anchors_match(&old, &new, &outcome)?;
    Ok(())
}

#[test]
fn nested_heading_block_reuses_without_navigation_entries() -> Result<(), Box<dyn Error>> {
    let old = parse_text("> # Nested\n\nA\n")?;
    let new = parse_text("X\n\n> # Nested\n\nA\n")?;
    assert!(old.navigation().is_empty());
    assert!(new.navigation().is_empty());
    let outcome = reuse(&old, &new);
    assert_eq!(outcome.refusal(), None);
    let quote_pair = outcome
        .reused()
        .iter()
        .find(|pair| old.blocks()[pair.old_index()].kind() == BlockKind::BlockQuote)
        .ok_or("nested heading container must reuse")?;
    assert_eq!(
        new.blocks()[quote_pair.new_index()].kind(),
        BlockKind::BlockQuote
    );
    assert_all_reused_pairs_match(&old, &new, &outcome)?;
    assert_heading_source_anchors_match(&old, &new, &outcome)?;
    Ok(())
}

#[test]
fn footnote_reference_alone_refuses_reuse() -> Result<(), Box<dyn Error>> {
    let plain = parse_text("A\n\nB\n")?;
    let referenced = parse_text("> [^n]: nested\n\nText[^n].\n")?;
    assert!(
        referenced
            .protected_ranges()
            .iter()
            .any(|range| range.kind() == ProtectionKind::FootnoteReference)
    );
    assert!(
        referenced
            .protected_ranges()
            .iter()
            .all(|range| range.kind() != ProtectionKind::FootnoteDefinition)
    );
    assert_reuse_refused(
        &plain,
        &referenced,
        ReuseRefusal::Footnote,
        "footnote reference without a top-level definition",
    );
    assert_reuse_refused(
        &referenced,
        &plain,
        ReuseRefusal::Footnote,
        "footnote reference without a top-level definition reversed",
    );
    Ok(())
}

#[test]
fn nested_footnote_definition_refuses_via_reference_definition() -> Result<(), Box<dyn Error>> {
    let plain = parse_text("A\n\nB\n")?;
    let nested = parse_text("> [^n]: nested\n\nC\n")?;
    assert!(nested.navigation().is_empty());
    assert!(
        nested
            .protected_ranges()
            .iter()
            .all(|range| range.kind() != ProtectionKind::FootnoteDefinition)
    );
    assert_reuse_refused(
        &plain,
        &nested,
        ReuseRefusal::ReferenceDefinition,
        "nested footnote definition",
    );
    assert_reuse_refused(
        &nested,
        &plain,
        ReuseRefusal::ReferenceDefinition,
        "nested footnote definition reversed",
    );
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
            assert_reuse_refused(
                left,
                right,
                ReuseRefusal::ReferenceDefinition,
                &format!("{label} for {source:?}"),
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
        assert_reuse_refused(&old, &new, ReuseRefusal::Fence, label);
        assert_reuse_refused(&new, &old, ReuseRefusal::Fence, "fence reversed");
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
/// M0 的 `Parsed::navigation()` 只暴露标题、顶层目录与脚注的只读角色，不暴露链接目标；
/// 含 HTML、脚注或引用定义的文档在 `global_dependency` 处整体拒绝复用。因此复用路径
/// 只比较可安全复用的 Heading 导航条目，不把被拒绝的链接/脚注目标伪装成可复用。
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
    text: String,
    precision: MappingPrecision,
}

/// 取 `[start, end)` 内显式 Gap 的源码文本；Gap 不参与块重定位，
/// 因此比较其条目数与字节内容，而不是要求旧绝对范围可直接用于新快照。
fn gap_texts(parsed: &Parsed, start: usize, end: usize) -> Result<Vec<String>, Box<dyn Error>> {
    parsed
        .gaps()
        .iter()
        .filter_map(|gap| {
            let range = gap.range().byte_range();
            (range.start >= start && range.end <= end).then_some(range)
        })
        .map(|range| {
            let source = parsed.snapshot().src_range(range)?;
            Ok(parsed
                .snapshot()
                .slice(&source, TextBudget::new(BUDGET))?
                .into_owned())
        })
        .collect()
}

fn assert_reused_gaps_match(
    old: &Parsed,
    new: &Parsed,
    outcome: &ReuseOutcome,
) -> Result<(), Box<dyn Error>> {
    for window in outcome.reused().windows(2) {
        let (left, right) = (&window[0], &window[1]);
        // 只有旧/新块索引都相邻时才存在同一段块间 Gap；中间块被修改时跳过。
        if right.old_index() != left.old_index() + 1 || right.new_index() != left.new_index() + 1 {
            continue;
        }
        let old_gaps = gap_texts(
            old,
            left.old_range().byte_range().end,
            right.old_range().byte_range().start,
        )?;
        let new_gaps = gap_texts(
            new,
            left.new_range().byte_range().end,
            right.new_range().byte_range().start,
        )?;
        assert_eq!(
            old_gaps, new_gaps,
            "reused block gap content must match full reparse"
        );
    }
    Ok(())
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
    assert_reused_gaps_match(old, new, outcome)?;
    Ok(())
}

fn assert_tree_partition(parsed: &Parsed) -> Result<(), Box<dyn Error>> {
    let mut ranges: Vec<_> = parsed
        .blocks()
        .iter()
        .map(|block| block.range().byte_range())
        .chain(parsed.gaps().iter().map(|gap| gap.range().byte_range()))
        .collect();
    ranges.sort_by_key(|range| (range.start, range.end));
    let mut covered_until = 0usize;
    for range in ranges {
        assert_eq!(
            range.start, covered_until,
            "block/Gap must not leave holes or overlap"
        );
        assert!(range.end >= range.start, "ranges must be ordered");
        covered_until = range.end;
    }
    assert_eq!(
        covered_until,
        parsed.snapshot().len_bytes(),
        "tree must cover source bytes"
    );
    Ok(())
}

fn navigation_entries_within<'a>(
    parsed: &'a Parsed,
    range: &Range<usize>,
) -> Vec<&'a NavigationEntry> {
    parsed
        .navigation()
        .entries()
        .iter()
        .filter(|entry| {
            let source = entry.source().byte_range();
            source.start >= range.start && source.end <= range.end
        })
        .collect()
}

/// 成功复用只允许 Heading 导航条目：逐对重定位旧条目端点并比较种类、级别与空目标，
/// 同时确认其他复用块内没有 Toc、脚注或嵌套 Heading 条目。
fn assert_heading_source_anchors_match(
    old: &Parsed,
    new: &Parsed,
    outcome: &ReuseOutcome,
) -> Result<(), Box<dyn Error>> {
    for pair in outcome.reused() {
        let old_block = old
            .blocks()
            .get(pair.old_index())
            .ok_or("missing old block")?;
        let new_block = new
            .blocks()
            .get(pair.new_index())
            .ok_or("missing new block")?;
        let old_block_range = old_block.range().byte_range();
        let new_block_range = new_block.range().byte_range();
        let old_entries = navigation_entries_within(old, &old_block_range);
        let new_entries = navigation_entries_within(new, &new_block_range);
        if old_block.kind() == BlockKind::Heading {
            assert_eq!(old_entries.len(), 1, "heading block must have one entry");
            assert_eq!(new_entries.len(), 1, "reused heading must have one entry");
            let old_entry = old_entries[0];
            let new_entry = new_entries[0];
            assert_eq!(old_entry.kind(), NavigationKind::Heading);
            assert_eq!(new_entry.kind(), old_entry.kind());
            assert_eq!(new_entry.heading_level(), old_entry.heading_level());
            assert!(old_entry.targets().is_empty());
            assert!(new_entry.targets().is_empty());
            let old_source = old_entry.source().byte_range();
            let old_start = old.snapshot().src_offset(old_source.start)?;
            let old_end = old.snapshot().src_offset(old_source.end)?;
            let new_source = new_entry.source().byte_range();
            assert_eq!(
                pair.translate(new.snapshot(), &old_start)?.byte_offset(),
                new_source.start
            );
            assert_eq!(
                pair.translate(new.snapshot(), &old_end)?.byte_offset(),
                new_source.end
            );
            assert_eq!(new_source, new_block_range);
        } else {
            assert!(
                old_entries.is_empty(),
                "non-heading reused block must not contain navigation entries"
            );
            assert!(
                new_entries.is_empty(),
                "reused non-heading block must not contain navigation entries"
            );
        }
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
        let drawn_text = parsed
            .projection()
            .drawn_text()
            .get(clipped_drawn.clone())
            .ok_or("clipped drawn range is not a valid UTF-8 slice")?;
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
            text: drawn_text.to_string(),
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
fn reused_blocks_match_full_reparse_mapping_protection_and_heading_anchor()
-> Result<(), Box<dyn Error>> {
    let cases: [(&str, &str, usize); 4] = [
        ("# H\n\nA\n\nB\n\nC\n", "X\n\n# H\n\nA\n\nB\n\nC\n", 4),
        ("# H\n\nA\n\nB\n\nC\n", "# H\n\nA2\n\nB\n\nC\n", 3),
        ("# H\n\nA\n\nB\n\nC\n", "# H\n\nA\n\nB\n\nC\n\nD\n", 4),
        ("> Q\n\nA\n\nB\n", "X\n\n> Q\n\nA\n\nB\n", 3),
    ];
    for (old_text, new_text, expected) in cases {
        let old = parse_text(old_text)?;
        let new = parse_text(new_text)?;
        assert_tree_partition(&old)?;
        assert_tree_partition(&new)?;
        let outcome = reuse(&old, &new);
        assert_eq!(outcome.refusal(), None, "old {old_text:?} new {new_text:?}");
        assert_eq!(
            outcome.reused().len(),
            expected,
            "old {old_text:?} new {new_text:?}"
        );
        assert_all_reused_pairs_match(&old, &new, &outcome)?;
        assert_heading_source_anchors_match(&old, &new, &outcome)?;
    }
    Ok(())
}

#[test]
fn reused_blocks_match_gap_and_aligned_synthetic_mappings() -> Result<(), Box<dyn Error>> {
    // Aligned：实体解码与行内代码的 source 长度不同于 drawn；Synthetic：表格单元格
    // 的分隔制表符没有源码片段。两类都必须随复用块一起与全量重算逐段一致。
    let cases: [(&str, &str); 2] = [
        (
            "# H &amp; `a`\n\nA\n\nB\n",
            "Y\n\n# H &amp; `a`\n\nA\n\nB\n",
        ),
        (
            "| a | b |\n| - | - |\n| c | d |\n\nX\n",
            "Y\n\n| a | b |\n| - | - |\n| c | d |\n\nX\n",
        ),
    ];
    for (old_text, new_text) in cases {
        let old = parse_text(old_text)?;
        let new = parse_text(new_text)?;
        let outcome = reuse(&old, &new);
        assert_eq!(outcome.refusal(), None, "old {old_text:?}");
        assert!(
            !outcome.is_empty(),
            "sample must exercise a real reuse hit: {old_text:?}"
        );
        assert_all_reused_pairs_match(&old, &new, &outcome)?;
        assert_heading_source_anchors_match(&old, &new, &outcome)?;
    }
    let aligned = parse_text("# H &amp; `a`\n\nA\n")?;
    let precisions: Vec<_> = aligned
        .projection()
        .segments()
        .iter()
        .map(|segment| segment.precision())
        .collect();
    assert!(
        precisions.contains(&MappingPrecision::Aligned),
        "entity/code sample must produce Aligned mapping"
    );
    let table = parse_text("| a | b |\n| - | - |\n| c | d |\n\nX\n")?;
    assert!(
        table
            .projection()
            .segments()
            .iter()
            .any(|segment| segment.precision() == MappingPrecision::Synthetic),
        "table sample must produce Synthetic mapping"
    );
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
