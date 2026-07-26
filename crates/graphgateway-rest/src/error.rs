//! Unified error types for the REST layer.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use graphgateway_types::{ApiErrorBody, ApiErrorDetail};

/// Unified API error type for the REST layer.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("authentication required")]
    Unauthorized,
    #[error("invalid access token")]
    Forbidden,
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    Internal(String),
}

impl ApiError {
    fn code(&self) -> &str {
        match self {
            Self::Unauthorized => "UNAUTHORIZED",
            Self::Forbidden => "FORBIDDEN",
            Self::BadRequest(_) => "BAD_REQUEST",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::Forbidden => StatusCode::FORBIDDEN,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let body = ApiErrorBody {
            error: ApiErrorDetail {
                code: self.code().to_string(),
                message: self.to_string(),
                trace_id: None,
            },
        };
        (status, Json(body)).into_response()
    }
}
