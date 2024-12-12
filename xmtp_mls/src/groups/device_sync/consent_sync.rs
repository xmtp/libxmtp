use super::*;
use crate::{Client, XmtpApi};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::xmtp::mls::message_contents::UserPreferenceUpdate as UserPreferenceUpdateProto;

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub(crate) async fn send_consent_update(
        &self,
        provider: &XmtpOpenMlsProvider,
        record: StoredConsentRecord,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!(
            inbox_id = self.inbox_id(),
            installation_id = hex::encode(self.installation_public_key()),
            "Streaming consent update. {:?}",
            record
        );

        let sync_group = self.ensure_sync_group(provider).await?;
        let update_proto: UserPreferenceUpdateProto = UserPreferenceUpdate::ConsentUpdate(record)
            .try_into()
            .map_err(|e| DeviceSyncError::Bincode(format!("{e:?}")))?;
        let content_bytes = serde_json::to_vec(&update_proto)?;
        sync_group.prepare_message(&content_bytes, provider, |_time_ns| PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: new_request_id(),
                message_type: Some(MessageType::UserPreferenceUpdate(update_proto)),
            })),
        })?;

        sync_group.sync_until_last_intent_resolved(provider).await?;

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

#[cfg(test)]
pub(crate) mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use wasm_bindgen_test::wasm_bindgen_test;

    const HISTORY_SERVER_HOST: &str = "localhost";
    const HISTORY_SERVER_PORT: u16 = 5558;

    use xmtp_common::{
        assert_ok,
        time::{Duration, Instant},
    };

    use super::*;
    use crate::{
        builder::ClientBuilder,
        groups::scoped_client::ScopedGroupClient,
        storage::consent_record::{ConsentState, ConsentType},
        utils::test::wait_for_min_intents,
    };
    use xmtp_cryptography::utils::generate_local_wallet;
    use xmtp_id::InboxOwner;

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 1))]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_consent_sync() {
        xmtp_common::logger();
        let history_sync_url = format!("http://{}:{}", HISTORY_SERVER_HOST, HISTORY_SERVER_PORT);
        let wallet = generate_local_wallet();
        let amal_a = ClientBuilder::new_test_client_with_history(&wallet, &history_sync_url).await;

        let amal_a_provider = amal_a.mls_provider().unwrap();
        let amal_a_conn = amal_a_provider.conn_ref();
        wait_for_min_intents(amal_a_conn, 3).await;

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
        // make sure amal's workers have time to sync
        // 3 Intents:
        //  1.) UpdateGroupMembership Intent for new sync group
        //  2.) Device Sync Request
        //  3.) MessageHistory Sync Request
        tracing::info!("Waiting for intents published");
        wait_for_min_intents(amal_b_conn, 3).await;

        let old_group_id = amal_a.get_sync_group(amal_a_conn).unwrap().group_id;
        tracing::info!("Old Group Id: {}", hex::encode(&old_group_id));
        // Check for new welcomes to new groups in the first installation (should be welcomed to a new sync group from amal_b).
        amal_a.sync_welcomes(&amal_a_provider).await.unwrap();
        let new_group_id = amal_a.get_sync_group(amal_a_conn).unwrap().group_id;
        tracing::info!("New Group Id: {}", hex::encode(&new_group_id));
        // group id should have changed to the new sync group created by the second installation
        assert_ne!(old_group_id, new_group_id);

        let consent_a = amal_a.syncable_consent_records(amal_a_conn).unwrap().len();

        // Have amal_a receive the message (and auto-process)
        let amal_a_sync_group = amal_a.get_sync_group(amal_a_conn).unwrap();
        assert_ok!(amal_a_sync_group.sync_with_conn(&amal_a_provider).await);
        xmtp_common::wait_for_some(|| async {
            amal_b
                .get_latest_sync_reply(&amal_b_provider, DeviceSyncKind::Consent)
                .await
                .unwrap()
        })
        .await
        .unwrap();

        // Wait up to 20 seconds for sync to process (typically is almost instant)
        xmtp_common::wait_for_eq(
            || {
                let consent_b = amal_b.syncable_consent_records(amal_b_conn).unwrap().len();
                futures::future::ready(consent_b != consent_a)
            },
            true,
        )
        .await
        .unwrap();

        // Test consent streaming
        let amal_b_sync_group = amal_b.get_sync_group(amal_b_conn).unwrap();
        let bo_wallet = generate_local_wallet();

        // Ensure bo is not consented with amal_b
        let mut bo_consent_with_amal_b = amal_b_conn
            .get_consent_record(bo_wallet.get_address(), ConsentType::Address)
            .unwrap();
        assert!(bo_consent_with_amal_b.is_none());

        // Consent with bo on the amal_a installation
        amal_a
            .set_consent_states(&[StoredConsentRecord::new(
                ConsentType::Address,
                ConsentState::Allowed,
                bo_wallet.get_address(),
            )])
            .await
            .unwrap();
        assert!(amal_a_conn
            .get_consent_record(bo_wallet.get_address(), ConsentType::Address)
            .unwrap()
            .is_some());
        let amal_a_subscription = amal_a.local_events().subscribe();

        // Wait for the consent to get streamed to the amal_b
        let start = Instant::now();
        while bo_consent_with_amal_b.is_none() {
            assert_ok!(amal_b_sync_group.sync_with_conn(&amal_b_provider).await);
            bo_consent_with_amal_b = amal_b_conn
                .get_consent_record(bo_wallet.get_address(), ConsentType::Address)
                .unwrap();

            if start.elapsed() > Duration::from_secs(1) {
                panic!("Consent update did not stream");
            }
        }

        // No new messages were generated for the amal_a installation during this time.
        assert!(amal_a_subscription.is_empty());
    }
}
