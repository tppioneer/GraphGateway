use axum::Json;
use axum::http::StatusCode;
use serde::Serialize;

/// Minimal health-check response.
///
/// Deliberately does NOT leak paths, tokens, Workspace names, or detailed
/// version information.
#[derive(Serialize)]
pub(super) struct HealthzResponse {
    status: &'static str,
}

pub(super) async fn healthz() -> (StatusCode, Json<HealthzResponse>) {
    (StatusCode::OK, Json(HealthzResponse { status: "ok" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use std::sync::Arc;
    use tower::ServiceExt;

    use crate::state::AppState;

    #[tokio::test]
    async fn healthz_returns_ok() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = Arc::new(AppState {
            token: "test-token".into(),
            start_time: std::time::Instant::now(),
            endpoint: "http://127.0.0.1:0".into(),
            server_version: "0.1.0".into(),
            shutdown_tx: std::sync::Mutex::new(Some(tx)),
        });

        let app = crate::build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["status"], "ok");
    }
}
