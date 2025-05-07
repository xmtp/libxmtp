use super::*;
use crate::groups::device_sync_legacy::preference_sync_legacy::LegacyUserPreferenceUpdate;
use crate::Client;
use xmtp_common::time::now_ns;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::user_preferences::{HmacKey, StoredUserPreferences};
use xmtp_proto::api_client::trait_impls::XmtpApi;
use xmtp_proto::xmtp::device_sync::content::HmacKeyUpdate as HmacKeyUpdateProto;
use xmtp_proto::xmtp::device_sync::content::{
    device_sync_content::Content as ContentProto, user_preference_update::Update as UpdateProto,
    PreferenceUpdates, UserPreferenceUpdate as UserPreferenceUpdateProto,
};
use xmtp_proto::ConversionError;

#[derive(Clone, Debug, PartialEq)]
pub enum UserPreferenceUpdate {
    Consent(StoredConsentRecord),
    Hmac { key: Vec<u8>, cycled_at_ns: i64 },
}

impl<ApiClient, V> Client<ApiClient, V>
where
    ApiClient: XmtpApi,
    V: SmartContractSignatureVerifier,
{
    pub(crate) async fn sync_preferences(
        &self,
        provider: &XmtpOpenMlsProvider,
        updates: Vec<UserPreferenceUpdate>,
    ) -> Result<(), ClientError> {
        self.send_device_sync_message(
            provider,
            ContentProto::PreferenceUpdates(PreferenceUpdates {
                updates: updates.clone().into_iter().map(From::from).collect(),
            }),
        )
        .await?;

        if let Some(handle) = self.worker_handle() {
            updates.iter().for_each(|update| match update {
                UserPreferenceUpdate::Consent(_) => {
                    handle.increment_metric(SyncMetric::ConsentSent)
                }
                UserPreferenceUpdate::Hmac { .. } => handle.increment_metric(SyncMetric::HmacSent),
            });
        }

        // TODO: v1 support - remove this on next hammer
        let legacy_updates = updates.into_iter().map(Into::into).collect();
        LegacyUserPreferenceUpdate::v1_sync_across_devices(legacy_updates, self).await?;

        Ok(())
    }

    pub(crate) async fn cycle_hmac(
        &self,
        provider: &XmtpOpenMlsProvider,
    ) -> Result<(), ClientError> {
        tracing::info!("Sending new HMAC key to sync group.");

        self.sync_preferences(
            provider,
            vec![UserPreferenceUpdate::Hmac {
                key: HmacKey::random_key(),
                cycled_at_ns: now_ns(),
            }],
        )
        .await?;

        Ok(())
    }
}

pub(super) fn store_preference_updates(
    updates: Vec<UserPreferenceUpdateProto>,
    provider: &XmtpOpenMlsProvider,
    handle: &WorkerHandle<SyncMetric>,
) -> Result<Vec<UserPreferenceUpdate>, StorageError> {
    let mut changed = vec![];
    for update in updates.into_iter().filter_map(|u| u.update) {
        match update {
            UpdateProto::Consent(consent_save) => {
                tracing::info!(
                    "Storing consent update from sync group. State: {:?}",
                    consent_save.state
                );

                let consent_record: StoredConsentRecord = consent_save.try_into()?;
                let updated = provider
                    .conn_ref()
                    .insert_newer_consent_record(consent_record.clone())?;

                if updated {
                    changed.push(UserPreferenceUpdate::Consent(consent_record));
                }

                handle.increment_metric(SyncMetric::ConsentReceived);
            }
            UpdateProto::Hmac(HmacKeyUpdateProto { key, cycled_at_ns }) => {
                tracing::info!("Storing new HMAC key from sync group");
                StoredUserPreferences::store_hmac_key(
                    provider.conn_ref(),
                    &key,
                    Some(cycled_at_ns),
                )?;
                changed.push(UserPreferenceUpdate::Hmac { key, cycled_at_ns });
                handle.increment_metric(SyncMetric::HmacReceived);
            }
        }
    }

    Ok(changed)
}

impl TryFrom<UserPreferenceUpdateProto> for UserPreferenceUpdate {
    type Error = ConversionError;
    fn try_from(update: UserPreferenceUpdateProto) -> Result<Self, Self::Error> {
        let Some(update) = update.update else {
            return Err(ConversionError::Unspecified("update"));
        };
        update.try_into()
    }
}
impl TryFrom<UpdateProto> for UserPreferenceUpdate {
    type Error = ConversionError;
    fn try_from(update: UpdateProto) -> Result<Self, Self::Error> {
        let update = match update {
            UpdateProto::Consent(consent) => Self::Consent(consent.try_into()?),
            UpdateProto::Hmac(HmacKeyUpdateProto { key, cycled_at_ns }) => {
                Self::Hmac { key, cycled_at_ns }
            }
        };
        Ok(update)
    }
}

impl From<UserPreferenceUpdate> for UserPreferenceUpdateProto {
    fn from(update: UserPreferenceUpdate) -> Self {
        UserPreferenceUpdateProto {
            update: Some(match update {
                UserPreferenceUpdate::Consent(consent) => UpdateProto::Consent(consent.into()),
                UserPreferenceUpdate::Hmac { key, cycled_at_ns } => {
                    UpdateProto::Hmac(HmacKeyUpdateProto { key, cycled_at_ns })
                }
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        groups::{device_sync::handle::SyncMetric, scoped_client::ScopedGroupClient},
        utils::{LocalTesterBuilder, Tester},
    };
    use xmtp_db::user_preferences::StoredUserPreferences;

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_hmac_sync() {
        let amal_a = Tester::builder().sync_worker().build().await;
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
