use std::process::Command;

#[xmtp_common::test(unwrap_try = true)]
fn api_key_generation_is_offline_and_does_not_create_local_state() {
    let directory = tempfile::tempdir()?;
    let mut keys = Vec::new();
    for _ in 0..2 {
        let output = Command::new(env!("CARGO_BIN_EXE_xdbg"))
            .arg("generate-api-key")
            .env("XDBG_DB_ROOT", directory.path())
            .output()?;
        assert!(output.status.success());
        assert!(output.stderr.is_empty());
        let key = String::from_utf8(output.stdout)?;
        assert_eq!(key.len(), 65);
        assert!(key.ends_with('\n'));
        assert_eq!(hex::decode(key.trim())?.len(), 32);
        keys.push(key);
    }
    assert_ne!(keys[0], keys[1]);
    assert_eq!(std::fs::read_dir(directory.path())?.count(), 0);
}

#[xmtp_common::test(unwrap_try = true)]
fn network_commands_still_require_a_url() {
    let output = Command::new(env!("CARGO_BIN_EXE_xdbg"))
        .args(["info", "--app"])
        .output()?;
    assert!(!output.status.success());
    assert!(String::from_utf8(output.stderr)?.contains("--url is required"));
}
