//! Spec 006 §7 through the mobile binding (CFG-106).
//!
//! The values a deployment publishes are covered in `xmtp_mls`. What is covered
//! here is the translation: every field of the snapshot the client holds
//! reaches the uniffi record, and the static fetch of CFG-081 reads the same
//! deployment with no database, no client, and no credential.

use super::*;
use crate::server_configuration::{FfiServerConfiguration, fetch_server_configuration};
use xmtp_configuration::{ServerConfiguration, backend_test_url};

/// Read every field of the record and assert it carries what the client holds.
///
/// Every `usize` the client keeps is published as a `u64` (§7), so the
/// comparison widens the client side rather than narrowing the record.
fn assert_mirrors(ffi: &FfiServerConfiguration, core: &ServerConfiguration) {
    assert_eq!(ffi.identifier, core.identifier);
    assert_eq!(ffi.server_version, core.server_version);
    assert_eq!(ffi.min_libxmtp_version, core.min_libxmtp_version);
    assert_eq!(
        ffi.smart_contract_wallet_chains,
        core.smart_contract_wallet_chains
    );

    assert_eq!(ffi.auth.enabled, core.auth.enabled);
    assert_eq!(ffi.auth.audiences, core.auth.audiences);
    assert_eq!(ffi.auth.issuers, core.auth.issuers);
    assert_eq!(ffi.auth.required_scopes, core.auth.required_scopes);
    assert_eq!(ffi.auth.keys.len(), core.auth.keys.len());
    for (key, expected) in ffi.auth.keys.iter().zip(core.auth.keys.iter()) {
        assert_eq!(key.kid, expected.kid);
        assert_eq!(key.alg, expected.alg);
    }

    assert_eq!(
        ffi.retention.group_message_seconds,
        core.retention.group_message_seconds
    );
    assert_eq!(
        ffi.retention.welcome_seconds,
        core.retention.welcome_seconds
    );
    assert_eq!(
        ffi.retention.key_package_seconds,
        core.retention.key_package_seconds
    );

    assert_eq!(
        ffi.limits.max_envelope_bytes,
        core.limits.max_envelope_bytes as u64
    );
    assert_eq!(
        ffi.limits.max_request_bytes,
        core.limits.max_request_bytes as u64
    );
    assert_eq!(
        ffi.limits.max_response_bytes,
        core.limits.max_response_bytes as u64
    );
    assert_eq!(
        ffi.limits.max_publish_topics,
        core.limits.max_publish_topics as u64
    );
    assert_eq!(
        ffi.limits.max_query_topics,
        core.limits.max_query_topics as u64
    );
    assert_eq!(
        ffi.limits.max_query_limit,
        core.limits.max_query_limit as u64
    );
    assert_eq!(
        ffi.limits.default_query_limit,
        core.limits.default_query_limit as u64
    );
    assert_eq!(
        ffi.limits.max_newest_metadata_topics,
        core.limits.max_newest_metadata_topics as u64
    );
    assert_eq!(
        ffi.limits.max_newest_full_topics,
        core.limits.max_newest_full_topics as u64
    );
    assert_eq!(
        ffi.limits.max_update_adds,
        core.limits.max_update_adds as u64
    );
    assert_eq!(
        ffi.limits.max_update_removes,
        core.limits.max_update_removes as u64
    );
    assert_eq!(
        ffi.limits.max_stream_topics,
        core.limits.max_stream_topics as u64
    );
    assert_eq!(
        ffi.limits.max_static_topics,
        core.limits.max_static_topics as u64
    );
    assert_eq!(
        ffi.limits.max_lookup_identifiers,
        core.limits.max_lookup_identifiers as u64
    );
    assert_eq!(
        ffi.limits.max_scw_signatures,
        core.limits.max_scw_signatures as u64
    );
    assert_eq!(
        ffi.limits.max_identity_entries,
        core.limits.max_identity_entries as u64
    );
    assert_eq!(
        ffi.limits.max_update_frames_per_second,
        core.limits.max_update_frames_per_second
    );
    assert_eq!(ffi.limits.max_update_burst, core.limits.max_update_burst);
    assert_eq!(
        ffi.limits.max_ping_frames_per_second,
        core.limits.max_ping_frames_per_second
    );
    assert_eq!(ffi.limits.max_ping_burst, core.limits.max_ping_burst);

    assert_eq!(ffi.mls.max_group_members, core.mls.max_group_members as u64);
    assert_eq!(
        ffi.mls.max_installations_per_inbox,
        core.mls.max_installations_per_inbox as u64
    );
    assert_eq!(ffi.mls.commit_log_enabled, core.mls.commit_log_enabled);
}

// CFG-080: the snapshot the client resolved at build reaches the record whole.
#[xmtp_common::test(unwrap_try = true)]
async fn server_configuration_exposes_every_published_field() {
    let alix = Tester::new().await;

    let published = alix.server_configuration();
    assert_mirrors(&published, alix.inner_client.server_configuration());

    // The shared backend is a real deployment, so the snapshot is a real answer
    // rather than the compiled fallback.
    assert!(!published.identifier.is_empty());
    assert!(!published.server_version.is_empty());
    assert!(published.limits.max_envelope_bytes > 0);
    assert!(published.limits.max_query_limit > 0);
    assert!(published.mls.max_group_members > 0);
    assert!(!published.smart_contract_wallet_chains.is_empty());
}

// CFG-081: the static fetch reads the same deployment with no database, no
// client, and no credential.
#[xmtp_common::test(unwrap_try = true)]
async fn fetch_server_configuration_reads_the_shared_backend() {
    let fetched = fetch_server_configuration(backend_test_url(), None).await?;

    assert!(!fetched.identifier.is_empty());
    assert!(!fetched.server_version.is_empty());

    // The same deployment a client binds to answered.
    let alix = Tester::new().await;
    assert_eq!(fetched, alix.server_configuration());
}

// CFG-082: an explicit refresh returns what the backend answered now, and the
// snapshot the running client holds is unchanged.
#[xmtp_common::test(unwrap_try = true)]
async fn refresh_server_configuration_returns_the_fetched_copy() {
    let alix = Tester::new().await;
    let snapshot = alix.server_configuration();

    let refreshed = alix.refresh_server_configuration().await?;

    assert_eq!(refreshed.identifier, snapshot.identifier);
    assert_eq!(refreshed, snapshot);
    assert_eq!(alix.server_configuration(), snapshot);
}
