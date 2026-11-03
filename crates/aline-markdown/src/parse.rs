//! 解析入口与无头 `Parsed` 结果；失败或超限时回到原文视图并保留诊断。

use std::fmt;
use std::ops::Range;

use aline_buffer::{BufferError, SrcRange, TextBudget, TextSnapshot};
use pulldown_cmark::Parser;

use crate::budget::{CancelCheck, NeverCancel, ParseBudget, ParseStats};
use crate::build::{self, BuildError};
use crate::config::ParseConfig;
use crate::diagnostic::{Diagnostic, DiagnosticKind};
use crate::mapping::{MappingBudget, MappingError, Projection};
use crate::mdast::{Block, BlockKind, Gap};
use crate::navigation::NavigationIndex;
use crate::pre::{
    FrontMatter, Rewrite, RewriteError, RewriteMap, rewrite_front_matter_with_cancel,
};
use crate::presentation::{self, PresentationNode};
use crate::protection::{ProtectedRange, ProtectionKind};
use crate::style::StyledRange;

/// 解析入口失败；错误不发布部分树。
#[derive(Debug)]
pub enum ParseError {
    /// 快照读取或范围校验失败。
    Source(BufferError),
    /// 投影构造失败。
    Mapping(MappingError),
    /// 解析副本重写失败。
    Rewrite(RewriteError),
    /// 配置版本为 0 或不受支持。
    InvalidConfig,
    /// 注入预算不足以安全完成解析。
    BudgetExceeded,
    /// 调用方请求取消；没有发布部分树。
    Cancelled,
    /// 内部构树失败；调用方应使用原文回退。
    Internal,
}

impl fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for ParseError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::Mapping(error) => Some(error),
            Self::Rewrite(error) => Some(error),
            _ => None,
        }
    }
}

/// 一次无头解析的不可变结果。
#[derive(Clone, Debug)]
pub struct Parsed {
    snapshot: TextSnapshot,
    config: ParseConfig,
    projection: Projection,
    blocks: Vec<Block>,
    gaps: Vec<Gap>,
    navigation: NavigationIndex,
    protected: Vec<ProtectedRange>,
    styled: Vec<StyledRange>,
    diagnostics: Vec<Diagnostic>,
    front_matter: Option<FrontMatter>,
    rewrite: Option<Rewrite>,
    stats: ParseStats,
    presentation: Vec<PresentationNode>,
}

impl Parsed {
    /// 返回结果绑定的源码快照。
    pub fn snapshot(&self) -> &TextSnapshot {
        &self.snapshot
    }

    /// 返回本次解析配置。
    pub const fn config(&self) -> ParseConfig {
        self.config
    }

    /// 返回源码到绘制的投影。
    pub fn projection(&self) -> &Projection {
        &self.projection
    }

    /// 返回顶层块，按源码起点升序。
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// 返回同一解析事件链生成的只读嵌套语义树。
    ///
    /// 节点来源属于当前快照，绘制范围属于当前全文投影；原文回退只返回 Raw 节点。
    /// 消费者据此布局基础 Markdown，不应在 UI 重扫 Markdown 或手算坐标。
    pub fn presentation(&self) -> &[PresentationNode] {
        &self.presentation
    }

    /// 返回块之间的显式间隙，按源码起点升序。
    pub fn gaps(&self) -> &[Gap] {
        &self.gaps
    }

    /// 返回只读导航索引；原文回退时为空。
    pub fn navigation(&self) -> &NavigationIndex {
        &self.navigation
    }

    /// 返回受保护源码子区间。
    pub fn protected_ranges(&self) -> &[ProtectedRange] {
        &self.protected
    }

    /// 返回有限 HTML 的展示样式区间。
    pub fn styled_ranges(&self) -> &[StyledRange] {
        &self.styled
    }

    /// 返回解析诊断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    /// 返回识别出的文首 front matter。
    pub fn front_matter(&self) -> Option<&FrontMatter> {
        self.front_matter.as_ref()
    }

