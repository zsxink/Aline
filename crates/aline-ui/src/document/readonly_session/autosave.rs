//! 自动保存防抖的纯计时状态机；不依赖 GPUI、不读时钟，时间由调用方注入。
//!
//! 本模块只决定“何时适合请求一次自动保存”，不冻结快照、不排队、不写盘；
//! 保存条件与队列状态由调用方在会话层判定。

/// 自动保存防抖窗口；规格固定为停止输入 1 秒。
pub(crate) const AUTOSAVE_DEBOUNCE_MILLIS: u64 = 1_000;

/// 自动保存计时结算结果；宿主只据此决定是否走既有保存入口。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AutosaveAction {
    /// 到期且条件齐备；调用方应走既有保存入口。
    Fire,
    /// 当前没有待办计时。
    Idle,
    /// 尚未到期；调用方应在给定毫秒后再次唤醒。
    NotDue { remaining_millis: u64 },
    /// 组合活动；由组合结束后的输入结算重新布防。
    DeferredComposition,
    /// 冲突或重载撤销暂停。
    Paused,
    /// 只读或无保存目标；未命名文档绝不自动选路径。
    NoTarget,
    /// 没有本地修改。
    NotDirty,
    /// 保存队列忙或处于不确定发布。
    Busy,
}

/// 防抖计时；只保存下一次到期时刻，不读时钟、不读文档。
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct AutosaveScheduler {
    deadline: Option<u64>,
}

impl AutosaveScheduler {
    /// 记录一次输入/内容活动。组合活动时清除到期时刻，等待组合结束后的新活动。
    pub(crate) fn note_activity(&mut self, now_millis: u64, composing: bool) {
        self.deadline = if composing {
            None
        } else {
            Some(now_millis.saturating_add(AUTOSAVE_DEBOUNCE_MILLIS))
        };
    }

    pub(crate) fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    pub(crate) fn clear(&mut self) {
        self.deadline = None;
    }
}
