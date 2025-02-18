fn main() {
  let target = std::env::var("TARGET").unwrap_or_default();
  if cfg!(target_os = "macos") && target == "wasm32-unknown-unknown" {
    println!("cargo:rustc-link-arg=--linker=wasm-ld");
    println!("cargo:rustc-env=CC_wasm32-unknown-unknown=/opt/homebrew/opt/llvm/bin/clang");
  }
}
