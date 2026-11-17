//! 源码查找和替换规划。文档宿主持有工作版本、查询代次和当前项，按版本采纳结果。
//! 本模块读取冻结源码并返回受检命中或变更计划，真实写入仍由文档拥有者负责。

use std::collections::VecDeque;
use std::fmt;
use std::ops::Range;

use aline_buffer::{
    DocumentRevision, SnapshotId, SrcRange, SrcSelection, TextBudget, TextSnapshot,
};
use aline_markdown::{MappingPrecision, Parsed};

use crate::budget::EditBudget;
use crate::command::{EditContext, EditMode, SourceEdit};
use crate::errors::map_buffer_error;
use crate::permissions::check_visual_permissions;
use crate::plan::{PlanOutcome, ReplaceKind, bind_parsed, plan_replace, precheck};
use crate::refusal::{BudgetKind, EditRefusal, RereadFailure};

/// 当前文档、工作版本（包含草稿）和查询代次。前两项由文档宿主注入。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FindVersion<Document, Working> {
    document: Document,
    working: Working,
    query_generation: u64,
}

impl<Document, Working> FindVersion<Document, Working> {
    /// 构造结果所属的身份。查询文字或大小写选项改变时，宿主须推进查询代次。
    /// 初始代次可为 `0`，消费时同样需要复验。
    pub const fn new(document: Document, working: Working, query_generation: u64) -> Self {
        Self {
            document,
            working,
            query_generation,
        }
    }
}

/// 查找字面量时采用的大小写规则。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FindCase {
    /// 按原始 UTF-8 字节匹配。
    Sensitive,
    /// 按 Rust 的逐标量 Unicode 小写展开匹配，统一普通与词尾 sigma。
    ///
    /// 命中首尾必须为原文标量边界。此规则不做 Unicode 规范化或区域设置折叠，
    /// 例如 `ß` 与 `ss` 不等价。
    Insensitive,
}

/// 查找的显式预算，用于限制源码、查询、Unicode 扫描工作区和结果分配。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FindBudget {
    text: TextBudget,
    max_matches: usize,
}

impl FindBudget {
    /// 源码、查询与 Unicode 扫描工作区分别受字节预算约束。
    /// 超出任何一项预算或命中上限时整批拒绝。
    pub const fn new(text: TextBudget, max_matches: usize) -> Self {
        Self { text, max_matches }
    }
}

/// 当前源码的一个字面量命中，携带快照范围和实际命中片段。
pub struct FindMatch {
    range: SrcRange,
    text: String,
}

impl FindMatch {
    /// 返回以 UTF-8 字节标识且绑定查找快照的半开源码范围。
    pub const fn range(&self) -> &SrcRange {
        &self.range
    }

    /// 返回原文片段，保留大小写与 Unicode 表示。
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// 同一冻结输入的全部结果，按源码顺序排列且互不重叠。
pub struct FindResults<Document, Working> {
    version: FindVersion<Document, Working>,
    revision: DocumentRevision,
    snapshot: SnapshotId,
    query: String,
    case: FindCase,
    budget: FindBudget,
    matches: Vec<FindMatch>,
}

impl<Document, Working> fmt::Debug for FindResults<Document, Working> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FindResults")
            .field("empty_query", &self.is_empty_query())
            .field("match_count", &self.matches.len())
            .finish_non_exhaustive()
    }
}

impl<Document, Working> FindResults<Document, Working> {
    /// 区分尚无查询和查询无匹配。
    pub fn is_empty_query(&self) -> bool {
        self.query.is_empty()
    }

    /// 返回同一查询代次的升序非重叠结果；列表索引从零开始。
    pub fn matches(&self) -> &[FindMatch] {
        &self.matches
    }
}

