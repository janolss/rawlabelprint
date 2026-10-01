use crate::http_server::resolve::{list_printers_json, resolve_printer};
use crate::http_server::response::json_ok;
use crate::http_server::HttpSharedState;
use crate::print::send_raw_to_printer;
use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, Method, StatusCode};
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

pub(crate) async fn do_print(
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

    let result = send_raw_to_printer(&printer, print_data);
    state
        .record_print("/", &printer, print_data.as_bytes(), result.clone())
        .await;
    result.map_err(|e| {
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
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")), vec![]);
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
        let body = do_print(&state, "", zpl).await.expect("print ok");
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
        let err = do_print(&state, "", "^XA^XZ").await.unwrap_err();
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
        do_print(&state, "", "^XA^XZ").await.expect("print ok");
        let _ = accept.await;
        assert!(state.print_log.list().is_empty());
    }
}
