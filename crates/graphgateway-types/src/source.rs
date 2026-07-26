//! Graph Source — a queryable code-graph origin.
//!
//! See design §3.1, §4, §6.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::ids::SourceId;

// ---------------------------------------------------------------------------
// SourceKind
// ---------------------------------------------------------------------------

/// Whether a [`Source`] is local (loopback) or remote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Local `127.0.0.1` endpoint (often managed by the Tauri sidecar or
    /// local mcp-proxy process).
    Local,
    /// Remote HTTPS endpoint.
    Remote,
    /// Catch-all for forward compatibility — treat as remote, read-only.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// SourceRole
// ---------------------------------------------------------------------------

/// Purpose of a [`Source`] within a [`Workspace`](crate::Workspace).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceRole {
    /// Current development main graph — the only writable Source.
    Primary,
    /// Read-only baseline used for comparison.
    Baseline,
    /// Fixed-version upstream dependency.
    Dependency,
    /// Micro-service workspace member.
    Member,
    /// Catch-all for forward compatibility — treat as read-only.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Transport
// ---------------------------------------------------------------------------

/// MCP transport protocol used by an [`Endpoint`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    /// MCP Streamable HTTP (2025-03-26 spec and later).
    StreamableHttp,
    /// Catch-all for forward compatibility.
    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Adapter
// ---------------------------------------------------------------------------

/// Identity of the proxy / transport adapter in front of a GitNexus stdio
/// MCP process.
///
/// Serialises as a plain snake_case string.  Unknown values (forward
/// compatibility) are preserved in [`Adapter::Unknown`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Adapter {
    /// A third-party `mcp-proxy` translating Streamable HTTP ↔ stdio MCP.
    McpProxy,
    /// Direct stdio MCP (no proxy — reserved for tests / embedded usage).
    Direct,
    /// Catch-all for forward compatibility — holds the original string.
    Unknown(String),
}

impl Serialize for Adapter {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Adapter::McpProxy => serializer.serialize_str("mcp_proxy"),
            Adapter::Direct => serializer.serialize_str("direct"),
            Adapter::Unknown(s) => serializer.serialize_str(s),
        }
    }
}

impl<'de> Deserialize<'de> for Adapter {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        Ok(match s.as_str() {
            "mcp_proxy" => Adapter::McpProxy,
            "direct" => Adapter::Direct,
            _other => Adapter::Unknown(s),
        })
    }
}

// ---------------------------------------------------------------------------
// Endpoint
// ---------------------------------------------------------------------------

/// Network-addressable MCP endpoint for a [`Source`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    /// Full URL including scheme, host, port, and path.
    ///
    /// Examples: `"http://127.0.0.1:38471/mcp"`,
    /// `"https://graph.example.com/mcp"`.
    pub url: String,
    /// MCP transport protocol.
    pub transport: Transport,
    /// Adapter type or product name.
    pub adapter: Adapter,
}

impl Endpoint {
    /// Parse the endpoint URL and return the parsed components.
    ///
    /// Returns `None` when the URL cannot be parsed.
    fn parsed_url(&self) -> Option<Url> {
        Url::parse(&self.url).ok()
    }

    /// Returns `true` when the endpoint URL points to a loopback address.
    ///
    /// Uses proper URL parsing to prevent bypass via query strings,
    /// hostname suffixes, or other tricks.  Accepted loopback forms:
    ///
    /// - IPv4: `127.0.0.1` (any port)
    /// - IPv6: `[::1]` (any port)
    /// - Hostname: `localhost` (exact match, case-insensitive)
    pub fn is_loopback(&self) -> bool {
        let Some(parsed) = self.parsed_url() else {
            return false;
        };

        match parsed.host() {
            Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
            Some(url::Host::Domain(host)) => {
                // Exact, case-insensitive match — "localhost" only.
                // Rejects suffixes like "127.0.0.1.example.invalid" and
                // subdomain tricks like "localhost.evil.com".
                host.eq_ignore_ascii_case("localhost")
            }
            None => false,
        }
    }

    /// Returns `true` when the endpoint URL uses the HTTPS scheme.
    ///
    /// Parses the URL to determine the scheme rather than relying on
    /// string prefix matching.
    pub fn is_tls(&self) -> bool {
        self.parsed_url()
            .map(|u| u.scheme() == "https")
            .unwrap_or(false)
    }

    /// Returns the URL scheme, or `None` when the URL cannot be parsed.
    pub fn scheme(&self) -> Option<String> {
        self.parsed_url().map(|u| u.scheme().to_string())
    }
}

// ---------------------------------------------------------------------------
// BranchSelector
// ---------------------------------------------------------------------------

