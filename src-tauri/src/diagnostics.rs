//! Opt-in export of bounded, typed measurements. Never includes raw logs,
//! device addresses/names, TXT data, credentials or arbitrary error strings.
use cap_core::{
    stream_metrics::StreamMetrics,
    streaming::{LiveDiagnosticsHandle, LiveFrameSender, LocalBufferPolicy},
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

pub type SessionRecord = Arc<Mutex<Record>>;

const FALLBACK_WINDOW_MS: u64 = 30_000;
fn recent_underruns(now_ms: u64, times: impl Iterator<Item = u64>) -> usize {
    times.filter(|&at| at <= now_ms && now_ms - at <= FALLBACK_WINDOW_MS).count()
}
fn underruns_unstable(total: u64, now_ms: u64, times: &[u64]) -> bool {
    // Preserve protection for a legacy transport with no event timeline.
    total >= 3 && (times.is_empty() || recent_underruns(now_ms, times.iter().copied()) >= 3)
}

/// Export only our typed, privacy-filtered report; no raw log attachment.
pub fn save_report(dir: &std::path::Path, report: &serde_json::Value) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e| e.to_string())?.as_millis();
    let body = serde_json::to_vec_pretty(report).map_err(|e| e.to_string())?;
    for _ in 0..16 {
        let nonce = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = dir.join(format!("DeskAudio-Cast-diagnostics-{stamp}-{nonce}.json"));
        match std::fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&body).and_then(|_| file.sync_all()) {
                    drop(file); let _ = std::fs::remove_file(&path); return Err(error.to_string());
                }
                return Ok(path);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.to_string()),
        }
    }
    Err("diagnostics_unique_path_failed".into())
}

#[cfg(test)] mod export_tests {
    #[test] fn export_exists_is_valid_and_does_not_overwrite() {
        let dir = std::env::temp_dir().join(format!("desk-audio-export-test-{}", std::process::id()));
        let report = serde_json::json!({"sessions": [], "schema_version":1});
        let first = super::save_report(&dir, &report).unwrap();
        let second = super::save_report(&dir, &report).unwrap();
        assert_ne!(first, second);
        for path in [first, second] {
            assert_eq!(serde_json::from_slice::<serde_json::Value>(&std::fs::read(&path).unwrap()).unwrap(), report);
            std::fs::remove_file(path).unwrap();
        }
        // Never recursively remove a computed temporary directory.
        let _ = std::fs::remove_dir(dir);
    }
}

struct Output {
    sender: LiveDiagnosticsHandle,
    metrics: Arc<StreamMetrics>,
    transport_scope: &'static str,
}

pub struct PreparationGuard(SessionRecord);
impl PreparationGuard {
    pub fn new(record: SessionRecord) -> Self {
        Self(record)
    }
}
impl Drop for PreparationGuard {
    fn drop(&mut self) {
        let mut record = self.0.lock().unwrap();
        if record.status == "preparing" {
            record.failed();
        }
    }
}

pub struct Record {
    began: Instant,
    ended: Option<Instant>,
    status: &'static str,
    last_failure_stage: Option<&'static str>,
    requested_latency_ms: u32,
    policy: LocalBufferPolicy,
    capture_start_ms: f64,
    capture: audio_capture::CaptureDiagnostics,
    forwarded: u64,
    rejected: u64,
    max_capture_gap_ms: f64,
    outputs: Vec<Output>,
    fallback_trigger:Option<serde_json::Value>,
}

