//! Graph Workspace — the logical query context for a task.
//!
//! See design §3.3, §10–11.

use serde::{Deserialize, Serialize};

use crate::ids::{SourceId, WorkspaceId};
use crate::source::Source;

// ---------------------------------------------------------------------------
// ConsistencyMode
// ---------------------------------------------------------------------------

/// How strictly the Router must match [`Generation`](crate::Generation)
/// versions across Sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsistencyMode {
    /// Exact `generation_id` binding — missing or stale → fail.
    /// Suitable for design review and fixed-dependency work.
    Exact,
    /// Prefer the latest completed local generation.  Falls back
    /// according to [`WaitPolicy`].
    NearRealTime,
    /// Each member independently selects its latest completed
    /// generation.  Partial results are permitted but must be
    /// clearly marked with provenance.
    Eventual,
    /// Catch-all for forward compatibility — treat as exact.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// WaitPolicy
// ---------------------------------------------------------------------------

/// What to do when the desired generation is not (yet) ready.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WaitPolicy {
    /// Block until a new generation completes.
    Wait,
    /// Use the most-recently-completed generation.
    #[default]
    UseLastComplete,
    /// Reject the request.
    Reject,
    /// Catch-all for forward compatibility.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// WritePolicy
// ---------------------------------------------------------------------------

/// Which Sources in a Workspace are allowed to receive write operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum WritePolicy {
    /// Only the local [`SourceRole::Primary`](crate::source::SourceRole)
    /// is writable.
    #[default]
    PrimaryOnly,
    /// All Sources are read-only (design review, historical analysis).
    AllReadOnly,
    /// Catch-all for forward compatibility.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Workspace
// ---------------------------------------------------------------------------

/// Graph Workspace — the logical query context for one task.
///
/// A [`Workspace`] owns a collection of [`Source`]s with distinct
/// roles.  The Router resolves it into a [`ResolvedGraphView`]
/// before executing any tool.
///
/// # Invariants (enforced by validation in `graphgateway-core`)
///
/// - `sources` must not be empty.
/// - Every source must have a unique [`SourceId`].
/// - `primary_source_id` must exist in `sources`.
/// - `max_staleness_ms` is only meaningful for
///   [`ConsistencyMode::NearRealTime`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Workspace {
    /// Unique identifier for this Workspace.
    pub workspace_id: WorkspaceId,
    /// Human-readable label (for UI / logs).
    pub name: String,
    /// All Sources belonging to this Workspace.
    pub sources: Vec<Source>,
    /// The Source designated as the primary (local development) entry.
    pub primary_source_id: SourceId,
    /// Default consistency mode for queries that don't override it.
    pub consistency: ConsistencyMode,
    /// Default write policy.
    #[serde(default)]
    pub write_policy: WritePolicy,
    /// Default behaviour when a generation is not ready.
    #[serde(default)]
    pub wait_policy: WaitPolicy,
    /// Maximum tolerated generation age in milliseconds.
    ///
    /// Relevant for [`ConsistencyMode::NearRealTime`]; ignored
    /// otherwise.  `None` means no staleness limit.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_staleness_ms: Option<u64>,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{Adapter, Endpoint, SourceKind, SourceRole, Transport};

    fn sample_endpoint() -> Endpoint {
        Endpoint {
            url: "http://127.0.0.1:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        }
    }

    fn sample_source(id: &str, role: SourceRole, local: bool) -> Source {
        Source {
            source_id: SourceId::new(id).unwrap(),
            repo_id: id.into(),
            branch: "main".into(),
            role,
            location: if local {
                SourceKind::Local
            } else {
                SourceKind::Remote
            },
            endpoint: sample_endpoint(),
            writable: local && role == SourceRole::Primary,
        }
    }

    #[test]
    fn workspace_serde_roundtrip() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("payment-feature").unwrap(),
            name: "Payment Feature Workspace".into(),
            sources: vec![
                sample_source("payment-local", SourceRole::Primary, true),
                sample_source("account-remote", SourceRole::Member, false),
            ],
            primary_source_id: SourceId::new("payment-local").unwrap(),
            consistency: ConsistencyMode::NearRealTime,
            write_policy: WritePolicy::PrimaryOnly,
            wait_policy: WaitPolicy::UseLastComplete,
            max_staleness_ms: Some(300_000),
        };
        let json = serde_json::to_string_pretty(&ws).unwrap();
        let ws2: Workspace = serde_json::from_str(&json).unwrap();
        assert_eq!(ws, ws2);
    }

    #[test]
    fn workspace_defaults() {
        let json = r#"{
            "workspace_id":"ws-1",
            "name":"test",
            "sources":[],
            "primary_source_id":"p",
            "consistency":"exact"
        }"#;
        let ws: Workspace = serde_json::from_str(json).unwrap();
        assert_eq!(ws.write_policy, WritePolicy::PrimaryOnly);
        assert_eq!(ws.wait_policy, WaitPolicy::UseLastComplete);
        assert!(ws.max_staleness_ms.is_none());
    }

    #[test]
    fn consistency_mode_unknown_falls_back() {
        let v: ConsistencyMode = serde_json::from_str(r#""strong""#).unwrap();
        assert_eq!(v, ConsistencyMode::Unknown);
    }

    #[test]
    fn wait_policy_unknown_falls_back() {
        let v: WaitPolicy = serde_json::from_str(r#""best-effort""#).unwrap();
        assert_eq!(v, WaitPolicy::Unknown);
    }

    #[test]
    fn write_policy_unknown_falls_back() {
        let v: WritePolicy = serde_json::from_str(r#""selective""#).unwrap();
        assert_eq!(v, WritePolicy::Unknown);
    }
}
