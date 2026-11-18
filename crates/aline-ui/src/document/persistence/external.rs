//! 外部磁盘观察与冲突呈现的冻结输入。只读盘并交回完成项，不修改文档状态。

use std::{path::PathBuf, sync::Arc};

use aline_buffer::SelectionSpec;
use aline_source::{
    AtomicFailureKind, Cancellation, ExistingTarget, IoBudget, SourceBaseline, StorageBudget,
    decode,
};

/// 触发外改检查的入口；决定脏文档的处置与反馈是否静默。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalCheckIntent {
    /// 窗口重新激活后的静默检查。
    Focus,
    /// 用户显式请求重试检查。
    Retry,
    /// 用户选择载入磁盘内容。
    Reload,
    /// 用户选择确认覆盖磁盘。
    Overwrite,
}

/// 宿主可辨的检查拒绝类别。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalCheckRejection {
    /// 当前文档只读或没有可写目标。
    NoTarget,
    /// 保存队列正在执行。
    Busy,
    /// 组合输入进行中，稍后再查。
    Deferred,
    /// 不确定发布待用户处理。
    Uncertain,
}

/// 窗口可命名的外改检查指令；不暴露观察细节。
pub(crate) enum ExternalCheckCommand {
    /// 已冻结检查输入，交给后台执行。
    Run(ExternalCheckJob),
    /// 请求被拒绝，按类别映射本地化反馈。
    Rejected(ExternalCheckRejection),
}

/// 后台观察的冻结输入；语义等价于一次只读“保存前观察”，但不排入保存队列。
pub(crate) struct ExternalCheckJob {
    intent: ExternalCheckIntent,
    expected: SourceBaseline,
    path: PathBuf,
    storage_budget: StorageBudget,
    io_budget: IoBudget,
}

impl ExternalCheckJob {
    /// 由会话在请求时冻结比较基线与预算；路径必须是与保存目标相同的绝对路径。
    pub(crate) fn new(
        intent: ExternalCheckIntent,
        expected: SourceBaseline,
        path: PathBuf,
        storage_budget: StorageBudget,
        io_budget: IoBudget,
    ) -> Self {
        Self {
            intent,
            expected,
            path,
            storage_budget,
            io_budget,
        }
    }

    /// 固定顺序：观察既有目标（与保存同一入口、同一预算、同一拒绝分类）→
    /// 整段 bytes 相等即 Unchanged → 否则按存储预算解码；解码失败是 Unsupported。
    pub(crate) fn run(self) -> ExternalCheckCompletion {
        // 3.3 不提供取消按钮；每次后台观察使用新的不可取消令牌。
        let cancellation = Cancellation::new();
        let observation = match ExistingTarget::observe(
            &self.path,
            self.expected.storage().version(),
            self.io_budget,
            &cancellation,
        ) {
            Ok(target) => {
                if target.storage().bytes() == self.expected.storage().bytes() {
                    ExternalObservation::Unchanged
                } else {
                    match decode(target.storage(), self.storage_budget) {
                        Ok(decoded) => ExternalObservation::Changed {
                            baseline: decoded.into_baseline(),
                        },
                        Err(_) => ExternalObservation::Unsupported,
                    }
                }
            }
            Err(failure) => match failure.kind() {
                AtomicFailureKind::Conflict {
                    target_exists: false,
                } => ExternalObservation::Missing,
                AtomicFailureKind::Conflict {
                    target_exists: true,
                } => ExternalObservation::Unstable,
                AtomicFailureKind::ExistingTarget { .. } => ExternalObservation::Unsupported,
                AtomicFailureKind::Io { .. } | AtomicFailureKind::Budget { .. } => {
                    ExternalObservation::Unreadable
                }
                AtomicFailureKind::Cancelled { .. } => ExternalObservation::Cancelled,
                _ => ExternalObservation::Failed,
            },
        };
        ExternalCheckCompletion {
            intent: self.intent,
            expected: self.expected,
            path: self.path,
            observation,
        }
    }
}

