//! Stable domain error types.
//!
//! Every error carries a machine-readable `code`, a human-readable
//! `message`, and optional `details` / `cause` for diagnostics.
//! Serialization is snapshot-tested so downstream consumers can
//! depend on the wire format.
//!
//! See design §16 for the error-code catalogue.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// ErrorCode
// ---------------------------------------------------------------------------

/// Machine-stable error codes.
///
/// These codes are part of the Router's public contract.  Consumers
/// (IDE, CLI, CI) may branch on them; they must not change across
/// releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Workspace does not exist or is not accessible.
    WorkspaceNotFound,
    /// Specified Source does not exist in the active Workspace.
    SourceNotFound,
    /// Multiple Sources match the request; disambiguation required.
    SourceAmbiguous,
    /// The requested generation has not finished indexing.
    GenerationNotReady,
    /// Exact-consistency mode requires a generation that is missing or
    /// deleted.
    ExactSourceMissing,
    /// The freshest available generation exceeds the configured
    /// staleness limit.
    StaleLimitExceeded,
    /// A target node lacks a required tool capability.
    CapabilityUnavailable,
    /// A write operation was attempted on a read-only Source or
    /// Workspace.
    WriteScopeDenied,
    /// Group Bridge version vector does not match current member
    /// generations.
    BridgeVersionMismatch,
    /// A downstream MCP node is unreachable or returned a transport
    /// error.
    DownstreamUnavailable,
    /// One or more Sources returned results, but at least one Source
    /// is degraded or missing.
    PartialResult,
    /// Catch-all for forward compatibility.
    #[serde(other)]
    Unknown,
}

impl ErrorCode {
    /// Human-readable short label for logging / tracing.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WorkspaceNotFound => "WORKSPACE_NOT_FOUND",
            Self::SourceNotFound => "SOURCE_NOT_FOUND",
            Self::SourceAmbiguous => "SOURCE_AMBIGUOUS",
            Self::GenerationNotReady => "GENERATION_NOT_READY",
            Self::ExactSourceMissing => "EXACT_SOURCE_MISSING",
            Self::StaleLimitExceeded => "STALE_LIMIT_EXCEEDED",
            Self::CapabilityUnavailable => "CAPABILITY_UNAVAILABLE",
            Self::WriteScopeDenied => "WRITE_SCOPE_DENIED",
            Self::BridgeVersionMismatch => "BRIDGE_VERSION_MISMATCH",
            Self::DownstreamUnavailable => "DOWNSTREAM_UNAVAILABLE",
            Self::PartialResult => "PARTIAL_RESULT",
            Self::Unknown => "UNKNOWN",
        }
    }
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// DomainError
// ---------------------------------------------------------------------------

/// A stable, serializable domain error.
///
/// # Contract
///
/// - `code` is machine-readable and **must not** change across
///   releases for the same logical condition.
/// - `message` is a human-readable description (may change).
/// - `details` carries structured context (affected IDs, limits, etc.).
/// - `cause` is the originating error's display string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainError {
    /// Machine-stable error code.
    pub code: ErrorCode,
    /// Human-readable summary.
    pub message: String,
    /// Optional structured context (keys are snake_case).
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<HashMap<String, String>>,
    /// Optional originating error message.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cause: Option<String>,
}

impl DomainError {
    /// Create a new error with just a code and message.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
            cause: None,
        }
    }

    /// Attach structured details.
    pub fn with_details(mut self, details: HashMap<String, String>) -> Self {
        self.details = Some(details);
        self
    }

    /// Append a single detail key-value pair.
    pub fn with_detail(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let mut d = self.details.unwrap_or_default();
        d.insert(key.into(), value.into());
        self.details = Some(d);
        self
    }

    /// Set the originating cause.
    pub fn with_cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }
}

impl std::fmt::Display for DomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if let Some(ref cause) = self.cause {
            write!(f, " (caused by: {cause})")?;
        }
        Ok(())
    }
}

