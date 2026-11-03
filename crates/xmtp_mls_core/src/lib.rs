//! Vocabulary that every layer of the client shares: events, configuration,
//! policies, notification settings, metrics, preflight, and the durable tasks
//! that storage transactions enqueue. Code here names no type from a feature
//! module, so a later change can move this directory into its own crate.

pub(crate) mod api_status;
pub mod change_callbacks;
#[cfg(any(test, feature = "test-hooks"))]
#[doc(hidden)]
pub mod commit_lock;
pub mod config;
pub mod config_refresh;
pub mod events;
pub mod hmac_epoch;
pub mod metrics;
pub mod notifications;
pub(crate) mod policy;
pub(crate) mod preflight;
pub mod reconnect;
pub(crate) mod state_tx;
pub(crate) mod sync_tasks;
pub mod task_channels;
#[cfg(test)]
pub(crate) mod test_context;
pub mod version;

#[cfg(all(any(test, feature = "test-hooks"), not(target_arch = "wasm32")))]
#[doc(hidden)]
pub use state_tx::precommit_test_hook;
