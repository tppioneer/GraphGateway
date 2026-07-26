use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use graphgateway_types::{API_VERSION, StatusResponse};
use std::sync::Arc;

use crate::state::AppState;

pub(super) async fn status(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<StatusResponse>) {
    let uptime = state.start_time.elapsed();
    let started_at = chrono::Utc::now()
        .checked_sub_signed(chrono::TimeDelta::from_std(uptime).unwrap_or_default())
        .map(|dt| dt.to_rfc3339())
        .unwrap_or_else(|| "unknown".to_string());

    (
        StatusCode::OK,
        Json(StatusResponse {
            server_version: state.server_version.clone(),
            api_version: API_VERSION.to_string(),
            pid: std::process::id(),
            started_at,
            uptime_ms: uptime.as_millis() as u64,
            endpoint: state.endpoint.clone(),
            mode: "owned-sidecar".to_string(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn status_no_auth_returns_401() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = Arc::new(AppState {
            token: "secret-token".into(),
            start_time: std::time::Instant::now(),
            endpoint: "http://127.0.0.1:9999".into(),
            server_version: "0.1.0".into(),
            shutdown_tx: std::sync::Mutex::new(Some(tx)),
        });

        let app = crate::build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/status")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_wrong_token_returns_401() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = Arc::new(AppState {
            token: "secret-token".into(),
            start_time: std::time::Instant::now(),
            endpoint: "http://127.0.0.1:9999".into(),
            server_version: "0.1.0".into(),
            shutdown_tx: std::sync::Mutex::new(Some(tx)),
        });

        let app = crate::build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/status")
                    .header("Authorization", "Bearer wrong-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn status_correct_token_returns_200() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = Arc::new(AppState {
            token: "secret-token".into(),
            start_time: std::time::Instant::now(),
            endpoint: "http://127.0.0.1:9999".into(),
            server_version: "0.1.0".into(),
            shutdown_tx: std::sync::Mutex::new(Some(tx)),
        });

        let app = crate::build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/status")
                    .header("Authorization", "Bearer secret-token")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 4096).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["mode"], "owned-sidecar");
        assert!(json["pid"].as_u64().is_some());
    }
}
