//! Drain coalesced shared-mode packets before waiting for another WASAPI event.
//! Does not change PCM, buffer sizes or pacing. Pure helper also runs in tests.

#[derive(Default)]
pub(crate) struct NativePacket {
    pub frames: u32,
    pub discontinuity: bool,
    pub timestamp_error: bool,
}

#[derive(Debug, Default)]
pub(crate) struct DrainBatch {
    pub packets: u64,
    pub frames: u64,
    pub discontinuities: u64,
    pub timestamp_errors: u64,
    pub limit_reached: bool,
}

pub(crate) fn drain_pending<E>(limit: usize, mut read_next: impl FnMut() -> Result<Option<NativePacket>, E>) -> Result<DrainBatch, E> {
    let mut batch = DrainBatch::default();
    for _ in 0..limit {
        let Some(packet) = read_next()? else { return Ok(batch); };
        batch.packets += 1;
        batch.frames += u64::from(packet.frames);
        batch.discontinuities += u64::from(packet.discontinuity);
        batch.timestamp_errors += u64::from(packet.timestamp_error);
    }
    batch.limit_reached = limit > 0;
    Ok(batch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn coalesced_wake_drains_every_packet_in_order_without_another_event() {
        let mut pending = VecDeque::from([vec![1i16; 441], vec![2; 441], vec![3; 441]]);
        let expected: Vec<_> = pending.iter().flatten().copied().collect();
        let mut delivered = Vec::new();
        let batch = drain_pending::<()>(64, || Ok(pending.pop_front().map(|pcm| {
            let frames = pcm.len() as u32;
            delivered.extend(pcm);
            NativePacket { frames, ..Default::default() }
        }))).unwrap();
        assert!(pending.is_empty());
        assert_eq!(delivered, expected); // No replacement, sample loss or extra delay.
        assert_eq!((batch.packets, batch.frames), (3, 1323));
        assert!(!batch.limit_reached);
    }

    #[test]
    fn bounded_pass_can_continue_immediately_without_discarding_backlog() {
        let mut remaining = 5;
        let mut read = || { if remaining == 0 { return Ok::<_, ()>(None); } remaining -= 1; Ok(Some(NativePacket { frames: 352, ..Default::default() })) };
        let a = drain_pending(3, &mut read).unwrap();
        assert!(a.limit_reached);
        let b = drain_pending(3, &mut read).unwrap();
        assert!(!b.limit_reached);
        assert_eq!(a.frames + b.frames, 5 * 352);
    }

    #[test]
    fn native_failure_is_propagated_and_flags_are_counted_not_audio_logged() {
        assert_eq!(drain_pending::<&str>(64, || Err("native_read_failed")).unwrap_err(), "native_read_failed");
        let mut first = true;
        let batch = drain_pending::<()>(64, || Ok(if std::mem::take(&mut first) {
            Some(NativePacket { frames: 441, discontinuity: true, timestamp_error: true })
        } else { None })).unwrap();
        assert_eq!((batch.discontinuities, batch.timestamp_errors), (1, 1));
    }
}
