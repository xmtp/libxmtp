use super::*;
use xmtp_cryptography::secret::Secret;
use xmtp_db::consent_record::{
    ConsentChange, ConsentWrite, MAX_AHEAD_NS, Owed, StoredConsentRecord, parse_consent_save,
    write_consent,
};
use xmtp_db::group_message::StoredGroupMessage;
use xmtp_db::user_preferences::{RootKeyWrite, write_root_key};
use xmtp_proto::xmtp::device_sync::content::{
    PreferenceUpdate as PreferenceUpdateProto, preference_update::Update as UpdateProto,
};

/// A stored preference change, for the legacy in-process streams. `Debug`
/// shows the HMAC root key only as its length: `Secret` redacts it.
#[derive(Clone, Debug)]
pub enum PreferenceUpdate {
    Consent(StoredConsentRecord),
    Hmac { key: Secret, cycled_at_ns: i64 },
}

/// What one sync message changed.
#[derive(Default)]
pub(super) struct Applied {
    pub consent: Vec<ConsentChange>,
    /// Set when the root key changed: whether the change was marked owed.
    pub root_key: Option<bool>,
}

/// Merge a peer's sync message. `updates` is borrowed, so a transaction
/// retry copies no HMAC root key; the caller wipes the keys.
// implements: CONS-051, CONS-061, SYNC-023, SYNC-027, SYNC-032
pub(super) fn apply_preference_updates(
    db: &impl DbQuery,
    own_inbox: &str,
    msg: &StoredGroupMessage,
    updates: &[PreferenceUpdateProto],
) -> Result<Applied, StorageError> {
    // A value from the selected sync group keeps the mark; one from
    // an older group is passed on.
    let selected = db.all_sync_groups()?.into_iter().next();
    let owed = match selected {
        Some(group) if group.id == msg.group_id => Owed::Keep,
        _ => Owed::Mark,
    };
    let mut applied = Applied::default();
    for update in updates.iter().filter_map(|update| update.update.as_ref()) {
        match update {
            UpdateProto::Consent(save) => {
                let Some((key, value)) = parse_consent_save(save) else {
                    tracing::warn!("skipping an unusable synced consent record");
                    continue;
                };
                // A 16-byte DM id is its peer's record.
                let key = key.pre8_dm(db, own_inbox)?.map_or(key, |(peer, _)| peer);
                let write = ConsentWrite::from_peer(value, msg.sent_at_ns);
                applied
                    .consent
                    .extend(write_consent(db, &key, write, owed)?);
            }
            UpdateProto::Hmac(hmac) => {
                let write = RootKeyWrite::Received {
                    key: &hmac.key,
                    cycled_at_ns: hmac.cycled_at_ns,
                    bound_ns: msg.sent_at_ns.saturating_add(MAX_AHEAD_NS),
                };
                if write_root_key(db, write, owed)? {
                    applied.root_key = Some(owed == Owed::Mark);
                }
            }
        }
    }
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tester;
    use xmtp_db::consent_record::{ConsentKey, ConsentState, ConsentType, ConsentValue};
    use xmtp_db::group::{ConversationType, GroupMembershipState, StoredGroup};
    use xmtp_db::user_preferences::StoredUserPreferences;
    use xmtp_proto::types::GroupId;
    use xmtp_proto::xmtp::device_sync::{consent_backup::ConsentSave, content::HmacKeyUpdate};

    /// `Debug` of an HMAC update shows no root key bytes.
    #[xmtp_common::test]
    fn hmac_update_debug_omits_the_root_key() {
        let key: Vec<u8> = (0..42).map(|i| 0x80 | i).collect();
        let built = PreferenceUpdate::Hmac {
            key: key.clone().into(),
            cycled_at_ns: 5,
        };
        let forms = [
            hex::encode(&key),
            hex::encode_upper(&key),
            // How `Debug` prints a byte slice, anywhere in the list.
            format!("{:?}", &key[..4])
                .trim_matches(['[', ']'])
                .to_owned(),
        ];
        let debug = format!("{:?}", vec![built]);
        for form in &forms {
            assert!(
                !debug.contains(form.as_str()),
                "PreferenceUpdate Debug shows the key as {form}: {debug}"
            );
        }
    }

    fn consent(entity_type: i32, state: i32, entity: &str, at: i64) -> PreferenceUpdateProto {
        PreferenceUpdateProto {
            update: Some(UpdateProto::Consent(ConsentSave {
                entity_type,
                state,
                entity: entity.into(),
                consented_at_ns: at,
            })),
        }
    }

    fn root_key(key: Vec<u8>, cycled_at_ns: i64) -> PreferenceUpdateProto {
        PreferenceUpdateProto {
            update: Some(UpdateProto::Hmac(HmacKeyUpdate { key, cycled_at_ns })),
        }
    }

    /// A sync message stored in `group_id` at envelope time `sent_at_ns`.
    fn message(group_id: GroupId, sent_at_ns: i64) -> StoredGroupMessage {
        StoredGroupMessage {
            sent_at_ns,
            ..crate::test::mock::generate_stored_msg(xmtp_proto::types::Cursor(1), group_id)
        }
    }

    fn sync_group(db: &impl DbQuery, id: u8, created_at_ns: i64) -> Result<GroupId, StorageError> {
        use xmtp_db::Store;
        let id = GroupId::from([id; 16]);
        StoredGroup::builder()
            .id(id)
            .created_at_ns(created_at_ns)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id("me")
            .conversation_type(ConversationType::Sync)
            .build()?
            .store(db)?;
        Ok(id)
    }

    // verifies: SYNC-023, CONS-051, SYNC-032
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
        tester!(alix, disable_workers);
        let db = alix.context.db();
        let now = now_ns();
        let bad = if (entity_type, state) == (2, 2) {
            root_key(vec![1; 41], now + 1)
        } else {
            consent(entity_type, state, "bad", now + 1)
        };
        let updates = vec![bad, consent(2, 2, "good", now), root_key(vec![2; 42], now)];
        let group = sync_group(&db, 1, now).unwrap();
        let applied = apply_preference_updates(&db, "me", &message(group, now), &updates).unwrap();
        assert_eq!(applied.consent.len(), 1);
        let stored: Vec<_> = db
            .consent_records()
            .unwrap()
            .iter()
            .map(|r| (r.entity.clone(), r.state))
            .collect();
        assert_eq!(stored, vec![("good".to_string(), ConsentState::Allowed)]);
        assert_eq!(
            StoredUserPreferences::load(&db).unwrap().hmac_key,
            Some(vec![2; 42].into())
        );
    }

    /// A received time is capped at a day after its envelope; a 16-byte DM id
    /// moves to its peer's record; a value from the selected sync group keeps
    /// its mark and one from an older group is marked to pass it on.
    // verifies: CONS-054, CONS-061, SYNC-027, SYNC-032
    #[xmtp_common::test(unwrap_try = true)]
    async fn received_values_are_capped_keyed_and_marked() {
        use xmtp_db::Store;
        tester!(alix, disable_workers);
        let db = alix.context.db();
        let own = alix.inbox_id();
        let envelope = now_ns();
        let older = sync_group(&db, 1, envelope - 10)?;
        let selected = sync_group(&db, 2, envelope)?;
        let peer = "ab".repeat(32);
        let dm = GroupId::from([7; 16]);
        StoredGroup::builder()
            .id(dm)
            .created_at_ns(1)
            .membership_state(GroupMembershipState::Restored)
            .added_by_inbox_id(own)
            .conversation_type(ConversationType::Dm)
            .dm_id(Some(format!("dm:{}", {
                let mut pair = [own.to_string(), peer.clone()];
                pair.sort();
                pair.join(":")
            })))
            .build()?
            .store(&db)?;
        // A 32-byte DM never moves: a former member can build one.
        let forged = GroupId::from([8; 32]);
        StoredGroup::builder()
            .id(forged)
            .created_at_ns(1)
            .membership_state(GroupMembershipState::Allowed)
            .added_by_inbox_id(own)
            .conversation_type(ConversationType::Dm)
            .dm_id(Some(format!("dm:{}", {
                let mut pair = [own.to_string(), "cd".repeat(32)];
                pair.sort();
                pair.join(":")
            })))
            .build()?
            .store(&db)?;
        let far = envelope + 2 * xmtp_common::NS_IN_DAY;
        let updates = vec![
            consent(1, 2, &hex::encode(forged), 5),
            consent(1, 3, &hex::encode(dm).to_uppercase(), 5),
            consent(2, 2, "far", far),
            root_key(vec![3; 42], far),
        ];
        let applied = apply_preference_updates(&db, own, &message(selected, envelope), &updates)?;
        assert_eq!(applied.root_key, Some(false));
        let cap = envelope + MAX_AHEAD_NS;
        let row = |key: &ConsentKey| db.consent_row(key).unwrap().unwrap();
        assert_eq!(
            row(&ConsentKey::inbox("far")).value(),
            ConsentValue {
                state: ConsentState::Allowed,
                consented_at_ns: cap
            }
        );
        assert_eq!(row(&ConsentKey::inbox(&peer)).state, ConsentState::Denied);
        assert!(db.consent_row(&ConsentKey::group(dm.as_ref()))?.is_none());
        assert_eq!(
            row(&ConsentKey::group(forged.as_ref())).state,
            ConsentState::Allowed
        );
        assert!(
            db.consent_row(&ConsentKey::inbox(&"cd".repeat(32)))?
                .is_none()
        );
        let prefs = StoredUserPreferences::load(&db)?;
        assert_eq!(prefs.hmac_key_cycled_at_ns, Some(cap));
        // From the selected group: nothing is owed.
        for key in [
            ConsentKey::inbox("far"),
            ConsentKey::inbox(&peer),
            ConsentKey::group(forged.as_ref()),
        ] {
            assert_eq!(row(&key).owed_seq, 0);
        }
        assert_eq!(prefs.hmac_key_owed_seq, 0);
        // From an older group: a change is passed on.
        let later = vec![consent(ConsentType::InboxId as i32, 3, "far", cap + 1)];
        let applied = apply_preference_updates(&db, own, &message(older, envelope), &later)?;
        assert!(applied.consent[0].marked);
        assert!(row(&ConsentKey::inbox("far")).owed_seq > 0);
    }
}
