//! Soft allowlist for browser `Origin` on print and device requests.
//!
//! Authorization is the allow/deny list (and pending approval), not CORS.
//! `origin_cors_middleware` enforces the allowlist for every route and mirrors
//! the request `Origin` into `Access-Control-Allow-Origin` (never `*`) so
//! browsers can read success and 403 bodies.
//!
//! Requests without an Origin header and without browser `Sec-Fetch-*` metadata
//! (curl/scripts) are allowed. Browser origins must be approved once in Settings.

use crate::http_server::response::{json_err_code, ORIGIN_DENIED};
use crate::http_server::HttpSharedState;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::time::{Duration, Instant};

pub(crate) const DENIED_STATUS: StatusCode = StatusCode::FORBIDDEN;
const MAX_PENDING_ORIGINS: usize = 128;
const PENDING_ORIGIN_TTL: Duration = Duration::from_secs(10 * 60);

/// Normalize Origin for storage/compare (trim + lowercase).
pub fn normalize_origin(raw: &str) -> String {
    raw.trim().to_ascii_lowercase()
}

/// Extract Origin from headers. Empty / missing → None (non-browser clients).
pub fn request_origin(headers: &HeaderMap) -> Option<String> {
    let value = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    Some(normalize_origin(value))
}

pub fn deny_message(origin: &str) -> String {
    format!(
        "Website not approved for printing ({origin}). Open RawLabelPrint Settings to allow it, then try again."
    )
}

pub fn denied_message(origin: &str) -> String {
    format!("Website is not allowed to print ({origin}).")
}

fn browser_fetch_metadata(headers: &HeaderMap) -> bool {
    headers.contains_key("sec-fetch-site")
        || headers.contains_key("sec-fetch-mode")
        || headers.contains_key("sec-fetch-dest")
}

/// Origin string used to isolate device sessions. Empty when the client sent none.
pub fn session_origin(headers: &HeaderMap) -> String {
    request_origin(headers).unwrap_or_default()
}

/// Reflect request Origin for CORS; omit when the client sent none (curl/scripts).
fn apply_cors_headers(headers: &mut HeaderMap, origin: Option<&str>) {
    let Some(origin) = origin else {
        return;
    };
    let Ok(value) = HeaderValue::from_str(origin) else {
        return;
    };
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    headers.insert(header::VARY, HeaderValue::from_static("Origin"));
}

fn preflight_response(req_headers: &HeaderMap, origin: Option<&str>) -> Response {
    let mut response = StatusCode::NO_CONTENT.into_response();
    let headers = response.headers_mut();
    apply_cors_headers(headers, origin);
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    let allow_headers = req_headers
        .get(header::ACCESS_CONTROL_REQUEST_HEADERS)
        .cloned()
        .unwrap_or_else(|| HeaderValue::from_static("content-type"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, allow_headers);
    response
}

/// Central origin gate + CORS for all HTTP routes. OPTIONS skips the allowlist
/// queue so preflight does not spam Settings; GET/POST still enqueue pending.
pub async fn origin_cors_middleware(
    State(state): State<HttpSharedState>,
    req: Request,
    next: Next,
) -> Response {
    let headers = req.headers().clone();
    let method = req.method().clone();
    let origin = request_origin(&headers);

    if method == Method::OPTIONS {
        return preflight_response(&headers, origin.as_deref());
    }

    if let Err((status, msg)) = ensure_origin_allowed(&state, &headers).await {
        let mut response = json_err_code(status, ORIGIN_DENIED, msg);
        apply_cors_headers(response.headers_mut(), origin.as_deref());
        return response;
    }

    let mut response = next.run(req).await;
    apply_cors_headers(response.headers_mut(), origin.as_deref());
    response
}

/// Returns Ok(()) if the request may use printers.
/// A new browser origin is queued for approval. A denied origin is not queued again.
pub async fn ensure_origin_allowed(
    state: &HttpSharedState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let Some(origin) = request_origin(headers) else {
        if browser_fetch_metadata(headers) {
            return Err((
                DENIED_STATUS,
                "Browser request is missing an Origin header.".into(),
            ));
        }
        return Ok(());
    };

    {
        let cfg = state.config.read().await;
        if cfg.is_origin_denied(&origin) {
            return Err((DENIED_STATUS, denied_message(&origin)));
        }
        if cfg.is_origin_allowed(&origin) {
            return Ok(());
        }
    }

    let newly_pending = {
        let mut pending = state.pending_origins.write().await;
        prune_expired_pending(&mut pending, Instant::now());
        if pending.contains_key(&origin) || pending.len() >= MAX_PENDING_ORIGINS {
            false
        } else {
            pending.insert(origin.clone(), Instant::now());
            true
        }
    };
    if newly_pending {
        let _ = state.pending_tx.send(origin.clone());
    }

    Err((DENIED_STATUS, deny_message(&origin)))
}

pub async fn list_pending(state: &HttpSharedState) -> Vec<String> {
    let mut pending = state.pending_origins.write().await;
    prune_expired_pending(&mut pending, Instant::now());
    pending.keys().cloned().collect()
}

fn prune_expired_pending(pending: &mut crate::http_server::PendingOrigins, now: Instant) {
    pending.retain(|_, created| now.duration_since(*created) < PENDING_ORIGIN_TTL);
}

pub async fn dismiss_pending(state: &HttpSharedState, origin: &str) -> bool {
    let key = normalize_origin(origin);
    state.pending_origins.write().await.remove(&key).is_some()
}

/// Snapshot type used by Settings UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginPermissions {
    pub allowed: Vec<String>,
    pub pending: Vec<String>,
    pub denied: Vec<String>,
}

