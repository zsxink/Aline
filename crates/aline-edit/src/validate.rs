//! 计划消费前的完整复验：工作版本、投影、选区、期望片段与候选重读。

use aline_buffer::{CheckedBatch, SelectionSpec, SrcSelection, TextSnapshot};
use aline_markdown::Parsed;

use crate::command::{EditContext, GroupReason};
use crate::errors::{map_batch_error, map_buffer_error};
use crate::permissions::check_visual_permissions;
use crate::plan::{ChangePlan, Preview, ensure_target_selection};
use crate::refusal::EditRefusal;
use crate::reread::{build_candidate, verify};

/// 已按当前状态完整复验、但尚未写入的变更。
#[derive(Debug)]
pub struct ValidatedChange<'a> {
    plan: &'a ChangePlan,
    batch: &'a CheckedBatch,
}

impl<'a> ValidatedChange<'a> {
    /// 返回来源计划。
    pub const fn plan(&self) -> &'a ChangePlan {
        self.plan
    }

    /// 返回与当前快照复验一致的补丁批次。
    pub const fn batch(&self) -> &'a CheckedBatch {
        self.batch
    }

    /// 返回 after 坐标的目标选区。
    pub const fn target_selection(&self) -> SelectionSpec {
        self.plan.target_selection()
    }

    /// 返回本次操作唯一的分组原因。
    pub const fn group_reason(&self) -> GroupReason {
        self.plan.group_reason()
    }

    /// 返回候选源码字节长度。
    pub const fn after_len(&self) -> usize {
        self.plan.after_len()
    }

    /// 返回候选文本只读预览。
    pub fn preview(&self) -> &Preview {
        self.plan.preview()
    }
}

/// 在真实写入前，用当前快照、解析结果和选区复验计划。
///
/// 任一检查失败都返回拒绝，调用方无需做任何回滚；成功结果仍只是可消费凭据，
/// 由文档所有者负责原子发布。
pub fn validate_change<'a>(
    plan: &'a ChangePlan,
    snapshot: &TextSnapshot,
    parsed: &Parsed,
    selection: &SrcSelection,
    context: &EditContext,
) -> Result<ValidatedChange<'a>, EditRefusal> {
    if context.has_pending_input() {
        return Err(EditRefusal::PendingInput);
    }
    if context.has_composition() {
        return Err(EditRefusal::CompositionActive);
    }
    if context.mode() != plan.mode() {
        return Err(EditRefusal::WrongMode {
            expected: plan.mode(),
            actual: context.mode(),
        });
    }
    if snapshot.revision() != plan.before_revision() {
        return Err(EditRefusal::WrongVersion {
            expected: plan.before_revision(),
            actual: snapshot.revision(),
        });
    }
    if snapshot.id() != plan.before_snapshot_id() {
        return Err(EditRefusal::WrongSnapshot);
    }
    if parsed.snapshot().id() != snapshot.id() || parsed.projection().snapshot_id() != snapshot.id()
    {
        return Err(EditRefusal::ProjectionMismatch);
    }
    if parsed.projection().id() != plan.before_projection_id() {
        return Err(EditRefusal::ProjectionMismatch);
    }
    snapshot
        .validate_selection(selection)
        .map_err(map_buffer_error)?;
    if selection != plan.before_selection() {
        return Err(EditRefusal::SelectionMismatch);
    }
    plan.batch().validate(snapshot).map_err(map_batch_error)?;
    for expectation in &plan.expectations {
        let range = snapshot
            .src_range(expectation.range.clone())
            .map_err(map_buffer_error)?;
        let actual = snapshot
            .slice(&range, plan.budget.text())
            .map_err(map_buffer_error)?;
        if actual.as_ref() != expectation.expected {
            return Err(EditRefusal::ExpectedFragmentMismatch {
                range: expectation.range.clone(),
            });
        }
    }
    if plan.mode() == crate::command::EditMode::Visual {
        for expectation in &plan.expectations {
            check_visual_permissions(parsed, &expectation.range)?;
        }
    }

    let candidate = build_candidate(snapshot, plan.parse_config, plan.batch(), plan.budget)?;
    verify(
        plan.contract,
        parsed,
        &candidate,
        plan.batch().patches(),
        plan.budget,
    )?;
    ensure_target_selection(&candidate.snapshot, plan.target_selection())?;

    Ok(ValidatedChange {
        plan,
        batch: plan.batch(),
    })
}
