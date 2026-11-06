//! Identity code over core's `Identity`: creating an identity and its key
//! packages, identity updates and their resolution, inbox handles and key
//! package maintenance.

pub mod identity_ext;
pub mod identity_lifecycle;
pub mod identity_updates;
pub mod inbox_handles;
pub mod key_package_maintenance;

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
