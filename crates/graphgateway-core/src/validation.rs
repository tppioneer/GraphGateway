//! Domain validation functions.
//!
//! These enforce the invariants listed in the task card and design:
//!
//! - IDs must be non-empty.
//! - Sources must have consistent local/remote configuration.
//! - Workspace sources must have unique IDs.
//! - A [`ResolvedGraphView`] must not be missing generation info.
//!
//! All validators return [`DomainError`] so callers get machine-readable
//! error codes.

use graphgateway_types::{
    GenerationId, ResolvedGraphView, Source, SourceId, SourceKind, SourceRole, ViewMember,
    Workspace, WorkspaceId, WritePolicy,
};
use std::collections::HashSet;

use crate::error::{DomainError, ErrorCode};

// ---------------------------------------------------------------------------
// ID validation
// ---------------------------------------------------------------------------

/// Reject an empty or whitespace-only [`SourceId`].
pub fn validate_source_id(id: &SourceId) -> Result<(), DomainError> {
    if id.as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::SourceNotFound,
            "source_id must not be empty",
        ));
    }
    Ok(())
}

/// Reject an empty or whitespace-only [`WorkspaceId`].
pub fn validate_workspace_id(id: &WorkspaceId) -> Result<(), DomainError> {
    if id.as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::WorkspaceNotFound,
            "workspace_id must not be empty",
        ));
    }
    Ok(())
}

