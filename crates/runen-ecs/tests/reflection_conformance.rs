#[test]
fn reflection_macro_conformance() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/reflect_generic.rs");
    cases.pass("tests/ui/reflect_const_generic.rs");
    cases.pass("tests/ui/reflect_where_clause.rs");
    cases.compile_fail("tests/ui/reflect_borrowed.rs");
    cases.compile_fail("tests/ui/reflect_union.rs");
    cases.compile_fail("tests/ui/removed_reflection_roles.rs");
    cases.compile_fail("tests/ui/removed_reflection_resource_role.rs");
    cases.compile_fail("tests/ui/removed_reflection_world_apis.rs");
}
