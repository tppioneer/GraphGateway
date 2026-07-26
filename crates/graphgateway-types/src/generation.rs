//! Immutable graph generation — the stable unit of versioning for queries.
//!
//! See design §3.2, §10–11.

use serde::{Deserialize, Serialize};

use crate::ids::GenerationId;

// ---------------------------------------------------------------------------
// GenerationStatus
// ---------------------------------------------------------------------------

/// Readiness of a generation for query use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationStatus {
    /// Indexing is in progress; queries should not be served.
    Building,
    /// Indexing completed successfully; queries may proceed.
    Ready,
    /// The generation is no longer the active one but remains available
    /// for in-flight read queries.
    Superseded,
    /// Indexing failed permanently.
    Failed,
    /// Catch-all for forward compatibility.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Generation
// ---------------------------------------------------------------------------

/// A resolved, immutable snapshot of a `(repo_id, branch)` pair.
///
/// Once resolved, a [`Generation`] ties a branch to a specific commit
/// (`head_sha`) and an opaque [`GenerationId`] assigned by the GitNexus
/// node.  The Router uses this to guarantee repeatable query results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Generation {
    /// Immutable generation identifier assigned by the indexing node.
    pub generation_id: GenerationId,
    /// Repository identifier.
    pub repo_id: String,
    /// Git branch or tag name.
    pub branch: String,
    /// Full commit SHA at the time of indexing.
    pub head_sha: String,
    /// Current readiness status.
    pub status: GenerationStatus,
}

// ---------------------------------------------------------------------------
// GenerationBinding
// ---------------------------------------------------------------------------

/// A pinned binding from a consumer package to a specific upstream
/// [`Generation`].
///
/// See design §10.3 (upstream dependency development).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerationBinding {
    /// Consumer repository or service name.
    pub consumer: String,
    /// Package identifier (e.g. Maven coordinate, npm package name).
    pub package: String,
    /// The Source that provides this dependency.
    pub provider_source: String,
    /// The pinned generation.
    pub generation_id: GenerationId,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_serde_roundtrip() {
        let g = Generation {
            generation_id: GenerationId::new("gen-17").unwrap(),
            repo_id: "payment-service".into(),
            branch: "feature/refund".into(),
            head_sha: "abc123def456".into(),
            status: GenerationStatus::Ready,
        };
        let json = serde_json::to_string_pretty(&g).unwrap();
        let g2: Generation = serde_json::from_str(&json).unwrap();
        assert_eq!(g, g2);
    }

    #[test]
    fn generation_status_unknown_falls_back() {
        let v: GenerationStatus = serde_json::from_str(r#""archived""#).unwrap();
        assert_eq!(v, GenerationStatus::Unknown);
    }

    #[test]
    fn binding_serde_roundtrip() {
        let b = GenerationBinding {
            consumer: "payment-service".into(),
            package: "com.example:account-sdk".into(),
            provider_source: "account-sdk-main".into(),
            generation_id: GenerationId::new("gen-42").unwrap(),
        };
        let json = serde_json::to_string(&b).unwrap();
        let b2: GenerationBinding = serde_json::from_str(&json).unwrap();
        assert_eq!(b, b2);
    }
}
