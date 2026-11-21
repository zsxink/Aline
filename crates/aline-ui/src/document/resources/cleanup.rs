//! 临时资源清理：只在文档释放后按存活规则删除不再被引用的临时文件。
//! 清理永不触碰资产目录；恢复扫描失败时整体放弃删除（fail closed）。

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use aline_source::fs as platform_fs;

use super::catalog::{TempResource, TempResourceId};
use crate::document::recovery::{RecoveryDocumentId, RecoveryStore};

/// 保留临时资源的原因；只用于诊断与测试断言。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum RetainedReason {
    DocumentOpen,
    Recovery,
    InFlight,
}

/// 纯函数的清理计划；`removable` 只包含可安全删除的临时文件。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct CleanupPlan {
    pub(crate) removable: Vec<TempResourceId>,
    pub(crate) retained: Vec<(TempResourceId, RetainedReason)>,
}

/// 按固定优先级计算存活：文档打开 → 恢复保留 → 在途任务 → 可删除。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn plan_cleanup(
    entries: &[TempResource],
    document_open: bool,
    recovery_references: &BTreeSet<String>,
    in_flight: usize,
) -> CleanupPlan {
    let mut plan = CleanupPlan::default();
    for entry in entries {
        if document_open {
            plan.retained.push((entry.id, RetainedReason::DocumentOpen));
        } else if recovery_references.contains(&entry.locator) {
            plan.retained.push((entry.id, RetainedReason::Recovery));
        } else if in_flight > 0 {
            plan.retained.push((entry.id, RetainedReason::InFlight));
        } else {
            plan.removable.push(entry.id);
        }
    }
    plan
}

/// 清理任务只持有目录、条目与恢复引用上下文；不借用文档。
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct ResourceCleanupJob {
    directory: PathBuf,
    entries: Vec<TempResource>,
    recovery: Option<(RecoveryStore, RecoveryDocumentId)>,
}

/// 清理结果；`PartiallyRetained` 表示至少一个可删除条目失败。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) enum CleanupOutcome {
    Cleaned,
    PartiallyRetained,
    Skipped,
}

#[cfg_attr(not(test), allow(dead_code))]
impl ResourceCleanupJob {
    pub(crate) fn new(
        directory: PathBuf,
        entries: Vec<TempResource>,
        recovery: Option<(RecoveryStore, RecoveryDocumentId)>,
    ) -> Self {
        Self {
            directory,
            entries,
            recovery,
        }
    }

    /// 文档已释放后执行；恢复引用读取失败时一条也不删。
    pub(crate) fn run(self) -> CleanupOutcome {
        let recovery_references = match &self.recovery {
            Some((store, document)) => match store.referenced_resources(*document) {
                Ok(references) => references,
                Err(_) => return CleanupOutcome::Skipped,
            },
            None => BTreeSet::new(),
        };
        let plan = plan_cleanup(&self.entries, false, &recovery_references, 0);
        let mut removed_any = false;
        let mut failed = false;
        for id in plan.removable {
            let Some(entry) = self.entries.iter().find(|entry| entry.id == id) else {
                failed = true;
                continue;
            };
            let path = self.directory.join(&entry.file_name);
            if platform_fs::verify_ordinary_file(&path).is_err() {
                failed = true;
                continue;
            }
            if platform_fs::file_identity(&path).is_err() {
                failed = true;
                continue;
            }
            match fs::remove_file(&path) {
                Ok(()) => removed_any = true,
                Err(_) => failed = true,
            }
        }
        if removed_any && platform_fs::sync_directory(&self.directory).is_err() {
            failed = true;
        }
        if failed {
            CleanupOutcome::PartiallyRetained
        } else {
            CleanupOutcome::Cleaned
        }
    }
}
