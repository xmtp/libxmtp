use std::error::Error;
use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=NIX_GIT_SHA");

    if let Ok(sha) = std::env::var("NIX_GIT_SHA") {
        // Nix-driven build: short-circuit vergen and emit the SHA ourselves.
        println!("cargo:rustc-env=VERGEN_GIT_SHA={sha}");
    } else {
        let build = BuildBuilder::all_build()?;
        let gix = GixBuilder::default().branch(true).sha(true).build()?;
        Emitter::default()
            .add_instructions(&build)?
            .add_instructions(&gix)?
            .emit()?;
    }
    Ok(())
}
