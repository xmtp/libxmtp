//! The CI workspace tests (nix/package/nextest.nix) build with the `ci-test`
//! Cargo profile, which keeps release speed and turns on overflow checks and
//! debug assertions in the workspace crates. These tests fail when a build
//! that names that profile has the checks off. Other builds skip them.

fn ci_test_profile() -> bool {
    option_env!("CARGO_PROFILE") == Some("ci-test")
}

#[xmtp_common::test]
fn ci_test_profile_checks_overflow() {
    if !ci_test_profile() {
        return;
    }
    let sum = std::panic::catch_unwind(|| std::hint::black_box(u8::MAX) + std::hint::black_box(1));
    assert!(
        sum.is_err(),
        "the ci-test profile must check integer overflow"
    );
}

#[xmtp_common::test]
fn ci_test_profile_checks_debug_assertions() {
    if !ci_test_profile() {
        return;
    }
    assert!(
        cfg!(debug_assertions),
        "the ci-test profile must enable debug assertions"
    );
}
