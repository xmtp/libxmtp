use super::*;
use std::collections::{HashMap, HashSet};
use xmtp_common::time::now_ns;
use xmtp_cryptography::secret::Secret;
use xmtp_db::consent_record::StoredConsentRecord;
use xmtp_db::user_preferences::{HmacKey, StoredUserPreferences};
use xmtp_proto::ConversionError;
use xmtp_proto::xmtp::device_sync::content::HmacKeyUpdate as HmacKeyUpdateProto;
use xmtp_proto::xmtp::device_sync::content::{
    PreferenceUpdate as PreferenceUpdateProto, PreferenceUpdates,
    device_sync_content::Content as ContentProto, preference_update::Update as UpdateProto,
};

/// `Debug` shows the HMAC root key only as its length: `Secret` redacts it.
#[derive(Clone, Debug, PartialEq)]
pub enum PreferenceUpdate {
    Consent(StoredConsentRecord),
    Hmac { key: Secret, cycled_at_ns: i64 },
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

        #[cfg(test)]
        worker::test_hooks::observe_preferences(
            self.context.installation_id(),
            worker::test_hooks::PreferenceStage::Published,
            &updates,
        );
        Ok(updates)
    }

    // implements: SYNC-015
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

// implements: SYNC-023
pub(super) struct StoredPreferenceUpdates {
    pub legacy: Vec<PreferenceUpdate>,
    pub public: Vec<PreferenceUpdate>,
}

