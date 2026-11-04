use prost::Message as _;
use xmtp_common::{NS_IN_DAY, NS_IN_HOUR, time::now_ns};
use xmtp_cryptography::secret::Zeroizing;
use xmtp_db::{
    ConnectionExt, StorageError,
    consent_record::{
        ConsentKey, ConsentState, ConsentType, MAX_AHEAD_NS, Owed, StoredConsentRecord,
    },
    group::{ConversationType, GroupMembershipState},
    prelude::*,
    sync_outbox::has_owed,
    user_preferences::StoredUserPreferences,
};
#[cfg(not(target_arch = "wasm32"))]
use xmtp_logging::{LogRecord, LogSinkTarget, SinkError};
use xmtp_mls::client::ConsentAct;
use xmtp_mls::prelude::*;
use xmtp_mls::tester;
use xmtp_mls::utils::test::device_sync::{add_task_group_ids, decide_at, held, publish, receive};
#[cfg(not(target_arch = "wasm32"))]
use xmtp_mls::worker::device_sync::test_support::log_incoming_preference_updates;
use xmtp_mls::worker::device_sync::{
    test_support::{
        PAGE_BYTES, PAGE_UPDATES, decode_supported_content, decode_sync_message, sync_message_bytes,
    },
    worker::SyncMetric,
    *,
};
use xmtp_mls_core::sync_tasks::schedule_add_missing_installations;
use xmtp_mls_engine::groups::send_message_opts::SendMessageOpts;
use xmtp_mls_engine::groups::test_support::crafted_welcome::assert_welcome_rejected;
use xmtp_mls_engine::groups::{GroupError, MlsGroup};
use xmtp_proto::types::GroupId;
use xmtp_proto::xmtp::device_sync::content::{
    HmacKeyUpdate, PreferenceUpdate as PreferenceUpdateMessage, PreferenceUpdates,
    device_sync_content::Content as ContentProto,
    preference_update::Update as PreferenceUpdateProto,
};
use xmtp_proto::xmtp::mls::database::task::Task as TaskKindProto;

mod consent_and_preferences;
mod foreign_inbox;
mod installation_tasks;
#[cfg(not(target_arch = "wasm32"))]
mod log_redaction;
mod newcomer_catch_up;
mod publication;

/// `newer` creates a sync group that holds every installation of the inbox.
/// Each of `others` installs its Welcome and sends what it owes there, and
/// every installation applies what arrived.
async fn join_sync_group(
    newer: &xmtp_mls::utils::Tester,
    others: &[&xmtp_mls::utils::Tester],
) -> Result<GroupId, Box<dyn std::error::Error>> {
    let group = newer.device_sync_client().get_sync_group().await?;
    for other in others {
        other.sync_welcomes().await?;
        publish(other).await?;
    }
    publish(newer).await?;
    for client in others.iter().copied().chain([newer]) {
        receive(client).await?;
    }
    Ok(group.group_id)
}

fn consent_update(
    entity_type: ConsentType,
    state: ConsentState,
    entity: &str,
) -> PreferenceUpdateMessage {
    PreferenceUpdateMessage {
        update: Some(PreferenceUpdateProto::Consent(
            StoredConsentRecord::new(entity_type, state, entity.into()).into(),
        )),
    }
}

fn root_key_update(key: Vec<u8>, cycled_at_ns: i64) -> PreferenceUpdateMessage {
    PreferenceUpdateMessage {
        update: Some(PreferenceUpdateProto::Hmac(HmacKeyUpdate {
            key,
            cycled_at_ns,
        })),
    }
}

fn sync_bytes(updates: Vec<PreferenceUpdateMessage>) -> Zeroizing<Vec<u8>> {
    sync_message_bytes(ContentProto::PreferenceUpdates(PreferenceUpdates {
        updates,
    }))
}

/// Revoke `revoked`'s installation with `owner`'s wallet signature.
pub(super) async fn revoke(
    owner: &xmtp_mls::utils::Tester,
    revoked: &xmtp_mls::utils::Tester,
) -> Result<(), xmtp_mls_identity::identity_updates::IdentityUpdateError> {
    use xmtp_id::associations::test_utils::add_wallet_signature;

    let identity = owner.identity_updates();
    let mut request = identity
        .revoke_installations(vec![revoked.context.installation_id().to_vec()])
        .await?;
    add_wallet_signature(&mut request, &owner.builder.owner).await;
    identity.apply_signature_request(request).await
}

// These fixtures use native pool errors. Browser connection lifecycle tests
// cover closed persistent handles in the WASM database module.
#[cfg(not(target_arch = "wasm32"))]
mod disconnect_propagation {
    //! Pins that a dropped-pool signal survives `DeviceSyncError`, so the sync
    //! worker's `needs_db_reconnect()` stays `true`.
    use xmtp_db::{ConnectionError, PlatformStorageError, StorageError};
    use xmtp_mls::worker::device_sync::DeviceSyncError;
    use xmtp_mls_core::mls_store::MlsStoreError;
    use xmtp_mls_core::reconnect::NeedsDbReconnect;
    use xmtp_mls_engine::groups::GroupError;

    /// A `StorageError` that signals the connection pool was dropped.
    fn disconnect_storage() -> StorageError {
        StorageError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A `ConnectionError` that signals the connection pool was dropped.
    fn disconnect_connection() -> ConnectionError {
        ConnectionError::Platform(PlatformStorageError::PoolNeedsConnection)
    }

    /// A storage error that is NOT a disconnect — must never trip the contract.
    fn benign_storage() -> StorageError {
        StorageError::InvalidHmacLength
    }

    #[xmtp_common::test]
    fn device_sync_error_forwards_disconnect() {
        assert!(DeviceSyncError::Storage(disconnect_storage()).needs_db_reconnect());
        assert!(DeviceSyncError::Db(disconnect_connection()).needs_db_reconnect());
        assert!(
            DeviceSyncError::Group(GroupError::Storage(disconnect_storage())).needs_db_reconnect()
        );
        assert!(
            DeviceSyncError::MlsStore(MlsStoreError::Connection(disconnect_connection()))
                .needs_db_reconnect()
        );
        assert!(!DeviceSyncError::Storage(benign_storage()).needs_db_reconnect());
        assert!(!DeviceSyncError::InvalidPayload.needs_db_reconnect());
    }
}