impl<Document: PartialEq, Working: PartialEq> FindResults<Document, Working> {
    /// 宿主采纳后台结果和执行替换前，以实际当前快照和查询身份复验。
    pub fn validate(
        &self,
        snapshot: &TextSnapshot,
        version: &FindVersion<Document, Working>,
    ) -> Result<(), EditRefusal> {
        if self.version != *version {
            return Err(EditRefusal::StaleFindResults);
        }
        if self.revision != snapshot.revision() {
            return Err(EditRefusal::WrongVersion {
                expected: self.revision,
                actual: snapshot.revision(),
            });
        }
        if self.snapshot != *snapshot.id() {
            return Err(EditRefusal::WrongSnapshot);
        }
        Ok(())
    }
}

/// 搜索冻结的完整源码；跨块与草稿覆盖段按连续源码匹配。
pub fn find_matches<Document, Working>(
    snapshot: &TextSnapshot,
    version: FindVersion<Document, Working>,
    query: &str,
    case: FindCase,
    budget: FindBudget,
) -> Result<FindResults<Document, Working>, EditRefusal> {
    let mut matches = Vec::new();
    if !query.is_empty() {
        check_source_bytes(query.len(), budget.text)?;
        let range = snapshot
            .src_range(0..snapshot.len_bytes())
            .map_err(map_buffer_error)?;
        let source = snapshot
            .slice(&range, budget.text)
            .map_err(map_buffer_error)?;
        let mut emit = |bytes: Range<usize>| {
            if matches.len() >= budget.max_matches {
                return Err(EditRefusal::BudgetExceeded {
                    kind: BudgetKind::FindMatches,
                    needed: matches.len().checked_add(1),
                    limit: Some(budget.max_matches),
                });
            }
            let range = snapshot
                .src_range(bytes.clone())
                .map_err(map_buffer_error)?;
            let text = copy_text(source.get(bytes).ok_or(EditRefusal::InvalidPlan)?)?;
            reserve(&mut matches, 1)?;
            matches.push(FindMatch { range, text });
            Ok(())
        };
        match case {
            FindCase::Sensitive => {
                for (src_off, matched) in source.match_indices(query) {
                    let end = src_off
                        .checked_add(matched.len())
                        .ok_or(EditRefusal::LengthOverflow)?;
                    emit(src_off..end)?;
                }
            }
            FindCase::Insensitive => find_lowercase(&source, query, budget.text, emit)?,
        }
    }
    Ok(FindResults {
        version,
        revision: snapshot.revision(),
        snapshot: snapshot.id().clone(),
        query: copy_text(query)?,
        case,
        budget,
        matches,
    })
}

/// 选择当前项或本次查找的全部命中，并提供替换文本。
#[derive(Clone, Copy)]
pub enum FindReplacement<'a> {
    /// 替换当前项，索引在本次结果中从零开始。
    Current {
        /// 当前项在结果中的索引。
        match_index: usize,
        /// 替换文字，可为空。
        text: &'a str,
    },
    /// 把所有命中替换为给定文字，一次计划仅包含一批补丁。
    All(&'a str),
}

/// 搜索计划保留的原意图。匹配结果必须在新的候选快照上重新生成。
#[derive(Clone)]
pub(crate) struct FindIntent {
    query: String,
    query_generation: u64,
    case: FindCase,
    budget: FindBudget,
    match_index: Option<usize>,
    replacement: String,
}

impl fmt::Debug for FindIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FindIntent")
            .field("match_index", &self.match_index)
            .field("case", &self.case)
            .finish_non_exhaustive()
    }
}

impl FindIntent {
    pub(crate) const fn query_generation(&self) -> u64 {
        self.query_generation
    }

    pub(crate) fn replan(
        &self,
        parsed: &Parsed,
        selection: &SrcSelection,
        context: &EditContext,
        budget: EditBudget,
    ) -> Result<PlanOutcome, EditRefusal> {
        // 调用方已经复验原计划身份。候选仅用于本次事务内重搜，沿用原查询代次。
        let candidate = FindVersion::new((), (), self.query_generation);
        let results = find_matches(
            parsed.snapshot(),
            candidate,
            &self.query,
            self.case,
            self.budget,
        )?;
        let replacement = match self.match_index {
            Some(match_index) => FindReplacement::Current {
                match_index,
                text: &self.replacement,
            },
            None => FindReplacement::All(&self.replacement),
        };
        plan_find_replace(
            &results,
            &candidate,
            parsed,
            selection,
            context,
            replacement,
            budget,
        )
    }
}

