#![recursion_limit = "256"]
#![warn(clippy::unwrap_used)]

pub use xmtp_mls_attachments as attachments;
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
pub use xmtp_mls_engine::groups;
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
pub use xmtp_mls_core::mls_store;
use xmtp_mls_core::mutex_registry;
pub use xmtp_mls_identity::{identity_lifecycle, identity_updates, inbox_handles};
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
/// The streams crate with the engine's receiver modules, at their old paths.
pub mod subscriptions {
    #[doc(hidden)]
    pub use xmtp_mls_core::events as internal;
    pub use xmtp_mls_engine::subscriptions::{barrier, incoming};
    pub use xmtp_mls_streams::*;

    // A message stream against a backend that charges stream rent.
    #[cfg(all(test, not(target_arch = "wasm32")))]
    mod rate_limit_tests;
}
pub mod task_handlers;
pub mod utils;
/// The core worker framework with the feature workers, at their old paths.
pub mod worker {
    pub use xmtp_mls_core::worker::*;
    pub use xmtp_mls_device_sync as device_sync;
    pub use xmtp_mls_identity::key_package_maintenance;
    pub(crate) use xmtp_mls_notifications::worker as notifications;
}
#[cfg(test)]
mod worker_tests;
pub use definitions::*;

#[cfg(any(test, feature = "test-utils"))]
pub mod test;

pub use client::{Client, Network};
pub use xmtp_common as common;
pub use xmtp_db as db;
use xmtp_db::StorageError;
pub use xmtp_id::InboxOwner;
/// The old `xmtp_mls_common` paths, now served by the three crates it split into.
pub mod mls_common {
    pub use xmtp_mls_group_keys::{commit_log, group_keys, mls_ext};
    pub use xmtp_mls_group_state::{
        app_data, group, group_metadata, group_mutable_metadata, inbox_id, libxmtp_version,
    };
    pub use xmtp_mls_tls_collections::{tls_map, tls_set};
}
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
