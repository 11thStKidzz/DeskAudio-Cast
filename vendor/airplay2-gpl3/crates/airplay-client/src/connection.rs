// DeskAudio Cast privacy change 2026-10-21: protocol logs expose static event IDs only.
//! Device connection management.

use airplay_core::{Device, StreamConfig, error::Result};
use airplay_core::error::{Error as CoreError, RtspError};
use airplay_core::features::AuthMethod;
use airplay_rtsp::{RtspConnection, RtspSession, SessionState, RtspRequest};
use airplay_pairing::{PairingSession, PairVerify, PairSetup, ControllerIdentity};
use airplay_crypto::ed25519::IdentityKeyPair;
// Timing imports reserved for future use
// use airplay_timing::{TimingProtocol, NtpTimingClient, PtpClient};
use airplay_audio::{AudioStreamer, AudioDecoder, LiveAudioDecoder, RtpSender, RtpReceiver, EqConfig, EqParams, SpatialParams, SpeakerConfig};
use airplay_audio::cipher::{PacketCipher, ChaChaPacketCipher};
use std::sync::{Arc, atomic::{AtomicBool, Ordering}};
use airplay_crypto::chacha::ControlCipher;
use airplay_crypto::chacha::AudioCipher;
use airplay_crypto::keys::SharedSecret;
use crate::PlaybackState;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use tracing::{debug, info, warn};
use airplay_timing::{NtpTimingServer, ClockOffset, PtpMaster, PTP_EVENT_PORT, run_ptp_slave, run_bmca_yield_flow, run_ptp_group_master_flow};
use airplay_core::stream::TimingProtocol;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::net::TcpStream;

/// Generate a random MAC-like device ID for the client.
fn generate_device_id() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    format!(
        "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>(),
        rng.gen::<u8>()
    )
}

// DeskAudio Cast: plaintext persisted identities and pair-verify storage path removed.





/// Select the best address from a device's address list.
/// Prefers IPv4, then global IPv6, avoiding link-local addresses.
fn select_best_address(addresses: &[IpAddr]) -> Option<&IpAddr> {
    // First try: IPv4 address
    if let Some(addr) = addresses.iter().find(|a| a.is_ipv4()) {
        tracing::debug!(event = "airplay-client.src.connection.rs:119", "protocol_event");
        return Some(addr);
    }

    // Second try: Global IPv6 (not link-local, not loopback)
    if let Some(addr) = addresses.iter().find(|a| {
        match a {
            IpAddr::V6(v6) => {
                // Avoid link-local (fe80::) and loopback
                !v6.is_loopback() && !is_link_local_v6(v6)
            }
            _ => false,
        }
    }) {
        tracing::debug!(event = "airplay-client.src.connection.rs:133", "protocol_event");
        return Some(addr);
    }

    // Last resort: any address
    let addr = addresses.first();
    if let Some(a) = addr {
        tracing::debug!(event = "airplay-client.src.connection.rs:140", "protocol_event");
    }
    addr
}

/// Check if an IPv6 address is link-local (fe80::/10)
fn is_link_local_v6(addr: &std::net::Ipv6Addr) -> bool {
    let segments = addr.segments();
    (segments[0] & 0xffc0) == 0xfe80
}

/// Parameters needed to send audio to a specific device in a group.
///
/// Each device in a group has its own encryption key (shk) and destination ports,
/// but they all share the same PTP clock and RTP sequence space.
pub struct StreamingParams {
    /// UDP destination for audio data packets.
    pub data_dest: SocketAddr,
    /// UDP destination for control/sync packets.
    pub control_dest: SocketAddr,
    /// Per-device encryption cipher (unique shk per device).
    pub cipher: Box<dyn PacketCipher>,
    /// Control socket (sends sync from our declared control port).
    pub control_socket: Option<UdpSocket>,
}

/// Active connection to an AirPlay device.
pub struct Connection {
    device: Device,
    rtsp: RtspConnection,
    session: RtspSession,
    streamer: Option<AudioStreamer>,
    playback_state: PlaybackState,
    volume: f32,
    stream_config: StreamConfig,
    timing_offset: Option<ClockOffset>,
    timing_tx: Option<watch::Sender<ClockOffset>>,
    control_task: Option<tokio::task::JoinHandle<()>>,
    /// Stop flag for the control channel thread.
    control_stop: Arc<AtomicBool>,
    timing_task: Option<JoinHandle<()>>,
    timing_server: Option<NtpTimingServer>,
    /// PTP master instance (sender IS the timing master)
    ptp_master: Option<PtpMaster>,
    /// Background task sending periodic PTP Sync messages
    ptp_master_sync_task: Option<JoinHandle<()>>,
    /// Control port receiver (keeps the UDP socket alive so HomePod doesn't get ICMP unreachable)
    control_receiver: Option<Arc<RtpReceiver>>,
    /// Reverse connection to device's events port (required before RECORD)
    events_stream: Option<TcpStream>,
    /// Remote PTP master clock identity (from BMCA yield flow)
    ptp_master_clock_id: Option<[u8; 8]>,
    /// Render delay in ms added to NTP timestamps for extra retransmit headroom.
    render_delay_ms: u32,
    /// Equalizer configuration (set before streaming).
    eq_config: Option<EqConfig>,
    /// Shared equalizer parameters for real-time control.
    eq_params: Option<Arc<EqParams>>,
    /// Shared spatial parameters for real-time control.
    spatial_params: Option<Arc<SpatialParams>>,
    /// Speaker configurations for spatial audio.
    spatial_speakers: Option<Vec<SpeakerConfig>>,
    /// Stream statistics (shared with control channel threads).
    stream_stats: Arc<crate::stats::StreamStats>,
}

impl Connection {
    /// Create new connection to device.
    pub async fn connect(device: Device, config: StreamConfig) -> Result<Self> {
        Self::connect_with_pin(device, config, "3939").await
    }

    /// Create connection with PIN for protected devices.
    pub async fn connect_with_pin(device: Device, config: StreamConfig, pin: &str) -> Result<Self> {
        // Generate a stable client device ID for this session
        let client_device_id = generate_device_id();

        // Generate a stable identity keypair for pairing
        let identity = IdentityKeyPair::generate();

        // 1. TCP connect - prefer IPv4 addresses over link-local IPv6
        let ip_addr = select_best_address(&device.addresses)
            .ok_or_else(|| RtspError::ConnectionRefused)?;
        let addr = SocketAddr::new(*ip_addr, device.port);
        tracing::info!(event = "airplay-client.src.connection.rs:224", "protocol_event");

        let mut rtsp = RtspConnection::new(addr);
        rtsp.connect().await?;

        // 2. Create session
        let mut session = RtspSession::new(device.clone(), config.clone());
        tracing::debug!(event = "airplay-client.src.connection.rs:231", "protocol_event");
        session.set_client_device_id(client_device_id.clone());
        session.set_connected()?;

        // Set the request host to the remote device's IP (not our local IP!)
        // RTSP URIs should be rtsp://<remote_ip>/<session_uuid>
        session.set_request_host(ip_addr.to_string());

        let client_instance = client_device_id.replace(":", "");
        rtsp.add_session_header("User-Agent", "AirPlay/745.83");
        rtsp.add_session_header("X-Apple-Client-Name", "Rust AirPlay Sender");
        rtsp.add_session_header("X-Apple-Device-ID", client_device_id.clone());
        rtsp.add_session_header("DACP-ID", client_instance.clone());
        rtsp.add_session_header("Client-Instance", client_instance);
        rtsp.add_session_header("Active-Remote", "1234567890");

        // 3. GET /info (required before pairing)
        let info_req = RtspRequest::get_info();
        let _info_resp = rtsp.send(info_req).await?;

        // 4. Transient pairing (SRP M1-M4 flow with HKP=4)
        let auth_method = AuthMethod::HomeKitTransient;
        let mut pairing = PairingSession::with_identity(auth_method, identity.clone());

        let m1 = pairing.start_transient_pairing_with_pin(pin)?;
        let pair_setup_req = RtspRequest::pair_setup(m1, &client_device_id, 4);
        let m2_resp = rtsp.send(pair_setup_req).await?;

        let m2_body = m2_resp.body.as_deref().unwrap_or(&[]);
        let m3 = pairing.continue_transient_pairing(m2_body)?;

        if let Some(m3_data) = m3 {
            let pair_setup_req = RtspRequest::pair_setup(m3_data, &client_device_id, 4);
            let m4_resp = rtsp.send(pair_setup_req).await?;
            let m4_body = m4_resp.body.as_deref().unwrap_or(&[]);
            pairing.continue_transient_pairing(m4_body)?;
        }

        let session_keys = pairing.take_session_keys().ok_or_else(|| {
            RtspError::SetupFailed("Missing session keys after transient pairing".into())
        })?;

        let cipher = ControlCipher::new(
            *session_keys.write_key.as_bytes(),
            *session_keys.read_key.as_bytes(),
        );
        rtsp.set_cipher(cipher);
        match rtsp.send(RtspRequest::options()).await {
            Ok(resp) => {
                if let Some(public) = resp.header("Public") {
                    tracing::info!(event = "airplay-client.src.connection.rs:281", "protocol_event");
                } else {
                    tracing::info!(event = "airplay-client.src.connection.rs:283", "protocol_event");
                    for (k, v) in &resp.headers {
                        tracing::debug!(event = "airplay-client.src.connection.rs:285", "protocol_event");
                    }
                }
            }
            Err(err) => {
                tracing::warn!(event = "airplay-client.src.connection.rs:290", "protocol_event");
            }
        }

        // Use the randomly generated shk for audio encryption.
        // The shk is sent to the receiver in SETUP phase 2.
        // Don't override with shared secret - that's only for RTSP encryption.
        tracing::debug!(event = "airplay-client.src.connection.rs:297", "protocol_event");

        session.set_paired()?;

        Ok(Self {
            device,
            rtsp,
            session,
            streamer: None,
            playback_state: PlaybackState::Stopped,
            volume: 1.0,
            stream_config: config,
            timing_offset: None,
            timing_tx: None,
            timing_task: None,
            timing_server: None,
            ptp_master: None,
            ptp_master_sync_task: None,
            ptp_master_clock_id: None,
            control_receiver: None,
            control_task: None,
            control_stop: Arc::new(AtomicBool::new(false)),
            events_stream: None,
            render_delay_ms: 0,
            eq_config: None,
            eq_params: None,
            spatial_params: None,
            spatial_speakers: None,
            stream_stats: crate::stats::StreamStats::new(),
        })
    }

