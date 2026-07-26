mod healthz;
mod shutdown;
mod status;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::middleware::{Next, from_fn_with_state};
use axum::response::Response;
use axum::routing::{get, post};
use std::sync::Arc;
use tower_http::limit::RequestBodyLimitLayer;

use crate::state::AppState;

/// Build the full REST router.
///
/// Routes:
/// - `GET  /healthz`             — no auth, no Host validation
/// - `GET  /api/v1/status`       — Bearer token required, Host validated
/// - `POST /api/v1/system/shutdown` — Bearer token required, Host validated
pub fn build_router(state: Arc<AppState>) -> Router {
    // Public routes (no auth, no Host validation — health check is minimal)
    let public = Router::new().route("/healthz", get(healthz::healthz));

    // Authenticated routes with Host validation
    let authenticated = Router::new()
        .route("/api/v1/status", get(status::status))
        .route("/api/v1/system/shutdown", post(shutdown::shutdown))
        .layer(from_fn_with_state(
            Arc::clone(&state),
            host_validation_middleware,
        ))
        .layer(from_fn_with_state(Arc::clone(&state), auth_middleware));

    Router::new()
        .merge(public)
        .merge(authenticated)
        .layer(RequestBodyLimitLayer::new(16 * 1024))
        .with_state(state)
}

/// Middleware that validates the Bearer token for authenticated routes.
async fn auth_middleware(
    state: axum::extract::State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let auth_header = req
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());

    if let Err(response) = crate::auth::require_auth(&state, auth_header).await {
        return response;
    }

    next.run(req).await
}

/// Middleware that validates the Host header points to the known loopback
/// endpoint.
async fn host_validation_middleware(
    state: axum::extract::State<Arc<AppState>>,
    req: Request<Body>,
    next: Next,
) -> Response {
    // Extract the expected host:port from our endpoint.
    // endpoint is like "http://127.0.0.1:38470"
    let expected_host = state
        .endpoint
        .strip_prefix("http://")
        .unwrap_or(&state.endpoint);

    // Check the Host header.
    if let Some(host) = req.headers().get(axum::http::header::HOST)
        && let Ok(host_str) = host.to_str()
        && host_str != expected_host
    {
        tracing::warn!(
            expected = %expected_host,
            received = %host_str,
            "Host header mismatch"
        );
        return Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("invalid Host header"))
            .unwrap();
    }

    // Origin check for browser-initiated requests.
    if let Some(origin) = req.headers().get(axum::http::header::ORIGIN)
        && let Ok(origin_str) = origin.to_str()
    {
        let allowed = origin_str == "tauri://localhost"
            || origin_str == "https://tauri.localhost"
            || origin_str.starts_with("http://localhost")
            || origin_str.starts_with("http://127.0.0.1");

        if !allowed {
            tracing::warn!(origin = %origin_str, "Origin rejected");
            return Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from("origin not allowed"))
                .unwrap();
        }
    }

    next.run(req).await
}
