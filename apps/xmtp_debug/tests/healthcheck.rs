//! End-to-end integration test for `xdbg healthcheck`.
//!
//! Requires `just backend up` and `XMTP_BACKEND_URL`. The test is
//! `#[ignore]` so plain `cargo test -p xdbg` doesn't try to run it. Invoke
//! with `dev/nix-shell 'cargo test -p xdbg --test healthcheck -- --ignored'`.

use std::process::Command;

#[test]
#[ignore = "requires `dev/up` running locally"]
fn healthcheck_passes_on_local_backend() {
    let xdbg = env!("CARGO_BIN_EXE_xdbg");
    let tmp = tempfile::tempdir().expect("tempdir");

    let url = std::env::var("XMTP_BACKEND_URL").expect("XMTP_BACKEND_URL is required");
    let run = || {
        Command::new(xdbg)
            .env("XDBG_DB_ROOT", tmp.path())
            .args(["--url", url.as_str(), "healthcheck"])
            .status()
            .expect("spawn xdbg")
    };

    let first = run();
    assert!(first.success(), "first healthcheck run failed: {first:?}");

    let second = run();
    assert!(
        second.success(),
        "second healthcheck run failed: {second:?}"
    );
}
