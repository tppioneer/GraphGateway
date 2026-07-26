//! Provenance records attached to every Router response so that
//! consumers can trace which Source / generation produced each piece
//! of data.
//!
//! See design §12 (response model).

use serde::{Deserialize, Serialize};

use crate::ids::{GenerationId, SourceId};

// ---------------------------------------------------------------------------
// ProvenanceEntry
// ---------------------------------------------------------------------------

/// Metadata for one Source that contributed to a merged result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceEntry {
    /// Source identifier within the Workspace.
    pub source_id: SourceId,
    /// Repository identifier.
    pub repo_id: String,
    /// Git branch or tag name.
    pub branch: String,
    /// Resolved generation used for this Source's queries.
    pub generation_id: GenerationId,
    /// Full commit SHA.
    pub head_sha: String,
    /// Whether this Source is local or remote.
    pub backend: String,
    /// Milliseconds since the generation was completed (lower is
    /// fresher).
    pub freshness_ms: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provenance_entry_serde_roundtrip() {
        let entry = ProvenanceEntry {
            source_id: SourceId::new("payment-local").unwrap(),
            repo_id: "payment-service".into(),
            branch: "feature/refund".into(),
            generation_id: GenerationId::new("gen-17").unwrap(),
            head_sha: "abc123".into(),
            backend: "local".into(),
            freshness_ms: 1200,
        };
        let json = serde_json::to_string_pretty(&entry).unwrap();
        let e2: ProvenanceEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, e2);
    }

    #[test]
    fn provenance_entry_matches_design_example() {
        // §12 example provenance block
        let json = r#"{
            "source_id": "payment-local",
            "repo_id": "payment-service",
            "branch": "feature/refund",
            "generation_id": "gen-17",
            "head_sha": "abc123",
            "backend": "local",
            "freshness_ms": 1200
        }"#;
        let entry: ProvenanceEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.backend, "local");
        assert_eq!(entry.freshness_ms, 1200);
    }
}
