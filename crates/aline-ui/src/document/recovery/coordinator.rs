//! 恢复会话与两阶段发布：准备任务在后台完成全部整段物化与资源读取，
//! UI 线程只做廉价捕获；发布任务写盘并返回结算。
//!
//! 恢复计时与自动保存互不共享任务或状态（W4-42）；关闭/替换先取消在途任务，
//! 再由后台退役任务清理谱系（W4-53）。本模块不读时钟、不解析路径、不触碰
//! 源文件；所有时间由宿主注入。

// 4.1b 只接会话与任务；部分分类与访问器仅由 crate 内测试或 4.2 UI 消费。
#![cfg_attr(not(test), allow(dead_code))]

use std::collections::HashSet;
use std::fmt;
use std::num::NonZeroU64;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use aline_buffer::{BufferError, SnapshotId, TextBudget, TextSnapshot};
use aline_source::{Cancellation, SourceBaseline};

use super::record::{
    MAX_GENERATION_RESOURCE_BYTES, MAX_PATH_BYTES, MAX_REFERENCE_BYTES, MAX_RESOURCE_BYTES,
    MAX_RESOURCE_COUNT, MAX_TEXT_BYTES,
};
use super::resource::{RecoveryConfig, RecoveryResourceSource};
use super::schedule::{RecoveryAction, RecoveryScheduler};
use super::store::{
    RecoveryDocumentId, RecoveryGenerationInput, RecoveryPath, RecoveryPublishCause,
    RecoveryPublishOutcome, RecoveryResourceInput, RecoveryStore, RecoveryStoreError,
    RetireOutcome,
};
use crate::document::persistence::saved_equivalent;
use crate::document::{CurrentSnapshot, DocumentId, WorkingVersion};

/// 反馈键：恢复副本写入失败；内容仍在内存中。
pub(crate) const RECOVERY_FAILED_KEY: &str = "document.recovery_failed";
/// 反馈键：恢复副本因引用资源不可用而未发布。
pub(crate) const RECOVERY_INCOMPLETE_KEY: &str = "document.recovery_incomplete";

/// 一次会话身份；clone 保留身份，只有同一 `Arc` 才算同一次会话。
#[derive(Clone, Debug)]
pub(crate) struct RecoverySessionId(Arc<()>);

impl RecoverySessionId {
    /// 建立新身份；不注册、不写盘。
    pub(crate) fn fresh() -> Self {
        Self(Arc::new(()))
    }

    /// 按指针相等判断是否同一次会话。
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// 任务标签：会话身份、文档身份与尝试号共同决定迟到任务是否仍有效。
#[derive(Clone, Debug)]
pub(crate) struct RecoveryJobTag {
    pub(crate) session: RecoverySessionId,
    pub(crate) document_id: DocumentId,
    pub(crate) attempt: u64,
}

/// 会话阶段；只有 Idle 才允许布防与开始新准备。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryPhase {
    Idle,
    Preparing,
    Publishing,
}

/// 一次恢复开始的输出：交给后台准备、交给后台发布，或已就地结算。
// 准备任务按裁决直传，不装箱；恢复是 10 秒低频路径，仅对本枚举豁免大小阈值 lint。
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub(crate) enum RecoveryStart {
    Prepare(RecoveryPrepareJob),
    Publish(RecoveryPublishJob),
    Settled {
        feedback: Option<&'static str>,
    },
    /// 迟到准备已分配谱系但本会话不再采用；由后台退役（W4-53）。
    Retire(RecoveryCloseJob),
}

/// 本次准备被跳过的原因；正常跳过不产生反馈。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoverySkip {
    /// 当前文本与保存基线等价，无需恢复副本。
    Clean,
    /// 与上次成功发布的版本和快照身份都相同。
    Unchanged,
    /// 准备期间版本、快照或路径代次已改变。
    VersionChanged,
    /// 会话忙碌或不可布防。
    Busy,
    /// 存储已停用；会话置为 blocked。
    Disabled,
    /// 准备或发布被取消。
    Cancelled,
}