impl std::error::Error for DomainError {}

// ---------------------------------------------------------------------------
// Convenience constructors (one per ErrorCode)
// ---------------------------------------------------------------------------

impl DomainError {
    pub fn workspace_not_found(id: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::WorkspaceNotFound,
            format!("workspace '{}' not found", id.into()),
        )
    }

    pub fn source_not_found(id: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::SourceNotFound,
            format!("source '{}' not found", id.into()),
        )
    }

    pub fn source_ambiguous(ids: &[impl AsRef<str>]) -> Self {
        let list: Vec<&str> = ids.iter().map(|s| s.as_ref()).collect();
        Self::new(
            ErrorCode::SourceAmbiguous,
            format!("multiple sources match: {}", list.join(", ")),
        )
    }

    pub fn generation_not_ready(repo_id: impl Into<String>, branch: impl Into<String>) -> Self {
        let repo = repo_id.into();
        let br = branch.into();
        Self::new(
            ErrorCode::GenerationNotReady,
            format!("generation for {repo}/{br} is not ready"),
        )
        .with_detail("repo_id", repo)
        .with_detail("branch", br)
    }

    pub fn stale_limit_exceeded(max_ms: u64, actual_ms: u64) -> Self {
        Self::new(
            ErrorCode::StaleLimitExceeded,
            format!("staleness {actual_ms}ms exceeds limit of {max_ms}ms"),
        )
        .with_detail("max_ms", max_ms.to_string())
        .with_detail("actual_ms", actual_ms.to_string())
    }

    pub fn capability_unavailable(tool: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::CapabilityUnavailable,
            format!("required capability '{}' is unavailable", tool.into()),
        )
    }

    pub fn write_scope_denied(reason: impl Into<String>) -> Self {
        Self::new(ErrorCode::WriteScopeDenied, reason.into())
    }

    pub fn downstream_unavailable(endpoint: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::DownstreamUnavailable,
            format!("downstream node '{}' is unavailable", endpoint.into()),
        )
    }

    pub fn partial_result(msg: impl Into<String>) -> Self {
        Self::new(ErrorCode::PartialResult, msg.into())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- ErrorCode ----------------------------------------------------------

    #[test]
    fn error_code_as_str() {
        assert_eq!(ErrorCode::WorkspaceNotFound.as_str(), "WORKSPACE_NOT_FOUND");
        assert_eq!(ErrorCode::PartialResult.as_str(), "PARTIAL_RESULT");
    }

    #[test]
    fn error_code_display() {
        assert_eq!(format!("{}", ErrorCode::SourceNotFound), "SOURCE_NOT_FOUND");
    }

    #[test]
    fn error_code_serde_roundtrip() {
        for code in [
            ErrorCode::WorkspaceNotFound,
            ErrorCode::SourceNotFound,
            ErrorCode::SourceAmbiguous,
            ErrorCode::GenerationNotReady,
            ErrorCode::ExactSourceMissing,
            ErrorCode::StaleLimitExceeded,
            ErrorCode::CapabilityUnavailable,
            ErrorCode::WriteScopeDenied,
            ErrorCode::BridgeVersionMismatch,
            ErrorCode::DownstreamUnavailable,
            ErrorCode::PartialResult,
        ] {
            let json = serde_json::to_string(&code).unwrap();
            let code2: ErrorCode = serde_json::from_str(&json).unwrap();
            assert_eq!(code, code2, "round-trip failed for {code:?}");
        }
    }

    #[test]
    fn error_code_unknown_falls_back() {
        let v: ErrorCode = serde_json::from_str(r#""SOME_FUTURE_CODE""#).unwrap();
        assert_eq!(v, ErrorCode::Unknown);
    }

    // -- DomainError --------------------------------------------------------

    #[test]
    fn domain_error_simple() {
        let e = DomainError::workspace_not_found("ws-1");
        assert_eq!(e.code, ErrorCode::WorkspaceNotFound);
        assert!(e.message.contains("ws-1"));
        assert!(e.details.is_none());
        assert!(e.cause.is_none());
    }

    #[test]
    fn domain_error_with_details() {
        let e = DomainError::generation_not_ready("r", "b");
        let d = e.details.as_ref().unwrap();
        assert_eq!(d.get("repo_id").unwrap(), "r");
        assert_eq!(d.get("branch").unwrap(), "b");
    }

    #[test]
    fn domain_error_with_cause() {
        let e = DomainError::downstream_unavailable("http://x").with_cause("connection refused");
        assert_eq!(e.cause.as_deref(), Some("connection refused"));
    }

    #[test]
    fn domain_error_display() {
        let e = DomainError::new(ErrorCode::WriteScopeDenied, "baseline is read-only")
            .with_cause("role=baseline");
        let s = e.to_string();
        assert!(s.contains("WRITE_SCOPE_DENIED"));
        assert!(s.contains("baseline is read-only"));
        assert!(s.contains("role=baseline"));
    }

    // -- Snapshot tests (serialization contract) ----------------------------

    /// Helper: deserialise a JSON string back into a DomainError and
    /// assert both the field values and the round-trip.
    fn assert_error_snapshot(json: &str, expected_code: ErrorCode) {
        let err: DomainError = serde_json::from_str(json).expect("deserialise snapshot");
        assert_eq!(err.code, expected_code, "code mismatch in: {json}");
        // Round-trip: serialise and parse again.
        let json2 = serde_json::to_string_pretty(&err).unwrap();
        let err2: DomainError = serde_json::from_str(&json2).expect("round-trip deserialise");
        assert_eq!(err, err2, "round-trip mismatch");
    }

    #[test]
    fn snapshot_simple_error() {
        assert_error_snapshot(
            r#"{"code":"WORKSPACE_NOT_FOUND","message":"workspace 'ws-missing' not found","details":null,"cause":null}"#,
            ErrorCode::WorkspaceNotFound,
        );
        // Also verify the serialisation side (None fields are skipped).
        let e = DomainError::workspace_not_found("ws-missing");
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("\"code\":\"WORKSPACE_NOT_FOUND\""));
        assert!(json.contains("workspace 'ws-missing' not found"));
        // details/cause are None and therefore absent from output.
        assert!(!json.contains("\"details\""));
        assert!(!json.contains("\"cause\""));
    }

    #[test]
    fn snapshot_error_with_details() {
        let e = DomainError::stale_limit_exceeded(300_000, 450_000);
        let json = serde_json::to_string(&e).unwrap();
        // details is a HashMap — key order is non-deterministic.
        // Parse both sides to compare structurally.
        let parsed: DomainError = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.code, ErrorCode::StaleLimitExceeded);
        let d = parsed.details.as_ref().unwrap();
        assert_eq!(d.get("max_ms").unwrap(), "300000");
        assert_eq!(d.get("actual_ms").unwrap(), "450000");
    }

    #[test]
    fn snapshot_error_with_cause() {
        let e = DomainError::downstream_unavailable("https://graph.example.com/mcp")
            .with_cause("dns error: no such host");
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("DOWNSTREAM_UNAVAILABLE"));
        assert!(json.contains("dns error: no such host"));
        let round: DomainError = serde_json::from_str(&json).unwrap();
        assert_eq!(round.cause.as_deref(), Some("dns error: no such host"));
    }

    #[test]
    fn snapshot_error_partial_result() {
        let e = DomainError::partial_result(
            "2 of 3 sources returned results; account-remote is degraded",
        );
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("PARTIAL_RESULT"));
        assert!(json.contains("account-remote is degraded"));
        let round: DomainError = serde_json::from_str(&json).unwrap();
        assert_eq!(round.code, ErrorCode::PartialResult);
    }

    #[test]
    fn domain_error_is_std_error() {
        let e = DomainError::workspace_not_found("x");
        // The `impl Error` bound — just ensure it compiles.
        let _: &dyn std::error::Error = &e;
    }
}
