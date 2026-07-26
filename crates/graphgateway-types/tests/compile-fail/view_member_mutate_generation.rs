// P103-R1 compile-fail: cannot mutate ViewMember fields in place.
// All fields are private and only & accessors are provided.

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

    // Attempt to mutate generation_id through the accessor — should fail.
    let gen_id = member.generation_id();
    // Cannot assign to the deref target because it's behind a shared reference.
    *gen_id = GenerationId::new("gen-2").unwrap();
    //~^ ERROR cannot assign
}
