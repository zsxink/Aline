//! 组 6 诊断证据补充：整源回退诊断、节点预算回退、合成围栏映射与 Mermaid 来源识别。
//!
//! 本文件只消费 `aline-markdown` 的公开 API，不修改解析实现；预算数值均为注入值，
//! 不代表 V-06 发布限额，也不宣称渲染能力或 M0 出口通过。

use std::error::Error;

use aline_buffer::{TextBudget, TextSnapshot};
use aline_markdown::{
    BlockKind, Diagnostic, DiagnosticKind, MappingPrecision, ParseBudget, ParseConfig, Parsed,
    ProtectionKind, Rewrite, RewriteEdit, parse,
};

/// 快照读取预算；只用于构造测试输入，与解析预算无关。
const SNAPSHOT_BUDGET: usize = 1024 * 1024;

/// 只取 `dump_tree()` 中某一类行，供**负向**断言使用。
///
/// dump 自 R09 起含 `drawn <len> text "…"` 与 `segment …`，整篇源码文字会进入 dump；
/// 因此 `!dump.contains("Table")` 这类断言会被源码正文里的同名词语误触发。
/// 负向断言必须先限定到对应行族，否则它测的是「夹具文本不含该词」而不是结构。
fn dump_lines_of(dump: &str, prefix: &str) -> String {
    dump.lines()
        .filter(|line| line.starts_with(prefix))
        .collect::<Vec<_>>()
        .join("\n")
}

fn snapshot_of(text: &str) -> Result<TextSnapshot, Box<dyn Error>> {
    Ok(TextSnapshot::new(
        text.to_string(),
        TextBudget::new(SNAPSHOT_BUDGET),
    )?)
}

fn parse_with(text: &str, budget: ParseBudget) -> Result<Parsed, Box<dyn Error>> {
    let snapshot = snapshot_of(text)?;
    Ok(parse(&snapshot, ParseConfig::m0(), budget)?)
}

/// 断言整源回退的完整契约：原文视图、单个 Raw 块、唯一诊断与整源保护。
fn assert_whole_source_fallback(
    parsed: &Parsed,
    source: &str,
    expected: DiagnosticKind,
) -> Result<(), Box<dyn Error>> {
    assert_eq!(parsed.projection().drawn_text(), source);
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::Raw)
    );
    assert!(parsed.gaps().is_empty());
    assert!(parsed.styled_ranges().is_empty());
    assert_eq!(
        parsed
            .protected_ranges()
            .iter()
            .map(|range| range.kind())
            .collect::<Vec<_>>(),
        vec![ProtectionKind::RawFallback]
    );
    assert_eq!(parsed.diagnostics().len(), 1);
    let diagnostic = parsed.diagnostics().first();
    assert_eq!(diagnostic.map(|entry| entry.kind()), Some(expected));
    assert_eq!(
        diagnostic.map(|entry| entry.range().byte_range()),
        Some(0..source.len())
    );
    let range = parsed.snapshot().src_range(0..source.len())?;
    let text = parsed
        .snapshot()
        .slice(&range, TextBudget::new(SNAPSHOT_BUDGET))?;
    assert_eq!(text.as_ref(), source);
    Ok(())
}

/// 编译期枚举清单：新增任何渲染相关种类都会让该 match 不再穷尽。
fn diagnostic_kind_name(kind: DiagnosticKind) -> &'static str {
    match kind {
        DiagnosticKind::RawFallback => "RawFallback",
        DiagnosticKind::BudgetExceeded => "BudgetExceeded",
    }
}

/// 编译期枚举清单：`BlockKind` 目前没有 Mermaid/图表/渲染种类。
fn block_kind_name(kind: BlockKind) -> &'static str {
    match kind {
        BlockKind::FrontMatter => "FrontMatter",
        BlockKind::Heading => "Heading",
        BlockKind::Paragraph => "Paragraph",
        BlockKind::BlockQuote => "BlockQuote",
        BlockKind::List => "List",
        BlockKind::CodeBlock => "CodeBlock",
        BlockKind::Table => "Table",
        BlockKind::FootnoteDefinition => "FootnoteDefinition",
        BlockKind::HtmlBlock => "HtmlBlock",
        BlockKind::Math => "Math",
        BlockKind::ThematicBreak => "ThematicBreak",
        BlockKind::Raw => "Raw",
    }
}

#[test]
fn segment_budget_fallback_keeps_full_source_and_reports_budget_exceeded()
-> Result<(), Box<dyn Error>> {
    let source = "# 标题\n\n正文\n";
    let parsed = parse_with(source, ParseBudget::m0().with_max_segments(0))?;
    // 读码结论：构树预算耗尽走 parse.rs 的 BudgetExceeded 分支；RawFallback 只用于
    // 非预算的重写/构树失败。因此这里按实际实现断言 BudgetExceeded，而不是把预算
    // 回退记成 RawFallback。
    assert_whole_source_fallback(&parsed, source, DiagnosticKind::BudgetExceeded)?;
    let len = source.len();
    let kind = DiagnosticKind::BudgetExceeded.as_str();
    let dump = parsed.dump_tree();
    assert!(dump.contains(&format!("block 0..{len} Raw")));
    assert!(dump.contains(&format!("protected 0..{len} RawFallback")));
    assert!(dump.contains(&format!("diagnostic 0..{len} {kind}")));
    Ok(())
}

