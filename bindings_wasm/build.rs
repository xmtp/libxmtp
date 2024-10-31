use std::env;
// use std::process::Command;

fn main() {
  let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap();
  if target_arch != "wasm32" {
    // Emit a compile error if the target is not wasm32-unknown-unknown
    panic!("This crate only supports the wasm32 architecture");
  }
}
