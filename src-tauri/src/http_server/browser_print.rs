use crate::http_server::fetch::fetch_url_bytes;
use crate::http_server::multipart::{
    multipart_boundary, parse_multipart_parts, parse_write_request,
};
use crate::http_server::origin::{ensure_origin_allowed, session_origin};
use crate::http_server::resolve::{
    collect_known_printers, resolve_by_device_ref, BrowserDeviceRef,
};
use crate::http_server::response::{empty_ok, json_err, json_ok, public_print_error, text_ok};
use crate::http_server::HttpSharedState;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub(crate) struct DefaultQuery {
    #[serde(rename = "type")]
    device_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReadBody {
    device: BrowserDeviceRef,
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

async fn compatible_enabled(state: &HttpSharedState) -> bool {
    state.config.read().await.browser_print_compatible
}

#[allow(clippy::result_large_err)]
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

pub(crate) async fn handle_available(
    State(state): State<HttpSharedState>,
    method: Method,
    headers: HeaderMap,
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
    if let Err((status, msg)) = ensure_origin_allowed(&state, &headers).await {
        return json_err(status, msg);
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

pub(crate) async fn handle_default(
    State(state): State<HttpSharedState>,
    method: Method,
    headers: HeaderMap,
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
    if let Err((status, msg)) = ensure_origin_allowed(&state, &headers).await {
        return json_err(status, msg);
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

pub(crate) async fn handle_bp_config(State(state): State<HttpSharedState>) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }

    json_ok(
        json!({
            "application": {
                "version": env!("CARGO_PKG_VERSION"),
                "build_number": 1,
                "api_level": 2,
                "platform": std::env::consts::OS,
                "supportedConversions": {}
            }
        })
        .to_string(),
    )
}

pub(crate) async fn handle_write(
    State(state): State<HttpSharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }
    if let Err((status, msg)) = ensure_origin_allowed(&state, &headers).await {
        return json_err(status, msg);
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

    let permit = match state.device_io.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return json_err(StatusCode::TOO_MANY_REQUESTS, "Too many print operations");
        }
    };
    let uid = printer.browser_print_uid();
    let origin = session_origin(&headers);
    let sessions = state.sessions.clone();
    let printer_for_write = printer.clone();
    let data_for_write = data.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        sessions.write(&uid, &origin, &printer_for_write, &data_for_write)
    })
    .await;

    let mapped = match &result {
        Ok(inner) => inner.clone(),
        Err(e) => Err(e.to_string()),
    };
    state.record_print("/write", &printer, &data, mapped).await;

    match result {
        Ok(Ok(())) => json_ok("{}".into()),
        Ok(Err(e)) => json_err(StatusCode::INTERNAL_SERVER_ERROR, public_print_error(&e)),
        Err(e) => json_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            public_print_error(&e.to_string()),
        ),
    }
}

pub(crate) async fn handle_convert(
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

pub(crate) async fn handle_convert_scan(
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
        if let Err((status, msg)) = ensure_origin_allowed(state, headers).await {
            return Ok(json_err(status, msg));
        }
        let device = meta
            .device
            .ok_or_else(|| "device required when options.action=print".to_string())?;
        let printer = resolve_by_device_ref(state, &device)
            .await
            .ok_or_else(|| "Device not found".to_string())?;
        let permit = match state.device_io.clone().try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                return Ok(json_err(
                    StatusCode::TOO_MANY_REQUESTS,
                    "Too many print operations",
                ));
            }
        };
        let uid = printer.browser_print_uid();
        let origin = session_origin(headers);
        let sessions = state.sessions.clone();
        let printer_for_write = printer.clone();
        let blob_for_write = blob.clone();
        let result = tokio::task::spawn_blocking(move || {
            let _permit = permit;
            sessions.write(&uid, &origin, &printer_for_write, &blob_for_write)
        })
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r);
        state
            .record_print("/convert", &printer, &blob, result.clone())
            .await;
        if let Err(err) = result {
            return Ok(json_err(
                StatusCode::INTERNAL_SERVER_ERROR,
                public_print_error(&err),
            ));
        }
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
    let lower = t.to_ascii_lowercase();
    lower.starts_with("^xa") || t.starts_with('~') || t.starts_with("CT~~CD")
}

