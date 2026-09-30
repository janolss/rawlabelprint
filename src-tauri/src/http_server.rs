use crate::config::{AppConfig, PrinterInfo};
use crate::print::{send_raw_to_printer, DeviceSessionPool};
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
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
        // BrowserPrint.js convert / convertAndSendFile / scanImage
        .route("/convert", post(handle_convert))
        .route("/convert/scan", post(handle_convert_scan))
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

async fn handle_write(
    State(state): State<HttpSharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }

    let parsed = match parse_write_request(&headers, &body) {
        Ok(v) => v,
        Err(e) => return json_err(StatusCode::BAD_REQUEST, e),
    };

    let Some(printer) = resolve_by_device_ref(&state, &parsed.device).await else {
        return json_err(StatusCode::NOT_FOUND, "Device not found");
    };

    let data = if !parsed.data.is_empty() {
        parsed.data
    } else if let Some(url) = parsed.url {
        match fetch_url_bytes(&url).await {
            Ok(bytes) => bytes,
            Err(e) => return json_err(StatusCode::BAD_REQUEST, e),
        }
    } else {
        return json_err(StatusCode::BAD_REQUEST, "No data or url provided");
    };

    let uid = printer.browser_print_uid();
    let sessions = state.sessions.clone();
    let result =
        tokio::task::spawn_blocking(move || sessions.write(&uid, &printer, &data)).await;

    match result {
        Ok(Ok(())) => json_ok("{}".into()),
        Ok(Err(e)) => json_err(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => json_err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    }
}

struct ParsedWrite {
    device: BrowserDeviceRef,
    data: Vec<u8>,
    url: Option<String>,
}

/// BrowserPrint.js uses either JSON (`device.send` / `sendUrl`) or multipart
/// FormData with fields `json` + `blob` (`device.sendFile`).
fn parse_write_request(headers: &HeaderMap, body: &[u8]) -> Result<ParsedWrite, String> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if let Some(boundary) = multipart_boundary(content_type) {
        return parse_write_multipart(body, &boundary);
    }

    // Fallback: some clients omit Content-Type but still send JSON.
    if body.starts_with(b"--") {
        return Err("multipart body without boundary in Content-Type".into());
    }

    let parsed: WriteBody =
        serde_json::from_slice(body).map_err(|e| format!("Invalid JSON: {e}"))?;
    Ok(ParsedWrite {
        device: parsed.device,
        data: parsed.data.unwrap_or_default().into_bytes(),
        url: parsed.url,
    })
}

