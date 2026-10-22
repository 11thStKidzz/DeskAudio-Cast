//! Per-session timings and health counters; intentionally contains no identities.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};

#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct StreamTimings {
    pub connect_pair_ms: f64,
    pub setup_ms: f64,
    pub initial_volume_ms: f64,
    pub start_ms: f64,
    pub ready_ms: f64,
}

pub struct StreamMetrics {
    began: Instant,
    timings: Mutex<StreamTimings>,
    feedback_failures: AtomicU64,
    feedback_successes: AtomicU64,
    consecutive_failures: AtomicU64,
    transport: Mutex<Option<std::sync::Arc<ap2rs_client::StreamStats>>>,
}

impl Default for StreamMetrics {
    fn default() -> Self {
        Self {
            began: Instant::now(),
            timings: Mutex::new(StreamTimings::default()),
            feedback_failures: AtomicU64::new(0),
            feedback_successes: AtomicU64::new(0),
            consecutive_failures: AtomicU64::new(0),
            transport: Mutex::new(None),
        }
    }
}
impl StreamMetrics {
    pub(crate) fn attach_transport(&self, stats: std::sync::Arc<ap2rs_client::StreamStats>) {
        *self.transport.lock().unwrap() = Some(stats);
    }

    /// Counts requests and locally fulfilled retransmissions, not confirmed
    /// receiver playback or a unique packet-loss percentage.
    pub fn retransmit_snapshot(&self) -> Option<(u64, u64)> {
        self.transport.lock().unwrap().as_ref().map(|stats| {
            let snapshot = stats.snapshot();
            (snapshot.rtx_requested, snapshot.rtx_fulfilled)
        })
    }
    pub(crate) fn stage(&self, stage: &str, elapsed: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.0;
        let mut timings = self.timings.lock().unwrap();
        match stage {
            "connect_pair" => timings.connect_pair_ms = ms,
            "setup" => timings.setup_ms = ms,
            "initial_volume" => timings.initial_volume_ms = ms,
            "start" => {
                timings.start_ms = ms;
                timings.ready_ms = self.began.elapsed().as_secs_f64() * 1000.0;
            }
            _ => return,
        }
        tracing::info!(stage, elapsed_ms = ms, "AirPlay session timing");
    }
    pub(crate) fn feedback(&self, success: bool) {
        if success {
            self.consecutive_failures.store(0, Ordering::Relaxed);
            self.feedback_successes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.consecutive_failures.fetch_add(1, Ordering::Relaxed);
            self.feedback_failures.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn unhealthy(&self) -> bool {
        self.consecutive_failures.load(Ordering::Relaxed) >= 3
    }
    pub fn snapshot(&self) -> (StreamTimings, u64, u64) {
        (
            self.timings.lock().unwrap().clone(),
            self.feedback_successes.load(Ordering::Relaxed),
            self.feedback_failures.load(Ordering::Relaxed),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn retransmit_counts_distinguish_unavailable_from_zero_and_follow_live_stats() {
        let metrics = StreamMetrics::default();
        assert_eq!(metrics.retransmit_snapshot(), None);
        let stats = ap2rs_client::StreamStats::new();
        metrics.attach_transport(stats.clone());
        assert_eq!(metrics.retransmit_snapshot(), Some((0, 0)));
        stats.rtx_requested.fetch_add(7, Ordering::Relaxed);
        stats.rtx_fulfilled.fetch_add(5, Ordering::Relaxed);
        assert_eq!(metrics.retransmit_snapshot(), Some((7, 5)));
    }
}
