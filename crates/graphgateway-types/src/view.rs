//! Resolved Graph View — a point-in-time, immutable snapshot of a
//! Workspace suitable for executing queries.
//!
//! See design §3.4, §7, §8.

use serde::{Deserialize, Serialize};

use crate::capability::CapabilitySnapshot;
use crate::ids::{GenerationId, SourceId, ViewId, WorkspaceId};
use crate::source::Endpoint;

// ---------------------------------------------------------------------------
// ViewMember
// ---------------------------------------------------------------------------

/// One Source resolved to a concrete generation, endpoint, and capability
/// snapshot within a [`ResolvedGraphView`].
///
/// # Immutability
///
/// All fields are private.  Once constructed, callers cannot replace the
/// generation, endpoint, or capability snapshot in place — a newer
/// generation requires creating a new [`ResolvedGraphView`] with a new
/// [`ViewId`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ViewMember {
    /// Source identifier within the parent Workspace.
    source_id: SourceId,
    /// Repository identifier.
    repo_id: String,
    /// Git branch or tag name.
    branch: String,
    /// Resolved, immutable generation identifier.
    generation_id: GenerationId,
    /// Full commit SHA at generation time.
    head_sha: String,
    /// Whether this member was unavailable during resolution.
    ///
    /// When `true`, `generation_id` and `head_sha` reflect the
    /// last-known-good values and query results for this member
    /// will be marked degraded.
    #[serde(default)]
    unavailable: bool,
    /// The resolved endpoint that was active when this member was
    /// captured.
    endpoint: Endpoint,
    /// The capability snapshot for this Source at resolution time.
    capability: CapabilitySnapshot,
}

impl ViewMember {
    /// Create a new [`ViewMember`] with validated required fields.
    ///
    /// The member is created as available (`unavailable = false`).
    /// Use [`ViewMember::new_unavailable`] to construct a member that was
    /// unavailable during resolution.
    ///
    /// # Errors
    ///
    /// Returns `Err` when `repo_id`, `branch`, or `head_sha` is empty or
    /// whitespace-only, or when `capability.source_id` does not match
    /// `source_id`.
    pub fn new(
        source_id: SourceId,
        repo_id: String,
        branch: String,
        generation_id: GenerationId,
        head_sha: String,
        endpoint: Endpoint,
        capability: CapabilitySnapshot,
    ) -> Result<Self, String> {
        Self::construct(
            source_id,
            repo_id,
            branch,
            generation_id,
            head_sha,
            endpoint,
            capability,
            false,
        )
    }

    /// Create a [`ViewMember`] that was unavailable during resolution.
    ///
    /// The member is marked `unavailable: true`.  Query results for this
    /// member will be marked degraded.  The `generation_id` and
    /// `head_sha` should reflect the last-known-good values.
    ///
    /// # Errors
    ///
    /// Returns `Err` for the same reasons as [`ViewMember::new`].
    pub fn new_unavailable(
        source_id: SourceId,
        repo_id: String,
        branch: String,
        generation_id: GenerationId,
        head_sha: String,
        endpoint: Endpoint,
        capability: CapabilitySnapshot,
    ) -> Result<Self, String> {
        Self::construct(
            source_id,
            repo_id,
            branch,
            generation_id,
            head_sha,
            endpoint,
            capability,
            true,
        )
    }

    /// Shared validation and construction.
    #[allow(clippy::too_many_arguments)]
    fn construct(
        source_id: SourceId,
        repo_id: String,
        branch: String,
        generation_id: GenerationId,
        head_sha: String,
        endpoint: Endpoint,
        capability: CapabilitySnapshot,
        unavailable: bool,
    ) -> Result<Self, String> {
        if repo_id.trim().is_empty() {
            return Err("repo_id must not be empty".into());
        }
        if branch.trim().is_empty() {
            return Err("branch must not be empty".into());
        }
        if head_sha.trim().is_empty() {
            return Err("head_sha must not be empty".into());
        }
        if capability.source_id != source_id {
            return Err(format!(
                "capability snapshot source_id '{}' does not match member source_id '{}'",
                capability.source_id, source_id,
            ));
        }
        Ok(Self {
            source_id,
            repo_id,
            branch,
            generation_id,
            head_sha,
            unavailable,
            endpoint,
            capability,
        })
    }

    // ------------------------------------------------------------------
    // Read-only accessors
    // ------------------------------------------------------------------

    /// Source identifier within the parent Workspace.
    pub fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    /// Repository identifier.
    pub fn repo_id(&self) -> &str {
        &self.repo_id
    }

    /// Git branch or tag name.
    pub fn branch(&self) -> &str {
        &self.branch
    }

