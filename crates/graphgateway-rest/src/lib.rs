//! GraphGateway REST management API.
//!
//! Provides:
//! - Auth middleware (Bearer token, constant-time comparison)
//! - Route handlers for healthz, status, shutdown
//! - App state and router construction

mod auth;
mod error;
mod routes;
mod state;

pub use auth::validate_token;
pub use error::ApiError;
pub use routes::build_router;
pub use state::AppState;
