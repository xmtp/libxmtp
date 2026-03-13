#![recursion_limit = "256"]

xmtp_common::if_native! {
    mod cached_signature_verifier;
    mod config;
    mod handlers;
    mod health_check;
    mod version;
    mod native;

    pub use native::*;

    #[macro_use]
    extern crate tracing;
}

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
fn main() {
    native_main().unwrap();
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
fn main() {}
