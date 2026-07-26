use std::sync::Mutex;
use std::time::Instant;
use tokio::sync::oneshot;

/// Shared application state held by the Axum server.
pub struct AppState {
    /// The expected Bearer token for authenticated endpoints.
    pub token: String,
    /// Instant the server started (for uptime calculation).
    pub start_time: Instant,
    /// The full `http://127.0.0.1:<port>` endpoint.
    pub endpoint: String,
    /// Server version string.
    pub server_version: String,
    /// One-shot sender used to trigger graceful shutdown.
    /// Taken once, after which further shutdown requests are no-ops.
    pub shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
}
