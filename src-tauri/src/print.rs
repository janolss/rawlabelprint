//! Direct TCP RAW printing and HQES status check.
//! Ported from reference/desktop/src/main.js `print-text` / `get-printer-config`.
//! Also keeps short-lived TCP sessions for Browser Print write→read flows.

use crate::config::PrinterInfo;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_millis(250);
const SESSION_IDLE: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterStatus {
    pub print_port: u16,
    pub config_port: u16,
    pub status: String,
    pub error_messages: Vec<String>,
    pub warning_messages: Vec<String>,
    /// Human-readable connect / probe detail (e.g. os error 65).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

struct DeviceSession {
    stream: TcpStream,
    last_used: Instant,
}

/// Open TCP sessions keyed by Browser Print device uid (write then read).
#[derive(Default)]
pub struct DeviceSessionPool {
    inner: Mutex<HashMap<String, DeviceSession>>,
}

impl DeviceSessionPool {
    pub fn new() -> Self {
        Self::default()
    }

    fn purge_idle(map: &mut HashMap<String, DeviceSession>) {
        map.retain(|_, s| s.last_used.elapsed() < SESSION_IDLE);
    }

    pub fn write(&self, uid: &str, printer: &PrinterInfo, data: &[u8]) -> Result<(), String> {
        let mut map = self.inner.lock().map_err(|e| e.to_string())?;
        Self::purge_idle(&mut map);

        if let Some(session) = map.get_mut(uid) {
            match session.stream.write_all(data).and_then(|_| session.stream.flush()) {
                Ok(()) => {
                    session.last_used = Instant::now();
                    return Ok(());
                }
                Err(_) => {
                    map.remove(uid);
                }
            }
        }

        let mut stream = connect_printer(printer)?;
        stream
            .write_all(data)
            .map_err(|e| format!("Failed writing to printer: {e}"))?;
        let _ = stream.flush();
        map.insert(
            uid.to_string(),
            DeviceSession {
                stream,
                last_used: Instant::now(),
            },
        );
        Ok(())
    }

    pub fn read(&self, uid: &str, printer: &PrinterInfo) -> Result<String, String> {
        let mut map = self.inner.lock().map_err(|e| e.to_string())?;
        Self::purge_idle(&mut map);

        if !map.contains_key(uid) {
            // Open a fresh connection so a lone /read still returns something (often empty).
            let stream = connect_printer(printer)?;
            map.insert(
                uid.to_string(),
                DeviceSession {
                    stream,
                    last_used: Instant::now(),
                },
            );
        }

        let session = map.get_mut(uid).ok_or_else(|| "Session missing".to_string())?;
        let _ = session.stream.set_read_timeout(Some(READ_TIMEOUT));
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        loop {
            match session.stream.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > 65536 {
                        break;
                    }
                }
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    break;
                }
                Err(_) => break,
            }
        }
        session.last_used = Instant::now();
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}

fn connect_printer(printer: &PrinterInfo) -> Result<TcpStream, String> {
    let addr: SocketAddr = format!("{}:{}", printer.address, printer.print_port)
        .parse()
        .map_err(|e| format!("Invalid printer address: {e}"))?;
    let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("Could not connect to printer: {e}"))?;
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    Ok(stream)
}

pub fn send_raw_to_printer(printer: &PrinterInfo, data: &str) -> Result<(), String> {
    let mut stream = connect_printer(printer)?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(data.as_bytes())
        .map_err(|e| format!("Failed writing to printer: {e}"))?;
    let _ = stream.flush();
    Ok(())
}

