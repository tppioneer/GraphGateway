//! Capability snapshot — records which tools a downstream GitNexus
//! node supports, so the Router can validate a QueryPlan before
//! dispatching.
//!
//! See design §5.2, §14.

use serde::{Deserialize, Serialize};

use crate::ids::SourceId;

// ---------------------------------------------------------------------------
// ToolCapability
// ---------------------------------------------------------------------------

/// Versioned capability for a single MCP tool.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolCapability {
    /// Tool name (e.g. `"query"`, `"impact"`, `"pdg_query"`).
    pub name: String,
    /// Semantic version string, or `null` when the tool is not
    /// supported.
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

impl ToolCapability {
    /// Returns `true` when this tool is available (version is `Some`).
    pub fn available(&self) -> bool {
        self.version.is_some()
    }
}

// ---------------------------------------------------------------------------
// CapabilitySnapshot
// ---------------------------------------------------------------------------

/// A snapshot of a single Source's available MCP tools, captured during
/// initialisation or session re-negotiation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySnapshot {
    /// The Source this snapshot belongs to.
    pub source_id: SourceId,
    /// Map of tool name → version info.  A tool whose `version` is
    /// `None` is not supported by this node.
    pub capabilities: Vec<ToolCapability>,
    /// ISO-8601 timestamp of when this snapshot was taken.
    pub captured_at: String,
}

impl CapabilitySnapshot {
    /// Look up a tool by name.
    pub fn tool(&self, name: &str) -> Option<&ToolCapability> {
        self.capabilities.iter().find(|c| c.name == name)
    }

    /// Returns `true` when `tool` is present with a non-null version.
    pub fn supports(&self, tool: &str) -> bool {
        self.tool(tool).is_some_and(|c| c.available())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_capability_available() {
        let tc = ToolCapability {
            name: "query".into(),
            version: Some("1.2".into()),
        };
        assert!(tc.available());
    }

    #[test]
    fn tool_capability_unavailable() {
        let tc = ToolCapability {
            name: "pdg_query".into(),
            version: None,
        };
        assert!(!tc.available());
    }

    #[test]
    fn capability_snapshot_serde_roundtrip() {
        let snap = CapabilitySnapshot {
            source_id: SourceId::new("provider").unwrap(),
            capabilities: vec![
                ToolCapability {
                    name: "query".into(),
                    version: Some("1.2".into()),
                },
                ToolCapability {
                    name: "impact".into(),
                    version: Some("1.1".into()),
                },
                ToolCapability {
                    name: "pdg_query".into(),
                    version: None,
                },
            ],
            captured_at: "2026-07-24T10:30:00Z".into(),
        };
        let json = serde_json::to_string_pretty(&snap).unwrap();
        let snap2: CapabilitySnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(snap, snap2);
    }

    #[test]
    fn capability_snapshot_lookup() {
        let snap = CapabilitySnapshot {
            source_id: SourceId::new("provider").unwrap(),
            capabilities: vec![ToolCapability {
                name: "query".into(),
                version: Some("1.2".into()),
            }],
            captured_at: "now".into(),
        };
        assert!(snap.supports("query"));
        assert!(!snap.supports("pdg_query"));
        assert!(snap.tool("nonexistent").is_none());
    }

    #[test]
    fn capability_snapshot_matches_design_example() {
        // §14 example
        let json = r#"{
            "source_id": "provider",
            "capabilities": [
                {"name": "query", "version": "1.2"},
                {"name": "impact", "version": "1.1"},
                {"name": "pdg_query", "version": null},
                {"name": "branch_generation", "version": "1.0"}
            ],
            "captured_at": "2026-07-24T10:30:00Z"
        }"#;
        let snap: CapabilitySnapshot = serde_json::from_str(json).unwrap();
        assert!(snap.supports("query"));
        assert!(!snap.supports("pdg_query"));
    }
}
