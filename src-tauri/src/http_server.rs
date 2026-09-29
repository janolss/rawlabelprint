use crate::config::{AppConfig, PrinterInfo};
use crate::print::{send_raw_to_printer, DeviceSessionPool};
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get, post};
use axum::Router;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
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

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);

    let app = Router::new()
        // RawLabelPrint simple API (always on)
        .route("/", any(handle_root))
        // Zebra Browser Print compatible API (gated by config flag)
        .route("/available", any(handle_available))
        .route("/default", any(handle_default))
        .route("/config", get(handle_bp_config))
        .route("/write", post(handle_write))
        .route("/read", post(handle_read))
        .with_state(state)
        .layer(cors);

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

#[derive(Debug, Deserialize)]
struct PrintQuery {
    #[serde(alias = "p")]
    printer: Option<String>,
    #[serde(alias = "d")]
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PrintBody {
    printer: Option<String>,
    data: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DefaultQuery {
    #[serde(rename = "type")]
    device_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BrowserDeviceRef {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    uid: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WriteBody {
    device: BrowserDeviceRef,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadBody {
    device: BrowserDeviceRef,
}

fn json_ok(body: String) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    )
        .into_response()
}

fn json_err(status: StatusCode, msg: impl Into<String>) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        json!({ "error": msg.into() }).to_string(),
    )
        .into_response()
}

fn text_ok(body: String) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        body,
    )
        .into_response()
}

fn empty_ok() -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
        ],
        String::new(),
    )
        .into_response()
}

async fn compatible_enabled(state: &HttpSharedState) -> bool {
    state.config.read().await.browser_print_compatible
}

async fn require_compatible(state: &HttpSharedState) -> Result<(), Response> {
    if compatible_enabled(state).await {
        Ok(())
    } else {
        Err(json_err(
            StatusCode::NOT_FOUND,
            "Browser Print compatible mode is disabled. Enable it in Settings.",
        ))
    }
}

async fn handle_root(
    State(state): State<HttpSharedState>,
    method: Method,
    Query(query): Query<PrintQuery>,
    body: Bytes,
) -> Response {
    if method == Method::OPTIONS {
        return (
            StatusCode::NO_CONTENT,
            [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
            "",
        )
            .into_response();
    }

    let result = match method {
        Method::GET => handle_get(&state, query).await,
        Method::POST => handle_post(&state, &body).await,
        _ => Err((
            StatusCode::METHOD_NOT_ALLOWED,
            json!({ "error": "Method not allowed" }).to_string(),
        )),
    };

    match result {
        Ok(body) => json_ok(body),
        Err((status, body)) => (
            status,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            body,
        )
            .into_response(),
    }
}

async fn handle_get(
    state: &HttpSharedState,
    query: PrintQuery,
) -> Result<String, (StatusCode, String)> {
    let printer_name = query.printer.unwrap_or_default();
    let print_data = query.data.unwrap_or_default();

    if printer_name.is_empty() && print_data.is_empty() {
        return Ok(list_printers_json(state).await);
    }

    do_print(state, &printer_name, &print_data).await
}

async fn handle_post(
    state: &HttpSharedState,
    body: &Bytes,
) -> Result<String, (StatusCode, String)> {
    let parsed: PrintBody = serde_json::from_slice(body).map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": format!("Invalid JSON: {e}") }).to_string(),
        )
    })?;

    let printer_name = parsed.printer.unwrap_or_default();
    let print_data = parsed.data.unwrap_or_default();
    do_print(state, &printer_name, &print_data).await
}

async fn list_printers_json(state: &HttpSharedState) -> String {
    let printers = collect_known_printers(state).await;
    let list: Vec<_> = printers
        .iter()
        .map(|p| json!({ "Name": p.display_name() }))
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
}

