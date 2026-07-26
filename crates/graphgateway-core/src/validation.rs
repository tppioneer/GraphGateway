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
    GenerationId, ResolvedGraphView, Source, SourceId, SourceKind, ViewMember, Workspace,
    WorkspaceId,
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
/// - A writable Source must be local.
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
            // Unknown location kind — we can't enforce loopback/TLS.
            // Accept with a warning path.
        }
    }

    if source.writable && source.location != SourceKind::Local {
        return Err(DomainError::new(
            ErrorCode::WriteScopeDenied,
            format!(
                "source '{}' is writable but not local (location={:?})",
                source.source_id, source.location,
            ),
        ));
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
pub fn validate_workspace(ws: &Workspace) -> Result<(), DomainError> {
    validate_workspace_id(&ws.workspace_id)?;

    if ws.sources.is_empty() {
        return Err(DomainError::workspace_not_found(ws.workspace_id.as_ref())
            .with_detail("reason", "sources list is empty"));
    }

    // Validate each source and check for duplicate IDs.
    let mut seen = HashSet::new();
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
    }

    // Primary must exist.
    if !ws
        .sources
        .iter()
        .any(|s| s.source_id == ws.primary_source_id)
    {
        return Err(DomainError::source_not_found(ws.primary_source_id.as_ref())
            .with_detail("reason", "primary_source_id not in sources list"));
    }

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
    if view.view_id.as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::WorkspaceNotFound,
            "view_id must not be empty",
        ));
    }
    if view.workspace_id.as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::WorkspaceNotFound,
            "workspace_id must not be empty",
        ));
    }

    if view.members.is_empty() {
        return Err(DomainError::new(
            ErrorCode::GenerationNotReady,
            "resolved view has no members",
        ));
    }

    for member in &view.members {
        validate_view_member(member)?;
    }

    Ok(())
}

/// Validate a single [`ViewMember`].
pub fn validate_view_member(member: &ViewMember) -> Result<(), DomainError> {
    if member.source_id.as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::SourceNotFound,
            "view member has empty source_id",
        ));
    }
    if member.generation_id.as_ref().trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::GenerationNotReady,
            format!("view member '{}' has empty generation_id", member.source_id,),
        ));
    }
    if member.head_sha.trim().is_empty() {
        return Err(DomainError::new(
            ErrorCode::GenerationNotReady,
            format!("view member '{}' has empty head_sha", member.source_id,),
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
    use graphgateway_types::{Adapter, ConsistencyMode, Endpoint, SourceRole, Transport, ViewId};

    // -- Helpers ------------------------------------------------------------

    fn sample_endpoint(url: &str) -> Endpoint {
        Endpoint {
            url: url.into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        }
    }

    fn make_source(id: &str, location: SourceKind, writable: bool) -> Source {
        let url = match location {
            SourceKind::Local => "http://127.0.0.1:38471/mcp",
            SourceKind::Remote => "https://graph.example.com/mcp",
            SourceKind::Unknown => "http://unknown/mcp",
        };
        Source {
            source_id: SourceId::new(id).unwrap(),
            repo_id: id.into(),
            branch: "main".into(),
            role: SourceRole::Primary,
            location,
            endpoint: sample_endpoint(url),
            writable,
        }
    }

    // -- ID validation ------------------------------------------------------

    #[test]
    fn reject_empty_source_id() {
        // `SourceId::new("")` already rejects at construction time.
        // This test covers the code path where an ID is deserialised
        // from JSON without going through the constructor.
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
        let mut src = make_source("s1", SourceKind::Local, false);
        src.repo_id = "  ".into();
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_empty_branch() {
        let mut src = make_source("s1", SourceKind::Local, false);
        src.branch = "".into();
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_local_with_non_loopback_endpoint() {
        let mut src = make_source("s1", SourceKind::Local, false);
        src.endpoint = sample_endpoint("https://example.com/mcp");
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_remote_without_tls() {
        let mut src = make_source("s1", SourceKind::Remote, false);
        src.endpoint = sample_endpoint("http://example.com/mcp");
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn reject_writable_remote() {
        let src = make_source("s1", SourceKind::Remote, true);
        assert!(validate_source(&src).is_err());
    }

    #[test]
    fn accept_valid_local_source() {
        let src = make_source("s1", SourceKind::Local, true);
        assert!(validate_source(&src).is_ok());
    }

    #[test]
    fn accept_valid_remote_source() {
        let src = make_source("s1", SourceKind::Remote, false);
        assert!(validate_source(&src).is_ok());
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
                make_source("s1", SourceKind::Local, true),
                make_source("s1", SourceKind::Remote, false),
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
            sources: vec![make_source("s1", SourceKind::Local, true)],
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
                make_source("s1", SourceKind::Local, true),
                make_source("s2", SourceKind::Remote, false),
            ],
            primary_source_id: SourceId::new("s1").unwrap(),
            consistency: ConsistencyMode::NearRealTime,
            write_policy: Default::default(),
            wait_policy: Default::default(),
            max_staleness_ms: None,
        };
        assert!(validate_workspace(&ws).is_ok());
    }

    // -- ResolvedGraphView validation ---------------------------------------

    #[test]
    fn reject_view_with_no_members() {
        let view = ResolvedGraphView {
            view_id: ViewId::new("v-1").unwrap(),
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            members: vec![],
            resolved_at: "now".into(),
        };
        assert!(validate_resolved_view(&view).is_err());
    }

    #[test]
    fn reject_view_member_with_empty_generation() {
        // GenerationId::new("") rejects at construction time.
        // Use serde to create an empty one for validation-layer testing.
        let gid: GenerationId = serde_json::from_str(r#""""#).unwrap();
        let view = ResolvedGraphView {
            view_id: ViewId::new("v-1").unwrap(),
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            members: vec![ViewMember {
                source_id: SourceId::new("s1").unwrap(),
                repo_id: "r".into(),
                branch: "b".into(),
                generation_id: gid,
                head_sha: "abc".into(),
                unavailable: false,
            }],
            resolved_at: "now".into(),
        };
        assert!(validate_resolved_view(&view).is_err());
    }

    #[test]
    fn reject_view_member_with_empty_head_sha() {
        let view = ResolvedGraphView {
            view_id: ViewId::new("v-1").unwrap(),
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            members: vec![ViewMember {
                source_id: SourceId::new("s1").unwrap(),
                repo_id: "r".into(),
                branch: "b".into(),
                generation_id: GenerationId::new("gen-1").unwrap(),
                head_sha: "  ".into(),
                unavailable: false,
            }],
            resolved_at: "now".into(),
        };
        assert!(validate_resolved_view(&view).is_err());
    }

    #[test]
    fn accept_valid_view() {
        let view = ResolvedGraphView {
            view_id: ViewId::new("v-1").unwrap(),
            workspace_id: WorkspaceId::new("ws-1").unwrap(),
            members: vec![ViewMember {
                source_id: SourceId::new("s1").unwrap(),
                repo_id: "r".into(),
                branch: "b".into(),
                generation_id: GenerationId::new("gen-1").unwrap(),
                head_sha: "abc".into(),
                unavailable: false,
            }],
            resolved_at: "now".into(),
        };
        assert!(validate_resolved_view(&view).is_ok());
    }
}
