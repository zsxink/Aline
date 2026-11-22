//! 原生单窗的打开、编辑动作及文档派生视图。
//!
//! 窗口持有一个文档会话。读取和解析刷新各保留一个在途任务及至多一个最新候选；
//! 文件选择、任务与焦点在本层装配，render 只读已采用的解析。

mod edit_actions;
mod editable_input;
mod find_panel;
mod font_invalidate;
mod geometry;
mod m2_evidence;
mod probe;
mod readonly_content;
mod readonly_evidence;
mod readonly_hit;
mod readonly_layout;
mod readonly_reader;
mod recovery_panel;
mod recovery_runtime;
mod recovery_slot;
mod shell;
mod tokens;

/// 原生窗口同源协调器的无窗口回归；真实 chooser/菜单/焦点另作 macOS 实验。
#[cfg(test)]
mod tests;

use edit_actions::{EditAction, edit_error_key};
use find_panel::FindPanel;
use geometry::content_metrics;
use readonly_content::{NavigationIntent, navigation_intent};
use readonly_hit::{HitError, HitTarget, PreparedLayout};
use readonly_layout::LayoutError;
use readonly_reader::{ContentCopy, ReaderSurface, SourceNavigationTarget};
use recovery_panel::{
    RecoveryPanelAction, RecoveryPanelEntryStatus, RecoveryPanelState, RecoveryPanelStep,
};
use recovery_runtime::{RecoveryRuntime, production_recovery};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use aline_app_core::Translator;
use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
use aline_markdown::{DiagnosticKind, ParseBudget, ParseConfig, Parsed};
use aline_source::StorageBudget;
use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Image, KeyBinding, Menu, MenuItem, MouseButton,
    PathPromptOptions, Role, SharedString, Subscription, Task, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, rgb, rgba, size,
};

use crate::document::{
    AssociationCommand, AssociationOutcome, AssociationStep, AutosaveAction, DocumentStatistics,
    ExternalApplyResult, ExternalCheckCommand, ExternalCheckCompletion, ExternalCheckIntent,
    ExternalCheckJob, ExternalCheckRejection, ExternalConflictView, ExternalDiskState,
    PreparedRecovery, RecoveryAction, RecoveryCloseJob, RecoveryConfig, RecoveryConfigError,
    RecoveryDocumentId, RecoveryPrepareJob, RecoveryPublishJob, RecoveryRestoreCompletion,
    RecoveryRestoreJob, RecoveryScan, RecoverySettlement, RecoveryStart, RecoveryStore,
    ResourceCleanupJob, ResourceMigrationJob, RestoredRecovery, SaveCommand, SaveJob,
    SaveRejection, SaveResult, SaveTargetCommand, StageTimer, StageTiming, TempRoot,
    WriteCompletion, WriteKind,
};
use crate::fonts::{FontEnvironment, FontStatus};
use crate::{
    DocLimits, DocumentId, EditorMode, ModeSwitchOutcome, OpenDisposition, OpenError,
    OpenRequestId, ParseRefreshJob, PublicationKind, PublicationOutcome, ReadonlyDocument,
    ReadonlyOpenCompletion, ReadonlyOpenJob, ReadonlyOpenLimits, ReadonlySession, WaitHandle,
};

gpui::actions!(
    m1_window,
    [
        OpenDocument,
        SaveDocument,
        SaveAs,
        CancelSaveWait,
        LoadDisk,
        ConfirmOverwrite,
        RetryExternalCheck,
        DismissConflict,
        CloseDocument,
        FocusNext,
        FocusPrevious,
        VisualView,
        SourceView,
        ReadPageUp,
        ReadPageDown,
        ReadStart,
        ReadEnd,
        CancelNavigation,
        InvalidateFont
    ]
);

gpui::actions!(
    m2_document,
    [
        Cut, Copy, Paste, PastePlain, SelectAll, Undo, Redo, OpenFind
    ]
);

#[cfg(debug_assertions)]
gpui::actions!(m1_debug, [ReportLayout]);

/// M1 不随包分发字体；V-04 取得授权夹具后再在此登记（字族, 静态 TTF/OTF 字节）。
const BUNDLED_FONTS: &[(&str, &[u8])] = &[];

/// M2 实验预算沿用现有 16 MiB 解析输入上限；不是发布限额。
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;

/// M2 实验预算：可编辑会话的历史节点上限；不是发布限额。
const MAX_HISTORY_NODES: usize = 4096;

/// M2 实验预算：可编辑会话的历史补丁字节上限；不是发布限额。
const MAX_HISTORY_PATCH_BYTES: usize = 1 << 20;

/// 原生窗口预算：M2 起生产打开即可编辑；历史预算为实验值。
pub(super) fn open_limits() -> ReadonlyOpenLimits {
    ReadonlyOpenLimits::new(
        StorageBudget::new(MAX_DOCUMENT_BYTES),
        DocLimits::new(
            TextBudget::new(MAX_DOCUMENT_BYTES),
            LineIndexBudget::new(1_000_000),
            HistoryBudget::new(MAX_HISTORY_NODES, MAX_HISTORY_PATCH_BYTES),
        ),
        ParseConfig::m0(),
        ParseBudget::m0(),
    )
    .with_editing()
}

/// 窗口层保存宿主状态；只记录阶段与等待句柄，不复制文档队列。
#[derive(Clone, Debug)]
enum SaveHost {
    /// 当前没有显式保存请求。
    Idle,
    /// 组合期内等待平台自然结束；句柄用于取消或结算。
    Waiting(WaitHandle),
    /// 目标选择器尚未打开；组合自然结束前仅保存后续意图。
    WaitingTarget {
        handle: WaitHandle,
        request: TargetRequest,
    },
    /// 平台已结束组合，等待窗口延后回调消费选择器资格。
    ReadyTarget(TargetRequest),
    /// 原生位置选择器在途，回执只能由同一请求消费。
    ChoosingTarget(TargetRequest),
    /// 已完成冻结并交给后台执行，等待结算回执。
    Running { automatic: bool },
}

/// 位置选择请求只持实例身份与种类；不复制当前文本或组合状态。
#[derive(Clone, Debug)]
struct TargetRequest {
    document: DocumentId,
    kind: WriteKind,
    token: Rc<()>,
}

impl TargetRequest {
    fn matches(&self, other: &Self) -> bool {
        self.document == other.document && Rc::ptr_eq(&self.token, &other.token)
    }
}

/// 无 GPUI 的窗口协调规则；会话继续独占文档与请求资格。
pub(super) struct NativeOpenCoordinator {
    session: ReadonlySession,
    reading: bool,
    queued: Option<ReadonlyOpenJob>,
    refreshing: bool,
    queued_refresh: Option<ParseRefreshJob>,
    save_host: SaveHost,
    external_checking: bool,
    pending_external_refresh: bool,
    feedback: Option<&'static str>,
    return_focus: bool,
    closed: bool,
    /// 启动恢复面板的纯状态；渲染与任务派生由窗口负责。
    panel: RecoveryPanelState,
    /// 生产恢复存储与临时根；仅在配置成功时存在。
    recovery_store: Option<RecoveryStore>,
    recovery_temp_root: Option<TempRoot>,
    /// 启动扫描代次；0 表示尚未开始，保证每窗口只扫描一次。
    startup_scan_generation: u64,
}

/// 外改检查结算后窗口应执行的副作用；反馈键已选定。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ExternalHostOutcome {
    pub(super) feedback: Option<&'static str>,
    pub(super) refresh: bool,
    pub(super) start_save: bool,
}

/// 保存结算结果；反馈键已按手工/自动入口选定。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct SaveSettlement {
    /// 自动保存成功静默；失败/冲突/不确定保留既有反馈键。
    pub(super) feedback: Option<&'static str>,
}

/// 保存入口派生给窗口的下一步；目标对话框与迁移任务都由窗口装配。
#[allow(clippy::large_enum_variant)]
pub(super) enum SaveIntent {
    /// 后台执行一次写盘。
    Save(SaveJob),
    /// 后台执行一次图片迁移，完成后交回协调器复验。
    Migrate(ResourceMigrationJob),
    /// 打开系统保存位置选择器。
    ChooseTarget(WriteKind),
}

/// 冲突面板展示数据：状态、保存基线、当前编辑、磁盘文本（不可读时为 None）。
type ConflictPanelData = (ExternalDiskState, Arc<str>, Arc<str>, Option<Arc<str>>);

impl NativeOpenCoordinator {
    pub(super) fn new(limits: ReadonlyOpenLimits) -> Self {
        Self {
            session: ReadonlySession::new(limits),
            reading: false,
            queued: None,
            refreshing: false,
            queued_refresh: None,
            save_host: SaveHost::Idle,
            external_checking: false,
            pending_external_refresh: false,
            feedback: None,
            return_focus: false,
            closed: false,
            panel: RecoveryPanelState::default(),
            recovery_store: None,
            recovery_temp_root: None,
            startup_scan_generation: 0,
        }
    }

    pub(super) fn session(&self) -> &ReadonlySession {
        &self.session
    }

    /// 仅供窗口层在原子采纳路径上执行恢复文档构造与准备。
    pub(super) fn session_mut(&mut self) -> &mut ReadonlySession {
        &mut self.session
    }