async fn collect_known_printers(state: &HttpSharedState) -> Vec<PrinterInfo> {
    let config = state.config.read().await;
    let discovered = state.discovered.read().await;

    let mut by_key: HashMap<String, PrinterInfo> = HashMap::new();
    for p in discovered.iter() {
        by_key.insert(p.address.clone(), p.clone());
    }
    for p in config.added_printers.iter() {
        by_key.insert(p.address.clone(), p.clone());
    }
    if let Some(p) = &config.default_printer {
        by_key.insert(p.address.clone(), p.clone());
    }
    by_key.into_values().collect()
}

async fn do_print(
    state: &HttpSharedState,
    printer_name: &str,
    print_data: &str,
) -> Result<String, (StatusCode, String)> {
    if print_data.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            json!({ "error": "NO PRINT DATA PROVIDED" }).to_string(),
        ));
    }

    let printer = resolve_printer(state, printer_name).await.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            json!({ "error": "PRINTER NOT FOUND" }).to_string(),
        )
    })?;

    send_raw_to_printer(&printer, print_data).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({ "error": e }).to_string(),
        )
    })?;

    Ok(json!({
        "printer": printer.display_name(),
        "data": print_data
    })
    .to_string())
}

async fn resolve_printer(state: &HttpSharedState, printer_name: &str) -> Option<PrinterInfo> {
    let config = state.config.read().await;
    let discovered = state.discovered.read().await;

    if printer_name.trim().is_empty() {
        return config.default_printer.clone();
    }

    if let Some(p) = &config.default_printer {
        if p.matches_name(printer_name) {
            return Some(p.clone());
        }
    }
    for p in config.added_printers.iter() {
        if p.matches_name(printer_name) {
            return Some(p.clone());
        }
    }
    for p in discovered.iter() {
        if p.matches_name(printer_name) {
            return Some(p.clone());
        }
    }
    None
}

async fn resolve_by_device_ref(
    state: &HttpSharedState,
    device: &BrowserDeviceRef,
) -> Option<PrinterInfo> {
    let uid = device.uid.as_deref().unwrap_or("").trim();
    let name = device.name.as_deref().unwrap_or("").trim();

    let printers = collect_known_printers(state).await;
    if !uid.is_empty() {
        if let Some(p) = printers.iter().find(|p| p.browser_print_uid() == uid) {
            return Some(p.clone());
        }
        // Also accept raw address or net:host:port forms
        if let Some(rest) = uid.strip_prefix("net:") {
            let host = rest.split(':').next().unwrap_or(rest);
            if let Some(p) = printers.iter().find(|p| p.address == host) {
                return Some(p.clone());
            }
        }
        if let Some(p) = printers.iter().find(|p| p.address == uid || p.serial_number == uid) {
            return Some(p.clone());
        }
    }
    if !name.is_empty() {
        if let Some(p) = printers.iter().find(|p| p.matches_name(name)) {
            return Some(p.clone());
        }
    }
    None
}

// --- Browser Print compatible handlers ---

async fn handle_available(
    State(state): State<HttpSharedState>,
    method: Method,
) -> Response {
    if method == Method::OPTIONS {
        return (
            StatusCode::NO_CONTENT,
            [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
            "",
        )
            .into_response();
    }
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }

    let printers = collect_known_printers(&state).await;
    let devices: Vec<_> = printers
        .iter()
        .map(|p| p.to_browser_print_device())
        .collect();

    json_ok(
        json!({
            "printer": devices.clone(),
            "deviceList": devices,
        })
        .to_string(),
    )
}

async fn handle_default(
    State(state): State<HttpSharedState>,
    method: Method,
    Query(query): Query<DefaultQuery>,
) -> Response {
    if method == Method::OPTIONS {
        return (
            StatusCode::NO_CONTENT,
            [(header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")],
            "",
        )
            .into_response();
    }
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }

    // BrowserPrint.js treats empty body as null default device.
    let cfg = state.config.read().await;
    let Some(printer) = cfg.default_printer.clone() else {
        return empty_ok();
    };

    if let Some(t) = query.device_type.as_deref() {
        if !t.is_empty() && !t.eq_ignore_ascii_case("printer") {
            return empty_ok();
        }
    }

    json_ok(printer.to_browser_print_device().to_string())
}

async fn handle_bp_config(State(state): State<HttpSharedState>) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }

    json_ok(
        json!({
            "application": {
                "version": "0.1.0",
                "build_number": 1,
                "api_level": 2,
                "platform": "macOS",
                "supportedConversions": {}
            }
        })
        .to_string(),
    )
}

