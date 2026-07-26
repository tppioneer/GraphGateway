//! Shared types for the GraphGateway system.
//!
//! This crate has zero dependencies on transport, storage, or Tauri.
//! All types are pure data with serde support.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Public modules (domain contracts — P1-03)
// ---------------------------------------------------------------------------

pub mod capability;
pub mod generation;
pub mod ids;
pub mod provenance;
pub mod source;
pub mod view;
pub mod workspace;

// Re-export every public symbol so callers can `use graphgateway_types::*`
// or refer to a single module.
pub use capability::*;
pub use generation::*;
pub use ids::*;
pub use provenance::*;
pub use source::*;
pub use view::*;
pub use workspace::*;

// ---------------------------------------------------------------------------
// Protocol / API constants
// ---------------------------------------------------------------------------

/// The current sidecar startup protocol version.
pub const PROTOCOL_VERSION: u32 = 1;

/// The REST API version string.
pub const API_VERSION: &str = "v1";

// ---------------------------------------------------------------------------
// Sidecar startup protocol types
// ---------------------------------------------------------------------------

/// Configuration sent by the Tauri backend to the Sidecar via stdin
/// at process start.  One JSON object, one line.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartupConfig {
    pub protocol_version: u32,
    pub access_token: String,
    pub parent_pid: u32,
    pub data_dir: String,
}

/// Ready message emitted by the Sidecar on stdout once the HTTP server
/// is listening.  Exactly one line of JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReadyMessage {
    #[serde(rename = "type")]
    pub msg_type: String,
    pub protocol_version: u32,
    pub pid: u32,
    pub endpoint: String,
    pub api_version: String,
    pub server_version: String,
}

impl ReadyMessage {
    /// Validate that `msg_type` is `"ready"` and the protocol version matches.
    pub fn validate(&self, expected_protocol: u32) -> Result<(), String> {
        if self.msg_type != "ready" {
            return Err(format!("unexpected ready message type: {}", self.msg_type));
        }
        if self.protocol_version != expected_protocol {
            return Err(format!(
                "protocol version mismatch: expected {}, got {}",
                expected_protocol, self.protocol_version
            ));
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// REST API types
// ---------------------------------------------------------------------------

/// Response body for `GET /api/v1/status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusResponse {
    pub server_version: String,
    pub api_version: String,
    pub pid: u32,
    pub started_at: String,
    pub uptime_ms: u64,
    pub endpoint: String,
    pub mode: String,
}

/// Body for `POST /api/v1/system/shutdown`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShutdownRequest {
    /// Optional reason / audit note.
    pub reason: Option<String>,
}

/// A stable error envelope returned by the REST API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

impl ApiErrorBody {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorDetail {
                code: code.into(),
                message: message.into(),
                trace_id: None,
            },
        }
    }

    pub fn with_trace(mut self, trace_id: impl Into<String>) -> Self {
        self.error.trace_id = Some(trace_id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Sidecar lifecycle state (used by Tauri → frontend)
// ---------------------------------------------------------------------------

/// Sidecar lifecycle state reported to the frontend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SidecarState {
    Stopped,
    Starting,
    Ready,
    Stopping,
    Failed,
}

impl SidecarState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stopped => "stopped",
            Self::Starting => "starting",
            Self::Ready => "ready",
            Self::Stopping => "stopping",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for SidecarState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Snapshot of what the Tauri backend knows about the sidecar.
/// This is returned to the frontend via `sidecar_status`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SidecarSnapshot {
    pub state: SidecarState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub uptime_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    /// Desktop product version (from the Tauri app itself).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desktop_version: Option<String>,
}

// ---------------------------------------------------------------------------
// Tests (pre-existing)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ready_message_validation() {
        let msg = ReadyMessage {
            msg_type: "ready".into(),
            protocol_version: 1,
            pid: 1234,
            endpoint: "http://127.0.0.1:9999".into(),
            api_version: "v1".into(),
            server_version: "0.1.0".into(),
        };
        assert!(msg.validate(1).is_ok());
    }

    #[test]
    fn ready_message_bad_type() {
        let msg = ReadyMessage {
            msg_type: "error".into(),
            protocol_version: 1,
            pid: 1234,
            endpoint: "http://127.0.0.1:9999".into(),
            api_version: "v1".into(),
            server_version: "0.1.0".into(),
        };
        assert!(msg.validate(1).is_err());
    }

    #[test]
    fn ready_message_wrong_protocol() {
        let msg = ReadyMessage {
            msg_type: "ready".into(),
            protocol_version: 2,
            pid: 1234,
            endpoint: "http://127.0.0.1:9999".into(),
            api_version: "v1".into(),
            server_version: "0.1.0".into(),
        };
        assert!(msg.validate(1).is_err());
    }

    #[test]
    fn startup_config_deny_unknown() {
        let json = r#"{"protocol_version":1,"access_token":"t","parent_pid":1,"data_dir":"/d","extra":42}"#;
        assert!(serde_json::from_str::<StartupConfig>(json).is_err());
    }

    #[test]
    fn startup_config_roundtrip() {
        let cfg = StartupConfig {
            protocol_version: 1,
            access_token: "secret".into(),
            parent_pid: 42,
            data_dir: "/tmp/gw".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let cfg2: StartupConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(cfg2.protocol_version, 1);
        assert_eq!(cfg2.access_token, "secret");
    }
}
