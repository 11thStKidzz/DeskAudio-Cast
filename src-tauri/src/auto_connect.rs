//! A single, opt-in native worker. No dependency on WebView timers/visibility.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use std::time::{Duration, Instant};
#[derive(Default)]
pub struct AutoConnectState {
    pub enabled: AtomicBool,
    pub paused: AtomicBool,
    pub policy: Mutex<RetryPolicy>,
}
impl AutoConnectState {
    pub fn allowed(&self) -> bool {
        self.enabled.load(Ordering::SeqCst) && !self.paused.load(Ordering::SeqCst)
    }
    pub fn configure(&self, enabled: bool) {
        self.paused.store(false, Ordering::SeqCst);
        *self.policy.lock().unwrap() = RetryPolicy::default();
        self.enabled.store(enabled, Ordering::SeqCst);
    }
    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
    }
}
#[derive(Default)]
pub struct RetryPolicy {
    failures: u32,
    next: Option<Instant>,
}
impl RetryPolicy {
    pub fn due(&self, now: Instant) -> bool {
        self.next.is_none_or(|next| now >= next)
    }
    pub fn failed(&mut self, now: Instant) {
        self.failures = self.failures.saturating_add(1);
        self.next = Some(
            now + Duration::from_secs(match self.failures {
                1 => 5,
                2 => 10,
                3 => 20,
                _ => 30,
            }),
        );
    }
    pub fn succeeded(&mut self) {
        *self = Self::default();
    }
}
pub fn authentication_failure(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    [
        "unauthorized",
        "forbidden",
        "authentication",
        "password",
        "pin required",
        "pairing rejected",
    ]
    .iter()
    .any(|word| error.contains(word))
}

pub fn matching_device<'a>(
    devices: impl IntoIterator<Item = &'a cap_core::Device>,
    identity: Option<&str>,
    ip: &str,
    port: u16,
) -> Option<cap_core::Device> {
    devices
        .into_iter()
        .filter(|device| device.available && !device.manual)
        .find(|device| match identity {
            Some(expected) => device.receiver_id() == expected,
            None => {
                device.port == port
                    && device
                        .addresses
                        .iter()
                        .any(|address| address.to_string() == ip)
            }
        })
        .cloned()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_by_default_and_manual_stop_does_not_fight_user() {
        let state = AutoConnectState::default();
        assert!(!state.allowed());
        state.configure(true);
        assert!(state.allowed());
        state.pause();
        assert!(!state.allowed());
        state.configure(true);
        assert!(state.allowed());
        state.configure(false);
        assert!(!state.allowed());
    }
    #[test]
    fn backoff_is_bounded_and_success_resets_it() {
        let mut p = RetryPolicy::default();
        let now = Instant::now();
        assert!(p.due(now));
        for delay in [5, 10, 20, 30, 30, 30] {
            p.failed(now);
            assert!(!p.due(now + Duration::from_secs(delay - 1)));
            assert!(p.due(now + Duration::from_secs(delay)));
        }
        p.succeeded();
        assert!(p.due(now));
        p.failed(now);
        assert!(p.due(now + Duration::from_secs(5)));
    }
    #[test]
    fn credentials_are_not_retried_forever() {
        assert!(authentication_failure("stream: Authentication failed"));
        assert!(!authentication_failure("stream: connection refused"));
    }
    fn receiver(mac: &str, ip: &str) -> cap_core::Device {
        cap_core::Device::from_txt(
            format!("{mac}._airplay._tcp.local."),
            "same friendly name".into(),
            "test.local".into(),
            vec![ip.parse().unwrap()],
            7000,
            std::collections::HashMap::from([
                ("deviceid".into(), mac.into()),
                ("model".into(), "AudioAccessory5,1".into()),
            ]),
        )
    }
    #[test]
    fn known_identity_survives_dhcp_and_rejects_recycled_old_ip() {
        let original = receiver("AA:BB:CC:DD:EE:01", "192.0.2.1");
        let moved = receiver("AA:BB:CC:DD:EE:01", "192.0.2.2");
        let impostor = receiver("AA:BB:CC:DD:EE:02", "192.0.2.1");
        let candidates = [impostor, moved];
        let selected = matching_device(
            &candidates,
            Some(&original.receiver_id()),
            "192.0.2.1",
            7000,
        )
        .unwrap();
        assert_eq!(
            selected.addresses,
            vec!["192.0.2.2".parse::<std::net::IpAddr>().unwrap()]
        );
        assert!(matching_device(
            &candidates[..1],
            Some(&original.receiver_id()),
            "192.0.2.1",
            7000
        )
        .is_none());
    }
    #[test]
    fn legacy_address_requires_exact_port_and_does_not_match_only_by_name() {
        let original = receiver("AA:BB:CC:DD:EE:01", "192.0.2.1");
        assert!(matching_device([&original], None, "192.0.2.1", 7000).is_some());
        assert!(matching_device([&original], None, "192.0.2.1", 7001).is_none());
        assert!(matching_device([&original], None, "192.0.2.2", 7000).is_none());
    }
    #[test]
    fn unavailable_or_unverified_manual_advertisements_are_not_identity_matches() {
        let mut receiver = receiver("AA:BB:CC:DD:EE:01", "192.0.2.1");
        let id = receiver.receiver_id();
        receiver.available = false;
        assert!(matching_device([&receiver], Some(&id), "192.0.2.1", 7000).is_none());
        receiver.available = true;
        receiver.manual = true;
        assert!(matching_device([&receiver], Some(&id), "192.0.2.1", 7000).is_none());
    }
}