    /// 返回重写映射；原文回退时没有重写。
    pub fn rewrite_map(&self) -> Option<&RewriteMap> {
        self.rewrite.as_ref().map(Rewrite::map)
    }

    /// 返回本次解析的只读统计；数值只用于验证注入预算。
    pub const fn stats(&self) -> ParseStats {
        self.stats
    }

    /// 用当前快照复验源码范围；旧快照范围会明确失败。
    pub fn validate_range(&self, range: &SrcRange) -> Result<Range<usize>, ParseError> {
        self.snapshot
            .validate_range(range)
            .map_err(ParseError::Source)
    }

    /// 生成稳定、无头、可人工审查的树 dump。
    pub fn dump_tree(&self) -> String {
        format!("{self}")
    }

    /// 返回源码原始字节的 FNV-1a 64 摘要，供 dump 标识「这一份源码」。
    ///
    /// 算法是手写的 Fowler–Noll–Vo 1a：以 `0xcbf29ce484222325` 为初值，逐字节
    /// `hash ^= byte` 后乘以素数 `0x100000001b3`（64 位环绕）。它**不是密码学哈希**，
    /// 只用于把「来源映射错误但块范围与总长度相同」的情况在 dump 里区分开；碰撞可能
    /// 存在，不得用于安全判定或去重裁决。
    ///
    /// 读取走 `iter_chunks()`，不物化全文、不注入新预算；底层表示异常时返回 `None`，
    /// dump 打印 `unavailable` 而不是静默给一个假摘要。
    fn source_identity(&self) -> Option<u64> {
        const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
        const PRIME: u64 = 0x0000_0100_0000_01b3;
        let mut hash = OFFSET_BASIS;
        for chunk in self.snapshot.iter_chunks().ok()? {
            for byte in chunk.as_bytes() {
                hash ^= u64::from(*byte);
                hash = hash.wrapping_mul(PRIME);
            }
        }
        Some(hash)
    }
}

impl fmt::Display for Parsed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            formatter,
            "config {} features 0x{:08x}",
            self.config.version(),
            self.config.features().bits()
        )?;
        // 既有前缀 `source <len>` / `drawn <len>` 保持不变（外部测试按前缀匹配），
        // 其后追加身份与绘制文字本体，使「范围/长度相同但来源不同」可被人工审查发现。
        match self.source_identity() {
            Some(hash) => writeln!(
                formatter,
                "source {} identity fnv1a64 0x{hash:016x} revision {}",
                self.snapshot.len_bytes(),
                self.snapshot.revision().value()
            )?,
            None => writeln!(
                formatter,
                "source {} identity fnv1a64 unavailable revision {}",
                self.snapshot.len_bytes(),
                self.snapshot.revision().value()
            )?,
        }
        writeln!(
            formatter,
            "drawn {} text {:?}",
            self.projection.drawn_text().len(),
            self.projection.drawn_text()
        )?;
        // 逐片段行按绘制起点升序，覆盖 `projection().segments()` 全部片段，不省略、不合并。
        for segment in self.projection.segments() {
            let drawn = segment.drawn_range();
            let source = segment.source_range().byte_range();
            writeln!(
                formatter,
                "segment {}..{} -> {}..{} {}",
                drawn.start,
                drawn.end,
                source.start,
                source.end,
                segment.precision().as_str()
            )?;
        }
        let mut entries: Vec<(usize, String)> = Vec::new();
        for block in &self.blocks {
            let range = block.range().byte_range();
            entries.push((
                range.start,
                format!(
                    "block {}..{} {}",
                    range.start,
                    range.end,
                    block.kind().as_str()
                ),
            ));
        }
        for gap in &self.gaps {
            let range = gap.range().byte_range();
            entries.push((range.start, format!("gap {}..{}", range.start, range.end)));
        }
        entries.sort_by_key(|entry| entry.0);
        for (_, line) in entries {
            writeln!(formatter, "{line}")?;
        }
        for protected in &self.protected {
            let range = protected.range().byte_range();
            writeln!(
                formatter,
                "protected {}..{} {}",
                range.start,
                range.end,
                protected.kind().as_str()
            )?;
        }
        for styled in &self.styled {
            let range = styled.range().byte_range();
            let style = styled.style();
            writeln!(
                formatter,
                "style {}..{} bold={} italic={} strike={} underline={} mark={} code={} link={}",
                range.start,
                range.end,
                style.bold(),
                style.italic(),
                style.strike(),
                style.underline(),
                style.mark(),
                style.code(),
                style.link()
            )?;
        }
        for diagnostic in &self.diagnostics {
            let range = diagnostic.range().byte_range();
            writeln!(
                formatter,
                "diagnostic {}..{} {}",
                range.start,
                range.end,
                diagnostic.kind().as_str()
            )?;
        }
        Ok(())
    }
}

