//! RAW printing over TCP (LAN) or USB CDC/serial, plus HQES status.
//! Also keeps short-lived sessions for Browser Print write→read flows.

use crate::config::PrinterInfo;
use crate::usb_discovery::find_port_path_by_serial;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const IO_TIMEOUT: Duration = Duration::from_secs(5);
const READ_TIMEOUT: Duration = Duration::from_millis(250);
/// Max wait for answers to status/info/config/SGD queries (Zebra client waits for ETX).
const QUERY_READ_DEADLINE: Duration = Duration::from_secs(2);
const ETX: u8 = 0x03;
const SESSION_IDLE: Duration = Duration::from_secs(30);
const MAX_DEVICE_SESSIONS: usize = 64;
const USB_BAUD: u32 = 115_200;

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

enum PrinterStream {
    Tcp(TcpStream),
    Serial(Box<dyn serialport::SerialPort>),
}

impl Read for PrinterStream {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.read(buf),
            Self::Serial(s) => s.read(buf),
        }
    }
}

impl Write for PrinterStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Tcp(s) => s.write(buf),
            Self::Serial(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Tcp(s) => s.flush(),
            Self::Serial(s) => s.flush(),
        }
    }
}

impl PrinterStream {
    fn set_read_timeout(&mut self, timeout: Option<Duration>) -> Result<(), String> {
        match self {
            Self::Tcp(s) => s.set_read_timeout(timeout).map_err(|e| e.to_string()),
            Self::Serial(s) => s
                .set_timeout(timeout.unwrap_or(Duration::from_millis(1)))
                .map_err(|e| e.to_string()),
        }
    }

    fn set_write_timeout(&mut self, timeout: Option<Duration>) -> Result<(), String> {
        match self {
            Self::Tcp(s) => s.set_write_timeout(timeout).map_err(|e| e.to_string()),
            Self::Serial(s) => s
                .set_timeout(timeout.unwrap_or(IO_TIMEOUT))
                .map_err(|e| e.to_string()),
        }
    }
}

struct DeviceSession {
    stream: PrinterStream,
    last_used: Instant,
    /// ETX-terminated frames the last write is expected to answer with (0 = unknown / idle read).
    expected_etx: usize,
    /// True when the last write was a query that expects a reply.
    awaiting_reply: bool,
}

/// Classifies a write as a Zebra query and returns the number of ETX-terminated frames
/// in the reply (`~HS` answers with 3 STX…ETX lines). `None` means plain print data.
fn query_reply_frames(data: &[u8]) -> Option<usize> {
    if data.len() > 1024 {
        return None;
    }
    let text = String::from_utf8_lossy(data).to_ascii_lowercase();
    let t = text.trim();
    if t.starts_with("~hs") {
        Some(3)
    } else if t.starts_with("~hi") || t.contains("^hh") {
        Some(1)
    } else if t.contains("getvar") || t.starts_with("~hq") {
        Some(0)
    } else {
        None
    }
}