/// 复验当前查询身份、快照与实际命中片段，生成常规受检变更计划。
///
/// 可视化模式只替换在当前投影有可见来源的命中，并保留现有语义保护。
/// 仅命中不可见源码时返回 `RereadFailure::Projection`。宿主可定位源码模式，
/// 再基于当前身份重规划整个操作。宿主须在每次消费时通过
/// `EditContext::with_find_query_generation` 传入实时查询代次。
pub fn plan_find_replace<Document: PartialEq, Working: PartialEq>(
    results: &FindResults<Document, Working>,
    version: &FindVersion<Document, Working>,
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
    replacement: FindReplacement<'_>,
    budget: EditBudget,
) -> Result<PlanOutcome, EditRefusal> {
    precheck(context)?;
    let snapshot = parsed.snapshot();
    results.validate(snapshot, version)?;
    bind_parsed(snapshot, parsed)?;
    snapshot
        .validate_selection(selection)
        .map_err(map_buffer_error)?;
    if results.is_empty_query() {
        return Err(EditRefusal::EmptyFindQuery);
    }
    let (matches, replacement_text) = match replacement {
        FindReplacement::Current { match_index, text } => (
            std::slice::from_ref(
                results
                    .matches
                    .get(match_index)
                    .ok_or(EditRefusal::FindMatchUnavailable)?,
            ),
            text,
        ),
        FindReplacement::All(text) if !results.matches.is_empty() => (results.matches(), text),
        FindReplacement::All(_) => return Err(EditRefusal::FindMatchUnavailable),
    };
    if context.mode() == EditMode::Visual {
        check_visible_targets(parsed, matches, replacement_text, budget.text())?;
    }
    // 先限制重复替换文本的总分配，再交由 CheckedBatch 复验准确的补丁预算。
    let changed_count = matches
        .iter()
        .filter(|hit| hit.text() != replacement_text)
        .count();
    let inserted_bytes = changed_count
        .checked_mul(replacement_text.len())
        .ok_or(EditRefusal::LengthOverflow)?;
    if inserted_bytes > budget.text().max_bytes() {
        return Err(EditRefusal::BudgetExceeded {
            kind: BudgetKind::CandidateBytes,
            needed: Some(inserted_bytes),
            limit: Some(budget.text().max_bytes()),
        });
    }
    let mut edits = Vec::new();
    reserve(&mut edits, changed_count)?;
    for hit in matches {
        let bytes = snapshot
            .validate_range(hit.range())
            .map_err(map_buffer_error)?;
        let actual = snapshot
            .slice(hit.range(), budget.text())
            .map_err(map_buffer_error)?;
        if actual.as_ref() != hit.text() {
            return Err(EditRefusal::ExpectedFragmentMismatch { range: bytes });
        }
        if hit.text() != replacement_text {
            edits.push(SourceEdit::new(
                bytes,
                copy_text(hit.text())?,
                copy_text(replacement_text)?,
            ));
        }
    }
    let outcome = plan_replace(
        snapshot,
        parsed,
        selection,
        context,
        &edits,
        budget,
        ReplaceKind::Find,
    )?;
    if let PlanOutcome::Changed(mut plan) = outcome {
        plan.find_intent = Some(FindIntent {
            query: copy_text(&results.query)?,
            query_generation: results.version.query_generation,
            case: results.case,
            budget: results.budget,
            match_index: match replacement {
                FindReplacement::Current { match_index, .. } => Some(match_index),
                FindReplacement::All(_) => None,
            },
            replacement: copy_text(replacement_text)?,
        });
        return Ok(PlanOutcome::Changed(plan));
    }
    Ok(outcome)
}

