//! Uses the actual imported protocol APIs, without network or audio hardware.
use ap2rs_core::{codec::{AudioCodec, AudioFormat, SampleRate},stream::StreamConfig};

#[test]
fn low_level_identity_storage_also_refuses_file_access() {
    let identity=ap2rs_pairing::ControllerIdentity::generate();
    let path=std::env::temp_dir().join(format!("desk-audio-no-identity-{}.txt", identity.id()));
    assert!(!path.exists());
    assert_eq!(identity.save(&path).unwrap_err().kind(),std::io::ErrorKind::Unsupported);
    assert_eq!(ap2rs_pairing::ControllerIdentity::load(&path).err().unwrap().kind(),std::io::ErrorKind::Unsupported);
    assert_eq!(ap2rs_pairing::ControllerIdentity::load_or_generate(&path).err().unwrap().kind(),std::io::ErrorKind::Unsupported);
    assert!(!path.exists());
}

#[test]
fn unsupported_aac_is_rejected_while_alac_still_encodes() {
    let mut format=AudioFormat {
        codec:AudioCodec::Aac,sample_rate:SampleRate::Hz44100,bit_depth:16,channels:2,frames_per_packet:352
    };
    assert!(ap2rs_audio::create_encoder(format).is_err());
    format.codec=AudioCodec::Alac;
    let mut encoder=ap2rs_audio::create_encoder(format).unwrap();
    let packet=encoder.encode(&vec![0i16;704]).unwrap();
    assert_eq!(packet.samples,352);
    assert!(!packet.data.is_empty());
}

#[tokio::test]
async fn persistent_pairing_apis_fail_before_network_or_identity_file_use() {
    let descriptor=cap_core::pairing::DeviceDescriptor {
        ip:"192.0.2.1".parse().unwrap(),port:7000,name:"test fixture".into(),
        mac:None,model:None,features:None,advertised:None
    };
    let device=descriptor.into_ap2_device().unwrap();
    // TEST-NET address is never contacted: disabled entrypoints return immediately.
    let config=StreamConfig::default();
    let result=ap2rs_client::Connection::connect_auto(device.clone(),config.clone(),"unused").await;
    assert!(result.err().unwrap().to_string().contains("Persistent pairing is disabled"));
    let result=ap2rs_client::Connection::connect_with_pin_pairing(device,config,"unused").await;
    assert!(result.err().unwrap().to_string().contains("Persistent pairing is disabled"));
}
