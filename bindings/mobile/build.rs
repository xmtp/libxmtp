use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let target_family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    if target_family == "wasm" {
        return Ok(());
    }

    use std::process::Command;
    use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

    Command::new("make")
        .args(["libxmtp-version"])
        .status()
        .expect("failed to make libxmtp-version");

    let build = BuildBuilder::all_build()?;
    let git = GixBuilder::default()
        .sha(true)
        .branch(true)
        .commit_date(true)
        .commit_timestamp(true)
        .build()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit()?;

    Ok(())
}
