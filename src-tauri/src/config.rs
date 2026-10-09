use serde::{Deserialize, Serialize};
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};

const MAX_DENIED_ORIGINS: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Connection {
    #[default]
    Network,
    Usb,
}

impl Connection {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Usb => "usb",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrinterInfo {
    /// Optional display override (manual adds / API Name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub model: String,
    #[serde(default)]
    pub firmware: String,
    #[serde(default)]
    pub serial_number: String,
    /// IP for network printers, or serial device path for USB (e.g. `/dev/ttyACM0`).
    pub address: String,
    /// UDP source port from discovery (informational).
    #[serde(default)]
    pub port: u16,
    #[serde(default = "default_print_port")]
    pub print_port: u16,
    #[serde(default = "default_config_port")]
    pub config_port: u16,
    /// `"network"` (TCP RAW) or `"usb"` (CDC/serial).
    #[serde(default)]
    pub connection: Connection,
}

fn default_print_port() -> u16 {
    9100
}

fn default_config_port() -> u16 {
    80
}

impl PrinterInfo {
    pub fn is_usb(&self) -> bool {
        self.connection == Connection::Usb
    }

    pub fn browser_print_connection(&self) -> &'static str {
        self.connection.as_str()
    }

    pub fn display_name(&self) -> String {
        if let Some(name) = &self.name {
            if !name.trim().is_empty() {
                return name.clone();
            }
        }
        if self.model.trim().is_empty() {
            self.address.clone()
        } else {
            format!("{} ({})", self.model, self.address)
        }
    }

    #[cfg(test)]
    pub fn matches_name(&self, query: &str) -> bool {
        let q = query.trim();
        if q.is_empty() {
            return false;
        }
        if self.address.eq_ignore_ascii_case(q) {
            return true;
        }
        if self.display_name().eq_ignore_ascii_case(q) {
            return true;
        }
        if let Some(name) = &self.name {
            if name.eq_ignore_ascii_case(q) {
                return true;
            }
        }
        if self.browser_print_uid().eq_ignore_ascii_case(q) {
            return true;
        }
        false
    }

    /// Stable uid used by Browser Print `/write` and `/read`.
    pub fn browser_print_uid(&self) -> String {
        if !self.serial_number.trim().is_empty() {
            return self.serial_number.clone();
        }
        if self.is_usb() {
            return format!("usb:{}", self.address);
        }
        format!("net:{}:{}", self.address, self.print_port)
    }

    pub fn to_browser_print_device(&self) -> serde_json::Value {
        serde_json::json!({
            "deviceType": "printer",
            "uid": self.browser_print_uid(),
            "name": self.display_name(),
            "connection": self.browser_print_connection(),
            "version": 2,
            "provider": "com.zebra.ds.webdriver.desktop.provider.DefaultDeviceProvider",
            "manufacturer": "Zebra Technologies"
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default = "default_listen_address")]
    pub listen_address: String,
    #[serde(default = "default_http_port")]
    pub port: u16,
    #[serde(default)]
    pub default_printer: Option<PrinterInfo>,
    #[serde(default)]
    pub added_printers: Vec<PrinterInfo>,
    #[serde(default)]
    pub launch_at_login: bool,
    /// When true, expose Zebra Browser Print HTTP routes (/available, /default, /write, /read, /config)
    /// so BrowserPrint.js clients work as a drop-in replacement.
    #[serde(default = "default_browser_print_compatible")]
    pub browser_print_compatible: bool,
    /// When true, capture recent print requests in an in-memory ring buffer for Settings.
    #[serde(default = "default_debug_logging")]
    pub debug_logging: bool,
    /// Browser Origins allowed to print via the local HTTP API (soft allowlist).
    #[serde(default)]
    pub allowed_origins: Vec<String>,
    /// Browser Origins that must not print until removed in Settings.
    #[serde(default)]
    pub denied_origins: Vec<String>,
}

fn default_listen_address() -> String {
    "127.0.0.1".into()
}

/// HTTP bind address is always loopback. Non-loopback values in config/UI are ignored.
pub fn sanitize_listen_address(_requested: &str) -> String {
    default_listen_address()
}

fn default_http_port() -> u16 {
    9100
}

fn default_browser_print_compatible() -> bool {
    true
}

fn default_debug_logging() -> bool {
    false
}

pub fn parse_ip_address(address: &str) -> Result<String, String> {
    let address = address.trim();
    if address.parse::<IpAddr>().is_err() {
        return Err("Address must be an IP address".into());
    }
    Ok(address.to_string())
}

pub fn require_listen_port(port: u16) -> Result<u16, String> {
    if port == 0 {
        Err("Port must be between 1 and 65535".into())
    } else {
        Ok(port)
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            port: default_http_port(),
            default_printer: None,
            added_printers: Vec::new(),
            launch_at_login: false,
            browser_print_compatible: default_browser_print_compatible(),
            debug_logging: default_debug_logging(),
            allowed_origins: Vec::new(),
            denied_origins: Vec::new(),
        }
    }
}

