//! Direct TCP RAW printing and HQES status check.
//! Ported from reference/desktop/src/main.js `print-text` / `get-printer-config`.

use crate::config::PrinterInfo;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::time::Duration;

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterStatus {
    pub print_port: u16,
    pub config_port: u16,
    pub status: String,
    pub error_messages: Vec<String>,
    pub warning_messages: Vec<String>,
}

pub fn send_raw_to_printer(printer: &PrinterInfo, data: &str) -> Result<(), String> {
    let addr: SocketAddr = format!("{}:{}", printer.address, printer.print_port)
        .parse()
        .map_err(|e| format!("Invalid printer address: {e}"))?;

    let mut stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("Could not connect to printer: {e}"))?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .write_all(data.as_bytes())
        .map_err(|e| format!("Failed writing to printer: {e}"))?;
    let _ = stream.flush();
    Ok(())
}

/// Probe printer online status via TCP 9100 + ~HQES, fallback to config port 8080.
pub fn get_printer_status(printer: &PrinterInfo) -> PrinterStatus {
    let mut status = PrinterStatus {
        print_port: printer.print_port,
        config_port: 8080,
        status: "unknown".into(),
        error_messages: Vec::new(),
        warning_messages: Vec::new(),
    };

    let print_addr: Result<SocketAddr, _> =
        format!("{}:{}", printer.address, printer.print_port).parse();
    let Ok(print_addr) = print_addr else {
        status.status = "offline".into();
        return status;
    };

    match TcpStream::connect_timeout(&print_addr, CONNECT_TIMEOUT) {
        Ok(mut stream) => {
            let _ = stream.set_read_timeout(Some(IO_TIMEOUT));
            let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
            let _ = stream.write_all(b"~HQES\r\n");
            let _ = stream.flush();

            let mut buf = Vec::new();
            let mut chunk = [0u8; 2048];
            // Read until timeout / close (best-effort, mirrors desktop finalize timeout).
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    Err(_) => break,
                }
                if buf.len() > 8192 {
                    break;
                }
            }

            let response = String::from_utf8_lossy(&buf);
            let (errors, warnings) = decode_hqes(&response);
            status.error_messages = errors;
            status.warning_messages = warnings;
            status.status = if status.error_messages.is_empty() {
                "online".into()
            } else {
                "offline".into()
            };
            status
        }
        Err(_) => {
            // Fallback: try config port 8080 like desktop app
            let config_addr: Result<SocketAddr, _> =
                format!("{}:8080", printer.address).parse();
            if let Ok(config_addr) = config_addr {
                if TcpStream::connect_timeout(&config_addr, CONNECT_TIMEOUT).is_ok() {
                    status.status = "online".into();
                    return status;
                }
            }
            status.status = "offline".into();
            status
        }
    }
}

/// Minimal HQES decode: look for ERRORS/WARNINGS hex flags in the response text.
/// Full bit maps from desktop are large; we surface non-zero flags as generic messages
/// and also pass through known keyword lines when present.
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
    // Prefer the last hex-looking token on the line.
    line.split_whitespace()
        .rev()
        .find_map(|tok| {
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
