//! 只读快照到受检变更计划的纯边界，不写真实文档。
//!
//! 本 crate 只负责：绑定工作版本生成计划、复验期望片段、构造候选文本并重新解析、
//! 在候选上核对命令语义与目标选区。任何失败都在真实写入之前返回拒绝，
//! 不会修改快照、草稿、待提交组、组合、选区、撤销/重做或版本状态。
//! 真实缓冲的原子发布仍由文档所有者完成。

mod budget;
mod command;
mod errors;
mod permissions;
mod plan;
mod refusal;
mod reread;
mod validate;

pub use budget::EditBudget;
pub use command::{EditCommand, EditContext, EditMode, GroupReason, InlineFormat, SourceEdit};
pub use plan::{ChangePlan, NoChangeReason, PlanOutcome, Preview, plan_change};
pub use refusal::{BudgetKind, EditRefusal, RereadFailure};
pub use validate::{ValidatedChange, validate_change};
