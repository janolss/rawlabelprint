#![cfg(test)]

use crate::config::{AppConfig, PrinterInfo, CONNECTION_NETWORK};
use crate::http_server::{build_app_router, HttpSharedState};
use crate::print::DeviceSessionPool;
use crate::print_log::PrintLog;
use axum::response::Response;
use axum::Router;
use http_body_util::BodyExt;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

pub(crate) fn sample_printer(address: &str, serial: &str) -> PrinterInfo {
    PrinterInfo {
        name: Some(format!("Printer {address}")),
        model: "ZD421".into(),
        firmware: String::new(),
        serial_number: serial.into(),
        address: address.into(),
        port: 0,
        print_port: 9100,
        config_port: 80,
        connection: CONNECTION_NETWORK.into(),
    }
}

pub(crate) fn lan_printer(address: &str, print_port: u16) -> PrinterInfo {
    PrinterInfo {
        name: Some("LanPrinter".into()),
        model: "ZD421".into(),
        firmware: String::new(),
        serial_number: "SERIAL1".into(),
        address: address.into(),
        port: 0,
        print_port,
        config_port: 80,
        connection: CONNECTION_NETWORK.into(),
    }
}

pub(crate) fn test_state(
    compatible: bool,
    default: Option<PrinterInfo>,
    discovered: Vec<PrinterInfo>,
) -> HttpSharedState {
    let mut config = AppConfig {
        browser_print_compatible: compatible,
        ..Default::default()
    };
    if let Some(p) = default {
        config.upsert_printer(p, true);
    }
    HttpSharedState {
        config: Arc::new(RwLock::new(config)),
        discovered: Arc::new(RwLock::new(discovered)),
        sessions: Arc::new(DeviceSessionPool::new()),
        print_log: Arc::new(PrintLog::new()),
    }
}

pub(crate) async fn state_with_printer(
    printer: PrinterInfo,
    debug_logging: bool,
) -> HttpSharedState {
    let mut config = AppConfig {
        debug_logging,
        browser_print_compatible: true,
        ..Default::default()
    };
    config.upsert_printer(printer, true);
    HttpSharedState {
        config: Arc::new(RwLock::new(config)),
        discovered: Arc::new(RwLock::new(Vec::new())),
        sessions: Arc::new(DeviceSessionPool::new()),
        print_log: Arc::new(PrintLog::new()),
    }
}

pub(crate) fn build_test_router(state: HttpSharedState) -> Router {
    build_app_router(state)
}

pub(crate) async fn body_bytes(response: Response) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec()
}

pub(crate) async fn body_json(response: Response) -> serde_json::Value {
    let bytes = body_bytes(response).await;
    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null)
}

pub(crate) async fn accept_one_payload(listener: TcpListener) -> Vec<u8> {
    use tokio::io::AsyncReadExt;
    let (mut sock, _) = listener.accept().await.expect("accept");
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(200), sock.read(&mut tmp)).await
        {
            Ok(Ok(0)) | Err(_) => break,
            Ok(Ok(n)) => buf.extend_from_slice(&tmp[..n]),
            Ok(Err(_)) => break,
        }
    }
    buf
}