async fn handle_write(State(state): State<HttpSharedState>, body: Bytes) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }

    // JSON body (Device.send). Multipart sendFile is not required for ZPL label flows.
    let parsed: WriteBody = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return json_err(StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")),
    };

    let Some(printer) = resolve_by_device_ref(&state, &parsed.device).await else {
        return json_err(StatusCode::NOT_FOUND, "Device not found");
    };

    let data = if let Some(d) = parsed.data {
        d
    } else if let Some(url) = parsed.url {
        match fetch_url_bytes(&url).await {
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(s) => s,
                Err(e) => String::from_utf8_lossy(&e.into_bytes()).into_owned(),
            },
            Err(e) => return json_err(StatusCode::BAD_REQUEST, e),
        }
    } else {
        return json_err(StatusCode::BAD_REQUEST, "No data or url provided");
    };

    let uid = printer.browser_print_uid();
    let sessions = state.sessions.clone();
    let result = tokio::task::spawn_blocking(move || sessions.write(&uid, &printer, data.as_bytes()))
        .await;

    match result {
        Ok(Ok(())) => json_ok("{}".into()),
        Ok(Err(e)) => json_err(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn handle_read(State(state): State<HttpSharedState>, body: Bytes) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }

    let parsed: ReadBody = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return json_err(StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")),
    };

    let Some(printer) = resolve_by_device_ref(&state, &parsed.device).await else {
        return json_err(StatusCode::NOT_FOUND, "Device not found");
    };

    let uid = printer.browser_print_uid();
    let sessions = state.sessions.clone();
    let result = tokio::task::spawn_blocking(move || sessions.read(&uid, &printer)).await;

    match result {
        Ok(Ok(text)) => text_ok(text),
        Ok(Err(e)) => json_err(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

async fn fetch_url_bytes(url: &str) -> Result<Vec<u8>, String> {
    // Minimal dependency-free HTTP GET via std — prefer reqwest if added later.
    // For now only support http:// URLs with a simple blocking fetch in spawn_blocking.
    let url = url.to_string();
    tokio::task::spawn_blocking(move || {
        use std::io::Read;
        use std::net::TcpStream;
        use std::time::Duration;

        let url = url
            .strip_prefix("http://")
            .ok_or_else(|| "Only http:// URLs are supported for sendUrl".to_string())?;
        let (host_port, path) = match url.split_once('/') {
            Some((h, p)) => (h, format!("/{p}")),
            None => (url, "/".to_string()),
        };
        let (host, port) = match host_port.split_once(':') {
            Some((h, p)) => (h, p.parse::<u16>().unwrap_or(80)),
            None => (host_port, 80),
        };
        let addr = format!("{host}:{port}");
        let mut stream = TcpStream::connect(addr).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .ok();
        let req = format!(
            "GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n"
        );
        use std::io::Write;
        stream.write_all(req.as_bytes()).map_err(|e| e.to_string())?;
        let mut buf = Vec::new();
        stream.read_to_end(&mut buf).map_err(|e| e.to_string())?;
        if let Some(pos) = find_header_end(&buf) {
            Ok(buf[pos..].to_vec())
        } else {
            Ok(buf)
        }
    })
    .await
    .map_err(|e| e.to_string())?
}

fn find_header_end(buf: &[u8]) -> Option<usize> {
    buf.windows(4)
        .position(|w| w == b"\r\n\r\n")
        .map(|i| i + 4)
}
