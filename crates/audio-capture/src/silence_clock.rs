//! Absolute sample-count clock for synthetic silence. No sleeps or audio I/O.
use std::time::{Duration, Instant};

pub(crate) struct SilenceClock {
    origin: Instant,
    next_block: u64,
    frames: u32,
    rate: u32,
}

impl SilenceClock {
    pub(crate) fn new(origin: Instant, frames: u32, rate: u32) -> Self {
        assert!(frames > 0 && rate > 0);
        Self {
            origin,
            next_block: 0,
            frames,
            rate,
        }
    }

    pub(crate) fn next_deadline(&self) -> Instant {
        let nanos = (self.next_block as u128 * self.frames as u128 * 1_000_000_000)
            .div_ceil(self.rate as u128);
        self.origin + Duration::from_nanos(nanos as u64)
    }

    /// Consume elapsed slots even when backpressured. Return (emit, skipped).
    /// Ordinary late wakes catch up within capacity; long stalls never flood
    /// the queue with seconds of obsolete silence. Never evicts real audio.
    pub(crate) fn take_due(&mut self, now: Instant, capacity: usize) -> (usize, u64) {
        if now < self.origin {
            return (0, 0);
        }
        let elapsed = now.duration_since(self.origin).as_nanos();
        let total =
            (elapsed * self.rate as u128 / (self.frames as u128 * 1_000_000_000)) as u64 + 1;
        let due = total.saturating_sub(self.next_block);
        self.next_block = self.next_block.max(total);
        let emit = due.min(capacity as u64) as usize;
        (emit, due - emit as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn late_wakes_do_not_accumulate_sample_clock_drift() {
        let origin = Instant::now();
        let mut clock = SilenceClock::new(origin, 352, 44_100);
        let mut sent = 0;
        // Simulate ~16ms Windows wakeups for 60s without actually waiting.
        for tick in 0..=3750 {
            let (due, skipped) = clock.take_due(origin + Duration::from_millis(tick * 16), 4);
            sent += due;
            assert_eq!(skipped, 0);
        }
        assert_eq!(sent, 60 * 44_100 / 352 + 1);
        assert!(clock.next_deadline() > origin + Duration::from_secs(60));
    }

    #[test]
    fn long_stall_is_bounded_and_does_not_replay_old_silence() {
        let origin = Instant::now();
        let mut clock = SilenceClock::new(origin, 352, 44_100);
        let now = origin + Duration::from_secs(10);
        let (sent, skipped) = clock.take_due(now, 2);
        assert_eq!(sent, 2);
        assert!(skipped > 1000);
        assert_eq!(clock.take_due(now, 2), (0, 0));
        assert!(clock.next_deadline() > now);
    }

    #[test]
    fn backpressure_consumes_slots_without_eviction() {
        let origin = Instant::now();
        let mut clock = SilenceClock::new(origin, 352, 44_100);
        assert_eq!(clock.take_due(origin, 0), (0, 1));
        assert_eq!(clock.take_due(origin, 2), (0, 0));
    }

    #[test]
    fn no_early_silence_and_restart_has_no_historical_debt() {
        let origin = Instant::now();
        let mut clock = SilenceClock::new(origin + Duration::from_millis(100), 352, 44_100);
        assert_eq!(clock.take_due(origin, 2), (0, 0));
        let resume = origin + Duration::from_secs(100);
        clock = SilenceClock::new(resume, 352, 44_100);
        assert_eq!(clock.take_due(resume, 2), (1, 0));
    }

    #[test]
    fn fractional_packet_duration_does_not_round_to_eight_ms() {
        let origin = Instant::now();
        let mut clock = SilenceClock::new(origin, 352, 44_100);
        let (count, skipped) = clock.take_due(origin + Duration::from_secs(3600), usize::MAX);
        assert_eq!(count as u64, 3600 * 44_100 / 352 + 1);
        assert_eq!(skipped, 0);
    }
}
