// DeskAudio Cast privacy change 2026-10-21: protocol logs expose static event IDs only.
// DeskAudio Cast, 2026-10-21, GPL-3.0-or-later.
// New live worker written against the explicitly GPLv3 upstream RTP/ALAC APIs.
// No fork live_policy or fork sender implementation is copied here.
use super::*;
use crate::live_options::{LiveDiagnosticsHandle, LiveStreamOptions};
use std::collections::VecDeque;
use std::time::Instant as Mono;

#[cfg(windows)]
struct AudioPriority(*mut std::ffi::c_void);
#[cfg(windows)]
#[link(name="avrt")]
extern "system" {
    fn AvSetMmThreadCharacteristicsW(name:*const u16, index:*mut u32)->*mut std::ffi::c_void;
    fn AvRevertMmThreadCharacteristics(handle:*mut std::ffi::c_void)->i32;
}
#[cfg(windows)]
impl AudioPriority {
    fn enter()->Self {
        let name:[u16;6]=[65,117,100,105,111,0]; let mut index=0;
        // Documented system scheduling API; optional failure leaves normal priority.
        Self(unsafe {AvSetMmThreadCharacteristicsW(name.as_ptr(),&mut index)})
    }
}
#[cfg(windows)]
impl Drop for AudioPriority {
    fn drop(&mut self) {if !self.0.is_null() {unsafe {AvRevertMmThreadCharacteristics(self.0);}}}
}

pub(super) async fn start(owner:&mut AudioStreamer, mut decoder:LiveAudioDecoder, options:LiveStreamOptions)->Result<()> {
    options.validate()?;
    owner.stop().await?;
    let format={owner.inner.lock().await.config.audio_format};
    if format.frames_per_packet==0 || format.sample_rate.as_hz()==0 || format.channels==0 {
        return Err(airplay_core::error::StreamingError::InvalidFormat("Invalid stream format".into()).into());
    }
    let encoder=create_encoder(format)?;
    let diagnostics=decoder.diagnostics();
    decoder.set_recv_timeout(std::time::Duration::ZERO);
    owner.state_cache.store(StreamerState::Buffering as u8,Ordering::Release);
    let (ready_tx,ready_rx)=tokio::sync::oneshot::channel();
    let inner=owner.inner.clone(); let state=owner.state_cache.clone(); let position=owner.timestamp_cache.clone();
    let sent=owner.packets_sent.clone(); let underruns=owner.underruns.clone();
    owner.sender_thread=Some(std::thread::Builder::new().name("desk-audio-live".into()).spawn(move || {
        #[cfg(windows)] let _priority=AudioPriority::enter();
        let result=run(inner,state.clone(),position,sent,underruns,decoder,encoder,diagnostics,options,ready_tx);
        if result.is_err() {state.store(StreamerState::Error as u8,Ordering::Release); tracing::warn!(event = "airplay-audio.src.live_engine.rs:47", "protocol_event");}
    }).map_err(airplay_core::error::Error::Connection)?);
    match tokio::time::timeout(options.startup_timeout+Duration::from_secs(1),ready_rx).await {
        Ok(Ok(()))=>Ok(()),
        _=>{owner.stop().await?;Err(airplay_core::error::Error::Timeout)}
    }
}