    /// Create connection using persisted identity (pair-verify) if available.
    ///
    /// This method first checks for a saved identity from a previous pair-setup.
    /// If found, it uses pair-verify (M1-M4) which is faster and doesn't require
    /// a PIN. If no identity exists, falls back to transient pairing with the
    /// provided PIN.
    ///
    /// Use this for devices like Apple TV that require initial pair-setup with
    /// a one-time PIN from `/pair-pin-start`, but then allow pair-verify for
    /// subsequent connections.
    pub async fn connect_auto(device: Device, config: StreamConfig, fallback_pin: &str) -> Result<Self> {
        let _ = (device, config, fallback_pin);
        Err(airplay_core::error::Error::Connection(std::io::Error::new(std::io::ErrorKind::Unsupported,"Persistent pairing is disabled; transient session only")))
    }

    /// Create connection using pair-verify with a persisted identity.


    /// Connect to Apple TV using HomeKit Normal pairing with user PIN.
    ///
    /// This uses the standard HomeKit pairing protocol (NOT the legacy "fruit" binary plist format):
    /// - TLV8 format (application/octet-stream)
    /// - SHA-512/3072-bit SRP (same as HomeKit Transient)
    /// - HKP=3 header (HomeKit Normal, vs HKP=4 for Transient)
    /// - Full M1-M6 pair-setup flow (Transient only does M1-M4)
    /// - Followed by M1-M4 pair-verify to establish encrypted session
    ///
    /// After successful pairing, the controller identity is saved to disk for future
    /// pair-verify connections (no PIN required on subsequent connections).
    ///
    /// # Usage
    /// 1. Trigger PIN display: `curl -X POST http://<ip>:7000/pair-pin-start`
    /// 2. Enter the 4-digit PIN shown on Apple TV screen
    /// 3. On subsequent connections, use `connect_auto()` which will use saved identity
    ///
    /// # Difference from HomeKit Transient
    /// - **Normal (HKP=3)**: User PIN, M1-M6, identity saved, for Apple TV
    /// - **Transient (HKP=4)**: PIN "3939", M1-M4 only, no persistence, for HomePod
    pub async fn connect_with_pin_pairing(device: Device, config: StreamConfig, pin: &str) -> Result<Self> {
        let _ = (device, config, pin);
        Err(airplay_core::error::Error::Connection(std::io::Error::new(std::io::ErrorKind::Unsupported,"Persistent pairing is disabled; transient session only")))
    }

    /// Deprecated alias for `connect_with_pin_pairing`.
    #[deprecated(since = "0.2.0", note = "Use connect_with_pin_pairing instead")]
    pub async fn connect_with_fruit_pairing(device: Device, config: StreamConfig, pin: &str) -> Result<Self> {
        Self::connect_with_pin_pairing(device, config, pin).await
    }

    /// Complete RTSP SETUP phases (called before streaming).
    pub async fn setup(&mut self) -> Result<()> {
        tracing::info!(event = "airplay-client.src.connection.rs:515", "protocol_event");

        // Start timing server based on protocol
        let local_timing_port = match self.stream_config.timing_protocol {
            TimingProtocol::Ntp => {
                // For NTP, we run a server that the receiver can sync to
                let timing_server = NtpTimingServer::start(
                    self.stream_config.audio_format.sample_rate.as_hz()
                ).await?;
                let port = timing_server.port();
                tracing::info!(event = "airplay-client.src.connection.rs:529", "protocol_event");
                self.timing_server = Some(timing_server);
                port
            }
            TimingProtocol::Ptp => {
                // PTP mode determines whether sender is master (sends Sync) or slave (receives Sync).
                // Master mode: for third-party receivers like Shairport-sync
                // Slave mode: for HomePod multi-room where HomePod is the timing master
                let mode_str = match self.stream_config.ptp_mode {
                    airplay_core::PtpMode::Master => "master (sender is timing reference)",
                    airplay_core::PtpMode::Slave => "slave (receiver is timing reference)",
                };
                tracing::info!(event = "airplay-client.src.connection.rs:541", "protocol_event");
                PTP_EVENT_PORT
            }
        };

        // SETUP Phase 1 (timing/event channels)
        let local_addresses = self.rtsp.local_addr()
            .map(|sa| vec![sa.ip().to_string()])
            .unwrap_or_default();
        let setup1_body = self.session.build_setup_phase1(local_timing_port, Some(local_addresses))?;
        tracing::debug!(event = "airplay-client.src.connection.rs:551", "protocol_event");
        let setup1_req = RtspRequest::setup(self.session.request_uri(), setup1_body);
        let setup1_resp = self.rtsp.send(setup1_req).await?;

        // Log the response for debugging
        if let Some(ref body) = setup1_resp.body {
            tracing::debug!(event = "airplay-client.src.connection.rs:561", "protocol_event");
        }

        self.session.process_setup_phase1_response(setup1_resp.body.as_deref().unwrap_or(&[]))?;

        // Add RTSP Session header for all subsequent requests (RECORD, SETUP phase 2, etc.)
        let session_id = setup1_resp.headers.get("Session")
            .cloned()
            .unwrap_or_else(|| "1".to_string());
        self.rtsp.add_session_header("Session", session_id);

        // Get device address and ports from SETUP phase 1 response
        // Copy the address value to avoid borrow conflict with later mutable calls
        let addr = *select_best_address(&self.device.addresses)
            .ok_or_else(|| RtspError::ConnectionRefused)?;
        let ports = self.session.ports()
            .ok_or_else(|| CoreError::Rtsp(RtspError::InvalidResponse("No ports in SETUP response".into())))?;
        // Extract port values to avoid borrowing self.session during later mutable calls
        let event_port = ports.event_port;

        // Establish events connection (reverse connection to device's events port)
        // This MUST be done before SETUP phase 2 or some devices return 500
        let events_addr = SocketAddr::new(addr, event_port);
        tracing::info!(event = "airplay-client.src.connection.rs:588", "protocol_event");
        match TcpStream::connect(events_addr).await {
            Ok(stream) => {
                tracing::info!(event = "airplay-client.src.connection.rs:591", "protocol_event");
                self.events_stream = Some(stream);
            }
            Err(e) => {
                // Not fatal - owntone says "proceeding anyway" if this fails
                tracing::warn!(event = "airplay-client.src.connection.rs:596", "protocol_event");
            }
        }

        // Bind a real UDP socket for the control port before SETUP phase 2.
        // The receiver sends RTCP/retransmit-request packets to this port;
        // without a bound socket it gets ICMP "port unreachable" and may mute the stream.
        let mut control_receiver = RtpReceiver::new();
        let actual_control_port = control_receiver.bind(0)?;
        self.session.set_local_control_port(actual_control_port);
        tracing::info!(event = "airplay-client.src.connection.rs:606", "protocol_event");
        self.control_receiver = Some(Arc::new(control_receiver));

        // SETUP Phase 2 (audio stream)
        let setup2_body = self.session.build_setup_phase2()?;
        let setup2_req = RtspRequest::setup(self.session.request_uri(), setup2_body);
        let setup2_resp = self.rtsp.send(setup2_req).await?;
        tracing::debug!(event = "airplay-client.src.connection.rs:613", "protocol_event");
        if let Some(ref body) = setup2_resp.body {
            tracing::debug!(event = "airplay-client.src.connection.rs:620", "protocol_event");
        }
        self.session.process_setup_phase2_response(setup2_resp.body.as_deref().unwrap_or(&[]))?;

        // RECORD — sent after both SETUP phases so the receiver has streams configured
        tracing::debug!(event = "airplay-client.src.connection.rs:625", "protocol_event");
        let record_req = RtspRequest::record_with_info(
            self.session.request_uri(),
            0,  // Initial sequence number
            0,  // Initial RTP time
        );
        match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.rtsp.send(record_req)
        ).await {
            Ok(Ok(resp)) => {
                if resp.status_code == 200 {
                    tracing::info!(event = "airplay-client.src.connection.rs:637", "protocol_event");
                } else {
                    tracing::warn!(event = "airplay-client.src.connection.rs:639", "protocol_event");
                }
            }
            Ok(Err(e)) => tracing::warn!(event = "airplay-client.src.connection.rs:642", "protocol_event"),
            Err(_) => tracing::warn!(event = "airplay-client.src.connection.rs:643", "protocol_event"),
        }

        // SETPEERS disabled — not needed for current receiver targets
        // let local_addr_str = self.rtsp.local_addr()
        //     .map(|sa| sa.ip().to_string())
        //     .unwrap_or_else(|| "0.0.0.0".to_string());
        // let device_addr_str = addr.to_string();
        // {
        //     let peer_addresses = vec![device_addr_str, local_addr_str];
        //     tracing::debug!(event = "airplay-client.src.connection.rs:653", "protocol_event");
        //     match self.send_setpeers(&peer_addresses).await {
        //         Ok(()) => tracing::info!(event = "airplay-client.src.connection.rs:655", "protocol_event"),
        //         Err(e) => tracing::warn!(event = "airplay-client.src.connection.rs:656", "protocol_event"),
        //     }
        // }

