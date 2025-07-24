use std::sync::Arc;

use crate::client::ClientError;
use crate::context::{XmtpMlsLocalContext, XmtpSharedContext};
use crate::groups::device_sync::worker::SyncMetric;
use crate::groups::device_sync::DeviceSyncClient;
use crate::Client;
use serde::{Deserialize, Serialize};
use xmtp_common::time::now_ns;
use xmtp_db::{
    consent_record::StoredConsentRecord, prelude::*, user_preferences::StoredUserPreferences,
};
use xmtp_db::{ConnectionExt, StorageError, XmtpDb, XmtpOpenMlsProvider};
use xmtp_id::scw_verifier::SmartContractSignatureVerifier;
use xmtp_proto::xmtp::device_sync::content::{
    preference_update::Update as PreferenceUpdateProto, HmacKeyUpdate as HmacKeyUpdateProto,
    PreferenceUpdate as NewUserPreferenceUpdateProto,
    V1UserPreferenceUpdate as UserPreferenceUpdateProto,
};
use xmtp_proto::xmtp::mls::message_contents::PlaintextEnvelope as PlaintextEnvelopeProto;
use xmtp_proto::ConversionError;
use xmtp_proto::{
    api_client::XmtpApi,
    xmtp::mls::message_contents::{
        plaintext_envelope::v2::MessageType,
        plaintext_envelope::{Content, V2},
    },
};

use super::PreferenceUpdate;

/// This struct is only kept around to deserialize messages from
/// old libxmtp versions. It should not be used for any internal logic
/// or processing.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum LegacyUserPreferenceUpdate {
    ConsentUpdate(StoredConsentRecord),
    HmacKeyUpdate { key: Vec<u8> },
}

impl LegacyUserPreferenceUpdate {
    pub fn decode(update: &[u8]) -> Result<PreferenceUpdate, bincode::Error> {
        let update: Self = bincode::deserialize(update)?;
        let update = match update {
            LegacyUserPreferenceUpdate::ConsentUpdate(c) => PreferenceUpdate::Consent(c),
            LegacyUserPreferenceUpdate::HmacKeyUpdate { key } => PreferenceUpdate::Hmac {
                key,
                cycled_at_ns: 0,
            },
        };
        Ok(update)
    }
}

/// Process and insert incoming preference updates over the sync group
pub(crate) fn process_incoming_preference_update(
    update_proto: UserPreferenceUpdateProto,
    context: &impl XmtpSharedContext,
    storage: &impl XmtpMlsStorageProvider,
) -> Result<Vec<PreferenceUpdate>, StorageError> {
    let proto_content = update_proto.contents;

    let mut updates = vec![];
    let mut consent_updates = vec![];

    for update in proto_content {
        if let Ok(update) = LegacyUserPreferenceUpdate::decode(&update) {
            match update.clone() {
                PreferenceUpdate::Consent(consent_record) => {
                    consent_updates.push(consent_record);
                }
                PreferenceUpdate::Hmac { key, .. } => {
                    updates.push(update);
                    StoredUserPreferences::store_hmac_key(&storage.db(), &key, None)?;
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
        let changed = storage
            .db()
            .insert_or_replace_consent_records(&consent_updates)?;
        let changed: Vec<_> = changed.into_iter().map(PreferenceUpdate::Consent).collect();
        updates.extend(changed);
    }

    if let Some(handle) = context.workers().sync_metrics() {
        updates.iter().for_each(|u| match u {
            PreferenceUpdate::Consent(_) => handle.increment_metric(SyncMetric::V1ConsentReceived),
            PreferenceUpdate::Hmac { .. } => handle.increment_metric(SyncMetric::V1HmacReceived),
        });
    }

    Ok(updates)
}

impl LegacyUserPreferenceUpdate {
    /// Send a preference update through the sync group for other devices to consume
    /// Returns updates synced
    pub(crate) async fn v1_sync_across_devices<C: XmtpSharedContext>(
        updates: Vec<Self>,
        device_sync: &DeviceSyncClient<C>,
    ) -> Result<Vec<Self>, ClientError> {
        let sync_group = device_sync.get_sync_group().await?;

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
        sync_group.prepare_message(&content_bytes, |now| PlaintextEnvelopeProto {
            content: Some(Content::V2(V2 {
                message_type: Some(MessageType::UserPreferenceUpdate(update_proto)),
                idempotency_key: now.to_string(),
            })),
        })?;

        // sync_group.publish_intents(&provider).await?;
        sync_group.sync_until_last_intent_resolved().await?;

        updates.iter().for_each(|u| match u {
            LegacyUserPreferenceUpdate::ConsentUpdate(_) => {
                tracing::info!("Sent consent to group_id: {:?}", sync_group.group_id);
                device_sync
                    .metrics
                    .increment_metric(SyncMetric::V1ConsentSent)
            }
            LegacyUserPreferenceUpdate::HmacKeyUpdate { .. } => {
                device_sync.metrics.increment_metric(SyncMetric::V1HmacSent)
            }
        });

        Ok(updates)
    }
}

impl From<PreferenceUpdate> for LegacyUserPreferenceUpdate {
    fn from(update: PreferenceUpdate) -> Self {
        match update {
            PreferenceUpdate::Consent(rec) => Self::ConsentUpdate(rec),
            PreferenceUpdate::Hmac { key, .. } => Self::HmacKeyUpdate { key },
        }
    }
}

impl TryFrom<NewUserPreferenceUpdateProto> for LegacyUserPreferenceUpdate {
    type Error = ConversionError;
    fn try_from(update: NewUserPreferenceUpdateProto) -> Result<Self, Self::Error> {
        let NewUserPreferenceUpdateProto {
            update: Some(update),
        } = update
        else {
            return Err(ConversionError::Unspecified("update"));
        };

        let update = match update {
            PreferenceUpdateProto::Consent(consent) => {
                LegacyUserPreferenceUpdate::ConsentUpdate(consent.try_into()?)
            }
            PreferenceUpdateProto::Hmac(HmacKeyUpdateProto { key, .. }) => {
                LegacyUserPreferenceUpdate::HmacKeyUpdate { key }
            }
        };

        Ok(update)
    }
}

#[cfg(test)]
mod tests {

    use crate::{
        groups::device_sync_legacy::preference_sync_legacy::LegacyUserPreferenceUpdate,
        utils::{LocalTesterBuilder, Tester},
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
            consented_at_ns: 0,
        };
        let update = LegacyUserPreferenceUpdate::ConsentUpdate(consent_record);

        let bytes = bincode::serialize(&update).unwrap();

        let old_update: OldUserPreferenceUpdate = bincode::deserialize(&bytes).unwrap();

        let OldUserPreferenceUpdate::ConsentUpdate(update) = old_update;
        assert_eq!(update.state, ConsentState::Allowed);
    }
}
