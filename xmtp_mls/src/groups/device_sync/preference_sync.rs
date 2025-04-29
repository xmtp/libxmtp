use super::*;
use crate::{groups::scoped_client::ScopedGroupClient, Client};
use serde::{Deserialize, Serialize};
use xmtp_db::user_preferences::HmacKey;
use xmtp_db::{consent_record::StoredConsentRecord, user_preferences::StoredUserPreferences};
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::xmtp::device_sync::content::{
    device_sync_content::Content as ContentProto, PreferenceUpdates,
    UserPreferenceUpdate as UserPreferenceUpdateProto,
};
use xmtp_proto::xmtp::device_sync::content::{
    user_preference_update::Update as PreferenceUpdateProto, HmacKeyUpdate as HmacKeyUpdateProto,
};
use xmtp_proto::ConversionError;

mod preference_sync_legacy;

pub(super) async fn sync<C: XmtpApi, V: SmartContractSignatureVerifier>(
    updates: Vec<UserPreferenceUpdateProto>,
    client: &Client<C, V>,
    provider: &XmtpOpenMlsProvider,
) -> Result<(), ClientError> {
    client
        .send_device_sync_message(
            provider,
            ContentProto::PreferenceUpdates(PreferenceUpdates { updates }),
        )
        .await?;

    if let Some(handle) = client.worker_handle() {
        updates.iter().for_each(|update| {
            if let UserPreferenceUpdateProto {
                update: Some(update),
            } = update
            {
                match update {
                    user_preference_update::Update::ConsentUpdate(_) => {
                        handle.increment_metric(SyncMetric::ConsentSent)
                    }
                    user_preference_update::Update::HmacKeyUpdate(_) => {
                        handle.increment_metric(SyncMetric::HmacSent)
                    }
                }
            }
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
        vec![UserPreferenceUpdateProto {
            update: Some(user_preference_update::Update::HmacKeyUpdate(
                HmacKeyUpdateProto {
                    key: HmacKey::random_key(),
                },
            )),
        }],
        client,
        provider,
    )
    .await?;

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

#[cfg(test)]
mod tests {
    use crate::{
        groups::{
            device_sync::{handle::SyncMetric, preference_sync::UserPreferenceUpdate},
            scoped_client::ScopedGroupClient,
        },
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