pub(crate) async fn handle_read(
    State(state): State<HttpSharedState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(resp) = require_compatible(&state).await {
        return resp;
    }
    if let Err((status, msg)) = ensure_origin_allowed(&state, &headers).await {
        return json_err(status, msg);
    }

    let parsed: ReadBody = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(e) => return json_err(StatusCode::BAD_REQUEST, format!("Invalid JSON: {e}")),
    };

    let Some(printer) = resolve_by_device_ref(&state, &parsed.device).await else {
        return json_err(StatusCode::NOT_FOUND, "Device not found");
    };

    let permit = match state.device_io.clone().try_acquire_owned() {
        Ok(permit) => permit,
        Err(_) => {
            return json_err(StatusCode::TOO_MANY_REQUESTS, "Too many print operations");
        }
    };
    let uid = printer.browser_print_uid();
    let origin = session_origin(&headers);
    let sessions = state.sessions.clone();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        sessions.read(&uid, &origin, &printer)
    })
    .await;

    match result {
        Ok(Ok(text)) => text_ok(text),
        Ok(Err(e)) => json_err(StatusCode::INTERNAL_SERVER_ERROR, public_print_error(&e)),
        Err(e) => json_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            public_print_error(&e.to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_server::test_util::{
        accept_one_payload, body_bytes, body_json, build_test_router, lan_printer, sample_printer,
        state_with_printer, test_state,
    };
    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{HeaderValue, Request};
    use axum::response::IntoResponse;
    use std::sync::Arc;
    use tower::ServiceExt;

    #[test]
    fn raw_label_detection() {
        assert!(blob_looks_like_raw_label(b"^XA^XZ"));
        assert!(blob_looks_like_raw_label(b"CT~~CD,~CC^~CT~\n^XA"));
        assert!(blob_looks_like_raw_label(b"~HS"));
        assert!(!blob_looks_like_raw_label(b"prefix ^XZ suffix"));
        assert!(!blob_looks_like_raw_label(b"%PDF-1.4"));
        assert!(!blob_looks_like_raw_label(&[0xff, 0xd8, 0xff, 0xe0]));
    }

    #[tokio::test]
    async fn available_and_default_contract_when_compatible() {
        let printer = sample_printer("10.0.0.1", "SN1");
        let state = test_state(true, Some(printer));
        let app = build_test_router(state);

        let available = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/available")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(available.status(), StatusCode::OK);
        let avail_json = body_json(available).await;
        assert!(avail_json["printer"].is_array());
        assert!(avail_json["deviceList"].is_array());
        assert_eq!(avail_json["printer"][0]["uid"], "SN1");

        let default = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/default?type=printer")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(default.status(), StatusCode::OK);
        let def_json = body_json(default).await;
        assert_eq!(def_json["uid"], "SN1");

        let config = app
            .oneshot(
                Request::builder()
                    .uri("/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(config.status(), StatusCode::OK);
        let cfg_json = body_json(config).await;
        assert_eq!(cfg_json["application"]["api_level"], 2);
        assert_eq!(
            cfg_json["application"]["version"],
            env!("CARGO_PKG_VERSION")
        );
    }

    #[tokio::test]
    async fn default_empty_body_when_no_default_printer() {
        let state = test_state(true, None);
        let app = build_test_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/default")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = body_bytes(response).await;
        assert!(bytes.is_empty());
    }

    #[tokio::test]
    async fn compatible_off_returns_404_for_bp_routes() {
        let state = test_state(false, Some(sample_printer("10.0.0.1", "SN1")));
        let app = build_test_router(state);

        let available = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/available")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(available.status(), StatusCode::NOT_FOUND);

        let write = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/write")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(r#"{"device":{"uid":"SN1"},"data":"^XA^XZ"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(write.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn write_json_reaches_mock_tcp() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(std::sync::Mutex::new(Vec::new()));
        let received_clone = received.clone();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
            let mut buf = [0u8; 4096];
            use std::io::Read;
            let n = stream.read(&mut buf).unwrap_or(0);
            *received_clone.lock().unwrap() = buf[..n].to_vec();
        });

        let mut printer = sample_printer("127.0.0.1", "MOCKWRITE");
        printer.print_port = port;
        let state = test_state(true, Some(printer));
        let app = build_test_router(state);

        let zpl = "^XA^FDHttpWrite^FS^XZ";
        let response = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/write")
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(Body::from(format!(
                        r#"{{"device":{{"uid":"MOCKWRITE"}},"data":"{zpl}"}}"#
                    )))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        handle.join().unwrap();
        assert_eq!(String::from_utf8_lossy(&received.lock().unwrap()), zpl);
    }

    #[tokio::test]
    async fn handle_write_records_in_print_log() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(accept_one_payload(listener));

        let state = state_with_printer(lan_printer("127.0.0.1", addr.port()), true).await;
        let zpl = "^XA^FDWriteLog^FS^XZ";
        let body = format!(r#"{{"device":{{"uid":"SERIAL1"}},"data":"{zpl}"}}"#);
        let response =
            handle_write(State(state.clone()), HeaderMap::new(), Bytes::from(body)).await;
        let (parts, _) = response.into_response().into_parts();
        assert_eq!(parts.status, StatusCode::OK);

        let received = accept.await.expect("join");
        assert_eq!(String::from_utf8_lossy(&received), zpl);

        let entries = state.print_log.list();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].route, "/write");
        assert!(entries[0].ok);
        assert_eq!(entries[0].data, zpl.as_bytes());
    }

    #[tokio::test]
    async fn handle_convert_print_records_in_print_log() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(accept_one_payload(listener));

        let state = state_with_printer(lan_printer("127.0.0.1", addr.port()), true).await;
        let boundary = "----TestBoundary";
        let zpl = "^XA^FDConvert^FS^XZ";
        let body = format!(
            "--{boundary}\r\n\
Content-Disposition: form-data; name=\"json\"\r\n\r\n\
{{\"device\":{{\"uid\":\"SERIAL1\"}},\"options\":{{\"action\":\"print\"}}}}\r\n\
--{boundary}\r\n\
Content-Disposition: form-data; name=\"blob\"\r\n\r\n\
{zpl}\r\n\
--{boundary}--\r\n"
        );
        let mut headers = HeaderMap::new();
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&format!("multipart/form-data; boundary={boundary}")).unwrap(),
        );

        let response = handle_convert(State(state.clone()), headers, Bytes::from(body)).await;
        let (parts, _) = response.into_response().into_parts();
        assert_eq!(parts.status, StatusCode::OK);

        let received = accept.await.expect("join");
        assert_eq!(String::from_utf8_lossy(&received), zpl);

        let entries = state.print_log.list();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].route, "/convert");
        assert!(entries[0].ok);
    }

    #[tokio::test]
    async fn available_and_read_require_an_approved_origin() {
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")));
        let app = build_test_router(state.clone());
        let available = app
            .oneshot(
                Request::builder()
                    .uri("/available")
                    .header(header::ORIGIN, "https://new.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(available.status(), StatusCode::FORBIDDEN);

        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            HeaderValue::from_static("https://new.example"),
        );
        let response = handle_read(
            State(state),
            headers,
            Bytes::from(r#"{"device":{"uid":"SN1"}}"#),
        )
        .await;
        let (parts, _) = response.into_response().into_parts();
        assert_eq!(parts.status, StatusCode::FORBIDDEN);
    }
}