/// Open I/O sessions keyed by Browser Print device uid (write then read).
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

    fn insert_session(
        map: &mut HashMap<String, DeviceSession>,
        uid: &str,
        stream: PrinterStream,
        query: Option<usize>,
    ) {
        if map.len() >= MAX_DEVICE_SESSIONS {
            if let Some(oldest) = map
                .iter()
                .min_by_key(|(_, session)| session.last_used)
                .map(|(key, _)| key.clone())
            {
                map.remove(&oldest);
            }
        }
        map.insert(
            uid.to_string(),
            DeviceSession {
                stream,
                last_used: Instant::now(),
                expected_etx: query.unwrap_or(0),
                awaiting_reply: query.is_some(),
            },
        );
    }

    pub fn purge_idle_sessions(&self) {
        if let Ok(mut map) = self.inner.lock() {
            Self::purge_idle(&mut map);
        }
    }

    pub fn write(&self, uid: &str, printer: &PrinterInfo, data: &[u8]) -> Result<(), String> {
        let mut map = self.inner.lock().map_err(|e| e.to_string())?;
        Self::purge_idle(&mut map);

        let query = query_reply_frames(data);
        if let Some(session) = map.get_mut(uid) {
            if query.is_some() {
                drain_pending(&mut session.stream);
            }
            match session
                .stream
                .write_all(data)
                .and_then(|_| session.stream.flush())
            {
                Ok(()) => {
                    session.last_used = Instant::now();
                    session.expected_etx = query.unwrap_or(0);
                    session.awaiting_reply = query.is_some();
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
        Self::insert_session(&mut map, uid, stream, query);
        Ok(())
    }

    pub fn read(&self, uid: &str, printer: &PrinterInfo) -> Result<String, String> {
        // Take the session out so a slow reply does not block other printers.
        let taken = {
            let mut map = self.inner.lock().map_err(|e| e.to_string())?;
            Self::purge_idle(&mut map);
            map.remove(uid)
        };
        let mut session = match taken {
            Some(s) => s,
            None => DeviceSession {
                // Open a fresh connection so a lone /read still returns something (often empty).
                stream: connect_printer(printer)?,
                last_used: Instant::now(),
                expected_etx: 0,
                awaiting_reply: false,
            },
        };

        let buf = if session.awaiting_reply {
            read_query_reply(&mut session.stream, session.expected_etx)
        } else {
            read_idle(&mut session.stream)
        };
        session.awaiting_reply = false;
        session.expected_etx = 0;
        session.last_used = Instant::now();

        let mut map = self.inner.lock().map_err(|e| e.to_string())?;
        Self::insert_session(&mut map, uid, session.stream, None);
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }
}

/// Discards stale bytes so a query reply is not mixed with earlier output.
fn drain_pending(stream: &mut PrinterStream) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(10)));
    let mut chunk = [0u8; 1024];
    while let Ok(n) = stream.read(&mut chunk) {
        if n == 0 {
            break;
        }
    }
}

fn is_timeout(e: &std::io::Error) -> bool {
    e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut
}

/// Reads whatever is available until the printer is quiet for `READ_TIMEOUT`.
fn read_idle(stream: &mut PrinterStream) -> Vec<u8> {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > 65536 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    buf
}

/// Reads a query reply: waits up to `QUERY_READ_DEADLINE` for the first byte, then until
/// `expected_etx` ETX frames arrived (or, when 0, until the printer goes quiet).
fn read_query_reply(stream: &mut PrinterStream, expected_etx: usize) -> Vec<u8> {
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    let deadline = Instant::now() + QUERY_READ_DEADLINE;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    while Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > 65536 {
                    break;
                }
                if expected_etx > 0 && buf.iter().filter(|b| **b == ETX).count() >= expected_etx {
                    break;
                }
            }
            Err(e) if is_timeout(&e) => {
                if !buf.is_empty() && expected_etx == 0 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    buf
}

fn open_serial_path(path: &str) -> Result<Box<dyn serialport::SerialPort>, String> {
    serialport::new(path, USB_BAUD)
        .data_bits(serialport::DataBits::Eight)
        .parity(serialport::Parity::None)
        .stop_bits(serialport::StopBits::One)
        .timeout(IO_TIMEOUT)
        .open()
        .map_err(|e| format_usb_open_error(path, &e))
}

fn format_usb_open_error(path: &str, err: &serialport::Error) -> String {
    let base = format!("Could not open USB printer {path}: {err}");
    #[cfg(target_os = "linux")]
    {
        if err.kind() == serialport::ErrorKind::Io(std::io::ErrorKind::PermissionDenied) {
            return format!(
                "{base}. On Linux, install/reload the RawLabelPrint udev rule (Zebra VID 0a5f) or add your user to the dialout/lp group, then replug the printer"
            );
        }
    }
    base
}

fn connect_usb(printer: &PrinterInfo) -> Result<PrinterStream, String> {
    match open_serial_path(&printer.address) {
        Ok(port) => Ok(PrinterStream::Serial(port)),
        Err(first_err) => {
            if let Some(path) = find_port_path_by_serial(&printer.serial_number) {
                if path != printer.address {
                    return open_serial_path(&path).map(PrinterStream::Serial);
                }
            }
            Err(first_err)
        }
    }
}

fn connect_tcp(printer: &PrinterInfo) -> Result<PrinterStream, String> {
    let addr: SocketAddr = format!("{}:{}", printer.address, printer.print_port)
        .parse()
        .map_err(|e| format!("Invalid printer address: {e}"))?;
    let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)
        .map_err(|e| format!("Could not connect to printer: {e}"))?;
    let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
    let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
    Ok(PrinterStream::Tcp(stream))
}

fn connect_printer(printer: &PrinterInfo) -> Result<PrinterStream, String> {
    if printer.is_usb() {
        connect_usb(printer)
    } else {
        connect_tcp(printer)
    }
}

