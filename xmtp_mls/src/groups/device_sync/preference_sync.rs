use super::*;
use crate::{groups::scoped_client::ScopedGroupClient, Client};
use serde::{Deserialize, Serialize};
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::{consent_record::StoredConsentRecord, user_preferences::StoredUserPreferences};
use xmtp_proto::{
    api_client::trait_impls::XmtpApi,
    xmtp::mls::message_contents::UserPreferenceUpdate as UserPreferenceUpdateProto,
};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum UserPreferenceUpdate {
    ConsentUpdate(StoredConsentRecord),
    HmacKeyUpdate { key: Vec<u8> },
}

impl UserPreferenceUpdate {
    pub(crate) async fn sync<C: XmtpApi, V: SmartContractSignatureVerifier>(
        updates: Vec<Self>,
        client: &Client<C, V>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), ClientError> {
        client
            .send_device_sync_message(
                provider,
                DeviceSyncContent::PreferenceUpdates(updates.clone()),
            )
            .await?;

        if let Some(handle) = client.worker_handle() {
            updates.iter().for_each(|update| match update {
                Self::ConsentUpdate(_) => handle.increment_metric(SyncMetric::ConsentSent),
                Self::HmacKeyUpdate { .. } => handle.increment_metric(SyncMetric::HmacSent),
            });
        }

        // TODO: v1 support - remove this on next hammer
        Self::v1_sync_across_devices(updates.clone(), client).await?;

        Ok(())
    }

    pub(crate) async fn cycle_hmac<C: XmtpApi, V: SmartContractSignatureVerifier>(
        client: &Client<C, V>,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), ClientError> {
        tracing::info!("Sending new HMAC key to sync group.");

        Self::sync(
            vec![Self::HmacKeyUpdate {
                key: HmacKey::random_key(),
            }],
            client,
            provider,
        )
        .await?;

        Ok(())
    }

    /// Send a preference update through the sync group for other devices to consume
    async fn v1_sync_across_devices<C: XmtpApi, V: SmartContractSignatureVerifier>(
        updates: Vec<Self>,
        client: &Client<C, V>,
    ) -> Result<(), ClientError> {
        let provider = client.mls_provider()?;
        let sync_group = client.get_sync_group(&provider).await?;

        tracing::info!(
            "Outgoing preference update {updates:?} sync group: {:?}",
            sync_group.group_id
        );

        let contents = updates
            .iter()
            .map(bincode::serialize)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| ClientError::Generic(e.to_string()))?;
        let update_proto = UserPreferenceUpdateProto { contents };
        let content_bytes =
            serde_json::to_vec(&update_proto).map_err(|e| ClientError::Generic(e.to_string()))?;
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

    pub(super) fn store(
        self,
        provider: &XmtpOpenMlsProvider,
        handle: &WorkerHandle<SyncMetric>,
    ) -> Result<Vec<Self>, StorageError> {
        let mut changed = vec![];
        match self {
            Self::ConsentUpdate(consent_record) => {
                tracing::info!(
                    "Storing consent update from sync group. State: {:?}",
                    consent_record.state
                );
                let updated = provider
                    .conn_ref()
                    .insert_or_replace_consent_records(&[consent_record])?;
                changed.extend(
                    updated
                        .into_iter()
                        .map(Self::ConsentUpdate)
                        .collect::<Vec<_>>(),
                );

                handle.increment_metric(SyncMetric::ConsentReceived);
            }
            Self::HmacKeyUpdate { key } => {
                tracing::info!("Storing new HMAC key from sync group");
                StoredUserPreferences::store_hmac_key(provider.conn_ref(), &key)?;
                changed.push(Self::HmacKeyUpdate { key });
                handle.increment_metric(SyncMetric::HmacReceived);
            }
        }

        Ok(changed)
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

        let mut updates = vec![];
        let mut consent_updates = vec![];

        for update in proto_content {
            if let Ok(update) = bincode::deserialize::<UserPreferenceUpdate>(&update) {
                match update {
                    UserPreferenceUpdate::ConsentUpdate(consent_record) => {
                        consent_updates.push(consent_record);
                    }
                    UserPreferenceUpdate::HmacKeyUpdate { key } => {
                        updates.push(Self::HmacKeyUpdate { key: key.clone() });
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
            let changed = conn.insert_or_replace_consent_records(&consent_updates)?;
            let changed: Vec<_> = changed.into_iter().map(Self::ConsentUpdate).collect();
            updates.extend(changed);
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
        utils::{Tester, XmtpClientTesterBuilder},
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

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_hmac_sync() {
        let amal_a = Tester::builder().with_sync_worker().build().await;
        let amal_b = amal_a.builder.build().await;

        amal_a.test_has_same_sync_group_as(&amal_b).await?;

        amal_a.worker().wait(SyncMetric::HmacSent, 1).await?;

        amal_a.sync_device_sync(&amal_a.provider).await?;
        amal_a.worker().wait(SyncMetric::HmacReceived, 1).await?;

        // Wait for a to process the new hmac key
        amal_b
            .get_sync_group(&amal_b.provider)
            .await?
            .sync()
            .await?;
        amal_b.worker().wait(SyncMetric::HmacReceived, 1).await?;

        let pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref())?;
        let pref_b = StoredUserPreferences::load(amal_b.provider.conn_ref())?;

        assert_eq!(pref_a.hmac_key, pref_b.hmac_key);

        amal_a
            .revoke_installations(vec![amal_b.installation_id().to_vec()])
            .await?;

        amal_a.sync_device_sync(&amal_a.provider).await?;
        let new_pref_a = StoredUserPreferences::load(amal_a.provider.conn_ref())?;
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }
}
