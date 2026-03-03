use owo_colors::OwoColorize;

fn main() {
  let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
  let target_family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();

  if target_family != "wasm" {
    println!(
      "{}",
      format!(
        "This crate only supports WebAssembly targets with unknown target family. \
             Got target family: '{}'. Expected: 'wasm'. An empty build will be created.",
        target_family
      )
      .red()
    );
  }

  if target_os != "unknown" {
    println!(
      "{}",
      format!(
        "This crate only supports WebAssembly targets. \
             Got target os: '{}'. Expected: 'unknown'. An empty build will be created.",
        target_os
      )
      .red()
    );
  }
}
