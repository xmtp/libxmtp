pub mod mock;

#[cfg(not(target_arch = "wasm32"))]
mod native_tests;

#[ctor::ctor]
fn _setup() {
    let _ = xmtp_common::logger();
}
