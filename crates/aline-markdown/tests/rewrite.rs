//! 组 6.2：解析副本重写、逐级复合、跨片段来源、控制字节与失败回退。

use std::error::Error;
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    CancelCheck, CancelToken, MappingPrecision, ParseBudget, ParseConfig, Rewrite, RewriteEdit,
    RewriteError, RewriteMap, parse, rewrite_front_matter, rewrite_front_matter_with_cancel,
};

/// 逐片读取映射；复合预期按两层片段边界手工推导，不调用 compose 生成。
fn segments_of(map: &RewriteMap) -> Vec<(Range<usize>, Range<usize>, MappingPrecision)> {
    map.segments()
        .iter()
        .map(|segment| {
            (
                segment.source_range(),
                segment.rewritten_range(),
                segment.precision(),
            )
        })
        .collect()
}

struct CancelAfter {
    checks: AtomicUsize,
    limit: usize,
}

impl CancelCheck for CancelAfter {
    fn is_cancelled(&self) -> bool {
        self.checks.fetch_add(1, Ordering::Relaxed) >= self.limit
    }
}

fn edit(
    source: std::ops::Range<usize>,
    replacement: &str,
    precision: MappingPrecision,
) -> Result<RewriteEdit, RewriteError> {
    RewriteEdit::new(source, replacement.to_string(), precision)
}

#[test]
fn front_matter_is_hidden_in_parse_copy_and_maps_back_to_source() -> Result<(), Box<dyn Error>> {
    let source = "---\na: 1\n---\n# H\n";
    let rewrite = rewrite_front_matter(source, ParseBudget::m0())?;
    assert_eq!(rewrite.text(), "\n# H\n");
    assert_eq!(rewrite.map().rewritten_len(), 5);
    let heading = rewrite.map().to_source(1..4)?;
    assert_eq!(heading.source_range(), 13..16);
    assert_eq!(heading.precision(), MappingPrecision::Exact);
    let start = rewrite.map().to_source(0..0)?;
    assert_eq!(start.source_range(), 12..12);
    let end = rewrite.map().to_source(5..5)?;
    assert_eq!(end.source_range(), 17..17);
    assert!(rewrite.front_matter().is_some());
    Ok(())
}

#[test]
fn multi_edit_map_preserves_each_source_segment() -> Result<(), Box<dyn Error>> {
    let source = "abcdef";
    let edits = vec![
        edit(1..3, "XY", MappingPrecision::Exact)?,
        edit(4..4, "!", MappingPrecision::Synthetic)?,
    ];
    let rewrite = Rewrite::apply(source, &edits, ParseBudget::m0())?;
    assert_eq!(rewrite.text(), "aXYd!ef");
    assert_eq!(rewrite.map().rewritten_len(), 7);
    let replaced = rewrite.map().to_source(1..3)?;
    assert_eq!(replaced.source_range(), 1..3);
    assert_eq!(replaced.precision(), MappingPrecision::Aligned);
    let inserted = rewrite.map().to_source(4..5)?;
    assert_eq!(inserted.source_range(), 4..4);
    assert_eq!(inserted.precision(), MappingPrecision::Synthetic);
    let gap = rewrite.map().to_source(3..4)?;
    assert_eq!(gap.source_range(), 3..4);
    assert_eq!(gap.precision(), MappingPrecision::Exact);
    let tail = rewrite.map().to_source(5..7)?;
    assert_eq!(tail.source_range(), 4..6);
    assert_eq!(tail.precision(), MappingPrecision::Exact);
    let whole = rewrite.map().to_source(0..7)?;
    assert_eq!(whole.source_range(), 0..6);
    assert_eq!(whole.precision(), MappingPrecision::Synthetic);
    Ok(())
}

