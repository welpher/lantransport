//! Enumerate LAN IPv4 addresses, primary route first.

use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

pub fn ipv4_addresses() -> Vec<Ipv4Addr> {
    let primary = primary_ipv4();
    let mut addresses = Vec::new();

    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for interface in interfaces {
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

pub fn urls(port: u16, allow_lan: bool) -> Vec<String> {
    let mut urls = vec![format!("http://127.0.0.1:{port}")];
    if allow_lan {
        for ip in ipv4_addresses() {
            urls.push(format!("http://{ip}:{port}"));
        }
    }
    urls
}