impl AppConfig {
    pub fn normalize_origin(raw: &str) -> String {
        raw.trim().to_ascii_lowercase()
    }

    pub fn is_origin_allowed(&self, origin: &str) -> bool {
        let key = Self::normalize_origin(origin);
        self.allowed_origins
            .iter()
            .any(|o| Self::normalize_origin(o) == key)
    }

    pub fn is_origin_denied(&self, origin: &str) -> bool {
        let key = Self::normalize_origin(origin);
        self.denied_origins
            .iter()
            .any(|o| Self::normalize_origin(o) == key)
    }

    /// Insert origin into the soft allowlist. Returns true if newly added.
    pub fn allow_origin(&mut self, origin: &str) -> bool {
        let key = Self::normalize_origin(origin);
        if key.is_empty() {
            return false;
        }
        self.denied_origins
            .retain(|o| Self::normalize_origin(o) != key);
        if self.is_origin_allowed(&key) {
            return false;
        }
        self.allowed_origins.push(key);
        true
    }

    /// Record a denial. Drops the oldest entry past the cap. Returns true if newly stored.
    pub fn deny_origin(&mut self, origin: &str) -> bool {
        let key = Self::normalize_origin(origin);
        if key.is_empty() {
            return false;
        }
        self.allowed_origins
            .retain(|o| Self::normalize_origin(o) != key);
        if self.is_origin_denied(&key) {
            return false;
        }
        if self.denied_origins.len() >= MAX_DENIED_ORIGINS {
            self.denied_origins.remove(0);
        }
        self.denied_origins.push(key);
        true
    }

    pub fn remove_denied_origin(&mut self, origin: &str) -> bool {
        let key = Self::normalize_origin(origin);
        let before = self.denied_origins.len();
        self.denied_origins
            .retain(|o| Self::normalize_origin(o) != key);
        self.denied_origins.len() != before
    }

    /// Remove origin from the soft allowlist. Returns true if it was present.
    pub fn revoke_origin(&mut self, origin: &str) -> bool {
        let key = Self::normalize_origin(origin);
        let before = self.allowed_origins.len();
        self.allowed_origins
            .retain(|o| Self::normalize_origin(o) != key);
        self.allowed_origins.len() != before
    }

    /// Ensure default printer is always present in the saved list.
    pub fn normalize_saved_printers(&mut self) {
        if let Some(default) = self.default_printer.clone() {
            self.upsert_printer(default, true);
        }
    }

    /// Insert or replace by address. Optionally mark as default.
    pub fn upsert_printer(&mut self, printer: PrinterInfo, make_default: bool) {
        self.added_printers.retain(|p| p.address != printer.address);
        let refresh_default = make_default
            || self
                .default_printer
                .as_ref()
                .map(|d| d.address == printer.address)
                .unwrap_or(false);
        if refresh_default {
            self.default_printer = Some(printer.clone());
        }
        self.added_printers.push(printer);
    }

    pub fn remove_printer(&mut self, address: &str) {
        self.added_printers.retain(|p| p.address != address);
        if self
            .default_printer
            .as_ref()
            .map(|p| p.address == address)
            .unwrap_or(false)
        {
            self.default_printer = self.added_printers.first().cloned();
        }
    }