/// 一次恢复会话；独立于自动保存，只保存计时、阶段与最近成功发布身份。
pub(crate) struct RecoverySession {
    id: RecoverySessionId,
    store: RecoveryStore,
    resources: Arc<dyn RecoveryResourceSource>,
    scheduler: RecoveryScheduler,
    document: Option<RecoveryDocumentId>,
    phase: RecoveryPhase,
    next_attempt: u64,
    attempt: Option<u64>,
    cancellation: Option<Cancellation>,
    last_published: Option<PublishedRecovery>,
    blocked: bool,
}

/// `begin` 的输入：全部来自当前文档的廉价捕获。
pub(crate) struct RecoveryBegin {
    pub(crate) document_id: DocumentId,
    pub(crate) snapshot: CurrentSnapshot,
    pub(crate) captured_composition: bool,
    pub(crate) path: RecoveryPath,
    pub(crate) path_generation: u64,
    pub(crate) logical: Option<TextSnapshot>,
    pub(crate) storage: SourceBaseline,
    pub(crate) text_budget: TextBudget,
}

/// `begin` 的结果：要么交给后台准备，要么已就地结算。
// 准备任务按裁决直传，不装箱；恢复是 10 秒低频路径，仅对本枚举豁免大小阈值 lint。
#[allow(clippy::large_enum_variant)]
pub(crate) enum RecoveryBeginOutcome {
    Prepare(RecoveryPrepareJob),
    Settled(RecoverySettlement),
}

/// 一次恢复结算；宿主只按 `feedback` 决定是否显示反馈。
#[derive(Debug)]
pub(crate) struct RecoverySettlement {
    pub(crate) tag: RecoveryJobTag,
    pub(crate) feedback: Option<&'static str>,
    pub(crate) published: Option<PublishedRecovery>,
    pub(crate) skip: Option<RecoverySkip>,
}

/// 最近一次成功发布的工作版本、快照身份与代次；只用于跳过重复版本。
#[derive(Clone, Debug)]
pub(crate) struct PublishedRecovery {
    pub(crate) version: WorkingVersion,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) generation: NonZeroU64,
    pub(crate) generation_bytes: u64,
}

/// `accept_prepared` 的结果：交给后台发布、已就地结算，或丢弃迟到任务。
pub(crate) enum RecoveryAcceptOutcome {
    Publish(RecoveryPublishJob),
    Settled(RecoverySettlement),
    /// 标签不匹配但已分配谱系；由后台退役，避免活动谱系无限增长。
    Retire(RecoveryCloseJob),
    /// 标签或阶段不匹配；零副作用，宿主直接丢弃。
    Stale,
}

impl RecoverySession {
    /// 用已打开的存储与资源源建立会话；不读时钟、不分配谱系。
    pub(crate) fn new(config: RecoveryConfig) -> Self {
        Self {
            id: RecoverySessionId::fresh(),
            store: config.store,
            resources: config.resources,
            scheduler: RecoveryScheduler::new(),
            document: None,
            phase: RecoveryPhase::Idle,
            next_attempt: 1,
            attempt: None,
            cancellation: None,
            last_published: None,
            blocked: false,
        }
    }

    /// 只读清理上下文：克隆存储与当前谱系身份；未分配谱系时为 `None`。
    pub(crate) fn cleanup_context(&self) -> Option<(RecoveryStore, RecoveryDocumentId)> {
        self.document.map(|document| (self.store.clone(), document))
    }

    /// 读取指定谱系全部保留代的资源引用；只读，异常由调用方 fail closed。
    pub(crate) fn referenced_resources(
        &self,
        document: RecoveryDocumentId,
    ) -> Result<std::collections::BTreeSet<String>, RecoveryStoreError> {
        self.store.referenced_resources(document)
    }

    /// 返回本次会话身份；迟到任务据此判断归属。
    pub(crate) fn id(&self) -> &RecoverySessionId {
        &self.id
    }

    /// 返回会话是否空闲；只有空闲才允许布防。
    pub(crate) fn is_idle(&self) -> bool {
        self.phase == RecoveryPhase::Idle
    }

