//! Soft allowlist for browser `Origin` on print requests.
//!
//! Requests without an Origin header (curl/scripts) are allowed.
//! Browser origins must be approved once in Settings before printing.

use crate::http_server::HttpSharedState;
use axum::http::{HeaderMap, StatusCode};

pub(crate) const DENIED_STATUS: StatusCode = StatusCode::FORBIDDEN;

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

/// Returns Ok(()) if printing is allowed. On first sight of a new origin, queues it for approval.
pub async fn ensure_origin_allowed(
    state: &HttpSharedState,
    headers: &HeaderMap,
) -> Result<(), (StatusCode, String)> {
    let Some(origin) = request_origin(headers) else {
        return Ok(());
    };

    {
        let cfg = state.config.read().await;
        if cfg.is_origin_allowed(&origin) {
            return Ok(());
        }
    }

    let newly_pending = {
        let mut pending = state.pending_origins.write().await;
        pending.insert(origin.clone())
    };
    if newly_pending {
        let _ = state.pending_tx.send(origin.clone());
    }

    Err((DENIED_STATUS, deny_message(&origin)))
}

pub async fn list_pending(state: &HttpSharedState) -> Vec<String> {
    state.pending_origins.read().await.iter().cloned().collect()
}

pub async fn dismiss_pending(state: &HttpSharedState, origin: &str) -> bool {
    let key = normalize_origin(origin);
    state.pending_origins.write().await.remove(&key)
}

/// Snapshot type used by Settings UI.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OriginPermissions {
    pub allowed: Vec<String>,
    pub pending: Vec<String>,
}

pub async fn permissions_snapshot(state: &HttpSharedState) -> OriginPermissions {
    let allowed = {
        let cfg = state.config.read().await;
        let mut list = cfg.allowed_origins.clone();
        list.sort();
        list
    };
    let pending = list_pending(state).await;
    OriginPermissions { allowed, pending }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http_server::test_util::test_state;
    use axum::http::HeaderValue;

    fn headers_with_origin(origin: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            axum::http::header::ORIGIN,
            HeaderValue::from_str(origin).unwrap(),
        );
        h
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
        let state = test_state(true, None, vec![]);
        assert!(ensure_origin_allowed(&state, &HeaderMap::new())
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn new_origin_is_denied_and_queued() {
        let state = test_state(true, None, vec![]);
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
        let state = test_state(true, None, vec![]);
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
        let state = test_state(true, None, vec![]);
        let h = headers_with_origin("https://a.test");
        let _ = ensure_origin_allowed(&state, &h).await;
        let _ = ensure_origin_allowed(&state, &h).await;
        assert_eq!(state.pending_origins.read().await.len(), 1);
    }

    #[test]
    fn request_origin_ignores_blank() {
        let mut h = HeaderMap::new();
        h.insert(axum::http::header::ORIGIN, HeaderValue::from_static("   "));
        assert!(request_origin(&h).is_none());
    }

    #[tokio::test]
    async fn permissions_snapshot_sorts_allowed() {
        let state = test_state(true, None, vec![]);
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
    }
}
