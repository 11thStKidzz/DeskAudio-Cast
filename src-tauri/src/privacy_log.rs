//! Closed allowlist: arbitrary errors, protocol packets and device data never reach a log sink.
//! Do not expand this list without inspecting every field at the target's callsites.
pub const METRICS_TARGET: &str = "desk_audio_metrics";

pub fn allowed(metadata: &tracing::Metadata<'_>) -> bool {
    metadata.target() == METRICS_TARGET
        && matches!(*metadata.level(), tracing::Level::INFO | tracing::Level::WARN | tracing::Level::ERROR)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use std::io::Write;
    use tracing_subscriber::layer::SubscriberExt;
    #[derive(Clone)]
    struct Sink(Arc<Mutex<Vec<u8>>>);
    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes); Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> { Ok(()) }
    }
    #[test]
    fn even_verbose_logging_blocks_protocol_payloads_and_arbitrary_errors() {
        use tracing_subscriber::Layer;
        let sink = Sink(Default::default());
        let writer = sink.clone();
        let layer = tracing_subscriber::fmt::layer().with_ansi(false)
            .with_writer(move || writer.clone())
            .with_filter(tracing_subscriber::filter::filter_fn(super::allowed));
        tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
            tracing::trace!(target:"airplay_crypto", "SECRET_TEST_SENTINEL");
            tracing::debug!(target:"airplay_pairing", "SECRET_TEST_SENTINEL");
            tracing::info!(target:"airplay_audio::streamer", "PCM_TEST_SENTINEL");
            tracing::warn!(target:"airplay_rtsp", "KEY_TEST_SENTINEL");
            tracing::error!(target:"conexion_airplay_lib", "PRIVATE_ERROR_SENTINEL");
            tracing::info!(target:"desk_audio_metrics", packets_sent=42u64, "receiver audio diagnostics");
        });
        let output = String::from_utf8(sink.0.lock().unwrap().clone()).unwrap();
        assert!(!output.contains("SENTINEL"));
        assert!(output.contains("packets_sent=42"));
    }
}