    /// 返回下一次到期时刻；None 表示没有待办计时。
    pub(crate) fn deadline(&self) -> Option<u64> {
        self.scheduler.deadline()
    }

    /// 仅空闲且未停用时布防；忙碌或停用不重置计时。
    pub(crate) fn arm(&mut self, now: u64) {
        if !self.blocked && self.phase == RecoveryPhase::Idle {
            self.scheduler.arm(now);
        }
    }

    /// 结算到期状态；忙碌或停用恒为 Idle，不消费计时。
    pub(crate) fn poll(&mut self, now: u64) -> RecoveryAction {
        if self.blocked || self.phase != RecoveryPhase::Idle {
            return RecoveryAction::Idle;
        }
        self.scheduler.poll(now)
    }

    /// 取消在途任务并清除计时；阶段保持，由结算或关闭收尾。
    pub(crate) fn cancel(&mut self) {
        if let Some(cancellation) = &self.cancellation {
            cancellation.cancel();
        }
        self.scheduler.clear();
    }

    /// 就地记录一次失败结算并重新布防；返回失败反馈键。
    pub(crate) fn settle_failure(&mut self, now: u64) -> &'static str {
        self.clear_attempt();
        self.arm(now);
        RECOVERY_FAILED_KEY
    }

    /// 开始一次准备；返回 None 表示会话不可开始（不反馈、不布防）。
    pub(crate) fn begin(&mut self, input: RecoveryBegin, now: u64) -> Option<RecoveryBeginOutcome> {
        if self.blocked || self.phase != RecoveryPhase::Idle {
            return None;
        }
        let document_id = input.document_id.clone();
        // 只比较身份，不读文本；同版本同快照身份直接跳过。
        if let Some(published) = &self.last_published
            && published.version == input.snapshot.version()
            && &published.snapshot_id == input.snapshot.snapshot_id()
        {
            self.arm(now);
            return Some(RecoveryBeginOutcome::Settled(RecoverySettlement {
                tag: RecoveryJobTag {
                    session: self.id.clone(),
                    document_id,
                    attempt: self.attempt.unwrap_or(0),
                },
                feedback: None,
                published: None,
                skip: Some(RecoverySkip::Unchanged),
            }));
        }
        let attempt = self.next_attempt;
        let Some(next) = attempt.checked_add(1) else {
            let feedback = Some(self.settle_failure(now));
            return Some(RecoveryBeginOutcome::Settled(RecoverySettlement {
                tag: RecoveryJobTag {
                    session: self.id.clone(),
                    document_id,
                    attempt,
                },
                feedback,
                published: None,
                skip: None,
            }));
        };
        self.next_attempt = next;
        let cancellation = Cancellation::new();
        self.phase = RecoveryPhase::Preparing;
        self.attempt = Some(attempt);
        self.cancellation = Some(cancellation.clone());
        Some(RecoveryBeginOutcome::Prepare(RecoveryPrepareJob {
            tag: RecoveryJobTag {
                session: self.id.clone(),
                document_id,
                attempt,
            },
            store: self.store.clone(),
            resources: self.resources.clone(),
            document: self.document,
            candidate_nanos: candidate_nanos(),
            path: input.path,
            path_generation: input.path_generation,
            captured_composition: input.captured_composition,
            snapshot: input.snapshot,
            logical: input.logical,
            storage: input.storage,
            text_budget: input.text_budget,
            cancellation,
        }))
    }

    /// 复验后台准备结果；版本/快照/路径代次已改变时丢弃本次准备。
    pub(crate) fn accept_prepared(
        &mut self,
        prepared: PreparedRecovery,
        current: &CurrentSnapshot,
        current_path_generation: u64,
        _now: u64,
    ) -> RecoveryAcceptOutcome {
        let tag = prepared.tag().clone();
        if !tag.session.same(&self.id)
            || &tag.document_id != current.document_id()
            || Some(tag.attempt) != self.attempt
        {
            return retire_stale(prepared);
        }
        if self.phase != RecoveryPhase::Preparing {
            // 同一尝试已进入发布：重复结果可能指向正在发布的活动谱系，不得退役。
            return RecoveryAcceptOutcome::Stale;
        }
        match prepared {
            PreparedRecovery::Ready(ready) => {
                let Some(cancellation) = self.cancellation.clone() else {
                    return RecoveryAcceptOutcome::Stale;
                };
                if ready.version != current.version()
                    || &ready.snapshot_id != current.snapshot_id()
                    || ready.path_generation != current_path_generation
                {
                    // 保留本次已分配的谱系供下一轮复用；关闭时再由退役任务清理。
                    self.document = Some(ready.document);
                    self.clear_attempt();
                    return RecoveryAcceptOutcome::Settled(RecoverySettlement {
                        tag,
                        feedback: None,
                        published: None,
                        skip: Some(RecoverySkip::VersionChanged),
                    });
                }
                self.phase = RecoveryPhase::Publishing;
                self.document = Some(ready.document);
                RecoveryAcceptOutcome::Publish(RecoveryPublishJob {
                    tag,
                    store: ready.store,
                    document: ready.document,
                    input: ready.input,
                    version: ready.version,
                    snapshot_id: ready.snapshot_id,
                    cancellation,
                })
            }
            PreparedRecovery::Skipped { skip, .. } => {
                self.clear_attempt();
                RecoveryAcceptOutcome::Settled(RecoverySettlement {
                    tag,
                    feedback: None,
                    published: None,
                    skip: Some(skip),
                })
            }
            PreparedRecovery::Incomplete { .. } => {
                self.clear_attempt();
                RecoveryAcceptOutcome::Settled(RecoverySettlement {
                    tag,
                    feedback: Some(RECOVERY_INCOMPLETE_KEY),
                    published: None,
                    skip: None,
                })
            }
            PreparedRecovery::Failed { .. } => {
                self.clear_attempt();
                RecoveryAcceptOutcome::Settled(RecoverySettlement {
                    tag,
                    feedback: Some(RECOVERY_FAILED_KEY),
                    published: None,
                    skip: None,
                })
            }
            PreparedRecovery::Cancelled { .. } => {
                self.clear_attempt();
                RecoveryAcceptOutcome::Settled(RecoverySettlement {
                    tag,
                    feedback: None,
                    published: None,
                    skip: Some(RecoverySkip::Cancelled),
                })
            }
        }
    }

    /// 结算一次发布；标签不匹配时零副作用。成功结算后从当前时刻重新布防。
    pub(crate) fn settle(
        &mut self,
        settlement: RecoverySettlement,
        now: u64,
    ) -> Option<&'static str> {
        if !settlement.tag.session.same(&self.id) || Some(settlement.tag.attempt) != self.attempt {
            return None;
        }
        self.clear_attempt();
        if let Some(published) = settlement.published {
            self.last_published = Some(published);
        }
        if settlement.skip == Some(RecoverySkip::Disabled) {
            self.blocked = true;
            self.scheduler.clear();
        } else {
            self.arm(now);
        }
        settlement.feedback
    }

    /// 取消在途任务并取走关闭退役任务；无谱系时返回 None。
    pub(crate) fn into_close_job(self) -> Option<RecoveryCloseJob> {
        if let Some(cancellation) = &self.cancellation {
            cancellation.cancel();
        }
        let document = self.document?;
        Some(RecoveryCloseJob {
            store: self.store,
            document,
        })
    }

    /// 清空本次尝试的三项状态；不改计时与最近成功发布身份。
    fn clear_attempt(&mut self) {
        self.phase = RecoveryPhase::Idle;
        self.attempt = None;
        self.cancellation = None;
    }
}