pub fn send_raw_to_printer(printer: &PrinterInfo, data: &str) -> Result<(), String> {
    send_raw_bytes_to_printer(printer, data.as_bytes())
}

pub fn send_raw_bytes_to_printer(printer: &PrinterInfo, data: &[u8]) -> Result<(), String> {
    let mut stream = connect_printer(printer)?;
    stream.set_write_timeout(Some(IO_TIMEOUT))?;
    stream
        .write_all(data)
        .map_err(|e| format!("Failed writing to printer: {e}"))?;
    let _ = stream.flush();
    Ok(())
}

/// Probe printer online status via TCP print port + ~HQES, fallback to HTTP config port.
fn raw_port_hint(print_port: u16) -> String {
    #[cfg(target_os = "macos")]
    {
        format!(
            "check Local Network permission for RawLabelPrint, or that RAW port {print_port} is open"
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        format!("check firewall/routing, or that RAW port {print_port} is open")
    }
}

fn network_access_hint() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Enable Local Network for RawLabelPrint in System Settings"
    }
    #[cfg(not(target_os = "macos"))]
    {
        "Check that the printer is on the same LAN and not blocked by a firewall"
    }
}

fn read_hqes_response(stream: &mut PrinterStream) -> Vec<u8> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if buf.len() > 8192 || buf.windows(2).any(|w| w == b"\n\n" || w == b"\r\n") {
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
    buf
}

fn status_from_hqes(printer: &PrinterInfo, buf: &[u8]) -> PrinterStatus {
    let mut status = PrinterStatus {
        print_port: printer.print_port,
        config_port: printer.config_port,
        status: "unknown".into(),
        error_messages: Vec::new(),
        warning_messages: Vec::new(),
        detail: None,
    };
    let response = String::from_utf8_lossy(buf);
    let (errors, warnings) = decode_hqes(&response);
    status.error_messages = errors;
    status.warning_messages = warnings;
    status.status = if status.error_messages.is_empty() {
        "online".into()
    } else {
        "offline".into()
    };
    if buf.is_empty() {
        status.detail = Some(format!(
            "Connected to {} (no ~HQES response)",
            printer.address
        ));
    }
    status
}

fn get_usb_printer_status(printer: &PrinterInfo) -> PrinterStatus {
    match connect_usb(printer) {
        Ok(mut stream) => {
            let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
            let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
            if let Err(e) = stream.write_all(b"~HQES\r\n") {
                return PrinterStatus {
                    print_port: printer.print_port,
                    config_port: printer.config_port,
                    status: "offline".into(),
                    error_messages: vec![format!("USB write failed: {e}")],
                    warning_messages: Vec::new(),
                    detail: Some(format!("Connected to {} but write failed", printer.address)),
                };
            }
            let _ = stream.flush();
            let buf = read_hqes_response(&mut stream);
            status_from_hqes(printer, &buf)
        }
        Err(e) => PrinterStatus {
            print_port: printer.print_port,
            config_port: printer.config_port,
            status: "offline".into(),
            error_messages: vec![e.clone()],
            warning_messages: Vec::new(),
            detail: Some(e),
        },
    }
}