/// Describes which branch (or Git ref) a Source is tracking.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BranchSelector {
    /// Repository identifier (used by GitNexus for indexing).
    pub repo_id: String,
    /// Git branch or tag name.
    ///
    /// Examples: `"main"`, `"feature/refund"`, `"tags/v2.1.0"`.
    pub branch: String,
    /// Optional full Git ref (e.g. `"refs/heads/feature/refund"`).
    /// When absent, `branch` is treated as a short branch name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_ref: Option<String>,
}

// ---------------------------------------------------------------------------
// Source
// ---------------------------------------------------------------------------

/// A queryable code-graph source.
///
/// One [`Source`] represents one `(repo_id, branch)` pair served from a
/// single [`Endpoint`].  The Router resolves each Source to a
/// [`Generation`](crate::Generation) before executing any query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Source {
    /// Unique identifier for this Source within its Workspace.
    pub source_id: SourceId,
    /// Repository identifier.
    pub repo_id: String,
    /// Git branch or tag being indexed.
    pub branch: String,
    /// Role within the owning Workspace.
    pub role: SourceRole,
    /// Location kind.
    pub location: SourceKind,
    /// Network endpoint of the MCP node.
    pub endpoint: Endpoint,
    /// Whether the Router may send write operations to this Source.
    #[serde(default)]
    pub writable: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_endpoint() -> Endpoint {
        Endpoint {
            url: "http://127.0.0.1:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        }
    }

    fn sample_source(local: bool) -> Source {
        Source {
            source_id: SourceId::new("test-src").unwrap(),
            repo_id: "payment-service".into(),
            branch: "main".into(),
            role: SourceRole::Primary,
            location: if local {
                SourceKind::Local
            } else {
                SourceKind::Remote
            },
            endpoint: sample_endpoint(),
            writable: local,
        }
    }

    // -- SourceKind serde ---------------------------------------------------

    #[test]
    fn source_kind_local_roundtrip() {
        let json = r#""local""#;
        let v: SourceKind = serde_json::from_str(json).unwrap();
        assert_eq!(v, SourceKind::Local);
        assert_eq!(serde_json::to_string(&v).unwrap(), json);
    }

    #[test]
    fn source_kind_remote_roundtrip() {
        let json = r#""remote""#;
        let v: SourceKind = serde_json::from_str(json).unwrap();
        assert_eq!(v, SourceKind::Remote);
        assert_eq!(serde_json::to_string(&v).unwrap(), json);
    }

    #[test]
    fn source_kind_unknown_falls_back() {
        let v: SourceKind = serde_json::from_str(r#""satellite""#).unwrap();
        assert_eq!(v, SourceKind::Unknown);
    }

    // -- SourceRole serde ---------------------------------------------------

    #[test]
    fn source_role_roundtrip() {
        for (role, expected) in [
            ("primary", SourceRole::Primary),
            ("baseline", SourceRole::Baseline),
            ("dependency", SourceRole::Dependency),
            ("member", SourceRole::Member),
        ] {
            let json = format!(r#""{}""#, role);
            let v: SourceRole = serde_json::from_str(&json).unwrap();
            assert_eq!(v, expected);
            assert_eq!(serde_json::to_string(&v).unwrap(), json);
        }
    }

    #[test]
    fn source_role_unknown_falls_back() {
        let v: SourceRole = serde_json::from_str(r#""observer""#).unwrap();
        assert_eq!(v, SourceRole::Unknown);
    }

    // -- Transport serde ----------------------------------------------------

    #[test]
    fn transport_streamable_http_roundtrip() {
        let json = r#""streamable_http""#;
        let v: Transport = serde_json::from_str(json).unwrap();
        assert_eq!(v, Transport::StreamableHttp);
        assert_eq!(serde_json::to_string(&v).unwrap(), json);
    }

    #[test]
    fn transport_unknown_falls_back() {
        let v: Transport = serde_json::from_str(r#""websocket""#).unwrap();
        assert_eq!(v, Transport::Unknown);
    }

    // -- Adapter serde ------------------------------------------------------

    #[test]
    fn adapter_mcp_proxy_roundtrip() {
        let json = r#""mcp_proxy""#;
        let v: Adapter = serde_json::from_str(json).unwrap();
        assert_eq!(v, Adapter::McpProxy);
    }

    #[test]
    fn adapter_direct_roundtrip() {
        let json = r#""direct""#;
        let v: Adapter = serde_json::from_str(json).unwrap();
        assert_eq!(v, Adapter::Direct);
    }

    #[test]
    fn adapter_unknown_roundtrip_preserves_value() {
        let json = r#""custom-gateway""#;
        let v: Adapter = serde_json::from_str(json).unwrap();
        assert_eq!(v, Adapter::Unknown("custom-gateway".into()));
        assert_eq!(serde_json::to_string(&v).unwrap(), json);
    }

    // -- Endpoint helpers ---------------------------------------------------

    // === Positive loopback tests ===

    #[test]
    fn endpoint_is_loopback_ipv4() {
        let ep = Endpoint {
            url: "http://127.0.0.1:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_loopback());
    }

    #[test]
    fn endpoint_is_loopback_ipv4_any_port() {
        let ep = Endpoint {
            url: "http://127.0.0.1:9999/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_loopback());
    }

    #[test]
    fn endpoint_is_loopback_ipv4_class_a() {
        // 127.0.0.0/8 — all addresses in this block are loopback.
        let ep = Endpoint {
            url: "http://127.255.255.255:8080/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_loopback());
    }

    #[test]
    fn endpoint_is_loopback_ipv6() {
        let ep = Endpoint {
            url: "http://[::1]:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_loopback());
    }

    #[test]
    fn endpoint_is_loopback_localhost() {
        let ep = Endpoint {
            url: "http://localhost:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_loopback());
    }

    #[test]
    fn endpoint_is_loopback_localhost_uppercase() {
        let ep = Endpoint {
            url: "http://LOCALHOST:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_loopback());
    }

    #[test]
    fn endpoint_is_loopback_localhost_mixed_case() {
        let ep = Endpoint {
            url: "http://LocalHost:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_loopback());
    }

    // === Negative loopback tests (bypass attempts) ===

    #[test]
    fn endpoint_is_not_loopback_remote() {
        let ep = Endpoint {
            url: "https://graph.example.com/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_loopback());
        assert!(ep.is_tls());
    }

    #[test]
    fn endpoint_not_loopback_localhost_in_query() {
        // P103-R2: "localhost" as a query parameter does NOT make it loopback.
        let ep = Endpoint {
            url: "https://example.invalid/?localhost".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_loopback());
    }

    #[test]
    fn endpoint_not_loopback_ip_suffix() {
        // P103-R2: "127.0.0.1" as a hostname suffix does NOT make it loopback.
        let ep = Endpoint {
            url: "http://127.0.0.1.example.invalid/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_loopback());
    }

    #[test]
    fn endpoint_not_loopback_localhost_subdomain() {
        let ep = Endpoint {
            url: "http://localhost.evil.com/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_loopback());
    }

    #[test]
    fn endpoint_not_loopback_localhost_in_path() {
        let ep = Endpoint {
            url: "https://example.com/localhost/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_loopback());
    }

    #[test]
    fn endpoint_not_loopback_malformed_url() {
        // A malformed URL cannot be parsed — treat as not loopback.
        let ep = Endpoint {
            url: "not-a-valid-url!!!".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_loopback());
        assert!(!ep.is_tls());
    }

    #[test]
    fn endpoint_not_loopback_empty_url() {
        let ep = Endpoint {
            url: "".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_loopback());
        assert!(!ep.is_tls());
    }

    // === TLS tests ===

    #[test]
    fn endpoint_is_tls_https() {
        let ep = Endpoint {
            url: "https://graph.example.com/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(ep.is_tls());
    }

    #[test]
    fn endpoint_is_not_tls_http() {
        let ep = Endpoint {
            url: "http://127.0.0.1:38471/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert!(!ep.is_tls());
    }

    #[test]
    fn endpoint_scheme() {
        let ep = Endpoint {
            url: "https://graph.example.com/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        };
        assert_eq!(ep.scheme().as_deref(), Some("https"));
    }

    // -- Source serde round-trip --------------------------------------------

    #[test]
    fn source_json_roundtrip() {
        let src = sample_source(true);
        let json = serde_json::to_string_pretty(&src).unwrap();
        let src2: Source = serde_json::from_str(&json).unwrap();
        assert_eq!(src, src2);
    }

    #[test]
    fn source_writable_defaults_to_false() {
        let json = r#"{
            "source_id":"s1",
            "repo_id":"r",
            "branch":"b",
            "role":"primary",
            "location":"local",
            "endpoint":{"url":"http://127.0.0.1:1/mcp","transport":"streamable_http","adapter":"mcp_proxy"}
        }"#;
        let src: Source = serde_json::from_str(json).unwrap();
        assert!(!src.writable);
    }

    // -- BranchSelector serde -----------------------------------------------

    #[test]
    fn branch_selector_roundtrip() {
        let bs = BranchSelector {
            repo_id: "payment-service".into(),
            branch: "feature/refund".into(),
            git_ref: Some("refs/heads/feature/refund".into()),
        };
        let json = serde_json::to_string(&bs).unwrap();
        let bs2: BranchSelector = serde_json::from_str(&json).unwrap();
        assert_eq!(bs, bs2);
    }

    #[test]
    fn branch_selector_without_git_ref() {
        let json = r#"{"repo_id":"r","branch":"main"}"#;
        let bs: BranchSelector = serde_json::from_str(json).unwrap();
        assert_eq!(bs.branch, "main");
        assert!(bs.git_ref.is_none());
    }
}
