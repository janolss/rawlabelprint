use crate::config::{AppConfig, PrinterInfo};
use crate::print::send_raw_to_printer;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::any;
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
        .route("/", any(handle_root))
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
        Ok(body) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"),
            ],
            body,
        )
            .into_response(),
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

    let list: Vec<_> = by_key
        .values()
        .map(|p| json!({ "Name": p.display_name() }))
        .collect();
    serde_json::to_string(&list).unwrap_or_else(|_| "[]".into())
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
