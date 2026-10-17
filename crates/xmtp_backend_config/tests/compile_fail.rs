//! A `ValidatedConfig`, and a section it hands out, come only from
//! validation, and neither can be changed afterwards.

#[xmtp_common::test(unwrap_try = true)]
fn validated_configuration_comes_only_from_validation() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/validated_config_literal.rs");
    cases.compile_fail("tests/ui/validated_config_assignment.rs");
    cases.compile_fail("tests/ui/validated_section_literal.rs");
}