impl Record {
    pub fn add_output(&mut self, sender: LiveFrameSender, metrics: Arc<StreamMetrics>) {
        self.outputs.push(Output {
            sender: sender.diagnostics_handle(),
            metrics,
            transport_scope: "receiver",
        });
    }
    pub fn add_group_output(&mut self, sender: LiveFrameSender, metrics: Arc<StreamMetrics>) {
        self.outputs.push(Output {
            sender: sender.diagnostics_handle(),
            metrics,
            transport_scope: "group",
        });
    }
    pub fn capture_started(&mut self, elapsed: std::time::Duration) {
        self.capture_start_ms = elapsed.as_secs_f64() * 1000.0;
    }
    pub fn update(
        &mut self,
        capture: audio_capture::CaptureDiagnostics,
        forwarded: u64,
        rejected: u64,
        gap: std::time::Duration,
    ) {
        self.capture = capture;
        self.forwarded = forwarded;
        self.rejected = rejected;
        self.max_capture_gap_ms = self.max_capture_gap_ms.max(gap.as_secs_f64() * 1000.0);
        tracing::info!(target: "desk_audio_metrics",
            queued_capture_ms = self.capture.queued_ms,
            capture_dropped_oldest = self.capture.dropped_oldest,
            capture_dropped_newest = self.capture.dropped_newest,
            capture_stale_drops = self.capture.stale_drops,
            max_capture_delivery_age_ms = self.capture.max_delivery_age_ms,
            "capture queue diagnostics"
        );
        for (index, output) in self.outputs.iter().enumerate() {
            let live = output.sender.snapshot();
            let (_, _, feedback_failures) = output.metrics.snapshot();
            let retransmit = output.metrics.retransmit_snapshot();
            tracing::info!(target: "desk_audio_metrics",
                receiver = index + 1,
                input_queued_blocks = live.queued_blocks,
                input_queue_drops = live.queue_drops,
                stale_drops = live.stale_drops,
                encoder_buffer_ms = live.encoder_buffer_ms,
                encoder_underruns = live.underruns,
                capture_delivery_to_udp_p95_ms =
                    live.capture_to_send_p95_us.map(|v| v as f64 / 1000.0),
                packets_sent = live.packets_sent,
                send_errors = live.send_errors,
                feedback_failures,
                retransmit_requested_packets = retransmit.map(|v| v.0),
                retransmit_fulfilled_packets = retransmit.map(|v| v.1),
                "receiver audio diagnostics"
            );
        }
    }
    pub fn send_errors(&self) -> u64 {
        self.outputs
            .iter()
            .map(|o| o.sender.snapshot().send_errors)
            .sum()
    }
    pub fn needs_balanced_fallback(&self) -> bool {
        self.policy == LocalBufferPolicy::LowLatency
            && self.outputs.iter().any(|output| {
                let live = output.sender.snapshot();
                let times: Vec<_> = live.underrun_events.iter().map(|e| e.elapsed_ms).collect();
                underruns_unstable(live.underruns, live.elapsed_ms, &times)
                    || live.queue_drops > 0 || live.stale_drops > 0
            })
    }
    pub fn streaming(&mut self) {
        self.status = "streaming";
    }
    pub fn capture_fallback_trigger(&mut self){
        if self.fallback_trigger.is_none(){
            self.fallback_trigger=Some(serde_json::json!({"elapsed_ms":self.began.elapsed().as_millis() as u64,
                "rule":"recent_underruns_gte_3_or_any_queue_or_stale_drop","window_ms":FALLBACK_WINDOW_MS,"monitor_interval_ms":2000,
                "outputs":self.outputs.iter().map(|o|{let s=o.sender.snapshot();serde_json::json!({"underruns":s.underruns,"live_elapsed_ms":s.elapsed_ms,"recent_underruns":recent_underruns(s.elapsed_ms,s.underrun_events.iter().map(|e|e.elapsed_ms)),"queue_drops":s.queue_drops,"stale_drops":s.stale_drops})}).collect::<Vec<_>>() }));
        }
    }
    pub fn record_failure(&mut self, stage: &'static str) {
        self.last_failure_stage = Some(stage);
    }
    pub fn failed(&mut self) {
        self.status = "failed";
        self.ended.get_or_insert(Instant::now());
    }
    pub fn finish(&mut self) {
        if self.status == "preparing" {
            self.failed();
        } else if self.status == "streaming" {
            self.status = "stopped";
            self.ended = Some(Instant::now());
        }
    }
    fn snapshot(&self) -> serde_json::Value {
        let local_options = self.policy.options();
        let receiver_range = cap_core::streaming::latency_frames(self.requested_latency_ms).ok();
        let outputs: Vec<_> = self.outputs.iter().enumerate().map(|(index, output)| {
            let s = output.sender.snapshot();
            let (timings, successes, failures) = output.metrics.snapshot();
            let retransmit = output.metrics.retransmit_snapshot();
            serde_json::json!({
                "receiver": index + 1, "transport_scope": output.transport_scope, "timings": timings,
                "feedback_successes": successes, "feedback_failures": failures,
                "retransmit_requested_packets": retransmit.map(|v| v.0),
                "retransmit_fulfilled_packets": retransmit.map(|v| v.1),
                "input_queued_blocks": s.queued_blocks, "input_queue_drops": s.queue_drops,
                "stale_drops": s.stale_drops, "packets_sent": s.packets_sent, "send_errors": s.send_errors,
                "encoder_buffer_ms": s.encoder_buffer_ms, "encoder_underruns": s.underruns, "max_input_queue_age_ms": s.max_queue_age_us as f64 / 1000.0,
                "capture_delivery_to_udp_p95_ms": s.capture_to_send_p95_us.map(|v| v as f64 / 1000.0),
                "first_packet_age_ms": s.first_packet_age_us.map(|v| v as f64 / 1000.0),
                "max_encode_us":s.max_encode_us,"max_wake_late_us":s.max_wake_late_us,"live_elapsed_ms":s.elapsed_ms,
                "underrun_events":s.underrun_events.iter().map(|e|serde_json::json!({
                    "live_elapsed_ms":e.elapsed_ms,"producer_blocks":e.producer_blocks,"decoder_blocks":e.decoder_blocks,
                    "input_queued_blocks":e.queued_blocks,"last_supply_gap_us":e.last_supply_gap_us,"last_decode_gap_us":e.last_decode_gap_us,
                    "previous_encode_us":e.previous_encode_us,"previous_wake_late_us":e.previous_wake_late_us})).collect::<Vec<_>>(),
            })
        }).collect();
        serde_json::json!({
            "status": self.status, "last_failure_stage": self.last_failure_stage, "session_duration_ms": self.ended.unwrap_or_else(Instant::now).saturating_duration_since(self.began).as_millis() as u64,
            "audio_format": { "sample_rate": 44_100, "channels": 2, "sample_bits": 16, "codec": "alac", "timing": "ntp" },
            "requested_receiver_latency_ms": self.requested_latency_ms, "local_buffer_policy": self.policy,
            "requested_receiver_min_frames": receiver_range.map(|v| v.0),
            "requested_receiver_max_frames": receiver_range.map(|v| v.1),
            "local_buffer_settings": {
                "encoder_capacity_ms": local_options.buffer_ms,
                "startup_ms": local_options.startup_ms,
                "refill_ms": local_options.refill_ms,
                "resume_ms": local_options.resume_ms,
                "input_capacity_blocks": self.policy.input_capacity(),
                "sender_capacity_packets": local_options.sender_capacity,
                "age_expiry_ms": local_options.max_age.map(|v| v.as_millis() as u64),
            },
            "capture_start_ms": self.capture_start_ms, "capture": self.capture,
            "blocks_forwarded": self.forwarded, "blocks_rejected": self.rejected,
            "max_capture_gap_ms": self.max_capture_gap_ms, "outputs": outputs,
            "fallback_trigger":self.fallback_trigger,
            "underrun_timeline_scope":"latest 64 events per output; relative to decoder creation; numeric metadata only, not acoustic measurement",
        })
    }
}