/// Store synced preference updates. `updates` is borrowed, so a transaction
/// retry copies no HMAC root key; the caller wipes the keys.
// implements: SYNC-017, CONS-003, SYNC-022
pub(super) fn store_preference_updates(
    updates: &[PreferenceUpdateProto],
    conn: &impl DbQuery,
    handle: &WorkerMetrics<SyncMetric>,
) -> Result<StoredPreferenceUpdates, StorageError> {
    // Validate before reducing so an unusable entry cannot displace a usable one.
    let mut reduced = HashMap::new();
    for (index, update) in updates.iter().filter_map(|u| u.update.as_ref()).enumerate() {
        let key = match update {
            UpdateProto::Consent(consent) => {
                if let Err(error) = StoredConsentRecord::try_from(consent.clone()) {
                    tracing::warn!(%error, "skipping an unusable synced consent record");
                    continue;
                }
                Some((consent.entity_type, consent.entity.as_str()))
            }
            UpdateProto::Hmac(hmac) => {
                if hmac.key.len() != 42 {
                    tracing::warn!(
                        len = hmac.key.len(),
                        "skipping a synced root key that is not 42 bytes"
                    );
                    continue;
                }
                None
            }
        };
        let winner = reduced.entry(key).or_insert((index, update));
        let newer = match (update, winner.1) {
            (UpdateProto::Consent(a), UpdateProto::Consent(b)) => {
                (a.consented_at_ns, a.state) > (b.consented_at_ns, b.state)
            }
            (UpdateProto::Hmac(a), UpdateProto::Hmac(b)) => {
                (a.cycled_at_ns, &a.key) > (b.cycled_at_ns, &b.key)
            }
            _ => false,
        };
        if newer {
            *winner = (index, update);
        }
    }
    let mut reduced: Vec<_> = reduced.into_values().collect();
    // Keep winners in message order for the legacy batch and public events.
    reduced.sort_unstable_by_key(|(index, _)| *index);
    let mut changed = vec![];
    let mut initial_consents = HashMap::new();
    let initial_hmac = StoredUserPreferences::load(conn)?;
    for (_, update) in reduced {
        match update {
            UpdateProto::Consent(consent_save) => {
                tracing::info!(
                    "Storing consent update from sync group. State: {:?}",
                    consent_save.state
                );

                let consent_record: StoredConsentRecord = consent_save.clone().try_into()?;
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
                let updated = conn
                    .insert_newer_consent_record(consent_record.clone())?
                    .state_changed;

                if updated {
                    changed.push(PreferenceUpdate::Consent(consent_record));
                }

                handle.increment_metric(SyncMetric::ConsentReceived);
            }
            UpdateProto::Hmac(HmacKeyUpdateProto { key, cycled_at_ns }) => {
                tracing::info!("Storing new HMAC key from sync group");
                let before = StoredUserPreferences::load(conn)?;
                StoredUserPreferences::store_hmac_key(conn, key, Some(*cycled_at_ns))?;
                let after = StoredUserPreferences::load(conn)?;
                if before.hmac_key != after.hmac_key
                    || before.hmac_key_cycled_at_ns != after.hmac_key_cycled_at_ns
                {
                    changed.push(PreferenceUpdate::Hmac {
                        key: Secret::new(key.clone()),
                        cycled_at_ns: *cycled_at_ns,
                    });
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
            UpdateProto::Hmac(mut hmac) => Self::Hmac {
                // Moves the decoded buffer; no copy is made.
                key: Secret::from(std::mem::take(&mut hmac.key)),
                cycled_at_ns: hmac.cycled_at_ns,
            },
        };
        Ok(update)
    }
}

/// The proto holds a plain copy of an HMAC root key. Encode it with
/// `sync_message_bytes`, which wipes that copy.
impl From<PreferenceUpdate> for PreferenceUpdateProto {
    fn from(update: PreferenceUpdate) -> Self {
        PreferenceUpdateProto {
            update: Some(match update {
                PreferenceUpdate::Consent(consent) => UpdateProto::Consent(consent.into()),
                PreferenceUpdate::Hmac { key, cycled_at_ns } => {
                    UpdateProto::Hmac(HmacKeyUpdateProto {
                        key: key.as_slice().to_vec(),
                        cycled_at_ns,
                    })
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

    /// `Debug` of an HMAC update, and of the update decoded from its proto,
    /// shows no root key bytes.
    #[xmtp_common::test(unwrap_try = true)]
    fn hmac_update_debug_omits_the_root_key() {
        let key: Vec<u8> = (0..42).map(|i| 0x80 | i).collect();
        let decoded = PreferenceUpdate::try_from(UpdateProto::Hmac(HmacKeyUpdateProto {
            key: key.clone(),
            cycled_at_ns: 5,
        }))?;
        let built = PreferenceUpdate::Hmac {
            key: key.clone().into(),
            cycled_at_ns: 5,
        };
        assert_eq!(decoded, built);
        let forms = [
            hex::encode(&key),
            hex::encode_upper(&key),
            // How `Debug` prints a byte slice, anywhere in the list.
            format!("{:?}", &key[..4])
                .trim_matches(['[', ']'])
                .to_owned(),
        ];
        for debug in [format!("{decoded:?}"), format!("{:?}", vec![built])] {
            for form in &forms {
                assert!(
                    !debug.contains(form.as_str()),
                    "PreferenceUpdate Debug shows the key as {form}: {debug}"
                );
            }
        }
    }

    // verifies: PROC-036
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn cycle_hmac_log_omits_installation_id() {
        use futures::FutureExt;
        use xmtp_logging::{Level, test_logging::LogCapture};

        tester!(alix, disable_workers);
        let capture = LogCapture::new(Level::Info);
        let sync = alix.context.device_sync_client();
        tracing::dispatcher::with_default(&capture.dispatch(), || {
            // Poll the real call through its log emission, before network completion.
            let _ = sync.cycle_hmac().now_or_never();
        });
        let output = capture.output();
        let event = output
            .lines()
            .find(|line| line.contains("Sending new HMAC key to sync group."))
            .expect("cycle_hmac emitted its INFO event");
        assert!(
            !event.contains(&alix.context.installation_id().to_string()),
            "cycle_hmac log contains the full installation ID"
        );
    }

    // verifies: SYNC-015, SYNC-024
    #[rstest::rstest]
    #[xmtp_common::test(unwrap_try = true)]
    async fn test_hmac_sync() {
        use crate::worker::device_sync::tests::revoke;

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
        revoke(&amal_a, &amal_b).await?;
        amal_a
            .worker()
            .register_interest(SyncMetric::HmacSent, sent_before_revoke + 1)
            .wait()
            .await?;
        let sync_group = amal_a.device_sync_client().get_sync_group().await?;
        let revoked = amal_b.context.installation_id().to_vec();
        let leaves = sync_group.with_group_snapshot(|g| Ok(g.members().collect::<Vec<_>>()))?;
        assert!(leaves.iter().all(|leaf| leaf.signature_key != revoked));

        amal_a.sync_all_welcomes_and_device_sync_groups().await?;
        amal_a
            .worker()
            .register_interest(SyncMetric::HmacReceived, received_before_revoke + 1)
            .wait()
            .await?;
        let new_pref_a = StoredUserPreferences::load(amal_a.context.db())?;
        assert_ne!(pref_a.hmac_key, new_pref_a.hmac_key);
    }

    // verifies: SYNC-017, CONS-003, SYNC-022
    #[rstest::rstest]
    #[case::address(3, 2)]
    #[case::unspecified_state(2, 0)]
    #[case::unknown_enum(99, 2)]
    #[case::short_key(2, 2)]
    #[xmtp_common::test(unwrap_try = true)]
    async fn unusable_update_is_skipped_and_the_rest_apply(
        #[case] entity_type: i32,
        #[case] state: i32,
    ) {
        use xmtp_proto::xmtp::device_sync::consent_backup::ConsentSave;
        tester!(alix, disable_workers);
        let db = alix.context.db();
        let bad = if (entity_type, state) == (2, 2) {
            UpdateProto::Hmac(HmacKeyUpdateProto {
                key: vec![1; 41],
                cycled_at_ns: i64::MAX,
            })
        } else {
            UpdateProto::Consent(ConsentSave {
                entity_type,
                state,
                entity: "bad".into(),
                consented_at_ns: i64::MAX,
            })
        };
        let good =
            StoredConsentRecord::new(ConsentType::InboxId, ConsentState::Allowed, "good".into());
        let updates = vec![
            PreferenceUpdateProto { update: Some(bad) },
            PreferenceUpdate::Consent(good.clone()).into(),
            PreferenceUpdate::Hmac {
                key: vec![2; 42].into(),
                cycled_at_ns: i64::MAX - 1,
            }
            .into(),
        ];
        let metrics = WorkerMetrics::new(alix.context.installation_id());
        let result = store_preference_updates(&updates, &db, &metrics);
        assert!(result.is_ok(), "{:?}", result.err());
        assert_eq!(db.consent_records().unwrap(), vec![good]);
        let prefs = StoredUserPreferences::load(&db).unwrap();
        assert_eq!(prefs.hmac_key, Some(vec![2; 42].into()));
    }

    // verifies: EVENT-001, EVENT-010, CONS-010, CONS-003, SYNC-022
    #[xmtp_common::test(unwrap_try = true)]
    async fn sync_batch_reduces_legacy_updates() {
        use xmtp_proto::xmtp::device_sync::consent_backup::ConsentSave;
        tester!(alix, disable_workers);
        let metrics = WorkerMetrics::new(alix.context.installation_id());
        let db = alix.context.db();
        let hmac = |key: u8, cycled_at_ns| PreferenceUpdate::Hmac {
            key: vec![key; 42].into(),
            cycled_at_ns,
        };
        let first: Vec<_> = [hmac(1, i64::MAX - 1), hmac(2, i64::MAX - 1)]
            .into_iter()
            .map(Into::into)
            .collect();
        let first = store_preference_updates(&first, &db, &metrics)?;
        assert_eq!(
            StoredUserPreferences::load(&db)?.hmac_key,
            Some(vec![2; 42].into())
        );
        assert_eq!(first.legacy, vec![hmac(2, i64::MAX - 1)]);
        assert_eq!(first.public, vec![hmac(2, i64::MAX - 1)]);
        let no_change: Vec<_> = [hmac(2, i64::MAX - 1), hmac(3, i64::MAX - 3)]
            .into_iter()
            .map(Into::into)
            .collect();
        let no_change = store_preference_updates(&no_change, &db, &metrics)?;
        assert!(no_change.legacy.is_empty());
        assert!(no_change.public.is_empty());

        let mut allowed = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Allowed,
            "one-entity".into(),
        );
        allowed.consented_at_ns = 101;
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
        let changed = store_preference_updates(&updates, &db, &metrics)?;
        let stored = db.get_consent_record(denied.entity.clone(), denied.entity_type)?;
        assert_eq!(stored, Some(denied.clone()));
        assert_eq!(
            changed.legacy,
            vec![PreferenceUpdate::Consent(denied.clone())]
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
        let round_trip = store_preference_updates(&round_trip, &db, &metrics)?;
        assert!(round_trip.legacy.is_empty());
        assert!(round_trip.public.is_empty());

        let mut unknown = StoredConsentRecord::new(
            ConsentType::InboxId,
            ConsentState::Unknown,
            "new-unknown".into(),
        );
        unknown.consented_at_ns = 104;
        let mut unusable: ConsentSave = unknown.clone().into();
        unusable.state = 0;
        unusable.consented_at_ns = 105;
        let inserted = store_preference_updates(
            &[
                PreferenceUpdateProto {
                    update: Some(UpdateProto::Consent(unusable)),
                },
                PreferenceUpdate::Consent(unknown.clone()).into(),
            ],
            &db,
            &metrics,
        )?;
        let stored = db.get_consent_record(unknown.entity.clone(), unknown.entity_type)?;
        assert_eq!(stored, Some(unknown.clone()));
        assert_eq!(inserted.public, vec![PreferenceUpdate::Consent(unknown)]);
    }

    // verifies: CONS-010
    #[xmtp_common::test(unwrap_try = true)]
    async fn peer_sync_consent_keeps_the_latest_record() {
        tester!(alix, disable_workers);
        let metrics = WorkerMetrics::new(alix.context.installation_id());
        let db = alix.context.db();
        let updates = [
            (ConsentState::Allowed, 10),
            (ConsentState::Allowed, 30),
            (ConsentState::Denied, 20),
        ]
        .map(|(state, consented_at_ns)| {
            PreferenceUpdate::Consent(StoredConsentRecord {
                entity_type: ConsentType::InboxId,
                state,
                entity: "peer-inbox".into(),
                consented_at_ns,
            })
            .into()
        });
        let changed = store_preference_updates(&updates, &db, &metrics)?;
        // Reduction keeps Allowed@30; the older records are not written.
        assert_eq!(changed.legacy.len(), 1);
        let stored = db
            .get_consent_record("peer-inbox".into(), ConsentType::InboxId)?
            .unwrap();
        assert_eq!(stored.state, ConsentState::Allowed);
        assert_eq!(stored.consented_at_ns, 30);
    }
}
