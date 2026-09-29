use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    pub address: String,
    /// UDP source port from discovery (informational).
    #[serde(default)]
    pub port: u16,
    #[serde(default = "default_print_port")]
    pub print_port: u16,
    #[serde(default = "default_config_port")]
    pub config_port: u16,
}

fn default_print_port() -> u16 {
    9100
}

fn default_config_port() -> u16 {
    80
}

impl PrinterInfo {
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

    pub fn matches_name(&self, query: &str) -> bool {
        let q = query.trim();
        if q.is_empty() {
            return false;
        }
        if self.address.eq_ignore_ascii_case(q) {
            return true;
        }
        if self.model.eq_ignore_ascii_case(q) {
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
        format!("net:{}:{}", self.address, self.print_port)
    }

    pub fn to_browser_print_device(&self) -> serde_json::Value {
        serde_json::json!({
            "deviceType": "printer",
            "uid": self.browser_print_uid(),
            "name": self.display_name(),
            "connection": "network",
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
}

fn default_listen_address() -> String {
    "127.0.0.1".into()
}

fn default_http_port() -> u16 {
    9100
}

fn default_browser_print_compatible() -> bool {
    true
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
        }
    }
}

impl AppConfig {
    /// Ensure default printer is always present in the saved list.
    pub fn normalize_saved_printers(&mut self) {
        if let Some(default) = self.default_printer.clone() {
            self.upsert_printer(default, true);
        }
    }

    /// Insert or replace by address. Optionally mark as default.
    pub fn upsert_printer(&mut self, printer: PrinterInfo, make_default: bool) {
        self.added_printers
            .retain(|p| p.address != printer.address);
        if make_default {
            self.default_printer = Some(printer.clone());
        } else if self
            .default_printer
            .as_ref()
            .map(|d| d.address == printer.address)
            .unwrap_or(false)
        {
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
        if address != original_address
            && self.added_printers.iter().any(|p| p.address == address)
        {
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
        printer.print_port = if print_port == 0 { 9100 } else { print_port };

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

pub fn config_path(app_data_dir: &PathBuf) -> PathBuf {
    app_data_dir.join("config.json")
}

pub fn load_config(app_data_dir: &PathBuf) -> AppConfig {
    let path = config_path(app_data_dir);
    let mut config = match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    };
    config.normalize_saved_printers();
    config
}

pub fn save_config(app_data_dir: &PathBuf, config: &AppConfig) -> Result<(), String> {
    fs::create_dir_all(app_data_dir).map_err(|e| e.to_string())?;
    let path = config_path(app_data_dir);
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

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
        };
        assert_eq!(p.display_name(), "Shipping");
        assert!(p.matches_name("shipping"));
        assert!(p.matches_name("10.0.0.5"));
        assert!(p.matches_name("ZD421"));
    }

    #[test]
    fn browser_print_uid_prefers_serial() {
        let with_serial = PrinterInfo {
            name: None,
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: "ABC123".into(),
            address: "10.0.0.5".into(),
            port: 0,
            print_port: 9100,
            config_port: 80,
        };
        assert_eq!(with_serial.browser_print_uid(), "ABC123");

        let without = PrinterInfo {
            serial_number: String::new(),
            ..with_serial.clone()
        };
        assert_eq!(without.browser_print_uid(), "net:10.0.0.5:9100");
    }

    #[test]
    fn upsert_and_update_saved_printers() {
        let mut cfg = AppConfig::default();
        let a = PrinterInfo {
            name: Some("A".into()),
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: String::new(),
            address: "10.0.0.1".into(),
            port: 0,
            print_port: 9100,
            config_port: 80,
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
}
