use std::process::Command;

#[xmtp_common::test(unwrap_try = true)]
fn version_reports_the_run_time_git_sha() {
    let directory = tempfile::tempdir()?;
    let output = Command::new(env!("CARGO_BIN_EXE_xdbg"))
        .arg("--version")
        .env("XDBG_DB_ROOT", directory.path())
        .env("XDBG_GIT_SHA", "0123abcd")
        .output()?;
    assert!(output.status.success());
    let text = String::from_utf8(output.stdout)? + &String::from_utf8(output.stderr)?;
    let expected = format!("{}-0123abcd", env!("CARGO_PKG_VERSION"));
    assert!(text.contains(&expected), "{expected} not in {text}");
}
