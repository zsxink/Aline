//! 恢复任务槽：只记录计时任务与工作任务的代次归属，不依赖 GPUI。
//!
//! 窗口持有真实 `Task` 句柄，本类型只回答三件事：现在能否布防、新任务应取
//! 哪个代次、一次完成回调是否仍拥有槽。替换文档或关闭时 `reset` 使所有旧
//! 代次失效，迟到回调不得清除新任务（W4-55）。

/// 一次恢复任务的代次身份；只有当前槽仍持有同一身份才算拥有。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryTaskId(u64);

/// 计时槽与工作槽的代次归属；不保存 `Task`、不读时钟。
#[derive(Debug, Default)]
pub(crate) struct RecoverySlot {
    next: u64,
    timer: Option<RecoveryTaskId>,
    work: Option<RecoveryTaskId>,
}

impl RecoverySlot {
    /// 建立空槽；不分配代次。
    pub(crate) const fn new() -> Self {
        Self {
            next: 0,
            timer: None,
            work: None,
        }
    }

    /// 分配单调递增的代次；耗尽时返回 None，调用方放弃本次任务。
    fn allocate(&mut self) -> Option<RecoveryTaskId> {
        let next = self.next.checked_add(1)?;
        self.next = next;
        Some(RecoveryTaskId(next))
    }

    /// 仅当计时槽与工作槽都空时才允许布防。
    pub(crate) const fn can_arm(&self) -> bool {
        self.timer.is_none() && self.work.is_none()
    }

    /// 占用计时槽；已有任务时返回 None。
    pub(crate) fn begin_timer(&mut self) -> Option<RecoveryTaskId> {
        if !self.can_arm() {
            return None;
        }
        let id = self.allocate()?;
        self.timer = Some(id);
        Some(id)
    }

    /// 计时到点后占用工作槽；工作槽已占用时返回 None。
    pub(crate) fn begin_work(&mut self) -> Option<RecoveryTaskId> {
        if self.work.is_some() {
            return None;
        }
        let id = self.allocate()?;
        self.work = Some(id);
        Some(id)
    }

    /// 计时完成：仅当前代次拥有槽时释放并返回 true。
    pub(crate) fn finish_timer(&mut self, id: RecoveryTaskId) -> bool {
        if self.timer == Some(id) {
            self.timer = None;
            true
        } else {
            false
        }
    }

    /// 工作完成：仅当前代次拥有槽时释放并返回 true。
    pub(crate) fn finish_work(&mut self, id: RecoveryTaskId) -> bool {
        if self.work == Some(id) {
            self.work = None;
            true
        } else {
            false
        }
    }

    /// 替换文档或关闭：使所有旧代次失效；迟到回调随后全部被拒绝。
    pub(crate) fn reset(&mut self) {
        self.timer = None;
        self.work = None;
    }
}
