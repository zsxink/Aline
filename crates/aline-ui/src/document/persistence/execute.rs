//! 已冻结保存的后台执行包；按固定顺序观察、复核、编码并执行同目录受检替换。
//! 本模块只构造完成项交回会话结算，不读取编辑历史或修改文档状态。

use std::path::PathBuf;

use aline_buffer::TextBudget;
use aline_source::{ExistingTarget, IoBudget, StorageBudget, prepare_checked_replace};

use super::{FrozenWrite, WriteCompletion, WriteOutcome, WriteStop};

/// 已冻结保存的后台执行包；不借用文档，只读目标并写入 staging。
pub(crate) struct SaveJob {
    frozen: FrozenWrite,
    text_budget: TextBudget,
    storage_budget: StorageBudget,
    io_budget: IoBudget,
    staging: PathBuf,
}

impl SaveJob {
    /// 由会话排队的冻结写入与打开时注入的预算构造；staging 由宿主显式给出。
    pub(in crate::document) fn new(
        frozen: FrozenWrite,
        text_budget: TextBudget,
        storage_budget: StorageBudget,
        io_budget: IoBudget,
        staging: PathBuf,
    ) -> Self {
        Self {
            frozen,
            text_budget,
            storage_budget,
            io_budget,
            staging,
        }
    }

    /// 真实磁盘阶段；任何结果都作为完成项交回会话结算，不吞掉发布事实。
    ///
    /// 固定顺序：观察既有目标 → 复核观察与编码基线 → 编码 → 同目录 staging
    /// 受检替换并发布 → 用实际回执结算。观察失败直接作为未发布完成项返回；
    /// staging 的清理或残留由 source 的替换契约决定，本模块不新增清理路径。
    pub(crate) fn run(self) -> WriteCompletion {
        let Self {
            frozen,
            text_budget,
            storage_budget,
            io_budget,
            staging,
        } = self;
        let target = match ExistingTarget::observe(
            frozen.path(),
            frozen.baseline().storage().version(),
            io_budget,
            frozen.cancellation(),
        ) {
            Ok(target) => target,
            // 观察阶段没有发布副作用；冻结输入仍交回会话以保留内存与诊断。
            Err(failure) => {
                return WriteCompletion {
                    job: frozen,
                    outcome: WriteOutcome::NotPublished(WriteStop::File(failure)),
                };
            }
        };
        if let Err(error) = frozen.check_observation(&target) {
            return frozen.stop_before_write(error);
        }
        let encoded = match frozen.encode(text_budget, storage_budget) {
            Ok(encoded) => encoded,
            Err(error) => return frozen.stop_before_write(error),
        };
        let result = prepare_checked_replace(
            encoded.bytes(),
            &target,
            &staging,
            io_budget,
            frozen.cancellation(),
        )
        .and_then(|prepared| prepared.publish(frozen.cancellation()));
        frozen.finish_replacement(&encoded, result)
    }
}
