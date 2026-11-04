//! Device-sync suites that build clients. They reach device sync from
//! outside the crate, through the facade, so the device-sync crate takes them
//! along when it moves out.

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(not(target_arch = "wasm32"))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}

mod archive;
mod tests;
