//! In-memory ring buffer of recent print requests for Settings debug UI.

use crate::config::Connection;
use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_CAPACITY: usize = 50;
const DEFAULT_MAX_PREVIEW_BYTES: usize = 20 * 1024;
/// Max bytes kept per entry for resend (aligned with HTTP body limit).
const DEFAULT_MAX_STORED_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintLogEntry {
    pub id: u64,
    pub timestamp_ms: u64,
    pub route: String,
    pub printer_name: String,
    pub printer_address: String,
    pub print_port: u16,
    #[serde(skip)]
    pub connection: Connection,
    pub data_preview: String,
    pub data_bytes: usize,
    pub truncated: bool,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Payload kept for resend (may be truncated to max_stored_bytes); omitted from UI JSON.
    #[serde(skip)]
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintLogSummary {
    pub id: u64,
    pub timestamp_ms: u64,
    pub route: String,
    pub printer_name: String,
    pub printer_address: String,
    pub print_port: u16,
    pub data_preview: String,
    pub data_bytes: usize,
    pub truncated: bool,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub struct PrintLog {
    inner: Mutex<PrintLogInner>,
}

struct PrintLogInner {
    entries: VecDeque<PrintLogEntry>,
    next_id: u64,
    capacity: usize,
    max_preview_bytes: usize,
    max_stored_bytes: usize,
}

impl PrintLog {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PrintLogInner {
                entries: VecDeque::with_capacity(DEFAULT_CAPACITY),
                next_id: 1,
                capacity: DEFAULT_CAPACITY,
                max_preview_bytes: DEFAULT_MAX_PREVIEW_BYTES,
                max_stored_bytes: DEFAULT_MAX_STORED_BYTES,
            }),
        }
    }

    pub fn record(
        &self,
        route: impl Into<String>,
        printer: &crate::config::PrinterInfo,
        data: &[u8],
        result: Result<(), String>,
    ) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        let (data_preview, preview_truncated) = preview_bytes(data, guard.max_preview_bytes);
        let (stored, stored_truncated) = truncate_stored(data, guard.max_stored_bytes);
        let (ok, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        let entry = PrintLogEntry {
            id: guard.next_id,
            timestamp_ms: now_ms(),
            route: route.into(),
            printer_name: printer.display_name(),
            printer_address: printer.address.clone(),
            print_port: if printer.connection == Connection::Network && printer.print_port == 0 {
                9100
            } else {
                printer.print_port
            },
            connection: printer.connection,
            data_preview,
            data_bytes: data.len(),
            truncated: preview_truncated || stored_truncated,
            ok,
            error,
            data: stored,
        };
        guard.next_id = guard.next_id.saturating_add(1);
        if guard.entries.len() >= guard.capacity {
            guard.entries.pop_front();
        }
        guard.entries.push_back(entry);
    }

    #[cfg(test)]
    pub fn list(&self) -> Vec<PrintLogEntry> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        // Newest first for the UI.
        guard.entries.iter().rev().cloned().collect()
    }

    pub fn list_summaries(&self) -> Vec<PrintLogSummary> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        guard
            .entries
            .iter()
            .rev()
            .map(|entry| PrintLogSummary {
                id: entry.id,
                timestamp_ms: entry.timestamp_ms,
                route: entry.route.clone(),
                printer_name: entry.printer_name.clone(),
                printer_address: entry.printer_address.clone(),
                print_port: entry.print_port,
                data_preview: entry.data_preview.clone(),
                data_bytes: entry.data_bytes,
                truncated: entry.truncated,
                ok: entry.ok,
                error: entry.error.clone(),
            })
            .collect()
    }

    pub fn get(&self, id: u64) -> Option<PrintLogEntry> {
        let Ok(guard) = self.inner.lock() else {
            return None;
        };
        guard.entries.iter().find(|e| e.id == id).cloned()
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.inner.lock() {
            guard.entries.clear();
        }
    }
}

