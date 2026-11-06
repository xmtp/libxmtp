//! Leaves of the engine suites that need the client: its streams, workers,
//! device sync, inbox handles, startup, error type or test fixtures. Each
//! file holds the leaves of one suite.

mod deadlines;
mod groups_tests;
mod message_list;
#[cfg(not(target_arch = "wasm32"))]
mod oneshot;
mod post_quantum_interop;
mod receive;
mod recovery;
mod subscriptions;
#[cfg(not(target_arch = "wasm32"))]
mod test_bad_key_packages;
mod test_commit_log_local;
mod test_commit_log_remote;
#[cfg(not(target_arch = "wasm32"))]
mod test_configuration;
mod test_consent;
mod test_delete_message;
mod test_dictionary_creation;
mod test_dm;
#[cfg(not(target_arch = "wasm32"))]
mod test_failed_installations;
mod test_group_id;
mod test_inbox_handles;
#[cfg(not(target_arch = "wasm32"))]
mod test_legacy_placeholder_topics;
mod test_membership;
mod test_message_deletion_lifetime;
mod test_metadata_fields;
mod test_metadata_permissions;
mod test_min_version;
#[cfg(not(target_arch = "wasm32"))]
mod test_network;
mod test_proposals_app_data;
mod test_proposals_pause;
mod test_proposals_permissions;
#[cfg(not(target_arch = "wasm32"))]
mod test_rate_limits;
mod test_self_removal;
mod test_send_receive;
mod test_sync_concurrency;
mod test_welcome_pointers;
mod test_welcomes;
mod uncommittable_proposals;
