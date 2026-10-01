//! Zebra LAN discovery using the Browser Print UDP protocol (port 4201).
//! Ported from reference/desktop/src/main.js `search-zebra-printers`.

use crate::config::PrinterInfo;
use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

const DISCOVERY_PORT: u16 = 4201;
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const BROADCAST_MESSAGE: [u8; 6] = [0x2e, 0x2c, 0x3a, 0x01, 0x00, 0x00];

fn parse_null_terminated(buf: &[u8], offset: usize) -> String {
    if offset >= buf.len() {
        return String::new();
    }
    let end = buf[offset..]
        .iter()
        .position(|&b| b == 0)
        .map(|i| offset + i)
        .unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[offset..end])
        .trim()
        .to_string()
}

fn broadcast_addresses() -> Vec<Ipv4Addr> {
    let mut out = Vec::new();
    if let Ok(ifaces) = if_addrs::get_if_addrs() {
        for iface in ifaces {
            if iface.is_loopback() {
                continue;
            }
            if let if_addrs::IfAddr::V4(v4) = iface.addr {
                let ip = v4.ip.octets();
                let mask = v4.netmask.octets();
                let bcast = Ipv4Addr::new(
                    ip[0] | (!mask[0]),
                    ip[1] | (!mask[1]),
                    ip[2] | (!mask[2]),
                    ip[3] | (!mask[3]),
                );
                out.push(bcast);
            }
        }
    }
    out.sort();
    out.dedup();
    if out.is_empty() {
        out.push(Ipv4Addr::BROADCAST);
    }
    out
}

/// Blocking UDP discovery. Run on a blocking thread / spawn_blocking.
pub fn search_zebra_printers() -> Result<Vec<PrinterInfo>, String> {
    let socket = UdpSocket::bind(("0.0.0.0", DISCOVERY_PORT))
        .map_err(|e| format!("Failed to bind UDP {DISCOVERY_PORT}: {e}"))?;
    socket
        .set_broadcast(true)
        .map_err(|e| format!("set_broadcast failed: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(250)))
        .map_err(|e| e.to_string())?;

    for bcast in broadcast_addresses() {
        let addr = SocketAddr::from((bcast, DISCOVERY_PORT));
        let _ = socket.send_to(&BROADCAST_MESSAGE, addr);
    }

    let mut found: HashMap<String, PrinterInfo> = HashMap::new();
    let deadline = Instant::now() + DISCOVERY_TIMEOUT;
    let mut buf = [0u8; 1024];

    while Instant::now() < deadline {
        match socket.recv_from(&mut buf) {
            Ok((len, src)) => {
                let msg = &buf[..len];
                // Response magic: 0x3a 0x2c 0x2e, length > 84
                if len > 84 && msg[0] == 0x3a && msg[1] == 0x2c && msg[2] == 0x2e {
                    let address = src.ip().to_string();
                    if found.contains_key(&address) {
                        continue;
                    }
                    let model = parse_null_terminated(msg, 12);
                    let firmware = parse_null_terminated(msg, 40);
                    let serial_number = parse_null_terminated(msg, 84);
                    found.insert(
                        address.clone(),
                        PrinterInfo {
                            name: None,
                            model,
                            firmware,
                            serial_number,
                            address,
                            port: src.port(),
                            print_port: 9100,
                            config_port: 80,
                        },
                    );
                }
            }
            Err(ref e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                continue;
            }
            Err(e) => return Err(format!("UDP recv error: {e}")),
        }
    }

    Ok(found.into_values().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_null_terminated_fields() {
        let mut buf = vec![0u8; 120];
        buf[0] = 0x3a;
        buf[1] = 0x2c;
        buf[2] = 0x2e;
        buf[12..17].copy_from_slice(b"ZD421");
        buf[40..45].copy_from_slice(b"V75.0");
        buf[84..90].copy_from_slice(b"ABC123");
        assert_eq!(parse_null_terminated(&buf, 12), "ZD421");
        assert_eq!(parse_null_terminated(&buf, 40), "V75.0");
        assert_eq!(parse_null_terminated(&buf, 84), "ABC123");
    }

    #[test]
    fn parse_null_terminated_empty_or_short_buffer() {
        assert_eq!(parse_null_terminated(&[], 0), "");
        assert_eq!(parse_null_terminated(&[b'A', b'B'], 5), "");
        assert_eq!(parse_null_terminated(b"ABC", 0), "ABC");
        assert_eq!(parse_null_terminated(b"  hi  \0xx", 0), "hi");
    }
}
