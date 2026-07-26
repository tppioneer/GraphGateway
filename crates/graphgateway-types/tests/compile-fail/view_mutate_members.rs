// P103-R1 compile-fail: cannot mutate ResolvedGraphView members in place.
// The members() accessor returns &[ViewMember] — immutable slice.

use graphgateway_types::*;

fn main() {
    let endpoint = Endpoint {
        url: "http://127.0.0.1:1/mcp".into(),
        transport: Transport::StreamableHttp,
        adapter: Adapter::McpProxy,
    };
    let cap = CapabilitySnapshot {
        source_id: SourceId::new("s1").unwrap(),
        capabilities: vec![],
        captured_at: "now".into(),
    };
    let member = ViewMember::new(
        SourceId::new("s1").unwrap(),
        "repo".into(),
        "main".into(),
        GenerationId::new("gen-1").unwrap(),
        "abc".into(),
        endpoint,
        cap,
    )
    .unwrap();

    let view = ResolvedGraphView::new(
        ViewId::new("v-1").unwrap(),
        WorkspaceId::new("ws-1").unwrap(),
        vec![member],
        "now".into(),
    )
    .unwrap();

    // Attempt to push a new member — members() returns &[ViewMember].
    view.members().push(ViewMember::new(
        SourceId::new("s2").unwrap(),
        "repo2".into(),
        "main".into(),
        GenerationId::new("gen-2").unwrap(),
        "def".into(),
        Endpoint {
            url: "http://127.0.0.1:2/mcp".into(),
            transport: Transport::StreamableHttp,
            adapter: Adapter::McpProxy,
        },
        CapabilitySnapshot {
            source_id: SourceId::new("s2").unwrap(),
            capabilities: vec![],
            captured_at: "now".into(),
        },
    ).unwrap());
    //~^^ ERROR no method named `push`
}
