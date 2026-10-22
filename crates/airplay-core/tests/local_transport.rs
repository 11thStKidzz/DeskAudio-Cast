//! Actual encoder/streamer/UDP regressions, confined to 127.0.0.1.
//! No discovery, pairing, microphone, HomePod or claims of acoustic success.
use ap2rs_audio::{
    AlacEncoder, AudioEncoder, AudioStreamer, LiveAudioDecoder, LivePcmFrame, RtpHeader, RtpSender,
};
use ap2rs_core::stream::StreamConfig;
use cap_core::streaming::LocalBufferPolicy;
use std::net::UdpSocket;
use std::time::{Duration, Instant};

fn frame(index: usize) -> LivePcmFrame {
    // Alternate silence, rich deterministic audio and sharp transitions.
    let samples = (0..704)
        .map(|sample| {
            if index % 60 < 15 {
                0
            } else {
                let x = (index * 704 + sample) as u32;
                (x.wrapping_mul(1664525).wrapping_add(1013904223) >> 16) as i16
            }
        })
        .collect();
    LivePcmFrame {
        samples,
        channels: 2,
        sample_rate: 44_100,
    }
}

#[test]
fn complex_audio_and_silence_encode_with_continuous_sample_positions() {
    let mut encoder = AlacEncoder::new(StreamConfig::default().audio_format).unwrap();
    for index in 0..500 {
        let packet = encoder.encode(&frame(index).samples).unwrap();
        assert_eq!(packet.timestamp, index as u64 * 352);
        assert_eq!(packet.samples, 352);
        assert!(!packet.data.is_empty());
        // Record the uncompressed escape case too; not an audio-quality test.
        assert!(packet.data.len() <= 704 * 2 + 256);
    }
}

async fn run_local_stream(gap: bool, policy: LocalBufferPolicy) {
    let socket = UdpSocket::bind("127.0.0.1:0").unwrap();
    socket
        .set_read_timeout(Some(Duration::from_millis(50)))
        .unwrap();
    let mut rtp = RtpSender::new(socket.local_addr().unwrap(), 42);
    rtp.bind(0).unwrap();
    let (producer, decoder) =
        LiveAudioDecoder::create_pair_with_max_age(44_100, 2, policy.input_capacity(), None);
    let stats = producer.diagnostics_handle();
    let prefill = policy.input_capacity().min(24);
    for index in 0..prefill {
        assert!(producer.try_send(frame(index)));
    }

    // Receiver observes wire bytes, not the sender's optimistic packet counter.
    let receiver = std::thread::spawn(move || {
        let mut headers = Vec::new();
        let mut after_gap = 0;
        let began = Instant::now();
        let mut last = began;
        let mut max_gap = Duration::ZERO;
        while began.elapsed() < Duration::from_secs(6) && headers.len() < 160 {
            let mut bytes = [0; 4096];
            match socket.recv(&mut bytes) {
                Ok(size) if size >= 12 && bytes[1] & 0x7f == 96 => {
                    let now = Instant::now();
                    if !headers.is_empty() {
                        max_gap = max_gap.max(now.duration_since(last));
                    }
                    last = now;
                    headers.push(RtpHeader::parse(&bytes[..size]).unwrap());
                    if began.elapsed() > Duration::from_millis(650) {
                        after_gap += 1;
                    }
                }
                Ok(_) => (),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    ()
                }
                Err(error) => panic!("local receiver: {error}"),
            }
        }
        (headers, after_gap, max_gap)
    });
    let mut streamer = AudioStreamer::new(StreamConfig::default());
    streamer.set_rtp_sender(rtp).await;
    tokio::time::timeout(
        Duration::from_secs(3),
        streamer.start_live_with_options(decoder, policy.options()),
    )
    .await
    .unwrap()
    .unwrap();
    let input = producer.clone();
    let feeder = std::thread::spawn(move || {
        let mut deadline = Instant::now();
        let mut rejected = 0;
        for index in prefill..prefill + 180 {
            if gap && index == 65 {
                std::thread::sleep(Duration::from_millis(250));
                deadline = Instant::now();
            }
            if !input.try_send(frame(index)) {
                rejected += 1;
            }
            deadline += Duration::from_nanos(352 * 1_000_000_000 / 44_100);
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        }
        rejected
    });
    let (headers, after_gap, max_gap) =
        tokio::task::spawn_blocking(move || receiver.join().unwrap())
            .await
            .unwrap();
    let rejected = tokio::task::spawn_blocking(move || feeder.join().unwrap())
        .await
        .unwrap();
    // Always stop the worker before asserting, even when an observation failed.
    tokio::time::timeout(Duration::from_secs(2), streamer.stop())
        .await
        .unwrap()
        .unwrap();
    assert!(producer.is_closed(), "stop must close the real producer");
    assert_eq!(headers.len(), 160, "actual UDP delivery stalled");
    assert!(
        after_gap > 20,
        "audio did not resume after the injected pause"
    );
    assert_eq!(rejected, 0, "producer overflowed; this run is not lossless");
    for pair in headers.windows(2) {
        assert_eq!(pair[1].sequence, pair[0].sequence.wrapping_add(1));
        assert_eq!(pair[1].timestamp, pair[0].timestamp.wrapping_add(352));
    }
    let snapshot = stats.snapshot();
    assert_eq!(snapshot.send_errors, 0);
    assert_eq!(snapshot.queue_drops, 0);
    eprintln!(
        "local UDP only: packets={}, max_receive_gap_ms={}, underruns={}, local_p95_us={:?}",
        headers.len(),
        max_gap.as_millis(),
        snapshot.underruns,
        snapshot.capture_to_send_p95_us
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn balanced_stream_delivers_complex_audio_with_continuous_rtp() {
    run_local_stream(false, LocalBufferPolicy::Balanced).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn producer_pause_recovers_without_permanent_udp_silence() {
    run_local_stream(true, LocalBufferPolicy::Balanced).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ultra_low_profile_preserves_packets_and_recovers_from_pause() {
    run_local_stream(true, LocalBufferPolicy::LowLatency).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stable_profile_preserves_packets_and_recovers_from_pause() {
    run_local_stream(true, LocalBufferPolicy::Stable).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn five_start_stop_cycles_close_each_producer() {
    for _ in 0..5 {
        let receiver = UdpSocket::bind("127.0.0.1:0").unwrap();
        let mut rtp = RtpSender::new(receiver.local_addr().unwrap(), 42);
        rtp.bind(0).unwrap();
        let (producer, decoder) = LiveAudioDecoder::create_pair_with_max_age(44_100, 2, 24, None);
        for index in 0..20 {
            assert!(producer.try_send(frame(index)));
        }
        let mut streamer = AudioStreamer::new(StreamConfig::default());
        streamer.set_rtp_sender(rtp).await;
        tokio::time::timeout(
            Duration::from_secs(3),
            streamer.start_live_with_options(decoder, LocalBufferPolicy::Balanced.options()),
        )
        .await
        .unwrap()
        .unwrap();
        tokio::time::sleep(Duration::from_millis(30)).await;
        tokio::time::timeout(Duration::from_secs(2), streamer.stop())
            .await
            .unwrap()
            .unwrap();
        assert!(producer.is_closed());
        assert!(!producer.try_send(frame(21)));
    }
}