    pub(super) fn feedback_key(&self) -> Option<&'static str> {
        self.feedback
    }

    pub(super) fn take_focus_return(&mut self) -> bool {
        std::mem::take(&mut self.return_focus)
    }

    /// 请求保存；返回 Some(job) 时窗口负责后台执行。反馈已在内部设置。
    pub(super) fn request_save(&mut self) -> Option<SaveJob> {
        if !matches!(self.save_host, SaveHost::Idle) {
            self.feedback = Some("document.save_busy");
            return None;
        }
        let Some(document) = self.session.document_mut() else {
            self.feedback = Some("document.save_unavailable");
            return None;
        };
        match document.request_save_command() {
            SaveCommand::Run(job) => {
                self.save_host = SaveHost::Running { automatic: false };
                // 成功进入执行即清除旧错误提示，避免过期反馈留在通知区。
                self.feedback = None;
                Some(job)
            }
            SaveCommand::Wait(handle) => {
                self.save_host = SaveHost::Waiting(handle);
                self.feedback = Some("document.save_waiting");
                None
            }
            SaveCommand::Rejected(rejection) => {
                self.feedback = Some(save_rejection_key(rejection));
                None
            }
        }
    }

    /// 自动保存请求；任何拒绝都不写反馈，不进入 ED-01 等待。
    pub(super) fn request_autosave(&mut self) -> Option<SaveJob> {
        if !matches!(self.save_host, SaveHost::Idle) {
            return None;
        }
        let document = self.session.document_mut()?;
        match document.request_save_command() {
            SaveCommand::Run(job) => {
                self.save_host = SaveHost::Running { automatic: true };
                // 与手工保存一致：进入执行即清除旧错误提示。
                self.feedback = None;
                Some(job)
            }
            SaveCommand::Wait(handle) => {
                // 组合已在 poll 前排除；理论不可达，撤掉等待避免协调器卡死。
                let _ = document.cancel_save_wait(handle);
                None
            }
            SaveCommand::Rejected(_) => None,
        }
    }

    /// 取消保存等待或首次关联复制；返回是否确实取消了。反馈已设置。
    pub(super) fn cancel_save_wait(&mut self) -> bool {
        if let SaveHost::WaitingTarget { handle, request } = &self.save_host {
            let handle = *handle;
            let document_id = request.document.clone();
            self.save_host = SaveHost::Idle;
            let result = self
                .session
                .document_mut()
                .filter(|document| document.core().document_id() == &document_id)
                .map(|document| document.edit_core()?.request_cancel_wait(handle));
            self.feedback = Some(if matches!(result, Some(Ok(()))) {
                "document.save_cancelled"
            } else {
                "document.save_failed"
            });
            return matches!(result, Some(Ok(())));
        }
        if matches!(self.save_host, SaveHost::ReadyTarget(_)) {
            self.save_host = SaveHost::Idle;
            self.feedback = Some("document.save_cancelled");
            return true;
        }
        if matches!(self.save_host, SaveHost::Running { .. }) {
            // 迁移/关联写盘复用 `Running` 表达忙态；只有关联活动时可取消，
            // 普通保存仍在后台执行，取消菜单不得声称已取消。
            let Some(document) = self.session.document_mut() else {
                return false;
            };
            if !document.association_active() {
                return false;
            }
            document.cancel_association();
            self.save_host = SaveHost::Idle;
            self.feedback = Some("document.save_cancelled");
            return true;
        }
        let SaveHost::Waiting(handle) = self.save_host else {
            return false;
        };
        // 无论文档结算结果如何都回到 Idle：句柄已不可复用，避免宿主死锁。
        self.save_host = SaveHost::Idle;
        if let Some(document) = self.session.document_mut()
            && document.association_wait_matches(handle)
        {
            document.cancel_association();
            self.feedback = Some("document.save_cancelled");
            return true;
        }
        let result = match self.session.document_mut() {
            Some(document) => document.cancel_save_wait(handle),
            None => Err(SaveRejection::NoTarget),
        };
        match result {
            Ok(()) => {
                self.feedback = Some("document.save_cancelled");
                true
            }
            Err(_) => {
                self.feedback = Some("document.save_failed");
                false
            }
        }
    }

    /// 平台自然结束后按意图返回；关联等待可转入迁移任务。
    pub(super) fn finish_save_wait_intent(
        &mut self,
        outcome: PublicationOutcome,
        now_millis: u64,
    ) -> Option<SaveIntent> {
        if let SaveHost::WaitingTarget { handle, request } = &self.save_host {
            let handle = *handle;
            let request = request.clone();
            if self.session.document().is_some_and(|document| {
                document.core().document_id() == &request.document
                    && document.core().composition().is_some()
            }) {
                return None;
            }
            let result = self
                .session
                .document_mut()
                .filter(|document| document.core().document_id() == &request.document)
                .map(|document| {
                    document
                        .edit_core()?
                        .finish_publication(handle, outcome, now_millis)
                });
            if matches!(result, Some(Ok(_))) {
                let kind = request.kind;
                self.save_host = SaveHost::ReadyTarget(request);
                self.feedback = None;
                return Some(SaveIntent::ChooseTarget(kind));
            }
            self.save_host = SaveHost::Idle;
            self.feedback = Some("document.save_failed");
            return None;
        }
        let SaveHost::Waiting(handle) = self.save_host else {
            return None;
        };
        let association = match self.session.document_mut() {
            Some(document) if document.association_wait_matches(handle) => {
                Some(document.finish_association_wait(handle, outcome, now_millis))
            }
            Some(_) => None,
            None => {
                self.save_host = SaveHost::Idle;
                self.feedback = Some("document.save_failed");
                return None;
            }
        };
        if let Some(command) = association {
            return match command {
                // 关联等待已结束并转入迁移复制：与 Save 分支一致进入运行态，
                // 使复制阶段 `cancel_save_wait` 走 `association_active` 语义。
                AssociationCommand::Run(job) => {
                    self.save_host = SaveHost::Running { automatic: false };
                    self.feedback = None;
                    Some(SaveIntent::Migrate(job))
                }
                AssociationCommand::Save(command) => self.apply_save_command(command),
                AssociationCommand::Wait(_) => {
                    self.save_host = SaveHost::Idle;
                    self.feedback = Some("document.save_failed");
                    None
                }
                AssociationCommand::Rejected(rejection) => {
                    self.save_host = SaveHost::Idle;
                    self.feedback = Some(save_rejection_key(rejection));
                    None
                }
            };
        }
        let Some(document) = self.session.document_mut() else {
            self.save_host = SaveHost::Idle;
            self.feedback = Some("document.save_failed");
            return None;
        };
        match document.finish_save_wait_command(handle, outcome, now_millis) {
            SaveCommand::Run(job) => {
                self.save_host = SaveHost::Running { automatic: false };
                self.feedback = None;
                Some(SaveIntent::Save(job))
            }
            // 自然结束后不可能再次进入等待；按内部失败落回空闲。
            SaveCommand::Wait(_) => {
                self.save_host = SaveHost::Idle;
                self.feedback = Some("document.save_failed");
                None
            }
            SaveCommand::Rejected(rejection) => {
                self.save_host = SaveHost::Idle;
                self.feedback = Some(save_rejection_key(rejection));
                None
            }
        }
    }

    /// 保存结算结果；反馈键已按手工/自动入口选定。
    pub(super) fn settle_save(&mut self, completion: WriteCompletion) -> SaveSettlement {
        let automatic = matches!(self.save_host, SaveHost::Running { automatic: true });
        if matches!(self.save_host, SaveHost::Running { .. }) {
            self.save_host = SaveHost::Idle;
        }
        // 文档已关闭时完成项只能是迟到结果，不产生新反馈。
        let Some(document) = self.session.document_mut() else {
            return SaveSettlement { feedback: None };
        };
        let result = document.settle_save_command(completion);
        if result == SaveResult::Conflict {
            // Ruling W4-31：保存冲突后补一次静默检查，刷新磁盘事实。
            self.pending_external_refresh = true;
        }
        let feedback = save_result_key(result);
        // 自动保存成功静默；失败/冲突/不确定保留既有反馈键。
        let feedback = if automatic && feedback == Some("document.save_saved") {
            None
        } else {
            feedback
        };
        SaveSettlement { feedback }
    }

    /// 是否存在组合期保存等待（菜单取消项可用性）。
    pub(super) fn save_waiting(&self) -> bool {
        matches!(
            self.save_host,
            SaveHost::Waiting(_) | SaveHost::WaitingTarget { .. }
        )
    }

    /// 取消菜单是否可用：组合期等待或首次保存/另存为关联（复制/写盘）活动。
    pub(super) fn save_cancel_enabled(&self) -> bool {
        self.save_waiting()
            || matches!(self.save_host, SaveHost::ReadyTarget(_))
            || self
                .session
                .document()
                .is_some_and(ReadonlyDocument::association_active)
    }

    /// 保存类入口（含另存为）是否应被禁用；关联态同样视为忙碌。
    pub(super) fn save_busy(&self) -> bool {
        !matches!(self.save_host, SaveHost::Idle)
            || self
                .session
                .document()
                .is_some_and(ReadonlyDocument::association_active)
    }

    /// 另存为菜单项可用性：仅可编辑且空闲文档可另存为。
    pub(super) fn save_as_enabled(&self) -> bool {
        self.session
            .document()
            .is_some_and(ReadonlyDocument::is_editable)
            && !self.save_busy()
    }

    /// 计算保存位置选择器参数；无可用初始目录时失败并反馈。
    /// 恢复文档优先用展示原路径的父目录，未命名记录才回退 HOME。
    pub(super) fn save_target_plan(&mut self, home: Option<&Path>) -> Option<(PathBuf, String)> {
        let current = self.session.document().map(|document| {
            document
                .recovered_original_path()
                .cloned()
                .unwrap_or_else(|| document.path().to_path_buf())
        });
        let plan = target_dialog_plan(current.as_deref(), home);
        if plan.is_none() {
            self.feedback = Some("document.save_target_failed");
        }
        plan
    }

    /// 取走关闭/替换后待执行的两类任务：恢复退役在前，资源清理在后。
    pub(super) fn take_retired(&mut self) -> (Vec<RecoveryCloseJob>, Vec<ResourceCleanupJob>) {
        (
            self.take_retired_recoveries(),
            self.session.take_retired_cleanups(),
        )
    }

    /// 装配生产恢复运行时；失败返回 None 并保留反馈键。
    /// 存储句柄同时留在协调器内，供扫描、恢复与丢弃任务克隆使用。
    pub(super) fn install_recovery_runtime(
        &mut self,
        runtime: RecoveryRuntime,
    ) -> Option<RecoveryStore> {
        let RecoveryRuntime {
            config,
            temp_root,
            feedback,
        } = runtime;
        if let Some(key) = feedback {
            self.feedback = Some(key);
        }
        let config = config?;
        let store = config.store.clone();
        if self.session.configure_recovery(Some(config)).is_err() {
            self.feedback = Some("document.recovery_disabled");
            return None;
        }
        self.session.configure_temp_root(temp_root.clone());
        self.recovery_temp_root = temp_root;
        self.recovery_store = Some(store.clone());
        Some(store)
    }

    /// 启动扫描每轮只允许开始一次；失败后由 [`Self::startup_scan_finished`]
    /// 重置代次，允许再次开始。返回本轮扫描代次。
    pub(super) fn begin_startup_scan(&mut self) -> Option<u64> {
        if self.startup_scan_generation != 0 || self.recovery_store.is_none() {
            return None;
        }
        self.startup_scan_generation = 1;
        Some(1)
    }

    /// 采纳启动扫描；代次不符或存储失败都不改动现有面板条目。
    pub(super) fn startup_scan_finished(
        &mut self,
        generation: u64,
        result: Result<RecoveryScan, crate::document::RecoveryStoreError>,
    ) -> bool {
        if self.startup_scan_generation != generation {
            return false;
        }
        match result {
            Ok(scan) => {
                let installed = self.panel.install_scan(generation, &scan);
                if installed {
                    self.feedback = self.panel.feedback_key();
                }
                installed
            }
            Err(_) => {
                // 失败后重置代次，允许后续入口再次 begin_startup_scan；
                // 不在此自动循环重扫。
                self.startup_scan_generation = 0;
                self.feedback = Some("document.recovery_scan_failed");
                false
            }
        }
    }

    /// 手工保存入口：无目标时请求选择器，待重试迁移优先重试。
    ///
    /// 无目标且组合未结束时等待平台自然结束，再打开目标选择器。
    pub(super) fn request_save_intent(&mut self) -> Option<SaveIntent> {
        if !matches!(self.save_host, SaveHost::Idle) {
            self.feedback = Some("document.save_busy");
            return None;
        }
        let Some(document) = self.session.document_mut() else {
            self.feedback = Some("document.save_unavailable");
            return None;
        };
        if document.core().composition().is_some() && document.save_target_required() {
            return self.wait_for_target(WriteKind::Create);
        }
        let command = document.request_save_or_target();
        self.apply_save_target_command(command)
    }

    /// 另存为入口：总是先请求新目标。
    ///
    /// 组合未结束时仅登记可取消等待，平台自然结束后才请求位置选择器。
    pub(super) fn request_save_as_intent(&mut self) -> Option<SaveIntent> {
        if !matches!(self.save_host, SaveHost::Idle) {
            self.feedback = Some("document.save_busy");
            return None;
        }
        let Some(document) = self.session.document_mut() else {
            self.feedback = Some("document.save_unavailable");
            return None;
        };
        if document.core().composition().is_some() {
            return self.wait_for_target(WriteKind::SaveAs);
        }
        let command = document.request_save_as_target();
        self.apply_save_target_command(command)
    }

    fn wait_for_target(&mut self, kind: WriteKind) -> Option<SaveIntent> {
        let document = self.session.document_mut()?;
        let publication = if kind == WriteKind::SaveAs {
            PublicationKind::SaveAs
        } else {
            PublicationKind::Save
        };
        let request = TargetRequest {
            document: document.core().document_id().clone(),
            kind,
            token: Rc::new(()),
        };
        match document
            .edit_core()
            .and_then(|core| core.begin_publication_wait(publication))
        {
            Ok(handle) => {
                self.save_host = SaveHost::WaitingTarget { handle, request };
                self.feedback = Some("document.save_waiting");
            }
            Err(_) => self.feedback = Some("document.save_failed"),
        }
        None
    }

    fn ready_target(&self) -> Option<TargetRequest> {
        match &self.save_host {
            SaveHost::ReadyTarget(request) => Some(request.clone()),
            _ => None,
        }
    }

    fn target_is_current(&self, request: &TargetRequest) -> bool {
        self.session
            .document()
            .is_some_and(|document| document.core().document_id() == &request.document)
    }

    fn ready_target_matches(&self, request: &TargetRequest) -> bool {
        matches!(&self.save_host, SaveHost::ReadyTarget(current) if current.matches(request))
            && self.target_is_current(request)
    }

    /// 在窗口副作用边界消费资格；Ready 请求取消或文档替换后不能打开面板。
    fn begin_target_selection(&mut self, kind: WriteKind) -> Option<TargetRequest> {
        let request = match &self.save_host {
            SaveHost::Idle => TargetRequest {
                document: self.session.document()?.core().document_id().clone(),
                kind,
                token: Rc::new(()),
            },
            SaveHost::ReadyTarget(request)
                if request.kind == kind && self.target_is_current(request) =>
            {
                request.clone()
            }
            _ => return None,
        };
        if let Some(document) = self.session.document_mut()
            && document.core().composition().is_some()
        {
            let publication = if kind == WriteKind::SaveAs {
                PublicationKind::SaveAs
            } else {
                PublicationKind::Save
            };
            match document
                .edit_core()
                .and_then(|core| core.begin_publication_wait(publication))
            {
                Ok(handle) => {
                    self.save_host = SaveHost::WaitingTarget { handle, request };
                    self.feedback = Some("document.save_waiting");
                }
                Err(_) => {
                    self.save_host = SaveHost::Idle;
                    self.feedback = Some("document.save_failed");
                }
            }
            return None;
        }
        self.save_host = SaveHost::ChoosingTarget(request.clone());
        Some(request)
    }

    fn target_selection_matches(&self, request: &TargetRequest) -> bool {
        matches!(&self.save_host, SaveHost::ChoosingTarget(current) if current.matches(request))
            && self.target_is_current(request)
    }

    fn finish_target_selection(
        &mut self,
        request: &TargetRequest,
        selection: Result<Option<PathBuf>, ()>,
        now_millis: u64,
    ) -> Option<SaveIntent> {
        if !matches!(&self.save_host, SaveHost::ChoosingTarget(current) if current.matches(request))
        {
            return None;
        }
        self.save_host = SaveHost::Idle;
        if !self.target_is_current(request) {
            return None;
        }
        self.accept_save_target(request.kind, selection, now_millis)
    }

    fn apply_save_target_command(&mut self, command: SaveTargetCommand) -> Option<SaveIntent> {
        match command {
            SaveTargetCommand::Save(command) => self.apply_save_command(command),
            SaveTargetCommand::ChooseTarget { kind } => Some(SaveIntent::ChooseTarget(kind)),
            SaveTargetCommand::RetryAssociation => self.retry_association_intent(),
        }
    }

    /// 保存命令到宿主意图的唯一映射；反馈在此选定。
    fn apply_save_command(&mut self, command: SaveCommand) -> Option<SaveIntent> {
        match command {
            SaveCommand::Run(job) => {
                self.save_host = SaveHost::Running { automatic: false };
                self.feedback = None;
                Some(SaveIntent::Save(job))
            }
            SaveCommand::Wait(handle) => {
                self.save_host = SaveHost::Waiting(handle);
                self.feedback = Some("document.save_waiting");
                None
            }
            SaveCommand::Rejected(rejection) => {
                self.feedback = Some(save_rejection_key(rejection));
                None
            }
        }
    }

    /// 用保留的迁移材料重试；迁移任务、写盘与失败反馈统一在此选定。
    fn retry_association_intent(&mut self) -> Option<SaveIntent> {
        let Some(document) = self.session.document_mut() else {
            self.feedback = Some("document.save_unavailable");
            return None;
        };
        match document.retry_association() {
            Ok(AssociationStep::Run(job)) => {
                self.save_host = SaveHost::Running { automatic: false };
                self.feedback = None;
                Some(SaveIntent::Migrate(job))
            }
            Ok(AssociationStep::Write(job)) => {
                self.save_host = SaveHost::Running { automatic: false };
                self.feedback = None;
                Some(SaveIntent::Save(job))
            }
            Ok(AssociationStep::Settled(outcome)) => {
                self.save_host = SaveHost::Idle;
                self.feedback = association_outcome_key(outcome);
                None
            }
            Err(rejection) => {
                self.feedback = Some(save_rejection_key(rejection));
                None
            }
        }
    }

    /// 采纳系统选择器结果；取消/失败不创建关联，反馈已设置。
    pub(super) fn accept_save_target(
        &mut self,
        kind: WriteKind,
        selection: Result<Option<PathBuf>, ()>,
        now_millis: u64,
    ) -> Option<SaveIntent> {
        let target = match selection {
            Err(()) => {
                self.feedback = Some("document.save_target_failed");
                return None;
            }
            Ok(None) => {
                self.feedback = Some("document.save_cancelled");
                return None;
            }
            Ok(Some(path)) => path,
        };
        let Some(document) = self.session.document_mut() else {
            self.feedback = Some("document.save_unavailable");
            return None;
        };
        match document.begin_association_save(kind, target, now_millis) {
            AssociationCommand::Run(job) => {
                self.save_host = SaveHost::Running { automatic: false };
                self.feedback = None;
                Some(SaveIntent::Migrate(job))
            }
            AssociationCommand::Wait(handle) => {
                self.save_host = SaveHost::Waiting(handle);
                self.feedback = Some("document.save_waiting");
                None
            }
            AssociationCommand::Save(command) => self.apply_save_command(command),
            AssociationCommand::Rejected(rejection) => {
                self.feedback = Some(save_rejection_key(rejection));
                None
            }
        }
    }

    /// 复验后台迁移结果；写盘任务交回窗口继续执行。
    pub(super) fn association_finished(
        &mut self,
        completion: crate::document::MigrationCompletion,
        now_millis: u64,
    ) -> Option<SaveIntent> {
        let Some(document) = self.session.document_mut() else {
            self.save_host = SaveHost::Idle;
            return None;
        };
        match document.association_prepared(completion, now_millis) {
            AssociationStep::Run(job) => Some(SaveIntent::Migrate(job)),
            AssociationStep::Write(job) => {
                self.save_host = SaveHost::Running { automatic: false };
                self.feedback = None;
                Some(SaveIntent::Save(job))
            }
            AssociationStep::Settled(AssociationOutcome::Stale) => {
                // 取消后或已被新关联取代的迟到完成：不改宿主状态、不覆盖反馈、
                // 不改路径或文档；后续保存可继续使用当前空闲/在途状态。
                None
            }
            AssociationStep::Settled(outcome) => {
                self.save_host = SaveHost::Idle;
                self.feedback = association_outcome_key(outcome);
                None
            }
        }
    }

    /// 首次关联成功后，宿主消费一次恢复布防请求。
    pub(super) fn take_recovery_pending_arm(&mut self) -> bool {
        self.session
            .document_mut()
            .is_some_and(ReadonlyDocument::take_recovery_pending_arm)
    }

    /// 请求外改检查；Some(job) 时窗口后台执行。显式入口设置反馈，Focus 静默。
    pub(super) fn request_external_check(
        &mut self,
        intent: ExternalCheckIntent,
    ) -> Option<ExternalCheckJob> {
        let Some(document) = self.session.document_mut() else {
            if intent != ExternalCheckIntent::Focus {
                self.feedback = Some("document.save_unavailable");
            }
            return None;
        };
        match document.request_external_check(intent) {
            ExternalCheckCommand::Run(job) => {
                self.external_checking = true;
                if intent != ExternalCheckIntent::Focus {
                    self.feedback = None;
                }
                Some(job)
            }
            ExternalCheckCommand::Rejected(rejection) => {
                if intent != ExternalCheckIntent::Focus {
                    self.feedback = Some(external_rejection_key(rejection));
                }
                None
            }
        }
    }

    /// 结算完成项；不改 save_host。
    pub(super) fn external_check_finished(
        &mut self,
        completion: ExternalCheckCompletion,
        now_millis: u64,
    ) -> ExternalHostOutcome {
        self.external_checking = false;
        let intent = completion.intent();
        let Some(document) = self.session.document_mut() else {
            return ExternalHostOutcome::default();
        };
        let applied = document.apply_external_check(completion, now_millis);
        let silent = intent == ExternalCheckIntent::Focus
            && matches!(
                applied,
                ExternalApplyResult::Presented(_)
                    | ExternalApplyResult::Deferred
                    | ExternalApplyResult::Failed
            );
        let outcome = match applied {
            ExternalApplyResult::Reloaded => ExternalHostOutcome {
                feedback: Some("document.external_reloaded"),
                refresh: true,
                start_save: false,
            },
            ExternalApplyResult::Unchanged | ExternalApplyResult::Stale => {
                ExternalHostOutcome::default()
            }
            ExternalApplyResult::OverwriteReady => ExternalHostOutcome {
                start_save: true,
                ..ExternalHostOutcome::default()
            },
            ExternalApplyResult::Presented(state) => ExternalHostOutcome {
                feedback: Some(external_state_key(state)),
                ..ExternalHostOutcome::default()
            },
            ExternalApplyResult::Deferred => ExternalHostOutcome {
                feedback: Some("document.external_deferred"),
                ..ExternalHostOutcome::default()
            },
            ExternalApplyResult::Failed => ExternalHostOutcome {
                feedback: Some("document.external_failed"),
                ..ExternalHostOutcome::default()
            },
        };
        if silent {
            // Ruling W4-33：Focus 静默只覆盖呈现类结果；Reloaded 始终提示来源。
            ExternalHostOutcome {
                feedback: None,
                ..outcome
            }
        } else {
            outcome
        }
    }

    /// 取走“保存冲突后补检查”的待办；窗口在保存结算后消费。
    pub(super) fn take_external_refresh(&mut self) -> bool {
        std::mem::take(&mut self.pending_external_refresh)
    }

    /// 收起冲突呈现；冲突状态与暂停不因此改变。
    pub(super) fn dismiss_external(&mut self) {
        if let Some(document) = self.session.document_mut() {
            document.dismiss_external();
        }
    }

    /// 组合活动时拒绝发起打开：原生面板会夺取焦点并打断组合。
    ///
    /// 返回 true 时只设置 `document.composition_pending`，不改打开请求、文档或焦点。
    pub(super) fn open_blocked_by_composition(&mut self) -> bool {
        let composing = self
            .session
            .document()
            .is_some_and(|document| document.core().composition().is_some());
        if composing {
            self.feedback = Some("document.composition_pending");
        }
        composing
    }

    pub(super) fn request_open(&mut self) -> Result<OpenRequestId, OpenError> {
        // 用户开始打开/替换文档：在飞恢复立即失效，迟到完成不得接管新文档。
        self.panel.invalidate_restore();
        let request = self.session.request_open()?;
        self.invalidate_target_selection();
        self.queued = None;
        self.feedback = None;
        self.return_focus = false;
        Ok(request)
    }

    fn invalidate_target_selection(&mut self) {
        if matches!(self.save_host, SaveHost::WaitingTarget { .. }) {
            let _ = self.cancel_save_wait();
        }
        if matches!(
            self.save_host,
            SaveHost::ReadyTarget(_) | SaveHost::ChoosingTarget(_)
        ) {
            self.save_host = SaveHost::Idle;
        }
    }

    /// 配置下一次打开使用的恢复会话；仅转发，不复制状态。
    /// 生产启动装配直接调用 session；本转发入口当前仅由 crate 内测试驱动。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn configure_recovery(
        &mut self,
        config: Option<RecoveryConfig>,
    ) -> Result<(), RecoveryConfigError> {
        self.session.configure_recovery(config)
    }

    /// 取走关闭/替换后待后台退役的恢复谱系；仅转发。
    pub(super) fn take_retired_recoveries(&mut self) -> Vec<RecoveryCloseJob> {
        self.session.take_retired_recoveries()
    }

    /// 布防恢复并返回下一次到期时刻；无文档或无会话返回 None。
    pub(super) fn arm_recovery(&mut self, now: u64) -> Option<u64> {
        let document = self.session.document_mut()?;
        document.recovery_arm(now);
        document.recovery_deadline()
    }

    /// 结算恢复计时；无文档返回 None。
    pub(super) fn recovery_poll(&mut self, now: u64) -> Option<RecoveryAction> {
        self.session
            .document_mut()
            .and_then(|document| document.recovery_poll(now))
    }

    /// 开始一次恢复准备；无文档或无会话返回 None。
    pub(super) fn recovery_start(&mut self, now: u64) -> Option<RecoveryStart> {
        self.session
            .document_mut()
            .and_then(|document| document.recovery_start(now))
    }

    /// 复验后台准备结果；无文档或无会话返回 None。
    pub(super) fn recovery_prepared(
        &mut self,
        prepared: PreparedRecovery,
        now: u64,
    ) -> Option<RecoveryStart> {
        self.session
            .document_mut()
            .and_then(|document| document.recovery_prepared(prepared, now))
    }

    /// 结算后台发布；外来标签返回 None。
    pub(super) fn recovery_settled(
        &mut self,
        settlement: RecoverySettlement,
        now: u64,
    ) -> Option<&'static str> {
        self.session
            .document_mut()
            .and_then(|document| document.recovery_settled(settlement, now))
    }

    /// 取消在途恢复并清除计时；无文档时零副作用。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn cancel_recovery(&mut self) {
        if let Some(document) = self.session.document_mut() {
            document.cancel_recovery();
        }
    }

    /// 成功与正常跳过保持静默；只有 Some 反馈键才写入通知区。
    pub(super) fn apply_recovery_feedback(&mut self, feedback: Option<&'static str>) {
        if let Some(key) = feedback {
            self.feedback = Some(key);
        }
    }

    pub(super) fn accept_dialog(
        &mut self,
        request: &OpenRequestId,
        selection: Result<Option<Vec<PathBuf>>, ()>,
    ) -> Option<ReadonlyOpenJob> {
        let selection = match selection {
            Ok(selection) => selection,
            Err(()) => {
                if self.session.cancel_open(request) {
                    self.feedback = Some("open_error.dialog");
                    self.return_focus = !self.closed;
                }
                return None;
            }
        };
        match self.session.accept_selection(request, selection) {
            Ok(job) => {
                self.return_focus = !self.closed;
                self.feedback = None;
                self.queued = job;
                self.take_next_read()
            }
            Err(OpenError::StaleRequest | OpenError::ClosedWindow) => None,
            Err(error) => {
                self.feedback = Some(open_error_key(&error));
                self.return_focus = !self.closed;
                None
            }
        }
    }

    fn read_finished<T>(
        &mut self,
        completion: ReadonlyOpenCompletion,
        prepare: impl FnOnce(&ReadonlyDocument) -> Result<T, LayoutError>,
    ) -> (Option<ReadonlyOpenJob>, Option<T>) {
        self.reading = false;
        let result = self.session.accept_open_with(completion, |document| {
            let parsed = document.parsed();
            let (mut mode, reason) = readonly_layout::initial_view(parsed.snapshot().len_bytes());
            if parse_fallback_key(parsed).is_some() {
                mode = EditorMode::Source;
            }
            document
                .switch_mode(mode)
                .map_err(|_| "open_error.internal")?;
            let prepared = prepare(document).map_err(layout_error_key)?;
            Ok((prepared, reason))
        });
        let prepared = match result {
            Ok((_, Some((prepared, reason)))) => {
                self.queued_refresh = None;
                self.feedback = reason;
                Some(prepared)
            }
            Ok((OpenDisposition::Failed(error), None)) => {
                self.feedback = Some(open_error_key(&error));
                None
            }
            Ok((OpenDisposition::RejectedUnresolvedDocument, None)) => {
                self.feedback = Some(self.session.close_blocked_key());
                None
            }
            Err(key) => {
                self.feedback = Some(key);
                None
            }
            Ok((_, None)) => None,
        };
        (self.take_next_read(), prepared)
    }

    fn take_next_read(&mut self) -> Option<ReadonlyOpenJob> {
        if self.reading || self.closed {
            return None;
        }
        let job = self.queued.take()?;
        if job.is_cancelled() {
            return None;
        }
        self.reading = true;
        Some(job)
    }

    fn close_document_with<T>(
        &mut self,
        before_close: impl FnOnce(&ReadonlyDocument) -> T,
    ) -> Result<Option<T>, ()> {
        // 用户开始关闭/替换或窗口关闭：在飞恢复立即失效；即使关闭随后被拒绝，
        // 迟到完成也不得重开文档。
        self.panel.invalidate_restore();
        match self.session.close_document_with(before_close) {
            Ok(value) => {
                self.invalidate_target_selection();
                self.queued = None;
                self.queued_refresh = None;
                self.feedback = None;
                self.return_focus = false;
                Ok(value)
            }
            Err(crate::CloseDisposition::Refused) => {
                // 拒绝原因决定用户能做什么：保存失败可重试，冲突需先处理，
                // 不确定发布只能另存为；不能统一提示为「有未结算编辑」。
                self.feedback = Some(self.session.close_blocked_key());
                Err(())
            }
            Err(crate::CloseDisposition::Closed) => Err(()),
        }
    }

    fn close_document(&mut self) -> bool {
        self.close_document_with(|_| ()).is_ok()
    }

    pub(super) fn close_window(&mut self) -> bool {
        if !self.close_document() {
            return false;
        }
        self.session.close_window();
        self.closed = true;
        true
    }
}

