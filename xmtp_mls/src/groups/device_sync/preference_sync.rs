use super::*;
use crate::{
    storage::{consent_record::StoredConsentRecord, user_preferences::StoredUserPreferences},
    Client,
};
use serde::{Deserialize, Serialize};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::mls::message_contents::UserPreferenceUpdate as UserPreferenceUpdateProto,
};

#[derive(Serialize, Deserialize, Clone, Debug)]
#[repr(i32)]
pub enum UserPreferenceUpdate {
    ConsentUpdate(StoredConsentRecord) = 1,
    HmacKeyUpdate { key: Vec<u8> } = 2,
}

impl UserPreferenceUpdate {
    /// Send a preference update through the sync group for other devices to consume
    pub(crate) async fn sync_across_devices<C: XmtpApi, V: SmartContractSignatureVerifier>(
        updates: Vec<Self>,
        client: &Client<C, V>,
    ) -> Result<(), DeviceSyncError> {
        let provider = client.mls_provider()?;
        let sync_group = client.ensure_sync_group(&provider).await?;

        let updates = updates
            .iter()
            .map(bincode::serialize)
            .collect::<Result<Vec<_>, _>>()?;
        let update_proto = UserPreferenceUpdateProto { contents: updates };
        let content_bytes = serde_json::to_vec(&update_proto)?;
        sync_group.prepare_message(&content_bytes, &provider, |now| PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                message_type: Some(MessageType::UserPreferenceUpdate(update_proto)),
                idempotency_key: now.to_string(),
            })),
        })?;

        sync_group.publish_intents(&provider).await?;

        Ok(())
    }

    /// Process and insert incoming preference updates over the sync group
    pub(crate) fn process_incoming_preference_update(
        update_proto: UserPreferenceUpdateProto,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<Self>, StorageError> {
        let conn = provider.conn_ref();

        let proto_content = update_proto.contents;

        let mut updates = Vec::with_capacity(proto_content.len());
        let mut consent_updates = vec![];

        for update in proto_content {
            if let Ok(update) = bincode::deserialize::<UserPreferenceUpdate>(&update) {
                updates.push(update.clone());
                match update {
                    UserPreferenceUpdate::ConsentUpdate(consent_record) => {
                        consent_updates.push(consent_record);
                    }
                    UserPreferenceUpdate::HmacKeyUpdate { key } => {
                        StoredUserPreferences {
                            hmac_key: Some(key),
                            ..StoredUserPreferences::load(conn)?
                        }
                        .store(conn)?;
                    }
                }
            } else {
                // Don't fail on errors since this may come from a newer version of the lib
                // that has new update types.
                tracing::warn!(
                    "Failed to deserialize preference update. Is this libxmtp version outdated?"
                );
            }
        }

        // Insert all of the consent records at once.
        if !consent_updates.is_empty() {
            conn.insert_or_replace_consent_records(&consent_updates)?;
        }

        Ok(updates)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        builder::ClientBuilder,
        groups::scoped_client::ScopedGroupClient,
        storage::consent_record::{ConsentState, StoredConsentType},
    };
    use crypto_utils::generate_local_wallet;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[derive(Serialize, Deserialize, Clone)]
    #[repr(i32)]
    enum OldUserPreferenceUpdate {
        ConsentUpdate(StoredConsentRecord) = 1,
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 1))]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_can_deserialize_between_versions() {
        let consent_record = StoredConsentRecord {
            entity: "hello there".to_string(),
            entity_type: StoredConsentType::Identity,
            state: ConsentState::Allowed,
        };
        let update = UserPreferenceUpdate::ConsentUpdate(consent_record);

        let bytes = bincode::serialize(&update).unwrap();

        let old_update: OldUserPreferenceUpdate = bincode::deserialize(&bytes).unwrap();

        let OldUserPreferenceUpdate::ConsentUpdate(update) = old_update;
        assert_eq!(update.state, ConsentState::Allowed);
    }

    #[wasm_bindgen_test(unsupported = tokio::test(flavor = "multi_thread", worker_threads = 1))]
    #[cfg_attr(target_family = "wasm", ignore)]
    async fn test_hmac_sync() {
        let wallet = generate_local_wallet();
        let amal_a =
            ClientBuilder::new_test_client_with_history(&wallet, "http://localhost:5558").await;
        let amal_a_provider = amal_a.mls_provider().unwrap();
        let amal_a_conn = amal_a_provider.conn_ref();
        let amal_a_worker = amal_a.sync_worker_handle().unwrap();

        let amal_b =
            ClientBuilder::new_test_client_with_history(&wallet, "http://localhost:5558").await;
        let amal_b_provider = amal_b.mls_provider().unwrap();
        let amal_b_conn = amal_b_provider.conn_ref();
        let amal_b_worker = amal_b.sync_worker_handle().unwrap();

        // wait for the new sync group
        amal_a_worker.wait_for_processed_count(1).await.unwrap();
        amal_b_worker.wait_for_processed_count(1).await.unwrap();

        amal_a.sync_welcomes(&amal_a_provider).await.unwrap();

        let sync_group_a = amal_a.get_sync_group(amal_a_conn).unwrap();
        let sync_group_b = amal_b.get_sync_group(amal_b_conn).unwrap();
        assert_eq!(sync_group_a.group_id, sync_group_b.group_id);

        sync_group_a.sync_with_conn(&amal_a_provider).await.unwrap();
        sync_group_b.sync_with_conn(&amal_a_provider).await.unwrap();

        // Wait for a to process the new hmac key
        amal_a_worker.wait_for_processed_count(2).await.unwrap();

        let pref_a = StoredUserPreferences::load(amal_a_conn).unwrap();
        let pref_b = StoredUserPreferences::load(amal_b_conn).unwrap();

        assert_eq!(pref_a.hmac_key, pref_b.hmac_key);

        amal_a
            .revoke_installations(vec![amal_b.installation_id().to_vec()])
            .await
            .unwrap();

        let new_pref_a = StoredUserPreferences::load(amal_a_conn).unwrap();
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }
}