    /// Update a saved printer identified by its previous address.
    pub fn update_printer(
        &mut self,
        original_address: &str,
        name: Option<String>,
        address: String,
        print_port: u16,
    ) -> Result<(), String> {
        let address = address.trim().to_string();
        if address.is_empty() {
            return Err("Address is required".into());
        }
        let address = if self
            .added_printers
            .iter()
            .find(|p| p.address == original_address)
            .is_some_and(|p| p.is_usb())
        {
            address
        } else {
            parse_ip_address(&address)?
        };
        if address != original_address && self.added_printers.iter().any(|p| p.address == address) {
            return Err(format!("A printer with address {address} already exists"));
        }

        let idx = self
            .added_printers
            .iter()
            .position(|p| p.address == original_address)
            .ok_or_else(|| "Printer not found".to_string())?;

        let mut printer = self.added_printers[idx].clone();
        printer.name = name.filter(|n| !n.trim().is_empty());
        printer.address = address.clone();
        if printer.is_usb() {
            // USB uses CDC path in `address`; print_port stays unused (0).
            printer.print_port = 0;
            printer.config_port = 0;
        } else {
            printer.print_port = if print_port == 0 { 9100 } else { print_port };
        }

        let was_default = self
            .default_printer
            .as_ref()
            .map(|d| d.address == original_address)
            .unwrap_or(false);

        self.added_printers.remove(idx);
        self.upsert_printer(printer, was_default);
        Ok(())
    }
}

pub fn config_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("config.json")
}

pub fn load_config(app_data_dir: &Path) -> AppConfig {
    let path = config_path(app_data_dir);
    let mut config = match fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str(&contents) {
            Ok(config) => config,
            Err(err) => {
                tracing::warn!("Ignoring unreadable config at {}: {err}", path.display());
                AppConfig::default()
            }
        },
        Err(_) => AppConfig::default(),
    };
    config.listen_address = sanitize_listen_address(&config.listen_address);
    config.normalize_saved_printers();
    config
}