fn open_error_key(error: &OpenError) -> &'static str {
    match error {
        OpenError::SelectionCount => "open_error.selection",
        OpenError::UnsupportedExtension => "open_error.extension",
        OpenError::NotRegularFile => "open_error.not_regular",
        OpenError::BudgetExceeded => "open_error.budget",
        OpenError::Io(_) => "open_error.io",
        OpenError::Decode(_) => "open_error.decode",
        OpenError::ClosedWindow
        | OpenError::RequestExhausted
        | OpenError::StaleRequest
        | OpenError::Cancelled
        | OpenError::InvalidRead
        | OpenError::Storage(_)
        | OpenError::Document(_)
        | OpenError::Parse(_)
        | OpenError::Allocation(_) => "open_error.internal",
    }
}

/// 保存拒绝类别到窗口反馈键的映射。
fn save_rejection_key(rejection: SaveRejection) -> &'static str {
    match rejection {
        SaveRejection::ReadOnly | SaveRejection::NoTarget => "document.save_unavailable",
        SaveRejection::Busy | SaveRejection::Closing => "document.save_busy",
        SaveRejection::Conflict => "document.save_conflict",
        SaveRejection::Uncertain => "document.save_uncertain",
        SaveRejection::Internal => "document.save_failed",
    }
}

/// 关联结算类别到窗口反馈键；成功写盘反馈由保存结算另行给出。
fn association_outcome_key(outcome: AssociationOutcome) -> Option<&'static str> {
    match outcome {
        AssociationOutcome::NeedsRetry => Some("document.save_migration_retry"),
        AssociationOutcome::Failed => Some("document.save_resources_failed"),
        AssociationOutcome::Cancelled => Some("document.save_cancelled"),
        AssociationOutcome::Stale => None,
    }
}

/// 外改检查拒绝类别到窗口反馈键的映射。
fn external_rejection_key(rejection: ExternalCheckRejection) -> &'static str {
    match rejection {
        ExternalCheckRejection::NoTarget => "document.save_unavailable",
        ExternalCheckRejection::Busy => "document.save_busy",
        ExternalCheckRejection::Deferred => "document.external_deferred",
        ExternalCheckRejection::Uncertain => "document.save_uncertain",
    }
}

/// 呈现状态到窗口反馈键的映射。
fn external_state_key(state: ExternalDiskState) -> &'static str {
    match state {
        ExternalDiskState::Changed => "document.save_conflict",
        ExternalDiskState::Missing => "document.external_missing",
        ExternalDiskState::Unreadable | ExternalDiskState::Unstable => {
            "document.external_unreadable"
        }
        ExternalDiskState::Unsupported => "document.external_unsupported",
    }
}

/// 冲突四出口的可用性。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct ConflictExits {
    pub(super) load_disk: bool,
    pub(super) overwrite: bool,
    pub(super) retry: bool,
    pub(super) dismiss: bool,
}

/// 四出口可用性；载入磁盘/覆盖都要求可解码的 Changed 观察（Ruling W4-23 修正）。
pub(super) fn conflict_exits(editable: bool, view: Option<&ExternalConflictView>) -> ConflictExits {
    let loadable =
        view.is_some_and(|view| view.state == ExternalDiskState::Changed && view.disk.is_some());
    ConflictExits {
        load_disk: editable && loadable,
        overwrite: editable && loadable,
        retry: editable && view.is_some(),
        dismiss: view.is_some(),
    }
}

/// 保存结算类别到窗口反馈键；迟到/外来结果不产生反馈。
fn save_result_key(result: SaveResult) -> Option<&'static str> {
    match result {
        SaveResult::Saved => Some("document.save_saved"),
        SaveResult::SavedUncertain => Some("document.save_uncertain"),
        SaveResult::Conflict => Some("document.save_conflict"),
        SaveResult::Failed => Some("document.save_failed"),
        SaveResult::Cancelled => Some("document.save_cancelled"),
        SaveResult::Stale => None,
    }
}

/// 整篇原文回退没有重写映射，原因始终随当前文档显示。
fn parse_fallback_key(parsed: &Parsed) -> Option<&'static str> {
    if parsed.rewrite_map().is_some() {
        return None;
    }
    parsed
        .diagnostics()
        .first()
        .map(|diagnostic| parse_fallback_message(diagnostic.kind()))
}

/// 整源回退的两个原因各自对应一条双语文案。
fn parse_fallback_message(kind: DiagnosticKind) -> &'static str {
    match kind {
        DiagnosticKind::BudgetExceeded => "reading.parse_budget_fallback",
        DiagnosticKind::RawFallback => "reading.parse_raw_fallback",
    }
}

/// 布局失败反馈：预算类提示预算，其余归为内部错误，避免把所有失败都写成预算。
fn layout_error_key(error: LayoutError) -> &'static str {
    match error {
        LayoutError::Budget
        | LayoutError::ItemBytes { .. }
        | LayoutError::ItemNodes { .. }
        | LayoutError::TableColumns { .. } => "open_error.budget",
        LayoutError::InvalidEnvironment
        | LayoutError::InvalidRange
        | LayoutError::StaleInput
        | LayoutError::GenerationExhausted => "open_error.internal",
    }
}

/// 保存选择器完整参数：初始目录与建议名都可用时才返回。
pub(super) fn target_dialog_plan(
    current: Option<&Path>,
    home: Option<&Path>,
) -> Option<(PathBuf, String)> {
    Some((
        target_initial_directory(current, home)?,
        target_suggested_name(current),
    ))
}

/// 保存选择器初始目录：绝对路径父目录优先，其次 HOME；都不可用返回 None。
pub(super) fn target_initial_directory(
    current: Option<&Path>,
    home: Option<&Path>,
) -> Option<PathBuf> {
    current
        .filter(|path| path.is_absolute())
        .and_then(Path::parent)
        .map(Path::to_path_buf)
        .or_else(|| home.map(Path::to_path_buf))
}

/// 保存选择器建议名：当前绝对路径文件名，否则固定 `untitled.md`。
pub(super) fn target_suggested_name(current: Option<&Path>) -> String {
    current
        .filter(|path| path.is_absolute())
        .and_then(Path::file_name)
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "untitled.md".to_owned())
}

