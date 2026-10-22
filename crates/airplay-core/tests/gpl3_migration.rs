//! GPLv3 candidate only. Synthetic PCM and loopback; never a HomePod hearing test.
use ap2rs_audio::{AlacEncoder,AudioEncoder,AudioStreamer,LiveAudioDecoder,LivePcmFrame,RtpSender,RtpPacket};
use ap2rs_core::stream::StreamConfig;
use cap_core::streaming::LocalBufferPolicy;
use std::{net::UdpSocket,time::Duration};

fn block(value:i16,frames:usize)->LivePcmFrame {
    LivePcmFrame{samples:vec![value;frames*2],channels:2,sample_rate:44100}
}
#[test]
fn diagnostics_do_not_keep_producer_alive_and_input_stays_bounded() {
    let (sender,decoder)=LiveAudioDecoder::create_pair_with_max_age(44100,2,2,None);
    let diagnostics=sender.diagnostics_handle();
    assert!(sender.try_send(block(1,352))); assert!(sender.try_send(block(2,352)));
    assert!(!sender.try_send(block(3,352)));
    assert_eq!(diagnostics.snapshot().queued_blocks,2);
    assert_eq!(diagnostics.snapshot().queue_drops,1);
    drop(decoder); assert!(sender.is_closed()); assert!(!sender.try_send(block(4,352)));
    assert_eq!(diagnostics.snapshot().queued_blocks,0);
}
#[test]
fn partial_input_is_joined_without_zero_padding_or_sample_changes() {
    let (sender,mut decoder)=LiveAudioDecoder::create_pair(44100,2,4);
    let format=StreamConfig::default().audio_format;
    assert!(sender.try_send(block(123,123)));
    assert!(decoder.decode_resampled(&format,352).unwrap().is_none());
    assert!(sender.try_send(block(-321,229)));
    let frame=decoder.decode_resampled(&format,352).unwrap().unwrap();
    assert_eq!(&frame.samples[..246],vec![123;246]);
    assert_eq!(&frame.samples[246..],vec![-321;458]);
}
#[test]
fn malformed_input_is_rejected_without_panicking() {
    let (sender,_decoder)=LiveAudioDecoder::create_pair(44100,2,4);
    let mut frame=block(0,1);frame.channels=0;assert!(!sender.try_send(frame));
    let mut frame=block(0,1);frame.samples.pop();assert!(!sender.try_send(frame));
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn underrun_timeline_distinguishes_a_paused_supplier_without_recording_audio(){
    let socket=UdpSocket::bind("127.0.0.1:0").unwrap();
    let mut rtp=RtpSender::new(socket.local_addr().unwrap(),1234);rtp.bind(0).unwrap();
    let (input,decoder)=LiveAudioDecoder::create_pair(44100,2,8);
    let diagnostic=input.diagnostics_handle();for _ in 0..4{assert!(input.try_send(block(11,352)));}
    let mut streamer=AudioStreamer::new(StreamConfig::default());streamer.set_rtp_senders(vec![rtp]).await;
    streamer.start_live_with_options(decoder,LocalBufferPolicy::LowLatency.options()).await.unwrap();
    for _ in 0..100{if diagnostic.snapshot().underruns>0{break;}tokio::time::sleep(Duration::from_millis(5)).await;}
    let s=diagnostic.snapshot();assert!(s.underruns>0);let event=&s.underrun_events[0];
    assert_eq!(event.producer_blocks,4);assert_eq!(event.decoder_blocks,4);assert_eq!(event.queued_blocks,0);
    assert!(event.last_supply_gap_us.is_some());assert_eq!(s.queue_drops,0);
    streamer.stop().await.unwrap();
}
#[tokio::test]
async fn invalid_policy_is_rejected_instead_of_allocating_unbounded_buffer() {
    let mut streamer=AudioStreamer::new(StreamConfig::default());
    let (_sender,decoder)=LiveAudioDecoder::create_pair(44100,2,4);
    let mut options=LocalBufferPolicy::LowLatency.options();options.buffer_ms=u32::MAX;
    assert!(streamer.start_live_with_options(decoder,options).await.is_err());
}
#[test]
fn alac_roundtrip_preserves_every_synthetic_pcm_sample() {
    use symphonia::core::{codecs::{CodecParameters,DecoderOptions,CODEC_TYPE_ALAC},formats::Packet,audio::SampleBuffer};
    let mut encoder=AlacEncoder::new(StreamConfig::default().audio_format).unwrap();
    let mut params=CodecParameters::new();params.codec=CODEC_TYPE_ALAC;params.extra_data=Some(encoder.magic_cookie().into_boxed_slice());
    let mut decoder=symphonia::default::get_codecs().make(&params,&DecoderOptions::default()).unwrap();
    for index in 0..100u32 {
        let samples:Vec<i16>=(0..704u32).map(|i| {
            if index%3==0 {0} else {((i+index*704).wrapping_mul(1664525).wrapping_add(1013904223)>>16) as i16}
        }).collect();
        let encoded=encoder.encode(&samples).unwrap();
        let packet=Packet::new_from_slice(0,encoded.timestamp,352,&encoded.data);
        let decoded=decoder.decode(&packet).unwrap();
        let mut output=SampleBuffer::<i16>::new(decoded.capacity() as u64,*decoded.spec());output.copy_interleaved_ref(decoded);
        assert_eq!(output.samples(),samples.as_slice(),"ALAC changed samples in packet {index}");
    }
}
#[tokio::test(flavor="multi_thread",worker_threads=2)]
async fn two_targets_share_rtp_time_but_use_separate_authenticated_keys() {
    use ap2rs_audio::cipher::ChaChaPacketCipher;
    use ap2rs_crypto::chacha::AudioCipher;
    let sockets:[UdpSocket;2]=std::array::from_fn(|_|UdpSocket::bind("127.0.0.1:0").unwrap());
    let mut senders=Vec::new();
    for (i,socket) in sockets.iter().enumerate() {
        socket.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
        let mut sender=RtpSender::new(socket.local_addr().unwrap(),1234);
        sender.bind(0).unwrap();sender.set_cipher(Box::new(ChaChaPacketCipher::new(AudioCipher::new([i as u8+1;32]))));senders.push(sender);
    }
    let (input,decoder)=LiveAudioDecoder::create_pair(44100,2,8);
    for i in 0..8 {assert!(input.try_send(block(i,352)));}
    let mut streamer=AudioStreamer::new(StreamConfig::default());streamer.set_rtp_senders(senders).await;
    streamer.start_live_with_options(decoder,LocalBufferPolicy::LowLatency.options()).await.unwrap();
    let received=tokio::task::spawn_blocking(move || {
        sockets.map(|socket| {
            let mut bytes=[0u8;4096];
            loop {
                let size=socket.recv(&mut bytes).unwrap();
                if size>=12 && bytes[1]&0x7f==96 {break bytes[..size].to_vec();}
            }
        })
    }).await.unwrap();
    streamer.stop().await.unwrap();
    let packets=received.map(|bytes|RtpPacket::parse_encrypted(&bytes).unwrap());
    assert_eq!(packets[0].header.timestamp,packets[1].header.timestamp);
    let mut plain=Vec::new();
    for (i,packet) in packets.iter().enumerate() {
        plain.push(AudioCipher::new([i as u8+1;32]).decrypt(&packet.payload,&packet.nonce.unwrap(),&packet.tag.unwrap(),packet.header.timestamp,packet.header.ssrc).unwrap());
        assert!(AudioCipher::new([99;32]).decrypt(&packet.payload,&packet.nonce.unwrap(),&packet.tag.unwrap(),packet.header.timestamp,packet.header.ssrc).is_err());
    }
    assert_eq!(plain[0],plain[1]);assert!(input.is_closed());
}
