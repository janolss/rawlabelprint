use crate::config::{save_config, AppConfig, PrinterInfo};
use crate::http_server::{start_http_server, HttpServerHandle, HttpSharedState};
use crate::print::DeviceSessionPool;
use crate::print_log::PrintLog;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct AppState {
    pub app_data_dir: PathBuf,
    pub config: Arc<RwLock<AppConfig>>,
    pub discovered: Arc<RwLock<Vec<PrinterInfo>>>,
    pub sessions: Arc<DeviceSessionPool>,
    pub print_log: Arc<PrintLog>,
    pub http: Mutex<Option<HttpServerHandle>>,
    pub http_status: Mutex<String>,
}

impl AppState {
    pub fn new(app_data_dir: PathBuf, config: AppConfig) -> Self {
        Self {
            app_data_dir,
            config: Arc::new(RwLock::new(config)),
            discovered: Arc::new(RwLock::new(Vec::new())),
            sessions: Arc::new(DeviceSessionPool::new()),
            print_log: Arc::new(PrintLog::new()),
            http: Mutex::new(None),
            http_status: Mutex::new("stopped".into()),
        }
    }

    pub fn http_shared(&self) -> HttpSharedState {
        HttpSharedState {
            config: self.config.clone(),
            discovered: self.discovered.clone(),
            sessions: self.sessions.clone(),
            print_log: self.print_log.clone(),
        }
    }

    pub async fn persist(&self) -> Result<(), String> {
        let cfg = self.config.read().await.clone();
        save_config(&self.app_data_dir, &cfg)
    }

    pub async fn record_print_log(
        &self,
        route: &str,
        printer_name: &str,
        printer_address: &str,
        print_port: u16,
        data: &[u8],
        result: Result<(), String>,
    ) {
        if !self.config.read().await.debug_logging {
            return;
        }
        self.print_log.record(
            route,
            printer_name,
            printer_address,
            print_port,
            data,
            result,
        );
    }

    pub async fn resolve_printer_for_resend(
        &self,
        entry: &crate::print_log::PrintLogEntry,
    ) -> PrinterInfo {
        let config = self.config.read().await;
        let discovered = self.discovered.read().await;

        if let Some(p) = &config.default_printer {
            if p.address == entry.printer_address {
                return p.clone();
            }
        }
        if let Some(p) = config
            .added_printers
            .iter()
            .find(|p| p.address == entry.printer_address)
        {
            return p.clone();
        }
        if let Some(p) = discovered
            .iter()
            .find(|p| p.address == entry.printer_address)
        {
            return p.clone();
        }

        PrinterInfo {
            name: Some(entry.printer_name.clone()),
            model: "Resend".into(),
            firmware: String::new(),
            serial_number: String::new(),
            address: entry.printer_address.clone(),
            port: 0,
            print_port: if entry.print_port == 0 {
                9100
            } else {
                entry.print_port
            },
            config_port: 80,
        }
    }

    pub async fn resend_print_log(&self, id: u64) -> Result<(), String> {
        let entry = self
            .print_log
            .get(id)
            .ok_or_else(|| format!("Print log entry {id} not found"))?;
        if entry.data.is_empty() {
            return Err("Print log entry has no data to resend".into());
        }

        let printer = self.resolve_printer_for_resend(&entry).await;
        let name = printer.display_name();
        let address = printer.address.clone();
        let print_port = printer.print_port;
        let data = entry.data.clone();
        let result = tokio::task::spawn_blocking(move || {
            crate::print::send_raw_bytes_to_printer(&printer, &data)
        })
        .await
        .map_err(|e| e.to_string())?;

        self.record_print_log(
            "resend",
            &name,
            &address,
            print_port,
            &entry.data,
            result.clone(),
        )
        .await;
        result
    }

    pub async fn restart_http(&self) -> Result<String, String> {
        {
            let mut http = self.http.lock().await;
            if let Some(handle) = http.take() {
                handle.stop().await;
            }
        }

        let (listen, port) = {
            let cfg = self.config.read().await;
            (
                crate::config::sanitize_listen_address(&cfg.listen_address),
                cfg.port,
            )
        };

        match start_http_server(&listen, port, self.http_shared()).await {
            Ok(handle) => {
                let addr = handle.bind_addr.clone();
                *self.http.lock().await = Some(handle);
                *self.http_status.lock().await = format!("listening on {addr}");
                Ok(addr)
            }
            Err(e) => {
                *self.http_status.lock().await = format!("error: {e}");
                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_server::test_util::accept_one_payload;

    fn sample_printer(address: &str, print_port: u16) -> PrinterInfo {
        PrinterInfo {
            name: Some("TestPrinter".into()),
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: "TESTUID".into(),
            address: address.into(),
            port: 0,
            print_port,
            config_port: 80,
        }
    }

    #[tokio::test]
    async fn resend_print_log_sends_payload_and_records_entry() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(accept_one_payload(listener));

        let mut cfg = AppConfig {
            debug_logging: true,
            default_printer: Some(sample_printer("127.0.0.1", addr.port())),
            ..Default::default()
        };
        cfg.normalize_saved_printers();

        let state = AppState::new(std::env::temp_dir().join("rawlabelprint-resend-test"), cfg);
        let zpl = b"^XA^FDResendMe^FS^XZ";
        state
            .record_print_log("/", "TestPrinter", "127.0.0.1", addr.port(), zpl, Ok(()))
            .await;

        let id = state.print_log.list()[0].id;
        state.resend_print_log(id).await.expect("resend ok");

        let received = accept.await.expect("join");
        assert_eq!(received, zpl);

        let entries = state.print_log.list();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].route, "resend");
        assert!(entries[0].ok);
        assert_eq!(entries[0].data, zpl);
    }

    #[tokio::test]
    async fn resend_print_log_missing_id_errors() {
        let state = AppState::new(
            std::env::temp_dir().join("rawlabelprint-resend-missing"),
            AppConfig::default(),
        );
        let err = state.resend_print_log(999).await.unwrap_err();
        assert!(err.contains("not found"));
    }

    #[tokio::test]
    async fn record_print_log_respects_debug_flag() {
        let cfg = AppConfig {
            debug_logging: false,
            ..Default::default()
        };
        let state = AppState::new(std::env::temp_dir().join("rawlabelprint-debug-off"), cfg);
        state
            .record_print_log("/", "A", "10.0.0.1", 9100, b"^XA^XZ", Ok(()))
            .await;
        assert!(state.print_log.list().is_empty());
    }

    #[tokio::test]
    async fn resolve_printer_for_resend_prefers_saved_printer() {
        let mut cfg = AppConfig::default();
        let printer = sample_printer("10.0.0.5", 9100);
        cfg.upsert_printer(printer.clone(), true);
        let state = AppState::new(std::env::temp_dir().join("rawlabelprint-resolve"), cfg);

        state
            .record_print_log("/", "Other", "10.0.0.5", 9100, b"^XA^XZ", Ok(()))
            .await;
        let entry = state.print_log.list()[0].clone();
        let resolved = state.resolve_printer_for_resend(&entry).await;
        assert_eq!(resolved.serial_number, "TESTUID");
        assert_eq!(resolved.display_name(), printer.display_name());
    }
}