pub async fn permissions_snapshot(state: &HttpSharedState) -> OriginPermissions {
    let (mut allowed, mut denied) = {
        let cfg = state.config.read().await;
        (cfg.allowed_origins.clone(), cfg.denied_origins.clone())
    };
    allowed.sort();
    denied.sort();
    let pending = list_pending(state).await;
    OriginPermissions {
        allowed,
        pending,
        denied,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_server::test_util::{body_json, build_test_router, sample_printer, test_state};
    use axum::body::Body;
    use axum::http::{HeaderValue, Request};
    use tower::ServiceExt;

    fn headers_with_origin(origin: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            axum::http::header::ORIGIN,
            HeaderValue::from_str(origin).unwrap(),
        );
        h
    }

    fn acao(response: &axum::response::Response) -> Option<&str> {
        response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .and_then(|v| v.to_str().ok())
    }

    fn assert_never_star(response: &axum::response::Response) {
        if let Some(value) = acao(response) {
            assert_ne!(value, "*");
        }
    }

    #[test]
    fn normalize_lowercases() {
        assert_eq!(
            normalize_origin(" https://Hotel.Example.COM "),
            "https://hotel.example.com"
        );
    }

    #[tokio::test]
    async fn missing_origin_is_allowed() {
        let state = test_state(true, None);
        assert!(ensure_origin_allowed(&state, &HeaderMap::new())
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn new_origin_is_denied_and_queued() {
        let state = test_state(true, None);
        let err = ensure_origin_allowed(&state, &headers_with_origin("https://app.example"))
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(err.1.contains("https://app.example"));
        let pending = list_pending(&state).await;
        assert_eq!(pending, vec!["https://app.example".to_string()]);
    }

    #[tokio::test]
    async fn allowed_origin_passes() {
        let state = test_state(true, None);
        {
            let mut cfg = state.config.write().await;
            cfg.allow_origin("https://app.example");
        }
        assert!(
            ensure_origin_allowed(&state, &headers_with_origin("https://APP.example"))
                .await
                .is_ok()
        );
        assert!(list_pending(&state).await.is_empty());
    }

    #[tokio::test]
    async fn duplicate_pending_does_not_grow() {
        let state = test_state(true, None);
        let h = headers_with_origin("https://a.test");
        let _ = ensure_origin_allowed(&state, &h).await;
        let _ = ensure_origin_allowed(&state, &h).await;
        assert_eq!(state.pending_origins.read().await.len(), 1);
    }

    #[tokio::test]
    async fn pending_origin_limit_does_not_stop_denial() {
        let state = test_state(true, None);
        for index in 0..MAX_PENDING_ORIGINS {
            let origin = format!("https://{index}.test");
            assert!(ensure_origin_allowed(&state, &headers_with_origin(&origin))
                .await
                .is_err());
        }

        let overflow = "https://overflow.test";
        assert!(
            ensure_origin_allowed(&state, &headers_with_origin(overflow))
                .await
                .is_err()
        );
        let pending = state.pending_origins.read().await;
        assert_eq!(pending.len(), MAX_PENDING_ORIGINS);
        assert!(!pending.contains_key(overflow));
    }

    #[test]
    fn expired_pending_origins_are_pruned() {
        let created = Instant::now();
        let current = created + Duration::from_secs(30);
        let mut pending = crate::http_server::PendingOrigins::new();
        pending.insert("https://expired.test".into(), created);
        pending.insert("https://current.test".into(), current);

        prune_expired_pending(
            &mut pending,
            created + PENDING_ORIGIN_TTL + Duration::from_secs(1),
        );

        assert_eq!(
            pending.keys().map(String::as_str).collect::<Vec<_>>(),
            vec!["https://current.test"]
        );
    }

    #[test]
    fn request_origin_ignores_blank() {
        let mut h = HeaderMap::new();
        h.insert(axum::http::header::ORIGIN, HeaderValue::from_static("   "));
        assert!(request_origin(&h).is_none());
    }

    #[tokio::test]
    async fn permissions_snapshot_sorts_allowed() {
        let state = test_state(true, None);
        {
            let mut cfg = state.config.write().await;
            cfg.allow_origin("https://b.test");
            cfg.allow_origin("https://a.test");
        }
        let _ = ensure_origin_allowed(&state, &headers_with_origin("https://c.test")).await;
        let snap = permissions_snapshot(&state).await;
        assert_eq!(
            snap.allowed,
            vec!["https://a.test".to_string(), "https://b.test".to_string()]
        );
        assert_eq!(snap.pending, vec!["https://c.test".to_string()]);
        assert!(snap.denied.is_empty());
    }

    #[tokio::test]
    async fn browser_metadata_without_origin_is_denied() {
        let state = test_state(true, None);
        let mut headers = HeaderMap::new();
        headers.insert("sec-fetch-mode", HeaderValue::from_static("no-cors"));
        headers.insert("sec-fetch-dest", HeaderValue::from_static("image"));
        let err = ensure_origin_allowed(&state, &headers).await.unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(list_pending(&state).await.is_empty());
    }

    #[tokio::test]
    async fn denied_origin_is_not_queued() {
        let state = test_state(true, None);
        {
            let mut cfg = state.config.write().await;
            cfg.deny_origin("https://blocked.test");
        }
        let err = ensure_origin_allowed(&state, &headers_with_origin("https://blocked.test"))
            .await
            .unwrap_err();
        assert_eq!(err.0, StatusCode::FORBIDDEN);
        assert!(list_pending(&state).await.is_empty());
        let snap = permissions_snapshot(&state).await;
        assert_eq!(snap.denied, vec!["https://blocked.test".to_string()]);
    }

    #[tokio::test]
    async fn middleware_allowed_origin_gets_exact_acao() {
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")));
        {
            let mut cfg = state.config.write().await;
            cfg.allow_origin("https://app.example");
        }
        let app = build_test_router(state);
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(header::ORIGIN, "https://app.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(acao(&response), Some("https://app.example"));
        assert_eq!(
            response
                .headers()
                .get(header::VARY)
                .and_then(|v| v.to_str().ok()),
            Some("Origin")
        );
        assert_never_star(&response);
    }

    #[tokio::test]
    async fn middleware_new_origin_is_forbidden_with_mirrored_acao() {
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")));
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header(header::ORIGIN, "https://new.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(acao(&response), Some("https://new.example"));
        assert_never_star(&response);
        let body = body_json(response).await;
        assert_eq!(body["code"], "ORIGIN_DENIED");
        assert_eq!(
            list_pending(&state).await,
            vec!["https://new.example".to_string()]
        );
    }

    #[tokio::test]
    async fn middleware_denied_origin_is_forbidden_without_pending() {
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")));
        {
            let mut cfg = state.config.write().await;
            cfg.deny_origin("https://blocked.example");
        }
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/available")
                    .header(header::ORIGIN, "https://blocked.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(acao(&response), Some("https://blocked.example"));
        assert_never_star(&response);
        assert!(list_pending(&state).await.is_empty());
    }

    #[tokio::test]
    async fn middleware_curl_without_origin_has_no_acao() {
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")));
        let app = build_test_router(state);
        let response = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(response
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }

    #[tokio::test]
    async fn middleware_browser_metadata_without_origin_is_forbidden() {
        let state = test_state(true, None);
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/")
                    .header("sec-fetch-mode", "no-cors")
                    .header("sec-fetch-dest", "image")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert!(list_pending(&state).await.is_empty());
        assert_never_star(&response);
    }

    #[tokio::test]
    async fn middleware_preflight_does_not_queue_pending() {
        let state = test_state(true, None);
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .method("OPTIONS")
                    .uri("/")
                    .header(header::ORIGIN, "https://preflight.example")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "POST")
                    .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "content-type")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NO_CONTENT);
        assert_eq!(acao(&response), Some("https://preflight.example"));
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_METHODS)
                .and_then(|v| v.to_str().ok()),
            Some("GET, POST, OPTIONS")
        );
        assert_eq!(
            response
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
                .and_then(|v| v.to_str().ok()),
            Some("content-type")
        );
        assert_never_star(&response);
        assert!(list_pending(&state).await.is_empty());
    }

    #[tokio::test]
    async fn middleware_config_requires_approved_origin() {
        let state = test_state(true, Some(sample_printer("10.0.0.1", "SN1")));
        let app = build_test_router(state.clone());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/config")
                    .header(header::ORIGIN, "https://config.example")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_eq!(acao(&response), Some("https://config.example"));
        assert_never_star(&response);
        let body = body_json(response).await;
        assert_eq!(body["code"], "ORIGIN_DENIED");
        assert_eq!(
            list_pending(&state).await,
            vec!["https://config.example".to_string()]
        );
    }
}
