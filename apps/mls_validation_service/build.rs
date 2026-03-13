use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let target_family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
    if target_family == "wasm" {
        return Ok(());
    }

    use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

    let build = BuildBuilder::all_build()?;
    let git = GixBuilder::default().branch(true).sha(true).build()?;
    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit()?;
    Ok(())
}
