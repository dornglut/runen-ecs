#[test]
fn structural_mutation_is_excluded_while_segments_live() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/contiguous_structural_mutation.rs");
    cases.compile_fail("tests/ui/contiguous_slice_blocks_structure.rs");
    cases.compile_fail("tests/ui/contiguous_segment_cannot_be_forged.rs");
}