fn check_visible_targets(
    parsed: &Parsed,
    matches: &[FindMatch],
    replacement: &str,
    budget: TextBudget,
) -> Result<(), EditRefusal> {
    if matches.iter().all(|hit| hit.text() == replacement) {
        return Ok(());
    }
    // 投影片段按绘制顺序保存。按源码起点排序，检查每个有序命中的连续覆盖。
    let mut visible = Vec::new();
    for segment in parsed.projection().segments() {
        if matches!(
            segment.precision(),
            MappingPrecision::Exact | MappingPrecision::Aligned
        ) && !segment.source_range().is_empty()
        {
            reserve(&mut visible, 1)?;
            visible.push((segment.source_range().byte_range(), segment.precision()));
        }
    }
    visible.sort_unstable_by_key(|(range, _)| range.start);
    let mut first = 0;
    for hit in matches.iter().filter(|hit| hit.text() != replacement) {
        let bytes = hit.range().byte_range();
        while visible
            .get(first)
            .is_some_and(|(range, _)| range.end <= bytes.start)
        {
            first += 1;
        }
        if !is_visible_target(parsed, bytes, &visible[first..], budget)? {
            // 保留批次的具体只读区拒绝，供宿主定位第一个受保护的源码目标。
            for hit in matches.iter().filter(|hit| hit.text() != replacement) {
                check_visual_permissions(parsed, &hit.range().byte_range())?;
            }
            return Err(EditRefusal::RereadFailed {
                reason: RereadFailure::Projection,
                cause: None,
            });
        }
    }
    Ok(())
}

fn is_visible_target(
    parsed: &Parsed,
    target: Range<usize>,
    visible: &[(Range<usize>, MappingPrecision)],
    budget: TextBudget,
) -> Result<bool, EditRefusal> {
    let mut covered_end = target.start;
    for (range, precision) in visible
        .iter()
        .take_while(|(range, _)| range.start < target.end)
    {
        if range.end <= covered_end {
            continue;
        }
        // 对齐片段不可逐字节逆推，只允许完整单位，如整个 &amp;。
        if *precision == MappingPrecision::Aligned
            && (range.start < target.start || range.end > target.end)
        {
            continue;
        }
        if range.start > covered_end
            && (covered_end == target.start
                || !is_visible_block_separator(parsed, covered_end..range.start, budget)?)
        {
            return Ok(false);
        }
        covered_end = range.end;
        if covered_end >= target.end {
            return Ok(true);
        }
    }
    Ok(false)
}

