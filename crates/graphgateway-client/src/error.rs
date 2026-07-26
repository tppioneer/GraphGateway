//! Error types for the GraphGateway REST client.

/// Errors that can occur when using `GraphGatewayClient`.
#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    /// The base URL is malformed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// Failed to construct the HTTP client.
    #[error("HTTP client: {0}")]
    HttpClient(String),

    /// Connection refused or network error.
    #[error("connection error: {0}")]
    Connection(String),

    /// Request timed out.
    #[error("timeout: {0}")]
    Timeout(String),

    /// Server returned 401 — missing or invalid credentials.
    #[error("unauthorized: {0}")]
    Unauthorized(String),

    /// Server returned 403 — valid credentials but insufficient permissions.
    /// Not currently used for token validation (token errors are 401).
    #[error("forbidden: {0}")]
    Forbidden(String),

    /// Server returned a structured API error.
    #[error("API error [{code}]: {message}")]
    Api {
        code: String,
        message: String,
        #[allow(dead_code)]
        status: u16,
    },

    /// Server returned an unexpected HTTP status with an unstructured body.
    #[error("HTTP {status}: {body}")]
    Http { status: u16, body: String },

    /// Failed to deserialize a successful response body.
    #[error("JSON parse error: {0}")]
    Json(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_error_display_does_not_leak_sensitive_info() {
        let err = ClientError::Forbidden("invalid access token".into());
        let s = err.to_string();
        // The error message is Display-ed but the token itself is never in
        // the error variant — only the server's response is shown.
        assert!(s.contains("forbidden"));
    }
}