pub fn save_config(app_data_dir: &Path, config: &AppConfig) -> Result<(), String> {
    fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = config_path(app_data_dir);
    let mut to_save = config.clone();
    to_save.listen_address = sanitize_listen_address(&to_save.listen_address);
    let json = serde_json::to_string_pretty(&to_save).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_printer(address: &str, serial: &str) -> PrinterInfo {
        PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: serial.into(),
            address: address.into(),
            port: 0,
            print_port: 9100,
            config_port: 80,
            connection: Connection::Network,
        }
    }

    #[test]
    fn allow_and_revoke_origin() {
        let mut cfg = AppConfig::default();
        assert!(cfg.allow_origin("https://Hotel.Example.com"));
        assert!(!cfg.allow_origin("https://hotel.example.com"));
        assert!(cfg.is_origin_allowed("HTTPS://HOTEL.EXAMPLE.COM"));
        assert!(cfg.revoke_origin("https://hotel.example.com"));
        assert!(!cfg.is_origin_allowed("https://hotel.example.com"));
        assert!(!cfg.revoke_origin("https://hotel.example.com"));
        assert!(!cfg.allow_origin("  "));
    }

    #[test]
    fn display_name_prefers_explicit_name() {
        let p = PrinterInfo {
            name: Some("Shipping".into()),
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: String::new(),
            address: "10.0.0.5".into(),
            port: 0,
            print_port: 9100,
            config_port: 80,
            connection: Connection::Network,
        };
        assert_eq!(p.display_name(), "Shipping");
        assert!(p.matches_name("shipping"));
        assert!(p.matches_name("10.0.0.5"));
        assert!(!p.matches_name("ZD421"));
    }

    #[test]
    fn display_name_falls_back_to_model_and_address() {
        let with_model = sample_printer("10.0.0.5", "");
        assert_eq!(with_model.display_name(), "ZD421 (10.0.0.5)");

        let address_only = PrinterInfo {
            model: String::new(),
            ..sample_printer("10.0.0.5", "")
        };
        assert_eq!(address_only.display_name(), "10.0.0.5");
    }

    #[test]
    fn matches_name_covers_uid_and_rejects_empty() {
        let p = sample_printer("10.0.0.5", "ABC123");
        assert!(p.matches_name("ABC123"));
        assert!(p.matches_name("ZD421 (10.0.0.5)"));
        assert!(!p.matches_name(""));
        assert!(!p.matches_name("   "));
        assert!(!p.matches_name("unknown"));
    }

    #[test]
    fn browser_print_uid_prefers_serial() {
        let with_serial = sample_printer("10.0.0.5", "ABC123");
        assert_eq!(with_serial.browser_print_uid(), "ABC123");

        let without = sample_printer("10.0.0.5", "");
        assert_eq!(without.browser_print_uid(), "net:10.0.0.5:9100");

        let usb = PrinterInfo {
            connection: Connection::Usb,
            print_port: 0,
            config_port: 0,
            address: "/dev/ttyACM0".into(),
            ..sample_printer("/dev/ttyACM0", "")
        };
        assert_eq!(usb.browser_print_uid(), "usb:/dev/ttyACM0");
        assert_eq!(usb.browser_print_connection(), "usb");
    }

    #[test]
    fn to_browser_print_device_shape() {
        let p = sample_printer("10.0.0.5", "SN1");
        let device = p.to_browser_print_device();
        assert_eq!(device["deviceType"], "printer");
        assert_eq!(device["uid"], "SN1");
        assert_eq!(device["name"], "ZD421 (10.0.0.5)");
        assert_eq!(device["connection"], "network");
        assert_eq!(device["version"], 2);
        assert_eq!(device["manufacturer"], "Zebra Technologies");
    }

    #[test]
    fn upsert_and_update_saved_printers() {
        let mut cfg = AppConfig::default();
        let a = PrinterInfo {
            name: Some("A".into()),
            ..sample_printer("10.0.0.1", "")
        };
        cfg.upsert_printer(a, true);
        assert_eq!(cfg.added_printers.len(), 1);
        assert_eq!(cfg.default_printer.as_ref().unwrap().address, "10.0.0.1");

        cfg.update_printer("10.0.0.1", Some("Renamed".into()), "10.0.0.2".into(), 9100)
            .unwrap();
        assert_eq!(cfg.added_printers.len(), 1);
        assert_eq!(cfg.added_printers[0].address, "10.0.0.2");
        assert_eq!(cfg.added_printers[0].name.as_deref(), Some("Renamed"));
        assert_eq!(cfg.default_printer.as_ref().unwrap().address, "10.0.0.2");

        cfg.remove_printer("10.0.0.2");
        assert!(cfg.added_printers.is_empty());
        assert!(cfg.default_printer.is_none());
    }

    #[test]
    fn remove_printer_falls_back_to_first_remaining() {
        let mut cfg = AppConfig::default();
        cfg.upsert_printer(sample_printer("10.0.0.1", ""), true);
        cfg.upsert_printer(sample_printer("10.0.0.2", ""), false);
        cfg.remove_printer("10.0.0.1");
        assert_eq!(cfg.added_printers.len(), 1);
        assert_eq!(cfg.default_printer.as_ref().unwrap().address, "10.0.0.2");
    }

    #[test]
    fn update_printer_rejects_unknown_and_duplicate() {
        let mut cfg = AppConfig::default();
        cfg.upsert_printer(sample_printer("10.0.0.1", ""), true);
        cfg.upsert_printer(sample_printer("10.0.0.2", ""), false);

        assert!(cfg
            .update_printer("9.9.9.9", None, "10.0.0.3".into(), 9100)
            .is_err());
        assert!(cfg
            .update_printer("10.0.0.1", None, "10.0.0.2".into(), 9100)
            .is_err());
        assert!(cfg
            .update_printer("10.0.0.1", None, "".into(), 9100)
            .is_err());
    }

    #[test]
    fn update_printer_zero_port_defaults_to_9100() {
        let mut cfg = AppConfig::default();
        cfg.upsert_printer(sample_printer("10.0.0.1", ""), true);
        cfg.update_printer("10.0.0.1", None, "10.0.0.1".into(), 0)
            .unwrap();
        assert_eq!(cfg.added_printers[0].print_port, 9100);
    }

    #[test]
    fn normalize_saved_printers_ensures_default_in_list() {
        let mut cfg = AppConfig {
            default_printer: Some(sample_printer("10.0.0.9", "SN9")),
            ..Default::default()
        };
        cfg.normalize_saved_printers();
        assert_eq!(cfg.added_printers.len(), 1);
        assert_eq!(cfg.added_printers[0].address, "10.0.0.9");
    }

    #[test]
    fn config_serde_roundtrip() {
        let mut cfg = AppConfig {
            browser_print_compatible: false,
            launch_at_login: true,
            ..Default::default()
        };
        cfg.upsert_printer(sample_printer("10.0.0.1", "S1"), true);
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert!(!back.browser_print_compatible);
        assert!(back.launch_at_login);
        assert_eq!(back.added_printers.len(), 1);
        assert_eq!(back.default_printer.as_ref().unwrap().serial_number, "S1");
    }

    #[test]
    fn load_save_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let mut cfg = AppConfig {
            browser_print_compatible: false,
            ..Default::default()
        };
        cfg.upsert_printer(sample_printer("10.0.0.1", "S1"), true);
        save_config(&path, &cfg).unwrap();

        let loaded = load_config(&path);
        assert!(!loaded.browser_print_compatible);
        assert_eq!(loaded.added_printers.len(), 1);
        assert_eq!(loaded.default_printer.as_ref().unwrap().address, "10.0.0.1");
    }

    #[test]
    fn load_config_missing_file_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load_config(dir.path());
        assert_eq!(loaded.port, 9100);
        assert!(loaded.browser_print_compatible);
        assert!(loaded.added_printers.is_empty());
        assert!(loaded.default_printer.is_none());
    }

    #[test]
    fn load_config_corrupt_json_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        fs::write(config_path(&path), "{not json").unwrap();
        let loaded = load_config(&path);
        assert_eq!(loaded.port, 9100);
        assert!(loaded.browser_print_compatible);
    }

    #[test]
    fn load_config_normalizes_orphan_default() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let json = r#"{
            "listenAddress": "127.0.0.1",
            "port": 9100,
            "defaultPrinter": {
                "model": "ZD421",
                "address": "10.0.0.7",
                "printPort": 9100,
                "configPort": 80
            },
            "addedPrinters": [],
            "browserPrintCompatible": true
        }"#;
        fs::write(config_path(&path), json).unwrap();
        let loaded = load_config(&path);
        assert_eq!(loaded.added_printers.len(), 1);
        assert_eq!(loaded.added_printers[0].address, "10.0.0.7");
    }

    #[test]
    fn sanitize_listen_address_always_loopback() {
        assert_eq!(sanitize_listen_address("0.0.0.0"), "127.0.0.1");
        assert_eq!(sanitize_listen_address("192.168.1.1"), "127.0.0.1");
        assert_eq!(sanitize_listen_address("127.0.0.1"), "127.0.0.1");
        assert_eq!(sanitize_listen_address(""), "127.0.0.1");
    }

    #[test]
    fn load_config_forces_loopback_even_if_file_says_otherwise() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let json = r#"{
            "listenAddress": "0.0.0.0",
            "port": 9100,
            "addedPrinters": [],
            "browserPrintCompatible": true
        }"#;
        fs::write(config_path(&path), json).unwrap();
        let loaded = load_config(&path);
        assert_eq!(loaded.listen_address, "127.0.0.1");
    }

    #[test]
    fn save_config_writes_loopback_only() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let cfg = AppConfig {
            listen_address: "0.0.0.0".into(),
            ..Default::default()
        };
        save_config(&path, &cfg).unwrap();
        let raw = fs::read_to_string(config_path(&path)).unwrap();
        assert!(raw.contains("127.0.0.1"));
        assert!(!raw.contains("0.0.0.0"));
    }

    #[test]
    fn deny_origin_removes_allow_and_is_cleared_on_allow() {
        let mut cfg = AppConfig::default();
        assert!(cfg.allow_origin("https://app.example"));
        assert!(cfg.deny_origin("https://app.example"));
        assert!(cfg.is_origin_denied("https://app.example"));
        assert!(!cfg.is_origin_allowed("https://app.example"));
        assert!(cfg.allow_origin("https://app.example"));
        assert!(!cfg.is_origin_denied("https://app.example"));
        assert!(cfg.is_origin_allowed("https://app.example"));
    }

    #[test]
    fn parse_ip_address_rejects_hostnames() {
        assert!(parse_ip_address("192.168.1.50").is_ok());
        assert!(parse_ip_address("printer.local").is_err());
        assert!(require_listen_port(0).is_err());
        assert_eq!(require_listen_port(9100).unwrap(), 9100);
    }

    #[test]
    fn load_config_unknown_connection_uses_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        let json = r#"{"addedPrinters":[{"model":"ZD421","address":"10.0.0.1","connection":"bluetooth"}]}"#;
        fs::write(config_path(&path), json).unwrap();
        let loaded = load_config(&path);
        assert!(loaded.added_printers.is_empty());
        assert!(!loaded.debug_logging);
    }
}
