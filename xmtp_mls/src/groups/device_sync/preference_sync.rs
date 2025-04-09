use super::*;
use crate::{utils::time::hmac_epoch, Client};
use serde::{Deserialize, Serialize};
use xmtp_db::{
    consent_record::StoredConsentRecord,
    user_preferences::{HmacKey, StoredUserPreferences},
};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::mls::message_contents::UserPreferenceUpdate as UserPreferenceUpdateProto,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[repr(i32)]
pub enum UserPreferenceUpdate {
    ConsentUpdate(StoredConsentRecord) = 1,
    HmacKeyUpdate { key: Vec<u8> } = 2,
}

impl UserPreferenceUpdate {
    pub(super) async fn sync<C: XmtpApi, V: SmartContractSignatureVerifier>(
        updates: Vec<Self>,
        client: &Client<C, V>,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("Outgoing preference updates {updates:?}");
        let provider = client.mls_provider()?;

        client
            .send_device_sync_message(
                &provider,
                DeviceSyncContent::PreferenceUpdates(updates.clone()),
            )
            .await?;

        // TODO: v1 support - remove this on next hammer
        // Self::v1_sync_across_devices(updates.clone(), client, handle).await?;

        updates.iter().for_each(|update| match update {
            Self::ConsentUpdate(_) => handle.increment_metric(SyncMetric::ConsentSent),
            Self::HmacKeyUpdate { .. } => handle.increment_metric(SyncMetric::HmacSent),
        });

        Ok(())
    }

    pub(super) async fn sync_hmac<C: XmtpApi, V: SmartContractSignatureVerifier>(
        client: &Client<C, V>,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError> {
        tracing::info!("Sending out HMAC key via sync group.");

        let provider = client.mls_provider()?;
        let pref = StoredUserPreferences::load(provider.conn_ref())?;

        let Some(key) = pref.hmac_key else {
            tracing::warn!("Attempted to send hmac key over sync, but did not have one to sync.");
            return Ok(());
        };

        Self::sync(vec![Self::HmacKeyUpdate { key }], client, handle).await?;

        Ok(())
    }

    /// Send a preference update through the sync group for other devices to consume
    async fn _v1_sync_across_devices<C: XmtpApi, V: SmartContractSignatureVerifier>(
        updates: Vec<Self>,
        client: &Client<C, V>,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), DeviceSyncError> {
        let provider = client.mls_provider()?;
        let sync_group = client.ensure_sync_group(&provider).await?;

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
        sync_group.publish_intents(&provider).await?;

        updates.iter().for_each(|u| match u {
            Self::ConsentUpdate(_) => handle.increment_metric(SyncMetric::V1ConsentSent),
            Self::HmacKeyUpdate { .. } => handle.increment_metric(SyncMetric::V1HmacSent),
        });

        Ok(())
    }

    pub(super) fn store(
        self,
        provider: &XmtpOpenMlsProvider,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<(), StorageError> {
        match self {
            Self::ConsentUpdate(consent_record) => {
                tracing::info!(
                    "Storing consent update from sync group. State: {:?}",
                    consent_record.state
                );
                provider
                    .conn_ref()
                    .insert_or_replace_consent_records(&[consent_record])?;
                handle.increment_metric(SyncMetric::ConsentReceived);
            }
            Self::HmacKeyUpdate { key } => {
                tracing::info!("Storing new HMAC key from sync group");
                let Ok(key) = key.try_into() else {
                    tracing::info!("Received HMAC key was wrong length.");
                    return Ok(());
                };
                StoredUserPreferences::store_hmac_key(
                    provider.conn_ref(),
                    &HmacKey {
                        key,
                        epoch: hmac_epoch(),
                    },
                )?;
                handle.increment_metric(SyncMetric::HmacReceived);
            }
        }

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
    use crate::{groups::scoped_client::ScopedGroupClient, utils::Tester};
    use xmtp_db::consent_record::{ConsentState, ConsentType};

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

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_hmac_sync() {
        let amal_a = Tester::new().await;
        amal_a.wait_for_sync_worker_init().await;
        let amal_b = amal_a.clone().await;
        amal_b.wait_for_sync_worker_init().await;

        amal_a.sync_welcomes(&amal_a.provider).await?;
        amal_a.worker.wait(SyncMetric::HmacSent, 2).await?;

        // Wait for a to process the new hmac key
        amal_b.get_sync_group(&amal_b.provider)?.sync().await?;
        amal_b.worker.wait(SyncMetric::HmacReceived, 2).await?;

        let pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref())?;
        let pref_b = StoredUserPreferences::load(amal_b.provider.conn_ref())?;

        assert_eq!(pref_a.hmac_key, pref_b.hmac_key);

        amal_a
            .revoke_installations(vec![amal_b.installation_id().to_vec()])
            .await?;

        let new_pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref())?;
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }
}
