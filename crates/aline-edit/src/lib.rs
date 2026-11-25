//! 只读快照到受检变更计划的纯边界，不写真实文档。
//!
//! 本 crate 只负责：绑定工作版本生成计划、复验期望片段、构造候选文本并重新解析、
//! 在候选上核对命令语义与目标选区。任何失败都在真实写入之前返回拒绝，
//! 不会修改快照、草稿、待提交组、组合、选区、撤销/重做或版本状态。
//! 真实缓冲的原子发布仍由文档所有者完成。

mod block_commands;
mod budget;
mod command;
mod errors;
mod find;
mod inline;
mod inline_semantics;
mod input;
mod list_commands;
mod markdown_format;
mod permissions;
mod plan;
mod refusal;
mod replan;
mod reread;
mod table;
mod validate;

pub use block_commands::{BlockCommand, ParagraphStyle, plan_block_command, plan_task_toggle};
pub use budget::EditBudget;
pub use command::{EditCommand, EditContext, EditMode, GroupReason, InlineFormat, SourceEdit};
pub use find::{
    FindBudget, FindCase, FindMatch, FindReplacement, FindResults, FindVersion, find_matches,
    plan_find_replace,
};
pub use input::{RawBlockEdit, is_blank_input_anchor, plan_raw_block_input};
pub use markdown_format::plan_markdown_format;
pub use plan::{ChangePlan, NoChangeReason, PlanOutcome, Preview, plan_change};
pub use refusal::{BudgetKind, EditRefusal, RereadFailure};
pub use table::{
    TableCommand, TableNavigation, TableOperation, TableTarget, plan_table_command,
    plan_table_missing_input, table_navigation,
};
pub use validate::{ValidatedChange, validate_change};
