//! End-to-end integration test for `xdbg healthcheck`.
//!
//! Requires `dev/up` (Docker XMTP node) running locally. The test is
//! `#[ignore]` so plain `cargo test -p xdbg` doesn't try to run it. Invoke
//! with `cargo test -p xdbg --test healthcheck -- --ignored`.

use std::process::Command;

#[test]
#[ignore = "requires `dev/up` running locally"]
fn healthcheck_passes_on_local_backend() {
    let xdbg = env!("CARGO_BIN_EXE_xdbg");
    let tmp = tempfile::tempdir().expect("tempdir");

    let run = || {
        Command::new(xdbg)
            .env("XDBG_DB_ROOT", tmp.path())
            .args(["--backend", "local", "healthcheck"])
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