/// 关闭/替换后的后台退役任务；失败保留谱系供下次扫描（W4-53）。
pub(crate) struct RecoveryCloseJob {
    store: RecoveryStore,
    document: RecoveryDocumentId,
}

impl fmt::Debug for RecoveryCloseJob {
    /// 只打印谱系身份；存储根本身不进入调试输出。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveryCloseJob")
            .field("document", &self.document)
            .finish_non_exhaustive()
    }
}

/// 标签不匹配时收尾迟到准备：已分配谱系则返回退役任务，否则零副作用。
fn retire_stale(prepared: PreparedRecovery) -> RecoveryAcceptOutcome {
    match prepared.into_close_job() {
        Some(job) => RecoveryAcceptOutcome::Retire(job),
        None => RecoveryAcceptOutcome::Stale,
    }
}

impl RecoveryCloseJob {
    /// 在后台退役谱系；任何错误都折算为保留谱系的 `Deferred`。
    pub(crate) fn run(self) -> RetireOutcome {
        match self.store.discard(self.document) {
            Ok(outcome) => outcome,
            Err(_) => RetireOutcome::Deferred {
                cause: RecoveryPublishCause::Uncertain {
                    operation: super::store::RecoveryOperation::Retire,
                },
            },
        }
    }
}

/// 后台准备任务：全部整段物化与资源读取都只在这里执行（W4-54）。
pub(crate) struct RecoveryPrepareJob {
    pub(crate) tag: RecoveryJobTag,
    pub(crate) store: RecoveryStore,
    pub(crate) resources: Arc<dyn RecoveryResourceSource>,
    pub(crate) document: Option<RecoveryDocumentId>,
    pub(crate) candidate_nanos: u64,
    pub(crate) path: RecoveryPath,
    pub(crate) path_generation: u64,
    pub(crate) captured_composition: bool,
    pub(crate) snapshot: CurrentSnapshot,
    pub(crate) logical: Option<TextSnapshot>,
    pub(crate) storage: SourceBaseline,
    pub(crate) text_budget: TextBudget,
    pub(crate) cancellation: Cancellation,
}