/// 取出只读文档冻结的源码文本，作为统计的不可变输入；不含窗口或可变核心。
fn statistics_source(document: &ReadonlyDocument) -> Option<String> {
    let snapshot = document.parsed().snapshot();
    let range = snapshot.src_range(0..snapshot.len_bytes()).ok()?;
    snapshot
        .slice(&range, TextBudget::new(MAX_DOCUMENT_BYTES))
        .ok()
        .map(|text| text.into_owned())
}

pub(super) struct NativeWindowCopy {
    translator: Rc<Translator>,
    title: SharedString,
    empty: SharedString,
    open: SharedString,
    save: SharedString,
    save_as: SharedString,
    cancel_save_wait: SharedString,
    load_disk: SharedString,
    confirm_overwrite: SharedString,
    retry_external_check: SharedString,
    dismiss_conflict: SharedString,
    close: SharedString,
    file_menu: SharedString,
    edit_menu: SharedString,
    find_action: SharedString,
    edit_actions: [SharedString; 7],
    selecting: SharedString,
    loading: SharedString,
    readonly: SharedString,
    visual_view: SharedString,
    source_view: SharedString,
    cancel_navigation: SharedString,
    statistics_pending: SharedString,
    recovery_title: SharedString,
    recovery_restore: SharedString,
    recovery_discard: SharedString,
    recovery_discard_prompt: SharedString,
    recovery_discard_confirm: SharedString,
    recovery_discard_cancel: SharedString,
    recovery_unnamed: SharedString,
    content: ContentCopy,
    errors: BTreeMap<&'static str, SharedString>,
}

impl NativeWindowCopy {
    pub(super) fn new(translator: Translator) -> Result<Self, String> {
        let text = |key| {
            translator
                .t(key)
                .map(SharedString::from)
                .map_err(|error| error.to_string())
        };
        let mut errors = BTreeMap::new();
        for key in [
            "document.close_pending_changes",
            "document.close_save_failed",
            "document.close_save_conflict",
            "document.close_save_uncertain",
            "document.edit_refused",
            "document.input_dropped",
            "document.input_applied_after_failure",
            "document.composition_pending",
            "document.refresh_failed",
            "document.save_saved",
            "document.save_failed",
            "document.save_conflict",
            "document.save_uncertain",
            "document.save_waiting",
            "document.save_cancelled",
            "document.save_busy",
            "document.save_unavailable",
            "document.external_conflict",
            "document.external_missing",
            "document.external_unreadable",
            "document.external_unsupported",
            "document.external_reloaded",
            "document.external_deferred",
            "document.external_failed",
            "document.reload_undo_paused",
            "document.conflict_baseline",
            "document.conflict_local",
            "document.conflict_disk",
            "document.recovery_failed",
            "document.recovery_incomplete",
            "document.save_resources_failed",
            "document.save_migration_retry",
            "document.save_target_failed",
            "document.resource_insert_failed",
            "document.recovery_disabled",
            "document.recovery_restore_failed",
            "document.recovery_discard_failed",
            "document.recovery_scan_failed",
            "document.recovery_restored",
            "find.failed",
            "find.refused",
            "open_error.selection",
            "open_error.extension",
            "open_error.not_regular",
            "open_error.budget",
            "open_error.io",
            "open_error.decode",
            "open_error.internal",
            "open_error.dialog",
            "reading.large_source",
            "reading.parse_budget_fallback",
            "reading.parse_raw_fallback",
            "reading.layout_budget",
            "font.register_failed",
            "font.family_missing",
        ] {
            errors.insert(key, text(key)?);
        }
        Ok(Self {
            title: text("app.name")?,
            empty: text("window.empty")?,
            open: text("action.open")?,
            save: text("action.save")?,
            save_as: text("action.save_as")?,
            cancel_save_wait: text("action.cancel_save_wait")?,
            load_disk: text("action.load_disk")?,
            confirm_overwrite: text("action.confirm_overwrite")?,
            retry_external_check: text("action.retry_external_check")?,
            dismiss_conflict: text("action.dismiss_conflict")?,
            close: text("action.close_document")?,
            file_menu: text("menu.file")?,
            edit_menu: text("menu.edit")?,
            find_action: text("action.find")?,
            edit_actions: [
                text("action.undo")?,
                text("action.redo")?,
                text("action.cut")?,
                text("action.copy")?,
                text("action.paste")?,
                text("action.paste_plain")?,
                text("action.select_all")?,
            ],
            selecting: text("window.selecting")?,
            loading: text("window.loading")?,
            readonly: text("window.readonly")?,
            visual_view: text("action.visual_view")?,
            source_view: text("action.source_view")?,
            cancel_navigation: text("action.cancel_navigation")?,
            statistics_pending: text("status.pending")?,
            recovery_title: text("document.recovery_panel_title")?,
            recovery_restore: text("document.recovery_restore")?,
            recovery_discard: text("document.recovery_discard")?,
            recovery_discard_prompt: text("document.recovery_discard_prompt")?,
            recovery_discard_confirm: text("document.recovery_discard_confirm")?,
            recovery_discard_cancel: text("document.recovery_discard_cancel")?,
            recovery_unnamed: text("document.recovery_unnamed")?,
            content: ContentCopy::new(
                text("content.front_matter")?,
                text("content.footnote")?,
                text("content.toc_empty")?,
                [
                    text("content.alert.note")?,
                    text("content.alert.tip")?,
                    text("content.alert.important")?,
                    text("content.alert.warning")?,
                    text("content.alert.caution")?,
                ],
            ),
            errors,
            translator: Rc::new(translator),
        })
    }

    /// 用插值模板生成状态栏统计标签；键与占位符在 catalog 构造时已校验。
    fn statistics_labels(
        &self,
        statistics: DocumentStatistics,
    ) -> Result<(SharedString, SharedString), String> {
        let characters = statistics.characters().to_string();
        let lines = statistics.lines().to_string();
        let text = |key, count: &str| {
            self.translator
                .t_with(key, &[("count", count)])
                .map(SharedString::from)
                .map_err(|error| error.to_string())
        };
        Ok((
            text("status.characters", &characters)?,
            text("status.lines", &lines)?,
        ))
    }

    /// 恢复记录标题按 `{id}` 现取；缺键或占位符错误时返回 Err。
    fn recovery_record_label(&self, id: u64) -> Result<SharedString, String> {
        let id = id.to_string();
        self.translator
            .t_with("document.recovery_record", &[("id", &id)])
            .map(SharedString::from)
            .map_err(|error| error.to_string())
    }

    /// 不可恢复计数按 `{count}` 现取；缺键或占位符错误时返回 Err。
    fn recovery_unrecoverable_label(&self, count: usize) -> Result<SharedString, String> {
        let count = count.to_string();
        self.translator
            .t_with("document.recovery_unrecoverable", &[("count", &count)])
            .map(SharedString::from)
            .map_err(|error| error.to_string())
    }

    fn install_menu(
        &self,
        cx: &App,
        can_close: bool,
        can_edit: bool,
        can_save_as: bool,
        can_cancel_save: bool,
        exits: ConflictExits,
    ) {
        let [undo, redo, cut, copy, paste, paste_plain, select_all] = &self.edit_actions;
        cx.set_menus([
            Menu::new(self.title.clone()),
            Menu::new(self.file_menu.clone()).items([
                MenuItem::action(self.open.clone(), OpenDocument),
                MenuItem::action(self.save.clone(), SaveDocument).disabled(!can_edit),
                MenuItem::action(self.save_as.clone(), SaveAs).disabled(!can_save_as),
                MenuItem::action(self.close.clone(), CloseDocument).disabled(!can_close),
                MenuItem::separator(),
                MenuItem::action(self.cancel_save_wait.clone(), CancelSaveWait)
                    .disabled(!can_cancel_save),
                MenuItem::separator(),
                MenuItem::action(self.load_disk.clone(), LoadDisk).disabled(!exits.load_disk),
                MenuItem::action(self.confirm_overwrite.clone(), ConfirmOverwrite)
                    .disabled(!exits.overwrite),
                MenuItem::action(self.retry_external_check.clone(), RetryExternalCheck)
                    .disabled(!exits.retry),
                MenuItem::action(self.dismiss_conflict.clone(), DismissConflict)
                    .disabled(!exits.dismiss),
            ]),
            Menu::new(self.edit_menu.clone()).items([
                MenuItem::action(undo.clone(), Undo).disabled(!can_edit),
                MenuItem::action(redo.clone(), Redo).disabled(!can_edit),
                MenuItem::action(cut.clone(), Cut).disabled(!can_edit),
                MenuItem::action(copy.clone(), Copy).disabled(!can_edit),
                MenuItem::action(paste.clone(), Paste).disabled(!can_edit),
                MenuItem::action(paste_plain.clone(), PastePlain).disabled(!can_edit),
                MenuItem::action(select_all.clone(), SelectAll).disabled(!can_edit),
                MenuItem::separator(),
                MenuItem::action(self.find_action.clone(), OpenFind),
            ]),
        ]);
    }
}

pub(crate) fn open_initial_window(
    cx: &mut App,
    copy: NativeWindowCopy,
    locale_fallback: Option<SharedString>,
) -> Result<(), String> {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenDocument, Some("M1Window")),
        KeyBinding::new("cmd-s", SaveDocument, Some("M1Window")),
        KeyBinding::new("cmd-shift-s", SaveAs, Some("M1Window")),
        KeyBinding::new("cmd-.", CancelSaveWait, Some("M1Window")),
        KeyBinding::new("cmd-w", CloseDocument, Some("M1Window")),
        KeyBinding::new("tab", FocusNext, Some("M1Window")),
        KeyBinding::new("shift-tab", FocusPrevious, Some("M1Window")),
        KeyBinding::new("pageup", ReadPageUp, Some("M1Window")),
        KeyBinding::new("pagedown", ReadPageDown, Some("M1Window")),
        KeyBinding::new("home", ReadStart, Some("M1Window")),
        KeyBinding::new("end", ReadEnd, Some("M1Window")),
        KeyBinding::new("escape", CancelNavigation, Some("M1Window")),
        KeyBinding::new("cmd-x", Cut, Some("M2Editor")),
        KeyBinding::new("cmd-c", Copy, Some("M2Editor")),
        KeyBinding::new("cmd-v", Paste, Some("M2Editor")),
        KeyBinding::new("cmd-shift-v", PastePlain, Some("M2Editor")),
        KeyBinding::new("cmd-a", SelectAll, Some("M2Editor")),
        KeyBinding::new("cmd-z", Undo, Some("M2Editor")),
        KeyBinding::new("cmd-shift-z", Redo, Some("M2Editor")),
        KeyBinding::new("cmd-f", OpenFind, Some("M1Window")),
    ]);
    #[cfg(debug_assertions)]
    cx.bind_keys([KeyBinding::new("cmd-alt-l", ReportLayout, Some("M1Window"))]);
    // 字体失效触发只在两个验证环境变量齐全时绑定；生产没有任何快捷键。
    if font_invalidate::FontInvalidation::from_env().is_some() {
        cx.bind_keys([KeyBinding::new(
            "cmd-alt-i",
            InvalidateFont,
            Some("M1Window"),
        )]);
    }
    copy.install_menu(cx, false, false, false, false, ConflictExits::default());
    let bounds = Bounds::centered(None, size(px(960.0), px(640.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            ..WindowOptions::default()
        },
        move |window, cx| {
            window.set_window_title(copy.title.as_ref());
            let entity = cx.new(|cx| NativeDocumentWindow::new(copy, locale_fallback, window, cx));
            let weak = entity.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                weak.update(cx, |this, cx| {
                    let close = this.state.close_window();
                    if !close {
                        cx.notify();
                    }
                    close
                })
                .unwrap_or(true)
            });
            entity
        },
    )
    .map(|_| ())
    .map_err(|error| error.to_string())
}

struct NativeDocumentWindow {
    copy: NativeWindowCopy,
    locale_fallback: Option<SharedString>,
    state: NativeOpenCoordinator,
    focus: FocusHandle,
    brand: Arc<Image>,
    dialog_task: Option<Task<()>>,
    dialog_return_focus: Option<FocusHandle>,
    read_task: Option<Task<()>>,
    save_task: Option<Task<()>>,
    autosave_task: Option<Task<()>>,
    /// 恢复计时槽代次；与 `recovery_task` 一一对应，迟到回调不得清除新任务。
    recovery_slot: recovery_slot::RecoverySlot,
    recovery_task: Option<Task<()>>,
    /// 启动扫描任务；每窗口只派生一次。
    startup_scan_task: Option<Task<()>>,
    /// 启动恢复面板的恢复/丢弃任务；面板保证同时只有一个。
    recovery_panel_task: Option<Task<()>>,
    external_task: Option<Task<()>>,
    activation_subscription: Option<Subscription>,
    parse_task: Option<Task<()>>,
    find_panel: Option<(Entity<FindPanel>, Subscription)>,
    find_return_focus: Option<FocusHandle>,
    find_replace_task: Option<Task<()>>,
    started: Instant,
    loaded_name: Option<SharedString>,
    reader: Option<ReaderSurface>,
    source_input_site: Option<editable_input::SourceInputSite>,
    visual_input_restore: Option<edit_actions::VisualInputRestore>,
    input_frame: Option<editable_input::InputFrame>,
    composing_input_frame: Option<editable_input::InputFrame>,
    input_task: Option<Task<()>>,
    input_scheduler: editable_input::InputScheduler,
    next_reader_generation: u64,
    statistics: Option<(DocumentId, SharedString, SharedString)>,
    statistics_task: Option<(DocumentId, Task<()>)>,
    fonts: FontEnvironment,
    probe: Option<probe::ProbeState>,
    evidence: Option<readonly_evidence::SharedEvidence>,
    /// 显式启用的 M2 状态采样；只在 `ALINE_M2_EVIDENCE_DIR` 存在时创建。
    m2_evidence: Option<m2_evidence::M2Evidence>,
    font_invalidation: Option<font_invalidate::FontInvalidation>,
}

