//! Vocabulary that every layer of the client shares: events, configuration,
//! policies, notification settings, metrics, preflight, and the durable tasks
//! that storage transactions enqueue, plus the client context, the local
//! identity, the MLS store and the worker runner. Code here names no type
//! from the identity, engine or feature crates.

pub mod api_status;
pub mod change_callbacks;
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
pub mod commit_lock;
pub mod config;
pub mod config_refresh;
pub mod context;
pub mod events;
pub mod hmac_epoch;
pub mod identity;
pub mod metrics;
pub mod mls_store;
pub mod mutex_registry;
pub mod notifications;
pub mod policy;
pub mod preflight;
pub mod reconnect;
pub mod state_tx;
pub mod sync_tasks;
pub mod task_channels;
#[cfg(test)]
pub(crate) mod test_context;
pub mod version;
pub mod worker;

#[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
#[doc(hidden)]
pub use state_tx::precommit_test_hook;

#[cfg_attr(not(target_arch = "wasm32"), ctor::ctor(unsafe))]
#[cfg(all(test, not(target_arch = "wasm32")))]
fn test_setup() {
    xmtp_common::logger();
    let _ = fdlimit::raise_fd_limit();
}
