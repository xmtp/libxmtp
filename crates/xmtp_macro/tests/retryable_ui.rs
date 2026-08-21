//! UI tests for the `#[derive(Retryable)]` macro's `compile_error!` paths.
//!
//! Each fixture in `tests/ui/` triggers exactly one rejection path; its
//! `.stderr` snapshot pins the spanned diagnostic. Regenerate snapshots after an
//! intentional message change with `TRYBUILD=overwrite cargo test -p xmtp_macro`.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn retryable_compile_fail() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/retryable/*.rs");
}