impl NativeDocumentWindow {
    fn new(
        copy: NativeWindowCopy,
        locale_fallback: Option<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        cx.on_release(|this, cx| {
            this.close_with_evidence(cx);
            this.state.close_window();
        })
        .detach();
        let mut fonts = FontEnvironment::new();
        fonts.activate_bundled(cx.text_system().as_ref(), BUNDLED_FONTS);
        let mut state = NativeOpenCoordinator::new(open_limits());
        state.feedback = match fonts.status() {
            FontStatus::RegisterFailed => Some("font.register_failed"),
            FontStatus::FamilyMissing => Some("font.family_missing"),
            FontStatus::System | FontStatus::Active => None,
        };
        let mut state = Self {
            copy,
            locale_fallback,
            state,
            focus,
            brand: shell::brand_icon(),
            dialog_task: None,
            dialog_return_focus: None,
            read_task: None,
            save_task: None,
            autosave_task: None,
            recovery_slot: recovery_slot::RecoverySlot::new(),
            recovery_task: None,
            startup_scan_task: None,
            recovery_panel_task: None,
            external_task: None,
            activation_subscription: None,
            parse_task: None,
            find_panel: None,
            find_return_focus: None,
            find_replace_task: None,
            started: Instant::now(),
            loaded_name: None,
            reader: None,
            source_input_site: None,
            visual_input_restore: None,
            input_frame: None,
            composing_input_frame: None,
            input_task: None,
            input_scheduler: editable_input::InputScheduler::new(),
            next_reader_generation: 0,
            statistics: None,
            statistics_task: None,
            fonts,
            probe: probe::ProbeState::from_env(),
            evidence: readonly_evidence::ReadonlyEvidence::from_env(),
            m2_evidence: m2_evidence::M2Evidence::from_env(),
            font_invalidation: font_invalidate::FontInvalidation::from_env(),
        };
        if state.probe.is_some() {
            cx.spawn_in(window, async move |weak, cx| {
                let mut last = std::time::Instant::now();
                let mut max_gap_ns = 0u128;
                loop {
                    let finished =
                        weak.update_in(cx, |this, window, cx| this.probe_tick(window, cx));
                    let again = matches!(finished, Ok(false));
                    let gap = last.elapsed().as_nanos();
                    last = std::time::Instant::now();
                    max_gap_ns = max_gap_ns.max(gap);
                    if !again {
                        break;
                    }
                    cx.background_executor().timer(probe::PROBE_TICK).await;
                }
                eprintln!(
                    "m1-probe event_loop_gap_ns={max_gap_ns} tick_ns={}",
                    probe::PROBE_TICK.as_nanos()
                );
            })
            .detach();
        }
        // M2 取证只在显式请求时运行：周期采样真实文档状态，不触发任何文档动作。
        if state.m2_evidence.is_some() {
            cx.spawn_in(window, async move |weak, cx| {
                loop {
                    let keep = weak.update_in(cx, |this, _window, _cx| {
                        let Some(evidence) = this.m2_evidence.as_mut() else {
                            return false;
                        };
                        if let Some(document) = this.state.session_mut().document_mut() {
                            evidence.sample(document);
                        }
                        true
                    });
                    if !matches!(keep, Ok(true)) {
                        break;
                    }
                    cx.background_executor()
                        .timer(m2_evidence::SAMPLE_TICK)
                        .await;
                }
            })
            .detach();
        }
        // `observe_window_activation` 的激活只标记 active、不立即回调；注册安全，
        // 且不做初始检查（Ruling W4-30）。
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.check_external(ExternalCheckIntent::Focus, window, cx);
            }
        });
        state.activation_subscription = Some(activation);
        // 生产恢复只接 HOME：配置失败也不影响内存文档与保存。启动只扫描一次。
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let runtime = production_recovery(home.as_deref());
        if let Some(store) = state.state.install_recovery_runtime(runtime)
            && let Some(generation) = state.state.begin_startup_scan()
        {
            let work = cx.background_executor().spawn(async move { store.scan() });
            state.startup_scan_task = Some(cx.spawn_in(window, async move |weak, cx| {
                let result = work.await;
                let _ = weak.update_in(cx, |this, _, cx| {
                    this.startup_scan_task = None;
                    this.state.startup_scan_finished(generation, result);
                    this.update_menu(cx);
                    cx.notify();
                });
            }));
        }
        state
    }

    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_task.is_some() {
            return;
        }
        // 组合未结束时不开原生面板：面板会夺取焦点并打断组合，候选可能丢失。
        // 必须在 `request_open` 之前判定，否则旧请求已被取消、反馈也会被清空。
        if self.state.open_blocked_by_composition() {
            cx.notify();
            return;
        }
        let request = match self.state.request_open() {
            Ok(request) => request,
            Err(error) => {
                self.state.feedback = Some(open_error_key(&error));
                cx.notify();
                return;
            }
        };
        let selection = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(self.copy.open.clone()),
        });
        self.dialog_return_focus = window.focused(cx);
        self.dialog_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let selection = match selection.await {
                Ok(Ok(selection)) => Ok(selection),
                _ => Err(()),
            };
            let _ = weak.update_in(cx, |this, window, cx| {
                this.dialog_task = None;
                let job = this.state.accept_dialog(&request, selection);
                if this.state.take_focus_return() {
                    this.dialog_return_focus
                        .take()
                        .unwrap_or_else(|| this.focus.clone())
                        .focus(window, cx);
                }
                if let Some(job) = job {
                    this.start_read(job, window, cx);
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
        self.update_menu(cx);
        cx.notify();
    }

    fn start_read(&mut self, job: ReadonlyOpenJob, window: &Window, cx: &mut Context<Self>) {
        // 新候选开始读取即作废当前统计：旧文档的数值不得伴随候选读取继续显示。
        self.invalidate_statistics();
        let dispatch_started = StageTimer::start();
        let work = cx.background_executor().spawn(async move {
            let dispatch_wait = dispatch_started.finish();
            let mut completion = job.run();
            completion.timings.dispatch_wait = dispatch_wait;
            (completion, StageTimer::start())
        });
        self.read_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let (mut completion, result_wait) = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                completion.timings.result_wait = result_wait.finish();
                let background = completion.timings;
                let generation = this.next_reader_generation.checked_add(1);
                let mut layout_timing = StageTiming::default();
                let mut adopt_started = None;
                let (next, prepared) = this.state.read_finished(completion, |document| {
                    let generation = generation.ok_or(LayoutError::GenerationExhausted)?;
                    let viewport_width = f32::from(window.viewport_size().width);
                    let metrics = content_metrics(viewport_width, viewport_width);
                    let layout_started = StageTimer::start();
                    let result = ReaderSurface::new(document, generation, this.evidence.clone())
                        .and_then(|mut reader| {
                            reader.prepare(document, window, metrics, &this.fonts)?;
                            Ok((reader, generation))
                        });
                    layout_timing = layout_started.finish();
                    adopt_started = Some(StageTimer::start());
                    result
                });
                let applied = prepared.is_some();
                if applied {
                    // 新文档接管窗口：旧文档的未完成输入、恢复计时与在途结果全部作废。
                    this.clear_input_state();
                    // 先作废旧恢复任务槽，再为新文档布防；旧计时迟到回调随后被拒绝。
                    this.reset_recovery_tasks();
                    this.arm_recovery(window, cx);
                    // 替换前会话已取消旧恢复；退役与清理任务在同一后台链执行。
                    this.drain_retired(cx);
                }
                if let Some((reader, generation)) = prepared {
                    reader.schedule_height_hint(window, cx);
                    this.reader = Some(reader);
                    this.next_reader_generation = generation;
                    this.loaded_name = this
                        .state
                        .session()
                        .document()
                        .and_then(|document| document.path().file_name())
                        .map(|name| SharedString::from(name.to_string_lossy().into_owned()));
                    if let Some(name) = &this.loaded_name {
                        window.set_window_title(name.as_ref());
                    }
                    this.refresh_find(cx);
                }
                let adopt_timing =
                    adopt_started.map_or_else(StageTiming::default, StageTimer::finish);
                if applied
                    && let (Some(evidence), Some(document)) =
                        (&this.evidence, this.state.session().document())
                    && let Err(error) = evidence.borrow_mut().opened(document)
                {
                    eprintln!("m1-readonly-evidence open failed: {error}");
                }
                this.read_task = None;
                this.probe_read_finished(background, layout_timing, adopt_timing, applied);
                match next {
                    Some(job) => this.start_read(job, window, cx),
                    None => this.refresh_statistics(window, cx),
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    fn close_document(&mut self, _: &CloseDocument, window: &mut Window, cx: &mut Context<Self>) {
        if !self.close_with_evidence(cx) {
            cx.notify();
            return;
        }
        self.loaded_name = None;
        self.reader = None;
        self.invalidate_statistics();
        self.refresh_find(cx);
        window.set_window_title(self.copy.title.as_ref());
        self.focus.focus(window, cx);
        self.update_menu(cx);
        cx.notify();
    }

    /// 显式保存：已有目标直接写盘，无目标或待重试迁移走各自入口。
    fn save_document(&mut self, _: &SaveDocument, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(intent) = self.state.request_save_intent() {
            self.run_save_intent(intent, window, cx);
        }
        self.update_menu(cx);
        cx.notify();
    }

    /// 另存为：总是先请求系统选择器；忙碌/只读由协调器反馈。
    fn save_as(&mut self, _: &SaveAs, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(intent) = self.state.request_save_as_intent() {
            self.run_save_intent(intent, window, cx);
        }
        self.update_menu(cx);
        cx.notify();
    }

    /// 保存意图分派：写盘、图片迁移与目标选择器都在宿主派生。
    fn run_save_intent(&mut self, intent: SaveIntent, window: &mut Window, cx: &mut Context<Self>) {
        match intent {
            SaveIntent::Save(job) => self.start_save(job, window, cx),
            SaveIntent::Migrate(job) => self.start_association(job, window, cx),
            SaveIntent::ChooseTarget(kind) => self.begin_target_dialog(kind, window, cx),
        }
    }

    /// 打开保存位置选择器；初始目录与建议名在进入异步前算好。
    fn begin_target_dialog(
        &mut self,
        kind: WriteKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.dialog_task.is_some() {
            return;
        }
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let Some((directory, suggested)) = self.state.save_target_plan(home.as_deref()) else {
            if matches!(self.state.save_host, SaveHost::ReadyTarget(_)) {
                self.state.save_host = SaveHost::Idle;
            }
            self.update_menu(cx);
            cx.notify();
            return;
        };
        let Some(request) = self.state.begin_target_selection(kind) else {
            self.update_menu(cx);
            cx.notify();
            return;
        };
        let selection = cx.prompt_for_new_path(&directory, Some(&suggested));
        let return_focus = window.focused(cx);
        self.dialog_return_focus = return_focus.clone();
        self.dialog_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let selection = match selection.await {
                Ok(Ok(selection)) => Ok(selection),
                _ => Err(()),
            };
            let _ = weak.update_in(cx, |this, window, cx| {
                this.dialog_task = None;
                if !this.state.target_selection_matches(&request) {
                    // 同一旧请求在文档替换后也必须退出 Choosing；新请求不受影响。
                    this.state.finish_target_selection(&request, Err(()), 0);
                    this.dialog_return_focus = None;
                    return;
                }
                let now = this.now_millis();
                if let Some(intent) = this.state.finish_target_selection(&request, selection, now) {
                    this.run_save_intent(intent, window, cx);
                }
                if let Some(focus) = this.dialog_return_focus.take().or(return_focus.clone()) {
                    focus.focus(window, cx);
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
        self.update_menu(cx);
        cx.notify();
    }

    /// 载入磁盘内容；检查结算后按结果刷新派生视图。
    fn load_disk(&mut self, _: &LoadDisk, window: &mut Window, cx: &mut Context<Self>) {
        self.check_external(ExternalCheckIntent::Reload, window, cx);
    }

    /// 确认以磁盘观察为新基线，随后走既有保存发布本地文本。
    fn confirm_overwrite(
        &mut self,
        _: &ConfirmOverwrite,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.check_external(ExternalCheckIntent::Overwrite, window, cx);
    }

    /// 用户显式重试磁盘检查。
    fn retry_external_check(
        &mut self,
        _: &RetryExternalCheck,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.check_external(ExternalCheckIntent::Retry, window, cx);
    }

    /// 收起冲突呈现；冲突状态与暂停保持。
    fn dismiss_conflict(&mut self, _: &DismissConflict, _: &mut Window, cx: &mut Context<Self>) {
        self.state.dismiss_external();
        self.update_menu(cx);
        cx.notify();
    }

    /// 宿主串行执行一次外改检查；Focus 入口静默。
    fn check_external(
        &mut self,
        intent: ExternalCheckIntent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.external_task.is_some() || self.state.external_checking {
            return; // 宿主串行，同一时刻只跑一次检查
        }
        let Some(job) = self.state.request_external_check(intent) else {
            self.update_menu(cx);
            cx.notify();
            return;
        };
        let work = cx.background_executor().spawn(async move { job.run() });
        self.external_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.external_task = None;
                let now_millis =
                    u64::try_from(this.started.elapsed().as_millis()).unwrap_or(u64::MAX);
                let outcome = this.state.external_check_finished(completion, now_millis);
                if let Some(key) = outcome.feedback {
                    this.state.feedback = Some(key);
                }
                if outcome.refresh {
                    this.refresh_after_edit(window, cx);
                }
                if outcome.start_save
                    && let Some(job) = this.state.request_save()
                {
                    this.start_save(job, window, cx);
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    /// 返回窗口启动以来的单调毫秒；饱和转换避免长期运行溢出。
    fn now_millis(&self) -> u64 {
        u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// 记录一次内容活动并确保有且只有一个待办唤醒任务。
    fn note_document_activity(&mut self, window: &Window, cx: &mut Context<Self>) {
        let now = self.now_millis();
        if let Some(document) = self.state.session.document_mut() {
            document.note_document_activity(now);
        } else {
            return;
        }
        self.arm_autosave(window, cx);
    }

    /// 按当前到期时刻布防；已有任务时直接复用（提前唤醒会自行重算剩余时间）。
    fn arm_autosave(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.autosave_task.is_some() {
            return;
        }
        let Some(deadline) = self
            .state
            .session()
            .document()
            .and_then(ReadonlyDocument::autosave_deadline)
        else {
            return;
        };
        let remaining = deadline.saturating_sub(self.now_millis());
        self.spawn_autosave_timer(remaining, window, cx);
    }

    /// 调度一次“叫醒”任务；所有时间判定仍在会话的纯状态机里。
    fn spawn_autosave_timer(&mut self, delay_millis: u64, window: &Window, cx: &mut Context<Self>) {
        self.autosave_task = Some(cx.spawn_in(window, async move |weak, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(delay_millis))
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.autosave_task = None;
                this.autosave_tick(window, cx);
            });
        }));
    }

    /// 一次到期结算：可能发起一次自动保存、重新布防或什么都不做。
    fn autosave_tick(&mut self, window: &Window, cx: &mut Context<Self>) {
        let now = self.now_millis();
        let action = match self.state.session.document_mut() {
            Some(document) => document.autosave_poll(now),
            None => return,
        };
        match action {
            AutosaveAction::Fire => {
                if let Some(job) = self.state.request_autosave() {
                    self.start_save(job, window, cx);
                }
            }
            AutosaveAction::NotDue { remaining_millis } => {
                self.spawn_autosave_timer(remaining_millis, window, cx);
            }
            AutosaveAction::Idle
            | AutosaveAction::DeferredComposition
            | AutosaveAction::Paused
            | AutosaveAction::NoTarget
            | AutosaveAction::NotDirty
            | AutosaveAction::Busy => {}
        }
        self.update_menu(cx);
        cx.notify();
    }

    /// 按当前到期时刻布防恢复；已有计时或工作时直接复用（提前唤醒会自行重算剩余时间）。
    fn arm_recovery(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.recovery_slot.can_arm() {
            return;
        }
        let now = self.now_millis();
        let Some(deadline) = self.state.arm_recovery(now) else {
            return;
        };
        let remaining = deadline.saturating_sub(now);
        self.spawn_recovery_timer(remaining, window, cx);
    }

    /// 替换文档或关闭窗口：作废旧恢复任务槽，迟到完成因代次不匹配被拒绝。
    fn reset_recovery_tasks(&mut self) {
        self.recovery_slot.reset();
        self.recovery_task = None;
    }

    /// 调度一次恢复“叫醒”任务；所有时间判定仍在会话的纯状态机里。
    fn spawn_recovery_timer(&mut self, delay_millis: u64, window: &Window, cx: &mut Context<Self>) {
        let Some(task_id) = self.recovery_slot.begin_timer() else {
            return;
        };
        self.recovery_task = Some(cx.spawn_in(window, async move |weak, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(delay_millis))
                .await;
            let _ = weak.update_in(cx, |this, window, cx| {
                // 槽已被替换或关闭：迟到唤醒不得清除新任务或结算新文档。
                if !this.recovery_slot.finish_timer(task_id) {
                    return;
                }
                this.recovery_task = None;
                this.recovery_tick(window, cx);
            });
        }));
    }

    /// 一次恢复到期结算：可能开始准备、重新布防或什么都不做。
    fn recovery_tick(&mut self, window: &Window, cx: &mut Context<Self>) {
        let now = self.now_millis();
        match self.state.recovery_poll(now) {
            Some(RecoveryAction::Fire) => match self.state.recovery_start(now) {
                Some(RecoveryStart::Prepare(job)) => self.start_recovery_prepare(job, window, cx),
                Some(RecoveryStart::Settled { feedback }) => {
                    self.state.apply_recovery_feedback(feedback);
                    self.arm_recovery(window, cx);
                }
                // 防御分支：正常开始只返回 Prepare/Settled；若有已分配谱系仍须退役。
                Some(RecoveryStart::Retire(job)) => {
                    self.spawn_recovery_retire(job, cx);
                }
                Some(RecoveryStart::Publish(_)) | None => {}
            },
            Some(RecoveryAction::NotDue { remaining_millis }) => {
                self.spawn_recovery_timer(remaining_millis, window, cx);
            }
            Some(RecoveryAction::Idle) | None => {}
        }
    }

    /// 后台完成整段物化；回到窗口复验版本后再决定发布。
    fn start_recovery_prepare(
        &mut self,
        job: RecoveryPrepareJob,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task_id) = self.recovery_slot.begin_work() else {
            return;
        };
        let work = cx.background_executor().spawn(async move { job.run() });
        self.recovery_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let prepared = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                // 槽已被替换或关闭：迟到准备不得清除新任务；已分配的谱系在此退役。
                if !this.recovery_slot.finish_work(task_id) {
                    if let Some(job) = prepared.into_close_job() {
                        this.spawn_recovery_retire(job, cx);
                    }
                    return;
                }
                this.recovery_task = None;
                let now = this.now_millis();
                match this.state.recovery_prepared(prepared, now) {
                    Some(RecoveryStart::Publish(job)) => {
                        this.start_recovery_publish(job, window, cx);
                    }
                    Some(RecoveryStart::Settled { feedback }) => {
                        this.state.apply_recovery_feedback(feedback);
                        this.arm_recovery(window, cx);
                    }
                    // 防御分支：正常准备完成不会直接退役；测试可构造该路径。
                    Some(RecoveryStart::Retire(job)) => {
                        this.spawn_recovery_retire(job, cx);
                        this.arm_recovery(window, cx);
                    }
                    // 防御分支：准备任务不会再产出准备任务；出现时按新任务继续。
                    Some(RecoveryStart::Prepare(job)) => {
                        this.start_recovery_prepare(job, window, cx);
                    }
                    None => {}
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    /// 后台发布一代恢复记录；回到窗口结算反馈并重新布防。
    fn start_recovery_publish(
        &mut self,
        job: RecoveryPublishJob,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task_id) = self.recovery_slot.begin_work() else {
            return;
        };
        let work = cx.background_executor().spawn(async move { job.run() });
        self.recovery_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let settlement = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                // 槽已被替换或关闭：迟到发布结算不得清除新任务。
                if !this.recovery_slot.finish_work(task_id) {
                    return;
                }
                this.recovery_task = None;
                let now = this.now_millis();
                let feedback = this.state.recovery_settled(settlement, now);
                this.state.apply_recovery_feedback(feedback);
                this.arm_recovery(window, cx);
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    /// 在后台退役已归属旧会话或迟到准备的恢复谱系（W4-53）。
    fn spawn_recovery_retire(&mut self, job: RecoveryCloseJob, cx: &mut Context<Self>) {
        cx.background_executor()
            .spawn(async move {
                job.run();
            })
            .detach();
    }

    /// 后台执行图片迁移；完成后复验引用再决定写盘或反馈。
    fn start_association(
        &mut self,
        job: ResourceMigrationJob,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.save_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.save_task = None;
                let now = this.now_millis();
                if let Some(intent) = this.state.association_finished(completion, now) {
                    this.run_save_intent(intent, window, cx);
                }
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    /// 后台执行真实写盘；完成后回到窗口结算并显示本地化反馈。
    fn start_save(&mut self, job: SaveJob, window: &Window, cx: &mut Context<Self>) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.save_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.save_task = None;
                let settlement = this.state.settle_save(completion);
                if let Some(key) = settlement.feedback {
                    this.state.feedback = Some(key);
                }
                // 首次关联成功后才创建恢复会话：宿主在此消费一次布防。
                if this.state.take_recovery_pending_arm() {
                    this.reset_recovery_tasks();
                    this.arm_recovery(window, cx);
                }
                if this.state.take_external_refresh() {
                    this.check_external(ExternalCheckIntent::Focus, window, cx);
                }
                // 保存期间到期但被 Busy 挡下的自动保存，或保存期间的新输入，在结算后立即重查。
                this.autosave_tick(window, cx);
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    /// 取消组合期保存等待；不触碰焦点、组合或输入状态。
    fn cancel_save_wait_action(
        &mut self,
        _: &CancelSaveWait,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.state.cancel_save_wait();
        self.update_menu(cx);
        cx.notify();
    }

    /// 面板“恢复”按钮：只读该代记录，成功后由窗口原子采纳内存文档。
    fn recovery_restore(
        &mut self,
        document: RecoveryDocumentId,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.recovery_panel_action(RecoveryPanelAction::Restore { document }, window, cx);
    }

    /// 面板“丢弃”按钮：先进入二次确认，不直接退役。
    fn recovery_request_discard(
        &mut self,
        document: RecoveryDocumentId,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        self.recovery_panel_action(RecoveryPanelAction::RequestDiscard { document }, window, cx);
    }

    fn recovery_cancel_discard(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.recovery_panel_action(RecoveryPanelAction::CancelDiscard, window, cx);
    }

    /// 确认丢弃：只有确认态匹配的条目才派生 `RecoveryStore::discard`。
    fn recovery_confirm_discard(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.recovery_panel_action(RecoveryPanelAction::ConfirmDiscard, window, cx);
    }

    /// 所有面板动作的入口；同一时刻只允许一个在途操作。
    fn recovery_panel_action(
        &mut self,
        action: RecoveryPanelAction,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if self.recovery_panel_task.is_some() {
            return;
        }
        let Some(store) = self.state.recovery_store.clone() else {
            return;
        };
        let temp_root = self.state.recovery_temp_root.clone();
        match self.state.panel.request(action, &store, temp_root) {
            RecoveryPanelStep::Read(job) => self.start_recovery_restore(job, window, cx),
            RecoveryPanelStep::Discard(job) => self.start_recovery_discard(job, window, cx),
            RecoveryPanelStep::Ignore | RecoveryPanelStep::Recovered(_) => {
                self.state.feedback = self.state.panel.feedback_key();
                self.update_menu(cx);
                cx.notify();
            }
        }
    }

    /// 后台读取一代恢复记录；完成后构造内存文档或保留记录报告失败。
    fn start_recovery_restore(
        &mut self,
        job: RecoveryRestoreJob,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.recovery_panel_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion: RecoveryRestoreCompletion = work.await;
            let _ = weak.update_in(cx, |this, window, cx| {
                this.recovery_panel_task = None;
                let document = completion.document;
                match this.state.panel.restore_finished(completion) {
                    RecoveryPanelStep::Recovered(restored) => {
                        this.open_recovered_document(document, restored, window, cx);
                    }
                    _ => {
                        this.state.feedback = this.state.panel.feedback_key();
                        this.update_menu(cx);
                        cx.notify();
                    }
                }
            });
        }));
    }

    /// 后台丢弃一代恢复记录；只有退役成功才隐藏条目。
    fn start_recovery_discard(
        &mut self,
        job: crate::document::RecoveryDiscardJob,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let work = cx.background_executor().spawn(async move { job.run() });
        self.recovery_panel_task = Some(cx.spawn_in(window, async move |weak, cx| {
            let completion = work.await;
            let _ = weak.update_in(cx, |this, _, cx| {
                this.recovery_panel_task = None;
                this.state.panel.discard_finished(completion);
                this.state.feedback = this.state.panel.feedback_key();
                this.update_menu(cx);
                cx.notify();
            });
        }));
    }

    /// 把恢复结果构造成当前可编辑文档；不授予路径代次、不布防自动保存/恢复。
    fn open_recovered_document(
        &mut self,
        document: RecoveryDocumentId,
        restored: RestoredRecovery,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.invalidate_statistics();
        let generation = self.next_reader_generation.checked_add(1);
        let mut layout_timing = StageTiming::default();
        let mut adopt_started = None;
        let result = self
            .state
            .session_mut()
            .open_recovered_with(restored, |candidate| {
                let generation = generation.ok_or(LayoutError::GenerationExhausted)?;
                let viewport_width = f32::from(window.viewport_size().width);
                let metrics = content_metrics(viewport_width, viewport_width);
                let layout_started = StageTimer::start();
                let result = ReaderSurface::new(candidate, generation, self.evidence.clone())
                    .and_then(|mut reader| {
                        reader.prepare(candidate, window, metrics, &self.fonts)?;
                        Ok((reader, generation))
                    });
                layout_timing = layout_started.finish();
                adopt_started = Some(StageTimer::start());
                result
            });
        let adopt_timing = adopt_started.map_or_else(StageTiming::default, StageTimer::finish);
        match result {
            Ok((open, Some((reader, generation)))) => {
                // 新文档接管窗口：旧状态与在途输入全部作废；恢复/自动保存等待首次关联。
                self.clear_input_state();
                self.reset_recovery_tasks();
                self.state.panel.mark_restored(document);
                reader.schedule_height_hint(window, cx);
                self.reader = Some(reader);
                self.next_reader_generation = generation;
                self.loaded_name = Some(
                    open.original_path
                        .as_deref()
                        .and_then(Path::file_name)
                        .map(|name| SharedString::from(name.to_string_lossy().into_owned()))
                        .unwrap_or_else(|| self.copy.recovery_unnamed.clone()),
                );
                if let Some(name) = &self.loaded_name {
                    window.set_window_title(name.as_ref());
                }
                self.refresh_find(cx);
                self.state.feedback = Some("document.recovery_restored");
                if let (Some(evidence), Some(adopted)) =
                    (&self.evidence, self.state.session().document())
                    && let Err(error) = evidence.borrow_mut().opened(adopted)
                {
                    eprintln!("m1-readonly-evidence recovered open failed: {error}");
                }
                self.probe_read_finished(
                    crate::document::OpenTimings::default(),
                    layout_timing,
                    adopt_timing,
                    true,
                );
                self.refresh_statistics(window, cx);
                // 采纳恢复文档等同一次文档打开：焦点落在窗口根，保证按键
                // 走窗口根的 key context，而不是悬空的恢复面板按钮。
                self.focus.focus(window, cx);
            }
            Ok((_, None)) => {}
            Err(_) => {
                self.state.panel.restore_open_failed(document);
                self.state.feedback = Some("document.recovery_restore_failed");
            }
        }
        self.update_menu(cx);
        cx.notify();
    }

    /// 取走关闭/替换后待退役的恢复任务与资源清理任务，按序在同一后台链执行。
    fn drain_retired(&mut self, cx: &mut App) {
        let (recoveries, cleanups) = self.state.take_retired();
        if recoveries.is_empty() && cleanups.is_empty() {
            return;
        }
        cx.background_executor()
            .spawn(async move {
                for job in recoveries {
                    job.run();
                }
                for job in cleanups {
                    job.run();
                }
            })
            .detach();
    }

    /// 关闭前捕获当前核心，关闭后在后台读磁盘并只创建证据文件。
    fn close_with_evidence(&mut self, cx: &mut App) -> bool {
        // M2 取证在释放前最后采样一次：脏判定与关闭结论只在此时可算。
        if let Some(evidence) = self.m2_evidence.as_mut()
            && let Some(document) = self.state.session_mut().document_mut()
        {
            evidence.sample(document);
        }
        let evidence = self.evidence.clone();
        let m2_evidence = self.m2_evidence.as_mut();
        let packets = match self.state.close_document_with(|document| {
            (
                evidence
                    .as_ref()
                    .map(|evidence| evidence.borrow_mut().closing(document)),
                m2_evidence.map(|evidence| evidence.closing(document)),
            )
        }) {
            Ok(Some(packets)) => Some(packets),
            Ok(None) => None,
            Err(()) => return false,
        };
        if let Some((m1_packet, m2_packet)) = packets {
            self.m2_evidence = None;
            if let Some(packet) = m1_packet {
                match packet {
                    Ok(packet) => {
                        let closed = self.state.session().document().is_none();
                        cx.background_executor()
                            .spawn(async move {
                                if packet.write(closed).is_err() {
                                    eprintln!("m1-readonly-evidence write failed");
                                }
                            })
                            .detach();
                    }
                    Err(error) => eprintln!("m1-readonly-evidence close failed: {error}"),
                }
            }
            if let Some(packet) = m2_packet {
                cx.background_executor()
                    .spawn(async move {
                        if packet.write().is_err() {
                            eprintln!("m2-evidence write failed");
                        }
                    })
                    .detach();
            }
        }
        // 关闭成功：取消待办自动保存与恢复计时，丢弃正文帧与排队输入；在途结果按文档身份被拒绝。
        // 会话已在释放前取消自动保存/关联并构造退役与清理任务。
        self.autosave_task = None;
        self.reset_recovery_tasks();
        // 退役完成后才执行资源清理；同一后台链保证顺序（W4-53）。
        self.drain_retired(cx);
        self.clear_input_state();
        true
    }

    /// 立即作废统计值与在途任务；关闭、替换或开始新读取时调用。
    fn invalidate_statistics(&mut self) {
        self.statistics = None;
        self.statistics_task = None;
    }

    /// 为当前文档在后台执行器调度一次统计；同一文档只算一次。
    ///
    /// 计算与文案插值都不在 render 路径：结果连同 `DocumentId` 回到窗口，
    /// 与当前文档身份一致才被采纳，文档已替换或关闭时直接丢弃。
    fn refresh_statistics(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(document) = self.state.session().document() else {
            self.invalidate_statistics();
            return;
        };
        let id = document.core().document_id().clone();
        if self
            .statistics
            .as_ref()
            .is_some_and(|(current, _, _)| current == &id)
            || self
                .statistics_task
                .as_ref()
                .is_some_and(|(current, _)| current == &id)
        {
            return;
        }
        // 先取源码再作废旧值：取文本只需要只读文档，作废会改写窗口状态。
        let source = statistics_source(document);
        // 文档已替换或旧任务属于旧文档：两者都作废，晚到结果还会按身份再拒绝。
        self.invalidate_statistics();
        let Some(source) = source else {
            self.state.feedback = Some("open_error.internal");
            return;
        };
        let work = cx
            .background_executor()
            .spawn(async move { DocumentStatistics::compute(&source) });
        let expected = id.clone();
        let task = cx.spawn_in(window, async move |weak, cx| {
            let statistics = work.await;
            let _ = weak.update_in(cx, |this, _, cx| {
                let current = this
                    .state
                    .session()
                    .document()
                    .map(|document| document.core().document_id().clone());
                if current.as_ref() != Some(&expected) {
                    return;
                }
                match this.copy.statistics_labels(statistics) {
                    Ok((characters, lines)) => {
                        this.statistics = Some((expected.clone(), characters, lines));
                    }
                    Err(_) => this.state.feedback = Some("open_error.internal"),
                }
                this.statistics_task = None;
                cx.notify();
            });
        });
        self.statistics_task = Some((id, task));
    }

    fn switch_view(&mut self, mode: EditorMode, cx: &mut Context<Self>) {
        if let Some(document) = self.state.session.document_mut() {
            let before = document.core().mode();
            match document.switch_mode(mode) {
                Err(error) => self.state.feedback = Some(edit_error_key(error)),
                Ok(ModeSwitchOutcome::Deferred) => {
                    self.state.feedback = Some("document.composition_pending");
                }
                Ok(ModeSwitchOutcome::Ready) => {
                    if let Some(evidence) = &self.evidence {
                        evidence.borrow_mut().switching(before, mode);
                    }
                    if self.state.feedback == Some("reading.large_source") {
                        self.state.feedback = None;
                    }
                }
            }
            self.refresh_find(cx);
            cx.notify();
        }
    }

    fn dispatch_edit(&mut self, action: EditAction, window: &Window, cx: &mut Context<Self>) {
        if !self.find_contains_focus(window, cx) {
            self.edit_document(action, window, cx);
            self.refresh_find(cx);
        }
    }

    /// 验证专用：推进一次字体代次，让布局缓存键失效并重新测量。
    ///
    /// 只有 `ALINE_M1_FONT_INVALIDATE_FAMILY` 与 `ALINE_M1_FONT_INVALIDATE_MARK`
    /// 同时设置时 `font_invalidation` 才存在，快捷键也只在此时绑定；生产路径
    /// 不会走到这里。触发后显式重测，因为字体代次进入布局缓存键，
    /// `prepare` 会在下一帧发现键不同并重置高度缓存。
    fn invalidate_font(&mut self, _: &InvalidateFont, window: &mut Window, cx: &mut Context<Self>) {
        let Some(request) = &self.font_invalidation else {
            return;
        };
        let invalidated = request.apply(&mut self.fonts, cx.text_system().as_ref());
        match invalidated {
            Some(result) => {
                if let Some(evidence) = &self.evidence {
                    evidence.borrow_mut().event("font-invalidated");
                }
                eprintln!(
                    "m1-font-invalidate family={} generation={}",
                    result.family, result.generation
                );
            }
            None => eprintln!("m1-font-invalidate not activated"),
        }
        let metrics = content_metrics(
            f32::from(window.viewport_size().width),
            f32::from(window.viewport_size().width),
        );
        let mut reset = false;
        let mut error = None;
        if let (Some(reader), Some(document)) = (&mut self.reader, self.state.session().document())
        {
            match reader.prepare(document, window, metrics, &self.fonts) {
                Ok(reader_reset) => reset = reader_reset,
                Err(layout_error) => error = Some(layout_error),
            }
        }
        // 字体代次变化会重置行高缓存；与 render 路径一致，必须在本帧末给
        // 未测量行补正文行高提示，否则滚动条比例与 End/PageUp 会低估总高。
        if reset && let Some(reader) = &self.reader {
            reader.schedule_height_hint(window, cx);
        }
        if let Some(error) = error {
            self.state.feedback = Some(layout_error_key(error));
        }
        cx.notify();
    }

    fn render_reading_row(&mut self, row: usize, cx: &mut Context<Self>) -> gpui::AnyElement {
        let Some(reader) = &self.reader else {
            return div().into_any_element();
        };
        let Some(document) = self.state.session.document_mut() else {
            return div().into_any_element();
        };
        // 组合活动时解析身份必然失效，行级 `HitTarget` 创建失败，reader 不会挂
        // 命中回调；因此反馈必须在本层行容器上给出。
        let composing = document.core().composition().is_some();
        if document.is_editable()
            && let Some(range) = reader.input_row(document, row)
            && let Some(site) =
                editable_input::InputSite::for_row(document, range, self.source_input_site.as_ref())
            && let Some((input, frame, geometry)) =
                editable_input::render(document, site, self.focus.clone(), cx)
        {
            self.input_frame = Some(frame);
            // 输入行由编辑框自行处理组合与点击，不套阅读行守卫。
            return reader.wrap_input_row(document, row, input, geometry);
        }
        let failure = self
            .copy
            .errors
            .get("reading.layout_budget")
            .cloned()
            .unwrap_or_default();
        // 普通行、虚拟列表行与源码行统一经 `render_row` 产出；闭包只持有弱实体，
        // 不借用 `self.reader` / `document` / `self.copy`。
        let element = {
            let weak = cx.entity().downgrade();
            let navigate = cx.entity().downgrade();
            reader.render_row(
                document,
                row,
                failure,
                &self.copy.content,
                move |target, candidate, prepared, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.accept_reading_hit(target, candidate, prepared, cx)
                    });
                },
                move |target, _, cx| {
                    let _ = navigate.update(cx, |this, cx| this.navigate_to_source(target, cx));
                },
            )
        };
        if composing {
            // 组合期间行容器兜底：只给既有组合待定反馈，不记录 hit、不滚动、
            // 不改选区、不结束组合，避免跨块点击无声无息。
            let dismiss = cx.entity().downgrade();
            return div()
                .w_full()
                // 窗口根可聚焦：按下时它的自动焦点转移会把焦点从输入框
                // 挪走，平台输入法随之结束组合。捕获阶段取消默认行为，
                // 点击仍会冒泡给出组合待定反馈。
                .capture_any_mouse_down(|_, window, _| window.prevent_default())
                .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                    let _ = dismiss.update(cx, |this, cx| {
                        this.state.feedback = Some("document.composition_pending");
                        cx.notify();
                    });
                })
                .child(element)
                .into_any_element();
        }
        element
    }

    /// 目录与脚注跳转：先写入只读导航锚点，再把目标行滚到顶部。
    fn navigate_to_source(&mut self, target: SourceNavigationTarget, cx: &mut Context<Self>) {
        let Some(reader) = &self.reader else {
            return;
        };
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        if !document.is_parse_current()
            || reader.check_index(document.parsed()).is_err()
            || reader.layout_token() != Some(target.layout)
            || document.parsed().projection().id() != &target.projection
            || target.source.snapshot_id() != document.parsed().snapshot().id()
        {
            return;
        }
        let Ok(identity) = document.input_identity() else {
            return;
        };
        if target.identity.validate(&identity).is_err() {
            return;
        }
        let Ok(selection) =
            aline_buffer::SrcSelection::new(target.source.clone(), target.source.clone())
        else {
            return;
        };
        if document.navigate(&target.identity, &selection).is_err() {
            return;
        }
        if reader
            .scroll_to_source(document, target.source.byte_offset())
            .is_ok()
            && let Some(evidence) = &self.evidence
        {
            evidence.borrow_mut().event("jump");
        }
        cx.notify();
    }

    fn accept_reading_hit(
        &mut self,
        target: &HitTarget,
        candidate: Result<Result<usize, usize>, HitError>,
        prepared: &PreparedLayout,
        cx: &mut Context<Self>,
    ) {
        let Some(reader) = &self.reader else {
            return;
        };
        let Some(document) = self.state.session.document_mut() else {
            return;
        };
        // 设计决定 3：组合活动时先等平台自然结束。组合态主要由行容器守卫；
        // 此处兜底旧帧竞态（组合开始后、下一次重绘前，旧元素仍持有命中回调）。
        // 只给反馈，不记录 hit、不滚动、不改选区、不结束组合。
        if document.core().composition().is_some() {
            self.state.feedback = Some("document.composition_pending");
            cx.notify();
            return;
        }
        let expected_edge = candidate
            .as_ref()
            .ok()
            .is_some_and(|candidate| candidate.is_err());
        let outcome = candidate.and_then(|candidate| {
            target.navigate_candidate(document, reader.layout_token(), candidate)
        });
        if let Ok(hit) = &outcome {
            if let Some(evidence) = &self.evidence {
                evidence.borrow_mut().event("hit");
            }
            debug_assert_eq!(hit.is_edge_candidate(), expected_edge);
            let start = hit.source_range().byte_range().start;
            match navigation_intent(document.parsed(), start) {
                Some(NavigationIntent::Jump { target }) => {
                    if reader.scroll_to_source(document, target.start).is_ok()
                        && let Some(evidence) = &self.evidence
                    {
                        evidence.borrow_mut().event("jump");
                    }
                }
                _ if document.is_editable() => {
                    let current_selection = document.core().current_selection();
                    let mapped = (|| {
                        let snapshot = document.parsed().snapshot();
                        aline_buffer::SrcSelection::new(
                            snapshot.src_offset(current_selection.anchor())?,
                            snapshot.src_offset(current_selection.head())?,
                        )
                    })();
                    if let Ok(selection) = mapped
                        && let Ok(identity) = document.input_identity()
                        // 进入本分支时前面的组合守卫已确认无组合，同步执行期间
                        // 组合状态不会变化，因此这里不会返回 `CompositionActive`。
                        && document.open_visual_at(&identity, &selection).is_ok()
                    {
                        cx.notify();
                    }
                }
                _ => {}
            }
            debug_assert!(match hit.precision() {
                aline_markdown::MappingPrecision::Exact
                | aline_markdown::MappingPrecision::Synthetic => hit.source_range().is_empty(),
                aline_markdown::MappingPrecision::Aligned
                | aline_markdown::MappingPrecision::Hidden => !hit.source_range().is_empty(),
            });
        }
        #[cfg(debug_assertions)]
        match &outcome {
            Ok(outcome) => eprintln!(
                "m1-hit accepted precision={:?} source_start={} source_end={} anchor={} edge={} layout_text_matches_projection={} open={} history={} revision={}",
                outcome.precision(),
                outcome.source_range().byte_range().start,
                outcome.source_range().byte_range().end,
                document.core().buffer_selection().head(),
                outcome.is_edge_candidate(),
                target.layout_text_matches(prepared).unwrap_or(false),
                document.core().has_open_block(),
                document.core().history_node_count(),
                document.core().version().revision().value()
            ),
            Err(error) => eprintln!("m1-hit rejected={error:?}"),
        }
        #[cfg(not(debug_assertions))]
        let _ = (outcome, prepared);
        cx.notify();
    }

    /// 工具栏编辑模式按钮：文字标签、可聚焦、当前模式用强调色弱背景表达。
    fn mode_button(
        &self,
        id: &'static str,
        label: SharedString,
        active: bool,
        mode: EditorMode,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        div()
            .id(id)
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(label.clone())
            .focus(|style| style.border_color(rgb(tokens::ACCENT)))
            .cursor_pointer()
            .flex()
            .items_center()
            .h(px(30.0))
            .px_3()
            .rounded_md()
            .border_1()
            .border_color(rgba(tokens::TRANSPARENT_BORDER))
            .text_size(px(13.0))
            .when(active, |button| {
                button
                    .bg(rgba(tokens::ACCENT_SOFT))
                    .text_color(rgb(tokens::ACCENT))
            })
            .when(!active, |button| {
                button
                    .text_color(rgb(tokens::TEXT))
                    .hover(|style| style.bg(rgba(tokens::HOVER)))
            })
            .capture_any_mouse_down(cx.listener(|this, _: &gpui::MouseDownEvent, window, _cx| {
                // 组合期间点击模式按钮属于拒绝路径。按钮可聚焦，按下时
                // GPUI 的自动焦点转移会移走输入框焦点，平台输入法随即
                // 结束组合并丢掉 marked 草稿（继续输入会从零重新组合）。
                // 捕获阶段取消默认焦点转移即可保留组合；点击照常冒泡到
                // on_click，由 request_view 给出「组合待定」反馈。
                let composing = this
                    .state
                    .session()
                    .document()
                    .is_some_and(|document| document.core().composition().is_some());
                if composing {
                    window.prevent_default();
                }
            }))
            .on_click(cx.listener(move |this, _, window, cx| this.request_view(mode, window, cx)))
            .on_key_down(
                cx.listener(move |this, event: &gpui::KeyDownEvent, window, cx| {
                    if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                        this.request_view(mode, window, cx);
                        cx.stop_propagation();
                    }
                }),
            )
            .child(label)
            .into_any_element()
    }

    fn update_menu(&self, cx: &App) {
        let editable = self
            .state
            .session()
            .document()
            .is_some_and(ReadonlyDocument::is_editable);
        let exits = conflict_exits(
            editable,
            self.state
                .session()
                .document()
                .and_then(ReadonlyDocument::external_view),
        );
        self.copy.install_menu(
            cx,
            self.state.session().document().is_some() || self.state.session().is_loading(),
            editable,
            self.state.save_as_enabled(),
            self.state.save_cancel_enabled(),
            exits,
        );
    }

    /// 冲突面板的只读预览节：标签 + 限高可滚动文本。
    fn conflict_preview(
        &self,
        id: &'static str,
        key: &'static str,
        text: Arc<str>,
    ) -> gpui::AnyElement {
        let label = self.copy.errors.get(key).cloned().unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(tokens::SECONDARY))
                    .child(label),
            )
            .child(
                div()
                    .id(id)
                    .overflow_y_scroll()
                    .max_h(px(80.0))
                    .text_xs()
                    .text_color(rgb(tokens::TEXT))
                    .child(SharedString::from(text)),
            )
            .into_any_element()
    }

    /// 冲突面板按钮：可用时有焦点、悬停与鼠标/键盘触发；禁用项不聚焦、不响应。
    fn conflict_button(
        &self,
        id: impl Into<gpui::ElementId>,
        label: SharedString,
        enabled: bool,
        action: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + Clone + 'static,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let click = action.clone();
        let key = action;
        div()
            .id(id)
            .aria_label(label.clone())
            .px_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(tokens::BORDER))
            .text_xs()
            .text_color(rgb(if enabled {
                tokens::TEXT
            } else {
                tokens::SECONDARY
            }))
            .when(enabled, |button| {
                button
                    .focusable()
                    .tab_stop(true)
                    .role(Role::Button)
                    .focus(|style| style.border_color(rgb(tokens::ACCENT)))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgba(tokens::HOVER)))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        click(this, window, cx);
                        // 按钮在按下时接管焦点；动作可能移除按钮本身（面板消失、
                        // 条目退役），悬空的焦点会让后续按键落不到窗口根的
                        // key context，必须把焦点还给窗口根。
                        this.focus.focus(window, cx);
                    }))
                    .on_key_down(cx.listener(
                        move |this, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                                key(this, window, cx);
                                this.focus.focus(window, cx);
                                cx.stop_propagation();
                            }
                        },
                    ))
            })
            .child(label)
            .into_any_element()
    }

    /// 渲染启动恢复面板（W4-56）：逐文档恢复/丢弃，无批量入口；
    /// 不可恢复项只报告，丢弃需二次确认。仅在无文档时显示。
    fn render_recovery_panel(&self, cx: &mut Context<Self>) -> Option<gpui::AnyElement> {
        if !self.state.panel.is_visible() {
            return None;
        }
        let busy = self.state.panel.is_busy();
        let mut panel = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .px_4()
            .py_3()
            .border_b_1()
            .border_color(rgb(tokens::BORDER))
            .bg(rgb(tokens::NOTICE_SURFACE))
            .child(
                div()
                    .text_sm()
                    .text_color(rgb(tokens::TEXT))
                    .child(self.copy.recovery_title.clone()),
            );
        if let Some(message) = self
            .state
            .panel
            .feedback_key()
            .and_then(|key| self.copy.errors.get(key))
        {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(tokens::SECONDARY))
                    .child(message.clone()),
            );
        }
        let unrecoverable = self.state.panel.unrecoverable();
        if unrecoverable > 0
            && let Ok(label) = self.copy.recovery_unrecoverable_label(unrecoverable)
        {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(tokens::SECONDARY))
                    .child(label),
            );
        }
        let confirming = self.state.panel.confirming();
        for entry in self.state.panel.entries() {
            let id = entry.document.get().get();
            let document_id = entry.document;
            let status = entry.status;
            let label = self
                .copy
                .recovery_record_label(id)
                .unwrap_or_else(|_| self.copy.recovery_title.clone());
            let mut row = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_2()
                .child(div().text_xs().text_color(rgb(tokens::TEXT)).child(label))
                .child(
                    div()
                        .text_xs()
                        .text_color(rgb(tokens::SECONDARY))
                        .child(SharedString::from(format!("{} B", entry.payload_bytes))),
                );
            if confirming == Some(entry.document) {
                row = row
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(tokens::SECONDARY))
                            .child(self.copy.recovery_discard_prompt.clone()),
                    )
                    .child(self.conflict_button(
                        SharedString::from(format!("recovery-confirm-{id}")),
                        self.copy.recovery_discard_confirm.clone(),
                        !busy,
                        |this, window, cx| this.recovery_confirm_discard(window, cx),
                        cx,
                    ))
                    .child(self.conflict_button(
                        SharedString::from(format!("recovery-cancel-{id}")),
                        self.copy.recovery_discard_cancel.clone(),
                        !busy,
                        |this, window, cx| this.recovery_cancel_discard(window, cx),
                        cx,
                    ));
            } else {
                if status == RecoveryPanelEntryStatus::Available {
                    row = row.child(self.conflict_button(
                        SharedString::from(format!("recovery-restore-{id}")),
                        self.copy.recovery_restore.clone(),
                        !busy,
                        move |this, window, cx| {
                            this.recovery_restore(document_id, window, cx);
                        },
                        cx,
                    ));
                }
                row = row.child(self.conflict_button(
                    SharedString::from(format!("recovery-discard-{id}")),
                    self.copy.recovery_discard.clone(),
                    !busy,
                    move |this, window, cx| {
                        this.recovery_request_discard(document_id, window, cx);
                    },
                    cx,
                ));
            }
            panel = panel.child(row);
        }
        Some(panel.into_any_element())
    }

    /// 渲染冲突面板；无视图时不渲染。
    fn render_conflict_panel(
        &self,
        conflict: Option<ConflictPanelData>,
        exits: ConflictExits,
        cx: &mut Context<Self>,
    ) -> Option<gpui::AnyElement> {
        let (state, baseline, local, disk) = conflict?;
        let title = self
            .copy
            .errors
            .get(external_state_key(state))
            .cloned()
            .unwrap_or_default();
        let mut panel = div()
            .w_full()
            .flex()
            .flex_col()
            .gap_1()
            .px_3()
            .py_2()
            .bg(rgb(tokens::NOTICE_SURFACE))
            .border_b_1()
            .border_color(rgb(tokens::BORDER))
            .child(div().text_xs().text_color(rgb(tokens::TEXT)).child(title))
            .child(self.conflict_preview(
                "conflict-baseline",
                "document.conflict_baseline",
                baseline,
            ))
            .child(self.conflict_preview("conflict-local", "document.conflict_local", local));
        if let Some(disk) = disk {
            panel =
                panel.child(self.conflict_preview("conflict-disk", "document.conflict_disk", disk));
        }
        let buttons = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_2()
            .child(self.conflict_button(
                "conflict-load-disk",
                self.copy.load_disk.clone(),
                exits.load_disk,
                |this, window, cx| {
                    this.check_external(ExternalCheckIntent::Reload, window, cx);
                },
                cx,
            ))
            .child(self.conflict_button(
                "conflict-confirm-overwrite",
                self.copy.confirm_overwrite.clone(),
                exits.overwrite,
                |this, window, cx| {
                    this.check_external(ExternalCheckIntent::Overwrite, window, cx);
                },
                cx,
            ))
            .child(self.conflict_button(
                "conflict-retry",
                self.copy.retry_external_check.clone(),
                exits.retry,
                |this, window, cx| {
                    this.check_external(ExternalCheckIntent::Retry, window, cx);
                },
                cx,
            ))
            .child(self.conflict_button(
                "conflict-save-as",
                self.copy.save_as.clone(),
                false,
                |_, _, _| {},
                cx,
            ))
            .child(self.conflict_button(
                "conflict-dismiss",
                self.copy.dismiss_conflict.clone(),
                exits.dismiss,
                |this, _, cx| {
                    this.state.dismiss_external();
                    this.update_menu(cx);
                    cx.notify();
                },
                cx,
            ));
        Some(panel.child(buttons).into_any_element())
    }
}

