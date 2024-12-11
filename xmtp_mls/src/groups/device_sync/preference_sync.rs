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

#[derive(Serialize, Deserialize, Clone)]
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
        let update_proto = UserPreferenceUpdateProto { content: updates };
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

        let proto_content = update_proto.content;

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
                        StoredUserPreferences::set_hmac_key(conn, key)?
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
    use crate::storage::consent_record::{ConsentState, ConsentType};

    use super::*;

    #[derive(Serialize, Deserialize, Clone)]
    #[repr(i32)]
    enum OldUserPreferenceUpdate {
        ConsentUpdate(StoredConsentRecord) = 1,
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_can_deserialize_between_versions() {
        let consent_record = StoredConsentRecord {
            entity: "hello there".to_string(),
            entity_type: ConsentType::Address,
            state: ConsentState::Allowed,
        };
        let update = UserPreferenceUpdate::ConsentUpdate(consent_record);

        let bytes = bincode::serialize(&update).unwrap();

        let old_update: OldUserPreferenceUpdate = bincode::deserialize(&bytes).unwrap();

        let OldUserPreferenceUpdate::ConsentUpdate(update) = old_update;
        assert_eq!(update.state, ConsentState::Allowed);
    }
}
