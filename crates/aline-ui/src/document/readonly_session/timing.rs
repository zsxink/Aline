//! 只读打开的验证计时：真实墙钟边界与单调耗时分别捕获，不参与业务判定。
//!
//! 状态只存在于一次受测调用栈；调用者消费完成值，不能从稍后的采纳时刻回推后台时间。
//! 墙钟和单调时钟可能具有不同分辨率或被分别校正，不能强求两者的纳秒差值相等。
//! 主耗时和起止边界一律取同一 Instant 时钟；墙钟仅用于关联外部运行时间线。

use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StageTiming {
    pub(crate) started_epoch_ns: u128,
    pub(crate) finished_epoch_ns: u128,
    pub(crate) duration_ns: u128,
    pub(crate) clock_uncertainty_ns: u128,
    started: Option<Instant>,
    finished: Option<Instant>,
}

impl StageTiming {
    /// 返回相对同一探针原点的真实单调边界；原点晚于本段或默认失败值时拒绝。
    pub(crate) fn monotonic_bounds(self, origin: Instant) -> Option<(u128, u128)> {
        Some((
            self.started?.checked_duration_since(origin)?.as_nanos(),
            self.finished?.checked_duration_since(origin)?.as_nanos(),
        ))
    }
}

pub(crate) struct StageTimer {
    started: Instant,
    started_epoch_ns: u128,
    start_capture_ns: u128,
}

impl StageTimer {
    pub(crate) fn start() -> Self {
        let started = Instant::now();
        let started_epoch_ns = epoch_ns();
        let start_capture_ns = started.elapsed().as_nanos();
        Self {
            started,
            started_epoch_ns,
            start_capture_ns,
        }
    }

    pub(crate) fn finish(self) -> StageTiming {
        let finished = Instant::now();
        let finished_epoch_ns = epoch_ns();
        let end_capture_ns = finished.elapsed().as_nanos();
        StageTiming {
            started_epoch_ns: self.started_epoch_ns,
            finished_epoch_ns,
            duration_ns: finished.duration_since(self.started).as_nanos(),
            clock_uncertainty_ns: self.start_capture_ns.saturating_add(end_capture_ns),
            started: Some(self.started),
            finished: Some(finished),
        }
    }
}

pub(crate) fn epoch_ns() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn real_monotonic_boundaries_equal_duration_without_assuming_wall_clock_resolution()
    -> Result<(), Box<dyn std::error::Error>> {
        let origin = Instant::now();
        for _ in 0..10_000 {
            let timing = StageTimer::start().finish();
            let (started, finished) = timing.monotonic_bounds(origin).ok_or("missing bounds")?;
            assert_eq!(finished - started, timing.duration_ns);
        }
        Ok(())
    }

    #[test]
    fn quantized_wall_clock_is_not_used_to_reconstruct_monotonic_boundaries()
    -> Result<(), Box<dyn std::error::Error>> {
        // macOS 实测 CLOCK_REALTIME 为 1 µs，CLOCK_UPTIME_RAW 约 42 ns。
        // 固定反例：真实单调 100 ns 可以跨越墙钟的 1000 ns 刻度；二者均保留。
        let origin = Instant::now();
        let started = origin
            .checked_add(Duration::from_nanos(10))
            .ok_or("start overflow")?;
        let finished = origin
            .checked_add(Duration::from_nanos(110))
            .ok_or("end overflow")?;
        let timing = StageTiming {
            started_epoch_ns: 1000,
            finished_epoch_ns: 2000,
            duration_ns: 100,
            clock_uncertainty_ns: 84,
            started: Some(started),
            finished: Some(finished),
        };
        assert_eq!(timing.monotonic_bounds(origin), Some((10, 110)));
        assert_eq!(timing.finished_epoch_ns - timing.started_epoch_ns, 1000);
        assert_ne!(
            timing.finished_epoch_ns - timing.started_epoch_ns,
            timing.duration_ns
        );
        assert!(timing.monotonic_bounds(finished).is_none());
        assert!(StageTiming::default().monotonic_bounds(origin).is_none());
        Ok(())
    }
}
