#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod builder;
pub mod client;
pub mod commit_lock;
pub mod context;
pub mod cursor_store;
mod definitions;
pub mod groups;
pub mod identity;
pub mod identity_updates;
mod intents;
pub mod messages;
pub mod mls_store;
mod mutex_registry;
pub mod subscriptions;
pub mod tasks;
pub mod utils;
pub mod verified_key_package_v2;
pub mod worker;
pub use definitions::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod test;
mod traits;

pub use client::{Client, Network};
pub use commit_lock::{CommitLockError, GroupCommitLock, MlsGroupGuard};
pub use xmtp_common as common;
pub use xmtp_db as db;
use xmtp_db::{DuplicateItem, StorageError};
pub use xmtp_id::InboxOwner;
pub use xmtp_mls_common as mls_common;
pub use xmtp_proto::api_client::*;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(all(test, target_arch = "wasm32"))]
wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor)]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
