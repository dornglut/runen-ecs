// These compile tests are owned by ECS because they prove the ECS public
// Query/SystemParam lifetime and capability contracts directly. The engine's
// trybuild coverage is not a substitute for this package-local API boundary.
#[test]
fn command_lifetime_cannot_escape_system_invocation() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/system_param_command_escape.rs");
}

#[test]
fn shared_world_cannot_drive_mutable_direct_query() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/query_mutable_shared_world.rs");
}

#[test]
fn direct_query_world_source_matches_query_access() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/query_world_source_pass.rs");
    cases.compile_fail("tests/ui/query_mutable_shared_world_get.rs");
    cases.compile_fail("tests/ui/query_mutable_shared_world_single.rs");
    cases.compile_fail("tests/ui/query_mutable_tuple_shared_world.rs");
    cases.compile_fail("tests/ui/query_mutable_optional_shared_world.rs");
}

#[test]
fn system_param_derive_uses_explicit_world_and_state_roles() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/ui/system_param_derive_roles_pass.rs");
    cases.compile_fail("tests/ui/system_param_derive_unrelated_lifetime.rs");
    cases.compile_fail("tests/ui/system_param_derive_third_lifetime.rs");
    cases.compile_fail("tests/ui/system_param_derive_wrong_state_world.rs");
}

#[test]
fn invocation_bound_parameters_cannot_escape() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/system_param_res_mut_escape.rs");
    cases.compile_fail("tests/ui/system_param_query_escape.rs");
}