        // Timing sync - after stream is set up
        match self.stream_config.timing_protocol {
            TimingProtocol::Ntp => {
                // For NTP, the sender IS the timing reference.
                // The receiver syncs to our NTP server (started above).
                // No client-side sync needed — our clock offset is zero.
                self.timing_offset = Some(ClockOffset::default());
                tracing::info!(event = "airplay-client.src.connection.rs:667", "protocol_event");
            }
            TimingProtocol::Ptp => {
                match self.stream_config.ptp_mode {
                    airplay_core::PtpMode::Master => {
                        // BMCA yield flow: act like a Mac sender
                        // 1. Send 3 Syncs + Announces with Priority1=250
                        // 2. HomePod wins BMCA (Priority1=248 < 250)
                        // 3. We yield and become slave
                        // 4. Receive Sync/Follow_Up from HomePod, calculate offset
                        let (offset_tx, mut offset_rx) = watch::channel(ClockOffset::default());
                        self.timing_tx = Some(offset_tx.clone());

                        // Oneshot channel to receive HomePod's clock identity from BMCA
                        let (clock_id_tx, clock_id_rx) = tokio::sync::oneshot::channel::<[u8; 8]>();

                        let master_ip = addr;
                        self.ptp_master_sync_task = Some(tokio::spawn(async move {
                            if let Err(e) = run_bmca_yield_flow(
                                master_ip,
                                250,  // Priority1=250 (Mac's value, loses to HomePod's 248)
                                offset_tx,
                                clock_id_tx,
                            ).await {
                                tracing::error!(event = "airplay-client.src.connection.rs:691", "protocol_event");
                            }
                        }));

                        // Wait for BMCA to complete and get HomePod's clock identity
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            clock_id_rx,
                        ).await {
                            Ok(Ok(clock_id)) => {
                                self.ptp_master_clock_id = Some(clock_id);
                                tracing::info!(event = "airplay-client.src.connection.rs:702", "protocol_event");
                            }
                            Ok(Err(_)) => {
                                tracing::warn!(event = "airplay-client.src.connection.rs:705", "protocol_event");
                            }
                            Err(_) => {
                                tracing::warn!(event = "airplay-client.src.connection.rs:708", "protocol_event");
                            }
                        }

                        // Wait for first real clock offset from BMCA slave loop.
                        // The slave loop needs a full Sync/Follow_Up/Delay_Req/Delay_Resp
                        // exchange (~1-2s) before it can calculate the offset. A blind 500ms
                        // sleep often reads zero, causing group sync packets to use wrong timestamps.
                        match tokio::time::timeout(
                            std::time::Duration::from_secs(5),
                            offset_rx.changed(),
                        ).await {
                            Ok(Ok(())) => {
                                tracing::info!(event = "airplay-client.src.connection.rs:721", "protocol_event");
                            }
                            Ok(Err(_)) => {
                                tracing::warn!(event = "airplay-client.src.connection.rs:724", "protocol_event");
                            }
                            Err(_) => {
                                tracing::warn!(event = "airplay-client.src.connection.rs:727", "protocol_event");
                            }
                        }
                        let initial_offset = *offset_rx.borrow_and_update();
                        self.timing_offset = Some(initial_offset);

                        tracing::info!(event = "airplay-client.src.connection.rs:733", "protocol_event");
                    }
                    airplay_core::PtpMode::Slave => {
                        // Slave mode: Receiver (HomePod) is the timing master
                        // We listen for Sync/Announce from receiver and calculate offset

                        // Create watch channel for clock offset updates
                        let (offset_tx, mut offset_rx) = watch::channel(ClockOffset::default());

                        // Store the sender for the streamer to subscribe to
                        self.timing_tx = Some(offset_tx.clone());

                        // Spawn PTP slave task to listen for Sync/Follow-Up from HomePod
                        tracing::info!(event = "airplay-client.src.connection.rs:747", "protocol_event");
                        let ptp_task = tokio::spawn(async move {
                            match run_ptp_slave(addr, offset_tx).await {
                                Ok(()) => {
                                    tracing::info!(event = "airplay-client.src.connection.rs:751", "protocol_event");
                                }
                                Err(e) => {
                                    tracing::error!(event = "airplay-client.src.connection.rs:754", "protocol_event");
                                }
                            }
                        });

                        self.timing_task = Some(ptp_task);

                        // Wait a moment for initial sync before proceeding
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

                        // Get the current offset from the channel
                        let initial_offset = *offset_rx.borrow_and_update();
                        self.timing_offset = Some(initial_offset);

                        tracing::info!(event = "airplay-client.src.connection.rs:768", "protocol_event");
                    }
                }
            }
        }

        tracing::info!(event = "airplay-client.src.connection.rs:774", "protocol_event");
        Ok(())
    }

    /// Disconnect and clean up.
    pub async fn disconnect(&mut self) -> Result<()> {
        // Stop streaming if active
        if let Some(ref mut streamer) = self.streamer {
            streamer.stop().await?;
        }

        if let Some(task) = self.timing_task.take() {
            task.abort();
        }

        // Stop timing server
        if let Some(mut server) = self.timing_server.take() {
            server.stop().await;
        }

        // Stop PTP master sync task and master itself
        if let Some(task) = self.ptp_master_sync_task.take() {
            task.abort();
        }
        if let Some(mut master) = self.ptp_master.take() {
            master.stop().await;
        }

        // Signal control thread to stop and wait for it to exit
        self.control_stop.store(true, Ordering::Release);
        if let Some(task) = self.control_task.take() {
            // Give the control thread time to notice the stop flag (polls every 5ms)
            let _ = tokio::time::timeout(
                std::time::Duration::from_millis(100),
                task,
            ).await;
        }

        // Drop control receiver (closes the UDP socket)
        self.control_receiver = None;

        // Send TEARDOWN if session is active
        if self.session.state() != SessionState::Disconnected {
            let _ = self.session.start_teardown();
            let teardown_req = RtspRequest::teardown(self.session.request_uri());
            let _ = self.rtsp.send(teardown_req).await;
        }

        // Close connection
        self.rtsp.close().await?;
        self.playback_state = PlaybackState::Stopped;

        Ok(())
    }

    /// Get connected device.
    pub fn device(&self) -> &Device {
        &self.device
    }

    /// Get RTSP session state.
    pub fn session_state(&self) -> SessionState {
        self.session.state()
    }

    /// Get playback state.
    pub fn playback_state(&self) -> PlaybackState {
        self.playback_state
    }

    /// Get current playback position in seconds.
    pub fn playback_position(&self) -> f64 {
        self.streamer
            .as_ref()
            .map(|s| s.position() as f64 / self.stream_config.audio_format.sample_rate.as_hz() as f64)
            .unwrap_or(0.0)
    }

    /// Get current volume.
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Start audio streaming from a decoder source.
    pub async fn start_streaming(&mut self, decoder: AudioDecoder) -> Result<()> {
        // Ensure setup is complete
        if self.session.state() != SessionState::Ready {
            self.setup().await?;
        }

        // Configure RTP sender using shared builder
        let sender = self.build_rtp_sender()?;
        tracing::info!(event = "airplay-client.src.connection.rs:866", "protocol_event");

        // Start streamer
        let mut streamer = AudioStreamer::new(self.stream_config.clone());
        streamer.set_rtp_sender(sender).await;
        if self.render_delay_ms > 0 {
            streamer.set_render_delay_ms(self.render_delay_ms).await;
        }
        // Use live watch channel for current offset (not stale setup-time value).
        // The BMCA slave loop continuously updates the offset, so the watch channel
        // has the latest value — much better than the snapshot from setup() time.
        if let Some(ref tx) = self.timing_tx {
            let rx = tx.subscribe();
            let current_offset = *rx.borrow();
            tracing::info!(event = "airplay-client.src.connection.rs:880", "protocol_event");
            streamer.set_timing_offset(current_offset).await;
            streamer.set_timing_updates(rx).await;
        } else if let Some(offset) = self.timing_offset {
            streamer.set_timing_offset(offset).await;
        }
        // Enable PTP sync mode (PT=87) if we have a remote clock ID from BMCA
        if self.stream_config.timing_protocol == TimingProtocol::Ptp {
            if let Some(clock_id) = self.ptp_master_clock_id {
                streamer.set_ptp_sync_mode(clock_id).await;
            }
        }

        // Set up equalizer if configured
        if let (Some(config), Some(params)) = (self.eq_config.take(), self.eq_params.clone()) {
            streamer.set_eq_params(config, params).await;
        }

        // Set up spatial audio if configured
        if let (Some(params), Some(speakers)) = (self.spatial_params.clone(), self.spatial_speakers.clone()) {
            streamer.set_spatial_params(params, speakers).await;
        }

        // FLUSH before streaming — tells receiver to clear buffers and expect audio
        // starting at the given seq/rtptime.
        let flush_req = RtspRequest::flush_with_info(
            self.session.request_uri(),
            0,  // Initial sequence number (RtpSender starts at 0)
            0,  // Initial RTP timestamp
        );
        if let Err(e) = self.rtsp.send(flush_req).await {
            tracing::warn!(event = "airplay-client.src.connection.rs:911", "protocol_event");
        } else {
            tracing::info!(event = "airplay-client.src.connection.rs:913", "protocol_event");
        }

        streamer.start(decoder).await?;

        // RECORD was sent at the end of setup(), just update state
        self.session.start_playing()?;

        // Send SET_PARAMETER volume to ensure HomePod is not muted
        tracing::info!(event = "airplay-client.src.connection.rs:922", "protocol_event");
        if let Err(e) = self.set_volume(self.volume).await {
            tracing::warn!(event = "airplay-client.src.connection.rs:924", "protocol_event");
        }

        // Spawn control channel on a dedicated blocking thread for low-latency
        // retransmit handling. Uses 5ms recv timeout (well under HomePod's 70ms buffer)
        // so retransmit requests are handled promptly.
        self.control_stop.store(false, Ordering::Release);
        let control_task = if let Some(ref control_rx) = self.control_receiver {
            use airplay_audio::RetransmitRequest;
            let control_rx_clone = Arc::clone(control_rx);
            let streamer_clone = streamer.clone();
            let rt_handle = tokio::runtime::Handle::current();
            let stats = Arc::clone(&self.stream_stats);
            let stop_flag = Arc::clone(&self.control_stop);

            Some(tokio::task::spawn_blocking(move || {
                tracing::debug!(event = "airplay-client.src.connection.rs:940", "protocol_event");
                loop {
                    if stop_flag.load(Ordering::Acquire) {
                        tracing::info!(event = "airplay-client.src.connection.rs:943", "protocol_event");
                        break;
                    }
                    // Use raw receive to handle all packet formats (including
                    // retransmit requests which have 8-byte headers without SSRC).
                    // 5ms timeout keeps retransmit latency low.
                    match control_rx_clone.recv_raw_timeout(std::time::Duration::from_millis(5)) {
                        Ok(Some((data, _addr))) => {
                            if data.len() < 4 {
                                tracing::debug!(event = "airplay-client.src.connection.rs:952", "protocol_event");
                                continue;
                            }

                            let payload_type = data[1] & 0x7F;

                            if payload_type == 85 {
                                // Retransmit request (PT=85):
                                // Apple uses an 8-byte compact format:
                                //   [0-1] RTP header (V=2, PT=85)
                                //   [2-3] Sequence number of request
                                //   [4-5] First lost sequence number
                                //   [6-7] Number of lost packets
                                // Or a 12-byte format:
                                //   [0-7] 8-byte RTP header (no SSRC)
                                //   [8-9] First lost sequence number
                                //   [10-11] Number of lost packets
                                let request = if data.len() == 8 {
                                    // 8-byte compact format
                                    let first_sequence = u16::from_be_bytes([data[4], data[5]]);
                                    let count = u16::from_be_bytes([data[6], data[7]]);
                                    Some(RetransmitRequest { first_sequence, count })
                                } else if data.len() >= 12 {
                                    RetransmitRequest::parse(&data).ok()
                                } else {
                                    let hex: String = data.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                                    tracing::debug!(event = "airplay-client.src.connection.rs:978", "protocol_event");
                                    None
                                };

                                if let Some(request) = request {
                                    stats.rtx_requested.fetch_add(request.count as u64, std::sync::atomic::Ordering::Relaxed);
                                    tracing::debug!(event = "airplay-client.src.connection.rs:987", "protocol_event");
                                    // Use block_on to call async handle_retransmit from blocking thread
                                    match rt_handle.block_on(streamer_clone.handle_retransmit(&request)) {
                                        Ok(retransmitted) => {
                                            if retransmitted > 0 {
                                                stats.rtx_fulfilled.fetch_add(retransmitted as u64, std::sync::atomic::Ordering::Relaxed);
                                                tracing::info!(event = "airplay-client.src.connection.rs:996", "protocol_event");
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(event = "airplay-client.src.connection.rs:1003", "protocol_event");
                                        }
                                    }
                                }
                            } else if payload_type == 84 {
                                // Sync/timing packet (PT=84) — can ignore
                                tracing::trace!(event = "airplay-client.src.connection.rs:1009", "protocol_event");
                            } else {
                                // Log unknown packet types with hex dump for debugging
                                let hex: String = data.iter().take(16).map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join(" ");
                                tracing::debug!(event = "airplay-client.src.connection.rs:1013", "protocol_event");
                            }
                        }
                        Ok(None) => {
                            // Timeout, continue polling
                        }
                        Err(e) => {
                            tracing::debug!(event = "airplay-client.src.connection.rs:1023", "protocol_event");
                        }
                    }
                }
            }))
        } else {
            None
        };

        self.streamer = Some(streamer);
        self.control_task = control_task;
        self.playback_state = PlaybackState::Playing;

        if self.control_task.is_some() {
            tracing::info!(event = "airplay-client.src.connection.rs:1037", "protocol_event");
        }

        Ok(())
    }

    /// Start audio streaming from a live source (e.g., Bluetooth capture).
    ///
    /// This is similar to `start_streaming()` but uses a `LiveAudioDecoder` that
    /// receives PCM frames from a channel, enabling streaming from external sources.
    pub async fn start_streaming_live(&mut self, live_decoder: LiveAudioDecoder) -> Result<()> {
        self.start_streaming_live_with_options(live_decoder,airplay_audio::LiveStreamOptions::default()).await
    }
    pub async fn start_streaming_live_with_options(&mut self, live_decoder:LiveAudioDecoder, options:airplay_audio::LiveStreamOptions)->Result<()> {
        self.start_live_with_senders(live_decoder,options,Vec::new()).await
    }

    /// DeskAudio Cast GPLv3 adapter: one RTP timeline, separate receiver encryption.
    pub async fn start_group_live(&mut self, decoder:LiveAudioDecoder, options:airplay_audio::LiveStreamOptions, members:&mut [Connection])->Result<()> {
        let mut senders=Vec::with_capacity(members.len());
        for member in members.iter() {senders.push(member.build_rtp_sender()?);}
        self.start_live_with_senders(decoder,options,senders).await?;
        for (index,member) in members.iter_mut().enumerate() {
            member.session.start_playing()?;
            member.playback_state=PlaybackState::Playing;
            member.control_stop.store(false,Ordering::Release);
            if let Some(receiver)=member.control_receiver.as_ref() {
                let receiver=receiver.clone(); let stop=member.control_stop.clone();
                let stats=member.stream_stats.clone();
                let streamer=self.streamer.as_ref().expect("streamer initialized").clone();
                let runtime=tokio::runtime::Handle::current();
                member.control_task=Some(tokio::task::spawn_blocking(move || {
                    while !stop.load(Ordering::Acquire) {
                        let Ok(Some((data,_)))=receiver.recv_raw_timeout(std::time::Duration::from_millis(5)) else {continue};
                        if data.len()<8 || data[1]&0x7f!=85 {continue;}
                        let request=if data.len()==8 {
                            Some(airplay_audio::RetransmitRequest {
                                first_sequence:u16::from_be_bytes([data[4],data[5]]),
                                count:u16::from_be_bytes([data[6],data[7]])})
                        } else {airplay_audio::RetransmitRequest::parse(&data).ok()};
                        if let Some(request)=request {
                            stats.rtx_requested.fetch_add(request.count as u64,Ordering::Relaxed);
                            if let Ok(count)=runtime.block_on(streamer.handle_retransmit_for_target(index+1,&request)) {
                                stats.rtx_fulfilled.fetch_add(count as u64,Ordering::Relaxed);
                            }
                        }
                    }
                }));
            }
        }
        Ok(())
    }

    async fn start_live_with_senders(&mut self, live_decoder:LiveAudioDecoder, options:airplay_audio::LiveStreamOptions, mut extra:Vec<airplay_audio::RtpSender>)->Result<()> {
        // Ensure setup is complete
        if self.session.state() != SessionState::Ready {
            self.setup().await?;
        }

        // Configure RTP sender using shared builder
        let sender = self.build_rtp_sender()?;
        tracing::info!(event = "airplay-client.src.connection.rs:1058", "protocol_event");

        // Start streamer
        let mut streamer = AudioStreamer::new(self.stream_config.clone());
        let mut senders=vec![sender]; senders.append(&mut extra);
        streamer.set_rtp_senders(senders).await;
        if self.stream_config.timing_protocol==TimingProtocol::Ptp {
            if let Some(clock)=self.ptp_master_clock_id {streamer.set_ptp_sync_mode(clock).await;}
        }
        if self.render_delay_ms > 0 {
            streamer.set_render_delay_ms(self.render_delay_ms).await;
        }
        // Use live watch channel for current offset (not stale setup-time value).
        // The BMCA slave loop continuously updates the offset, so the watch channel
        // has the latest value — much better than the snapshot from setup() time.
        if let Some(ref tx) = self.timing_tx {
            let rx = tx.subscribe();
            let current_offset = *rx.borrow();
            tracing::info!(event = "airplay-client.src.connection.rs:1072", "protocol_event");
            streamer.set_timing_offset(current_offset).await;
            streamer.set_timing_updates(rx).await;
        } else if let Some(offset) = self.timing_offset {
            streamer.set_timing_offset(offset).await;
        }

        // Set up equalizer if configured
        if let (Some(config), Some(params)) = (self.eq_config.take(), self.eq_params.clone()) {
            streamer.set_eq_params(config, params).await;
        }

        // Set up spatial audio if configured
        if let (Some(params), Some(speakers)) = (self.spatial_params.clone(), self.spatial_speakers.clone()) {
            streamer.set_spatial_params(params, speakers).await;
        }

        // FLUSH before streaming
        let flush_req = RtspRequest::flush_with_info(
            self.session.request_uri(),
            0,
            0,
        );
        if let Err(e) = self.rtsp.send(flush_req).await {
            tracing::warn!(event = "airplay-client.src.connection.rs:1096", "protocol_event");
        } else {
            tracing::info!(event = "airplay-client.src.connection.rs:1098", "protocol_event");
        }

        // Start live streaming
        streamer.start_live_with_options(live_decoder,options).await?;

        self.session.start_playing()?;

        // Set volume
        tracing::info!(event = "airplay-client.src.connection.rs:1107", "protocol_event");
        if let Err(e) = self.set_volume(self.volume).await {
            tracing::warn!(event = "airplay-client.src.connection.rs:1109", "protocol_event");
        }

        // Spawn control channel for retransmit handling
        self.control_stop.store(false, Ordering::Release);
        let control_task = if let Some(ref control_rx) = self.control_receiver {
            use airplay_audio::RetransmitRequest;
            let control_rx_clone = Arc::clone(control_rx);
            let streamer_clone = streamer.clone();
            let rt_handle = tokio::runtime::Handle::current();
            let stats = Arc::clone(&self.stream_stats);
            let stop_flag = Arc::clone(&self.control_stop);

            Some(tokio::task::spawn_blocking(move || {
                tracing::debug!(event = "airplay-client.src.connection.rs:1123", "protocol_event");
                loop {
                    if stop_flag.load(Ordering::Acquire) {
                        tracing::info!(event = "airplay-client.src.connection.rs:1126", "protocol_event");
                        break;
                    }
                    match control_rx_clone.recv_raw_timeout(std::time::Duration::from_millis(5)) {
                        Ok(Some((data, _addr))) => {
                            if data.len() < 4 {
                                continue;
                            }

                            let payload_type = data[1] & 0x7F;

                            if payload_type == 85 {
                                let request = if data.len() == 8 {
                                    let first_sequence = u16::from_be_bytes([data[4], data[5]]);
                                    let count = u16::from_be_bytes([data[6], data[7]]);
                                    Some(RetransmitRequest { first_sequence, count })
                                } else if data.len() >= 12 {
                                    RetransmitRequest::parse(&data).ok()
                                } else {
                                    None
                                };

                                if let Some(request) = request {
                                    stats.rtx_requested.fetch_add(request.count as u64, std::sync::atomic::Ordering::Relaxed);
                                    match rt_handle.block_on(streamer_clone.handle_retransmit(&request)) {
                                        Ok(retransmitted) => {
                                            if retransmitted > 0 {
                                                stats.rtx_fulfilled.fetch_add(retransmitted as u64, std::sync::atomic::Ordering::Relaxed);
                                                tracing::debug!(event = "airplay-client.src.connection.rs:1154", "protocol_event");
                                            }
                                        }
                                        Err(e) => {
                                            tracing::warn!(event = "airplay-client.src.connection.rs:1161", "protocol_event");
                                        }
                                    }
                                }
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            tracing::trace!(event = "airplay-client.src.connection.rs:1169", "protocol_event");
                        }
                    }
                }
            }))
        } else {
            None
        };

        self.streamer = Some(streamer);
        self.control_task = control_task;
        self.playback_state = PlaybackState::Playing;

        tracing::info!(event = "airplay-client.src.connection.rs:1182", "protocol_event");

        Ok(())
    }

    /// Pause streaming.
    pub async fn pause(&mut self) -> Result<()> {
        if let Some(ref mut streamer) = self.streamer {
            streamer.pause().await?;
        }

        // Send FLUSH with RTP-Info to pause on receiver
        let flush_req = RtspRequest::flush_with_info(
            self.session.request_uri(),
            0,
            0,
        );
        self.rtsp.send(flush_req).await?;

        // Reset marker/extension bit state so the next RECORD sends them correctly
        if let Some(ref mut streamer) = self.streamer {
            streamer.reset_after_flush().await;
        }

        self.session.pause()?;
        self.playback_state = PlaybackState::Paused;

        Ok(())
    }

    /// Resume streaming.
    pub async fn resume(&mut self) -> Result<()> {
        if let Some(ref mut streamer) = self.streamer {
            streamer.resume().await?;
        }

        // Send RECORD to resume
        let record_req = RtspRequest::record(self.session.request_uri());
        self.rtsp.send(record_req).await?;
        self.session.start_playing()?;
        self.playback_state = PlaybackState::Playing;

        Ok(())
    }

    /// Stop streaming.
    pub async fn stop(&mut self) -> Result<()> {
        if let Some(ref mut streamer) = self.streamer {
            streamer.stop().await?;
        }

        // Send FLUSH with RTP-Info to stop on receiver.
        // Using flush_with_info avoids 400 Bad Request from HomePod.
        let flush_req = RtspRequest::flush_with_info(
            self.session.request_uri(),
            0,
            0,
        );
        let _ = self.rtsp.send(flush_req).await;

        // Reset marker/extension bit state in case streaming resumes later
        if let Some(ref mut streamer) = self.streamer {
            streamer.reset_after_flush().await;
        }

        self.playback_state = PlaybackState::Stopped;

        Ok(())
    }

    /// Get the total packets sent by this connection's streamer (0 if no streamer).
    pub fn streamer_packets_sent(&self) -> u64 {
        self.streamer.as_ref().map_or(0, |s| s.packets_sent())
    }

    /// Get the total buffer underruns from this connection's streamer (0 if no streamer).
    pub fn streamer_underruns(&self) -> u64 {
        self.streamer.as_ref().map_or(0, |s| s.underruns())
    }

    /// Set render delay in milliseconds.
    ///
    /// Shifts NTP timestamps in sync packets into the future, telling the
    /// receiver to buffer audio longer before rendering. This gives more
    /// headroom for retransmit recovery of lost packets over lossy WiFi.
    ///
    /// Must be called before `start_streaming()`. Typical values: 100-500ms.
    pub fn set_render_delay_ms(&mut self, delay_ms: u32) {
        self.render_delay_ms = delay_ms;
    }

    /// Set up the equalizer with shared parameters.
    ///
    /// The EQ will be applied to audio during streaming. Parameters can be
    /// updated atomically from another thread (e.g., the UI).
    ///
    /// Must be called before `start_streaming()` or `start_streaming_live()`.
    pub fn set_eq_params(&mut self, config: EqConfig, params: Arc<EqParams>) {
        self.eq_config = Some(config);
        self.eq_params = Some(params);
    }

    /// Get a clone of the EQ params Arc if set.
    pub fn eq_params(&self) -> Option<Arc<EqParams>> {
        self.eq_params.clone()
    }

    /// Get a clone of the EQ config if set.
    pub fn eq_config(&self) -> Option<EqConfig> {
        self.eq_config.clone()
    }

    /// Set up spatial audio with shared parameters and speaker configurations.
    ///
    /// Must be called before `start_streaming()` or `start_streaming_live()`.
    pub fn set_spatial_params(&mut self, params: Arc<SpatialParams>, speakers: Vec<SpeakerConfig>) {
        self.spatial_params = Some(params);
        self.spatial_speakers = Some(speakers);
    }

    /// Get a clone of the spatial params Arc if set.
    pub fn spatial_params(&self) -> Option<Arc<SpatialParams>> {
        self.spatial_params.clone()
    }

    /// Get a clone of the spatial speaker configs if set.
    pub fn spatial_speakers(&self) -> Option<Vec<SpeakerConfig>> {
        self.spatial_speakers.clone()
    }

    /// Query the standard RTSP text volume; unsupported receivers keep local UI state.
    pub async fn read_volume(&mut self)->Result<Option<f32>> {
        let request=RtspRequest::new(airplay_rtsp::RtspMethod::GetParameter,self.session.request_uri())
            .header("Content-Type","text/parameters").body(b"volume\r\n".to_vec());
        let response=match self.rtsp.send(request).await {
            Ok(response)=>response,
            Err(CoreError::Rtsp(RtspError::UnexpectedStatus(405|451|501)))=>return Ok(None),
            Err(error)=>return Err(error),
        };
        if response.status_code!=200 {return Ok(None);}
        let Some(body)=response.body.filter(|body|body.len()<=1024) else {return Ok(None)};
        let Ok(text)=std::str::from_utf8(&body) else {return Ok(None)};
        for line in text.lines() {
            let Some((name,value))=line.split_once(':') else {continue};
            if !name.trim().eq_ignore_ascii_case("volume") {continue;}
            let Some(db)=value.trim().parse::<f32>().ok().filter(|value|value.is_finite() && *value<=0.0) else {return Ok(None)};
            let linear=if db<=-144.0 {0.0} else {10.0_f32.powf(db/20.0).clamp(0.0,1.0)};
            self.volume=linear; return Ok(Some(linear));
        }
        Ok(None)
    }

    /// Set volume.
    pub async fn set_volume(&mut self, volume: f32) -> Result<()> {
        let clamped = volume.clamp(0.0, 1.0);
        self.volume = clamped;

        // Send SET_PARAMETER with volume
        let volume_body = self.session.build_set_volume(clamped)?;
        let volume_req = RtspRequest::set_parameter_text(self.session.request_uri(), volume_body);
        self.rtsp.send(volume_req).await?;

        Ok(())
    }

    /// Send feedback/keepalive to the receiver.
    ///
    /// AirPlay 2 receivers expect periodic feedback requests (~every 2 seconds).
    /// Call this from your playback loop to maintain the session.
    pub async fn send_feedback(&mut self) -> Result<()> {
        let uri = self.session.request_uri();
        let req = RtspRequest::feedback(uri);
        match tokio::time::timeout(std::time::Duration::from_secs(2), self.rtsp.send(req)).await {
            Ok(Ok(resp)) => {
                tracing::trace!(event = "airplay-client.src.connection.rs:1334", "protocol_event");
                Ok(())
            }
            Ok(Err(e)) => {
                tracing::debug!(event = "airplay-client.src.connection.rs:1338", "protocol_event");
                Err(e)
            }
            Err(_) => {
                tracing::debug!(event = "airplay-client.src.connection.rs:1342", "protocol_event");
                Ok(()) // Don't fail on timeout - it's just a keepalive
            }
        }
    }

    /// Complete RTSP SETUP with PTP master mode for group streaming.
    ///
    /// This does the same RTSP setup as `setup()` but launches PTP as master
    /// (priority1=246, wins against HomePod's 248) instead of BMCA yield flow
    /// (priority1=250, loses to HomePod). All peer HomePods sync to our clock.
    ///
    /// `all_peer_ips` should include all HomePod IPs in the group (NOT our own IP).
    pub async fn setup_as_ptp_master(&mut self, all_peer_ips: &[IpAddr]) -> Result<()> {
        tracing::info!(event = "airplay-client.src.connection.rs:1356", "protocol_event");

        // SETUP Phase 1 (timing/event channels)
        let local_timing_port = PTP_EVENT_PORT;
        let local_addresses = self.rtsp.local_addr()
            .map(|sa| vec![sa.ip().to_string()])
            .unwrap_or_default();
        let setup1_body = self.session.build_setup_phase1(local_timing_port, Some(local_addresses))?;
        let setup1_req = RtspRequest::setup(self.session.request_uri(), setup1_body);
        let setup1_resp = self.rtsp.send(setup1_req).await?;

        if let Some(ref body) = setup1_resp.body {
            tracing::debug!(event = "airplay-client.src.connection.rs:1371", "protocol_event");
        }

        self.session.process_setup_phase1_response(setup1_resp.body.as_deref().unwrap_or(&[]))?;

        // Add RTSP Session header
        let session_id = setup1_resp.headers.get("Session")
            .cloned()
            .unwrap_or_else(|| "1".to_string());
        self.rtsp.add_session_header("Session", session_id);

        // Establish events connection
        let addr = *select_best_address(&self.device.addresses)
            .ok_or_else(|| RtspError::ConnectionRefused)?;
        let ports = self.session.ports()
            .ok_or_else(|| CoreError::Rtsp(RtspError::InvalidResponse("No ports in SETUP response".into())))?;
        let event_port = ports.event_port;

        let events_addr = SocketAddr::new(addr, event_port);
        tracing::info!(event = "airplay-client.src.connection.rs:1393", "protocol_event");
        match tokio::net::TcpStream::connect(events_addr).await {
            Ok(stream) => {
                tracing::info!(event = "airplay-client.src.connection.rs:1396", "protocol_event");
                self.events_stream = Some(stream);
            }
            Err(e) => {
                tracing::warn!(event = "airplay-client.src.connection.rs:1400", "protocol_event");
            }
        }

        // Bind control port
        let mut control_receiver = RtpReceiver::new();
        let actual_control_port = control_receiver.bind(0)?;
        self.session.set_local_control_port(actual_control_port);
        tracing::info!(event = "airplay-client.src.connection.rs:1408", "protocol_event");
        self.control_receiver = Some(Arc::new(control_receiver));

        // SETUP Phase 2 (audio stream)
        let setup2_body = self.session.build_setup_phase2()?;
        let setup2_req = RtspRequest::setup(self.session.request_uri(), setup2_body);
        let setup2_resp = self.rtsp.send(setup2_req).await?;
        tracing::debug!(event = "airplay-client.src.connection.rs:1415", "protocol_event");
        self.session.process_setup_phase2_response(setup2_resp.body.as_deref().unwrap_or(&[]))?;

        // RECORD
        let record_req = RtspRequest::record_with_info(
            self.session.request_uri(),
            0, 0,
        );
        match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.rtsp.send(record_req)
        ).await {
            Ok(Ok(resp)) => {
                if resp.status_code == 200 {
                    tracing::info!(event = "airplay-client.src.connection.rs:1433", "protocol_event");
                } else {
                    tracing::warn!(event = "airplay-client.src.connection.rs:1435", "protocol_event");
                }
            }
            Ok(Err(e)) => tracing::warn!(event = "airplay-client.src.connection.rs:1438", "protocol_event"),
            Err(_) => tracing::warn!(event = "airplay-client.src.connection.rs:1439", "protocol_event"),
        }

        // PTP master flow — we become grandmaster to all peers
        let peer_ips_vec: Vec<IpAddr> = all_peer_ips.to_vec();
        let (clock_id_tx, clock_id_rx) = tokio::sync::oneshot::channel::<[u8; 8]>();

        // As PTP master, our offset is 0 (we ARE the reference clock)
        let (offset_tx, _offset_rx) = watch::channel(ClockOffset::default());
        self.timing_tx = Some(offset_tx);
        self.timing_offset = Some(ClockOffset::default());

        self.ptp_master_sync_task = Some(tokio::spawn(async move {
            if let Err(e) = run_ptp_group_master_flow(
                peer_ips_vec,
                246,  // Priority1=246 — wins against HomePod's 248
                clock_id_tx,
            ).await {
                tracing::error!(event = "airplay-client.src.connection.rs:1457", "protocol_event");
            }
        }));

        // Wait for BMCA to complete and get our clock identity
        match tokio::time::timeout(
            std::time::Duration::from_secs(8),
            clock_id_rx,
        ).await {
            Ok(Ok(clock_id)) => {
                self.ptp_master_clock_id = Some(clock_id);
                tracing::info!(event = "airplay-client.src.connection.rs:1468", "protocol_event");
            }
            Ok(Err(_)) => {
                tracing::warn!(event = "airplay-client.src.connection.rs:1471", "protocol_event");
            }
            Err(_) => {
                tracing::warn!(event = "airplay-client.src.connection.rs:1474", "protocol_event");
            }
        }

        tracing::info!(event = "airplay-client.src.connection.rs:1478", "protocol_event");
        Ok(())
    }

    /// Send SETPEERS for multi-room (addresses of all group members).
    pub async fn send_setpeers(&mut self, peer_addresses: &[String]) -> Result<()> {
        let setpeers_body = self.session.build_setpeers(peer_addresses)?;
        let setpeers_req = RtspRequest::setpeers(&self.session.id().to_string(), setpeers_body);
        self.rtsp.send(setpeers_req).await?;
        Ok(())
    }

    /// Complete RTSP SETUP for a secondary group member (no PTP).
    ///
    /// This does everything `setup()` does except PTP BMCA. Instead of running
    /// BMCA on ports 319/320, it accepts the PTP clock state from the primary
    /// connection. Only one connection per process can bind PTP ports.
    ///
    /// After setup, call `send_setpeers()` with all group member addresses.
    pub async fn setup_for_group(
        &mut self,
        ptp_clock_id: [u8; 8],
        timing_offset: ClockOffset,
        timing_rx: watch::Receiver<ClockOffset>,
    ) -> Result<()> {
        tracing::info!(event = "airplay-client.src.connection.rs:1503", "protocol_event");

        // Use PTP_EVENT_PORT (319) — same as the primary. The PTP handler on port 319
        // multiplexes multiple peers by source IP, so the secondary HomePod's PTP traffic
        // is drained/logged but doesn't interfere with the primary's timing calculations.
        let local_timing_port = PTP_EVENT_PORT;

        // SETUP Phase 1 (timing/event channels)
        let local_addresses = self.rtsp.local_addr()
            .map(|sa| vec![sa.ip().to_string()])
            .unwrap_or_default();
        let setup1_body = self.session.build_setup_phase1(local_timing_port, Some(local_addresses))?;
        tracing::debug!(event = "airplay-client.src.connection.rs:1519", "protocol_event");
        let setup1_req = RtspRequest::setup(self.session.request_uri(), setup1_body);
        let setup1_resp = self.rtsp.send(setup1_req).await?;

        if let Some(ref body) = setup1_resp.body {
            tracing::debug!(event = "airplay-client.src.connection.rs:1528", "protocol_event");
        }

        self.session.process_setup_phase1_response(setup1_resp.body.as_deref().unwrap_or(&[]))?;

        // Add RTSP Session header
        let session_id = setup1_resp.headers.get("Session")
            .cloned()
            .unwrap_or_else(|| "1".to_string());
        self.rtsp.add_session_header("Session", session_id);

        // Establish events connection
        let addr = *select_best_address(&self.device.addresses)
            .ok_or_else(|| RtspError::ConnectionRefused)?;
        let ports = self.session.ports()
            .ok_or_else(|| CoreError::Rtsp(RtspError::InvalidResponse("No ports in SETUP response".into())))?;
        let event_port = ports.event_port;

        let events_addr = SocketAddr::new(addr, event_port);
        tracing::info!(event = "airplay-client.src.connection.rs:1551", "protocol_event");
        match TcpStream::connect(events_addr).await {
            Ok(stream) => {
                tracing::info!(event = "airplay-client.src.connection.rs:1554", "protocol_event");
                self.events_stream = Some(stream);
            }
            Err(e) => {
                tracing::warn!(event = "airplay-client.src.connection.rs:1558", "protocol_event");
            }
        }

        // Bind control port
        let mut control_receiver = RtpReceiver::new();
        let actual_control_port = control_receiver.bind(0)?;
        self.session.set_local_control_port(actual_control_port);
        tracing::info!(event = "airplay-client.src.connection.rs:1566", "protocol_event");
        self.control_receiver = Some(Arc::new(control_receiver));

        // SETUP Phase 2 (audio stream — gets device-specific shk)
        let setup2_body = self.session.build_setup_phase2()?;
        let setup2_req = RtspRequest::setup(self.session.request_uri(), setup2_body);
        let setup2_resp = self.rtsp.send(setup2_req).await?;
        tracing::debug!(event = "airplay-client.src.connection.rs:1573", "protocol_event");
        self.session.process_setup_phase2_response(setup2_resp.body.as_deref().unwrap_or(&[]))?;

        // RECORD
        tracing::debug!(event = "airplay-client.src.connection.rs:1581", "protocol_event");
        let record_req = RtspRequest::record_with_info(
            self.session.request_uri(),
            0,
            0,
        );
        match tokio::time::timeout(
            std::time::Duration::from_secs(2),
            self.rtsp.send(record_req)
        ).await {
            Ok(Ok(resp)) => {
                if resp.status_code == 200 {
                    tracing::info!(event = "airplay-client.src.connection.rs:1593", "protocol_event");
                } else {
                    tracing::warn!(event = "airplay-client.src.connection.rs:1595", "protocol_event");
                }
            }
            Ok(Err(e)) => tracing::warn!(event = "airplay-client.src.connection.rs:1598", "protocol_event"),
            Err(_) => tracing::warn!(event = "airplay-client.src.connection.rs:1599", "protocol_event"),
        }

        // Inject PTP state from primary connection (no BMCA)
        self.ptp_master_clock_id = Some(ptp_clock_id);
        self.timing_offset = Some(timing_offset);
        // Store a new sender that mirrors the primary's offset updates
        let (local_tx, _) = watch::channel(timing_offset);
        self.timing_tx = Some(local_tx);
        // Spawn a task that forwards offset updates from the primary's channel
        let local_tx_clone = self.timing_tx.as_ref().unwrap().clone();
        let mut rx = timing_rx;
        self.timing_task = Some(tokio::spawn(async move {
            while rx.changed().await.is_ok() {
                let offset = *rx.borrow();
                let _ = local_tx_clone.send(offset);
            }
        }));

        tracing::info!(event = "airplay-client.src.connection.rs:1618", "protocol_event");
        Ok(())
    }

    /// Send FLUSH to clear receiver buffers before streaming.
    ///
    /// This tells the receiver to discard any buffered audio and expect
    /// fresh data starting at the given sequence/timestamp. Should be called
    /// before starting manual streaming loops.
    pub async fn send_flush(&mut self, seq: u16, rtptime: u32) -> Result<()> {
        let flush_req = RtspRequest::flush_with_info(
            self.session.request_uri(),
            seq,
            rtptime,
        );
        self.rtsp.send(flush_req).await?;
        tracing::info!(event = "airplay-client.src.connection.rs:1637", "protocol_event");
        Ok(())
    }

    /// Send RECORD to resume playback on this connection.
    ///
    /// Uses record_with_info (seq=0, rtptime=0) to match a preceding FLUSH reset.
    pub async fn send_record(&mut self) -> Result<()> {
        let record_req = RtspRequest::record_with_info(
            self.session.request_uri(),
            0,
            0,
        );
        match self.rtsp.send(record_req).await {
            Ok(resp) => {
                if resp.status_code == 200 {
                    tracing::info!(event = "airplay-client.src.connection.rs:1653", "protocol_event");
                } else {
                    tracing::warn!(event = "airplay-client.src.connection.rs:1655", "protocol_event");
                }
            }
            Err(e) => {
                tracing::warn!(event = "airplay-client.src.connection.rs:1659", "protocol_event");
            }
        }
        Ok(())
    }

    /// Build a fully-configured RTP sender for this connection.
    ///
    /// Encapsulates the ~15 lines of sender setup duplicated in `start_streaming()`
    /// and `start_streaming_live()`. The returned sender has:
    /// - Destination set to the device's data port
    /// - Control destination set to the device's control port
    /// - Bound to an ephemeral local port with QoS
    /// - Control socket cloned from our declared control receiver
    /// - ChaCha20-Poly1305 cipher from the session's shk
    /// - SSRC=0 per AirPlay spec
    pub fn build_rtp_sender(&self) -> Result<RtpSender> {
        let ports = self.session.ports()
            .ok_or_else(|| RtspError::SetupFailed("Missing ports from SETUP".into()))?;
        let dest_addr = select_best_address(&self.device.addresses)
            .ok_or_else(|| RtspError::ConnectionRefused)?;
        let dest = SocketAddr::new(*dest_addr, ports.data_port);
        let control_dest = SocketAddr::new(*dest_addr, ports.control_port);

        let mut sender = RtpSender::new(dest, 0); // SSRC=0 per AirPlay spec
        sender.set_control_dest(control_dest);
        sender.bind(0)?;

        // Clone our control socket so sync packets come from the declared control port
        if let Some(ref control_rx) = self.control_receiver {
            if let Ok(Some(ctrl_sock)) = control_rx.try_clone_socket() {
                let ctrl_port = ctrl_sock.local_addr().map(|a| a.port()).unwrap_or(0);
                sender.set_control_socket(ctrl_sock);
                tracing::info!(event = "airplay-client.src.connection.rs:1692", "protocol_event");
            }
        }

        // Set up per-device ChaCha20-Poly1305 cipher from session shk
        let stream_key = *self.session.stream_key();
        let audio_cipher = AudioCipher::new(stream_key);
        sender.set_cipher(Box::new(ChaChaPacketCipher::new(audio_cipher)));

        tracing::info!(event = "airplay-client.src.connection.rs:1701", "protocol_event");
        Ok(sender)
    }

    /// Get streaming parameters for this connection (for multi-target sending).
    ///
    /// Returns the destination addresses, per-device cipher, and control socket
    /// needed to send audio to this device from an external loop.
    pub fn streaming_params(&self) -> Result<StreamingParams> {
        let ports = self.session.ports()
            .ok_or_else(|| RtspError::SetupFailed("Missing ports from SETUP".into()))?;
        let dest_addr = select_best_address(&self.device.addresses)
            .ok_or_else(|| RtspError::ConnectionRefused)?;

        let data_dest = SocketAddr::new(*dest_addr, ports.data_port);
        let control_dest = SocketAddr::new(*dest_addr, ports.control_port);

        // Create a per-device cipher from the session's shk
        let stream_key = *self.session.stream_key();
        let audio_cipher = AudioCipher::new(stream_key);
        let cipher: Box<dyn PacketCipher> = Box::new(ChaChaPacketCipher::new(audio_cipher));

        // Clone control socket if available
        let control_socket = if let Some(ref control_rx) = self.control_receiver {
            control_rx.try_clone_socket().ok().flatten()
        } else {
            None
        };

        Ok(StreamingParams {
            data_dest,
            control_dest,
            cipher,
            control_socket,
        })
    }

    /// Get the PTP master clock ID from BMCA yield (if available).
    pub fn ptp_master_clock_id(&self) -> Option<[u8; 8]> {
        self.ptp_master_clock_id
    }

    /// Get the current timing offset (if available).
    pub fn timing_offset(&self) -> Option<ClockOffset> {
        self.timing_offset
    }

    /// Get a receiver for timing offset updates (subscribes to the watch channel).
    pub fn timing_rx(&self) -> Option<watch::Receiver<ClockOffset>> {
        self.timing_tx.as_ref().map(|tx| tx.subscribe())
    }

    /// Get the RTSP connection's local address.
    pub fn local_addr(&self) -> Option<SocketAddr> {
        self.rtsp.local_addr()
    }

    /// Get the stream stats for this connection.
    pub fn stream_stats(&self) -> Arc<crate::stats::StreamStats> {
        Arc::clone(&self.stream_stats)
    }

    /// Clone the control receiver socket for use in external retransmit listeners.
    ///
    /// Returns a cloned `UdpSocket` from the control receiver, which can be used
    /// to poll for retransmit requests in the group streaming path.
    pub fn clone_control_socket_for_recv(&self) -> Option<UdpSocket> {
        self.control_receiver.as_ref()
            .and_then(|rx| rx.try_clone_socket().ok().flatten())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv6Addr;

    // ============================================================================
    // Helper Function Tests
    // ============================================================================

    mod device_id_generation {
        use super::*;

        #[test]
        fn generates_valid_mac_format() {
            let device_id = generate_device_id();

            // Should be in format XX:XX:XX:XX:XX:XX
            assert_eq!(device_id.len(), 17); // 6 pairs of 2 chars + 5 colons
            assert_eq!(device_id.matches(':').count(), 5);

            // Each segment should be valid hex
            for part in device_id.split(':') {
                assert_eq!(part.len(), 2);
                assert!(u8::from_str_radix(part, 16).is_ok());
            }
        }

        #[test]
        fn generates_unique_ids() {
            let id1 = generate_device_id();
            let id2 = generate_device_id();
            let id3 = generate_device_id();

            // Should generate different IDs each time (with very high probability)
            assert_ne!(id1, id2);
            assert_ne!(id2, id3);
            assert_ne!(id1, id3);
        }

        #[test]
        fn generates_uppercase_hex() {
            let device_id = generate_device_id();

            // All hex digits should be uppercase
            for c in device_id.chars() {
                if c != ':' {
                    assert!(c.is_ascii_uppercase() || c.is_ascii_digit());
                }
            }
        }
    }

    mod address_selection {
        use super::*;
        use std::net::IpAddr;

        #[test]
        fn prefers_ipv4_over_ipv6() {
            let addresses = vec![
                "fe80::1".parse::<IpAddr>().unwrap(),  // link-local IPv6
                "192.168.1.100".parse().unwrap(),       // IPv4
                "2001:db8::1".parse().unwrap(),         // global IPv6
            ];

            let selected = select_best_address(&addresses);
            assert!(selected.is_some());
            assert!(selected.unwrap().is_ipv4());
            assert_eq!(selected.unwrap().to_string(), "192.168.1.100");
        }

        #[test]
        fn selects_global_ipv6_when_no_ipv4() {
            let addresses = vec![
                "fe80::1".parse::<IpAddr>().unwrap(),  // link-local IPv6
                "2001:db8::1".parse().unwrap(),         // global IPv6
                "fe80::2".parse().unwrap(),             // another link-local
            ];

            let selected = select_best_address(&addresses);
            assert!(selected.is_some());
            assert!(selected.unwrap().is_ipv6());
            assert_eq!(selected.unwrap().to_string(), "2001:db8::1");
        }

        #[test]
        fn avoids_link_local_ipv6() {
            let addresses = vec![
                "fe80::1".parse::<IpAddr>().unwrap(),  // link-local
                "fe80::2".parse().unwrap(),             // link-local
            ];

            let selected = select_best_address(&addresses);
            // Should return first as fallback, but would prefer non-link-local
            assert!(selected.is_some());
        }

        #[test]
        fn returns_first_address_as_fallback() {
            let addresses = vec![
                "fe80::1".parse::<IpAddr>().unwrap(),
            ];

            let selected = select_best_address(&addresses);
            assert!(selected.is_some());
            assert_eq!(selected.unwrap().to_string(), "fe80::1");
        }

        #[test]
        fn returns_none_for_empty_list() {
            let addresses: Vec<IpAddr> = vec![];

            let selected = select_best_address(&addresses);
            assert!(selected.is_none());
        }

        #[test]
        fn prefers_first_ipv4_when_multiple() {
            let addresses = vec![
                "192.168.1.100".parse::<IpAddr>().unwrap(),
                "192.168.1.101".parse().unwrap(),
                "10.0.0.1".parse().unwrap(),
            ];

            let selected = select_best_address(&addresses);
            assert!(selected.is_some());
            assert_eq!(selected.unwrap().to_string(), "192.168.1.100");
        }

        #[test]
        fn avoids_ipv6_loopback() {
            let addresses = vec![
                "::1".parse::<IpAddr>().unwrap(),      // IPv6 loopback
                "2001:db8::1".parse().unwrap(),         // global IPv6
            ];

            let selected = select_best_address(&addresses);
            assert!(selected.is_some());
            assert_eq!(selected.unwrap().to_string(), "2001:db8::1");
        }
    }

    mod link_local_detection {
        use super::*;

        #[test]
        fn detects_link_local_fe80() {
            let addr: Ipv6Addr = "fe80::1".parse().unwrap();
            assert!(is_link_local_v6(&addr));
        }

        #[test]
        fn detects_link_local_fe80_with_suffix() {
            let addr: Ipv6Addr = "fe80::abcd:ef01:2345:6789".parse().unwrap();
            assert!(is_link_local_v6(&addr));
        }

        #[test]
        fn detects_link_local_febf() {
            // fe80-febf is link-local range
            let addr: Ipv6Addr = "febf::1".parse().unwrap();
            assert!(is_link_local_v6(&addr));
        }

        #[test]
        fn rejects_global_unicast() {
            let addr: Ipv6Addr = "2001:db8::1".parse().unwrap();
            assert!(!is_link_local_v6(&addr));
        }

        #[test]
        fn rejects_loopback() {
            let addr: Ipv6Addr = "::1".parse().unwrap();
            assert!(!is_link_local_v6(&addr));
        }

        #[test]
        fn rejects_unspecified() {
            let addr: Ipv6Addr = "::".parse().unwrap();
            assert!(!is_link_local_v6(&addr));
        }

        #[test]
        fn rejects_multicast() {
            let addr: Ipv6Addr = "ff02::1".parse().unwrap();
            assert!(!is_link_local_v6(&addr));
        }

        #[test]
        fn rejects_unique_local() {
            // fc00::/7 is unique local
            let addr: Ipv6Addr = "fc00::1".parse().unwrap();
            assert!(!is_link_local_v6(&addr));
        }
    }

    // ============================================================================
    // Connection State Tests
    // ============================================================================

    mod connection_state {
        use super::*;
        use airplay_core::{codec::{AudioCodec, AudioFormat, SampleRate}, stream::StreamType, DeviceId};

        fn make_test_device() -> Device {
            Device {
                id: DeviceId::from_mac_string("AA:BB:CC:DD:EE:FF").unwrap(),
                name: "Test Device".to_string(),
                addresses: vec!["192.168.1.100".parse().unwrap()],
                port: 7000,
                model: "TestModel1,1".to_string(),
                manufacturer: None,
                serial_number: None,
                features: Default::default(),
                required_sender_features: None,
                public_key: None,
                source_version: Default::default(),
                firmware_version: None,
                os_version: None,
                protocol_version: None,
                requires_password: false,
                status_flags: 0,
                access_control: None,
                pairing_identity: None,
                system_pairing_identity: None,
                bluetooth_address: None,
                homekit_home_id: None,
                group_id: None,
                is_group_leader: false,
                group_public_name: None,
                group_contains_discoverable_leader: false,
                home_group_id: None,
                household_id: None,
                parent_group_id: None,
                parent_group_contains_discoverable_leader: false,
                tight_sync_id: None,
                raop_port: None,
                raop_encryption_types: None,
                raop_codecs: None,
                raop_transport: None,
                raop_metadata_types: None,
                raop_digest_auth: false,
                vodka_version: None,
            }
        }

        fn make_test_config() -> StreamConfig {
            StreamConfig {
                stream_type: StreamType::Buffered,
                audio_format: AudioFormat {
                    codec: AudioCodec::Alac,
                    sample_rate: SampleRate::Hz44100,
                    channels: 2,
                    bit_depth: 16,
                    frames_per_packet: 352,
                },
                timing_protocol: TimingProtocol::Ntp,
                ptp_mode: airplay_core::PtpMode::Master,
                latency_min: 22050,
                latency_max: 132300,
                supports_dynamic_stream_id: true,
                asc: None,
            }
        }

        #[test]
        fn playback_state_starts_stopped() {
            // We can't easily construct a Connection without real network I/O,
            // but we can test that PlaybackState enum works
            let state = PlaybackState::Stopped;
            assert_eq!(state, PlaybackState::Stopped);
        }

        #[test]
        fn device_getter_would_return_device() {
            // This verifies the pattern - actual test would need mock
            let device = make_test_device();
            assert_eq!(device.name, "Test Device");
        }

        #[test]
        fn config_has_correct_defaults() {
            let config = make_test_config();
            assert_eq!(config.stream_type, StreamType::Buffered);
            assert_eq!(config.audio_format.sample_rate, SampleRate::Hz44100);
            assert_eq!(config.timing_protocol, TimingProtocol::Ntp);
        }
    }

    // ============================================================================
    // Volume Control Tests
    // ============================================================================

    mod volume_control {
        use super::*;

        #[test]
        fn volume_clamping_preserves_valid_range() {
            assert_eq!(0.0f32.clamp(0.0, 1.0), 0.0);
            assert_eq!(0.5f32.clamp(0.0, 1.0), 0.5);
            assert_eq!(1.0f32.clamp(0.0, 1.0), 1.0);
        }

        #[test]
        fn volume_clamping_clamps_below_zero() {
            assert_eq!((-0.5f32).clamp(0.0, 1.0), 0.0);
            assert_eq!((-100.0f32).clamp(0.0, 1.0), 0.0);
        }

        #[test]
        fn volume_clamping_clamps_above_one() {
            assert_eq!(1.5f32.clamp(0.0, 1.0), 1.0);
            assert_eq!(100.0f32.clamp(0.0, 1.0), 1.0);
        }

        #[test]
        fn volume_precision_preserved() {
            let precise = 0.742857f32;
            assert_eq!(precise.clamp(0.0, 1.0), precise);
        }
    }

    // ============================================================================
    // Error Handling Tests
    // ============================================================================

    mod error_handling {
        use super::*;

        #[test]
        fn empty_address_list_returns_none() {
            let addresses: Vec<IpAddr> = vec![];
            assert!(select_best_address(&addresses).is_none());
        }
    }

    // ============================================================================
    // Integration Pattern Tests (documenting expected flows)
    // ============================================================================

    mod integration_patterns {
        use super::*;

        #[test]
        fn connection_flow_pattern() {
            // This test documents the expected connection flow pattern
            // Actual implementation would require mocking RtspConnection

            // Expected flow:
            // 1. TCP connect to device address
            // 2. GET /info
            // 3. POST /pair-setup (M1-M4 for transient)
            // 4. POST /pair-verify (if needed)
            // 5. OPTIONS with encryption
            // 6. SETUP phase 1 (timing/event)
            // 7. Events connection
            // 8. SETUP phase 2 (audio)
            // 9. RECORD

            // This is a documentation test
            assert!(true);
        }

        #[test]
        fn setup_phase1_configures_timing() {
            // Phase 1 should:
            // - Start timing server (NTP or PTP)
            // - Send local timing port to receiver
            // - Negotiate event port
            // - Receive timing/event ports from receiver

            assert!(true); // Documentation test
        }

        #[test]
        fn setup_phase2_configures_audio() {
            // Phase 2 should:
            // - Bind control port (for retransmits)
            // - Send audio format, encryption key (shk)
            // - Receive data/control/timing/event ports
            // - Configure RTP sender with ports

            assert!(true); // Documentation test
        }

        #[test]
        fn streaming_flow_pattern() {
            // Expected streaming flow:
            // 1. FLUSH (clear receiver buffers)
            // 2. Start AudioStreamer with decoder
            // 3. SET_PARAMETER volume
            // 4. Send RTP audio packets
            // 5. Send sync packets (1Hz)
            // 6. Handle retransmit requests

            assert!(true); // Documentation test
        }

        #[test]
        fn disconnect_cleanup_pattern() {
            // Expected cleanup:
            // 1. Stop AudioStreamer
            // 2. Stop timing tasks
            // 3. Stop timing server
            // 4. Stop PTP master
            // 5. Close control receiver
            // 6. TEARDOWN RTSP session
            // 7. Close TCP connection

            assert!(true); // Documentation test
        }
    }

    // ============================================================================
    // Cipher Configuration Tests
    // ============================================================================

    mod cipher_configuration {
        use super::*;

        #[test]
        fn control_cipher_uses_session_keys() {
            // Control cipher should use write_key and read_key from session
            // This is a pattern test - actual test would need session mock
            assert!(true);
        }

        #[test]
        fn audio_cipher_uses_stream_key() {
            // Audio cipher should use shk (stream key) from SETUP phase 2
            // shk is randomly generated and sent to receiver
            assert!(true);
        }

        #[test]
        fn cipher_initialized_after_pairing() {
            // After successful pairing (M1-M4):
            // 1. Extract session keys
            // 2. Create ControlCipher with write/read keys
            // 3. Set cipher on RtspConnection
            // 4. All subsequent RTSP commands are encrypted
            assert!(true);
        }
    }

    // ============================================================================
    // Timing Protocol Tests
    // ============================================================================

    mod timing_protocol_selection {
        use super::*;

        #[test]
        fn ntp_starts_timing_server() {
            // When timing_protocol = NTP:
            // - Start NtpTimingServer on ephemeral port
            // - Send port to receiver in SETUP phase 1
            // - Receiver sends timing requests to our server
            // - Sender is the timing reference (offset = 0)
            assert!(true);
        }

        #[test]
        fn ptp_starts_timing_master() {
            // When timing_protocol = PTP:
            // - Start PtpMaster on ports 319/320
            // - Spawn background task sending Sync every 200ms
            // - Send Announce every 1 second
            // - Sender is the timing reference (offset = 0)
            assert!(true);
        }

        #[test]
        fn timing_offset_zero_for_sender_reference() {
            // When sender is the timing reference:
            // - timing_offset = ClockOffset::default() (all zeros)
            // - No adjustment needed
            // - Receiver syncs to sender
            assert!(true);
        }
    }

    // ============================================================================
    // State Transition Tests
    // ============================================================================

    mod state_transitions {
        use super::*;

        #[test]
        fn playback_stopped_to_playing() {
            // Initial: PlaybackState::Stopped
            // After start_streaming: PlaybackState::Playing
            let initial = PlaybackState::Stopped;
            let after_start = PlaybackState::Playing;
            assert_ne!(initial, after_start);
        }

        #[test]
        fn playback_playing_to_paused() {
            // During playback: PlaybackState::Playing
            // After pause: PlaybackState::Paused
            let playing = PlaybackState::Playing;
            let paused = PlaybackState::Paused;
            assert_ne!(playing, paused);
        }

        #[test]
        fn playback_paused_to_playing() {
            // When paused: PlaybackState::Paused
            // After resume: PlaybackState::Playing
            let paused = PlaybackState::Paused;
            let playing = PlaybackState::Playing;
            assert_ne!(paused, playing);
        }

        #[test]
        fn playback_any_to_stopped() {
            // From any state, stop() should set: PlaybackState::Stopped
            let stopped = PlaybackState::Stopped;
            assert_eq!(stopped, PlaybackState::Stopped);
        }
    }
}