/// 解析完整快照；配置版本 0 和读取失败直接返回错误，构树失败回退原文。
pub fn parse(
    snapshot: &TextSnapshot,
    config: ParseConfig,
    budget: ParseBudget,
) -> Result<Parsed, ParseError> {
    parse_with_cancel(snapshot, config, budget, &NeverCancel)
}

/// 与 [`parse`] 相同，并在重写、扫描与构树循环间轮询取消检查。
///
/// 取消时返回 [`ParseError::Cancelled`]，不返回截断或部分树。
pub fn parse_with_cancel(
    snapshot: &TextSnapshot,
    config: ParseConfig,
    budget: ParseBudget,
    cancel: &dyn CancelCheck,
) -> Result<Parsed, ParseError> {
    if cancel.is_cancelled() {
        return Err(ParseError::Cancelled);
    }
    if config.version() == 0 {
        return Err(ParseError::InvalidConfig);
    }
    let source_range = snapshot
        .src_range(0..snapshot.len_bytes())
        .map_err(ParseError::Source)?;
    let source = snapshot
        .slice(&source_range, TextBudget::new(budget.max_source_bytes()))
        .map_err(ParseError::Source)?;
    let rewrite = match rewrite_front_matter_with_cancel(&source, budget, cancel) {
        Ok(rewrite) => rewrite,
        Err(RewriteError::BudgetExceeded) => return Err(ParseError::BudgetExceeded),
        Err(RewriteError::Cancelled) => return Err(ParseError::Cancelled),
        Err(_) => {
            return raw_fallback(
                snapshot,
                config,
                budget,
                DiagnosticKind::RawFallback,
                ParseStats::default(),
            );
        }
    };
    if cancel.is_cancelled() {
        return Err(ParseError::Cancelled);
    }
    let events = Parser::new_ext(rewrite.text(), config.features().options()).into_offset_iter();
    let mut stats = ParseStats::default();
    let built = match build::build(
        snapshot, &source, &rewrite, events, budget, cancel, &mut stats,
    ) {
        Ok(built) => built,
        Err(BuildError::Cancelled) => return Err(ParseError::Cancelled),
        Err(BuildError::BudgetExceeded) => {
            return raw_fallback(
                snapshot,
                config,
                budget,
                DiagnosticKind::BudgetExceeded,
                stats,
            );
        }
        Err(_) => {
            return raw_fallback(snapshot, config, budget, DiagnosticKind::RawFallback, stats);
        }
    };
    if cancel.is_cancelled() {
        return Err(ParseError::Cancelled);
    }
    make_parsed(snapshot, config, rewrite, built, budget, stats, cancel)
}

