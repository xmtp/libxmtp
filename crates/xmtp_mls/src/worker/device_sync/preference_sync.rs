use super::*;
use std::collections::{HashMap, HashSet};
use xmtp_common::time::now_ns;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::user_preferences::{HmacKey, StoredUserPreferences};
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::device_sync::content::HmacKeyUpdate as HmacKeyUpdateProto;
use xmtp_proto::xmtp::device_sync::content::{
    PreferenceUpdate as PreferenceUpdateProto, PreferenceUpdates,
    device_sync_content::Content as ContentProto, preference_update::Update as UpdateProto,
};

#[derive(Clone, Debug, PartialEq)]
pub enum PreferenceUpdate {
    Consent(StoredConsentRecord),
    Hmac { key: Vec<u8>, cycled_at_ns: i64 },
}

impl<Context> DeviceSyncClient<Context>
where
    Context: XmtpSharedContext,
{
    // implements: SYNC-020
    pub(crate) async fn sync_preferences(
        &self,
        updates: Vec<PreferenceUpdate>,
    ) -> Result<Vec<PreferenceUpdate>, ClientError> {
        self.send_device_sync_message(ContentProto::PreferenceUpdates(PreferenceUpdates {
            updates: updates.clone().into_iter().map(From::from).collect(),
        }))
        .await?;

        updates.iter().for_each(|update| match update {
            PreferenceUpdate::Consent(_) => self.metrics.increment_metric(SyncMetric::ConsentSent),
            PreferenceUpdate::Hmac { .. } => self.metrics.increment_metric(SyncMetric::HmacSent),
        });

        Ok(updates)
    }

    // implements: SYNC-015
    pub(crate) async fn cycle_hmac(&self) -> Result<(), ClientError> {
        tracing::info!(
            "[{}] Sending new HMAC key to sync group.",
            self.context.installation_id()
        );

        self.sync_preferences(vec![PreferenceUpdate::Hmac {
            key: HmacKey::random_key(),
            cycled_at_ns: now_ns(),
        }])
        .await?;

        Ok(())
    }
}

// implements: SYNC-023
pub(super) struct StoredPreferenceUpdates {
    pub legacy: Vec<PreferenceUpdate>,
    pub public: Vec<PreferenceUpdate>,
}