/// Reject an empty or whitespace-only [`GenerationId`].
pub fn validate_generation_id(id: &GenerationId) -> Result<(), DomainError> {
    if id.as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::GenerationNotReady,
            "generation_id must not be empty",
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Source validation
// ---------------------------------------------------------------------------

/// Validate a single [`Source`]'s configuration.
///
/// # Rules
///
/// - `source_id` must be non-empty.
/// - `repo_id` must be non-empty.
/// - `branch` must be non-empty.
/// - A local Source must have a loopback endpoint.
/// - A remote Source must use TLS.
/// - `SourceKind::Unknown` is rejected (fail-closed — P103-R2).
/// - A writable Source must be local (per-source check; workspace-level
///   policy is enforced by [`validate_workspace`]).
/// - Non-Primary roles must not be writable (per-source check).
pub fn validate_source(source: &Source) -> Result<(), DomainError> {
    validate_source_id(&source.source_id)?;

    if source.repo_id.trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::SourceNotFound,
            format!("source '{}' has empty repo_id", source.source_id),
        ));
    }

    if source.branch.trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::SourceNotFound,
            format!("source '{}' has empty branch", source.source_id),
        ));
    }

    match source.location {
        SourceKind::Local => {
            if !source.endpoint.is_loopback() {
                return Err(DomainError::new(
                    ErrorCode::SourceNotFound,
                    format!(
                        "source '{}' is local but endpoint is not loopback: {}",
                        source.source_id, source.endpoint.url,
                    ),
                ));
            }
        }
        SourceKind::Remote => {
            if !source.endpoint.is_tls() {
                return Err(DomainError::new(
                    ErrorCode::SourceNotFound,
                    format!(
                        "source '{}' is remote but endpoint is not TLS: {}",
                        source.source_id, source.endpoint.url,
                    ),
                ));
            }
        }
        SourceKind::Unknown => {
            // P103-R2: fail closed — Unknown location kind cannot be
            // validated and must be rejected.
            return Err(DomainError::new(
                ErrorCode::SourceNotFound,
                format!(
                    "source '{}' has unknown location kind — cannot validate loopback/TLS",
                    source.source_id,
                ),
            ));
        }
    }

    // Per-source writable checks (workspace-level policy in validate_workspace).
    if source.writable {
        if source.location != SourceKind::Local {
            return Err(DomainError::new(
                ErrorCode::WriteScopeDenied,
                format!(
                    "source '{}' is writable but not local (location={:?})",
                    source.source_id, source.location,
                ),
            ));
        }
        if source.role != SourceRole::Primary {
            return Err(DomainError::new(
                ErrorCode::WriteScopeDenied,
                format!(
                    "source '{}' is writable but role is {:?} (must be Primary)",
                    source.source_id, source.role,
                ),
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Workspace validation
// ---------------------------------------------------------------------------

/// Validate a [`Workspace`] configuration.
///
/// # Rules
///
/// - `workspace_id` must be non-empty.
/// - `sources` must not be empty.
/// - Every source must pass [`validate_source`].
/// - Source IDs must be unique.
/// - `primary_source_id` must exist in `sources`.
///
/// # Writable rules (P103-R3)
///
/// - At most one Source may be writable.
/// - A writable Source must be local.
/// - A writable Source must have [`SourceRole::Primary`].
/// - A writable Source's `source_id` must equal
///   `workspace.primary_source_id`.
/// - [`WritePolicy::AllReadOnly`] forbids any writable Source.
/// - Non-Primary roles (baseline, dependency, member, unknown) cannot be
///   writable — this is also enforced per-source by [`validate_source`].
pub fn validate_workspace(ws: &Workspace) -> Result<(), DomainError> {
    validate_workspace_id(&ws.workspace_id)?;

    if ws.sources.is_empty() {
        return Err(DomainError::workspace_not_found(ws.workspace_id.as_ref())
            .with_detail("reason", "sources list is empty"));
    }

    // Validate each source and check for duplicate IDs.
    let mut seen = HashSet::new();
    let mut writable_count = 0u32;
    let mut writable_source_id: Option<&SourceId> = None;

    for source in &ws.sources {
        validate_source(source)?;

        if !seen.insert(source.source_id.as_ref()) {
            return Err(DomainError::new(
                ErrorCode::SourceAmbiguous,
                format!(
                    "workspace '{}' has duplicate source_id '{}'",
                    ws.workspace_id, source.source_id,
                ),
            ));
        }

        if source.writable {
            writable_count += 1;
            writable_source_id = Some(&source.source_id);
        }
    }

    // Primary must exist.
    let _primary_source = ws
        .sources
        .iter()
        .find(|s| s.source_id == ws.primary_source_id)
        .ok_or_else(|| {
            DomainError::source_not_found(ws.primary_source_id.as_ref())
                .with_detail("reason", "primary_source_id not in sources list")
        })?;

    // -- Writable policy enforcement (P103-R3) --

    // AllReadOnly policy: reject any writable source.
    if ws.write_policy == WritePolicy::AllReadOnly && writable_count > 0 {
        return Err(DomainError::new(
            ErrorCode::WriteScopeDenied,
            format!(
                "workspace '{}' has WritePolicy::AllReadOnly but {} writable source(s) configured",
                ws.workspace_id, writable_count,
            ),
        )
        .with_detail("write_policy", "all_read_only"));
    }

    // At most one writable source.
    if writable_count > 1 {
        return Err(DomainError::new(
            ErrorCode::WriteScopeDenied,
            format!(
                "workspace '{}' has {} writable sources; at most one is allowed",
                ws.workspace_id, writable_count,
            ),
        )
        .with_detail("writable_count", writable_count.to_string()));
    }

    // If there is a writable source, it must match primary_source_id.
    if let Some(wid) = writable_source_id
        && *wid != ws.primary_source_id
    {
        return Err(DomainError::new(
            ErrorCode::WriteScopeDenied,
            format!(
                "workspace '{}': writable source '{}' does not match primary_source_id '{}'",
                ws.workspace_id, wid, ws.primary_source_id,
            ),
        )
        .with_detail("writable_source_id", wid.as_ref().to_string())
        .with_detail(
            "primary_source_id",
            ws.primary_source_id.as_ref().to_string(),
        ));
    }

    // If the primary is writable in policy but source.writable is false,
    // that's allowed (the primary may be read-only for some operations).
    // If primary_source is explicitly writable, validate_source already
    // checked it is local + Primary role.

    Ok(())
}

// ---------------------------------------------------------------------------
// ResolvedGraphView validation
// ---------------------------------------------------------------------------

/// Validate a [`ResolvedGraphView`] before it is used for queries.
///
/// # Rules
///
/// - `view_id` must be non-empty.
/// - `workspace_id` must be non-empty.
/// - `members` must not be empty.
/// - Every member must have a non-empty `generation_id`.
/// - `head_sha` must not be empty for each member.
pub fn validate_resolved_view(view: &ResolvedGraphView) -> Result<(), DomainError> {
    if view.view_id().as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::WorkspaceNotFound,
            "view_id must not be empty",
        ));
    }
    if view.workspace_id().as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::WorkspaceNotFound,
            "workspace_id must not be empty",
        ));
    }

    if view.members().is_empty() {
        return Err(DomainError::new(
            ErrorCode::GenerationNotReady,
            "resolved view has no members",
        ));
    }

    for member in view.members() {
        validate_view_member(member)?;
    }

    Ok(())
}