impl Render for NativeDocumentWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let probe_ticket = self
            .probe
            .as_mut()
            .and_then(probe::ProbeState::begin_render);
        let phase = self
            .state
            .session()
            .pending_phase()
            .map(|phase| match phase {
                crate::ReadonlyOpenPhase::Selecting => self.copy.selecting.clone(),
                crate::ReadonlyOpenPhase::Loading => self.copy.loading.clone(),
            });
        let viewport_width = f32::from(window.viewport_size().width);
        let metrics = content_metrics(viewport_width, viewport_width);
        let mut reader_reset = false;
        if let (Some(reader), Some(document)) = (&mut self.reader, self.state.session().document())
        {
            match reader.prepare(document, window, metrics, &self.fonts) {
                Ok(reset) => reader_reset = reset,
                Err(error) => self.state.feedback = Some(layout_error_key(error)),
            }
        }
        if reader_reset && let Some(reader) = &self.reader {
            reader.schedule_height_hint(window, cx);
        }
        let document_open = self.state.session().document().is_some();
        let editable = self
            .state
            .session()
            .document()
            .is_some_and(ReadonlyDocument::is_editable);
        let mode = self
            .state
            .session()
            .document()
            .map(|document| document.core().mode());
        let entity = cx.entity().downgrade();
        self.input_frame = None;
        let reader_element = self.reader.as_ref().map(|reader| {
            reader.render(move |row, _, cx| {
                entity
                    .update(cx, |this, cx| this.render_reading_row(row, cx))
                    .unwrap_or_else(|_| div().into_any_element())
            })
        });
        let mut notices: Vec<SharedString> = Vec::new();
        if let Some(phase) = phase {
            notices.push(phase);
        }
        if let Some(message) = self
            .state
            .feedback_key()
            .and_then(|key| self.copy.errors.get(key))
        {
            notices.push(message.clone());
        }
        if let Some(message) = self
            .state
            .session()
            .document()
            .and_then(|document| parse_fallback_key(document.parsed()))
            .and_then(|key| self.copy.errors.get(key))
        {
            notices.push(message.clone());
        }
        if let Some(fallback) = &self.locale_fallback {
            notices.push(fallback.clone());
        }
        let cancel_pending = self
            .reader
            .as_ref()
            .is_some_and(|reader| reader.has_pending_restore());
        let notice = (!notices.is_empty() || cancel_pending).then(|| {
            div()
                .w_full()
                .flex()
                .flex_col()
                .items_start()
                .gap_1()
                .px_3()
                .py_2()
                .bg(rgb(tokens::NOTICE_SURFACE))
                .border_b_1()
                .border_color(rgb(tokens::BORDER))
                .text_xs()
                .text_color(rgb(tokens::SECONDARY))
                .children(notices)
                .when(cancel_pending, |banner| {
                    banner.child(
                        div()
                            .id("cancel-navigation")
                            .focusable()
                            .tab_stop(true)
                            .role(Role::Button)
                            .aria_label(self.copy.cancel_navigation.clone())
                            .focus(|style| style.border_color(rgb(tokens::ACCENT)))
                            .cursor_pointer()
                            .px_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(tokens::BORDER))
                            .hover(|style| style.bg(rgba(tokens::HOVER)))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.visual_input_restore = None;
                                if let Some(reader) = &this.reader {
                                    reader.cancel_restore();
                                    cx.notify();
                                }
                            }))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "enter" || event.keystroke.key == "space"
                                {
                                    this.visual_input_restore = None;
                                    if let Some(reader) = &this.reader {
                                        reader.cancel_restore();
                                        cx.notify();
                                    }
                                    cx.stop_propagation();
                                }
                            }))
                            .child(self.copy.cancel_navigation.clone()),
                    )
                })
        });
        let conflict_view = self
            .state
            .session()
            .document()
            .and_then(ReadonlyDocument::external_view);
        let conflict_exits_now = conflict_exits(editable, conflict_view);
        let conflict = conflict_view.map(|view| {
            (
                view.state,
                view.baseline.clone(),
                view.local.clone(),
                view.disk.clone(),
            )
        });
        let conflict_panel = self.render_conflict_panel(conflict, conflict_exits_now, cx);
        let recovery_panel =
            if recovery_panel::recovery_panel_visible(document_open, self.state.panel.is_visible())
            {
                self.render_recovery_panel(cx)
            } else {
                None
            };
        let open_button = div()
            .id("open-document")
            .focusable()
            .tab_stop(true)
            .role(Role::Button)
            .aria_label(self.copy.open.clone())
            .focus(|style| style.border_color(rgb(tokens::ACCENT)))
            .cursor_pointer()
            .px_4()
            .py_2()
            .rounded_md()
            .border_1()
            .border_color(rgb(tokens::BORDER))
            .text_size(px(14.0))
            .hover(|style| style.bg(rgba(tokens::HOVER)))
            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                    this.open_document(&OpenDocument, window, cx);
                    cx.stop_propagation();
                }
            }))
            .on_click(
                cx.listener(|this, _, window, cx| this.open_document(&OpenDocument, window, cx)),
            )
            .child(self.copy.open.clone());
        let content_body = match reader_element {
            Some(reader) => div().flex_1().min_h_0().w_full().child(reader),
            None => {
                let mut body = div().flex_1().min_h_0().w_full().flex().flex_col();
                if let Some(panel) = recovery_panel {
                    body = body.child(panel);
                }
                body.when(!document_open, |body| {
                    body.child(
                        div()
                            .flex_1()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .text_xl()
                                    .text_color(rgb(tokens::TEXT))
                                    .child(self.copy.empty.clone()),
                            )
                            .child(open_button),
                    )
                })
            }
        };
        let toolbar = shell::toolbar_row()
            .child(shell::brand(self.brand.clone(), self.copy.title.clone()))
            .children(mode.map(|mode| {
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(self.mode_button(
                        "mode-visual",
                        self.copy.visual_view.clone(),
                        mode == EditorMode::Visual,
                        EditorMode::Visual,
                        cx,
                    ))
                    .child(self.mode_button(
                        "mode-source",
                        self.copy.source_view.clone(),
                        mode == EditorMode::Source,
                        EditorMode::Source,
                        cx,
                    ))
            }))
            .when(document_open && !editable, |row| {
                row.child(
                    div()
                        .pl_2()
                        .text_size(px(12.0))
                        .text_color(rgb(tokens::SECONDARY))
                        .child(self.copy.readonly.clone()),
                )
            });
        let current_id = self
            .state
            .session()
            .document()
            .map(|document| document.core().document_id());
        let adopted = self
            .statistics
            .as_ref()
            .filter(|(id, _, _)| current_id.is_some_and(|current| current == id));
        let status = shell::status_row()
            .when(document_open, |row| match adopted {
                Some((_, characters, lines)) => row
                    .child(div().child(characters.clone()))
                    .child(div().child(lines.clone())),
                None => row.child(div().child(self.copy.statistics_pending.clone())),
            })
            .child(div().flex_1())
            .children(mode.map(|mode| {
                div().child(match mode {
                    EditorMode::Visual => self.copy.visual_view.clone(),
                    EditorMode::Source => self.copy.source_view.clone(),
                })
            }));
        let content = shell::content_row().child(
            div()
                .flex_1()
                .min_w_0()
                .min_h_0()
                .flex()
                .flex_col()
                .children(notice)
                .children(conflict_panel)
                .children(self.find_panel.as_ref().map(|(panel, _)| panel.clone()))
                .child(content_body),
        );
        let root = div()
            .size_full()
            .bg(rgb(tokens::SURFACE))
            .text_color(rgb(tokens::TEXT))
            .key_context(if editable {
                "M1Window M2Editor"
            } else {
                "M1Window"
            })
            .track_focus(&self.focus)
            .on_key_down(cx.listener(Self::document_key))
            .on_action(cx.listener(Self::open_document))
            .on_action(cx.listener(Self::save_document))
            .on_action(cx.listener(Self::save_as))
            .on_action(cx.listener(Self::load_disk))
            .on_action(cx.listener(Self::confirm_overwrite))
            .on_action(cx.listener(Self::retry_external_check))
            .on_action(cx.listener(Self::dismiss_conflict))
            .on_action(cx.listener(Self::cancel_save_wait_action))
            .on_action(cx.listener(Self::close_document))
            .on_action(cx.listener(Self::open_find))
            .on_action(cx.listener(|this, _: &Cut, window, cx| {
                this.dispatch_edit(EditAction::Cut, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Copy, window, cx| {
                this.dispatch_edit(EditAction::Copy, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Paste, window, cx| {
                this.dispatch_edit(EditAction::Paste, window, cx)
            }))
            .on_action(cx.listener(|this, _: &PastePlain, window, cx| {
                this.dispatch_edit(EditAction::Paste, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SelectAll, window, cx| {
                this.dispatch_edit(EditAction::SelectAll, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Undo, window, cx| {
                this.dispatch_edit(EditAction::Undo, window, cx)
            }))
            .on_action(cx.listener(|this, _: &Redo, window, cx| {
                this.dispatch_edit(EditAction::Redo, window, cx)
            }))
            .on_action(cx.listener(|this, _: &CancelNavigation, window, cx| {
                if this.close_focused_find(window, cx) || this.escape_document_input(window, cx) {
                    return;
                }
                let input_restore_cancelled = this.visual_input_restore.take().is_some();
                let navigation_cancelled = this
                    .reader
                    .as_ref()
                    .is_some_and(ReaderSurface::cancel_restore);
                if navigation_cancelled || input_restore_cancelled {
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &VisualView, window, cx| {
                this.request_view(EditorMode::Visual, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SourceView, window, cx| {
                this.request_view(EditorMode::Source, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ReadPageUp, window, cx| {
                if let Some(reader) = &this.reader {
                    reader.page(-1.0, window.viewport_size().height);
                    if let Some(evidence) = &this.evidence {
                        evidence.borrow_mut().event("scroll-key");
                    }
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ReadPageDown, window, cx| {
                if let Some(reader) = &this.reader {
                    reader.page(1.0, window.viewport_size().height);
                    if let Some(evidence) = &this.evidence {
                        evidence.borrow_mut().event("scroll-key");
                    }
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ReadStart, _, cx| {
                if let Some(reader) = &this.reader {
                    reader.start();
                    if let Some(evidence) = &this.evidence {
                        evidence.borrow_mut().event("scroll-key");
                    }
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &ReadEnd, _, cx| {
                if let Some(reader) = &this.reader {
                    reader.end();
                    if let Some(evidence) = &this.evidence {
                        evidence.borrow_mut().event("scroll-key");
                    }
                    cx.notify();
                }
            }))
            .on_action(cx.listener(|this, _: &FocusNext, window, cx| {
                this.move_window_focus(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &FocusPrevious, window, cx| {
                this.move_window_focus(false, window, cx)
            }))
            .on_action(cx.listener(Self::invalidate_font))
            .flex()
            .flex_col()
            .child(toolbar)
            // GPUI 0.3.9 采用最后注册的 handler，正文绘制时覆盖此组合借位。
            .children(self.offscreen_input(cx))
            .child(content)
            .child(status);
        #[cfg(debug_assertions)]
        let root = root.on_action(cx.listener(|this, _: &ReportLayout, _, _| {
            if let Some(reader) = &this.reader {
                reader.report();
            }
        }));
        if let Some(serial) = probe_ticket {
            let submitted = cx.entity().downgrade();
            // 锁定 GPUI 在同一 window update 中 draw 后调用 present；effect 回调
            // 因而观察到提交后的虚拟项，但不代表 GPU 完成或屏幕扫描输出。
            window.defer(cx, move |_, cx| {
                let _ = submitted.update(cx, |this, _| this.probe_frame(serial));
            });
            let laid_out = cx.entity().downgrade();
            root.on_children_prepainted(move |_, _, cx| {
                let _ = laid_out.update(cx, |this, _| this.probe_layout(serial));
            })
        } else {
            root
        }
    }
}
