//! 显式保存/另存/导出的发布等待纯状态。
//!
//! 等待对象只记录种类、句柄与取消状态，不执行保存、导出、磁盘写入或任务
//! 调度；平台自然结束由宿主调用 `finish_publication` 转交结果。

use crate::document::snapshot::CurrentSnapshot;

/// 显式发布的种类；三种种类共用同一等待状态机。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationKind {
    /// 保存到既有目标。
    Save,
    /// 另存为新目标。
    SaveAs,
    /// 导出为其他格式。
    Export,
}

/// 平台自然结束的方式。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PublicationOutcome {
    /// 平台确认完成。
    PlatformConfirmed,
    /// 平台侧取消完成。
    PlatformCancelled,
}

/// 本次发布等待的句柄；只由 `begin_publication_wait` 发放。
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaitHandle(u64);

impl WaitHandle {
    /// 构造等待句柄；仅由文档核心使用。
    pub(crate) const fn new(value: u64) -> Self {
        Self(value)
    }

    /// 返回句柄的单调编号，便于日志与断言区分等待。
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// 等待状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaitStatus {
    /// 等待平台自然结束。
    Pending,
    /// 宿主已请求取消发布，但组合与输入不受影响。
    Cancelled,
}

/// 等待的只读视图。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WaitView {
    handle: WaitHandle,
    kind: PublicationKind,
    status: WaitStatus,
}

impl WaitView {
    /// 构造等待视图；仅由文档核心使用。
    pub(crate) const fn new(handle: WaitHandle, kind: PublicationKind, status: WaitStatus) -> Self {
        Self {
            handle,
            kind,
            status,
        }
    }

    /// 返回等待句柄。
    pub const fn handle(self) -> WaitHandle {
        self.handle
    }

    /// 返回发布种类。
    pub const fn kind(self) -> PublicationKind {
        self.kind
    }

    /// 返回等待状态。
    pub const fn status(self) -> WaitStatus {
        self.status
    }
}

/// 平台自然结束后交给宿主的当前快照。
#[derive(Clone, Debug)]
pub struct PublicationResult {
    kind: PublicationKind,
    outcome: PublicationOutcome,
    snapshot: CurrentSnapshot,
}

impl PublicationResult {
    /// 构造发布结果；仅由文档核心使用。
    pub(crate) fn new(
        kind: PublicationKind,
        outcome: PublicationOutcome,
        snapshot: CurrentSnapshot,
    ) -> Self {
        Self {
            kind,
            outcome,
            snapshot,
        }
    }

    /// 返回发布种类。
    pub fn kind(&self) -> PublicationKind {
        self.kind
    }

    /// 返回平台结束方式。
    pub fn outcome(&self) -> PublicationOutcome {
        self.outcome
    }

    /// 返回自然结束后的当前快照；发布本身不新增历史步、不封口、不清 redo。
    pub fn snapshot(&self) -> &CurrentSnapshot {
        &self.snapshot
    }
}

/// 内部等待状态。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct WaitState {
    pub(crate) handle: WaitHandle,
    pub(crate) kind: PublicationKind,
    pub(crate) status: WaitStatus,
}