    /// Resolved, immutable generation identifier.
    pub fn generation_id(&self) -> &GenerationId {
        &self.generation_id
    }

    /// Full commit SHA at generation time.
    pub fn head_sha(&self) -> &str {
        &self.head_sha
    }

    /// Whether this member was unavailable during resolution.
    pub fn unavailable(&self) -> bool {
        self.unavailable
    }

    /// The resolved endpoint active when this member was captured.
    pub fn endpoint(&self) -> &Endpoint {
        &self.endpoint
    }

    /// The capability snapshot for this Source at resolution time.
    pub fn capability(&self) -> &CapabilitySnapshot {
        &self.capability
    }
}

// ---------------------------------------------------------------------------
// ResolvedGraphView
// ---------------------------------------------------------------------------

/// A point-in-time snapshot of a [`Workspace`](crate::Workspace)
/// after all Sources have been resolved to concrete generations.
///
/// # Immutability contract
///
/// All fields are private.  Once created, a [`ResolvedGraphView`]
/// **must not** have its `members` replaced, nor any individual
/// member's `generation_id`, `endpoint`, or `capability` altered.
/// Consumers that need a newer generation must create a new view
/// with a new [`ViewId`].
///
/// All query results referencing this view must carry the same
/// `view_id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedGraphView {
    /// Unique identifier for this resolved view.
    view_id: ViewId,
    /// The Workspace that was resolved.
    workspace_id: WorkspaceId,
    /// Resolved members (one per Source).
    members: Vec<ViewMember>,
    /// ISO-8601 timestamp of when this view was created.
    resolved_at: String,
}

impl ResolvedGraphView {
    /// Create a new [`ResolvedGraphView`] with validated fields.
    ///
    /// # Errors
    ///
    /// Returns `Err` when `members` is empty, or when `resolved_at` is
    /// empty or whitespace-only.
    pub fn new(
        view_id: ViewId,
        workspace_id: WorkspaceId,
        members: Vec<ViewMember>,
        resolved_at: String,
    ) -> Result<Self, String> {
        if members.is_empty() {
            return Err("members must not be empty".into());
        }
        if resolved_at.trim().is_empty() {
            return Err("resolved_at must not be empty".into());
        }
        Ok(Self {
            view_id,
            workspace_id,
            members,
            resolved_at,
        })
    }

    // ------------------------------------------------------------------
    // Read-only accessors
    // ------------------------------------------------------------------

    /// Unique identifier for this resolved view.
    pub fn view_id(&self) -> &ViewId {
        &self.view_id
    }

    /// The Workspace that was resolved.
    pub fn workspace_id(&self) -> &WorkspaceId {
        &self.workspace_id
    }

    /// Resolved members (one per Source).
    ///
    /// The returned slice is read-only; callers cannot mutate members
    /// through this reference.
    pub fn members(&self) -> &[ViewMember] {
        &self.members
    }

    /// ISO-8601 timestamp of when this view was created.
    pub fn resolved_at(&self) -> &str {
        &self.resolved_at
    }

    /// Look up a member by its [`SourceId`].
    pub fn member(&self, source_id: &SourceId) -> Option<&ViewMember> {
        self.members.iter().find(|m| m.source_id() == source_id)
    }

    /// Returns `true` when at least one member is marked unavailable.
    pub fn is_degraded(&self) -> bool {
        self.members.iter().any(|m| m.unavailable())
    }