impl fmt::Debug for RecoveryPrepareJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveryPrepareJob")
            .field("tag", &self.tag)
            .field("document", &self.document)
            .field("candidate_nanos", &self.candidate_nanos)
            .field("path_generation", &self.path_generation)
            .field("captured_composition", &self.captured_composition)
            .finish_non_exhaustive()
    }
}

/// 后台准备结果；只有 Ready 会进入发布阶段。
// 已物化的一代按裁决直传，不装箱；后台任务每 10 秒至多一次，仅对本枚举豁免阈值 lint。
#[allow(clippy::large_enum_variant)]
pub(crate) enum PreparedRecovery {
    Ready(PreparedReady),
    Skipped {
        tag: RecoveryJobTag,
        skip: RecoverySkip,
    },
    Incomplete {
        tag: RecoveryJobTag,
        reference: String,
    },
    Failed {
        tag: RecoveryJobTag,
    },
    Cancelled {
        tag: RecoveryJobTag,
    },
}

impl PreparedRecovery {
    /// 返回本次准备的任务标签。
    pub(crate) fn tag(&self) -> &RecoveryJobTag {
        match self {
            Self::Ready(ready) => &ready.tag,
            Self::Skipped { tag, .. }
            | Self::Incomplete { tag, .. }
            | Self::Failed { tag }
            | Self::Cancelled { tag } => tag,
        }
    }

    /// 槽已失效（替换/关闭）时取出迟到准备已分配的谱系；无谱系返回 None。
    pub(crate) fn into_close_job(self) -> Option<RecoveryCloseJob> {
        match self {
            Self::Ready(ready) => Some(RecoveryCloseJob {
                store: ready.store,
                document: ready.document,
            }),
            Self::Skipped { .. }
            | Self::Incomplete { .. }
            | Self::Failed { .. }
            | Self::Cancelled { .. } => None,
        }
    }
}

/// 已物化、可交给发布阶段的恢复代次输入。
pub(crate) struct PreparedReady {
    pub(crate) tag: RecoveryJobTag,
    pub(crate) store: RecoveryStore,
    pub(crate) document: RecoveryDocumentId,
    pub(crate) input: RecoveryGenerationInput,
    pub(crate) version: WorkingVersion,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) path_generation: u64,
}

