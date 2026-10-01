mod browser_print;
mod fetch;
mod multipart;
mod resolve;
mod response;
mod simple;

#[cfg(test)]
pub(crate) mod test_util;

use crate::config::{AppConfig, PrinterInfo};
use crate::print::DeviceSessionPool;
use crate::print_log::PrintLog;
use axum::http::Method;
use axum::routing::{any, get, post};
use axum::Router;
use browser_print::{
    handle_available, handle_bp_config, handle_convert, handle_convert_scan, handle_default,
    handle_read, handle_write,
};
use simple::handle_root;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::{oneshot, Mutex, RwLock};
use tower_http::cors::{Any, CorsLayer};

#[derive(Clone)]
pub struct HttpSharedState {
    pub config: Arc<RwLock<AppConfig>>,
    pub discovered: Arc<RwLock<Vec<PrinterInfo>>>,
    pub sessions: Arc<DeviceSessionPool>,
    pub print_log: Arc<PrintLog>,
}

impl HttpSharedState {
    pub(crate) async fn record_print(
        &self,
        route: &str,
        printer: &PrinterInfo,
        data: &[u8],
        result: Result<(), String>,
    ) {
        if !self.config.read().await.debug_logging {
            return;
        }
        self.print_log.record(
            route,
            printer.display_name(),
            printer.address.clone(),
            printer.print_port,
            data,
            result,
        );
    }
}

pub struct HttpServerHandle {
    shutdown: Mutex<Option<oneshot::Sender<()>>>,
    pub bind_addr: String,
}

impl HttpServerHandle {
    pub async fn stop(&self) {
        let mut guard = self.shutdown.lock().await;
        if let Some(tx) = guard.take() {
            let _ = tx.send(());
        }
    }
}

pub(crate) fn build_app_router(state: HttpSharedState) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    Router::new()
        // RawLabelPrint simple API (always on)
        .route("/", any(handle_root))
        // Zebra Browser Print compatible API (gated by config flag)
        .route("/available", any(handle_available))
        .route("/default", any(handle_default))
        .route("/config", get(handle_bp_config))
        .route("/write", post(handle_write))
        .route("/read", post(handle_read))
        // BrowserPrint.js convert / convertAndSendFile / scanImage
        .route("/convert", post(handle_convert))
        .route("/convert/scan", post(handle_convert_scan))
        .with_state(state)
        .layer(cors)
}

pub async fn start_http_server(
    listen_address: &str,
    port: u16,
    state: HttpSharedState,
) -> Result<HttpServerHandle, String> {
    let addr: SocketAddr = format!("{listen_address}:{port}")
        .parse()
        .map_err(|e| format!("Invalid listen address: {e}"))?;

    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Failed to bind HTTP {addr}: {e}"))?;

    let app = build_app_router(state);

    let (tx, rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = rx.await;
            })
            .await
            .ok();
    });

    Ok(HttpServerHandle {
        shutdown: Mutex::new(Some(tx)),
        bind_addr: addr.to_string(),
    })
}