fn make_parsed(
    snapshot: &TextSnapshot,
    config: ParseConfig,
    rewrite: Rewrite,
    built: build::Built,
    budget: ParseBudget,
    mut stats: ParseStats,
    cancel: &dyn CancelCheck,
) -> Result<Parsed, ParseError> {
    // M0 语义：`max_mapping_points` 同时作为投影索引点与片段数上限；
    // 片段数超限与索引点超限都映射为 `Mapping(BudgetExceeded)`。
    let mapping_budget =
        MappingBudget::new(budget.max_rewritten_bytes(), budget.max_mapping_points());
    let source_range = snapshot
        .src_range(0..snapshot.len_bytes())
        .map_err(ParseError::Source)?;
    let mapping_points = built.drawn.chars().count().saturating_add(1);
    let projection = Projection::from_segments(
        snapshot.clone(),
        source_range,
        built.drawn,
        built.segments,
        mapping_budget,
    )
    .map_err(ParseError::Mapping)?;
    stats.mapping_points = mapping_points;
    let presentation =
        match presentation::bind(built.presentation, snapshot, &projection, budget, cancel) {
            Ok(presentation) => presentation,
            Err(BuildError::Cancelled) => return Err(ParseError::Cancelled),
            Err(BuildError::BudgetExceeded) => {
                stats.budget_hits = stats.budget_hits.saturating_add(1);
                return raw_fallback(
                    snapshot,
                    config,
                    budget,
                    DiagnosticKind::BudgetExceeded,
                    stats,
                );
            }
            Err(_) => {
                return raw_fallback(snapshot, config, budget, DiagnosticKind::RawFallback, stats);
            }
        };
    Ok(Parsed {
        snapshot: snapshot.clone(),
        config,
        projection,
        blocks: built.blocks,
        gaps: built.gaps,
        navigation: built.navigation,
        protected: built.protected,
        styled: built.styled,
        diagnostics: built.diagnostics,
        front_matter: rewrite.front_matter().cloned(),
        rewrite: Some(rewrite),
        stats,
        presentation,
    })
}

fn raw_fallback(
    snapshot: &TextSnapshot,
    config: ParseConfig,
    budget: ParseBudget,
    kind: DiagnosticKind,
    mut stats: ParseStats,
) -> Result<Parsed, ParseError> {
    let len = snapshot.len_bytes();
    let source_range = snapshot.src_range(0..len).map_err(ParseError::Source)?;
    let source = snapshot
        .slice(&source_range, TextBudget::new(budget.max_source_bytes()))
        .map_err(ParseError::Source)?;
    let drawn = source.into_owned();
    let mapping_points = drawn.chars().count().saturating_add(1);
    let mut segments = Vec::new();
    if len > 0 {
        segments.push(
            crate::mapping::MappingSegment::new(
                0..len,
                source_range.clone(),
                crate::mapping::MappingPrecision::Exact,
            )
            .map_err(ParseError::Mapping)?,
        );
    }
    let projection = Projection::from_segments(
        snapshot.clone(),
        source_range.clone(),
        drawn,
        segments,
        MappingBudget::new(budget.max_rewritten_bytes(), budget.max_mapping_points()),
    )
    .map_err(ParseError::Mapping)?;
    stats.mapping_points = mapping_points;
    let blocks = if len > 0 {
        vec![Block::new(source_range.clone(), BlockKind::Raw)]
    } else {
        Vec::new()
    };
    let protected = if len > 0 {
        vec![ProtectedRange::new(
            source_range.clone(),
            ProtectionKind::RawFallback,
        )]
    } else {
        Vec::new()
    };
    let diagnostics = vec![Diagnostic::new(source_range, kind)];
    let presentation =
        presentation::raw_node(snapshot, &projection).map_err(|_| ParseError::Internal)?;
    Ok(Parsed {
        snapshot: snapshot.clone(),
        config,
        projection,
        blocks,
        gaps: Vec::new(),
        navigation: NavigationIndex::new(Vec::new()),
        protected,
        styled: Vec::new(),
        diagnostics,
        front_matter: None,
        rewrite: None,
        stats,
        presentation,
    })
}
