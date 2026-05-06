use std::error::Error;
use vergen_gix::{BuildBuilder, Emitter, GixBuilder};

fn main() -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-env-changed=NIX_GIT_SHA");
    println!("cargo:rerun-if-env-changed=NIX_GIT_COMMIT_DATE");

    let nix_sha = std::env::var("NIX_GIT_SHA").ok();
    let nix_date = std::env::var("NIX_GIT_COMMIT_DATE").ok();

    if let Some(sha) = &nix_sha {
        println!("cargo:rustc-env=VERGEN_GIT_SHA={sha}");
    }
    if let Some(date) = &nix_date {
        println!("cargo:rustc-env=VERGEN_GIT_COMMIT_DATE={date}");
    }

    let build = BuildBuilder::all_build()?;
    let git = GixBuilder::default()
        .sha(nix_sha.is_none())
        .branch(true)
        .commit_date(nix_date.is_none())
        .commit_timestamp(true)
        .build()?;

    Emitter::default()
        .add_instructions(&build)?
        .add_instructions(&git)?
        .emit()?;
    Ok(())
}
