//! 启动恢复面板的纯状态机（W4-56）：逐文档恢复/丢弃、无批量入口、
//! 不可恢复项只报告、丢弃必须二次确认。真实渲染/焦点/点击由窗口层负责。

use std::num::NonZeroU64;

use crate::document::{
    RecoveryDiscardCompletion, RecoveryDiscardJob, RecoveryDocumentId, RecoveryGenerationStatus,
    RecoveryLineageDisposition, RecoveryRestoreCompletion, RecoveryRestoreJob, RecoveryScan,
    RecoveryStore, RestoredRecovery, RetireOutcome, TempRoot,
};

/// 单条可恢复记录；只保留逐文档动作所需事实。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct RecoveryPanelEntry {
    pub(super) document: RecoveryDocumentId,
    pub(super) generation: NonZeroU64,
    pub(super) payload_bytes: u64,
    pub(super) status: RecoveryPanelEntryStatus,
}

/// 条目的当前呈现状态；已恢复条目只提供丢弃。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryPanelEntryStatus {
    Available,
    Restored,
}

/// 面板动作；没有任何批量变体。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RecoveryPanelAction {
    Restore { document: RecoveryDocumentId },
    RequestDiscard { document: RecoveryDocumentId },
    ConfirmDiscard,
    CancelDiscard,
}

