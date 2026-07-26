// P103-R1 compile-fail: cannot replace a ViewMember's capability snapshot
// in place. The capability() accessor returns &CapabilitySnapshot.

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

    // Attempt to mutate capability through the accessor.
    let c = member.capability();
    c.source_id = SourceId::new("evil").unwrap();
    //~^ ERROR cannot assign
}
