use super::*;
use crate::{storage::consent_record::StoredConsentRecord, Client};
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
    pub(crate) async fn sync_across_devices<C: XmtpApi, V: SmartContractSignatureVerifier>(
        updates: Vec<Self>,
        client: &Client<C, V>,
    ) -> Result<(), DeviceSyncError> {
        let provider = client.mls_provider()?;
        let conn = provider.conn_ref();
        let sync_group = client.get_sync_group(conn)?;

        let updates = updates
            .iter()
            .map(bincode::serialize)
            .collect::<Result<Vec<_>, _>>()?;
        let update_proto = UserPreferenceUpdateProto { content: updates };
        let content_bytes = serde_json::to_vec(&update_proto)?;
        sync_group.prepare_message(&content_bytes, &provider, |_time_ns| PlaintextEnvelope {
            content: Some(Content::V2(V2 {
                idempotency_key: new_request_id(),
                message_type: Some(MessageType::UserPreferenceUpdate(update_proto)),
            })),
        })?;

        sync_group
            .sync_until_last_intent_resolved(&provider)
            .await?;

        Ok(())
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