#[derive(Default)]
pub struct DiagnosticState {
    sessions: Mutex<VecDeque<SessionRecord>>,
}
impl DiagnosticState {
    pub fn begin(&self, latency: u32, policy: LocalBufferPolicy) -> SessionRecord {
        let record = Arc::new(Mutex::new(Record {
            began: Instant::now(),
            ended: None,
            status: "preparing",
            last_failure_stage: None,
            requested_latency_ms: latency,
            policy,
            capture_start_ms: 0.0,
            capture: Default::default(),
            forwarded: 0,
            rejected: 0,
            max_capture_gap_ms: 0.0,
            outputs: Vec::new(),
            fallback_trigger:None,
        }));
        let mut sessions = self.sessions.lock().unwrap();
        if sessions.len() == 6 {
            sessions.pop_front();
        }
        sessions.push_back(record.clone());
        record
    }
    pub fn report(&self) -> serde_json::Value {
        let sessions: Vec<_> = self
            .sessions
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.lock().unwrap().snapshot())
            .collect();
        serde_json::json!({
            "schema_version": 3, "app_version": env!("CARGO_PKG_VERSION"),
            "build_revision": option_env!("DESKAUDIO_BUILD_REVISION"),
            "source_fingerprint": option_env!("DESKAUDIO_SOURCE_FINGERPRINT"),
            "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
            "measurement": "software capture delivery to local UDP send; excludes audio-engine, receiver and acoustic delay",
            "sessions": sessions,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_incidents_over_twenty_five_hours_are_not_current_instability() {
        // Actual latest diagnostic, three separate incidents across ~25h.
        let events = [5_859_361, 80_702_891, 89_599_858];
        assert!(!underruns_unstable(3, 89_602_438, &events));
        assert_eq!(recent_underruns(89_602_438, events.into_iter()), 1);
    }
    #[test]
    fn genuine_burst_still_falls_back_and_old_events_expire() {
        let events = [1000, 1100, 1200];
        assert!(underruns_unstable(3, 2000, &events));
        assert!(!underruns_unstable(3, 31_201, &events));
        assert!(underruns_unstable(3, 2000, &[]));
        assert!(!underruns_unstable(2, 2000, &events));
        assert_eq!(recent_underruns(1000, [1001, 999].into_iter()), 1);
    }
    #[test]
    fn only_experimental_mode_falls_back_after_real_queue_overflow() {
        let state = DiagnosticState::default();
        for policy in [
            LocalBufferPolicy::LowLatency,
            LocalBufferPolicy::Balanced,
            LocalBufferPolicy::Stable,
        ] {
            let record = state.begin(500, policy);
            let (sender, _decoder) =
                cap_core::streaming::LiveAudioDecoder::create_pair_with_max_age(44_100, 2, 1, None);
            record
                .lock()
                .unwrap()
                .add_output(sender.clone(), Arc::new(StreamMetrics::default()));
            assert!(!record.lock().unwrap().needs_balanced_fallback());
            let frame = || cap_core::streaming::LivePcmFrame {
                samples: vec![0; 704],
                channels: 2,
                sample_rate: 44_100,
            };
            assert!(sender.try_send(frame()));
            assert!(!sender.try_send(frame()));
            assert_eq!(
                record.lock().unwrap().needs_balanced_fallback(),
                policy == LocalBufferPolicy::LowLatency
            );
        }
    }
    #[test]
    fn history_is_bounded_and_export_only_contains_typed_metrics() {
        let state = DiagnosticState::default();
        for _ in 0..10 {
            state
                .begin(0, LocalBufferPolicy::LowLatency)
                .lock()
                .unwrap()
                .finish();
        }
        let report = state.report();
        assert_eq!(report["sessions"].as_array().unwrap().len(), 6);
        assert_eq!(report["sessions"][0]["status"], "failed");
        let json = report.to_string();
        for field in [
            "password",
            "device_name",
            "address",
            "public_key",
            "raw_log",
        ] {
            assert!(!json.contains(field));
        }
    }
    #[test]
    fn fallback_decision_is_recorded_before_shutdown_without_changing_policy(){
        let state=DiagnosticState::default();let record=state.begin(500,LocalBufferPolicy::LowLatency);
        let mut r=record.lock().unwrap();assert!(!r.needs_balanced_fallback());
        r.capture_fallback_trigger();let first=r.fallback_trigger.clone();r.capture_fallback_trigger();
        assert_eq!(r.fallback_trigger,first);assert_eq!(r.policy,LocalBufferPolicy::LowLatency);
        assert_eq!(r.snapshot()["fallback_trigger"]["monitor_interval_ms"],2000);
        assert!(r.snapshot()["underrun_timeline_scope"].as_str().unwrap().contains("numeric"));
    }
}
