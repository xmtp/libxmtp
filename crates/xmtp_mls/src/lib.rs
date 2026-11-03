#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub mod attachments;
pub mod builder;
pub mod client;
/// The client context, at its old path.
pub mod context {
    pub use crate::features::XmtpMlsLocalContext;
    pub use crate::groups::context::XmtpSharedContext;
    pub use xmtp_mls_core::context::*;
}
/// The core crate, at its old path.
pub mod core {
    pub use xmtp_mls_core::*;
}
mod definitions;
#[cfg(any(test, feature = "diagnostics"))]
pub mod diagnostics;
pub mod features;
pub mod groups;
/// Core's identity with the identity crate's items, at their old paths.
pub mod identity {
    pub use xmtp_mls_core::identity::*;
    pub use xmtp_mls_identity::identity_ext::*;
    #[cfg(any(test, feature = "test-hooks"))]
    pub use xmtp_mls_identity::identity_lifecycle::ENABLE_WELCOME_POINTERS;
    #[cfg(any(test, feature = "test-hooks"))]
    #[doc(hidden)]
    pub use xmtp_mls_identity::identity_lifecycle::WelcomePointersHook;
    pub use xmtp_mls_identity::identity_lifecycle::{
        IdentityStrategy, XmtpKeyPackage, XmtpKeyPackageBuilder,
    };
}
pub use xmtp_mls_identity::{identity_lifecycle, identity_updates, inbox_handles};
mod intents;
pub use xmtp_mls_core::mls_store;
use xmtp_mls_core::mutex_registry;
pub use xmtp_mls_messages as messages;
pub mod prelude;
/// The deployment's published configuration, at its old path.
pub mod server_configuration {
    pub use crate::core::config_refresh as worker;
    pub use xmtp_mls_server_config::server_configuration::*;
}
use crate::core::state_tx;
pub use client::VisibilityConfirmationOptions;
pub use xmtp_mls_server_config::storage_location;
pub mod subscriptions;
pub mod task_handlers;
pub mod utils;
/// The core worker framework with the feature workers, at their old paths.
pub mod worker {
    pub mod device_sync;
    pub use xmtp_mls_core::worker::*;
    pub use xmtp_mls_identity::key_package_maintenance;
    pub(crate) mod notifications;
}
#[cfg(test)]
mod worker_tests;
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
