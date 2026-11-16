//! 将已绑定的命令意图重规划到封口输入后的等价候选。调用方持有真实文档、
//! 工作身份与发布能力，此处只读两个快照并返回独立的受检计划。

use aline_buffer::{SrcSelection, TextSnapshot};
use aline_markdown::Parsed;

use crate::command::{EditCommand, EditContext, GroupReason, SourceEdit};
use crate::errors::map_buffer_error;
use crate::plan::{ChangePlan, PlanOutcome, bind_parsed, plan_change};
use crate::refusal::{BudgetKind, EditRefusal};
use crate::validate::validate_binding;

impl ChangePlan {
    /// 在已应用待定输入的只读候选上重新规划独立命令。
    ///
    /// 原解析和选区须为原计划所绑定的逻辑工作输入，包含当时的草稿。
    /// 候选必须提供逐字节相同的源码、同一解析配置和同方向源码选区。
    /// 宿主仍负责复验文档工作身份与查询代次，并原子发布整笔事务。
    /// 该方法不放宽原计划的写前复验。普通输入计划不能作为独立命令重规划。
    pub fn replan_on_equivalent_candidate(
        &self,
        original: &Parsed,
        original_selection: &SrcSelection,
        candidate: &Parsed,
        candidate_selection: &SrcSelection,
        context: &EditContext,
    ) -> Result<PlanOutcome, EditRefusal> {
        validate_binding(
            self,
            original.snapshot(),
            original,
            original_selection,
            context,
        )?;
        let snapshot = candidate.snapshot();
        bind_parsed(snapshot, candidate)?;
        if candidate.config() != self.parse_config {
            return Err(EditRefusal::ProjectionMismatch);
        }
        snapshot
            .validate_selection(candidate_selection)
            .map_err(map_buffer_error)?;
        if original_selection.anchor().byte_offset() != candidate_selection.anchor().byte_offset()
            || original_selection.head().byte_offset() != candidate_selection.head().byte_offset()
        {
            return Err(EditRefusal::SelectionMismatch);
        }
        self.check_equivalent_source(original.snapshot(), snapshot)?;
        if let Some(find) = &self.find_intent {
            return find.replan(candidate, candidate_selection, context, self.budget);
        }
        let command = match self.group_reason() {
            GroupReason::InlineFormat(format) => {
                EditCommand::toggle_inline_format(candidate_selection.range().byte_range(), format)
            }
            GroupReason::RawBlockInput => return Err(EditRefusal::UnsupportedReplan),
            GroupReason::SourceReplace | GroupReason::VisibleReplace | GroupReason::BulkReplace => {
                self.direct_replace_intent()?
            }
        };
        plan_change(
            snapshot,
            candidate,
            candidate_selection,
            context,
            &command,
            self.budget,
        )
    }

    fn check_equivalent_source(
        &self,
        original: &TextSnapshot,
        candidate: &TextSnapshot,
    ) -> Result<(), EditRefusal> {
        let needed = original.len_bytes().max(candidate.len_bytes());
        let limit = self.budget.text().max_bytes();
        if needed > limit {
            return Err(EditRefusal::BudgetExceeded {
                kind: BudgetKind::SourceBytes,
                needed: Some(needed),
                limit: Some(limit),
            });
        }
        if original.len_bytes() != candidate.len_bytes() {
            return Err(EditRefusal::CandidateContentMismatch);
        }
        // 覆盖快照与 Buffer 候选可能有不同分片。按实际字节比较，无需另建全文。
        let original = original.iter_chunks().map_err(map_buffer_error)?;
        let candidate = candidate.iter_chunks().map_err(map_buffer_error)?;
        if !original
            .flat_map(str::bytes)
            .eq(candidate.flat_map(str::bytes))
        {
            return Err(EditRefusal::CandidateContentMismatch);
        }
        Ok(())
    }

    fn direct_replace_intent(&self) -> Result<EditCommand, EditRefusal> {
        let patches = self.batch().patches();
        if patches.len() != self.expectations.len() {
            return Err(EditRefusal::InvalidPlan);
        }
        let mut edits = Vec::new();
        edits
            .try_reserve_exact(patches.len())
            .map_err(|_| EditRefusal::Allocation)?;
        for ((range, replacement), expected) in patches.iter().zip(&self.expectations) {
            if *range != expected.range {
                return Err(EditRefusal::InvalidPlan);
            }
            // 等价检查已证明逻辑坐标未移动。常规计划入口会重读候选的期望片段，
            // 复做保护与语义验证，并为候选创建新的受检批次与 after 选区。
            edits.push(SourceEdit::new(
                range.clone(),
                copy_fragment(&expected.expected)?,
                copy_fragment(replacement)?,
            ));
        }
        Ok(EditCommand::replace(edits))
    }
}

fn copy_fragment(fragment: &str) -> Result<String, EditRefusal> {
    let mut copy = String::new();
    copy.try_reserve_exact(fragment.len())
        .map_err(|_| EditRefusal::Allocation)?;
    copy.push_str(fragment);
    Ok(copy)
}
