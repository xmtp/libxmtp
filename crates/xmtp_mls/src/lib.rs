#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod attachments;
pub mod builder;
pub mod client;
pub mod context;
pub mod core;
mod definitions;
#[cfg(any(test, feature = "diagnostics"))]
pub mod diagnostics;
pub mod features;
pub mod groups;
pub mod identity;
pub mod identity_lifecycle;
pub mod identity_updates;
pub mod inbox_handles;
mod intents;
pub mod messages;
pub mod mls_store;
mod mutex_registry;
pub mod prelude;
pub mod server_configuration;
use crate::core::state_tx;
pub mod storage_location;
pub use client::VisibilityConfirmationOptions;
pub mod subscriptions;
pub mod task_handlers;
pub mod utils;
pub mod worker;
pub use definitions::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod test;

pub use client::{Client, Network};
pub use xmtp_common as common;
pub use xmtp_db as db;
#[cfg(test)]
use xmtp_db::DuplicateItem;
use xmtp_db::StorageError;
pub use xmtp_id::InboxOwner;
pub use xmtp_mls_common as mls_common;
pub use xmtp_mls_validation as mls_validation;
pub use xmtp_proto::api_client::*;

pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