/// Probe printer online status via TCP print port + ~HQES, fallback to HTTP config port.
pub fn get_printer_status(printer: &PrinterInfo) -> PrinterStatus {
    let config_port = if printer.config_port == 0 {
        80
    } else {
        printer.config_port
    };
    let mut status = PrinterStatus {
        print_port: printer.print_port,
        config_port,
        status: "unknown".into(),
        error_messages: Vec::new(),
        warning_messages: Vec::new(),
        detail: None,
    };

    let print_addr: Result<SocketAddr, _> =
        format!("{}:{}", printer.address, printer.print_port).parse();
    let Ok(print_addr) = print_addr else {
        status.status = "offline".into();
        status.detail = Some(format!("Invalid address: {}", printer.address));
        return status;
    };

    match TcpStream::connect_timeout(&print_addr, CONNECT_TIMEOUT) {
        Ok(mut stream) => {
            // Short read timeout: printers often keep the RAW socket open after ~HQES.
            let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
            let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
            if let Err(e) = stream.write_all(b"~HQES\r\n") {
                status.status = "offline".into();
                status.detail = Some(format!("Connected but write failed: {e}"));
                return status;
            }
            let _ = stream.flush();

            let mut buf = Vec::new();
            let mut chunk = [0u8; 2048];
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.len() > 8192 || buf.windows(2).any(|w| w == b"\n\n" || w == b"\r\n")
                        {
                            // Likely complete HQES reply; don't wait for socket close.
                            break;
                        }
                    }
                    Err(e)
                        if e.kind() == std::io::ErrorKind::WouldBlock
                            || e.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        break;
                    }
                    Err(_) => break,
                }
            }

            let response = String::from_utf8_lossy(&buf);
            let (errors, warnings) = decode_hqes(&response);
            status.error_messages = errors;
            status.warning_messages = warnings;
            // Reachable RAW port counts as online even if ~HQES is empty/unsupported.
            status.status = if status.error_messages.is_empty() {
                "online".into()
            } else {
                "offline".into()
            };
            if buf.is_empty() {
                status.detail = Some(format!(
                    "Connected to {}:{} (no ~HQES response)",
                    printer.address, printer.print_port
                ));
            }
            status
        }
        Err(e) => {
            let print_err = format!(
                "TCP {}:{} — {e}",
                printer.address, printer.print_port
            );
            // Web UI is usually on config_port (80). Reachable HTTP helps diagnose Local Network vs RAW port.
            let config_addr: Result<SocketAddr, _> =
                format!("{}:{}", printer.address, config_port).parse();
            if let Ok(config_addr) = config_addr {
                match TcpStream::connect_timeout(&config_addr, CONNECT_TIMEOUT) {
                    Ok(_) => {
                        status.status = "offline".into();
                        status.detail = Some(format!(
                            "{print_err}. HTTP :{config_port} is reachable — check Local Network permission for RawLabelPrint, or that RAW port {} is open",
                            printer.print_port
                        ));
                        status.error_messages.push(print_err);
                        return status;
                    }
                    Err(e2) => {
                        status.status = "offline".into();
                        status.detail = Some(format!(
                            "{print_err}. HTTP :{config_port} also failed: {e2}. Enable Local Network for RawLabelPrint in System Settings"
                        ));
                        status.error_messages.push(print_err);
                        return status;
                    }
                }
            }
            status.status = "offline".into();
            status.detail = Some(print_err.clone());
            status.error_messages.push(print_err);
            status
        }
    }
}

fn decode_hqes(response: &str) -> (Vec<String>, Vec<String>) {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    for line in response.lines() {
        let upper = line.to_uppercase();
        if upper.contains("ERRORS:") {
            if let Some(flags) = extract_hex_flags(line) {
                if flags != 0 {
                    errors.push(format!("Printer reported error flags: 0x{flags:08X}"));
                }
            }
        } else if upper.contains("WARNINGS:") {
            if let Some(flags) = extract_hex_flags(line) {
                if flags != 0 {
                    warnings.push(format!("Printer reported warning flags: 0x{flags:08X}"));
                }
            }
        }
    }

    (errors, warnings)
}

fn extract_hex_flags(line: &str) -> Option<u32> {
    line.split_whitespace().rev().find_map(|tok| {
        let t = tok.trim_start_matches("0x").trim_start_matches("0X");
        u32::from_str_radix(t, 16).ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hqes_nonzero_error_flag() {
        let (errors, warnings) = decode_hqes("ERRORS:         00000001\nWARNINGS:       00000000\n");
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn decode_hqes_clean() {
        let (errors, warnings) = decode_hqes("ERRORS:         00000000\nWARNINGS:       00000000\n");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }
}
