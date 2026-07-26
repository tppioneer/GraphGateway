use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Serialize;
use std::sync::Arc;

use crate::state::AppState;

#[derive(Serialize)]
pub(super) struct ShutdownResponse {
    message: &'static str,
}

pub(super) async fn shutdown(
    State(state): State<Arc<AppState>>,
) -> (StatusCode, Json<ShutdownResponse>) {
    let mut tx = state.shutdown_tx.lock().unwrap();
    if let Some(sender) = tx.take() {
        tracing::info!("shutdown requested via REST API");
        let _ = sender.send(());
        (
            StatusCode::OK,
            Json(ShutdownResponse {
                message: "shutting down",
            }),
        )
    } else {
        (
            StatusCode::OK,
            Json(ShutdownResponse {
                message: "shutdown already in progress",
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[tokio::test]
    async fn shutdown_without_auth_returns_401() {
        let (tx, _rx) = tokio::sync::oneshot::channel();
        let state = Arc::new(AppState {
            token: "secret".into(),
            start_time: std::time::Instant::now(),
            endpoint: "http://127.0.0.1:0".into(),
            server_version: "0.1.0".into(),
            shutdown_tx: std::sync::Mutex::new(Some(tx)),
        });

        let app = crate::build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/system/shutdown")
                    .method("POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn shutdown_with_auth_returns_200() {
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let state = Arc::new(AppState {
            token: "secret".into(),
            start_time: std::time::Instant::now(),
            endpoint: "http://127.0.0.1:0".into(),
            server_version: "0.1.0".into(),
            shutdown_tx: std::sync::Mutex::new(Some(tx)),
        });

        let app = crate::build_router(state);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/v1/system/shutdown")
                    .method("POST")
                    .header("Authorization", "Bearer secret")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        // The shutdown signal should have been sent
        assert!(rx.try_recv().is_ok());
    }
}