impl RecoveryPrepareJob {
    /// 在后台完成全部整段物化、资源读取与谱系分配；UI 线程不执行本函数。
    pub(crate) fn run(self) -> PreparedRecovery {
        let tag = self.tag.clone();
        if self.cancellation.is_cancelled() {
            return PreparedRecovery::Cancelled { tag };
        }
        // 与保存基线等价时跳过：不分配谱系、不写盘。
        match saved_equivalent(self.logical.as_ref(), &self.storage, self.snapshot.text()) {
            Ok(true) => {
                return PreparedRecovery::Skipped {
                    tag,
                    skip: RecoverySkip::Clean,
                };
            }
            Ok(false) => {}
            Err(_) => return PreparedRecovery::Failed { tag },
        }
        let source = match self.snapshot.full_text(self.text_budget) {
            Ok(source) => source,
            Err(_) => return PreparedRecovery::Failed { tag },
        };
        if source.len() > MAX_TEXT_BYTES {
            return PreparedRecovery::Failed { tag };
        }
        let logical_saved = match self.logical.as_ref() {
            Some(logical) => match text_of(logical, self.text_budget) {
                Ok(text) => Some(text),
                Err(_) => return PreparedRecovery::Failed { tag },
            },
            None => None,
        };
        if logical_saved
            .as_ref()
            .is_some_and(|text| text.len() > MAX_TEXT_BYTES)
        {
            return PreparedRecovery::Failed { tag };
        }
        let baseline = self.storage.storage().bytes().to_vec();
        if baseline.len() > MAX_TEXT_BYTES {
            return PreparedRecovery::Failed { tag };
        }
        let path_len = match &self.path {
            RecoveryPath::Unlinked => 0,
            RecoveryPath::Unix(bytes) | RecoveryPath::WindowsUtf16Le(bytes) => bytes.len(),
        };
        if path_len > MAX_PATH_BYTES {
            return PreparedRecovery::Failed { tag };
        }
        // 资源按引用键去重，首次出现为准。
        let mut seen = HashSet::new();
        let mut unique = Vec::new();
        for resource in self.snapshot.resources() {
            let locator = resource.locator();
            if seen.insert(locator) {
                unique.push(locator.to_owned());
            }
        }
        if unique.len() > MAX_RESOURCE_COUNT {
            return PreparedRecovery::Failed { tag };
        }
        if unique
            .iter()
            .any(|locator| locator.is_empty() || locator.len() > MAX_REFERENCE_BYTES)
        {
            return PreparedRecovery::Failed { tag };
        }
        let mut resources = Vec::with_capacity(unique.len());
        let mut total = 0usize;
        for locator in unique {
            let bytes = match self.resources.read(&locator, MAX_RESOURCE_BYTES) {
                Ok(bytes) => bytes,
                Err(_) => {
                    return PreparedRecovery::Incomplete {
                        tag,
                        reference: locator,
                    };
                }
            };
            let Some(next) = total.checked_add(bytes.len()) else {
                return PreparedRecovery::Incomplete {
                    tag,
                    reference: locator,
                };
            };
            total = next;
            if total > MAX_GENERATION_RESOURCE_BYTES {
                return PreparedRecovery::Incomplete {
                    tag,
                    reference: locator,
                };
            }
            resources.push(RecoveryResourceInput {
                reference: locator,
                bytes,
            });
        }
        if self.cancellation.is_cancelled() {
            return PreparedRecovery::Cancelled { tag };
        }
        let opened = self.document.is_none();
        let document = match self.document {
            Some(document) => document,
            None => match self.store.open_lineage(self.candidate_nanos) {
                Ok(document) => document,
                Err(_) => return PreparedRecovery::Failed { tag },
            },
        };
        // 打开谱系后再次复验取消：关闭可能发生在整段物化中途。新谱系就地退役，
        // 复用谱系若已交给会话，则由关闭任务统一清理（W4-53）。
        if self.cancellation.is_cancelled() {
            if opened {
                let _ = self.store.discard(document);
            }
            return PreparedRecovery::Cancelled { tag };
        }
        let input = RecoveryGenerationInput {
            path_generation: self.path_generation,
            captured_composition: self.captured_composition,
            path: self.path,
            anchor_byte: self.snapshot.selection().anchor().byte_offset(),
            head_byte: self.snapshot.selection().head().byte_offset(),
            source,
            logical_saved,
            baseline: Some(baseline),
            resources,
        };
        PreparedRecovery::Ready(PreparedReady {
            tag,
            store: self.store,
            document,
            input,
            version: self.snapshot.version(),
            snapshot_id: self.snapshot.snapshot_id().clone(),
            path_generation: self.path_generation,
        })
    }
}

