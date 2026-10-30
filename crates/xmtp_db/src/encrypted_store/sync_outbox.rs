//! The sync outbox: the consent records and the root key that are owed to the
//! other installations, read in pages and cleared once published.

use super::{
    ConnectionExt,
    consent_record::{Owed, StoredConsentRecord},
    schema::{consent_records::dsl, user_preferences::dsl as prefs},
    user_preferences::{RootKeyWrite, StoredUserPreferences, write_root_key},
};
use crate::{ConnectionError, DbQuery, StorageError, StoredSecret};
use diesel::{dsl::exists, prelude::*, sql_types::BigInt};

/// A fresh owed mark.
pub fn next_owed_seq(db: &impl ConnectionExt) -> Result<i64, ConnectionError> {
    #[derive(QueryableByName)]
    struct Counter {
        #[diesel(sql_type = BigInt)]
        owed_counter: i64,
    }
    let counter = db.raw_query(|c| {
        diesel::sql_query(
            "UPDATE user_preferences SET owed_counter = owed_counter + 1 RETURNING owed_counter",
        )
        .get_result::<Counter>(c)
    })?;
    Ok(counter.owed_counter)
}

/// The owed root key, with the mark it was read with.
pub struct OwedRootKey {
    pub key: StoredSecret,
    pub cycled_at_ns: i64,
    pub owed_seq: i64,
}

/// Owed values in key order, root key first. Each keeps the mark it was read with.
#[derive(Default)]
pub struct OwedPage {
    pub root_key: Option<OwedRootKey>,
    pub consent: Vec<StoredConsentRecord>,
}

impl OwedPage {
    pub fn is_empty(&self) -> bool {
        self.root_key.is_none() && self.consent.is_empty()
    }
}

/// The most bytes one owed update takes in a sync message: a record with a
/// 256-byte entity, or the 42-byte root key, with framing.
const MAX_UPDATE_BYTES: usize = 400;

/// The owed values, at most `max_updates` of them in at most `max_bytes` of
/// encoded updates.
// implements: SYNC-029
pub fn owed_page(
    db: &impl DbQuery,
    max_updates: usize,
    max_bytes: usize,
) -> Result<OwedPage, StorageError> {
    let prefs = StoredUserPreferences::load(db)?;
    let root_key = match (prefs.hmac_key, prefs.hmac_key_cycled_at_ns) {
        (Some(key), Some(cycled_at_ns)) if prefs.hmac_key_owed_seq > 0 => Some(OwedRootKey {
            key,
            cycled_at_ns,
            owed_seq: prefs.hmac_key_owed_seq,
        }),
        _ => None,
    };
    let limit =
        max_updates.min(max_bytes / MAX_UPDATE_BYTES).max(1) - usize::from(root_key.is_some());
    let consent = db.raw_query(|c| {
        dsl::consent_records
            .filter(dsl::owed_seq.gt(0))
            .order((dsl::entity_type, dsl::entity))
            .limit(limit as i64)
            .load(c)
    })?;
    Ok(OwedPage { root_key, consent })
}

/// After the message carrying `page` is published, clear each key whose mark
/// is still the one in `page`. A key marked again since `owed_page` keeps
/// its new mark and goes out again.
// implements: SYNC-033
pub fn clear_owed(db: &impl ConnectionExt, page: &OwedPage) -> Result<(), ConnectionError> {
    for row in &page.consent {
        db.raw_query(|c| {
            diesel::update(dsl::consent_records.find((row.entity_type, &row.entity)))
                .filter(dsl::owed_seq.eq(row.owed_seq))
                .set(dsl::owed_seq.eq(0))
                .execute(c)
        })?;
    }
    if let Some(root) = &page.root_key {
        db.raw_query(|c| {
            diesel::update(prefs::user_preferences)
                .filter(prefs::hmac_key_owed_seq.eq(root.owed_seq))
                .set(prefs::hmac_key_owed_seq.eq(0))
                .execute(c)
        })?;
    }
    Ok(())
}

/// Whether any consent record or the root key is owed.
pub fn has_owed(db: &impl ConnectionExt) -> Result<bool, ConnectionError> {
    db.raw_query(|c| {
        diesel::select(
            exists(dsl::consent_records.filter(dsl::owed_seq.gt(0))).or(exists(
                prefs::user_preferences.filter(prefs::hmac_key_owed_seq.gt(0)),
            )),
        )
        .get_result(c)
    })
}

/// A new installation joined: owe it every record and the root key, under
/// one fresh mark. `true` when no root key was held and one was made; the
/// caller emits its events.
// implements: SYNC-028
pub fn mark_all_owed(db: &impl DbQuery) -> Result<bool, StorageError> {
    let seq = next_owed_seq(db)?;
    db.raw_query(|c| {
        diesel::update(dsl::consent_records)
            .set(dsl::owed_seq.eq(seq))
            .execute(c)
    })?;
    if StoredUserPreferences::load(db)?.hmac_key.is_none() {
        // Make one, marked owed.
        return write_root_key(db, RootKeyWrite::MADE, Owed::Mark);
    }
    db.raw_query(|c| {
        diesel::update(prefs::user_preferences)
            .set(prefs::hmac_key_owed_seq.eq(seq))
            .execute(c)
    })?;
    Ok(false)
}
