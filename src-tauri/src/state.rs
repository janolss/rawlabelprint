use crate::config::{save_config, AppConfig, PrinterInfo};
use crate::http_server::{start_http_server, HttpServerHandle, HttpSharedState};
use crate::print::DeviceSessionPool;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct AppState {
    pub app_data_dir: PathBuf,
    pub config: Arc<RwLock<AppConfig>>,
    pub discovered: Arc<RwLock<Vec<PrinterInfo>>>,
    pub sessions: Arc<DeviceSessionPool>,
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
            http: Mutex::new(None),
            http_status: Mutex::new("stopped".into()),
        }
    }

    pub fn http_shared(&self) -> HttpSharedState {
        HttpSharedState {
            config: self.config.clone(),
            discovered: self.discovered.clone(),
            sessions: self.sessions.clone(),
        }
    }

    pub async fn persist(&self) -> Result<(), String> {
        let cfg = self.config.read().await.clone();
        save_config(&self.app_data_dir, &cfg)
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
            (cfg.listen_address.clone(), cfg.port)
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
