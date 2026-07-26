// P103-R1 compile-fail: cannot mutate ResolvedGraphView's view_id in place.
// All fields are private — direct field access is impossible.

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

    let mut view = ResolvedGraphView::new(
        ViewId::new("v-1").unwrap(),
        WorkspaceId::new("ws-1").unwrap(),
        vec![member],
        "now".into(),
    )
    .unwrap();

    // Attempt to assign to a private field — should fail.
    view.view_id = ViewId::new("v-2").unwrap();
    //~^ ERROR field `view_id` of struct `ResolvedGraphView` is private
}