#[test]
fn maps_compose_through_two_rewrite_levels() -> Result<(), Box<dyn Error>> {
    let first = Rewrite::apply(
        "abcdef",
        &[edit(1..3, "X", MappingPrecision::Exact)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(first.text(), "aXdef");
    let second = Rewrite::apply(
        first.text(),
        &[edit(0..1, "", MappingPrecision::Hidden)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(second.text(), "Xdef");
    let composed = first.map().compose(second.map())?;
    assert_eq!(composed.rewritten_len(), 4);
    let mapped = composed.to_source(0..4)?;
    assert_eq!(mapped.source_range(), 1..6);
    assert_eq!(mapped.precision(), MappingPrecision::Aligned);
    let anchor = composed.to_source(0..0)?;
    // 空锚点落在 "X" 所在的 Aligned 片（源码 1..3），不再并入其后的 Exact 3..6；
    // 旧的 1..6 只有在把该片与后续 Exact 片并成一片时才可能，那正是 F-03 的并集化行为。
    assert_eq!(anchor.source_range(), 1..3);
    assert_eq!(anchor.precision(), MappingPrecision::Aligned);
    Ok(())
}

#[test]
fn compose_with_identity_rewrite_preserves_every_first_level_segment() -> Result<(), Box<dyn Error>>
{
    // 第一层：在 "abc" 偏移 1 插入合成 "!"，得到 "a!bc"。
    // 手工推导的第一层片段（源码 -> 重写）：
    //   Exact   0..1 -> 0..1
    //   Synthetic 1..1 -> 1..2
    //   Exact   1..3 -> 2..4
    let first = Rewrite::apply(
        "abc",
        &[edit(1..1, "!", MappingPrecision::Synthetic)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(first.text(), "a!bc");
    assert_eq!(
        segments_of(first.map()),
        vec![
            (0..1, 0..1, MappingPrecision::Exact),
            (1..1, 1..2, MappingPrecision::Synthetic),
            (1..3, 2..4, MappingPrecision::Exact),
        ]
    );
    // 第二层：恒等重写，片段为 Exact 0..4 -> 0..4。
    let identity = Rewrite::apply(first.text(), &[], ParseBudget::m0())?;
    assert_eq!(
        segments_of(identity.map()),
        vec![(0..4, 0..4, MappingPrecision::Exact)]
    );
    // 复合必须按片段边界求交，保留每一片，而不是并成 0..3 -> 0..4 Synthetic。
    let composed = first.map().compose(identity.map())?;
    assert_eq!(composed.rewritten_len(), 4);
    assert_eq!(
        segments_of(&composed),
        vec![
            (0..1, 0..1, MappingPrecision::Exact),
            (1..1, 1..2, MappingPrecision::Synthetic),
            (1..3, 2..4, MappingPrecision::Exact),
        ]
    );
    // "b" 位于重写 2..3，必须仍是 Exact 1..2，不能退化成合成并集。
    let b = composed.to_source(2..3)?;
    assert_eq!(b.source_range(), 1..2);
    assert_eq!(b.precision(), MappingPrecision::Exact);
    // 合成 "!" 的重写 1..2 只映射到空源码锚点 1..1。
    let synthetic = composed.to_source(1..2)?;
    assert_eq!(synthetic.source_range(), 1..1);
    assert_eq!(synthetic.precision(), MappingPrecision::Synthetic);
    // 合成片段起点锚点仍为 Synthetic 1..1。
    let synthetic_anchor = composed.to_source(1..1)?;
    assert_eq!(synthetic_anchor.source_range(), 1..1);
    assert_eq!(synthetic_anchor.precision(), MappingPrecision::Synthetic);
    // 各合法端点：Exact 片段内逐字节平移。
    let head = composed.to_source(0..1)?;
    assert_eq!(head.source_range(), 0..1);
    assert_eq!(head.precision(), MappingPrecision::Exact);
    let tail = composed.to_source(3..4)?;
    assert_eq!(tail.source_range(), 2..3);
    assert_eq!(tail.precision(), MappingPrecision::Exact);
    // 跨片段范围按最强精确性降级，并取源码并集。
    let straddling = composed.to_source(0..2)?;
    assert_eq!(straddling.source_range(), 0..1);
    assert_eq!(straddling.precision(), MappingPrecision::Synthetic);
    let whole = composed.to_source(0..4)?;
    assert_eq!(whole.source_range(), 0..3);
    assert_eq!(whole.precision(), MappingPrecision::Synthetic);
    Ok(())
}

#[test]
fn compose_pairing_rules_for_empty_ranges_are_explicit() -> Result<(), Box<dyn Error>> {
    // 第一层：隐藏 "ab" 的 "b"，得到 "a"；片段为 Exact 0..1 -> 0..1、Hidden 1..2 -> 1..1。
    let first = Rewrite::apply(
        "ab",
        &[edit(1..2, "", MappingPrecision::Hidden)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(first.text(), "a");
    assert_eq!(
        segments_of(first.map()),
        vec![
            (0..1, 0..1, MappingPrecision::Exact),
            (1..2, 1..1, MappingPrecision::Hidden),
        ]
    );
    // 第二层：把 "a" 替换为 "X"，得到 Aligned 0..1 -> 0..1。
    let second = Rewrite::apply(
        first.text(),
        &[edit(0..1, "X", MappingPrecision::Exact)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(second.text(), "X");
    assert_eq!(
        segments_of(second.map()),
        vec![(0..1, 0..1, MappingPrecision::Aligned)]
    );
    // Hidden 片段的空 rewritten 与第二层非空片段不重叠，复合后保留该隐藏片：
    // 源码 1..2 -> 重写 1..1，精确性 Hidden（与第二层 Exact 结合仍取较强者 Hidden）。
    let composed = first.map().compose(second.map())?;
    assert_eq!(composed.rewritten_len(), 1);
    assert_eq!(
        segments_of(&composed),
        vec![
            (0..1, 0..1, MappingPrecision::Aligned),
            (1..2, 1..1, MappingPrecision::Hidden),
        ]
    );
    let visible = composed.to_source(0..1)?;
    assert_eq!(visible.source_range(), 0..1);
    assert_eq!(visible.precision(), MappingPrecision::Aligned);
    let hidden_anchor = composed.to_source(1..1)?;
    assert_eq!(hidden_anchor.source_range(), 1..2);
    assert_eq!(hidden_anchor.precision(), MappingPrecision::Hidden);
    Ok(())
}

#[test]
fn compose_anchors_second_level_empty_sources_without_union() -> Result<(), Box<dyn Error>> {
    // 第一层：源码 "ab" 的 "a" 替换为 "X"，得到 "Xb"；
    // 片段为 Aligned 0..1 -> 0..1、Exact 1..2 -> 1..2。
    let first = Rewrite::apply(
        "ab",
        &[edit(0..1, "X", MappingPrecision::Exact)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(first.text(), "Xb");
    assert_eq!(
        segments_of(first.map()),
        vec![
            (0..1, 0..1, MappingPrecision::Aligned),
            (1..2, 1..2, MappingPrecision::Exact),
        ]
    );
    // 第二层：在 "Xb" 偏移 1 插入合成 "!"，得到 "X!b"。
    // 片段为 Exact 0..1 -> 0..1、Synthetic 1..1 -> 1..2、Exact 1..2 -> 2..3。
    let second = Rewrite::apply(
        first.text(),
        &[edit(1..1, "!", MappingPrecision::Synthetic)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(second.text(), "X!b");
    assert_eq!(
        segments_of(second.map()),
        vec![
            (0..1, 0..1, MappingPrecision::Exact),
            (1..1, 1..2, MappingPrecision::Synthetic),
            (1..2, 2..3, MappingPrecision::Exact),
        ]
    );
    // 复合：第二层的空来源合成片锚定到第一层 Exact 1..2 的起点 1..1；
    // 合成内容不伪造非空来源，也不把两侧片并成一片。
    let composed = first.map().compose(second.map())?;
    assert_eq!(composed.rewritten_len(), 3);
    assert_eq!(
        segments_of(&composed),
        vec![
            (0..1, 0..1, MappingPrecision::Aligned),
            (1..1, 1..2, MappingPrecision::Synthetic),
            (1..2, 2..3, MappingPrecision::Exact),
        ]
    );
    let head = composed.to_source(0..1)?;
    assert_eq!(head.source_range(), 0..1);
    assert_eq!(head.precision(), MappingPrecision::Aligned);
    let synthetic = composed.to_source(1..2)?;
    assert_eq!(synthetic.source_range(), 1..1);
    assert_eq!(synthetic.precision(), MappingPrecision::Synthetic);
    let synthetic_anchor = composed.to_source(1..1)?;
    assert_eq!(synthetic_anchor.source_range(), 1..1);
    assert_eq!(synthetic_anchor.precision(), MappingPrecision::Synthetic);
    let tail = composed.to_source(2..3)?;
    assert_eq!(tail.source_range(), 1..2);
    assert_eq!(tail.precision(), MappingPrecision::Exact);

    // 第二层的空来源隐藏片（空源码 + 空替换）同样只留空锚点。
    let hidden = Rewrite::apply(
        first.text(),
        &[edit(1..1, "", MappingPrecision::Hidden)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(hidden.text(), "Xb");
    assert_eq!(
        segments_of(hidden.map()),
        vec![
            (0..1, 0..1, MappingPrecision::Exact),
            (1..1, 1..1, MappingPrecision::Hidden),
            (1..2, 1..2, MappingPrecision::Exact),
        ]
    );
    let composed = first.map().compose(hidden.map())?;
    assert_eq!(
        segments_of(&composed),
        vec![
            (0..1, 0..1, MappingPrecision::Aligned),
            (1..1, 1..1, MappingPrecision::Hidden),
            (1..2, 1..2, MappingPrecision::Exact),
        ]
    );
    let hidden_anchor = composed.to_source(1..1)?;
    // 空来源隐藏片与紧随的可见 Exact 片同点：既有 anchor_to_source 让包含该点的
    // 可见片段优先，因此锚点解析为 Exact 1..1（该优先级不属于本次复合修复）。
    assert_eq!(hidden_anchor.source_range(), 1..1);
    assert_eq!(hidden_anchor.precision(), MappingPrecision::Exact);

    // 末尾的空来源隐藏片没有后续可见片段遮蔽，锚点保留 Hidden。
    let trailing = Rewrite::apply(
        first.text(),
        &[edit(2..2, "", MappingPrecision::Hidden)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(
        segments_of(trailing.map()),
        vec![
            (0..2, 0..2, MappingPrecision::Exact),
            (2..2, 2..2, MappingPrecision::Hidden),
        ]
    );
    let composed = first.map().compose(trailing.map())?;
    assert_eq!(
        segments_of(&composed),
        vec![
            (0..1, 0..1, MappingPrecision::Aligned),
            (1..2, 1..2, MappingPrecision::Exact),
            (2..2, 2..2, MappingPrecision::Hidden),
        ]
    );
    let hidden_anchor = composed.to_source(2..2)?;
    assert_eq!(hidden_anchor.source_range(), 2..2);
    assert_eq!(hidden_anchor.precision(), MappingPrecision::Hidden);
    Ok(())
}

#[test]
fn compose_keeps_hidden_anchor_outside_non_exact_merged_piece() -> Result<(), Box<dyn Error>> {
    // 第一层：删除 "abc" 的 "b"，得到 "ac"。
    // 片段为 Exact 0..1 -> 0..1、Hidden 1..2 -> 1..1、Exact 2..3 -> 1..2。
    let first = Rewrite::apply(
        "abc",
        &[
            edit(0..1, "a", MappingPrecision::Exact)?,
            edit(1..2, "", MappingPrecision::Hidden)?,
            edit(2..3, "c", MappingPrecision::Exact)?,
        ],
        ParseBudget::m0(),
    )?;
    assert_eq!(first.text(), "ac");
    assert_eq!(
        segments_of(first.map()),
        vec![
            (0..1, 0..1, MappingPrecision::Exact),
            (1..2, 1..1, MappingPrecision::Hidden),
            (2..3, 1..2, MappingPrecision::Exact),
        ]
    );
    // 第二层：把 "ac" 整体替换为 "Z"，得到 Aligned 0..2 -> 0..1（非 Exact、rewritten 非空）。
    let second = Rewrite::apply(
        first.text(),
        &[edit(0..2, "Z", MappingPrecision::Exact)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(second.text(), "Z");
    assert_eq!(
        segments_of(second.map()),
        vec![(0..2, 0..1, MappingPrecision::Aligned)]
    );
    // 第一层隐藏片的空 rewritten（点 1）落在第二层非 Exact 片段内部：它只塌缩到该片
    // 的终点 1..1，作为零宽锚点排在合并片之后，不与合并片 0..1 重叠。
    // 合并片的来源取可见交集 0..1 与 2..3 的并集 0..3；其中 1..2 是被隐藏消费的源码，
    // 精确性已降为 Aligned，不冒充逐字节对应。
    let composed = first.map().compose(second.map())?;
    assert_eq!(composed.rewritten_len(), 1);
    assert_eq!(
        segments_of(&composed),
        vec![
            (0..3, 0..1, MappingPrecision::Aligned),
            (1..2, 1..1, MappingPrecision::Hidden),
        ]
    );
    // 重写端顺序仍单调：合并片覆盖 0..1，零宽隐藏片在其终点。
    let merged = composed.to_source(0..1)?;
    assert_eq!(merged.source_range(), 0..3);
    assert_eq!(merged.precision(), MappingPrecision::Aligned);
    let anchor = composed.to_source(1..1)?;
    assert_eq!(anchor.source_range(), 1..2);
    assert_eq!(anchor.precision(), MappingPrecision::Hidden);
    Ok(())
}

#[test]
fn control_bytes_do_not_collide_with_synthetic_markers() -> Result<(), Box<dyn Error>> {
    let source = "abc";
    let rewrite = Rewrite::apply(
        source,
        &[edit(0..0, "\u{1}\u{2}\u{8}", MappingPrecision::Synthetic)?],
        ParseBudget::m0(),
    )?;
    assert_eq!(rewrite.text(), "\u{1}\u{2}\u{8}abc");
    let synthetic = rewrite.map().to_source(0..3)?;
    assert_eq!(synthetic.source_range(), 0..0);
    assert_eq!(synthetic.precision(), MappingPrecision::Synthetic);
    let exact = rewrite.map().to_source(3..6)?;
    assert_eq!(exact.source_range(), 0..3);
    assert_eq!(exact.precision(), MappingPrecision::Exact);
    Ok(())
}

#[test]
fn reversed_overlapping_and_non_boundary_edits_are_rejected() -> Result<(), Box<dyn Error>> {
    let (start, end) = (3usize, 1usize);
    assert!(matches!(
        edit(start..end, "x", MappingPrecision::Exact),
        Err(RewriteError::ReversedRange)
    ));
    let source = "abcdef";
    let overlapping = vec![
        edit(1..4, "x", MappingPrecision::Exact)?,
        edit(3..5, "y", MappingPrecision::Exact)?,
    ];
    assert!(matches!(
        Rewrite::apply(source, &overlapping, ParseBudget::m0()),
        Err(RewriteError::OverlappingEdits)
    ));
    let not_boundary = vec![edit(1..2, "x", MappingPrecision::Exact)?];
    assert!(matches!(
        Rewrite::apply("中文", &not_boundary, ParseBudget::m0()),
        Err(RewriteError::NotCharBoundary)
    ));
    let out_of_bounds = vec![edit(0..99, "x", MappingPrecision::Exact)?];
    assert!(matches!(
        Rewrite::apply(source, &out_of_bounds, ParseBudget::m0()),
        Err(RewriteError::InvalidRange)
    ));
    Ok(())
}

#[test]
fn parse_budget_failure_falls_back_to_original_text() -> Result<(), Box<dyn Error>> {
    let source = "# 标题\n\n正文\n";
    let snapshot = TextSnapshot::new(source.to_string(), TextBudget::new(1024))?;
    let budget = ParseBudget::m0().with_max_segments(0);
    let parsed = parse(&snapshot, ParseConfig::m0(), budget)?;
    assert_eq!(parsed.projection().drawn_text(), source);
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(aline_markdown::BlockKind::Raw)
    );
    assert_eq!(parsed.diagnostics().len(), 1);
    assert_eq!(parsed.stats().budget_hits(), 1);
    Ok(())
}

#[test]
fn pre_cancelled_rewrite_returns_cancelled_without_text() -> Result<(), Box<dyn Error>> {
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = Rewrite::apply_with_cancel("abc", &[], ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(RewriteError::Cancelled)));
    Ok(())
}

#[test]
fn cancellable_rewrite_stops_between_edits() -> Result<(), Box<dyn Error>> {
    let edits = vec![
        edit(0..1, "X", MappingPrecision::Exact)?,
        edit(2..3, "Y", MappingPrecision::Exact)?,
    ];
    let cancel = CancelAfter {
        checks: AtomicUsize::new(0),
        limit: 3,
    };
    let result = Rewrite::apply_with_cancel("abcdef", &edits, ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(RewriteError::Cancelled)));
    Ok(())
}

#[test]
fn pre_cancelled_front_matter_rewrite_returns_cancelled() -> Result<(), Box<dyn Error>> {
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = rewrite_front_matter_with_cancel("---\na: 1\n---\n", ParseBudget::m0(), &cancel);
    assert!(matches!(result, Err(RewriteError::Cancelled)));
    Ok(())
}
