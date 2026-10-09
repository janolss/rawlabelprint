//! Discover Zebra printers attached over USB CDC/ACM (serial).

use crate::config::{Connection, PrinterInfo};
use serialport::{SerialPortType, UsbPortInfo};
use std::sync::Mutex;

/// Zebra Technologies USB vendor ID.
pub const ZEBRA_USB_VID: u16 = 0x0A5F;

fn from_usb_port(port_name: String, info: UsbPortInfo) -> PrinterInfo {
    PrinterInfo {
        name: None,
        model: info
            .product
            .filter(|p| !p.trim().is_empty())
            .unwrap_or_else(|| "Zebra USB".into()),
        firmware: String::new(),
        serial_number: info.serial_number.unwrap_or_default(),
        address: port_name,
        port: 0,
        print_port: 0,
        config_port: 0,
        connection: Connection::Usb,
    }
}

/// Enumerate serial ports whose USB VID is Zebra (`0x0A5F`).
pub fn search_zebra_usb_printers() -> Result<Vec<PrinterInfo>, String> {
    let ports =
        serialport::available_ports().map_err(|e| format!("USB serial enumeration failed: {e}"))?;
    let mut out = Vec::new();
    for port in ports {
        if let SerialPortType::UsbPort(info) = port.port_type {
            if info.vid == ZEBRA_USB_VID {
                out.push(from_usb_port(port.port_name, info));
            }
        }
    }
    out.sort_by(|a, b| a.address.cmp(&b.address));
    Ok(out)
}

/// Resolve current serial path for a Zebra USB printer by USB serial number.
pub fn find_port_path_by_serial(serial: &str) -> Option<String> {
    let serial = serial.trim();
    if serial.is_empty() {
        return None;
    }
    let ports = serialport::available_ports().ok()?;
    for port in ports {
        if let SerialPortType::UsbPort(info) = port.port_type {
            if info.vid == ZEBRA_USB_VID
                && info
                    .serial_number
                    .as_deref()
                    .is_some_and(|s| s.eq_ignore_ascii_case(serial))
            {
                return Some(port.port_name);
            }
        }
    }
    None
}

/// Merge network + USB discovery results. Network first; skip USB entries whose uid
/// already appears (same physical printer discovered twice is unlikely but cheap to guard).
pub fn merge_discovered(network: Vec<PrinterInfo>, usb: Vec<PrinterInfo>) -> Vec<PrinterInfo> {
    let mut out = network;
    let existing: std::collections::HashSet<String> =
        out.iter().map(|p| p.browser_print_uid()).collect();
    for p in usb {
        if !existing.contains(&p.browser_print_uid()) {
            out.push(p);
        }
    }
    out
}

static DISCOVERY_LOCK: Mutex<()> = Mutex::new(());

/// Run UDP LAN discovery and USB enumeration; USB still returned if UDP fails.
pub fn search_all_printers() -> Result<Vec<PrinterInfo>, String> {
    let _guard = DISCOVERY_LOCK.lock().map_err(|e| e.to_string())?;
    let network = match crate::discovery::search_zebra_printers() {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!("LAN discovery failed (continuing with USB): {e}");
            Vec::new()
        }
    };
    let usb = search_zebra_usb_printers().unwrap_or_else(|e| {
        tracing::warn!("USB discovery failed: {e}");
        Vec::new()
    });
    Ok(merge_discovered(network, usb))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Connection;

    fn net_printer(addr: &str, serial: &str) -> PrinterInfo {
        PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: serial.into(),
            address: addr.into(),
            port: 0,
            print_port: 9100,
            config_port: 80,
            connection: Connection::Network,
        }
    }

    fn usb_printer(path: &str, serial: &str) -> PrinterInfo {
        PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: serial.into(),
            address: path.into(),
            port: 0,
            print_port: 0,
            config_port: 0,
            connection: Connection::Usb,
        }
    }

    #[test]
    fn merge_keeps_network_then_usb() {
        let merged = merge_discovered(
            vec![net_printer("10.0.0.1", "N1")],
            vec![usb_printer("/dev/ttyACM0", "U1")],
        );
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].address, "10.0.0.1");
        assert_eq!(merged[1].address, "/dev/ttyACM0");
        assert!(merged[1].is_usb());
    }

    #[test]
    fn merge_dedupes_same_uid() {
        let merged = merge_discovered(
            vec![net_printer("10.0.0.1", "SAME")],
            vec![usb_printer("/dev/ttyACM0", "SAME")],
        );
        assert_eq!(merged.len(), 1);
        assert!(!merged[0].is_usb());
    }

    #[test]
    fn discovery_lock_excludes_overlapping_callers() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;
        use std::thread;
        use std::time::Duration;

        let inside = Arc::new(AtomicUsize::new(0));
        let max_inside = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let inside = inside.clone();
            let max_inside = max_inside.clone();
            handles.push(thread::spawn(move || {
                let _guard = DISCOVERY_LOCK.lock().unwrap();
                let now = inside.fetch_add(1, Ordering::SeqCst) + 1;
                max_inside.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(40));
                inside.fetch_sub(1, Ordering::SeqCst);
            }));
        }
        for handle in handles {
            handle.join().unwrap();
        }
        assert_eq!(max_inside.load(Ordering::SeqCst), 1);
    }
}
