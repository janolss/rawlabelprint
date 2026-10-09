use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub(crate) fn json_ok(body: String) -> Response {
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

pub(crate) fn json_err(status: StatusCode, msg: impl Into<String>) -> Response {
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

pub(crate) fn text_ok(body: String) -> Response {
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

pub(crate) fn public_print_error(err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    if lower.contains("writ") {
        "Could not write to the printer".into()
    } else {
        "Could not reach the printer".into()
    }
}

pub(crate) fn empty_ok() -> Response {
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
