//! DeskAudio Cast connection boundary, not a firewall or a same-subnet proof.
use std::net::IpAddr;

pub fn validate_endpoint(ip: IpAddr, port: u16, discovered: bool) -> Result<(), &'static str> {
    if port == 0 { return Err("invalid_port"); }
    let allowed = match ip {
        IpAddr::V4(v) => v.is_private() || v.is_link_local(),
        IpAddr::V6(v) => {
            if let Some(v4) = v.to_ipv4_mapped() {
                v4.is_private() || v4.is_link_local()
            } else {
                let first = v.segments()[0];
                let local = first & 0xfe00 == 0xfc00 || first & 0xffc0 == 0xfe80;
                // Globally addressed LAN IPv6 is accepted only from a discovered route.
                local || (discovered && first & 0xe000 == 0x2000)
            }
        }
    };
    if allowed { Ok(()) } else { Err("local_network_address_required") }
}

pub fn validate_name(name: &str) -> Result<(), &'static str> {
    if name.len() > 512 || name.chars().any(char::is_control) { Err("invalid_device_name") } else { Ok(()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn accepts_private_networks_and_discovered_global_ipv6() {
        for ip in ["192.168.1.10","10.0.0.2","172.16.2.3","169.254.1.2","fc00::1","fe80::1","::ffff:192.168.1.2"] {
            assert!(validate_endpoint(ip.parse().unwrap(),7000,false).is_ok(), "{ip}");
        }
        assert!(validate_endpoint("2001:db8::1".parse().unwrap(),7000,true).is_ok());
    }
    #[test] fn rejects_public_unspecified_multicast_loopback_and_zero_ports() {
        for ip in ["8.8.8.8","0.0.0.0","127.0.0.1","224.0.0.251","255.255.255.255","::","::1","ff02::fb","2001:db8::1","::ffff:8.8.8.8"] {
            assert!(validate_endpoint(ip.parse().unwrap(),7000,false).is_err(), "{ip}");
        }
        assert!(validate_endpoint("192.168.1.2".parse().unwrap(),0,false).is_err());
        assert!(validate_name("receiver\nlog injection").is_err());
        assert!(validate_name(&"x".repeat(513)).is_err());
        assert!(validate_name("客厅音箱").is_ok());
    }
}
