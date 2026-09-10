#[test]
fn downstream_contract_boundaries() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/sealed_query_spec.rs");
}
