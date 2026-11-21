//! 独立 10 秒恢复计时；不依赖 GPUI、不读时钟，时间由宿主注入。
//!
//! 本模块只决定“下一次恢复准备何时到期”，不冻结快照、不排队、不写盘；
//! 版本复检、资源读取与两阶段发布由协调器在到期后判定。
//! 恢复计时与 1 秒自动保存防抖互不读写状态（W4-42）。

use super::record::RECOVERY_INTERVAL_MILLIS;

/// 恢复计时结算结果；宿主只据此决定是否开始一次准备。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryAction {
    /// 到期并已消费；调用方应开始一次准备。
    Fire,
    /// 当前没有待办计时，或会话忙碌/停用。
    Idle,
    /// 尚未到期；调用方应在给定毫秒后再次唤醒。
    NotDue { remaining_millis: u64 },
}

/// 恢复计时；只保存下一次到期时刻，不读时钟、不读文档。
#[derive(Debug, Default)]
pub(crate) struct RecoveryScheduler {
    deadline: Option<u64>,
}

impl RecoveryScheduler {
    /// 建立无到期状态；不读时钟。
    pub(crate) fn new() -> Self {
        Self { deadline: None }
    }

    /// 从注入时刻无条件重置到期；每次结算后都从结算时刻重新起算。
    pub(crate) fn arm(&mut self, now: u64) {
        self.deadline = Some(now.saturating_add(RECOVERY_INTERVAL_MILLIS));
    }

    /// 返回下一次到期时刻；None 表示没有待办。
    pub(crate) fn deadline(&self) -> Option<u64> {
        self.deadline
    }

    /// 清除到期时刻；关闭、取消或停用后调用。
    pub(crate) fn clear(&mut self) {
        self.deadline = None;
    }

    /// 只读结算到期状态；到期时消费 deadline，不读时钟。
    pub(crate) fn poll(&mut self, now: u64) -> RecoveryAction {
        let Some(deadline) = self.deadline else {
            return RecoveryAction::Idle;
        };
        if now >= deadline {
            self.deadline = None;
            return RecoveryAction::Fire;
        }
        RecoveryAction::NotDue {
            remaining_millis: deadline - now,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recovery_schedule_fires_at_ten_seconds_and_consumes_deadline() {
        let mut scheduler = RecoveryScheduler::new();
        assert_eq!(scheduler.deadline(), None);
        assert_eq!(scheduler.poll(0), RecoveryAction::Idle);

        scheduler.arm(0);
        assert_eq!(scheduler.deadline(), Some(RECOVERY_INTERVAL_MILLIS));
        assert_eq!(
            scheduler.poll(9_999),
            RecoveryAction::NotDue {
                remaining_millis: 1
            }
        );
        assert_eq!(scheduler.poll(10_000), RecoveryAction::Fire);
        // 到期已被消费；再次 poll 不再触发。
        assert_eq!(scheduler.deadline(), None);
        assert_eq!(scheduler.poll(10_000), RecoveryAction::Idle);
    }

    #[test]
    fn recovery_schedule_rearm_restarts_from_injected_now() {
        let mut scheduler = RecoveryScheduler::new();
        scheduler.arm(1_000);
        assert_eq!(scheduler.deadline(), Some(11_000));
        // 结算后从结算时刻重新起算，而不是沿用旧到期。
        scheduler.arm(4_321);
        assert_eq!(scheduler.deadline(), Some(14_321));
        assert_eq!(
            scheduler.poll(14_320),
            RecoveryAction::NotDue {
                remaining_millis: 1
            }
        );
        assert_eq!(scheduler.poll(14_321), RecoveryAction::Fire);
    }

    #[test]
    fn recovery_schedule_saturates_near_u64_max() {
        let mut scheduler = RecoveryScheduler::new();
        scheduler.arm(u64::MAX - 5);
        assert_eq!(scheduler.deadline(), Some(u64::MAX));
        assert_eq!(
            scheduler.poll(u64::MAX - 1),
            RecoveryAction::NotDue {
                remaining_millis: 1
            }
        );
        assert_eq!(scheduler.poll(u64::MAX), RecoveryAction::Fire);
        assert_eq!(scheduler.deadline(), None);
    }
}
