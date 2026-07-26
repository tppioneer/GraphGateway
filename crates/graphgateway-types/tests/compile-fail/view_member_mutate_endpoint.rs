// P103-R1 compile-fail: cannot replace a ViewMember's endpoint in place.
// The endpoint() accessor returns &Endpoint — immutable borrow.

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

    // Attempt to mutate endpoint through the accessor.
    let ep = member.endpoint();
    ep.url = "http://evil.com/mcp".into();
    //~^ ERROR cannot assign
}
