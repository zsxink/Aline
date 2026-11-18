//! 原生单窗的打开、编辑动作及文档派生视图。
//!
//! 窗口持有一个文档会话。读取和解析刷新各保留一个在途任务及至多一个最新候选；
//! 文件选择、任务与焦点在本层装配，render 只读已采用的解析。

mod edit_actions;
mod editable_input;
mod find_panel;
mod font_invalidate;
mod geometry;
mod probe;
mod readonly_content;
mod readonly_evidence;
mod readonly_hit;
mod readonly_layout;
mod readonly_reader;
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

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use aline_app_core::Translator;
use aline_buffer::{HistoryBudget, LineIndexBudget, TextBudget};
use aline_markdown::{DiagnosticKind, ParseBudget, ParseConfig, Parsed};
use aline_source::StorageBudget;
use gpui::{
    App, Bounds, Context, Entity, FocusHandle, Image, KeyBinding, Menu, MenuItem,
    PathPromptOptions, Role, SharedString, Subscription, Task, Window, WindowBounds, WindowOptions,
    div, prelude::*, px, rgb, rgba, size,
};

use crate::document::{
    AutosaveAction, DocumentStatistics, ExternalApplyResult, ExternalCheckCommand,
    ExternalCheckCompletion, ExternalCheckIntent, ExternalCheckJob, ExternalCheckRejection,
    ExternalConflictView, ExternalDiskState, SaveCommand, SaveJob, SaveRejection, SaveResult,
    StageTimer, StageTiming, WriteCompletion,
};
use crate::fonts::{FontEnvironment, FontStatus};
use crate::{
    DocLimits, DocumentId, EditorMode, ModeSwitchOutcome, OpenDisposition, OpenError,
    OpenRequestId, ParseRefreshJob, PublicationOutcome, ReadonlyDocument, ReadonlyOpenCompletion,
    ReadonlyOpenJob, ReadonlyOpenLimits, ReadonlySession, WaitHandle,
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
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaveHost {
    /// 当前没有显式保存请求。
    Idle,
    /// 组合期内等待平台自然结束；句柄用于取消或结算。
    Waiting(WaitHandle),
    /// 已完成冻结并交给后台执行，等待结算回执。
    Running { automatic: bool },
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
    /// 本次结算是否来自自动保存；宿主据此在结算后重查自动保存。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) automatic: bool,
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
        }
    }

    pub(super) fn session(&self) -> &ReadonlySession {
        &self.session
    }

    pub(super) fn feedback_key(&self) -> Option<&'static str> {
        self.feedback
    }

    pub(super) fn take_focus_return(&mut self) -> bool {
        std::mem::take(&mut self.return_focus)
    }

    /// 请求保存；返回 Some(job) 时窗口负责后台执行。反馈已在内部设置。
    pub(super) fn request_save(&mut self) -> Option<SaveJob> {
        if matches!(
            self.save_host,
            SaveHost::Running { .. } | SaveHost::Waiting(_)
        ) {
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

    /// 取消组合期保存等待；返回是否确实取消了等待。反馈已设置。
    pub(super) fn cancel_save_wait(&mut self) -> bool {
        let SaveHost::Waiting(handle) = self.save_host else {
            return false;
        };
        // 无论文档结算结果如何都回到 Idle：句柄已不可复用，避免宿主死锁。
        self.save_host = SaveHost::Idle;
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

    /// 平台自然结束；返回 Some(job) 时窗口负责后台执行。反馈已设置。
    ///
    /// 真实平台结束回调属于 W5；当前只有 crate 测试直接调用。
    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn finish_save_wait(
        &mut self,
        outcome: PublicationOutcome,
        now_millis: u64,
    ) -> Option<SaveJob> {
        let SaveHost::Waiting(handle) = self.save_host else {
            return None;
        };
        let Some(document) = self.session.document_mut() else {
            self.save_host = SaveHost::Idle;
            self.feedback = Some("document.save_failed");
            return None;
        };
        match document.finish_save_wait_command(handle, outcome, now_millis) {
            SaveCommand::Run(job) => {
                self.save_host = SaveHost::Running { automatic: false };
                self.feedback = None;
                Some(job)
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
            return SaveSettlement {
                feedback: None,
                automatic,
            };
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
        SaveSettlement {
            feedback,
            automatic,
        }
    }

    /// 是否存在组合期保存等待（菜单取消项可用性）。
    pub(super) fn save_waiting(&self) -> bool {
        matches!(self.save_host, SaveHost::Waiting(_))
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

    pub(super) fn request_open(&mut self) -> Result<OpenRequestId, OpenError> {
        let request = self.session.request_open()?;
        self.queued = None;
        self.feedback = None;
        self.return_focus = false;
        Ok(request)
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
                self.feedback = Some("document.close_pending_changes");
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
        match self.session.close_document_with(before_close) {
            Ok(value) => {
                self.queued = None;
                self.queued_refresh = None;
                self.feedback = None;
                self.return_focus = false;
                Ok(value)
            }
            Err(crate::CloseDisposition::Refused) => {
                self.feedback = Some("document.close_pending_changes");
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

    fn install_menu(
        &self,
        cx: &App,
        can_close: bool,
        can_edit: bool,
        can_cancel_save: bool,
        exits: ConflictExits,
    ) {
        let [undo, redo, cut, copy, paste, paste_plain, select_all] = &self.edit_actions;
        cx.set_menus([
            Menu::new(self.title.clone()),
            Menu::new(self.file_menu.clone()).items([
                MenuItem::action(self.open.clone(), OpenDocument),
                MenuItem::action(self.save.clone(), SaveDocument).disabled(!can_edit),
                MenuItem::action(self.save_as.clone(), SaveAs).disabled(true),
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
    copy.install_menu(cx, false, false, false, ConflictExits::default());
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
        // `observe_window_activation` 的激活只标记 active、不立即回调；注册安全，
        // 且不做初始检查（Ruling W4-30）。
        let activation = cx.observe_window_activation(window, |this, window, cx| {
            if window.is_window_active() {
                this.check_external(ExternalCheckIntent::Focus, window, cx);
            }
        });
        state.activation_subscription = Some(activation);
        state
    }

    fn open_document(&mut self, _: &OpenDocument, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog_task.is_some() {
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
                    // 新文档接管窗口：旧文档的未完成输入与在途结果全部作废。
                    this.clear_input_state();
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

    /// 显式保存到既有目标；只写反馈与菜单，不触碰焦点。
    fn save_document(&mut self, _: &SaveDocument, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(job) = self.state.request_save() {
            self.start_save(job, window, cx);
        }
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

    /// 关闭前捕获当前核心，关闭后在后台读磁盘并只创建证据文件。
    fn close_with_evidence(&mut self, cx: &mut App) -> bool {
        let evidence = self.evidence.clone();
        let packet = match self.state.close_document_with(|document| {
            evidence
                .as_ref()
                .map(|evidence| evidence.borrow_mut().closing(document))
        }) {
            Ok(Some(Some(Ok(packet)))) => Some(packet),
            Ok(Some(Some(Err(error)))) => {
                eprintln!("m1-readonly-evidence close failed: {error}");
                None
            }
            Ok(Some(None)) | Ok(None) => None,
            Err(()) => return false,
        };
        if let Some(packet) = packet {
            let closed = self.state.session().document().is_none();
            cx.background_executor()
                .spawn(async move {
                    if packet.write(closed).is_err() {
                        eprintln!("m1-readonly-evidence write failed");
                    }
                })
                .detach();
        }
        // 关闭成功：取消待办自动保存计时，丢弃正文帧与排队输入；在途结果按文档身份被拒绝。
        self.autosave_task = None;
        if let Some(document) = self.state.session.document_mut() {
            document.cancel_autosave();
        }
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
        if document.is_editable()
            && let Some(range) = reader.input_row(document, row)
            && let Some(site) =
                editable_input::InputSite::for_row(document, range, self.source_input_site.as_ref())
            && let Some((input, frame)) =
                editable_input::render(document, site, self.focus.clone(), cx)
        {
            self.input_frame = Some(frame);
            return reader.wrap_input_row(row, input);
        }
        let failure = self
            .copy
            .errors
            .get("reading.layout_budget")
            .cloned()
            .unwrap_or_default();
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
            self.state.save_waiting(),
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
        id: &'static str,
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
                    }))
                    .on_key_down(cx.listener(
                        move |this, event: &gpui::KeyDownEvent, window, cx| {
                            if event.keystroke.key == "enter" || event.keystroke.key == "space" {
                                key(this, window, cx);
                                cx.stop_propagation();
                            }
                        },
                    ))
            })
            .child(label)
            .into_any_element()
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
                                if let Some(reader) = &this.reader {
                                    reader.cancel_restore();
                                    cx.notify();
                                }
                            }))
                            .on_key_down(cx.listener(|this, event: &gpui::KeyDownEvent, _, cx| {
                                if event.keystroke.key == "enter" || event.keystroke.key == "space"
                                {
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
            None => div()
                .flex_1()
                .min_h_0()
                .w_full()
                .flex()
                .flex_col()
                .justify_center()
                .items_center()
                .gap_3()
                .when(!document_open, |body| {
                    body.child(
                        div()
                            .text_xl()
                            .text_color(rgb(tokens::TEXT))
                            .child(self.copy.empty.clone()),
                    )
                    .child(open_button)
                }),
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
                if let Some(reader) = &this.reader
                    && reader.cancel_restore()
                {
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
