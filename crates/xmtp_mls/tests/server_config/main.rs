//! Server configuration suites that use only the public API: the published
//! configuration as a client applies it, and an unauthenticated read.

mod server_configuration;

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(not(target_arch = "wasm32"))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
