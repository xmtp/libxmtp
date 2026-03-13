extern crate napi_build;

fn main() {
  let target_family = std::env::var("CARGO_CFG_TARGET_FAMILY").unwrap_or_default();
  if target_family != "wasm" {
    napi_build::setup();
  }
}