    /// Returns the number of members in this view.
    pub fn member_count(&self) -> usize {
        self.members.len()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::ToolCapability;
    use crate::source::{Adapter, Transport};

    fn sample_endpoint() -> Endpoint {
        Endpoint {
            url: "http://127.0.0.1:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        }
    }

    fn sample_capability(source_id: &str) -> CapabilitySnapshot {
        CapabilitySnapshot {
            source_id: SourceId::new(source_id).unwrap(),
            capabilities: vec![ToolCapability {
                name: "query".into(),
                version: Some("1.2".into()),
            }],
            captured_at: "2026-07-24T10:30:00Z".into(),
        }
    }

    fn sample_member(source_id: &str, gen_id: &str, head_sha: &str) -> ViewMember {
        ViewMember::new(
            SourceId::new(source_id).unwrap(),
            "payment-service".into(),
            "main".into(),
            GenerationId::new(gen_id).unwrap(),
            head_sha.into(),
            sample_endpoint(),
            sample_capability(source_id),
        )
        .unwrap()
    }

    fn sample_view() -> ResolvedGraphView {
        ResolvedGraphView::new(
            ViewId::new("view-20260724-001").unwrap(),
            WorkspaceId::new("payment-feature").unwrap(),
            vec![
                ViewMember::new(
                    SourceId::new("payment-local").unwrap(),
                    "payment-service".into(),
                    "feature/refund".into(),
                    GenerationId::new("gen-17").unwrap(),
                    "abc123".into(),
                    sample_endpoint(),
                    sample_capability("payment-local"),
                )
                .unwrap(),
                ViewMember::new(
                    SourceId::new("account-remote").unwrap(),
                    "account-service".into(),
                    "main".into(),
                    GenerationId::new("gen-42").unwrap(),
                    "def456".into(),
                    Endpoint {
                        url: "https://graph.example.com/mcp".into(),
                        transport: Transport::StreamableHttp,
                        adapter: Adapter::McpProxy,
                    },
                    sample_capability("account-remote"),
                )
                .unwrap(),
            ],
            "2026-07-24T10:30:00Z".into(),
        )
        .unwrap()
    }

    // -- Construction ---------------------------------------------------------

    #[test]
    fn view_member_new_rejects_empty_repo_id() {
        let result = ViewMember::new(
            SourceId::new("s1").unwrap(),
            "  ".into(),
            "b".into(),
            GenerationId::new("gen-1").unwrap(),
            "abc".into(),
            sample_endpoint(),
            sample_capability("s1"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn view_member_new_rejects_empty_branch() {
        let result = ViewMember::new(
            SourceId::new("s1").unwrap(),
            "r".into(),
            "".into(),
            GenerationId::new("gen-1").unwrap(),
            "abc".into(),
            sample_endpoint(),
            sample_capability("s1"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn view_member_new_rejects_empty_head_sha() {
        let result = ViewMember::new(
            SourceId::new("s1").unwrap(),
            "r".into(),
            "b".into(),
            GenerationId::new("gen-1").unwrap(),
            "  ".into(),
            sample_endpoint(),
            sample_capability("s1"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn view_member_new_rejects_mismatched_capability_source_id() {
        let result = ViewMember::new(
            SourceId::new("s1").unwrap(),
            "r".into(),
            "b".into(),
            GenerationId::new("gen-1").unwrap(),
            "abc".into(),
            sample_endpoint(),
            sample_capability("different-source"),
        );
        assert!(result.is_err());
    }

    #[test]
    fn resolved_view_new_rejects_empty_members() {
        let result = ResolvedGraphView::new(
            ViewId::new("v-1").unwrap(),
            WorkspaceId::new("ws-1").unwrap(),
            vec![],
            "now".into(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn resolved_view_new_rejects_empty_resolved_at() {
        let result = ResolvedGraphView::new(
            ViewId::new("v-1").unwrap(),
            WorkspaceId::new("ws-1").unwrap(),
            vec![sample_member("s1", "gen-1", "abc")],
            "  ".into(),
        );
        assert!(result.is_err());
    }

    // -- Accessors ------------------------------------------------------------

    #[test]
    fn view_member_accessors() {
        let m = sample_member("payment-local", "gen-17", "abc123");
        assert_eq!(m.source_id().as_ref(), "payment-local");
        assert_eq!(m.repo_id(), "payment-service");
        assert_eq!(m.branch(), "main");
        assert_eq!(m.generation_id().as_ref(), "gen-17");
        assert_eq!(m.head_sha(), "abc123");
        assert!(!m.unavailable());
        assert_eq!(m.endpoint().url, "http://127.0.0.1:38471/mcp");
        assert!(m.capability().supports("query"));
    }

    #[test]
    fn view_member_unavailable_constructor() {
        let m = ViewMember::new_unavailable(
            SourceId::new("s1").unwrap(),
            "r".into(),
            "b".into(),
            GenerationId::new("gen-1").unwrap(),
            "abc".into(),
            sample_endpoint(),
            sample_capability("s1"),
        )
        .unwrap();
        assert!(m.unavailable());
        assert_eq!(m.source_id().as_ref(), "s1");
        assert_eq!(m.generation_id().as_ref(), "gen-1");
        assert_eq!(m.head_sha(), "abc");
    }

    #[test]
    fn resolved_view_accessors() {
        let view = sample_view();
        assert_eq!(view.view_id().as_ref(), "view-20260724-001");
        assert_eq!(view.workspace_id().as_ref(), "payment-feature");
        assert_eq!(view.members().len(), 2);
        assert_eq!(view.member_count(), 2);
        assert_eq!(view.resolved_at(), "2026-07-24T10:30:00Z");
    }

    // -- Serde ----------------------------------------------------------------

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
        assert_eq!(m.repo_id(), "payment-service");
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
        // Construct a degraded view through the typed API — no JSON bypass
        // needed now that ViewMember::new_unavailable exists.
        let view = ResolvedGraphView::new(
            ViewId::new("view-degraded").unwrap(),
            WorkspaceId::new("ws-1").unwrap(),
            vec![
                ViewMember::new_unavailable(
                    SourceId::new("s1").unwrap(),
                    "r".into(),
                    "b".into(),
                    GenerationId::new("gen-1").unwrap(),
                    "abc".into(),
                    sample_endpoint(),
                    sample_capability("s1"),
                )
                .unwrap(),
            ],
            "now".into(),
        )
        .unwrap();
        assert!(view.is_degraded());
    }

    #[test]
    fn view_member_unavailable_defaults_false() {
        let json = r#"{
            "source_id":"s1",
            "repo_id":"r",
            "branch":"b",
            "generation_id":"gen-1",
            "head_sha":"abc",
            "endpoint":{"url":"http://127.0.0.1:1/mcp","transport":"streamable_http","adapter":"mcp_proxy"},
            "capability":{"source_id":"s1","capabilities":[],"captured_at":"now"}
        }"#;
        let m: ViewMember = serde_json::from_str(json).unwrap();
        assert!(!m.unavailable());
    }

    #[test]
    fn view_matches_design_example() {
        // Verify the JSON shape matches §3.4 of the design document,
        // extended with endpoint and capability snapshot per P103-R1.
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
                    "unavailable": false,
                    "endpoint": {"url":"http://127.0.0.1:38471/mcp","transport":"streamable_http","adapter":"mcp_proxy"},
                    "capability": {"source_id":"payment-local","capabilities":[{"name":"query","version":"1.2"}],"captured_at":"2026-07-24T10:30:00Z"}
                },
                {
                    "source_id": "account-remote",
                    "repo_id": "account-service",
                    "branch": "main",
                    "generation_id": "gen-42",
                    "head_sha": "def456",
                    "unavailable": false,
                    "endpoint": {"url":"https://graph.example.com/mcp","transport":"streamable_http","adapter":"mcp_proxy"},
                    "capability": {"source_id":"account-remote","capabilities":[{"name":"query","version":"1.2"}],"captured_at":"2026-07-24T10:30:00Z"}
                }
            ],
            "resolved_at": "2026-07-24T10:30:00Z"
        }"#;
        let view: ResolvedGraphView = serde_json::from_str(json).unwrap();
        assert_eq!(view.members().len(), 2);
        assert_eq!(
            view.members()[0].generation_id(),
            &GenerationId::new("gen-17").unwrap()
        );
        assert_eq!(
            view.members()[0].endpoint().url,
            "http://127.0.0.1:38471/mcp"
        );
        assert!(view.members()[0].capability().supports("query"));
    }

    #[test]
    fn view_member_endpoint_and_capability_bound_to_generation() {
        // P103-R1: each ViewMember carries its resolved endpoint and
        // capability snapshot for the same generation.  Verify that both
        // are accessible but cannot be mutated through the public API.
        let view = sample_view();
        let m = view
            .member(&SourceId::new("payment-local").unwrap())
            .unwrap();
        assert_eq!(m.generation_id().as_ref(), "gen-17");
        assert_eq!(m.endpoint().url, "http://127.0.0.1:38471/mcp");
        assert!(m.capability().supports("query"));
        // The returned references are immutable (&Endpoint, &CapabilitySnapshot);
        // mutation in place is prevented by the type system.
    }

    /// P103-R1 evidence: creating a newer generation requires a new view and
    /// ViewId — you cannot swap a generation_id on an existing view.
    #[test]
    fn newer_generation_requires_new_view() {
        let view1 = sample_view();
        let view_id1 = view1.view_id().clone();

        // Build a new view with a different generation — it gets a new ViewId.
        let view2 = ResolvedGraphView::new(
            ViewId::new("view-20260724-002").unwrap(),
            view1.workspace_id().clone(),
            vec![
                ViewMember::new(
                    SourceId::new("payment-local").unwrap(),
                    "payment-service".into(),
                    "feature/refund".into(),
                    GenerationId::new("gen-18").unwrap(), // newer generation
                    "abc124".into(),
                    sample_endpoint(),
                    sample_capability("payment-local"),
                )
                .unwrap(),
            ],
            "2026-07-24T11:00:00Z".into(),
        )
        .unwrap();

        // The original view is unchanged.
        assert_eq!(view1.view_id(), &view_id1);

        // The new view has a different generation and a different ViewId.
        assert_ne!(view1.view_id(), view2.view_id());
        assert_ne!(
            view1
                .member(&SourceId::new("payment-local").unwrap())
                .unwrap()
                .generation_id(),
            view2
                .member(&SourceId::new("payment-local").unwrap())
                .unwrap()
                .generation_id(),
        );
    }
}