/// 后台发布任务：把已物化的一代写盘并返回结算。
pub(crate) struct RecoveryPublishJob {
    pub(crate) tag: RecoveryJobTag,
    pub(crate) store: RecoveryStore,
    pub(crate) document: RecoveryDocumentId,
    pub(crate) input: RecoveryGenerationInput,
    pub(crate) version: WorkingVersion,
    pub(crate) snapshot_id: SnapshotId,
    pub(crate) cancellation: Cancellation,
}

impl fmt::Debug for RecoveryPublishJob {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveryPublishJob")
            .field("tag", &self.tag)
            .field("document", &self.document)
            .finish_non_exhaustive()
    }
}

impl RecoveryPublishJob {
    /// 在后台发布一代恢复记录；映射只使用稳定分类，不记录路径或资源值。
    pub(crate) fn run(self) -> RecoverySettlement {
        let tag = self.tag;
        let settlement = |feedback: Option<&'static str>,
                          published: Option<PublishedRecovery>,
                          skip: Option<RecoverySkip>| RecoverySettlement {
            tag: tag.clone(),
            feedback,
            published,
            skip,
        };
        match self
            .store
            .publish(self.document, self.input, &self.cancellation)
        {
            Ok(RecoveryPublishOutcome::Published {
                generation,
                generation_bytes,
            }) => settlement(
                None,
                Some(PublishedRecovery {
                    version: self.version,
                    snapshot_id: self.snapshot_id,
                    generation,
                    generation_bytes,
                }),
                None,
            ),
            // 不确定发布不写 last_published，下一 10 秒用新代重试。
            Ok(RecoveryPublishOutcome::PublishedUncertain { .. }) => {
                settlement(Some(RECOVERY_FAILED_KEY), None, None)
            }
            Ok(RecoveryPublishOutcome::NotPublished {
                cause: RecoveryPublishCause::Cancelled,
                ..
            }) => settlement(None, None, Some(RecoverySkip::Cancelled)),
            Ok(RecoveryPublishOutcome::NotPublished { .. }) => {
                settlement(Some(RECOVERY_FAILED_KEY), None, None)
            }
            Err(RecoveryStoreError::Disabled(_)) => settlement(
                Some(RECOVERY_FAILED_KEY),
                None,
                Some(RecoverySkip::Disabled),
            ),
            Err(_) => settlement(Some(RECOVERY_FAILED_KEY), None, None),
        }
    }
}

/// 把文本快照整段物化为字符串；受注入预算约束。
fn text_of(snapshot: &TextSnapshot, budget: TextBudget) -> Result<String, BufferError> {
    let range = snapshot.src_range(0..snapshot.len_bytes())?;
    Ok(snapshot.slice(&range, budget)?.into_owned())
}

/// 取当前系统时间纳秒作为谱系候选；0 替换为 1。
fn candidate_nanos() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let nanos = u64::try_from(nanos).unwrap_or(u64::MAX);
    if nanos == 0 { 1 } else { nanos }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 后台任务与结算必须可跨线程移动；资源读取源已声明 `Send + Sync`。
    /// 该断言跨平台运行，早于任何真实存储用例暴露非 `Send` 物化类型（W4-54）。
    #[test]
    fn recovery_prepare_job_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<RecoveryPrepareJob>();
        assert_send::<RecoveryPublishJob>();
        assert_send::<PreparedRecovery>();
        assert_send::<RecoverySettlement>();
        assert_send::<RecoveryCloseJob>();
        assert_send::<RecoveryStart>();
    }
}
