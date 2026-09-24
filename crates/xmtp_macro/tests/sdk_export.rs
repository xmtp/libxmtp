#[test]
fn callback_error_and_sdk_export_compile() {
    let cases = trybuild::TestCases::new();
    cases.compile_fail("tests/ui/callback_error_missing_from.rs");
    cases.pass("tests/ui/sdk_export_pass.rs");
}
