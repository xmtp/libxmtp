use std::error::Error;
use std::process::Command;
use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    Command::new("make")
        .args(["libxmtp-version"])
        .status()
        .expect("failed to make libxmtp-version");

    let build = BuildBuilder::all_build()?;
    let git = GixBuilder::default()
        .sha(true)
        .commit_timestamp(true)
        .build()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit()?;

    Ok(())
}
