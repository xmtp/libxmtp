pub mod mock;

#[cfg(not(target_arch = "wasm32"))]
mod native_tests;
