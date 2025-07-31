use super::*;
use crate::groups::device_sync_legacy::preference_sync_legacy::LegacyUserPreferenceUpdate;
use xmtp_common::time::now_ns;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::user_preferences::{HmacKey, StoredUserPreferences};
use xmtp_proto::xmtp::device_sync::content::HmacKeyUpdate as HmacKeyUpdateProto;
use xmtp_proto::xmtp::device_sync::content::{
    device_sync_content::Content as ContentProto, preference_update::Update as UpdateProto,
    PreferenceUpdate as PreferenceUpdateProto, PreferenceUpdates,
};
use xmtp_proto::ConversionError;

#[derive(Clone, Debug, PartialEq)]
pub enum PreferenceUpdate {
    Consent(StoredConsentRecord),
    Hmac { key: Vec<u8>, cycled_at_ns: i64 },
}

impl<Context> DeviceSyncClient<Context>
where
    Context: XmtpSharedContext,
{
    pub(crate) async fn sync_preferences(
        &self,
        updates: Vec<PreferenceUpdate>,
    ) -> Result<(Vec<PreferenceUpdate>, Vec<LegacyUserPreferenceUpdate>), ClientError> {
        self.send_device_sync_message(ContentProto::PreferenceUpdates(PreferenceUpdates {
            updates: updates.clone().into_iter().map(From::from).collect(),
        }))
        .await?;

        updates.iter().for_each(|update| match update {
            PreferenceUpdate::Consent(_) => self.metrics.increment_metric(SyncMetric::ConsentSent),
            PreferenceUpdate::Hmac { .. } => self.metrics.increment_metric(SyncMetric::HmacSent),
        });

        // TODO: v1 support - remove this on next hammer
        let legacy_updates = updates.clone().into_iter().map(Into::into).collect();
        let legacy_updates =
            LegacyUserPreferenceUpdate::v1_sync_across_devices(legacy_updates, self).await?;

        Ok((updates, legacy_updates))
    }

    pub(crate) async fn cycle_hmac(&self) -> Result<(), ClientError> {
        tracing::info!("Sending new HMAC key to sync group.");

        self.sync_preferences(vec![PreferenceUpdate::Hmac {
            key: HmacKey::random_key(),
            cycled_at_ns: now_ns(),
        }])
        .await?;

        Ok(())
    }
}

pub(super) fn store_preference_updates(
    updates: Vec<PreferenceUpdateProto>,
    conn: &impl DbQuery,
    handle: &WorkerMetrics<SyncMetric>,
) -> Result<Vec<PreferenceUpdate>, StorageError> {
    let mut changed = vec![];
    for update in updates.into_iter().filter_map(|u| u.update) {
        match update {
            UpdateProto::Consent(consent_save) => {
                tracing::info!(
                    "Storing consent update from sync group. State: {:?}",
                    consent_save.state
                );

                let consent_record: StoredConsentRecord = consent_save.try_into()?;
                let updated = conn.insert_newer_consent_record(consent_record.clone())?;

                if updated {
                    changed.push(PreferenceUpdate::Consent(consent_record));
                }

                handle.increment_metric(SyncMetric::ConsentReceived);
            }
            UpdateProto::Hmac(HmacKeyUpdateProto { key, cycled_at_ns }) => {
                tracing::info!("Storing new HMAC key from sync group");
                StoredUserPreferences::store_hmac_key(conn, &key, Some(cycled_at_ns))?;
                changed.push(PreferenceUpdate::Hmac { key, cycled_at_ns });
                handle.increment_metric(SyncMetric::HmacReceived);
            }
        }
    }

    Ok(changed)
}

impl TryFrom<PreferenceUpdateProto> for PreferenceUpdate {
    type Error = ConversionError;
    fn try_from(update: PreferenceUpdateProto) -> Result<Self, Self::Error> {
        let Some(update) = update.update else {
            return Err(ConversionError::Unspecified("update"));
        };
        update.try_into()
    }
}
impl TryFrom<UpdateProto> for PreferenceUpdate {
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

impl From<PreferenceUpdate> for PreferenceUpdateProto {
    fn from(update: PreferenceUpdate) -> Self {
        PreferenceUpdateProto {
            update: Some(match update {
                PreferenceUpdate::Consent(consent) => UpdateProto::Consent(consent.into()),
                PreferenceUpdate::Hmac { key, cycled_at_ns } => {
                    UpdateProto::Hmac(HmacKeyUpdateProto { key, cycled_at_ns })
                }
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{groups::device_sync::worker::SyncMetric, tester};
    use xmtp_db::user_preferences::StoredUserPreferences;

    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_hmac_sync() {
        tester!(amal_a, sync_worker);
        tester!(amal_b, from: amal_a);

        amal_a.test_has_same_sync_group_as(&amal_b).await?;

        amal_a.worker().wait(SyncMetric::HmacSent, 1).await?;

        amal_a.sync_all_welcomes_and_history_sync_groups().await?;
        amal_a.worker().wait(SyncMetric::HmacReceived, 1).await?;

        // Wait for a to process the new hmac key
        amal_b
            .context
            .device_sync_client()
            .get_sync_group()
            .await?
            .sync()
            .await?;
        amal_b.worker().wait(SyncMetric::HmacReceived, 1).await?;

        let pref_a = StoredUserPreferences::load(amal_a.context.db())?;
        let pref_b = StoredUserPreferences::load(amal_b.context.db())?;

        assert_eq!(pref_a.hmac_key, pref_b.hmac_key);

        amal_a
            .identity_updates()
            .revoke_installations(vec![amal_b.context.installation_id().to_vec()])
            .await?;

        amal_a.sync_all_welcomes_and_history_sync_groups().await?;
        amal_a.worker().wait(SyncMetric::HmacReceived, 2).await?;
        let new_pref_a = StoredUserPreferences::load(amal_a.context.db())?;
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }
}