fn multipart_boundary(content_type: &str) -> Option<String> {
    let ct = content_type.trim();
    if !ct.to_ascii_lowercase().starts_with("multipart/") {
        return None;
    }
    for part in ct.split(';').skip(1) {
        let part = part.trim();
        let (k, v) = part.split_once('=')?;
        if k.eq_ignore_ascii_case("boundary") {
            let v = v.trim().trim_matches('"');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

fn parse_write_multipart(body: &[u8], boundary: &str) -> Result<ParsedWrite, String> {
    let parts = parse_multipart_parts(body, boundary)?;
    let json_bytes = parts
        .get("json")
        .ok_or_else(|| "Missing multipart field 'json'".to_string())?;
    let meta: WriteBody =
        serde_json::from_slice(json_bytes).map_err(|e| format!("Invalid JSON: {e}"))?;

    let mut data = parts.get("blob").cloned().unwrap_or_default();
    if data.is_empty() {
        if let Some(d) = meta.data {
            data = d.into_bytes();
        }
    }

    Ok(ParsedWrite {
        device: meta.device,
        data,
        url: meta.url,
    })
}

/// Minimal multipart/form-data parser for BrowserPrint field names.
fn parse_multipart_parts(
    body: &[u8],
    boundary: &str,
) -> Result<HashMap<String, Vec<u8>>, String> {
    let delim = format!("--{boundary}").into_bytes();
    let mut map = HashMap::new();
    let mut pos = 0usize;

    while pos < body.len() {
        let Some(start) = find_bytes(&body[pos..], &delim).map(|i| pos + i) else {
            break;
        };
        let mut cursor = start + delim.len();
        if body.get(cursor..cursor + 2) == Some(b"--") {
            break; // closing boundary
        }
        if body.get(cursor..cursor + 2) == Some(b"\r\n") {
            cursor += 2;
        } else if body.get(cursor..cursor + 1) == Some(b"\n") {
            cursor += 1;
        }

        // find_header_end returns index of first body byte (after \r\n\r\n)
        let header_end = find_header_end(&body[cursor..])
            .map(|i| cursor + i)
            .ok_or_else(|| "Malformed multipart part (no header end)".to_string())?;
        let header_block_end = header_end.saturating_sub(4);
        let headers_text = std::str::from_utf8(&body[cursor..header_block_end])
            .map_err(|_| "Invalid multipart headers encoding".to_string())?;

        let name = multipart_field_name(headers_text)
            .ok_or_else(|| "Multipart part missing Content-Disposition name".to_string())?;

        let next_boundary = find_bytes(&body[header_end..], &delim)
            .map(|i| header_end + i)
            .unwrap_or(body.len());
        let mut value_end = next_boundary;
        if value_end >= 2 && &body[value_end - 2..value_end] == b"\r\n" {
            value_end -= 2;
        } else if value_end >= 1 && body[value_end - 1] == b'\n' {
            value_end -= 1;
        }

        map.insert(name, body[header_end..value_end].to_vec());
        pos = next_boundary;
    }

    if map.is_empty() {
        return Err("No multipart parts found".into());
    }
    Ok(map)
}

fn multipart_field_name(headers: &str) -> Option<String> {
    for line in headers.split('\n') {
        let line = line.trim().trim_end_matches('\r');
        let lower = line.to_ascii_lowercase();
        if !lower.starts_with("content-disposition:") {
            continue;
        }
        for part in line.split(';').skip(1) {
            let part = part.trim();
            let (k, v) = match part.split_once('=') {
                Some(kv) => kv,
                None => continue,
            };
            if k.eq_ignore_ascii_case("name") {
                return Some(v.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}

async fn handle_convert(
    State(state): State<HttpSharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }
    match handle_convert_inner(&state, &headers, &body, false).await {
        Ok(resp) => resp,
        Err(e) => json_err(StatusCode::BAD_REQUEST, e),
    }
}

async fn handle_convert_scan(
    State(state): State<HttpSharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }
    match handle_convert_inner(&state, &headers, &body, true).await {
        Ok(resp) => resp,
        Err(e) => json_err(StatusCode::BAD_REQUEST, e),
    }
}

#[derive(Debug, Deserialize)]
struct ConvertMeta {
    #[serde(default)]
    device: Option<BrowserDeviceRef>,
    #[serde(default)]
    options: Option<ConvertOptions>,
}

#[derive(Debug, Deserialize)]
struct ConvertOptions {
    #[serde(default)]
    action: Option<String>,
    /// Present in BrowserPrint convert FormData; reserved for future image/PDF conversion.
    #[serde(default, rename = "fromFormat")]
    #[allow(dead_code)]
    from_format: Option<String>,
}

async fn handle_convert_inner(
    state: &HttpSharedState,
    headers: &HeaderMap,
    body: &[u8],
    _scan: bool,
) -> Result<Response, String> {
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let boundary = multipart_boundary(content_type)
        .ok_or_else(|| "Expected multipart/form-data for /convert".to_string())?;
    let parts = parse_multipart_parts(body, &boundary)?;
    let json_bytes = parts
        .get("json")
        .ok_or_else(|| "Missing multipart field 'json'".to_string())?;
    let meta: ConvertMeta =
        serde_json::from_slice(json_bytes).map_err(|e| format!("Invalid JSON: {e}"))?;
    let blob = parts.get("blob").cloned().unwrap_or_default();

    // BrowserPrint convertAndSendFile uses options.action = "print".
    // If the payload is already printable raw (ZPL/EPL), write it through.
    let action = meta
        .options
        .as_ref()
        .and_then(|o| o.action.as_deref())
        .unwrap_or("");
    let looks_raw = blob_looks_like_raw_label(&blob);

    if action.eq_ignore_ascii_case("print") && looks_raw {
        let device = meta
            .device
            .ok_or_else(|| "device required when options.action=print".to_string())?;
        let printer = resolve_by_device_ref(state, &device)
            .await
            .ok_or_else(|| "Device not found".to_string())?;
        let uid = printer.browser_print_uid();
        let sessions = state.sessions.clone();
        tokio::task::spawn_blocking(move || sessions.write(&uid, &printer, &blob))
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e)?;
        return Ok(json_ok("{}".into()));
    }

    if looks_raw {
        // Return as converted payload so client can send it.
        let zpl = String::from_utf8_lossy(&blob).into_owned();
        return Ok(json_ok(json!({ "zpl": zpl, "data": zpl }).to_string()));
    }

    Err(
        "Image/PDF conversion is not supported. Send ZPL/EPL via device.send or device.sendFile."
            .into(),
    )
}

fn blob_looks_like_raw_label(blob: &[u8]) -> bool {
    let sample = &blob[..blob.len().min(512)];
    let text = String::from_utf8_lossy(sample);
    let t = text.trim_start();
    t.starts_with('^')
        || t.starts_with('~')
        || t.contains("^XA")
        || t.contains("^xz")
        || t.contains("^XZ")
        || t.starts_with("CT~~CD") // ZebraDesigner / Zebra setup preamble
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn multipart_boundary_extraction() {
        assert_eq!(
            multipart_boundary(
                "multipart/form-data; boundary=----WebKitFormBoundaryjCzCASXAXUOmmw2k"
            )
            .as_deref(),
            Some("----WebKitFormBoundaryjCzCASXAXUOmmw2k")
        );
        assert!(multipart_boundary("application/json").is_none());
    }

    #[test]
    fn parse_browserprint_sendfile_multipart() {
        let boundary = "----WebKitFormBoundaryjCzCASXAXUOmmw2k";
        let zpl = "^XA^FO50,50^FDHi^FS^XZ";
        let body = format!(
            "------WebKitFormBoundaryjCzCASXAXUOmmw2k\r\n\
Content-Disposition: form-data; name=\"json\"\r\n\r\n\
{{\"device\":{{\"name\":\"ZTC ZD421\",\"uid\":\"D6J231909982\",\"deviceType\":\"printer\"}}}}\r\n\
------WebKitFormBoundaryjCzCASXAXUOmmw2k\r\n\
Content-Disposition: form-data; name=\"blob\"; filename=\"blob\"\r\n\
Content-Type: text/plain\r\n\r\n\
{zpl}\r\n\
------WebKitFormBoundaryjCzCASXAXUOmmw2k--\r\n"
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}")).unwrap(),
        );

        let parsed = parse_write_request(&headers, body.as_bytes()).unwrap();
        assert_eq!(parsed.device.uid.as_deref(), Some("D6J231909982"));
        assert_eq!(String::from_utf8_lossy(&parsed.data), zpl);
    }

    #[test]
    fn parse_json_send_still_works() {
        let body = br#"{"device":{"uid":"ABC"},"data":"^XA^XZ"}"#;
        let headers = HeaderMap::new();
        let parsed = parse_write_request(&headers, body).unwrap();
        assert_eq!(parsed.device.uid.as_deref(), Some("ABC"));
        assert_eq!(String::from_utf8_lossy(&parsed.data), "^XA^XZ");
    }

    #[test]
    fn raw_label_detection() {
        assert!(blob_looks_like_raw_label(b"^XA^XZ"));
        assert!(blob_looks_like_raw_label(b"CT~~CD,~CC^~CT~\n^XA"));
        assert!(!blob_looks_like_raw_label(b"%PDF-1.4"));
    }
}
