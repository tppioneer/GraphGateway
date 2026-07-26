// P103-R1 compile-fail evidence: callers cannot mutate ViewMember or
// ResolvedGraphView fields in place after construction.
//
// Each .rs file in tests/compile-fail/ encodes one violation attempt.
// trybuild compiles each one and asserts that it fails with the
// expected error annotation.

#[test]
fn compile_fail_view_immutability() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/compile-fail/view_member_mutate_generation.rs");
    t.compile_fail("tests/compile-fail/view_member_mutate_endpoint.rs");
    t.compile_fail("tests/compile-fail/view_member_mutate_capability.rs");
    t.compile_fail("tests/compile-fail/view_mutate_members.rs");
    t.compile_fail("tests/compile-fail/view_assign_private_field.rs");
}
