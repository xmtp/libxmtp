//! Push notifications: the client registration and settings, and the task
//! that keeps the backend registration current.
#![warn(clippy::unwrap_used)]

pub mod client;
pub mod worker;

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
