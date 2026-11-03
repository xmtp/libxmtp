//! Core suites that build clients. They reach the client from outside the
//! crate, so the core crate takes them along when it moves out.

#[cfg(not(target_arch = "wasm32"))]
mod preflight;

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(not(target_arch = "wasm32"))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