#[test]
fn raw_fallback_diagnostic_type_and_dump_name_are_stable() -> Result<(), Box<dyn Error>> {
    let source = "x\n";
    let snapshot = snapshot_of(source)?;
    let range = snapshot.src_range(0..source.len())?;
    let diagnostic = Diagnostic::new(range, DiagnosticKind::RawFallback);
    assert_eq!(diagnostic.kind(), DiagnosticKind::RawFallback);
    assert_eq!(diagnostic.range().byte_range(), 0..source.len());
    assert_eq!(DiagnosticKind::RawFallback.as_str(), "RawFallback");
    assert_eq!(
        diagnostic_kind_name(DiagnosticKind::RawFallback),
        "RawFallback"
    );
    assert_eq!(
        diagnostic_kind_name(DiagnosticKind::BudgetExceeded),
        "BudgetExceeded"
    );
    // 局限说明：parse.rs 的 raw_fallback 只在重写/构树返回非预算错误时以 RawFallback
    // 构造诊断；公开 parse 入口没有可构造的非预算构树失败，预算注入路径统一映射为
    // BudgetExceeded。本用例只固定诊断类型、范围与稳定 dump 名称，不宣称 parse 的
    // RawFallback 分支已被执行。
    Ok(())
}

#[test]
fn node_budget_fallback_keeps_full_source_without_partial_tree() -> Result<(), Box<dyn Error>> {
    let source = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
    let parsed = parse_with(source, ParseBudget::m0().with_max_nodes(4))?;
    assert_whole_source_fallback(&parsed, source, DiagnosticKind::BudgetExceeded)?;
    let len = source.len();
    let dump = parsed.dump_tree();
    assert!(dump.contains(&format!("block 0..{len} Raw")));
    // 负向断言限定在 block 行族：dump 现含源码正文，直接 contains 会被文本误触发。
    let blocks = dump_lines_of(&dump, "block ");
    assert!(!blocks.contains("CodeBlock"));
    assert!(!blocks.contains("Table"));
    Ok(())
}

#[test]
fn dump_structure_checks_ignore_sensitive_words_in_drawn_text() -> Result<(), Box<dyn Error>> {
    let budget_source = "Table CodeBlock diagnostic\n";
    let budget_parsed = parse_with(budget_source, ParseBudget::m0().with_max_segments(0))?;
    assert_whole_source_fallback(
        &budget_parsed,
        budget_source,
        DiagnosticKind::BudgetExceeded,
    )?;
    let budget_dump = budget_parsed.dump_tree();
    assert!(budget_dump.contains("drawn "));
    assert!(budget_dump.contains("Table CodeBlock diagnostic"));
    let budget_blocks = dump_lines_of(&budget_dump, "block ");
    assert!(budget_blocks.contains("Raw"));
    assert!(!budget_blocks.contains("Table"));
    assert!(!budget_blocks.contains("CodeBlock"));
    let budget_diagnostics = dump_lines_of(&budget_dump, "diagnostic ");
    assert!(budget_diagnostics.contains("BudgetExceeded"));
    assert!(!budget_diagnostics.contains("Table"));
    assert!(!budget_diagnostics.contains("CodeBlock"));

    let mermaid_source = "```mermaid\ndiagnostic\n```\n";
    let mermaid_parsed = parse_with(mermaid_source, ParseBudget::m0())?;
    assert_eq!(mermaid_parsed.projection().drawn_text(), "diagnostic\n");
    let mermaid_dump = mermaid_parsed.dump_tree();
    assert!(mermaid_dump.contains("diagnostic"));
    let mermaid_blocks = dump_lines_of(&mermaid_dump, "block ");
    assert!(mermaid_blocks.contains("CodeBlock"));
    let mermaid_diagnostics = dump_lines_of(&mermaid_dump, "diagnostic ");
    assert!(mermaid_diagnostics.is_empty());
    Ok(())
}

