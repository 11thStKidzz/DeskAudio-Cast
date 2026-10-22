//! Deterministic silence/tone test, bypassing Windows capture and virtual cables.
//! Uses generated audio only. No PCM, keys or packet contents are logged.
use cap_core::streaming::{prepare_live_stream, LivePcmFrame, LocalBufferPolicy};
use std::net::IpAddr;
use std::time::Duration;

fn synthetic_sample(position: u64) -> i16 {
    const RATE: u64 = 44_100;
    const CYCLE: u64 = 5 * RATE;
    let at = position % CYCLE;
    let frequency = if position / CYCLE == 1 { 660.0 } else { 440.0 };
    if at < 2 * RATE { return 0; }
    ((std::f64::consts::TAU * frequency * (at - 2 * RATE) as f64 / RATE as f64).sin()
        * 0.05 * i16::MAX as f64) as i16
}

fn write_test_wav(path: &str) -> std::io::Result<()> {
    use std::io::Write;
    let frames = 15 * 44_100u32;
    let bytes = frames * 4;
    // Refuse to replace any existing file, even a previous diagnostic.
    let mut file = std::fs::OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + bytes).to_le_bytes())?;
    file.write_all(b"WAVEfmt ")?;
    file.write_all(&16u32.to_le_bytes())?;
    file.write_all(&1u16.to_le_bytes())?;
    file.write_all(&2u16.to_le_bytes())?;
    file.write_all(&44_100u32.to_le_bytes())?;
    file.write_all(&(44_100u32 * 4).to_le_bytes())?;
    file.write_all(&4u16.to_le_bytes())?;
    file.write_all(&16u16.to_le_bytes())?;
    file.write_all(b"data")?;
    file.write_all(&bytes.to_le_bytes())?;
    let mut pcm = Vec::with_capacity(bytes as usize);
    for position in 0..frames as u64 {
        let sample = synthetic_sample(position).to_le_bytes();
        pcm.extend_from_slice(&sample);
        pcm.extend_from_slice(&sample);
    }
    file.write_all(&pcm)?;
    println!("generated known test audio: 15 seconds, PCM 44100/16/stereo");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--write-wav") {
        let path = std::env::args().nth(2).ok_or("missing output WAV path")?;
        write_test_wav(&path)?;
        return Ok(());
    }
    // The pinned protocol library's INFO audio logs contain samples. Keep all
    // protocol logging off in this diagnostic; report only bounded counters.
    let ip: IpAddr = std::env::args().nth(1).ok_or("missing receiver IP")?.parse()?;
    let latency: u32 = std::env::args().nth(2).unwrap_or_else(|| "1500".into()).parse()?;
    let devices = cap_core::browse_once(Duration::from_secs(3)).await?;
    let receiver = devices.iter().find(|device| {
        device.addresses.contains(&ip) && device.id.contains("._airplay._tcp.")
    }).ok_or("receiver not discovered; refusing a different capability configuration")?;
    let descriptor = receiver.descriptor(ip, &devices)?;
    println!("preparing discovered receiver");
    let prepared = tokio::time::timeout(Duration::from_secs(20), prepare_live_stream(
        descriptor, Some(0.2), Some(latency), LocalBufferPolicy::Balanced,
    )).await??;
    let sender = prepared.sender();
    let producer = tokio::spawn(async move {
        const RATE: u64 = 44_100;
        const BLOCK: u64 = 352;
        const CYCLE: u64 = 5 * RATE;
        let started = tokio::time::Instant::now();
        let mut position = 0u64;
        let mut rejected = 0u64;
        let mut announced = None;
        while position < 3 * CYCLE {
            let cycle = position / CYCLE;
            let within = position % CYCLE;
            let audible = within >= 2 * RATE;
            if announced != Some((cycle, audible)) {
                println!("cycle={} phase={}", cycle + 1, if audible { "tone" } else { "silence" });
                announced = Some((cycle, audible));
            }
            let frames = BLOCK.min(3 * CYCLE - position);
            let mut samples = Vec::with_capacity(frames as usize * 2);
            for offset in 0..frames {
                let value = synthetic_sample(position + offset);
                samples.extend_from_slice(&[value, value]);
            }
            if !sender.try_send(LivePcmFrame { samples, channels: 2, sample_rate: RATE as u32 }) {
                rejected += 1;
            }
            position += frames;
            tokio::time::sleep_until(started + Duration::from_nanos(position * 1_000_000_000 / RATE)).await;
        }
        rejected
    });
    let handle = match tokio::time::timeout(Duration::from_secs(5), prepared.start()).await {
        Ok(Ok(handle)) => handle,
        Ok(Err(error)) => { producer.abort(); return Err(error.into()); }
        Err(error) => { producer.abort(); return Err(error.into()); }
    };
    let rejected = producer.await?;
    // Let the receiver play the tail before ending the session.
    tokio::time::sleep(Duration::from_secs(3)).await;
    let (_, connection, _heartbeat, _, _) = handle.into_parts();
    let stats = connection.lock().await.stream_stats().snapshot();
    let closed = tokio::time::timeout(Duration::from_secs(3), async {
        connection.lock().await.disconnect().await
    }).await;
    println!("complete rejected_blocks={rejected} retransmit_requested={} retransmit_fulfilled={} disconnected={}",
        stats.rtx_requested, stats.rtx_fulfilled, matches!(closed, Ok(Ok(()))));
    if rejected != 0 { return Err("test producer rejected audio blocks; hearing result is inconclusive".into()); }
    Ok(())
}
