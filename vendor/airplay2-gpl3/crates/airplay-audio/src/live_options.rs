// DeskAudio Cast independent adapter, 2026-10-21, GPL-3.0-or-later.
// Functional boundary: bounded live input and numeric, audio-free diagnostics.
use std::{collections::VecDeque, sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}}, time::{Duration, Instant}};

#[derive(Clone, Copy, Debug)]
pub struct LiveStreamOptions {
    pub buffer_ms: u32, pub startup_ms: u32, pub refill_ms: u32, pub resume_ms: u32,
    pub startup_timeout: Duration, pub sender_capacity: usize, pub max_age: Option<Duration>,
}
impl Default for LiveStreamOptions {
    fn default() -> Self { Self { buffer_ms:320, startup_ms:160, refill_ms:128, resume_ms:64,
        startup_timeout:Duration::from_secs(2), sender_capacity:1, max_age:None } }
}
impl LiveStreamOptions {
    pub(crate) fn validate(&self) -> airplay_core::error::Result<()> {
        if self.buffer_ms == 0 || self.buffer_ms > 2000 || self.startup_ms > self.buffer_ms ||
            self.refill_ms > self.buffer_ms || self.resume_ms > self.buffer_ms || self.sender_capacity == 0 {
            return Err(airplay_core::error::StreamingError::InvalidFormat("Invalid local buffer settings".into()).into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct UnderrunEvent {
    pub elapsed_ms:u64, pub producer_blocks:u64, pub decoder_blocks:u64, pub queued_blocks:usize,
    pub last_supply_gap_us:Option<u64>, pub last_decode_gap_us:Option<u64>,
    pub previous_encode_us:u64, pub previous_wake_late_us:u64,
}
#[derive(Clone, Debug, Default)]
pub struct LiveSnapshot {
    pub elapsed_ms:u64,
    pub queued_blocks:usize, pub queue_drops:u64, pub stale_drops:u64,
    pub packets_sent:u64, pub send_errors:u64, pub encoder_buffer_ms:f64,
    pub underruns:u64, pub max_queue_age_us:u64,
    pub capture_to_send_p95_us:Option<u64>, pub first_packet_age_us:Option<u64>,
    pub max_encode_us:u64, pub max_wake_late_us:u64, pub underrun_events:Vec<UnderrunEvent>,
}
pub(crate) struct Measurements {
    pub values:LiveSnapshot,
    pub arrivals:VecDeque<Instant>,
    pub ages:VecDeque<u64>,
    pub began:Instant,pub last_supply:Option<Instant>,pub last_decode:Option<Instant>,
    pub producer_blocks:u64,pub decoder_blocks:u64,pub events:VecDeque<UnderrunEvent>,
}
impl Default for Measurements {
    fn default()->Self{Self{values:LiveSnapshot::default(),arrivals:VecDeque::new(),ages:VecDeque::new(),began:Instant::now(),last_supply:None,last_decode:None,producer_blocks:0,decoder_blocks:0,events:VecDeque::new()}}
}
#[derive(Clone, Default)]
pub struct LiveDiagnosticsHandle {
    pub(crate) measurements:Arc<Mutex<Measurements>>,
    pub(crate) closed:Arc<AtomicBool>,
}
impl LiveDiagnosticsHandle {
    pub fn snapshot(&self) -> LiveSnapshot {
        let guard=self.measurements.lock().unwrap(); let mut s=guard.values.clone();
        s.elapsed_ms=guard.began.elapsed().as_millis().min(u64::MAX as u128) as u64;
        s.queued_blocks=guard.arrivals.len();
        s.underrun_events=guard.events.iter().cloned().collect();
        let mut ages:Vec<_>=guard.ages.iter().copied().collect();drop(guard);
        if !ages.is_empty() {
            ages.sort_unstable();
            s.capture_to_send_p95_us=Some(ages[(ages.len()*95).div_ceil(100).saturating_sub(1)]);
        }
        s
    }
    pub(crate) fn sent(&self, arrived:Instant, success:bool, encode_us:u64,wake_late_us:u64) {
        let mut g=self.measurements.lock().unwrap();
        g.values.max_encode_us=g.values.max_encode_us.max(encode_us);
        g.values.max_wake_late_us=g.values.max_wake_late_us.max(wake_late_us);
        if success {
            let age=arrived.elapsed().as_micros().min(u64::MAX as u128) as u64;
            g.values.packets_sent+=1; g.values.first_packet_age_us.get_or_insert(age);
            if g.ages.len()==256 {g.ages.pop_front();} g.ages.push_back(age);
        } else {g.values.send_errors+=1;}
    }
    pub(crate) fn underrun(&self,encode_us:u64,wake_late_us:u64){
        let now=Instant::now();let mut g=self.measurements.lock().unwrap();g.values.underruns+=1;
        let event=UnderrunEvent{elapsed_ms:now.saturating_duration_since(g.began).as_millis() as u64,
            producer_blocks:g.producer_blocks,decoder_blocks:g.decoder_blocks,queued_blocks:g.arrivals.len(),
            last_supply_gap_us:g.last_supply.map(|t|now.saturating_duration_since(t).as_micros() as u64),
            last_decode_gap_us:g.last_decode.map(|t|now.saturating_duration_since(t).as_micros() as u64),
            previous_encode_us:encode_us,previous_wake_late_us:wake_late_us};
        if g.events.len()==64{g.events.pop_front();}g.events.push_back(event);
    }
    pub(crate) fn is_closed(&self)->bool { self.closed.load(Ordering::Acquire) }
}

#[cfg(test)]mod tests{
    #[test]fn timeline_is_bounded_numeric_and_keeps_total_after_eviction(){
        let d=super::LiveDiagnosticsHandle::default();for i in 0..100{d.underrun(i,i*2);}
        let s=d.snapshot();assert_eq!(s.underruns,100);assert_eq!(s.underrun_events.len(),64);
        assert_eq!(s.underrun_events[0].previous_encode_us,36);assert_eq!(s.underrun_events.last().unwrap().previous_wake_late_us,198);
    }
}