pub(super) fn store_preference_updates(
    updates: Vec<PreferenceUpdateProto>,
    conn: &impl DbQuery,
    handle: &WorkerMetrics<SyncMetric>,
) -> Result<StoredPreferenceUpdates, StorageError> {
    let mut changed = vec![];
    let mut initial_consents = HashMap::new();
    let initial_hmac = StoredUserPreferences::load(conn)?;
    for update in updates.into_iter().filter_map(|u| u.update) {
        match update {
            UpdateProto::Consent(consent_save) => {
                tracing::info!(
                    "Storing consent update from sync group. State: {:?}",
                    consent_save.state
                );

                let consent_record: StoredConsentRecord = consent_save.try_into()?;
                let key = (
                    consent_record.entity_type as i32,
                    consent_record.entity.clone(),
                );
                if let std::collections::hash_map::Entry::Vacant(entry) =
                    initial_consents.entry(key)
                {
                    let initial = conn
                        .get_consent_record(
                            consent_record.entity.clone(),
                            consent_record.entity_type,
                        )?
                        .map(|record| record.state);
                    entry.insert(initial);
                }
                let updated = conn.insert_newer_consent_record(consent_record.clone())?;

                if updated {
                    changed.push(PreferenceUpdate::Consent(consent_record));
                }

                handle.increment_metric(SyncMetric::ConsentReceived);
            }
            UpdateProto::Hmac(HmacKeyUpdateProto { key, cycled_at_ns }) => {
                tracing::info!("Storing new HMAC key from sync group");
                let before = StoredUserPreferences::load(conn)?;
                StoredUserPreferences::store_hmac_key(conn, &key, Some(cycled_at_ns))?;
                let after = StoredUserPreferences::load(conn)?;
                if before.hmac_key != after.hmac_key
                    || before.hmac_key_cycled_at_ns != after.hmac_key_cycled_at_ns
                {
                    changed.push(PreferenceUpdate::Hmac { key, cycled_at_ns });
                }
                handle.increment_metric(SyncMetric::HmacReceived);
            }
        }
    }

    let final_hmac = StoredUserPreferences::load(conn)?;
    let hmac_changed = initial_hmac.hmac_key != final_hmac.hmac_key
        || initial_hmac.hmac_key_cycled_at_ns != final_hmac.hmac_key_cycled_at_ns;
    let mut seen_consents = HashSet::new();
    let mut seen_hmac = false;
    let mut public: Vec<_> = changed
        .iter()
        .rev()
        .filter(|update| match update {
            PreferenceUpdate::Consent(record) => {
                seen_consents.insert((record.entity_type as i32, record.entity.clone()))
                    && initial_consents
                        .get(&(record.entity_type as i32, record.entity.clone()))
                        .copied()
                        .flatten()
                        != Some(record.state)
            }
            PreferenceUpdate::Hmac { .. } => {
                !std::mem::replace(&mut seen_hmac, true) && hmac_changed
            }
        })
        .cloned()
        .collect();
    public.reverse();
    Ok(StoredPreferenceUpdates {
        legacy: changed,
        public,
    })
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
    use super::*;
    use crate::{
        tester,
        worker::{device_sync::worker::SyncMetric, metrics::WorkerMetrics},
    };
    use xmtp_db::consent_record::{ConsentState, ConsentType};
    use xmtp_db::user_preferences::StoredUserPreferences;

    // verifies: SYNC-015
    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_hmac_sync() {
        use xmtp_id::associations::test_utils::add_wallet_signature;

        tester!(amal_a, sync_worker);
        tester!(amal_b, from: amal_a);

        amal_a.test_has_same_sync_group_as(&amal_b).await?;

        xmtp_common::wait_for_eq(
            || async {
                amal_a.worker().get(SyncMetric::HmacSent)
                    + amal_b.worker().get(SyncMetric::HmacSent)
                    >= 1
            },
            true,
        )
        .await?;

        amal_a.sync_all_welcomes_and_device_sync_groups().await?;
        amal_a
            .worker()
            .register_interest(SyncMetric::HmacReceived, 1)
            .wait()
            .await?;

        // Wait for a to process the new hmac key
        amal_b
            .context
            .device_sync_client()
            .get_sync_group()
            .await?
            .sync()
            .await?;
        amal_b
            .worker()
            .register_interest(SyncMetric::HmacReceived, 1)
            .wait()
            .await?;

        let pref_a = StoredUserPreferences::load(amal_a.context.db())?;
        let pref_b = StoredUserPreferences::load(amal_b.context.db())?;

        assert_eq!(pref_a.hmac_key, pref_b.hmac_key);

        let sent_before_revoke = amal_a.worker().get(SyncMetric::HmacSent);
        let received_before_revoke = amal_a.worker().get(SyncMetric::HmacReceived);
        let mut revoke = amal_a
            .identity_updates()
            .revoke_installations(vec![amal_b.context.installation_id().to_vec()])
            .await?;
        add_wallet_signature(&mut revoke, &amal_a.builder.owner).await;
        amal_a
            .identity_updates()
            .apply_signature_request(revoke)
            .await?;
        amal_a
            .worker()
            .register_interest(SyncMetric::HmacSent, sent_before_revoke + 1)
            .wait()
            .await?;

        amal_a.sync_all_welcomes_and_device_sync_groups().await?;
        amal_a
            .worker()
            .register_interest(SyncMetric::HmacReceived, received_before_revoke + 1)
            .wait()
            .await?;
        let new_pref_a = StoredUserPreferences::load(amal_a.context.db())?;
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }

    // verifies: EVENT-001, EVENT-010
    #[xmtp_common::test(unwrap_try = true)]
    async fn sync_batch_preserves_legacy_updates() {
        tester!(alix, disable_workers);
        let metrics = WorkerMetrics::new(alix.context.installation_id());
        let db = alix.context.db();
        let hmac = |key: u8, cycled_at_ns| PreferenceUpdate::Hmac {
            key: vec![key; 42],
            cycled_at_ns,
        };
        let first: Vec<_> = [hmac(1, i64::MAX - 2), hmac(2, i64::MAX - 1)]
            .into_iter()
            .map(Into::into)
            .collect();
        let first = store_preference_updates(first, &db, &metrics)?;
        assert_eq!(
            first.legacy,
            vec![hmac(1, i64::MAX - 2), hmac(2, i64::MAX - 1)]
        );
        assert_eq!(first.public, vec![hmac(2, i64::MAX - 1)]);
        let no_change: Vec<_> = [hmac(2, i64::MAX - 1), hmac(3, i64::MAX - 3)]
            .into_iter()
            .map(Into::into)
            .collect();
        let no_change = store_preference_updates(no_change, &db, &metrics)?;
        assert!(no_change.legacy.is_empty());
        assert!(no_change.public.is_empty());
        assert_eq!(
            StoredUserPreferences::load(&db)?.hmac_key,
            Some(vec![2; 42])
        );

        let mut allowed = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Allowed,
            "one-entity".into(),
        );
        allowed.consented_at_ns = 100;
        let mut denied = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Denied,
            "one-entity".into(),
        );
        denied.consented_at_ns = 101;
        let updates = vec![
            PreferenceUpdate::Consent(allowed.clone()).into(),
            PreferenceUpdate::Consent(denied.clone()).into(),
        ];
        let changed = store_preference_updates(updates, &db, &metrics)?;
        assert_eq!(
            changed.legacy,
            vec![
                PreferenceUpdate::Consent(allowed),
                PreferenceUpdate::Consent(denied.clone())
            ]
        );
        assert_eq!(changed.public, vec![PreferenceUpdate::Consent(denied)]);

        let mut denied_again = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Denied,
            "one-entity".into(),
        );
        denied_again.consented_at_ns = 103;
        let mut allowed_again = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Allowed,
            "one-entity".into(),
        );
        allowed_again.consented_at_ns = 102;
        let round_trip = vec![
            PreferenceUpdate::Consent(allowed_again.clone()).into(),
            PreferenceUpdate::Consent(denied_again.clone()).into(),
        ];
        // The final state is Denied, which matches the state before this batch.
        let round_trip = store_preference_updates(round_trip, &db, &metrics)?;
        assert_eq!(round_trip.legacy.len(), 2);
        assert!(round_trip.public.is_empty());

        let mut unknown = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Unknown,
            "new-unknown".into(),
        );
        unknown.consented_at_ns = 104;
        let inserted = store_preference_updates(
            vec![PreferenceUpdate::Consent(unknown.clone()).into()],
            &db,
            &metrics,
        )?;
        assert_eq!(inserted.public, vec![PreferenceUpdate::Consent(unknown)]);
    }
}
