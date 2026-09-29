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
        false
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
}

fn default_listen_address() -> String {
    "127.0.0.1".into()
}

fn default_http_port() -> u16 {
    9100
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            listen_address: default_listen_address(),
            port: default_http_port(),
            default_printer: None,
            added_printers: Vec::new(),
            launch_at_login: false,
        }
    }
}

pub fn config_path(app_data_dir: &PathBuf) -> PathBuf {
    app_data_dir.join("config.json")
}

pub fn load_config(app_data_dir: &PathBuf) -> AppConfig {
    let path = config_path(app_data_dir);
    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
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
}