impl Default for PrintLog {
    fn default() -> Self {
        Self::new()
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn preview_bytes(data: &[u8], max_bytes: usize) -> (String, bool) {
    let truncated = data.len() > max_bytes;
    let slice = if truncated { &data[..max_bytes] } else { data };
    let mut preview = String::from_utf8_lossy(slice).into_owned();
    if truncated {
        preview.push_str("\n… [truncated]");
    }
    (preview, truncated)
}

fn truncate_stored(data: &[u8], max_bytes: usize) -> (Vec<u8>, bool) {
    if data.len() > max_bytes {
        (data[..max_bytes].to_vec(), true)
    } else {
        (data.to_vec(), false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::PrinterInfo;

    fn printer(name: &str, address: &str) -> PrinterInfo {
        PrinterInfo {
            name: Some(name.into()),
            model: String::new(),
            firmware: String::new(),
            serial_number: String::new(),
            address: address.into(),
            port: 0,
            print_port: 9100,
            config_port: 80,
            connection: Connection::Network,
        }
    }

    #[test]
    fn ring_buffer_evicts_oldest() {
        let log = PrintLog::new();
        {
            let mut guard = log.inner.lock().unwrap();
            guard.capacity = 3;
            guard.max_preview_bytes = 16;
        }
        for i in 0..5 {
            log.record(
                "/",
                &printer(&format!("p{i}"), "10.0.0.1"),
                b"^XA^XZ",
                Ok(()),
            );
        }
        let entries = log.list();
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0].printer_name, "p4");
        assert_eq!(entries[2].printer_name, "p2");
    }

    #[test]
    fn truncates_large_payload() {
        let log = PrintLog::new();
        {
            let mut guard = log.inner.lock().unwrap();
            guard.max_preview_bytes = 8;
        }
        log.record("/", &printer("A", "1.1.1.1"), b"0123456789ABCDEF", Ok(()));
        let entries = log.list();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].truncated);
        assert_eq!(entries[0].data_bytes, 16);
        assert!(entries[0].data_preview.contains("[truncated]"));
        assert_eq!(entries[0].data, b"0123456789ABCDEF");
    }

    #[test]
    fn truncates_stored_payload_to_max() {
        let log = PrintLog::new();
        {
            let mut guard = log.inner.lock().unwrap();
            guard.max_stored_bytes = 4;
            guard.max_preview_bytes = 100;
        }
        log.record("/", &printer("A", "1.1.1.1"), b"0123456789", Ok(()));
        let e = &log.list()[0];
        assert_eq!(e.data_bytes, 10);
        assert_eq!(e.data, b"0123");
        assert!(e.truncated);
    }

    #[test]
    fn records_errors() {
        let log = PrintLog::new();
        log.record(
            "/write",
            &printer("ZD421", "192.168.1.50"),
            b"^XA^XZ",
            Err("Could not connect".into()),
        );
        let e = &log.list()[0];
        assert!(!e.ok);
        assert_eq!(e.error.as_deref(), Some("Could not connect"));
        assert_eq!(e.route, "/write");
    }

    #[test]
    fn list_summaries_omit_payload_but_get_keeps_it() {
        let log = PrintLog::new();
        let payload = b"^XA^FDHi^FS^XZ";
        log.record("/", &printer("A", "10.0.0.1"), payload, Ok(()));
        let id = log.list_summaries()[0].id;

        let summary = log.list_summaries().remove(0);
        let json = serde_json::to_value(summary).unwrap();
        assert!(json.get("data").is_none());
        assert_eq!(json["dataBytes"], payload.len());
        assert_eq!(log.get(id).unwrap().data, payload);
    }

    #[test]
    fn get_by_id_returns_full_payload() {
        let log = PrintLog::new();
        log.record("/", &printer("A", "10.0.0.1"), b"^XA^FDHi^FS^XZ", Ok(()));
        let id = log.list()[0].id;
        let entry = log.get(id).expect("entry");
        assert_eq!(entry.data, b"^XA^FDHi^FS^XZ");
        assert_eq!(entry.print_port, 9100);
    }

    #[test]
    fn default_limits() {
        assert_eq!(DEFAULT_MAX_PREVIEW_BYTES, 20 * 1024);
        assert_eq!(DEFAULT_MAX_STORED_BYTES, 1024 * 1024);
    }
}
