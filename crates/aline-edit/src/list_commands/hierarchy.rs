//! 候选列表层级复验。解析树只用于验证移动与归属，源码仍由补丁保留。

use std::ops::Range;

use aline_buffer::{LineIndex, LineIndexBudget, SrcOffset, SrcRange};
use aline_markdown::{Block, BlockKind, Parsed, PresentationKind, PresentationNode};

use crate::budget::EditBudget;
use crate::errors::map_buffer_error;
use crate::refusal::{EditRefusal, RereadFailure};
use crate::reread::Candidate;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ListMove {
    block: SrcRange,
    kind: BlockKind,
    item: SrcOffset,
    parent: Option<SrcOffset>,
    before_indent: usize,
    after_indent: usize,
    leading_blank_bytes: usize,
}

impl ListMove {
    pub(super) fn with_leading_blank(mut self, bytes: usize) -> Self {
        self.leading_blank_bytes = bytes;
        self
    }
}

pub(super) fn list_move(
    parsed: &Parsed,
    block: &Block,
    item: &PresentationNode,
    parent: Option<&PresentationNode>,
    before_indent: usize,
    after_indent: usize,
) -> Result<ListMove, EditRefusal> {
    let anchor = |node: &PresentationNode| {
        parsed
            .snapshot()
            .src_offset(node.source_range().byte_range().start)
            .map_err(map_buffer_error)
    };
    Ok(ListMove {
        block: block.range().clone(),
        kind: block.kind(),
        item: anchor(item)?,
        parent: parent.map(anchor).transpose()?,
        before_indent,
        after_indent,
        leading_blank_bytes: 0,
    })
}

pub(crate) fn verify_move(
    expectation: &ListMove,
    before: &Parsed,
    candidate: &Candidate,
    patches: &[(Range<usize>, String)],
    budget: EditBudget,
) -> Result<(), EditRefusal> {
    let snapshot = before.snapshot();
    let block = snapshot
        .validate_range(&expectation.block)
        .map_err(map_buffer_error)?;
    let item = snapshot
        .validate_offset(&expectation.item)
        .map_err(map_buffer_error)?;
    let parent = expectation
        .parent
        .as_ref()
        .map(|parent| snapshot.validate_offset(parent).map_err(map_buffer_error))
        .transpose()?;
    crate::block_commands::verify(
        before,
        candidate,
        patches,
        block.start,
        Some(expectation.kind),
        1,
    )?;
    let [(patch, replacement)] = patches else {
        return Err(EditRefusal::InvalidPlan);
    };
    let original = snapshot
        .slice(
            &snapshot
                .src_range(patch.clone())
                .map_err(map_buffer_error)?,
            budget.text(),
        )
        .map_err(map_buffer_error)?;
    let line_budget = LineIndexBudget::new(original.len().saturating_add(1));
    let original_lines = LineIndex::build(&original, line_budget).map_err(map_buffer_error)?;
    let moved_text = replacement
        .get(expectation.leading_blank_bytes..)
        .ok_or(EditRefusal::InvalidPlan)?;
    let replacement_lines = LineIndex::build(moved_text, line_budget).map_err(map_buffer_error)?;
    if original_lines.line_count() != replacement_lines.line_count() {
        return Err(changed_hierarchy());
    }
    let transform = |src_off: usize| -> Option<usize> {
        if src_off < patch.start {
            return Some(src_off);
        }
        if src_off >= patch.end {
            return src_off
                .checked_sub(patch.len())?
                .checked_add(replacement.len());
        }
        let relative = src_off.checked_sub(patch.start)?;
        let line = original_lines.line_of_offset(relative)?;
        let column = relative.checked_sub(original_lines.line_range(line)?.start)?;
        patch
            .start
            .checked_add(expectation.leading_blank_bytes)?
            .checked_add(replacement_lines.line_range(line)?.start)?
            .checked_add(
                column
                    .checked_sub(expectation.before_indent)?
                    .checked_add(expectation.after_indent)?,
            )
    };

    let original_anchors = list_anchors(before, block.start)?;
    let candidate_anchors = list_anchors(&candidate.parsed, block.start)?;
    if original_anchors.len() != candidate_anchors.len() {
        return Err(changed_hierarchy());
    }
    for (original, after) in original_anchors.iter().zip(&candidate_anchors) {
        let expected_parent = if original.start == item && *original.kind == PresentationKind::Item
        {
            parent
        } else {
            original.parent
        };
        let expected_parent = expected_parent
            .map(|src_off| transform(src_off).ok_or_else(changed_hierarchy))
            .transpose()?;
        if transform(original.start) != Some(after.start)
            || expected_parent != after.parent
            || original.kind != after.kind
        {
            return Err(changed_hierarchy());
        }
    }
    Ok(())
}

struct ListAnchor<'a> {
    start: usize,
    parent: Option<usize>,
    kind: &'a PresentationKind,
}

fn list_anchors(parsed: &Parsed, start: usize) -> Result<Vec<ListAnchor<'_>>, EditRefusal> {
    let root = parsed
        .presentation()
        .iter()
        .find(|node| node.source_range().byte_range().start == start)
        .ok_or_else(changed_hierarchy)?;
    let mut anchors = Vec::new();
    collect_anchors(root.children(), None, &mut anchors)?;
    Ok(anchors)
}

fn collect_anchors<'a>(
    nodes: &'a [PresentationNode],
    parent: Option<usize>,
    anchors: &mut Vec<ListAnchor<'a>>,
) -> Result<(), EditRefusal> {
    for node in nodes {
        let start = node.source_range().byte_range().start;
        let kind = node.kind();
        let item = *kind == PresentationKind::Item;
        // 同时检查叶节点归属，防止原父项的续接段落被新兄弟项吸收。
        if item || (node.children().is_empty() && *kind != PresentationKind::Paragraph) {
            anchors
                .try_reserve(1)
                .map_err(|_| EditRefusal::Allocation)?;
            anchors.push(ListAnchor {
                start,
                parent,
                kind,
            });
        }
        collect_anchors(
            node.children(),
            if item { Some(start) } else { parent },
            anchors,
        )?;
    }
    Ok(())
}

fn changed_hierarchy() -> EditRefusal {
    EditRefusal::RereadFailed {
        reason: RereadFailure::BlockStructureChanged,
        cause: None,
    }
}
