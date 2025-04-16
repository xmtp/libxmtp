use super::*;
use crate::{groups::scoped_client::ScopedGroupClient, Client};
use xmtp_db::{consent_record::StoredConsentRecord, user_preferences::StoredUserPreferences};

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
        let sync_group = client.get_sync_group(&provider)?;

        tracing::info!(
            "Outgoing preference update {updates:?} sync group: {:?}",
            sync_group.group_id
        );

        let contents = updates
            .iter()
            .map(bincode::serialize)
            .collect::<Result<Vec<_>, _>>()?;
        let update_proto = UserPreferenceUpdateProto { contents };
        let content_bytes = serde_json::to_vec(&update_proto)?;
        sync_group.prepare_message(&content_bytes, &provider, |now| PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                message_type: Some(MessageType::UserPreferenceUpdate(update_proto)),
                idempotency_key: now.to_string(),
            })),
        })?;

        // sync_group.publish_intents(&provider).await?;
        sync_group
            .sync_until_last_intent_resolved(&provider)
            .await?;

        if let Some(handle) = client.device_sync.worker_handle() {
            updates.iter().for_each(|u| match u {
                UserPreferenceUpdate::ConsentUpdate(_) => {
                    tracing::info!("Sent consent to group_id: {:?}", sync_group.group_id);
                    handle.increment_metric(SyncMetric::V1ConsentSent)
                }
                UserPreferenceUpdate::HmacKeyUpdate { .. } => {
                    handle.increment_metric(SyncMetric::V1HmacSent)
                }
            });
        }

        Ok(())
    }

    /// Process and insert incoming preference updates over the sync group
    pub(crate) fn process_incoming_preference_update<C>(
        update_proto: UserPreferenceUpdateProto,
        client: &C,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<Vec<Self>, StorageError>
    where
        C: ScopedGroupClient,
    {
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

        if let Some(handle) = client.worker_handle() {
            updates.iter().for_each(|u| match u {
                UserPreferenceUpdate::ConsentUpdate(_) => {
                    handle.increment_metric(SyncMetric::V1ConsentReceived)
                }
                UserPreferenceUpdate::HmacKeyUpdate { .. } => {
                    handle.increment_metric(SyncMetric::V1HmacReceived)
                }
            });
        }

        Ok(updates)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        groups::{
            device_sync::{handle::SyncMetric, preference_sync::UserPreferenceUpdate},
            scoped_client::ScopedGroupClient,
        },
        utils::tester::{Tester, XmtpClientWalletTester},
    };
    use serde::{Deserialize, Serialize};
    use xmtp_db::{
        consent_record::{ConsentState, ConsentType, StoredConsentRecord},
        user_preferences::StoredUserPreferences,
    };

    #[derive(Serialize, Deserialize, Clone)]
    #[repr(i32)]
    enum OldUserPreferenceUpdate {
        ConsentUpdate(StoredConsentRecord) = 1,
    }

    #[xmtp_common::test]
    async fn test_can_deserialize_between_versions() {
        let consent_record = StoredConsentRecord {
            entity: "hello there".to_string(),
            entity_type: ConsentType::InboxId,
            state: ConsentState::Allowed,
        };
        let update = UserPreferenceUpdate::ConsentUpdate(consent_record);

        let bytes = bincode::serialize(&update).unwrap();

        let old_update: OldUserPreferenceUpdate = bincode::deserialize(&bytes).unwrap();

        let OldUserPreferenceUpdate::ConsentUpdate(update) = old_update;
        assert_eq!(update.state, ConsentState::Allowed);
    }

    #[xmtp_common::test]
    async fn test_hmac_sync() {
        let amal_a = Tester::new().await;
        let amal_b = amal_a.clone().await;

        // wait for the new sync group
        amal_a.worker.wait_for_init().await.unwrap();
        amal_b.worker.wait_for_init().await.unwrap();

        amal_a.sync_welcomes(&amal_a.provider).await.unwrap();

        let sync_group_a = amal_a.get_sync_group(&amal_a.provider).unwrap();
        let sync_group_b = amal_b.get_sync_group(&amal_b.provider).unwrap();
        assert_eq!(sync_group_a.group_id, sync_group_b.group_id);

        sync_group_a.sync_with_conn(&amal_a.provider).await.unwrap();
        sync_group_b.sync_with_conn(&amal_a.provider).await.unwrap();

        // Wait for a to process the new hmac key
        amal_a
            .worker
            .wait(SyncMetric::V1HmacReceived, 1)
            .await
            .unwrap();

        let pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref()).unwrap();
        let pref_b = StoredUserPreferences::load(amal_b.provider.conn_ref()).unwrap();

        assert_eq!(pref_a.hmac_key, pref_b.hmac_key);

        amal_a
            .revoke_installations(vec![amal_b.installation_id().to_vec()])
            .await
            .unwrap();

        let new_pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref()).unwrap();
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }
}
