fn main() {
  let target = std::env::var("TARGET").unwrap_or_default();
  // if cfg!(target_os = "macos") && target == "wasm32-unknown-unknown" {
    // println!("cargo:rustc-link-arg=--linker=wasm-ld");
  // }
}