fn is_visible_block_separator(
    parsed: &Parsed,
    range: Range<usize>,
    budget: TextBudget,
) -> Result<bool, EditRefusal> {
    // 跨块命中允许完整的块间分隔，前后仍须分别有可见字节。
    let Ok(index) = parsed
        .gaps()
        .binary_search_by_key(&range.start, |gap| gap.range().byte_range().start)
    else {
        return Ok(false);
    };
    let gap = parsed
        .gaps()
        .get(index)
        .ok_or(EditRefusal::InvalidPlan)?
        .range();
    if gap.byte_range() != range {
        return Ok(false);
    }
    let text = parsed
        .snapshot()
        .slice(gap, budget)
        .map_err(map_buffer_error)?;
    Ok(text
        .bytes()
        .all(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n')))
}

fn check_source_bytes(needed: usize, budget: TextBudget) -> Result<(), EditRefusal> {
    let limit = budget.max_bytes();
    if needed > limit {
        return Err(EditRefusal::BudgetExceeded {
            kind: BudgetKind::SourceBytes,
            needed: Some(needed),
            limit: Some(limit),
        });
    }
    Ok(())
}

fn copy_text(text: &str) -> Result<String, EditRefusal> {
    let mut owned = String::new();
    owned
        .try_reserve_exact(text.len())
        .map_err(|_| EditRefusal::Allocation)?;
    owned.push_str(text);
    Ok(owned)
}

fn reserve<T>(values: &mut Vec<T>, additional: usize) -> Result<(), EditRefusal> {
    values
        .try_reserve(additional)
        .map_err(|_| EditRefusal::Allocation)
}

fn lowercase(scalar: char) -> std::char::ToLowercase {
    if scalar == 'ς' { 'σ' } else { scalar }.to_lowercase()
}

/// 用查询前缀表顺序扫描小写标量，保留最近一个查询长度内的原文边界。
fn find_lowercase(
    source: &str,
    query: &str,
    budget: TextBudget,
    mut emit: impl FnMut(Range<usize>) -> Result<(), EditRefusal>,
) -> Result<(), EditRefusal> {
    // 先数展开后的标量，再按实际元素大小核算三份工作区的容量。
    let pattern_len = query
        .chars()
        .flat_map(lowercase)
        .try_fold(0usize, |count, _| {
            count.checked_add(1).ok_or(EditRefusal::LengthOverflow)
        })?;
    let workspace_per_scalar = size_of::<char>() + size_of::<usize>() + size_of::<Option<usize>>();
    let workspace_bytes = pattern_len
        .checked_mul(workspace_per_scalar)
        .ok_or(EditRefusal::LengthOverflow)?;
    let limit = budget.max_bytes();
    if workspace_bytes > limit {
        return Err(EditRefusal::BudgetExceeded {
            kind: BudgetKind::FindWorkspace,
            needed: Some(workspace_bytes),
            limit: Some(limit),
        });
    }
    let mut pattern = Vec::new();
    pattern
        .try_reserve_exact(pattern_len)
        .map_err(|_| EditRefusal::Allocation)?;
    pattern.extend(query.chars().flat_map(lowercase));
    let prefix_lengths = prefix_lengths(&pattern)?;
    let mut starts = VecDeque::new();
    starts
        .try_reserve_exact(pattern_len)
        .map_err(|_| EditRefusal::Allocation)?;
    let mut matched = 0;
    for (src_off, original) in source.char_indices() {
        let mut start = Some(src_off);
        let mut lowered = lowercase(original).peekable();
        while let Some(scalar) = lowered.next() {
            if starts.len() == pattern.len() {
                starts.pop_front();
            }
            starts.push_back(start.take());
            matched = advance_prefix(&pattern, &prefix_lengths, matched, scalar)?;
            if matched == pattern.len() {
                if lowered.peek().is_none()
                    && let Some(Some(first)) = starts.front()
                {
                    let end = src_off
                        .checked_add(original.len_utf8())
                        .ok_or(EditRefusal::LengthOverflow)?;
                    emit(*first..end)?;
                    matched = 0;
                } else {
                    matched = shorter_prefix(&prefix_lengths, matched)?;
                }
            }
        }
    }
    Ok(())
}

fn prefix_lengths(pattern: &[char]) -> Result<Vec<usize>, EditRefusal> {
    let mut lengths = Vec::new();
    lengths
        .try_reserve_exact(pattern.len())
        .map_err(|_| EditRefusal::Allocation)?;
    let mut matched = 0;
    lengths.push(0);
    for &scalar in pattern.iter().skip(1) {
        matched = advance_prefix(pattern, &lengths, matched, scalar)?;
        lengths.push(matched);
    }
    Ok(lengths)
}

fn advance_prefix(
    pattern: &[char],
    prefix_lengths: &[usize],
    mut matched: usize,
    scalar: char,
) -> Result<usize, EditRefusal> {
    loop {
        if pattern.get(matched) == Some(&scalar) {
            return matched.checked_add(1).ok_or(EditRefusal::LengthOverflow);
        }
        if matched == 0 {
            return Ok(0);
        }
        matched = shorter_prefix(prefix_lengths, matched)?;
    }
}

fn shorter_prefix(prefix_lengths: &[usize], matched: usize) -> Result<usize, EditRefusal> {
    let previous = matched.checked_sub(1).ok_or(EditRefusal::InvalidPlan)?;
    prefix_lengths
        .get(previous)
        .copied()
        .ok_or(EditRefusal::InvalidPlan)
}
