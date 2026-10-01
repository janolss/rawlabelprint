//! In-memory ring buffer of recent print requests for Settings debug UI.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const DEFAULT_CAPACITY: usize = 50;
const DEFAULT_MAX_PREVIEW_BYTES: usize = 20 * 1024;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintLogEntry {
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
    /// Full payload kept for resend; omitted from UI JSON.
    #[serde(skip)]
    pub data: Vec<u8>,
}

pub struct PrintLog {
    inner: Mutex<PrintLogInner>,
}

struct PrintLogInner {
    entries: VecDeque<PrintLogEntry>,
    next_id: u64,
    capacity: usize,
    max_preview_bytes: usize,
}

impl PrintLog {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(PrintLogInner {
                entries: VecDeque::with_capacity(DEFAULT_CAPACITY),
                next_id: 1,
                capacity: DEFAULT_CAPACITY,
                max_preview_bytes: DEFAULT_MAX_PREVIEW_BYTES,
            }),
        }
    }

    pub fn record(
        &self,
        route: impl Into<String>,
        printer_name: impl Into<String>,
        printer_address: impl Into<String>,
        print_port: u16,
        data: &[u8],
        result: Result<(), String>,
    ) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        let (data_preview, truncated) = preview_bytes(data, guard.max_preview_bytes);
        let (ok, error) = match result {
            Ok(()) => (true, None),
            Err(e) => (false, Some(e)),
        };
        let entry = PrintLogEntry {
            id: guard.next_id,
            timestamp_ms: now_ms(),
            route: route.into(),
            printer_name: printer_name.into(),
            printer_address: printer_address.into(),
            print_port: if print_port == 0 { 9100 } else { print_port },
            data_preview,
            data_bytes: data.len(),
            truncated,
            ok,
            error,
            data: data.to_vec(),
        };
        guard.next_id = guard.next_id.saturating_add(1);
        if guard.entries.len() >= guard.capacity {
            guard.entries.pop_front();
        }
        guard.entries.push_back(entry);
    }

    pub fn list(&self) -> Vec<PrintLogEntry> {
        let Ok(guard) = self.inner.lock() else {
            return Vec::new();
        };
        // Newest first for the UI.
        guard.entries.iter().rev().cloned().collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_evicts_oldest() {
        let log = PrintLog::new();
        {
            let mut guard = log.inner.lock().unwrap();
            guard.capacity = 3;
            guard.max_preview_bytes = 16;
        }
        for i in 0..5 {
            log.record("/", format!("p{i}"), "10.0.0.1", 9100, b"^XA^XZ", Ok(()));
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
        log.record("/", "A", "1.1.1.1", 9100, b"0123456789ABCDEF", Ok(()));
        let entries = log.list();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].truncated);
        assert_eq!(entries[0].data_bytes, 16);
        assert!(entries[0].data_preview.contains("[truncated]"));
        assert_eq!(entries[0].data, b"0123456789ABCDEF");
    }

    #[test]
    fn records_errors() {
        let log = PrintLog::new();
        log.record(
            "/write",
            "ZD421",
            "192.168.1.50",
            9100,
            b"^XA^XZ",
            Err("Could not connect".into()),
        );
        let e = &log.list()[0];
        assert!(!e.ok);
        assert_eq!(e.error.as_deref(), Some("Could not connect"));
        assert_eq!(e.route, "/write");
    }

    #[test]
    fn get_by_id_returns_full_payload() {
        let log = PrintLog::new();
        log.record("/", "A", "10.0.0.1", 9100, b"^XA^FDHi^FS^XZ", Ok(()));
        let id = log.list()[0].id;
        let entry = log.get(id).expect("entry");
        assert_eq!(entry.data, b"^XA^FDHi^FS^XZ");
        assert_eq!(entry.print_port, 9100);
    }

    #[test]
    fn default_preview_limit_is_20kb() {
        assert_eq!(DEFAULT_MAX_PREVIEW_BYTES, 20 * 1024);
    }
}
