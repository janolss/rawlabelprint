use crate::http_server::origin::ensure_origin_allowed;
use crate::http_server::resolve::{list_printers_json, resolve_printer};
use crate::http_server::response::{
    error_json, json_ok, public_print_error, DEVICE_NOT_FOUND, ORIGIN_DENIED, PRINTER_UNREACHABLE,
};
use crate::http_server::HttpSharedState;
use crate::print::send_raw_to_printer;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::Deserialize;
use serde_json::json;

#[derive(Debug, Deserialize)]
pub(crate) struct PrintQuery {
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

pub(crate) async fn handle_root(
    State(state): State<HttpSharedState>,
    method: Method,
    headers: HeaderMap,
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
        Method::GET => handle_get(&state, &headers, query).await,
        Method::POST => handle_post(&state, &headers, &body).await,
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
    headers: &HeaderMap,
    query: PrintQuery,
) -> Result<String, (StatusCode, String)> {
    let printer_name = query.printer.unwrap_or_default();
    let print_data = query.data.unwrap_or_default();

    if printer_name.is_empty() && print_data.is_empty() {
        if let Err((status, msg)) = ensure_origin_allowed(state, headers).await {
            return Err((status, error_json(msg, Some(ORIGIN_DENIED))));
        }
        return Ok(list_printers_json(state).await);
    }

    do_print(state, headers, &printer_name, &print_data).await
}

async fn handle_post(
    state: &HttpSharedState,
    headers: &HeaderMap,
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
    do_print(state, headers, &printer_name, &print_data).await
}

pub(crate) async fn do_print(
    state: &HttpSharedState,
    headers: &HeaderMap,
    printer_name: &str,
    print_data: &str,
) -> Result<String, (StatusCode, String)> {
    if print_data.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            json!({ "error": "NO PRINT DATA PROVIDED" }).to_string(),
        ));
    }

    if let Err((status, msg)) = ensure_origin_allowed(state, headers).await {
        return Err((status, error_json(msg, Some(ORIGIN_DENIED))));
    }

    let printer = resolve_printer(state, printer_name).await.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            error_json("PRINTER NOT FOUND", Some(DEVICE_NOT_FOUND)),
        )
    })?;

    let permit = state.device_io.clone().try_acquire_owned().map_err(|_| {
        (
            StatusCode::TOO_MANY_REQUESTS,
            json!({ "error": "Too many print operations" }).to_string(),
        )
    })?;
    let printer_for_send = printer.clone();
    let payload = print_data.to_string();
    let result = tokio::task::spawn_blocking(move || {
        let _permit = permit;
        send_raw_to_printer(&printer_for_send, &payload)
    })
    .await
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            error_json(
                public_print_error(&e.to_string()),
                Some(PRINTER_UNREACHABLE),
            ),
        )
    })?;
    state
        .record_print("/", &printer, print_data.as_bytes(), result.clone())
        .await;
    result.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            error_json(public_print_error(&e), Some(PRINTER_UNREACHABLE)),
        )
    })?;

    Ok(json!({
        "printer": printer.display_name(),
        "data": print_data
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_server::test_util::{
        accept_one_payload, body_json, build_test_router, lan_printer, sample_printer,
        state_with_printer, test_state,
    };
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn simple_api_lists_printers() {
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")));
        let app = build_test_router(state);
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let list = body_json(response).await;
        assert!(list.is_array());
        assert_eq!(list[0]["Name"], "Printer 10.0.0.1");
    }

    #[tokio::test]
    async fn do_print_records_success_in_print_log() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(accept_one_payload(listener));

        let state = state_with_printer(lan_printer("127.0.0.1", addr.port()), true).await;
        let zpl = "^XA^FDHttpLog^FS^XZ";
        let body = do_print(&state, &HeaderMap::new(), "", zpl)
            .await
            .expect("print ok");
        assert!(body.contains("LanPrinter"));

        let received = accept.await.expect("join");
        assert_eq!(String::from_utf8_lossy(&received), zpl);

        let entries = state.print_log.list();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].route, "/");
        assert!(entries[0].ok);
        assert_eq!(entries[0].data, zpl.as_bytes());
        assert_eq!(entries[0].printer_address, "127.0.0.1");
    }

    #[tokio::test]
    async fn do_print_records_failure_in_print_log() {
        let state = state_with_printer(lan_printer("127.0.0.1", 1), true).await;
        let err = do_print(&state, &HeaderMap::new(), "", "^XA^XZ")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);

        let entries = state.print_log.list();
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].ok);
        assert!(entries[0].error.as_ref().is_some_and(|e| !e.is_empty()));
    }

    #[tokio::test]
    async fn do_print_skips_log_when_debug_disabled() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let accept = tokio::spawn(accept_one_payload(listener));

        let state = state_with_printer(lan_printer("127.0.0.1", addr.port()), false).await;
        do_print(&state, &HeaderMap::new(), "", "^XA^XZ")
            .await
            .expect("print ok");
        let _ = accept.await;
        assert!(state.print_log.list().is_empty());
    }

    #[tokio::test]
    async fn do_print_rejects_unapproved_origin() {
        let state = state_with_printer(lan_printer("127.0.0.1", 9100), true).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            axum::http::HeaderValue::from_static("https://evil.test"),
        );
        let err = do_print(&state, &headers, "", "^XA^XZ").await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("evil.test"));
        assert!(state
            .pending_origins
            .read()
            .await
            .contains_key("https://evil.test"));
    }

    #[tokio::test]
    async fn browser_image_get_without_origin_is_rejected() {
        let state = state_with_printer(lan_printer("127.0.0.1", 9100), true).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            "sec-fetch-mode",
            axum::http::HeaderValue::from_static("no-cors"),
        );
        headers.insert(
            "sec-fetch-dest",
            axum::http::HeaderValue::from_static("image"),
        );
        let err = do_print(&state, &headers, "", "^XA^XZ").await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(state.pending_origins.read().await.is_empty());
    }

    #[tokio::test]
    async fn unknown_printer_keeps_message_and_adds_device_not_found_code() {
        let state = state_with_printer(lan_printer("127.0.0.1", 9100), true).await;
        let err = do_print(&state, &HeaderMap::new(), "missing", "^XA^XZ")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        let body: serde_json::Value = serde_json::from_str(&err.1).unwrap();
        assert_eq!(body["error"], "PRINTER NOT FOUND");
        assert_eq!(body["code"], "DEVICE_NOT_FOUND");
    }

    #[tokio::test]
    async fn unapproved_origin_keeps_message_and_adds_origin_denied_code() {
        let state = state_with_printer(lan_printer("127.0.0.1", 9100), true).await;
        let mut headers = HeaderMap::new();
        headers.insert(
            header::ORIGIN,
            axum::http::HeaderValue::from_static("https://evil.test"),
        );
        let err = do_print(&state, &headers, "", "^XA^XZ").await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        let body: serde_json::Value = serde_json::from_str(&err.1).unwrap();
        assert_eq!(
            body["error"],
            "Website not approved for printing (https://evil.test). Open RawLabelPrint Settings to allow it, then try again."
        );
        assert_eq!(body["code"], "ORIGIN_DENIED");
    }

    #[tokio::test]
    async fn unreachable_printer_keeps_message_and_adds_code() {
        let state = state_with_printer(lan_printer("127.0.0.1", 1), true).await;
        let err = do_print(&state, &HeaderMap::new(), "", "^XA^XZ")
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::INTERNAL_SERVER_ERROR);
        let body: serde_json::Value = serde_json::from_str(&err.1).unwrap();
        assert_eq!(body["error"], "Could not reach the printer");
        assert_eq!(body["code"], "PRINTER_UNREACHABLE");
    }

    #[tokio::test]
    async fn method_not_allowed_has_no_error_code() {
        let state = test_state(true, None);
        let app = build_test_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .method("PUT")
                    .uri("/")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
        let body = body_json(response).await;
        assert_eq!(body["error"], "Method not allowed");
        assert!(body.get("code").is_none());
    }

    #[tokio::test]
    async fn virtual_print_is_logged_when_debug_disabled_and_appends_capture() {
        let dir =
            std::env::temp_dir().join(format!("rawlabelprint-http-virtual-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let capture = dir.join("job.zpl");
        let printer = crate::config::PrinterInfo {
            name: Some("Preview".into()),
            model: String::new(),
            firmware: String::new(),
            serial_number: String::new(),
            address: "Preview".into(),
            port: 0,
            print_port: 0,
            config_port: 0,
            connection: crate::config::Connection::Virtual,
            capture_path: Some(capture.display().to_string()),
        };
        let state = state_with_printer(printer, false).await;
        let zpl = "^XA^FDVirtual^FS^XZ";
        do_print(&state, &HeaderMap::new(), "", zpl)
            .await
            .expect("print ok");
        assert_eq!(state.print_log.list().len(), 1);
        assert!(state.print_log.list()[0].ok);
        assert_eq!(std::fs::read(&capture).unwrap(), zpl.as_bytes());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