/// 面板请求的下一步；任务由宿主派生。
pub(super) enum RecoveryPanelStep {
    Ignore,
    Read(RecoveryRestoreJob),
    Discard(RecoveryDiscardJob),
    Recovered(RestoredRecovery),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PendingKind {
    Restore,
    Discard,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PendingOperation {
    document: RecoveryDocumentId,
    operation: u64,
    kind: PendingKind,
    /// 受理时的恢复代次；用户开始打开/关闭/替换或窗口关闭后旧代完成作废。
    epoch: u64,
}

/// 恢复面板的全部可测状态；同一时间只允许一个在途操作。
#[derive(Debug, Default)]
pub(super) struct RecoveryPanelState {
    last_scan: Option<u64>,
    entries: Vec<RecoveryPanelEntry>,
    unrecoverable: usize,
    pending: Option<PendingOperation>,
    confirming: Option<RecoveryDocumentId>,
    feedback: Option<&'static str>,
    next_operation: u64,
    /// 恢复生命周期代次；用户开始打开/关闭/替换或窗口关闭时递增。
    restore_epoch: u64,
}

impl RecoveryPanelState {
    /// 采纳一次扫描；迟到或重复代次不覆盖当前列表。
    pub(super) fn install_scan(&mut self, scan_generation: u64, scan: &RecoveryScan) -> bool {
        if self.last_scan.is_some_and(|last| scan_generation <= last) {
            return false;
        }
        self.last_scan = Some(scan_generation);
        let previous = std::mem::take(&mut self.entries);
        let mut entries = Vec::new();
        for lineage in scan.recoverable_lineages() {
            let Some(latest) = lineage.latest_complete() else {
                continue;
            };
            let status = previous
                .iter()
                .find(|entry| entry.document == lineage.document)
                .map_or(RecoveryPanelEntryStatus::Available, |entry| entry.status);
            entries.push(RecoveryPanelEntry {
                document: lineage.document,
                generation: latest.generation,
                payload_bytes: latest.payload_bytes,
                status,
            });
        }
        entries.sort_by_key(|entry| entry.document);
        self.entries = entries;
        self.unrecoverable = count_unrecoverable(scan);
        self.confirming = None;
        true
    }

    /// 用户开始打开/关闭/替换文档或窗口关闭：在飞恢复立即失效。
    /// 迟到完成随后被忽略，其临时材料由完成信封 Drop 清理；记录保持可恢复。
    pub(super) fn invalidate_restore(&mut self) {
        if self
            .pending
            .is_some_and(|pending| pending.kind == PendingKind::Restore)
        {
            self.pending = None;
        }
        self.restore_epoch = self.restore_epoch.wrapping_add(1);
    }

    /// 有可恢复条目或不可恢复计数时，面板在内容区显示。
    pub(super) fn is_visible(&self) -> bool {
        !self.entries.is_empty() || self.unrecoverable > 0
    }

    pub(super) fn entries(&self) -> &[RecoveryPanelEntry] {
        &self.entries
    }

    pub(super) fn unrecoverable(&self) -> usize {
        self.unrecoverable
    }

    pub(super) fn feedback_key(&self) -> Option<&'static str> {
        self.feedback
    }

    /// 当前处于丢弃确认态的文档；渲染确认提示用。
    pub(super) fn confirming(&self) -> Option<RecoveryDocumentId> {
        self.confirming
    }

    /// 是否有在途恢复/丢弃操作；渲染时禁用按钮。
    pub(super) fn is_busy(&self) -> bool {
        self.pending.is_some()
    }

    /// 处理一次面板动作；Restore 直接生成后台读取任务。
    pub(super) fn request(
        &mut self,
        action: RecoveryPanelAction,
        store: &RecoveryStore,
        temp_root: Option<TempRoot>,
    ) -> RecoveryPanelStep {
        match action {
            RecoveryPanelAction::Restore { document } => {
                if self.pending.is_some() {
                    return RecoveryPanelStep::Ignore;
                }
                let Some(entry) = self.entry(document) else {
                    return RecoveryPanelStep::Ignore;
                };
                if entry.status != RecoveryPanelEntryStatus::Available {
                    return RecoveryPanelStep::Ignore;
                }
                let generation = entry.generation;
                let operation = self.next_operation();
                self.pending = Some(PendingOperation {
                    document,
                    operation,
                    kind: PendingKind::Restore,
                    epoch: self.restore_epoch,
                });
                self.confirming = None;
                self.feedback = None;
                RecoveryPanelStep::Read(RecoveryRestoreJob::new(
                    store.clone(),
                    document,
                    generation,
                    operation,
                    temp_root,
                ))
            }
            RecoveryPanelAction::RequestDiscard { document } => {
                if self.pending.is_some() || !self.has_entry(document) {
                    return RecoveryPanelStep::Ignore;
                }
                self.confirming = Some(document);
                RecoveryPanelStep::Ignore
            }
            RecoveryPanelAction::ConfirmDiscard => self.discard_confirmed(store),
            RecoveryPanelAction::CancelDiscard => {
                self.confirming = None;
                RecoveryPanelStep::Ignore
            }
        }
    }

    /// 丢弃确认后的任务；只有确认态匹配的条目才可进入丢弃。
    pub(super) fn discard_confirmed(&mut self, store: &RecoveryStore) -> RecoveryPanelStep {
        if self.pending.is_some() {
            return RecoveryPanelStep::Ignore;
        }
        let Some(document) = self.confirming.take() else {
            return RecoveryPanelStep::Ignore;
        };
        if !self.has_entry(document) {
            return RecoveryPanelStep::Ignore;
        }
        let operation = self.next_operation();
        self.pending = Some(PendingOperation {
            document,
            operation,
            kind: PendingKind::Discard,
            epoch: self.restore_epoch,
        });
        self.feedback = None;
        RecoveryPanelStep::Discard(RecoveryDiscardJob::new(store.clone(), document, operation))
    }

    /// 结算后台恢复；迟到/旧令牌完成被忽略且不改变在途状态。
    pub(super) fn restore_finished(
        &mut self,
        completion: RecoveryRestoreCompletion,
    ) -> RecoveryPanelStep {
        let Some(pending) = self.pending else {
            return RecoveryPanelStep::Ignore;
        };
        if pending.kind != PendingKind::Restore
            || pending.document != completion.document
            || pending.operation != completion.operation
            || pending.epoch != self.restore_epoch
        {
            return RecoveryPanelStep::Ignore;
        }
        self.pending = None;
        match completion.result {
            Ok(restored) => {
                if let Some(entry) = self.entry_mut(completion.document) {
                    entry.status = RecoveryPanelEntryStatus::Restored;
                }
                self.feedback = Some("document.recovery_restored");
                RecoveryPanelStep::Recovered(restored)
            }
            Err(_) => {
                self.feedback = Some("document.recovery_restore_failed");
                RecoveryPanelStep::Ignore
            }
        }
    }

    /// 结算后台丢弃；只有成功退役才移除条目，其余保留并报告。
    pub(super) fn discard_finished(&mut self, completion: RecoveryDiscardCompletion) {
        let Some(pending) = self.pending else {
            return;
        };
        if pending.kind != PendingKind::Discard
            || pending.document != completion.document
            || pending.operation != completion.operation
        {
            return;
        }
        self.pending = None;
        match completion.result {
            Ok(RetireOutcome::Retired { .. } | RetireOutcome::AlreadyRetired { .. }) => {
                self.entries
                    .retain(|entry| entry.document != completion.document);
                if self.confirming == Some(completion.document) {
                    self.confirming = None;
                }
                self.feedback = None;
            }
            Ok(RetireOutcome::Deferred { .. }) | Err(_) => {
                self.feedback = Some("document.recovery_discard_failed");
            }
        }
    }

    /// 恢复文档已采纳后标记条目；旧谱系不会自动退役。
    pub(super) fn mark_restored(&mut self, document: RecoveryDocumentId) {
        if let Some(entry) = self.entry_mut(document) {
            entry.status = RecoveryPanelEntryStatus::Restored;
        }
    }

    /// 后台读取成功但内存文档构造/准备失败：条目回到可用并报告失败。
    pub(super) fn restore_open_failed(&mut self, document: RecoveryDocumentId) {
        if let Some(entry) = self.entry_mut(document) {
            entry.status = RecoveryPanelEntryStatus::Available;
        }
        self.feedback = Some("document.recovery_restore_failed");
    }

    fn next_operation(&mut self) -> u64 {
        let operation = self.next_operation;
        self.next_operation = self.next_operation.wrapping_add(1);
        operation
    }

    fn entry(&self, document: RecoveryDocumentId) -> Option<&RecoveryPanelEntry> {
        self.entries.iter().find(|entry| entry.document == document)
    }

    fn entry_mut(&mut self, document: RecoveryDocumentId) -> Option<&mut RecoveryPanelEntry> {
        self.entries
            .iter_mut()
            .find(|entry| entry.document == document)
    }

    fn has_entry(&self, document: RecoveryDocumentId) -> bool {
        self.entry(document).is_some()
    }
}

/// 恢复面板仅在无文档接管内容区时显示；文档打开期间只保留状态。
pub(super) const fn recovery_panel_visible(document_open: bool, panel_visible: bool) -> bool {
    !document_open && panel_visible
}

/// 统计无法进入恢复列表的记录：不可恢复谱系每条计一，可恢复谱系中的
/// 损坏/未知格式代也逐代报告。
fn count_unrecoverable(scan: &RecoveryScan) -> usize {
    let mut count = 0;
    for lineage in &scan.lineages {
        let recoverable = lineage.disposition == RecoveryLineageDisposition::Active
            && lineage.latest_complete().is_some();
        if !recoverable {
            count += 1;
            continue;
        }
        count += lineage
            .generations
            .iter()
            .filter(|generation| {
                matches!(
                    generation.status,
                    RecoveryGenerationStatus::Corrupt | RecoveryGenerationStatus::UnknownFormat
                )
            })
            .count();
    }
    count
}