/// 观察事实；`Changed` 携带可直接采纳的新磁盘基线。
pub(crate) enum ExternalObservation {
    /// 磁盘 bytes 仍等于保存基线。
    Unchanged,
    /// 磁盘 bytes 已变化且可解码。
    Changed { baseline: SourceBaseline },
    /// 目标不存在。
    Missing,
    /// 目标存在但读取或预算失败。
    Unreadable,
    /// 目标存在但不是可解码的受支持类型。
    Unsupported,
    /// 观察期间目标内容或身份不稳定。
    Unstable,
    /// 后台观察被取消；3.3 的宿主层不可取消。
    Cancelled,
    /// 不属于其它类别的观察失败。
    Failed,
}

/// 后台完成项；会话按 `intent/expected/path` 复验后结算。
pub(crate) struct ExternalCheckCompletion {
    intent: ExternalCheckIntent,
    expected: SourceBaseline,
    path: PathBuf,
    observation: ExternalObservation,
}

impl ExternalCheckCompletion {
    /// 返回触发本次检查的入口。
    pub(crate) fn intent(&self) -> ExternalCheckIntent {
        self.intent
    }

    /// 返回请求时冻结的保存基线；身份用于复验是否仍是当前基线。
    pub(crate) fn expected(&self) -> &SourceBaseline {
        &self.expected
    }

    /// 返回请求时的目标路径；与当前目标不一致即迟到结果。
    pub(crate) fn path(&self) -> &std::path::Path {
        &self.path
    }

    /// 返回观察事实。
    pub(crate) fn observation(&self) -> &ExternalObservation {
        &self.observation
    }
}

/// 呈现层可消费的磁盘状态；`Cancelled/Failed` 不进入视图。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalDiskState {
    /// 磁盘内容已变化且可解码。
    Changed,
    /// 磁盘目标已被删除。
    Missing,
    /// 目标读取失败或观察不稳定。
    Unreadable,
    /// 目标存在但类型或编码不受支持。
    Unsupported,
    /// 观察期间目标内容或身份不稳定。
    Unstable,
}

/// 基线/本地/磁盘三份只读文本预览（各截断到 2000 字节以内并落在字符边界，不做 diff）。
/// `baseline` 是保存基线，`local` 是含草稿的当前工作内容，`disk` 只有 Changed 时存在。
#[derive(Clone, Debug)]
pub(crate) struct ExternalConflictView {
    pub(crate) state: ExternalDiskState,
    pub(crate) baseline: Arc<str>,
    pub(crate) local: Arc<str>,
    pub(crate) disk: Option<Arc<str>>,
}

/// 会话结算类别；窗口只据此选反馈键与后续动作。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExternalApplyResult {
    /// 已重载并采纳新基线（Reload 历史边界）。
    Reloaded,
    /// 磁盘仍等于保存基线；清视图与暂停。
    Unchanged,
    /// 已采纳用户确认的磁盘观察为基线；下一步走既有保存。
    OverwriteReady,
    /// 已呈现冲突；观察事实对应此状态。
    Presented(ExternalDiskState),
    /// 组合活动；零副作用。
    Deferred,
    /// 迟到/路径或基线已变/在途保存；零副作用。
    Stale,
    /// 检查或重载失败；已保留内存内容。
    Failed,
}

const CONFLICT_PREVIEW_BYTES: usize = 2000;

/// 截断到字符边界；预览只用于呈现，不参与相等或脏判定。
pub(crate) fn preview(text: &str) -> Arc<str> {
    if text.len() <= CONFLICT_PREVIEW_BYTES {
        return Arc::from(text);
    }
    let mut end = CONFLICT_PREVIEW_BYTES;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    Arc::from(&text[..end])
}

/// 两个端点各自 clamp 到新文本长度，并回退到字符边界。
pub(crate) fn clamped_selection(current: SelectionSpec, new_text: &str) -> SelectionSpec {
    let clamp = |offset: usize| {
        let mut offset = offset.min(new_text.len());
        while !new_text.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    };
    SelectionSpec::new(clamp(current.anchor()), clamp(current.head()))
}
