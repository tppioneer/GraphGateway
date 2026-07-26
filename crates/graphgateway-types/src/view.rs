//! Resolved Graph View — a point-in-time, immutable snapshot of a
//! Workspace suitable for executing queries.
//!
//! See design §3.4, §7, §8.

use serde::{Deserialize, Serialize};

use crate::ids::{GenerationId, SourceId, ViewId, WorkspaceId};

// ---------------------------------------------------------------------------
// ViewMember
// ---------------------------------------------------------------------------

/// One Source resolved to a concrete generation within a
/// [`ResolvedGraphView`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewMember {
    /// Source identifier within the parent Workspace.
    pub source_id: SourceId,
    /// Repository identifier.
    pub repo_id: String,
    /// Git branch or tag name.
    pub branch: String,
    /// Resolved, immutable generation identifier.
    pub generation_id: GenerationId,
    /// Full commit SHA at generation time.
    pub head_sha: String,
    /// Whether this member was unavailable during resolution.
    ///
    /// When `true`, `generation_id` and `head_sha` reflect the
    /// last-known-good values and query results for this member
    /// will be marked degraded.
    #[serde(default)]
    pub unavailable: bool,
}

// ---------------------------------------------------------------------------
// ResolvedGraphView
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of a [`Workspace`](crate::Workspace)
/// after all Sources have been resolved to concrete generations.
///
/// # Immutability contract
///
/// Once created, a [`ResolvedGraphView`] **must not** have its
/// `members` replaced, nor any individual member's `generation_id`
/// or `head_sha` altered.  Consumers that need a newer generation
/// must create a new view with a new [`ViewId`].
///
/// All query results referencing this view must carry the same
/// `view_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedGraphView {
    /// Unique identifier for this resolved view.
    pub view_id: ViewId,
    /// The Workspace that was resolved.
    pub workspace_id: WorkspaceId,
    /// Resolved members (one per Source).
    pub members: Vec<ViewMember>,
    /// ISO-8601 timestamp of when this view was created.
    pub resolved_at: String,
}

impl ResolvedGraphView {
    /// Look up a member by its [`SourceId`].
    pub fn member(&self, source_id: &SourceId) -> Option<&ViewMember> {
        self.members.iter().find(|m| &m.source_id == source_id)
    }

    /// Returns `true` when at least one member is marked unavailable.
    pub fn is_degraded(&self) -> bool {
        self.members.iter().any(|m| m.unavailable)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_view() -> ResolvedGraphView {
        ResolvedGraphView {
            view_id: ViewId::new("view-20260724-001").unwrap(),
            workspace_id: WorkspaceId::new("payment-feature").unwrap(),
            members: vec![
                ViewMember {
                    source_id: SourceId::new("payment-local").unwrap(),
                    repo_id: "payment-service".into(),
                    branch: "feature/refund".into(),
                    generation_id: GenerationId::new("gen-17").unwrap(),
                    head_sha: "abc123".into(),
                    unavailable: false,
                },
                ViewMember {
                    source_id: SourceId::new("account-remote").unwrap(),
                    repo_id: "account-service".into(),
                    branch: "main".into(),
                    generation_id: GenerationId::new("gen-42").unwrap(),
                    head_sha: "def456".into(),
                    unavailable: false,
                },
            ],
            resolved_at: "2026-07-24T10:30:00Z".into(),
        }
    }

    #[test]
    fn view_serde_roundtrip() {
        let view = sample_view();
        let json = serde_json::to_string_pretty(&view).unwrap();
        let view2: ResolvedGraphView = serde_json::from_str(&json).unwrap();
        assert_eq!(view, view2);
    }

    #[test]
    fn member_lookup() {
        let view = sample_view();
        let sid = SourceId::new("payment-local").unwrap();
        let m = view.member(&sid).unwrap();
        assert_eq!(m.repo_id, "payment-service");
    }

    #[test]
    fn member_lookup_missing() {
        let view = sample_view();
        let sid = SourceId::new("no-such-source").unwrap();
        assert!(view.member(&sid).is_none());
    }

    #[test]
    fn is_degraded_false() {
        assert!(!sample_view().is_degraded());
    }

    #[test]
    fn is_degraded_true() {
        let mut view = sample_view();
        view.members[1].unavailable = true;
        assert!(view.is_degraded());
    }

    #[test]
    fn view_member_unavailable_defaults_false() {
        let json = r#"{
            "source_id":"s1",
            "repo_id":"r",
            "branch":"b",
            "generation_id":"gen-1",
            "head_sha":"abc"
        }"#;
        let m: ViewMember = serde_json::from_str(json).unwrap();
        assert!(!m.unavailable);
    }

    #[test]
    fn view_matches_design_example() {
        // Verify the JSON shape matches §3.4 of the design document.
        let json = r#"{
            "view_id": "view-20260724-001",
            "workspace_id": "payment-feature",
            "members": [
                {
                    "source_id": "payment-local",
                    "repo_id": "payment-service",
                    "branch": "feature/refund",
                    "generation_id": "gen-17",
                    "head_sha": "abc123",
                    "unavailable": false
                },
                {
                    "source_id": "account-remote",
                    "repo_id": "account-service",
                    "branch": "main",
                    "generation_id": "gen-42",
                    "head_sha": "def456",
                    "unavailable": false
                }
            ],
            "resolved_at": "2026-07-24T10:30:00Z"
        }"#;
        let view: ResolvedGraphView = serde_json::from_str(json).unwrap();
        assert_eq!(view.members.len(), 2);
        assert_eq!(
            view.members[0].generation_id,
            GenerationId::new("gen-17").unwrap()
        );
    }
}