#[allow(clippy::too_many_arguments)]
fn run(inner:Arc<Mutex<StreamerInner>>, state:Arc<AtomicU8>, position:Arc<AtomicU64>,
    sent:Arc<AtomicU64>, underruns:Arc<AtomicU64>, mut decoder:LiveAudioDecoder,
    mut encoder:Box<dyn AudioEncoder>, diagnostics:LiveDiagnosticsHandle,
    options:LiveStreamOptions, ready_tx:tokio::sync::oneshot::Sender<()>)->Result<()> {
    let format=inner.blocking_lock().config.audio_format;
    let packet_frames=format.frames_per_packet as usize;
    let rate=format.sample_rate.as_hz() as u64;
    let period=std::time::Duration::from_nanos(packet_frames as u64*1_000_000_000/rate);
    let packet_count=|ms:u32| ((ms as u64*rate).div_ceil(packet_frames as u64*1000)) as usize;
    let capacity=packet_count(options.buffer_ms).max(1);
    let fill=packet_count(options.refill_ms).max(1).min(capacity);
    let mut queue=VecDeque::with_capacity(capacity);
    let mut ready=Some(ready_tx); let began=Mono::now();
    let mut buffering=true; let mut first=true; let mut underrun=false;
    let mut deadline=Mono::now();
    let mut last_encode_us=0;let mut last_wake_late_us=0;
    loop {
        let current=state.load(Ordering::Acquire);
        if current==StreamerState::Stopped as u8 || current==StreamerState::Error as u8 {break;}
        if current==StreamerState::Paused as u8 {std::thread::sleep(Duration::from_millis(2));deadline=Mono::now();continue;}
        let target=if first {packet_count(options.startup_ms).max(1).max(fill)} else {fill};
        while queue.len()<target.min(capacity) {
            match decoder.decode_resampled(&format,packet_frames)? {
                Some(frame)=>queue.push_back((frame,decoder.packet_arrival.unwrap_or_else(Mono::now))),
                None=>break,
            }
        }
        diagnostics.measurements.lock().unwrap().values.encoder_buffer_ms=queue.len() as f64*period.as_secs_f64()*1000.0;
        if buffering {
            let required=if first {packet_count(options.startup_ms).max(1)} else {packet_count(options.resume_ms).max(1)};
            let timeout=first && began.elapsed()>=options.startup_timeout && !queue.is_empty();
            if queue.len()<required && !timeout {
                if decoder.is_eof() {break;}
                std::thread::sleep(Duration::from_millis(1));continue;
            }
            buffering=false; first=false; underrun=false; deadline=Mono::now();
            inner.blocking_lock().state=StreamerState::Streaming;
            state.store(StreamerState::Streaming as u8,Ordering::Release);
            if let Some(tx)=ready.take() {let _=tx.send(());}
        }
        // A temporarily empty queue is not an underrun before the scheduled
        // deadline. Wait only within the existing budget, never extend it.
        if let Some(wait)=pending_input_wait(queue.is_empty(),decoder.is_eof(),Mono::now(),deadline){spin_sleep::sleep(wait);continue;}
        let Some((frame,arrived))=queue.pop_front() else {
            if decoder.is_eof() {break;}
            if !underrun {
                underruns.fetch_add(1,Ordering::Relaxed);
                diagnostics.underrun(last_encode_us,last_wake_late_us);underrun=true;
            }
            buffering=true; state.store(StreamerState::Buffering as u8,Ordering::Release);continue;
        };
        spin_sleep::sleep(deadline.saturating_duration_since(Mono::now()));
        last_wake_late_us=Mono::now().saturating_duration_since(deadline).as_micros() as u64;
        if state.load(Ordering::Acquire)==StreamerState::Stopped as u8 {break;}
        let encode_started=Mono::now();
        let packet=encoder.encode(&frame.samples)?;
        last_encode_us=encode_started.elapsed().as_micros() as u64;
        let mut guard=inner.blocking_lock();
        if let Some(rx)=guard.timing_rx.as_mut() {
            let offset=*rx.borrow_and_update(); guard.clock_offset=Some(offset);
        }
        let rtp=guard.current_timestamp as u32;
        let mark=!guard.first_packet_sent;
        let sync=mark || rtp.wrapping_sub(guard.last_sync_rtp)>=rate as u32;
        let wall=guard.clock.now_wall_ns();
        let adjusted=guard.clock_offset.map_or(wall,|offset|guard.clock.apply_offset(wall,&offset));
        let render=adjusted.saturating_add(guard.render_delay_ns);
        let ptp=guard.use_ptp_sync; let clock_id=guard.ptp_master_clock_id;
        let mut success=true;
        for sender in &mut guard.rtp_senders {
            if sync {
                let sync_result=if ptp {sender.send_ptp_sync(rtp,render,rtp.wrapping_add(packet_frames as u32),&clock_id)}
                    else {sender.send_sync(rtp,unix_to_ntp(render))};
                if sync_result.is_err() {success=false;}
            }
            if sender.send_audio(96,rtp,&packet.data,mark).is_err() {success=false;}
        }
        if guard.rtp_senders.is_empty() {success=false;}
        if sync {guard.last_sync_rtp=rtp;}
        guard.first_packet_sent=true;
        guard.current_timestamp=guard.current_timestamp.wrapping_add(packet_frames as u64);
        position.store(guard.current_timestamp,Ordering::Relaxed);
        drop(guard);
        diagnostics.sent(arrived,success,last_encode_us,last_wake_late_us);
        if success {sent.fetch_add(1,Ordering::Relaxed);}
        deadline+=period;
        // A late scheduler wake must not cause a burst of overdue packets.
        if Mono::now().saturating_duration_since(deadline)>period {deadline=Mono::now();}
    }
    diagnostics.measurements.lock().unwrap().values.encoder_buffer_ms=0.0;
    if state.load(Ordering::Acquire)!=StreamerState::Error as u8 {state.store(StreamerState::Stopped as u8,Ordering::Release);}
    Ok(())
}

fn pending_input_wait(empty:bool,eof:bool,now:Mono,deadline:Mono)->Option<Duration>{
    (empty && !eof && now<deadline).then(||deadline.saturating_duration_since(now).min(Duration::from_millis(1)))
}
#[cfg(test)]mod deadline_tests{
    use super::*;
    #[test]fn supply_arriving_before_due_is_not_an_underrun(){
        let start=Mono::now();let due=start+Duration::from_millis(8);
        for ms in 0..8{assert!(pending_input_wait(true,false,start+Duration::from_millis(ms),due).is_some());}
        assert!(pending_input_wait(false,false,start+Duration::from_millis(7),due).is_none());
        assert!(pending_input_wait(true,false,due,due).is_none());
        assert!(pending_input_wait(true,false,due+Duration::from_millis(20),due).is_none());
        assert!(pending_input_wait(true,true,start,due).is_none());
    }
    #[test]fn waiting_never_extends_the_existing_deadline(){
        let now=Mono::now();let due=now+Duration::from_micros(200);
        assert_eq!(pending_input_wait(true,false,now,due),Some(Duration::from_micros(200)));
    }
}
