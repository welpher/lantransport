//! Enumerate LAN IPv4 addresses, primary route first.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

const SKIPPED_PREFIXES: &[&str] = &[
    "lo", "utun", "awdl", "llw", "bridge", "gif", "stf", "anpi", "ap1", "docker", "vmnet",
    "vmenet", "vboxnet", "tap", "tun",
];

fn is_virtual(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    SKIPPED_PREFIXES
        .iter()
        .any(|prefix| name.starts_with(prefix))
}

pub fn ipv4_addresses() -> Vec<Ipv4Addr> {
    let primary = primary_ipv4();
    let mut addresses = Vec::new();

    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
            if is_virtual(&interface.name) {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = interface.addr {
                let ip = v4.ip;
                if ip.is_loopback() || ip.is_link_local() || ip.is_unspecified() {
                    continue;
                }
                if !addresses.contains(&ip) {
                    addresses.push(ip);
                }
            }
        }
    }

    if let Some(primary) = primary {
        addresses.retain(|ip| *ip != primary);
        addresses.insert(0, primary);
    }

    addresses.sort_by_key(|ip| !is_private(*ip));
    addresses
}

pub fn is_private(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip.is_loopback()
}

fn primary_ipv4() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(addr) if !addr.ip().is_loopback() => Some(*addr.ip()),
        _ => None,
    }
}

pub fn port_available(port: u16) -> bool {
    if port == 0 {
        return false;
    }
    // A wildcard bind can succeed while another process listens on 127.0.0.1
    // (SO_REUSEADDR), so check both and release each probe immediately.
    let wildcard = std::net::TcpListener::bind((Ipv4Addr::UNSPECIFIED, port)).is_ok();
    let loopback = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).is_ok();
    wildcard && loopback
}

pub fn urls(port: u16, allow_lan: bool) -> Vec<String> {
    let mut urls = vec![format!("http://127.0.0.1:{port}")];
    if allow_lan {
        for ip in ipv4_addresses() {
            urls.push(format!("http://{ip}:{port}"));
        }
    }
    urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_busy_ports() {
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(!port_available(port));
        assert!(!port_available(0));
    }

    #[test]
    fn ignores_virtual_interfaces() {
        assert!(is_virtual("docker0"));
        assert!(is_virtual("utun3"));
        assert!(is_virtual("vmnet8"));
        assert!(is_virtual("vmenet0"));
        assert!(!is_virtual("en0"));
        assert!(!is_virtual("Ethernet"));
        assert!(!is_virtual("Wi-Fi"));
    }
}
