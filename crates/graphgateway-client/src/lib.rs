//! Strongly-typed REST client for the GraphGateway management API.
//!
//! Used by the Tauri backend and CLI.  Not intended for WebView consumption.
//!
//! # Security
//!
//! - The Bearer token is stored internally and never appears in `Debug`
//!   or `Display` output.
//! - All requests use explicit timeouts.

mod error;

pub use error::ClientError;
use graphgateway_types::StatusResponse;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use std::time::Duration;

/// Default request timeout.
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// A strongly-typed client for the GraphGateway REST management API.
///
/// # Example
///
/// ```no_run
/// use graphgateway_client::GraphGatewayClient;
///
/// let client = GraphGatewayClient::new(
///     "http://127.0.0.1:12345",
///     "my-token",
/// ).unwrap();
///
/// // client.health().await?;
/// // client.status().await?;
/// ```
#[derive(Clone)]
pub struct GraphGatewayClient {
    inner: reqwest::Client,
    base_url: String,
    token: String,
}

impl GraphGatewayClient {
    /// Create a new client.
    ///
    /// `base_url` must be the root of the REST API, e.g.
    /// `http://127.0.0.1:38470`.  Do NOT include a trailing slash.
    pub fn new(base_url: impl Into<String>, token: impl Into<String>) -> Result<Self, ClientError> {
        let base_url = base_url.into();
        let token = token.into();

        // Validate the URL.
        let _parsed: reqwest::Url = base_url
            .parse()
            .map_err(|e| ClientError::InvalidUrl(format!("invalid base URL: {e}")))?;

        let inner = reqwest::Client::builder()
            .timeout(DEFAULT_TIMEOUT)
            .no_proxy() // never proxy localhost calls
            .build()
            .map_err(|e| ClientError::HttpClient(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            inner,
            base_url,
            token,
        })
    }

    /// Create a client with a custom request timeout.
    pub fn with_timeout(
        base_url: impl Into<String>,
        token: impl Into<String>,
        timeout: Duration,
    ) -> Result<Self, ClientError> {
        let base_url = base_url.into();
        let token = token.into();

        let _parsed: reqwest::Url = base_url
            .parse()
            .map_err(|e| ClientError::InvalidUrl(format!("invalid base URL: {e}")))?;

        let inner = reqwest::Client::builder()
            .timeout(timeout)
            .no_proxy()
            .build()
            .map_err(|e| ClientError::HttpClient(format!("failed to build HTTP client: {e}")))?;

        Ok(Self {
            inner,
            base_url,
            token,
        })
    }

    // ------------------------------------------------------------------
    // Public API
    // ------------------------------------------------------------------

    /// Check server health.
    ///
    /// Calls `GET /healthz` (no authentication required).
    pub async fn health(&self) -> Result<(), ClientError> {
        let url = format!("{}/healthz", self.base_url);
        let resp = self
            .inner
            .get(&url)
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(self.map_http_error(resp).await)
        }
    }

    /// Get server status.
    ///
    /// Calls `GET /api/v1/status` (Bearer token required).
    pub async fn status(&self) -> Result<StatusResponse, ClientError> {
        let url = format!("{}/api/v1/status", self.base_url);
        let resp = self
            .inner
            .get(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;

        if resp.status().is_success() {
            resp.json::<StatusResponse>()
                .await
                .map_err(|e| ClientError::Json(format!("failed to parse status response: {e}")))
        } else {
            Err(self.map_http_error(resp).await)
        }
    }

    /// Request graceful server shutdown.
    ///
    /// Calls `POST /api/v1/system/shutdown` (Bearer token required).
    pub async fn shutdown(&self) -> Result<(), ClientError> {
        let url = format!("{}/api/v1/system/shutdown", self.base_url);
        let resp = self
            .inner
            .post(&url)
            .headers(self.auth_headers())
            .send()
            .await
            .map_err(|e| self.map_reqwest_error(e))?;

        if resp.status().is_success() {
            Ok(())
        } else {
            Err(self.map_http_error(resp).await)
        }
    }

    // ------------------------------------------------------------------
    // Internals
    // ------------------------------------------------------------------

    fn auth_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        // Safe: the token only contains hex characters.
        if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.token)) {
            headers.insert(AUTHORIZATION, val);
        }
        headers
    }

    fn map_reqwest_error(&self, e: reqwest::Error) -> ClientError {
        if e.is_timeout() {
            ClientError::Timeout(format!("request timed out: {e}"))
        } else if e.is_connect() {
            ClientError::Connection(format!("connection failed: {e}"))
        } else {
            ClientError::HttpClient(format!("HTTP client error: {e}"))
        }
    }

    async fn map_http_error(&self, resp: reqwest::Response) -> ClientError {
        let status = resp.status();
        let body_text = resp.text().await.unwrap_or_default();

        // Try to parse the structured error body.
        if let Ok(body) = serde_json::from_str::<graphgateway_types::ApiErrorBody>(&body_text) {
            match body.error.code.as_str() {
                "UNAUTHORIZED" => ClientError::Unauthorized(body.error.message),
                "FORBIDDEN" => ClientError::Forbidden(body.error.message),
                code => ClientError::Api {
                    code: code.to_string(),
                    message: body.error.message,
                    status: status.as_u16(),
                },
            }
        } else {
            ClientError::Http {
                status: status.as_u16(),
                body: body_text,
            }
        }
    }
}

// Token is deliberately excluded from Debug output.
impl std::fmt::Debug for GraphGatewayClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GraphGatewayClient")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_debug_redacts_token() {
        let c = GraphGatewayClient::new("http://127.0.0.1:12345", "secret-token").unwrap();
        let debug = format!("{c:?}");
        assert!(!debug.contains("secret-token"));
        assert!(debug.contains("<redacted>"));
    }

    #[test]
    fn client_invalid_url() {
        assert!(GraphGatewayClient::new("not-a-url", "token").is_err());
    }

    #[test]
    fn client_valid_url() {
        assert!(GraphGatewayClient::new("http://127.0.0.1:9999", "token").is_ok());
    }
}
