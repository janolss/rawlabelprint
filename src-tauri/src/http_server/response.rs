use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use serde_json::json;

pub(crate) const ORIGIN_DENIED: &str = "ORIGIN_DENIED";
pub(crate) const DEVICE_NOT_FOUND: &str = "DEVICE_NOT_FOUND";
pub(crate) const PRINTER_UNREACHABLE: &str = "PRINTER_UNREACHABLE";
pub(crate) const COMPAT_DISABLED: &str = "COMPAT_DISABLED";

pub(crate) fn error_json(msg: impl Into<String>, code: Option<&str>) -> String {
    let msg = msg.into();
    match code {
        Some(code) => json!({ "error": msg, "code": code }).to_string(),
        None => json!({ "error": msg }).to_string(),
    }
}

pub(crate) fn json_ok(body: String) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

pub(crate) fn json_err(status: StatusCode, msg: impl Into<String>) -> Response {
    json_body(status, error_json(msg, None))
}

pub(crate) fn json_err_code(status: StatusCode, code: &str, msg: impl Into<String>) -> Response {
    json_body(status, error_json(msg, Some(code)))
}

fn json_body(status: StatusCode, body: String) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body,
    )
        .into_response()
}

pub(crate) fn text_ok(body: String) -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        body,
    )
        .into_response()
}

pub(crate) fn public_print_error(err: &str) -> String {
    let lower = err.to_ascii_lowercase();
    if lower.contains("delivery status is unknown") {
        "Printer write failed; delivery status is unknown. Check the printer before retrying."
            .into()
    } else if lower.contains("writ") {
        "Could not write to the printer".into()
    } else {
        "Could not reach the printer".into()
    }
}

pub(crate) fn empty_ok() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        String::new(),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_write_delivery_is_reported_explicitly() {
        assert!(
            public_print_error("Printer write failed; delivery status is unknown: error")
                .contains("Check the printer before retrying")
        );
    }
}
