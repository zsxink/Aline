//! 已冻结保存的后台执行包；按固定顺序观察、复核、编码并执行同目录受检替换。
//! 本模块只构造完成项交回会话结算，不读取编辑历史或修改文档状态。

use std::path::PathBuf;

use aline_buffer::TextBudget;
use aline_source::fs::{
    FsError, create_staging, discard_staging, file_identity, publish_staged_new,
};
use aline_source::{
    AtomicFailureKind, BaselineError, DiskVersion, ExistingTarget, IoBudget, StorageBudget,
    StorageSnapshot, decode, prepare_checked_replace,
};

use super::{EncodedWrite, FrozenWrite, WriteCompletion, WriteMode, WriteOutcome, WriteStop};

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
    /// Existing 保持原顺序；Create/SaveAs 先观察新目标，不存在则独占发布，
    /// 已存在且允许覆盖则走受检替换，绝不沿用旧目标基线。
    pub(crate) fn run(self) -> WriteCompletion {
        let Self {
            frozen,
            text_budget,
            storage_budget,
            io_budget,
            staging,
        } = self;
        let associate_overwrite = match frozen.mode() {
            WriteMode::Existing => None,
            WriteMode::Associate { overwrite, .. } => Some(*overwrite),
        };
        match associate_overwrite {
            None => run_existing(frozen, text_budget, storage_budget, io_budget, staging),
            Some(overwrite) => run_associate(
                frozen,
                text_budget,
                storage_budget,
                io_budget,
                staging,
                overwrite,
            ),
        }
    }
}

fn run_existing(
    frozen: FrozenWrite,
    text_budget: TextBudget,
    storage_budget: StorageBudget,
    io_budget: IoBudget,
    staging: PathBuf,
) -> WriteCompletion {
    let target = match ExistingTarget::observe(
        frozen.path(),
        frozen.baseline().storage().version(),
        io_budget,
        frozen.cancellation(),
    ) {
        Ok(target) => target,
        // 观察阶段没有发布副作用；冻结输入仍交回会话以保留内存与诊断。
        Err(failure) => {
            return completion(frozen, WriteOutcome::NotPublished(WriteStop::File(failure)));
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

fn run_associate(
    frozen: FrozenWrite,
    text_budget: TextBudget,
    storage_budget: StorageBudget,
    io_budget: IoBudget,
    staging: PathBuf,
    overwrite: bool,
) -> WriteCompletion {
    let observed = match ExistingTarget::observe(
        frozen.path(),
        DiskVersion::initial(),
        io_budget,
        frozen.cancellation(),
    ) {
        Ok(observed) => Some(observed),
        Err(failure) => match failure.kind() {
            AtomicFailureKind::Conflict {
                target_exists: false,
            } => None,
            _ => {
                return completion(frozen, WriteOutcome::NotPublished(WriteStop::File(failure)));
            }
        },
    };
    let Some(observed) = observed else {
        return run_create(frozen, text_budget, storage_budget, staging);
    };
    if !overwrite {
        return completion(frozen, WriteOutcome::NotPublished(WriteStop::Conflict));
    }
    let encoded = match frozen.encode(text_budget, storage_budget) {
        Ok(encoded) => encoded,
        Err(error) => return frozen.stop_before_write(error),
    };
    let expected_version = observed.storage().version();
    let result = prepare_checked_replace(
        encoded.bytes(),
        &observed,
        &staging,
        io_budget,
        frozen.cancellation(),
    )
    .and_then(|prepared| prepared.publish(frozen.cancellation()));
    frozen.finish_associate(&encoded, expected_version, result)
}

fn run_create(
    frozen: FrozenWrite,
    text_budget: TextBudget,
    storage_budget: StorageBudget,
    _staging: PathBuf,
) -> WriteCompletion {
    let encoded = match frozen.encode(text_budget, storage_budget) {
        Ok(encoded) => encoded,
        Err(error) => return frozen.stop_before_write(error),
    };
    let Some(parent) = frozen.path().parent() else {
        return completion(
            frozen,
            WriteOutcome::NotPublished(WriteStop::Create(FsError::InvalidPath)),
        );
    };
    let staged = match create_staging(parent, ".aline-save", encoded.bytes().bytes()) {
        Ok(staged) => staged,
        Err(error) => {
            return completion(frozen, WriteOutcome::NotPublished(WriteStop::Create(error)));
        }
    };
    let identity = staged.identity();
    let published = publish_staged_new(&staged, frozen.path());
    let _ = discard_staging(staged);
    match published {
        Ok(_) => finish_new(frozen, &encoded, true),
        Err(error) => {
            let appeared = file_identity(frozen.path()).is_ok_and(|current| current == identity)
                && std::fs::read(frozen.path())
                    .is_ok_and(|bytes| bytes.as_slice() == encoded.bytes().bytes());
            if appeared {
                // 目标已按本任务 staging 发布，只有目录同步等耐久性未确认。
                finish_new(frozen, &encoded, false)
            } else if matches!(error, FsError::AlreadyExists) {
                completion(frozen, WriteOutcome::NotPublished(WriteStop::Conflict))
            } else {
                completion(frozen, WriteOutcome::NotPublished(WriteStop::Create(error)))
            }
        }
    }
}

fn finish_new(frozen: FrozenWrite, encoded: &EncodedWrite, synchronized: bool) -> WriteCompletion {
    let budget = StorageBudget::new(encoded.bytes().bytes().len());
    let baseline = StorageSnapshot::new(
        encoded.bytes().bytes().to_vec(),
        DiskVersion::initial(),
        budget,
    )
    .map_err(BaselineError::from)
    .and_then(|storage| {
        decode(&storage, budget)
            .map(|decoded| decoded.into_baseline())
            .map_err(BaselineError::from)
    });
    let outcome = match baseline {
        Ok(storage) => WriteOutcome::Published {
            storage,
            synchronized,
        },
        Err(error) => WriteOutcome::PublishedNewBaselineFailed {
            path: frozen.path().to_path_buf(),
            error,
        },
    };
    completion(frozen, outcome)
}

fn completion(frozen: FrozenWrite, outcome: WriteOutcome) -> WriteCompletion {
    WriteCompletion {
        job: frozen,
        outcome,
    }
}