pub fn get_printer_status(printer: &PrinterInfo) -> PrinterStatus {
    if printer.is_usb() {
        return get_usb_printer_status(printer);
    }

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
        Ok(tcp) => {
            let mut stream = PrinterStream::Tcp(tcp);
            let _ = stream.set_read_timeout(Some(READ_TIMEOUT));
            let _ = stream.set_write_timeout(Some(IO_TIMEOUT));
            if let Err(e) = stream.write_all(b"~HQES\r\n") {
                status.status = "offline".into();
                status.detail = Some(format!("Connected but write failed: {e}"));
                return status;
            }
            let _ = stream.flush();
            let buf = read_hqes_response(&mut stream);
            let mut probed = status_from_hqes(printer, &buf);
            probed.config_port = config_port;
            if buf.is_empty() {
                probed.detail = Some(format!(
                    "Connected to {}:{} (no ~HQES response)",
                    printer.address, printer.print_port
                ));
            }
            probed
        }
        Err(e) => {
            let print_err = format!("TCP {}:{} — {e}", printer.address, printer.print_port);
            // Web UI is usually on config_port (80). Reachable HTTP helps diagnose permission vs RAW port.
            let config_addr: Result<SocketAddr, _> =
                format!("{}:{}", printer.address, config_port).parse();
            if let Ok(config_addr) = config_addr {
                match TcpStream::connect_timeout(&config_addr, CONNECT_TIMEOUT) {
                    Ok(_) => {
                        status.status = "offline".into();
                        status.detail = Some(format!(
                            "{print_err}. HTTP :{config_port} is reachable — {hint}",
                            hint = raw_port_hint(printer.print_port)
                        ));
                        status.error_messages.push(print_err);
                        return status;
                    }
                    Err(e2) => {
                        status.status = "offline".into();
                        status.detail = Some(format!(
                            "{print_err}. HTTP :{config_port} also failed: {e2}. {hint}",
                            hint = network_access_hint()
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
    use crate::config::CONNECTION_NETWORK;
    use std::io::Read;
    use std::sync::{Arc, Mutex};
    use std::thread;
    use std::time::{Duration, Instant};

    #[test]
    fn decode_hqes_nonzero_error_flag() {
        let (errors, warnings) =
            decode_hqes("ERRORS:         00000001\nWARNINGS:       00000000\n");
        assert_eq!(errors.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn decode_hqes_clean() {
        let (errors, warnings) =
            decode_hqes("ERRORS:         00000000\nWARNINGS:       00000000\n");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn decode_hqes_mixed_error_and_warning() {
        let (errors, warnings) =
            decode_hqes("ERRORS: 00000002\nWARNINGS: 00000004\nOTHER: ignore\n");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("0x00000002"));
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("0x00000004"));
    }

    #[test]
    fn decode_hqes_malformed_lines_are_ignored() {
        let (errors, warnings) = decode_hqes("ERRORS:\nWARNINGS: not-hex\n");
        assert!(errors.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn extract_hex_flags_supports_0x_prefix() {
        assert_eq!(extract_hex_flags("ERRORS: 0x0000000A"), Some(0xA));
        assert_eq!(extract_hex_flags("no flags here"), None);
    }

    #[test]
    fn session_pool_write_reaches_mock_tcp() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
            let mut buf = Vec::new();
            let _ = stream.read_to_end(&mut buf);
            *received_clone.lock().unwrap() = buf;
        });

        let printer = PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: "MOCK1".into(),
            address: "127.0.0.1".into(),
            port: 0,
            print_port: port,
            config_port: 80,
            connection: CONNECTION_NETWORK.into(),
        };
        let zpl = b"^XA^FDHi^FS^XZ";
        {
            let pool = DeviceSessionPool::new();
            pool.write("MOCK1", &printer, zpl).unwrap();
        } // drop closes TCP so mock read_to_end completes

        handle.join().unwrap();
        assert_eq!(received.lock().unwrap().as_slice(), zpl);
    }

    #[test]
    fn session_pool_purges_idle_sessions() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut byte = [0u8; 1];
            stream.read_exact(&mut byte).unwrap();
        });
        let printer = PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: "MOCK1".into(),
            address: "127.0.0.1".into(),
            port: 0,
            print_port: port,
            config_port: 80,
            connection: CONNECTION_NETWORK.into(),
        };
        let pool = DeviceSessionPool::new();
        pool.write("MOCK1", &printer, b"x").unwrap();
        handle.join().unwrap();
        pool.inner
            .lock()
            .unwrap()
            .get_mut("MOCK1")
            .unwrap()
            .last_used = Instant::now() - SESSION_IDLE - Duration::from_secs(1);

        pool.purge_idle_sessions();

        assert!(pool.inner.lock().unwrap().is_empty());
    }

    #[test]
    fn session_pool_evicts_least_recently_used_at_capacity() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            for _ in 0..=MAX_DEVICE_SESSIONS {
                let (mut stream, _) = listener.accept().unwrap();
                let mut byte = [0u8; 1];
                stream.read_exact(&mut byte).unwrap();
            }
        });
        let printer = PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: String::new(),
            address: "127.0.0.1".into(),
            port: 0,
            print_port: port,
            config_port: 80,
            connection: CONNECTION_NETWORK.into(),
        };
        let pool = DeviceSessionPool::new();
        for index in 0..MAX_DEVICE_SESSIONS {
            pool.write(&format!("MOCK{index}"), &printer, b"x").unwrap();
        }
        pool.inner
            .lock()
            .unwrap()
            .get_mut("MOCK0")
            .unwrap()
            .last_used = Instant::now() - Duration::from_secs(5);
        pool.write("MOCK-new", &printer, b"x").unwrap();
        handle.join().unwrap();

        let sessions = pool.inner.lock().unwrap();
        assert_eq!(sessions.len(), MAX_DEVICE_SESSIONS);
        assert!(!sessions.contains_key("MOCK0"));
        assert!(sessions.contains_key("MOCK-new"));
    }

    #[test]
    fn query_reply_frames_classification() {
        assert_eq!(query_reply_frames(b"~hs\r\n"), Some(3));
        assert_eq!(query_reply_frames(b"~HI\r\n"), Some(1));
        assert_eq!(query_reply_frames(b"^XA^HH^XZ"), Some(1));
        assert_eq!(
            query_reply_frames(b"! U1 getvar \"device.host_status\"\r\n"),
            Some(0)
        );
        assert_eq!(query_reply_frames(b"^XA^FDHi^FS^XZ"), None);
    }

    fn mock_printer(port: u16) -> PrinterInfo {
        PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: "MOCK1".into(),
            address: "127.0.0.1".into(),
            port: 0,
            print_port: port,
            config_port: 80,
            connection: CONNECTION_NETWORK.into(),
        }
    }

    /// Mock printer that waits `delay_ms` after receiving a command, then sends `frames` with gaps.
    fn spawn_replying_printer(
        delay_ms: u64,
        frames: Vec<Vec<u8>>,
        gap_ms: u64,
    ) -> (u16, thread::JoinHandle<()>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0u8; 256];
            let _ = stream.read(&mut buf).unwrap();
            thread::sleep(Duration::from_millis(delay_ms));
            for frame in frames {
                stream.write_all(&frame).unwrap();
                thread::sleep(Duration::from_millis(gap_ms));
            }
            thread::sleep(Duration::from_millis(300));
        });
        (port, handle)
    }

    #[test]
    fn read_after_hs_waits_for_all_three_etx_frames() {
        let frames = vec![
            b"\x02030,0,0,0,000,0,0,0,000,0,0,0\x03\r\n".to_vec(),
            b"\x02000,0,0,0,0,2,4,0,00000000,1,000\x03\r\n".to_vec(),
            b"\x021234,0\x03\r\n".to_vec(),
        ];
        // First byte after 600 ms and 400 ms gaps exceed the old 250 ms idle read.
        let (port, handle) = spawn_replying_printer(600, frames, 400);
        let pool = DeviceSessionPool::new();
        let printer = mock_printer(port);
        pool.write("MOCK1", &printer, b"~hs\r\n").unwrap();
        let reply = pool.read("MOCK1", &printer).unwrap();
        assert_eq!(reply.matches('\u{3}').count(), 3, "reply: {reply:?}");
        handle.join().unwrap();
    }

    #[test]
    fn read_after_hi_returns_single_frame() {
        let (port, handle) = spawn_replying_printer(
            500,
            vec![b"\x02ZD421-203dpi,V84.20.18Z,8,8176KB\x03\r\n".to_vec()],
            0,
        );
        let pool = DeviceSessionPool::new();
        let printer = mock_printer(port);
        pool.write("MOCK1", &printer, b"~hi\r\n").unwrap();
        let reply = pool.read("MOCK1", &printer).unwrap();
        assert!(
            reply.contains("ZD421") && reply.ends_with("\u{3}\r\n"),
            "reply: {reply:?}"
        );
        handle.join().unwrap();
    }

    #[test]
    fn read_after_getvar_returns_value_without_etx() {
        let (port, handle) = spawn_replying_printer(400, vec![b"\"ready\"".to_vec()], 0);
        let pool = DeviceSessionPool::new();
        let printer = mock_printer(port);
        pool.write("MOCK1", &printer, b"! U1 getvar \"device.host_status\"\r\n")
            .unwrap();
        let reply = pool.read("MOCK1", &printer).unwrap();
        assert_eq!(reply, "\"ready\"");
        handle.join().unwrap();
    }

    #[test]
    fn read_after_plain_print_uses_idle_read() {
        let (port, handle) = spawn_replying_printer(0, vec![], 0);
        let pool = DeviceSessionPool::new();
        let printer = mock_printer(port);
        pool.write("MOCK1", &printer, b"^XA^FDHi^FS^XZ").unwrap();
        let started = Instant::now();
        let reply = pool.read("MOCK1", &printer).unwrap();
        assert!(reply.is_empty());
        assert!(started.elapsed() < Duration::from_secs(1));
        handle.join().unwrap();
    }
}