#[test]
fn synthetic_fence_maps_to_empty_source_anchor_not_real_source() -> Result<(), Box<dyn Error>> {
    let source = "x\n";
    let edit = RewriteEdit::new(0..0, "```\n".to_string(), MappingPrecision::Synthetic)?;
    let rewrite = Rewrite::apply(source, &[edit], ParseBudget::m0())?;

    // 期望（手算）：合成围栏 4 字节 + 原文 2 字节。
    assert_eq!(rewrite.text(), "```\nx\n");
    assert_eq!(rewrite.map().rewritten_len(), 6);
    assert!(!source.contains("```"));

    let segments = rewrite.map().segments();
    assert_eq!(segments.len(), 2);
    assert_eq!(
        segments.first().map(|segment| segment.source_range()),
        Some(0..0)
    );
    assert_eq!(
        segments.first().map(|segment| segment.rewritten_range()),
        Some(0..4)
    );
    assert_eq!(
        segments.first().map(|segment| segment.precision()),
        Some(MappingPrecision::Synthetic)
    );
    assert_eq!(
        segments.get(1).map(|segment| segment.source_range()),
        Some(0..2)
    );
    assert_eq!(
        segments.get(1).map(|segment| segment.rewritten_range()),
        Some(4..6)
    );
    assert_eq!(
        segments.get(1).map(|segment| segment.precision()),
        Some(MappingPrecision::Exact)
    );

    // 合成片段只能映射到空源码锚点，不能冒充真实源码范围。
    let synthetic = rewrite.map().to_source(0..4)?;
    assert!(synthetic.source_range().is_empty());
    assert_eq!(synthetic.source_range(), 0..0);
    assert_eq!(synthetic.precision(), MappingPrecision::Synthetic);
    let anchor = rewrite.map().to_source(0..0)?;
    assert_eq!(anchor.source_range(), 0..0);
    assert_eq!(anchor.precision(), MappingPrecision::Synthetic);

    // 原样片段保持精确范围。
    let first_byte = rewrite.map().to_source(4..5)?;
    assert_eq!(first_byte.source_range(), 0..1);
    assert_eq!(first_byte.precision(), MappingPrecision::Exact);
    let newline = rewrite.map().to_source(5..6)?;
    assert_eq!(newline.source_range(), 1..2);
    assert_eq!(newline.precision(), MappingPrecision::Exact);
    let original = rewrite.map().to_source(4..6)?;
    assert_eq!(original.source_range(), 0..2);
    assert_eq!(original.precision(), MappingPrecision::Exact);

    // 跨合成与原样的范围按最强精确性降级：不能被当作精确源范围。
    let straddling = rewrite.map().to_source(3..5)?;
    assert_eq!(straddling.source_range(), 0..1);
    assert_eq!(straddling.precision(), MappingPrecision::Synthetic);
    let whole = rewrite.map().to_source(0..6)?;
    assert_eq!(whole.source_range(), 0..2);
    assert_eq!(whole.precision(), MappingPrecision::Synthetic);
    assert_ne!(whole.precision(), MappingPrecision::Exact);
    Ok(())
}

#[test]
fn mermaid_fence_is_source_only_code_block_without_render_backend() -> Result<(), Box<dyn Error>> {
    let source = "```mermaid\ngraph TD\n```\n";
    let parsed = parse_with(source, ParseBudget::m0())?;
    assert!(parsed.diagnostics().is_empty());
    assert!(parsed.protected_ranges().is_empty());
    assert!(parsed.styled_ranges().is_empty());
    assert_eq!(parsed.blocks().len(), 1);
    assert_eq!(
        parsed.blocks().first().map(|block| block.kind()),
        Some(BlockKind::CodeBlock)
    );
    assert_eq!(
        parsed
            .blocks()
            .first()
            .map(|block| block.range().byte_range()),
        Some(0..23)
    );
    // 围栏正文作为源码文本保留；没有渲染后的图形或语言专属节点。
    assert_eq!(parsed.projection().drawn_text(), "graph TD\n");
    let dump = parsed.dump_tree();
    assert!(dump.contains("block 0..23 CodeBlock"));
    assert!(dump.contains("gap 23..24"));
    // 同上：源码正文里的「diagnostic」不算，只断言没有 diagnostic 行。
    assert_eq!(
        dump.lines()
            .filter(|line| line.starts_with("diagnostic "))
            .count(),
        0
    );
    assert_eq!(block_kind_name(BlockKind::CodeBlock), "CodeBlock");

    // 语言信息只存在于源码围栏文本中，可由块范围定位；公开 API 没有渲染后端类型，
    // 上面的穷尽 match 也确认 `BlockKind` 没有 Mermaid/图表种类。本用例不宣称渲染能力。
    let fence_range = parsed.snapshot().src_range(0..23)?;
    let fence = parsed
        .snapshot()
        .slice(&fence_range, TextBudget::new(SNAPSHOT_BUDGET))?;
    assert_eq!(fence.as_ref(), "```mermaid\ngraph TD\n```");
    assert!(fence.as_ref().starts_with("```mermaid"));
    assert!(fence.as_ref().ends_with("```"));
    let source_range = parsed.snapshot().src_range(0..source.len())?;
    let text = parsed
        .snapshot()
        .slice(&source_range, TextBudget::new(SNAPSHOT_BUDGET))?;
    assert_eq!(text.as_ref(), source);
    Ok(())
}