/// Validate a single [`ViewMember`].
pub fn validate_view_member(member: &ViewMember) -> Result<(), DomainError> {
    if member.source_id().as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::SourceNotFound,
            "view member has empty source_id",
        ));
    }
    if member.generation_id().as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::GenerationNotReady,
            format!(
                "view member '{}' has empty generation_id",
                member.source_id(),
            ),
        ));
    }
    if member.head_sha().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::GenerationNotReady,
            format!("view member '{}' has empty head_sha", member.source_id(),),
        ));
    }
    if &member.capability().source_id != member.source_id() {
        return Err(DomainError::new(
            ErrorCode::SourceNotFound,
            format!(
                "view member '{}' has capability snapshot for source '{}'",
                member.source_id(),
                member.capability().source_id,
            ),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use graphgateway_types::{
        Adapter, CapabilitySnapshot, ConsistencyMode, Endpoint, ToolCapability, Transport, ViewId,
    };

    // -- Helpers ------------------------------------------------------------

    fn sample_endpoint(url: &str) -> Endpoint {
        Endpoint {
            url: url.into(),
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
            captured_at: "now".into(),
        }
    }

    fn make_source(id: &str, role: SourceRole, location: SourceKind, writable: bool) -> Source {
        let url = match location {
            SourceKind::Local => "http://127.0.0.1:38471/mcp",
            SourceKind::Remote => "https://graph.example.com/mcp",
            SourceKind::Unknown => "http://unknown/mcp",
        };
        Source {
            source_id: SourceId::new(id).unwrap(),
            repo_id: id.into(),
            branch: "main".into(),
            role,
            location,
            endpoint: sample_endpoint(url),
            writable,
        }
    }

    fn make_member(source_id: &str, gen_id: &str, head_sha: &str) -> ViewMember {
        ViewMember::new(
            SourceId::new(source_id).unwrap(),
            "repo".into(),
            "main".into(),
            GenerationId::new(gen_id).unwrap(),
            head_sha.into(),
            sample_endpoint("http://127.0.0.1:1/mcp"),
            sample_capability(source_id),
        )
        .unwrap()
    }

    // -- ID validation ------------------------------------------------------

    #[test]
    fn reject_empty_source_id() {
        let id: SourceId = serde_json::from_str(r#""""#).unwrap();
        assert!(validate_source_id(&id).is_err());
    }

    #[test]
    fn accept_valid_source_id() {
        let id = SourceId::new("s1").unwrap();
        assert!(validate_source_id(&id).is_ok());
    }

    // -- Source validation --------------------------------------------------

    #[test]
    fn reject_empty_repo_id() {
        let mut src = make_source("s1", SourceRole::Primary, SourceKind::Local, false);
        src.repo_id = "  ".into();
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_empty_branch() {
        let mut src = make_source("s1", SourceRole::Primary, SourceKind::Local, false);
        src.branch = "".into();
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_local_with_non_loopback_endpoint() {
        let mut src = make_source("s1", SourceRole::Primary, SourceKind::Local, false);
        src.endpoint = sample_endpoint("https://example.com/mcp");
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_remote_without_tls() {
        let mut src = make_source("s1", SourceRole::Primary, SourceKind::Remote, false);
        src.endpoint = sample_endpoint("http://example.com/mcp");
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_writable_remote() {
        let src = make_source("s1", SourceRole::Primary, SourceKind::Remote, true);
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_writable_non_primary() {
        // P103-R3: a writable source with Baseline role is rejected at the
        // per-source level.
        let src = make_source("s1", SourceRole::Baseline, SourceKind::Local, true);
        let err = validate_source(&src).unwrap_err();
        assert_eq!(err.code, ErrorCode::WriteScopeDenied);
        assert!(err.message.contains("Primary"));
    }

    #[test]
    fn reject_writable_dependency() {
        let src = make_source("s1", SourceRole::Dependency, SourceKind::Local, true);
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_writable_member() {
        let src = make_source("s1", SourceRole::Member, SourceKind::Local, true);
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_writable_unknown_role() {
        let src = make_source("s1", SourceRole::Unknown, SourceKind::Local, true);
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn accept_valid_local_source() {
        let src = make_source("s1", SourceRole::Primary, SourceKind::Local, true);
        assert!(validate_source(&src).is_ok());
    }

    #[test]
    fn accept_valid_remote_source() {
        let src = make_source("s1", SourceRole::Primary, SourceKind::Remote, false);
        assert!(validate_source(&src).is_ok());
    }

    // -- SourceKind::Unknown fail-closed (P103-R2) ---------------------------

    #[test]
    fn reject_unknown_location_kind() {
        let src = make_source("s1", SourceRole::Primary, SourceKind::Unknown, false);
        let err = validate_source(&src).unwrap_err();
        assert_eq!(err.code, ErrorCode::SourceNotFound);
        assert!(err.message.contains("unknown location kind"));
    }

    // -- P103-R2: loopback bypass attempts -----------------------------------

    #[test]
    fn reject_local_with_localhost_in_query() {
        let mut src = make_source("s1", SourceRole::Primary, SourceKind::Local, false);
        src.endpoint = sample_endpoint("https://example.invalid/?localhost");
        let err = validate_source(&src).unwrap_err();
        assert!(err.message.contains("not loopback"));
    }

    #[test]
    fn reject_local_with_ip_suffix() {
        let mut src = make_source("s1", SourceRole::Primary, SourceKind::Local, false);
        src.endpoint = sample_endpoint("http://127.0.0.1.example.invalid/mcp");
        let err = validate_source(&src).unwrap_err();
        assert!(err.message.contains("not loopback"));
    }

    // -- Workspace validation -----------------------------------------------

    #[test]
    fn reject_empty_sources() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "empty".into(),
            sources: vec![],
            primary_source_id: SourceId::new("nope").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        assert!(validate_workspace(&ws).is_err());
    }

    #[test]
    fn reject_duplicate_source_ids() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "dup".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, true),
                make_source("s1", SourceRole::Primary, SourceKind::Remote, false),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        assert!(validate_workspace(&ws).is_err());
    }

    #[test]
    fn reject_missing_primary_source() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "bad-primary".into(),
            sources: vec![make_source(
                "s1",
                SourceRole::Primary,
                SourceKind::Local,
                true,
            )],
            primary_source_id: SourceId::new("s2").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        let err = validate_workspace(&ws).unwrap_err();
        assert_eq!(err.code, ErrorCode::SourceNotFound);
    }

    #[test]
    fn accept_valid_workspace() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "valid".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, true),
                make_source("s2", SourceRole::Member, SourceKind::Remote, false),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::NearRealTime,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        assert!(validate_workspace(&ws).is_ok());
    }

    // -- P103-R3: workspace writable validation ------------------------------

    #[test]
    fn reject_multiple_writable_sources() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "multi-writable".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, true),
                make_source("s2", SourceRole::Primary, SourceKind::Local, true),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        let err = validate_workspace(&ws).unwrap_err();
        assert_eq!(err.code, ErrorCode::WriteScopeDenied);
        assert!(err.message.contains("2 writable sources"));
    }

    #[test]
    fn reject_writable_not_matching_primary_source_id() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "mismatched-writable".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, false),
                make_source("s2", SourceRole::Primary, SourceKind::Local, true),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        let err = validate_workspace(&ws).unwrap_err();
        assert_eq!(err.code, ErrorCode::WriteScopeDenied);
        assert!(err.message.contains("does not match primary_source_id"));
    }

    #[test]
    fn reject_all_read_only_with_writable_source() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "all-read-only-writable".into(),
            sources: vec![make_source(
                "s1",
                SourceRole::Primary,
                SourceKind::Local,
                true,
            )],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: WritePolicy::AllReadOnly,
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        let err = validate_workspace(&ws).unwrap_err();
        assert_eq!(err.code, ErrorCode::WriteScopeDenied);
        assert!(err.message.contains("AllReadOnly"));
    }

    #[test]
    fn reject_writable_baseline_source() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "writable-baseline".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, false),
                make_source("s2", SourceRole::Baseline, SourceKind::Local, true),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        // The per-source check on s2 fails before workspace-level validation.
        let err = validate_workspace(&ws).unwrap_err();
        assert_eq!(err.code, ErrorCode::WriteScopeDenied);
    }

    #[test]
    fn reject_writable_dependency_source() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "writable-dep".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, false),
                make_source("s2", SourceRole::Dependency, SourceKind::Local, true),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        let err = validate_workspace(&ws).unwrap_err();
        assert_eq!(err.code, ErrorCode::WriteScopeDenied);
    }

    #[test]
    fn reject_writable_member_source() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "writable-member".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, false),
                make_source("s2", SourceRole::Member, SourceKind::Local, true),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        let err = validate_workspace(&ws).unwrap_err();
        assert_eq!(err.code, ErrorCode::WriteScopeDenied);
    }

    #[test]
    fn accept_no_writable_source() {
        // A workspace where no source is writable is valid.
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "no-writable".into(),
            sources: vec![
                make_source("s1", SourceRole::Primary, SourceKind::Local, false),
                make_source("s2", SourceRole::Member, SourceKind::Remote, false),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        assert!(validate_workspace(&ws).is_ok());
    }

    #[test]
    fn accept_all_read_only_with_no_writable() {
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "all-read-only-ok".into(),
            sources: vec![make_source(
                "s1",
                SourceRole::Primary,
                SourceKind::Local,
                false,
            )],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: WritePolicy::AllReadOnly,
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        assert!(validate_workspace(&ws).is_ok());
    }

    #[test]
    fn reject_unknown_write_policy_with_writable() {
        // Unknown write policy: treat like PrimaryOnly (the default behavior
        // is to accept a writable primary).  This test verifies that a
        // writable primary is still accepted under WritePolicy::Unknown.
        let ws = Workspace {
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            name: "unknown-policy".into(),
            sources: vec![make_source(
                "s1",
                SourceRole::Primary,
                SourceKind::Local,
                true,
            )],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::Exact,
            write_policy: WritePolicy::Unknown,
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        assert!(validate_workspace(&ws).is_ok());
    }

    // -- ResolvedGraphView validation ---------------------------------------

    #[test]
    fn reject_view_with_no_members() {
        let view = ResolvedGraphView::new(
            ViewId::new("v-1").unwrap(),
            WorkspaceId::new("ws-1").unwrap(),
            vec![],
            "now".into(),
        );
        assert!(view.is_err() || validate_resolved_view(&view.unwrap()).is_err());
    }

    #[test]
    fn reject_view_member_with_empty_generation() {
        // Construct via JSON: the validated constructor rejects empty
        // generation IDs, and fields are private after P103-R1.
        let view: ResolvedGraphView = serde_json::from_str(
            r#"{
                "view_id": "v-1",
                "workspace_id": "ws-1",
                "members": [
                    {
                        "source_id": "s1",
                        "repo_id": "r",
                        "branch": "b",
                        "generation_id": "",
                        "head_sha": "abc",
                        "unavailable": false,
                        "endpoint": {"url":"http://127.0.0.1:1/mcp","transport":"streamable_http","adapter":"mcp_proxy"},
                        "capability": {"source_id":"s1","capabilities":[],"captured_at":"now"}
                    }
                ],
                "resolved_at": "now"
            }"#,
        )
        .unwrap();
        assert!(validate_resolved_view(&view).is_err());
    }

    #[test]
    fn reject_view_member_with_empty_head_sha() {
        // Construct via JSON: the validated constructor rejects empty
        // head_sha, and fields are private after P103-R1.
        let view: ResolvedGraphView = serde_json::from_str(
            r#"{
                "view_id": "v-1",
                "workspace_id": "ws-1",
                "members": [
                    {
                        "source_id": "s1",
                        "repo_id": "r",
                        "branch": "b",
                        "generation_id": "gen-1",
                        "head_sha": "  ",
                        "unavailable": false,
                        "endpoint": {"url":"http://127.0.0.1:1/mcp","transport":"streamable_http","adapter":"mcp_proxy"},
                        "capability": {"source_id":"s1","capabilities":[],"captured_at":"now"}
                    }
                ],
                "resolved_at": "now"
            }"#,
        )
        .unwrap();
        assert!(validate_resolved_view(&view).is_err());
    }

    #[test]
    fn reject_view_member_with_mismatched_capability_source_id() {
        // Construct via JSON so the mismatched capability bypasses the
        // constructor validation.  This tests the validator's own
        // detection of the mismatch (P103-R1).
        let view: ResolvedGraphView = serde_json::from_str(
            r#"{
                "view_id": "v-1",
                "workspace_id": "ws-1",
                "members": [
                    {
                        "source_id": "s1",
                        "repo_id": "r",
                        "branch": "b",
                        "generation_id": "gen-1",
                        "head_sha": "abc",
                        "unavailable": false,
                        "endpoint": {"url":"http://127.0.0.1:1/mcp","transport":"streamable_http","adapter":"mcp_proxy"},
                        "capability": {"source_id":"other-source","capabilities":[],"captured_at":"now"}
                    }
                ],
                "resolved_at": "now"
            }"#,
        )
        .unwrap();
        let err = validate_resolved_view(&view).unwrap_err();
        assert_eq!(err.code, ErrorCode::SourceNotFound);
        assert!(err.message.contains("capability snapshot"));
    }

    #[test]
    fn accept_valid_view() {
        let view = ResolvedGraphView::new(
            ViewId::new("v-1").unwrap(),
            WorkspaceId::new("ws-1").unwrap(),
            vec![make_member("s1", "gen-1", "abc")],
            "now".into(),
        )
        .unwrap();
        assert!(validate_resolved_view(&view).is_ok());
    }
}
