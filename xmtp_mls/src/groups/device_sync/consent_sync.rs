use super::*;
use crate::{
    groups::scoped_client::ScopedGroupClient,
    storage::consent_record::{ConsentState, ConsentType},
    Client, XmtpApi,
};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::xmtp::mls::message_contents::{
    ConsentEntityType, ConsentState as ConsentStateProto, ConsentUpdate as ConsentUpdateProto,
};

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub(crate) async fn stream_consent_update(
        &self,
        provider: &XmtpOpenMlsProvider,
        record: &StoredConsentRecord,
    ) -> Result<(), DeviceSyncError> {
        let conn = provider.conn_ref();

        let consent_update_proto = ConsentUpdateProto {
            entity: record.entity.clone(),
            entity_type: match record.entity_type {
                ConsentType::Address => ConsentEntityType::Address,
                ConsentType::ConversationId => ConsentEntityType::ConversationId,
                ConsentType::InboxId => ConsentEntityType::InboxId,
            } as i32,
            state: match record.state {
                ConsentState::Allowed => ConsentStateProto::Allowed,
                ConsentState::Denied => ConsentStateProto::Denied,
                ConsentState::Unknown => ConsentStateProto::Unspecified,
            } as i32,
        };

        let sync_group = self.ensure_sync_group(provider).await?;
        let content_bytes = serde_json::to_vec(&consent_update_proto)?;
        sync_group.prepare_message(&content_bytes, conn, |_time_ns| PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: new_request_id(),
                message_type: Some(MessageType::ConsentUpdate(consent_update_proto)),
            })),
        })?;

        sync_group.publish_messages().await?;

        Ok(())
    }

    pub(super) fn syncable_consent_records(
        &self,
        conn: &DbConnection,
    ) -> Result<Vec<Syncable>, DeviceSyncError> {
        let consent_records = conn
            .consent_records()?
            .into_iter()
            .map(Syncable::ConsentRecord)
            .collect();
        Ok(consent_records)
    }
}

#[cfg(all(not(target_arch = "wasm32"), test))]
pub(crate) mod tests {
    const HISTORY_SERVER_HOST: &str = "localhost";
    const HISTORY_SERVER_PORT: u16 = 5558;

    use std::time::{Duration, Instant};

    use super::*;
    use crate::{
        assert_ok,
        builder::ClientBuilder,
        storage::consent_record::{ConsentState, ConsentType},
    };
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
    async fn test_consent_sync() {
        let history_sync_url = format!("http://{}:{}", HISTORY_SERVER_HOST, HISTORY_SERVER_PORT);

        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client_with_history(&wallet, &history_sync_url).await;

        let amal_a_provider = amal_a.mls_provider().unwrap();
        let amal_a_conn = amal_a_provider.conn_ref();

        // create an alix installation and consent with alix
        let alix_wallet = generate_local_wallet();
        let consent_record = StoredConsentRecord::new(
            ConsentType::Address,
            ConsentState::Allowed,
            alix_wallet.get_address(),
        );
        amal_a.set_consent_states(&[consent_record]).await.unwrap();

        // Ensure that consent record now exists.
        let syncable_consent_records = amal_a.syncable_consent_records(amal_a_conn).unwrap();
        assert_eq!(syncable_consent_records.len(), 1);

        // Create a second installation for amal with sync.
        let amal_b = ClientBuilder::new_test_client_with_history(&wallet, &history_sync_url).await;
        let amal_b_provider = amal_b.mls_provider().unwrap();
        let amal_b_conn = amal_b_provider.conn_ref();

        let consent_records_b = amal_b.syncable_consent_records(amal_b_conn).unwrap();
        assert_eq!(consent_records_b.len(), 0);

        let old_group_id = amal_a.get_sync_group().unwrap().group_id;
        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a.sync_welcomes(amal_a_conn).await.unwrap();
        let new_group_id = amal_a.get_sync_group().unwrap().group_id;
        // group id should have changed to the new sync group created by the second installation
        assert_ne!(old_group_id, new_group_id);

        let consent_a = amal_a.syncable_consent_records(amal_a_conn).unwrap().len();

        // Have amal_a receive the message (and auto-process)
        let amal_a_sync_group = amal_a.get_sync_group().unwrap();
        assert_ok!(amal_a_sync_group.sync_with_conn(&amal_a_provider).await);

        // Wait for up to 3 seconds for the reply on amal_b (usually is almost instant)
        let start = Instant::now();
        let mut reply = None;
        while reply.is_none() {
            reply = amal_b
                .get_latest_sync_reply(&amal_b_provider, DeviceSyncKind::Consent)
                .await
                .unwrap();
            if start.elapsed() > Duration::from_secs(3) {
                panic!("Did not receive consent reply.");
            }
        }

        // Wait up to 3 seconds for sync to process (typically is almost instant)
        let mut consent_b = 0;
        let start = Instant::now();
        while consent_b != consent_a {
            consent_b = amal_b.syncable_consent_records(amal_b_conn).unwrap().len();

            if start.elapsed() > Duration::from_secs(3) {
                panic!("Consent sync did not work. Consent: {consent_b}/{consent_a}");
            }
        }
    }
}
