use crate::config::{save_config, AppConfig, Connection, PrinterInfo};
use crate::http_server::{
    start_http_server, HttpServerHandle, HttpSharedState, PendingOrigins, MAX_CONCURRENT_DEVICE_IO,
};
use crate::print::DeviceSessionPool;
use crate::print_log::PrintLog;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex, RwLock, Semaphore};

pub struct AppState {
    pub app_data_dir: PathBuf,
    pub config: Arc<RwLock<AppConfig>>,
    pub sessions: Arc<DeviceSessionPool>,
    pub print_log: Arc<PrintLog>,
    pub pending_origins: Arc<RwLock<PendingOrigins>>,
    pub pending_tx: broadcast::Sender<String>,
    pub device_io: Arc<Semaphore>,
    pub http: Mutex<Option<HttpServerHandle>>,
    pub http_status: Mutex<String>,
}

impl AppState {
    pub fn new(app_data_dir: PathBuf, config: AppConfig) -> Self {
        let (pending_tx, _) = broadcast::channel(32);
        Self {
            app_data_dir,
            config: Arc::new(RwLock::new(config)),
            sessions: Arc::new(DeviceSessionPool::new()),
            print_log: Arc::new(PrintLog::new()),
            pending_origins: Arc::new(RwLock::new(PendingOrigins::new())),
            pending_tx,
            device_io: Arc::new(Semaphore::new(MAX_CONCURRENT_DEVICE_IO)),
            http: Mutex::new(None),
            http_status: Mutex::new("stopped".into()),
        }
    }

    pub fn http_shared(&self) -> HttpSharedState {
        HttpSharedState {
            config: self.config.clone(),
            sessions: self.sessions.clone(),
            print_log: self.print_log.clone(),
            pending_origins: self.pending_origins.clone(),
            pending_tx: self.pending_tx.clone(),
            device_io: self.device_io.clone(),
        }
    }

    pub fn subscribe_pending_origins(&self) -> broadcast::Receiver<String> {
        self.pending_tx.subscribe()
    }

    pub async fn persist(&self) -> Result<(), String> {
        let cfg = self.config.read().await.clone();
        save_config(&self.app_data_dir, &cfg)
    }

    pub async fn record_print_log(
        &self,
        route: &str,
        printer: &PrinterInfo,
        data: &[u8],
        result: Result<(), String>,
    ) {
        if !self.config.read().await.debug_logging {
            return;
        }
        self.print_log.record(route, printer, data, result);
    }

    pub async fn resolve_printer_for_resend(
        &self,
        entry: &crate::print_log::PrintLogEntry,
    ) -> PrinterInfo {
        let config = self.config.read().await;

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

        let network = entry.connection == Connection::Network;
        PrinterInfo {
            name: Some(entry.printer_name.clone()),
            model: "Resend".into(),
            firmware: String::new(),
            serial_number: String::new(),
            address: entry.printer_address.clone(),
            port: 0,
            print_port: if network && entry.print_port == 0 {
                9100
            } else {
                entry.print_port
            },
            config_port: if network { 80 } else { 0 },
            connection: entry.connection,
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
        let logged = printer.clone();
        let data = entry.data.clone();
        let result = tokio::task::spawn_blocking(move || {
            crate::print::send_raw_bytes_to_printer(&printer, &data)
        })
        .await
        .map_err(|e| e.to_string())?;

        self.record_print_log("resend", &logged, &entry.data, result.clone())
            .await;
        result
    }

    pub async fn restart_http(&self) -> Result<String, String> {
        let (listen, port) = {
            let cfg = self.config.read().await;
            (
                crate::config::sanitize_listen_address(&cfg.listen_address),
                cfg.port,
            )
        };

        let new_handle = match start_http_server(&listen, port, self.http_shared()).await {
            Ok(handle) => handle,
            Err(e) => {
                if self.http.lock().await.is_none() {
                    *self.http_status.lock().await = format!("error: {e}");
                }
                return Err(e);
            }
        };

        let addr = new_handle.bind_addr.clone();
        {
            let mut http = self.http.lock().await;
            if let Some(old) = http.take() {
                old.stop().await;
            }
            *http = Some(new_handle);
        }
        *self.http_status.lock().await = format!("listening on {addr}");
        Ok(addr)
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
            connection: Connection::Network,
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

        let printer = cfg.default_printer.clone().expect("default");
        let state = AppState::new(std::env::temp_dir().join("rawlabelprint-resend-test"), cfg);
        let zpl = b"^XA^FDResendMe^FS^XZ";
        state.record_print_log("/", &printer, zpl, Ok(())).await;

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
        let printer = sample_printer("10.0.0.1", 9100);
        state
            .record_print_log("/", &printer, b"^XA^XZ", Ok(()))
            .await;
        assert!(state.print_log.list().is_empty());
    }

    #[tokio::test]
    async fn resolve_printer_for_resend_prefers_saved_printer() {
        let mut cfg = AppConfig {
            debug_logging: true,
            ..Default::default()
        };
        let printer = sample_printer("10.0.0.5", 9100);
        cfg.upsert_printer(printer.clone(), true);
        let state = AppState::new(std::env::temp_dir().join("rawlabelprint-resolve"), cfg);

        state
            .record_print_log("/", &printer, b"^XA^XZ", Ok(()))
            .await;
        let entry = state.print_log.list()[0].clone();
        let resolved = state.resolve_printer_for_resend(&entry).await;
        assert_eq!(resolved.serial_number, "TESTUID");
        assert_eq!(resolved.display_name(), printer.display_name());
    }

    #[tokio::test]
    async fn resolve_printer_for_resend_keeps_usb_connection() {
        let state = AppState::new(
            std::env::temp_dir().join("rawlabelprint-usb-resend"),
            AppConfig {
                debug_logging: true,
                ..Default::default()
            },
        );
        let printer = PrinterInfo {
            name: Some("UsbPrinter".into()),
            model: "ZD421".into(),
            firmware: String::new(),
            serial_number: String::new(),
            address: "/dev/ttyACM0".into(),
            port: 0,
            print_port: 0,
            config_port: 0,
            connection: Connection::Usb,
        };
        state
            .record_print_log("/", &printer, b"^XA^XZ", Ok(()))
            .await;
        let entry = state.print_log.list()[0].clone();
        assert_eq!(entry.print_port, 0);
        assert_eq!(entry.connection, Connection::Usb);
        let resolved = state.resolve_printer_for_resend(&entry).await;
        assert!(resolved.is_usb());
        assert_eq!(resolved.address, "/dev/ttyACM0");
        assert_eq!(resolved.print_port, 0);
    }

    #[tokio::test]
    async fn restart_http_keeps_previous_listener_when_bind_fails() {
        let dir = tempfile::tempdir().unwrap();
        let hold = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let busy_port = hold.local_addr().unwrap().port();
        let free = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let free_port = free.local_addr().unwrap().port();
        drop(free);

        let cfg = AppConfig {
            port: free_port,
            ..Default::default()
        };
        let state = AppState::new(dir.path().to_path_buf(), cfg);
        state.restart_http().await.expect("first bind");

        {
            let mut cfg = state.config.write().await;
            cfg.port = busy_port;
        }
        assert!(state.restart_http().await.is_err());

        let connected = tokio::net::TcpStream::connect(("127.0.0.1", free_port)).await;
        assert!(connected.is_ok());
        state.http.lock().await.take().unwrap().stop().await;
    }
}
