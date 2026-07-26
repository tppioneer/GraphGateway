//! Authentication utilities for the REST API.
//!
//! Uses constant-time comparison via `graphgateway_core::constant_time_eq`
//! to avoid timing side-channel leakage of the access token.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use graphgateway_core::constant_time_eq;
use graphgateway_types::ApiErrorBody;
use std::sync::Arc;

use crate::state::AppState;

/// Validate a Bearer token against the expected value stored in [`AppState`].
///
/// Returns `Ok(())` on success, or an Axum `Response` (401/403) on failure.
/// Used by the auth middleware.
pub async fn require_auth(
    state: &Arc<AppState>,
    auth_header: Option<&str>,
) -> Result<(), Response> {
    let header = auth_header.ok_or_else(|| {
        let body = ApiErrorBody::new("UNAUTHORIZED", "missing Authorization header");
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    })?;

    let token = header.strip_prefix("Bearer ").ok_or_else(|| {
        let body = ApiErrorBody::new(
            "UNAUTHORIZED",
            "Authorization header must use Bearer scheme",
        );
        (StatusCode::UNAUTHORIZED, Json(body)).into_response()
    })?;

    let expected = state.token.as_bytes();
    let provided = token.as_bytes();

    if !constant_time_eq(expected, provided) {
        let body = ApiErrorBody::new("UNAUTHORIZED", "invalid access token");
        return Err((StatusCode::UNAUTHORIZED, Json(body)).into_response());
    }

    Ok(())
}

/// Validate a Bearer token string against the expected value.
///
/// Public so external consumers can reuse the same constant-time comparison.
pub fn validate_token(expected: &str, provided: &str) -> bool {
    constant_time_eq(expected.as_bytes(), provided.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_token_match() {
        assert!(validate_token("secret", "secret"));
    }

    #[test]
    fn validate_token_mismatch() {
        assert!(!validate_token("secret", "wrong"));
    }

    #[test]
    fn validate_token_empty() {
        assert!(!validate_token("secret", ""));
        assert!(!validate_token("", "secret"));
    }

    #[test]
    fn validate_token_case_sensitive() {
        assert!(!validate_token("Secret", "secret"));
    }
}
