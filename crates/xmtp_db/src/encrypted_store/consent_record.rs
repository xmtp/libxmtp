use super::{
    ConnectionExt, Sqlite,
    db_connection::DbConnection,
    group::{DmIdExt, StoredGroup},
    group_message::ContentType,
    schema::consent_records::{self, dsl},
};
use crate::{DbQuery, StorageError};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
};
use serde::{Deserialize, Serialize};
use xmtp_common::{ErrorCode, time::now_ns};
use xmtp_configuration::LEGACY_GROUP_ID_BYTES;
use xmtp_proto::{
    ConversionError,
    types::{ConversationType, GroupId},
    xmtp::device_sync::consent_backup::{ConsentSave, ConsentStateSave, ConsentTypeSave},
};
mod convert;

/// StoredConsentRecord holds a serialized ConsentRecord
#[derive(Insertable, Queryable, AsChangeset, Debug, Clone, Deserialize, Serialize)]
#[diesel(table_name = consent_records)]
#[diesel(primary_key(entity_type, entity))]
pub struct StoredConsentRecord {
    /// Enum, [`ConsentType`] representing the type of consent (conversation_id inbox_id, etc..)
    pub entity_type: ConsentType,
    /// Enum, [`ConsentState`] representing the state of consent (allowed, denied, etc..)
    pub state: ConsentState,
    /// The entity of what was consented (0x00 etc..)
    pub entity: String,

    pub consented_at_ns: i64,
    /// 0 when the record is not owed, else the mark that owes it.
    /// It never goes on the wire.
    #[serde(default)]
    pub owed_seq: i64,
}

impl StoredConsentRecord {
    /// A record at the current time that is not owed. The client writes
    /// through [`write_consent`]; this is for tests and fixtures.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn new(entity_type: ConsentType, state: ConsentState, entity: String) -> Self {
        Self {
            entity_type,
            state,
            entity,
            consented_at_ns: now_ns(),
            owed_seq: 0,
        }
    }

    fn from_parts(key: &ConsentKey, value: ConsentValue, owed_seq: i64) -> Self {
        Self {
            entity_type: key.entity_type,
            state: value.state,
            entity: key.entity.clone(),
            consented_at_ns: value.consented_at_ns,
            owed_seq,
        }
    }

    pub fn key(&self) -> ConsentKey {
        ConsentKey {
            entity_type: self.entity_type,
            entity: self.entity.clone(),
        }
    }

    pub fn value(&self) -> ConsentValue {
        ConsentValue {
            state: self.state,
            consented_at_ns: self.consented_at_ns,
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
crate::impl_store!(StoredConsentRecord, consent_records);

#[cfg(any(test, feature = "test-utils"))]
impl StoredConsentRecord {
    /// Merge this record with no cap, not owed. Tests only.
    pub fn merge_into(&self, db: &impl DbQuery) -> Result<Option<ConsentChange>, StorageError> {
        let write = ConsentWrite::Merge {
            value: self.value(),
            bound_ns: i64::MAX,
        };
        write_consent(db, &self.key(), write, Owed::Keep)
    }
}

/// The part of a record that merges. The key and the owed mark are not part of the order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConsentValue {
    pub state: ConsentState,
    pub consented_at_ns: i64,
}

/// A user decision. Its time comes from [`next_value`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    /// The app set this state.
    Set(ConsentState),
    /// The app created the conversation.
    Created,
    /// The app sent a message in the conversation.
    Sent,
}

impl Decision {
    /// The decision an app's send implies, if any.
    // implements: CONS-057
    pub fn on_send(conversation: ConversationType, content: ContentType) -> Option<Self> {
        let exempt = conversation.is_virtual()
            || matches!(
                content,
                ContentType::DeleteMessage | ContentType::LeaveRequest
            );
        (!exempt).then_some(Self::Sent)
    }
}

/// Every consent write in the client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConsentWrite {
    /// The user decided.
    Decide(Decision),
    /// A record that carries its own time, capped at `bound_ns`.
    Merge { value: ConsentValue, bound_ns: i64 },
    /// This installation published `sent`; store what its receivers stored.
    Settle { sent: ConsentValue, bound_ns: i64 },
}

/// How far after its carrier's time a received consent or root-key time may be.
pub const MAX_AHEAD_NS: i64 = xmtp_common::NS_IN_DAY;

impl ConsentWrite {
    /// A record from a sync message, capped by its envelope time.
    pub fn from_peer(value: ConsentValue, envelope_ns: i64) -> Self {
        Self::Merge {
            value,
            bound_ns: envelope_ns.saturating_add(MAX_AHEAD_NS),
        }
    }

    /// A record from an archive, capped by the local clock.
    pub fn from_archive(value: ConsentValue, now_ns: i64) -> Self {
        Self::Merge {
            value,
            bound_ns: now_ns.saturating_add(MAX_AHEAD_NS),
        }
    }

    /// A record this installation sent in a message the backend timed at `envelope_ns`.
    pub fn settle(sent: ConsentValue, envelope_ns: i64) -> Self {
        Self::Settle {
            sent,
            bound_ns: envelope_ns.saturating_add(MAX_AHEAD_NS),
        }
    }
}

/// Which value wins. No IO: the caller passes the stored value and the clock.
/// None when the write changes nothing.
// implements: CONS-053, CONS-054, CONS-055, CONS-056, CONS-057, SYNC-034
pub fn next_value(
    held: Option<ConsentValue>,
    write: ConsentWrite,
    now_ns: i64,
) -> Option<ConsentValue> {
    use ConsentState::*;
    let incoming = match write {
        // Lower the sender's own copy to what every receiver stored,
        // only while the key still holds the sent value. It never raises a time.
        ConsentWrite::Settle { sent, bound_ns } => {
            return (held == Some(sent) && sent.consented_at_ns > bound_ns).then_some(
                ConsentValue {
                    consented_at_ns: bound_ns,
                    ..sent
                },
            );
        }
        ConsentWrite::Decide(decision) => {
            let state = match decision {
                Decision::Set(state) => state,
                Decision::Created if held.is_none_or(|h| h.state == Unknown) => Allowed,
                Decision::Sent if held.is_none_or(|h| h.state != Allowed) => Allowed,
                Decision::Created | Decision::Sent => return None,
            };
            // After the clock and after what is held.
            let after_held = held.map_or(i64::MIN, |h| h.consented_at_ns.saturating_add(1));
            ConsentValue {
                state,
                consented_at_ns: now_ns.max(after_held),
            }
        }
        // A time too far ahead is stored at the limit, never refused.
        ConsentWrite::Merge { value, bound_ns } => ConsentValue {
            consented_at_ns: value.consented_at_ns.min(bound_ns),
            ..value
        },
    };
    // The greater (time, state) wins.
    let rank = |v: ConsentValue| (v.consented_at_ns, v.state);
    match held {
        Some(h) if rank(h) >= rank(incoming) => None,
        _ => Some(incoming),
    }
}

/// What a stored change does to the key's owed mark.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owed {
    /// Set a fresh mark: the change must reach the other installations.
    Mark,
    /// Leave the mark as it was: the change came from the selected sync group,
    /// or settles a value this installation published.
    Keep,
}

/// One stored change. `before` and `value.state` are reported states.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConsentChange {
    pub key: ConsentKey,
    pub before: ConsentState,
    pub value: ConsentValue,
    pub marked: bool,
}

/// The only function that writes `consent_records`. Callers use it inside a
/// state_write transaction; the compare-and-swap keeps it correct outside one.
// implements: SYNC-027
pub fn write_consent(
    db: &impl DbQuery,
    key: &ConsentKey,
    write: ConsentWrite,
    owed: Owed,
) -> Result<Option<ConsentChange>, StorageError> {
    loop {
        let held = db.consent_row(key)?;
        let held_value = held.as_ref().map(StoredConsentRecord::value);
        let Some(next) = next_value(held_value, write, now_ns()) else {
            return Ok(None);
        };
        let owed_seq = match owed {
            Owed::Mark => crate::sync_outbox::next_owed_seq(db)?,
            Owed::Keep => held.as_ref().map_or(0, |h| h.owed_seq),
        };
        let row = StoredConsentRecord::from_parts(key, next, owed_seq);
        if replace_consent_row(db, held.as_ref(), &row)? {
            return Ok(Some(ConsentChange {
                key: key.clone(),
                before: held_value.map_or(ConsentState::Unknown, |v| v.state),
                value: next,
                marked: owed == Owed::Mark,
            }));
        }
        // Another writer changed the row after the read. Read it again.
    }
}

/// Store `next` only if the row still holds `held`, or is still absent.
fn replace_consent_row(
    db: &impl ConnectionExt,
    held: Option<&StoredConsentRecord>,
    next: &StoredConsentRecord,
) -> Result<bool, crate::ConnectionError> {
    db.raw_query(|c| {
        let n = match held {
            None => diesel::insert_into(dsl::consent_records)
                .values(next)
                .on_conflict_do_nothing()
                .execute(c)?,
            Some(h) => diesel::update(dsl::consent_records.find((h.entity_type, &h.entity)))
                .filter(dsl::state.eq(h.state))
                .filter(dsl::consented_at_ns.eq(h.consented_at_ns))
                // A concurrent mark_all_owed is not overwritten.
                .filter(dsl::owed_seq.eq(h.owed_seq))
                .set(next)
                .execute(c)?,
        };
        Ok(n == 1)
    })
}

/// What a record is about, in canonical form.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ConsentKey {
    pub entity_type: ConsentType,
    pub entity: String,
}

/// The longest entity the client stores.
pub const MAX_CONSENT_ENTITY_LEN: usize = 256;

/// Why an app act cannot be stored.
#[derive(Debug, thiserror::Error, ErrorCode)]
pub enum ConsentActError {
    /// Invalid consent entity.
    ///
    /// The entity is too long, is not a group id, or names a DM whose dm id
    /// does not name this inbox. Not retryable.
    #[error("consent entity is not a valid key")]
    InvalidEntity,
    /// Conversation not held.
    ///
    /// The client holds no conversation with this group id. Not retryable
    /// until the conversation syncs; acting on the other member's inbox
    /// covers every DM with that person.
    #[error("the client does not hold conversation {0}")]
    ConversationNotHeld(String),
    #[error(transparent)]
    #[error_code(inherit)]
    Storage(#[from] StorageError),
}

impl ConsentKey {
    pub fn inbox(inbox_id: &str) -> Self {
        Self {
            entity_type: ConsentType::InboxId,
            entity: inbox_id.to_ascii_lowercase(),
        }
    }

    pub fn group(group_id: &[u8]) -> Self {
        Self {
            entity_type: ConsentType::ConversationId,
            entity: hex::encode(group_id),
        }
    }

    /// A held conversation's key: a DM's is its peer's inbox, any other
    /// group's its id. None for a DM whose dm id does not name the own inbox.
    // implements: CONS-052
    pub fn of_group(group: &StoredGroup, own_inbox: &str) -> Option<Self> {
        match &group.dm_id {
            None => Some(Self::group(group.id.as_ref())),
            Some(dm_id) => dm_id
                .other_inbox_id(own_inbox)
                .map(|peer| Self::inbox(&peer)),
        }
    }

    /// An entity as named, lowercased. None when it is too long to store.
    // implements: CONS-051
    pub fn parse(entity_type: ConsentType, entity: &str) -> Option<Self> {
        (entity.len() <= MAX_CONSENT_ENTITY_LEN).then(|| Self {
            entity_type,
            entity: entity.to_ascii_lowercase(),
        })
    }

    /// The key of an entity an app sets. An inbox is taken as named. A group
    /// id must name a conversation the client holds, and the act uses that
    /// conversation's key, so a DM act lands on the peer's inbox record.
    // implements: CONS-052
    pub fn for_app(
        db: &impl DbQuery,
        own_inbox: &str,
        entity_type: ConsentType,
        entity: &str,
    ) -> Result<Self, ConsentActError> {
        let key = Self::parse(entity_type, entity).ok_or(ConsentActError::InvalidEntity)?;
        if key.entity_type == ConsentType::InboxId {
            return Ok(key);
        }
        let id = key.group_id().ok_or(ConsentActError::InvalidEntity)?;
        let group = db
            .find_group(&id)
            .map_err(StorageError::from)?
            .ok_or_else(|| ConsentActError::ConversationNotHeld(key.entity.clone()))?;
        // A DM restored from another inbox's archive has no key.
        Self::of_group(&group, own_inbox).ok_or(ConsentActError::InvalidEntity)
    }

    /// The group id this key names, when it is a conversation key.
    pub fn group_id(&self) -> Option<GroupId> {
        (self.entity_type == ConsentType::ConversationId)
            .then(|| hex::decode(&self.entity).ok())
            .flatten()
            .and_then(|raw| GroupId::try_from(raw.as_slice()).ok())
    }

    /// A 16-byte group id is a pre-8.0 group. When the client holds it as a
    /// DM with a peer, a record for it is that peer's inbox record. Returns
    /// the peer's key and the DM. No other id moves: no Welcome can make a
    /// live DM with a 16-byte id, but a former member of a group
    /// can make one with its 32-byte id.
    // implements: CONS-061
    pub fn pre8_dm(
        &self,
        db: &impl DbQuery,
        own_inbox: &str,
    ) -> Result<Option<(Self, StoredGroup)>, StorageError> {
        let Some(id) = self
            .group_id()
            .filter(|id| id.as_ref().len() == LEGACY_GROUP_ID_BYTES)
        else {
            return Ok(None);
        };
        Ok(db
            .find_group(&id)?
            .filter(|g| g.dm_id.is_some())
            .and_then(|g| Self::of_group(&g, own_inbox).map(|peer| (peer, g))))
    }
}

/// The one parser for received and imported records. None for an unusable record.
// implements: CONS-051
pub fn parse_consent_save(save: &ConsentSave) -> Option<(ConsentKey, ConsentValue)> {
    // Rejects unspecified and address types and an unspecified state.
    let entity_type = ConsentType::try_from(save.entity_type()).ok()?;
    let state = ConsentState::try_from(save.state()).ok()?;
    let value = ConsentValue {
        state,
        consented_at_ns: save.consented_at_ns,
    };
    Some((ConsentKey::parse(entity_type, &save.entity)?, value))
}

/// A record can name a 16-byte DM before the client holds the DM: a
/// consent-only archive, or a peer that holds such a record. When that DM is
/// restored, the record under its id moves to the peer. Called only for a
/// new Restored DM.
// implements: CONS-061
pub fn fold_into_inbox(
    db: &impl DbQuery,
    own_inbox: &str,
    group: &StoredGroup,
) -> Result<Option<ConsentChange>, StorageError> {
    let orphan_key = ConsentKey::group(group.id.as_ref());
    let Some((peer, _)) = orphan_key.pre8_dm(db, own_inbox)? else {
        return Ok(None);
    };
    let orphan: Option<StoredConsentRecord> = db.raw_query(|c| {
        diesel::delete(dsl::consent_records.find((orphan_key.entity_type, &orphan_key.entity)))
            .get_result(c)
            .optional()
    })?;
    let Some(orphan) = orphan else {
        return Ok(None);
    };
    // The record was capped when it was stored.
    let write = ConsentWrite::Merge {
        value: orphan.value(),
        bound_ns: i64::MAX,
    };
    write_consent(db, &peer, write, Owed::Mark)
}

/// The consent an app, a filter, push, and fork recovery see for a
/// conversation: the record of its key, or unknown.
// implements: CONS-001, CONS-052
pub fn conversation_consent(
    db: &impl DbQuery,
    own_inbox: &str,
    group_id: &GroupId,
) -> Result<ConsentState, StorageError> {
    let key = match db.find_group(group_id)? {
        Some(group) => ConsentKey::of_group(&group, own_inbox),
        None => Some(ConsentKey::group(group_id.as_ref())),
    };
    Ok(match key {
        Some(key) => db
            .consent_row(&key)?
            .map_or(ConsentState::Unknown, |r| r.state),
        None => ConsentState::Unknown,
    })
}

pub trait QueryConsentRecord {
    /// The stored record of `key`.
    fn consent_row(
        &self,
        key: &ConsentKey,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError>;

    fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError>;
}

impl<C: ConnectionExt> QueryConsentRecord for DbConnection<C> {
    fn consent_row(
        &self,
        key: &ConsentKey,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::consent_records
                .find((key.entity_type, &key.entity))
                .first(conn)
                .optional()
        })
    }

    fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| consent_records::table.load(conn))
    }
}

impl<T: QueryConsentRecord + ?Sized> QueryConsentRecord for &T {
    fn consent_row(
        &self,
        key: &ConsentKey,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        (**self).consent_row(key)
    }

    fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).consent_records()
    }
}

#[repr(i32)]
#[derive(
    Debug,
    Copy,
    Clone,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    AsExpression,
    FromSqlRow,
    Hash,
    PartialOrd,
    Ord,
)]
#[diesel(sql_type = Integer)]
/// Type of consent record stored
pub enum ConsentType {
    /// Consent is for a conversation
    ConversationId = 1,
    /// Consent is for an inbox
    InboxId = 2,
}

impl ToSql<Integer, Sqlite> for ConsentType
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for ConsentType
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            1 => Ok(ConsentType::ConversationId),
            2 => Ok(ConsentType::InboxId),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[repr(i32)]
#[derive(
    Debug,
    Copy,
    Clone,
    Serialize,
    Deserialize,
    Eq,
    PartialEq,
    AsExpression,
    FromSqlRow,
    PartialOrd,
    Ord,
)]
#[diesel(sql_type = Integer)]
/// The state of the consent. The declaration order is the merge order of
/// Unknown < allowed < denied.
pub enum ConsentState {
    /// Consent is unknown
    Unknown = 0,
    /// Consent is allowed
    Allowed = 1,
    /// Consent is denied
    Denied = 2,
}

impl ToSql<Integer, Sqlite> for ConsentState
where
    i32: ToSql<Integer, Sqlite>,
{
    fn to_sql<'b>(&'b self, out: &mut Output<'b, '_, Sqlite>) -> serialize::Result {
        out.set_value(*self as i32);
        Ok(IsNull::No)
    }
}

impl FromSql<Integer, Sqlite> for ConsentState
where
    i32: FromSql<Integer, Sqlite>,
{
    fn from_sql(bytes: <Sqlite as Backend>::RawValue<'_>) -> deserialize::Result<Self> {
        match i32::from_sql(bytes)? {
            0 => Ok(ConsentState::Unknown),
            1 => Ok(ConsentState::Allowed),
            2 => Ok(ConsentState::Denied),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        sync_outbox::{OwedPage, clear_owed, mark_all_owed, owed_page, settle_sent},
        test_utils::with_connection,
    };
    use ConsentState::*;

    fn v(state: ConsentState, consented_at_ns: i64) -> ConsentValue {
        ConsentValue {
            state,
            consented_at_ns,
        }
    }

    fn merge(value: ConsentValue) -> ConsentWrite {
        ConsentWrite::Merge {
            value,
            bound_ns: i64::MAX,
        }
    }

    // verifies: CONS-053, CONS-054, CONS-055, CONS-056, CONS-057, SYNC-034
    #[xmtp_common::test]
    fn next_value_rules() {
        let now = 1_000;
        let ahead = now + 23 * xmtp_common::NS_IN_HOUR;
        let set = |state| ConsentWrite::Decide(Decision::Set(state));
        let created = ConsentWrite::Decide(Decision::Created);
        let sent = ConsentWrite::Decide(Decision::Sent);
        let settle = |sent, bound_ns| ConsentWrite::Settle { sent, bound_ns };
        let cases: [(Option<ConsentValue>, ConsentWrite, Option<ConsentValue>); _] = [
            // A decision comes after the held time and the clock.
            (
                Some(v(Allowed, ahead)),
                set(Denied),
                Some(v(Denied, ahead + 1)),
            ),
            (Some(v(Denied, 100)), set(Denied), Some(v(Denied, now))),
            (Some(v(Denied, 2_000)), set(Denied), Some(v(Denied, 2_001))),
            (None, set(Unknown), Some(v(Unknown, now))),
            // Creating allows an absent or unknown key only.
            (None, created, Some(v(Allowed, now))),
            (Some(v(Unknown, 5_000)), created, Some(v(Allowed, 5_001))),
            (Some(v(Allowed, 10)), created, None),
            (Some(v(Denied, 10)), created, None),
            // Sending allows unless the key reads allowed.
            (None, sent, Some(v(Allowed, now))),
            (Some(v(Unknown, 10)), sent, Some(v(Allowed, now))),
            (Some(v(Denied, 2_000)), sent, Some(v(Allowed, 2_001))),
            (Some(v(Allowed, 10)), sent, None),
            // The greater (time, state) wins; a tie goes to denied.
            (
                Some(v(Allowed, 10)),
                merge(v(Denied, 20)),
                Some(v(Denied, 20)),
            ),
            (
                Some(v(Allowed, 10)),
                merge(v(Denied, 10)),
                Some(v(Denied, 10)),
            ),
            (Some(v(Denied, 10)), merge(v(Allowed, 10)), None),
            (Some(v(Denied, 10)), merge(v(Denied, 10)), None),
            (Some(v(Denied, 10)), merge(v(Unknown, 9)), None),
            // Time 0 merges like any time, and any timed record beats it.
            (None, merge(v(Allowed, 0)), Some(v(Allowed, 0))),
            (
                Some(v(Allowed, 0)),
                merge(v(Unknown, 1)),
                Some(v(Unknown, 1)),
            ),
            // A time past the bound is stored at the bound.
            (
                None,
                ConsentWrite::Merge {
                    value: v(Allowed, 500),
                    bound_ns: 300,
                },
                Some(v(Allowed, 300)),
            ),
            (
                Some(v(Denied, 300)),
                ConsentWrite::Merge {
                    value: v(Allowed, 500),
                    bound_ns: 300,
                },
                None,
            ),
            // Settle lowers only the exact held value above the bound.
            (
                Some(v(Denied, 500)),
                settle(v(Denied, 500), 300),
                Some(v(Denied, 300)),
            ),
            (Some(v(Denied, 501)), settle(v(Denied, 500), 300), None),
            (Some(v(Allowed, 500)), settle(v(Denied, 500), 300), None),
            (Some(v(Denied, 300)), settle(v(Denied, 300), 300), None),
            (Some(v(Denied, 200)), settle(v(Denied, 200), 300), None),
            (None, settle(v(Denied, 500), 300), None),
        ];
        for (index, (held, write, expected)) in cases.into_iter().enumerate() {
            assert_eq!(
                next_value(held, write, now),
                expected,
                "case {index}: {write:?}"
            );
        }
        assert_eq!(
            ConsentWrite::from_peer(v(Allowed, 0), 7),
            ConsentWrite::Merge {
                value: v(Allowed, 0),
                bound_ns: 7 + MAX_AHEAD_NS
            }
        );
        for (conversation, content, implied) in [
            (ConversationType::Group, ContentType::Text, true),
            (ConversationType::Dm, ContentType::Reply, true),
            (ConversationType::Group, ContentType::DeleteMessage, false),
            (ConversationType::Dm, ContentType::LeaveRequest, false),
            (ConversationType::Sync, ContentType::Text, false),
            (ConversationType::Oneshot, ContentType::Text, false),
        ] {
            assert_eq!(
                Decision::on_send(conversation, content).is_some(),
                implied,
                "{conversation:?} {content:?}"
            );
        }
    }

    /// Records merged in any order, each twice, leave one value: the greatest.
    // verifies: CONS-053
    #[xmtp_common::test]
    fn merge_is_order_free() {
        let values = [
            v(Unknown, 10),
            v(Allowed, 10),
            v(Denied, 10),
            v(Allowed, 20),
            v(Unknown, 30),
            v(Denied, 5),
        ];
        let mut order: Vec<usize> = (0..values.len()).collect();
        let mut seen = 0;
        // Heap's algorithm over every permutation.
        let mut c = vec![0; order.len()];
        let mut check = |order: &[usize]| {
            let held = order.iter().chain(order).fold(None, |held, &i| {
                next_value(held, merge(values[i]), 0).or(held)
            });
            assert_eq!(held, Some(v(Unknown, 30)), "{order:?}");
            seen += 1;
        };
        check(&order);
        let mut i = 0;
        while i < order.len() {
            if c[i] < i {
                order.swap(if i % 2 == 0 { 0 } else { c[i] }, i);
                check(&order);
                c[i] += 1;
                i = 0;
            } else {
                c[i] = 0;
                i += 1;
            }
        }
        assert_eq!(seen, 720);
    }

    /// Commits a competing write right after the first read, as a concurrent
    /// writer outside the transaction would.
    struct WriteAfterFirstRead<C> {
        inner: C,
        competing: std::sync::Mutex<Option<StoredConsentRecord>>,
    }

    impl<C: ConnectionExt> ConnectionExt for WriteAfterFirstRead<C> {
        fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
        where
            F: FnOnce(&mut diesel::SqliteConnection) -> Result<T, diesel::result::Error>,
        {
            let result = self.inner.raw_query(fun);
            if let Some(row) = self.competing.lock().unwrap().take() {
                self.inner.raw_query(|c| {
                    diesel::replace_into(dsl::consent_records)
                        .values(&row)
                        .execute(c)
                })?;
            }
            result
        }

        fn disconnect(&self) -> Result<(), crate::ConnectionError> {
            self.inner.disconnect()
        }

        fn reconnect(&self) -> Result<(), crate::ConnectionError> {
            self.inner.reconnect()
        }
    }

    // verifies: SYNC-027, SYNC-033
    #[xmtp_common::test(unwrap_try = true)]
    fn write_consent_marks_and_swap() {
        with_connection(|conn| {
            let key = ConsentKey::inbox("Peer");
            assert_eq!(key.entity, "peer");
            let decide = |state| ConsentWrite::Decide(Decision::Set(state));
            let first = write_consent(conn, &key, decide(Denied), Owed::Mark)?.unwrap();
            assert!(first.marked);
            let mark = conn.consent_row(&key)?.unwrap().owed_seq;
            assert!(mark > 0);
            // A received value keeps the held mark and never clears it.
            let later = merge(v(Allowed, i64::MAX / 2));
            let kept = write_consent(conn, &key, later, Owed::Keep)?.unwrap();
            assert!(!kept.marked);
            assert_eq!(conn.consent_row(&key)?.unwrap().owed_seq, mark);
            // A new key received with Keep is not owed.
            let other = ConsentKey::group(&[1; 32]);
            write_consent(conn, &other, merge(v(Allowed, 1)), Owed::Keep)?;
            assert_eq!(conn.consent_row(&other)?.unwrap().owed_seq, 0);
            // A write that changes nothing stores nothing and keeps the mark.
            assert!(write_consent(conn, &key, merge(v(Unknown, 1)), Owed::Mark)?.is_none());
            assert_eq!(conn.consent_row(&key)?.unwrap().owed_seq, mark);
            // Each change draws a fresh mark.
            write_consent(conn, &key, decide(Allowed), Owed::Mark)?;
            assert!(conn.consent_row(&key)?.unwrap().owed_seq > mark);

            // A change committed between the read and the write is read again.
            let racing = ConsentKey::inbox("racing");
            write_consent(conn, &racing, merge(v(Allowed, 10)), Owed::Keep)?;
            let competing = StoredConsentRecord::from_parts(&racing, v(Denied, 50), 0);
            let raced = crate::DbConnection::new(WriteAfterFirstRead {
                inner: conn.conn.clone(),
                competing: std::sync::Mutex::new(Some(competing)),
            });
            let change = write_consent(&raced, &racing, merge(v(Allowed, 40)), Owed::Keep)?;
            assert!(change.is_none(), "the competing Denied@50 is greater");
            assert_eq!(conn.consent_row(&racing)?.unwrap().value(), v(Denied, 50));
            Ok::<_, StorageError>(())
        })?;
    }

    /// The page as a sync message carries it.
    fn sent(page: &OwedPage) -> Vec<xmtp_proto::xmtp::device_sync::content::PreferenceUpdate> {
        use xmtp_proto::xmtp::device_sync::content::{PreferenceUpdate, preference_update::Update};
        page.consent
            .iter()
            .map(|record| PreferenceUpdate {
                update: Some(Update::Consent(record.clone().into())),
            })
            .collect()
    }

    fn settle_and_clear(conn: &impl DbQuery, page: &OwedPage, e0: i64) -> Result<(), StorageError> {
        settle_sent(conn, &sent(page), e0)?;
        Ok(clear_owed(conn, page)?)
    }

    /// A published page settles its values to the cap and clears only marks
    /// it read; a key marked again while the page was in flight stays owed.
    // verifies: SYNC-033, SYNC-034, SYNC-028
    #[xmtp_common::test(unwrap_try = true)]
    fn sender_settles_to_the_cap() {
        with_connection(|conn| {
            let e0 = 1_000_000;
            let ahead = ConsentKey::inbox("ahead");
            let near = ConsentKey::inbox("near");
            let decided = |key, value| write_consent(conn, key, merge(value), Owed::Mark);
            decided(&ahead, v(Allowed, e0 + 2 * xmtp_common::NS_IN_DAY))?;
            decided(&near, v(Denied, e0 + 5))?;
            let page = owed_page(conn, 1_000, 256 * 1024)?;
            assert_eq!(page.consent.len(), 2);
            settle_and_clear(conn, &page, e0)?;
            let row = conn.consent_row(&ahead)?.unwrap();
            assert_eq!(row.value(), v(Allowed, e0 + MAX_AHEAD_NS));
            assert_eq!(row.owed_seq, 0);
            assert_eq!(conn.consent_row(&near)?.unwrap().value(), v(Denied, e0 + 5));
            assert!(owed_page(conn, 1_000, 256 * 1024)?.is_empty());

            // A newcomer's Welcome marks every key while a page is in flight.
            decided(&ahead, v(Allowed, e0 + 3 * xmtp_common::NS_IN_DAY))?;
            let page = owed_page(conn, 1_000, 256 * 1024)?;
            mark_all_owed(conn)?;
            settle_and_clear(conn, &page, e0)?;
            let row = conn.consent_row(&ahead)?.unwrap();
            assert_eq!(row.value(), v(Allowed, e0 + MAX_AHEAD_NS));
            assert!(row.owed_seq > 0, "the newcomer's mark stays");
            let again = owed_page(conn, 1_000, 256 * 1024)?;
            assert_eq!(again.consent.len(), 2);
            assert!(again.root_key.is_some(), "the root key is owed too");
            Ok::<_, StorageError>(())
        })?;
    }

    /// A page fits a quarter of a small deployment's envelope and holds at
    /// most the update count; the root key goes first.
    // verifies: SYNC-029
    #[xmtp_common::test(unwrap_try = true)]
    fn owed_pages_fit_the_envelope_limit() {
        with_connection(|conn| {
            for i in 0..100 {
                let key = ConsentKey::inbox(&format!("{i:0>256}"));
                write_consent(conn, &key, merge(v(Allowed, 1)), Owed::Mark)?;
            }
            assert_eq!(owed_page(conn, 1_000, 16 * 1024)?.consent.len(), 40);
            assert_eq!(owed_page(conn, 10, 256 * 1024)?.consent.len(), 10);
            crate::user_preferences::ensure_root_key(conn)?;
            let page = owed_page(conn, 10, 256 * 1024)?;
            assert!(page.root_key.is_some());
            assert_eq!(page.consent.len(), 9);
            Ok::<_, StorageError>(())
        })?;
    }

    /// Readers and acts use the conversation's key; an act on an unheld
    /// conversation is refused.
    // verifies: CONS-051, CONS-052, CONS-061
    #[xmtp_common::test(unwrap_try = true)]
    fn keys_follow_the_conversation() {
        with_connection(|conn| {
            use crate::{Store, group::tests::generate_group};
            let own = "me";
            let mut dm = generate_group(None);
            dm.id = GroupId::try_from([7u8; 16].as_slice())?;
            dm.dm_id = Some("dm:me:peer".into());
            dm.store(conn)?;
            let group = generate_group(None);
            group.store(conn)?;
            let for_app = |kind, entity: &str| ConsentKey::for_app(conn, own, kind, entity);
            let dm_hex = hex::encode(dm.id);
            assert_eq!(
                for_app(ConsentType::ConversationId, &dm_hex)?,
                ConsentKey::inbox("peer")
            );
            assert_eq!(
                for_app(
                    ConsentType::ConversationId,
                    &hex::encode(group.id).to_uppercase()
                )?,
                ConsentKey::group(group.id.as_ref())
            );
            // Q15: an unheld conversation is refused and nothing is stored; an
            // inbox the client has never seen is taken as named.
            let unheld = hex::encode([9u8; 32]);
            assert!(matches!(
                for_app(ConsentType::ConversationId, &unheld),
                Err(ConsentActError::ConversationNotHeld(_))
            ));
            assert!(conn.consent_row(&ConsentKey::group(&[9u8; 32]))?.is_none());
            let stranger = for_app(ConsentType::InboxId, "NeverSeen")?;
            assert_eq!(stranger, ConsentKey::inbox("neverseen"));
            write_consent(
                conn,
                &stranger,
                ConsentWrite::Decide(Decision::Set(Denied)),
                Owed::Mark,
            )?;
            assert_eq!(conn.consent_row(&stranger)?.unwrap().state, Denied);
            assert!(matches!(
                for_app(
                    ConsentType::InboxId,
                    &"a".repeat(MAX_CONSENT_ENTITY_LEN + 1)
                ),
                Err(ConsentActError::InvalidEntity)
            ));
            // A 16-byte record held from before folds into the peer when the DM is restored.
            let orphan = ConsentKey::group(dm.id.as_ref());
            write_consent(conn, &orphan, merge(v(Denied, 10)), Owed::Keep)?;
            let folded = fold_into_inbox(conn, own, &dm)?.unwrap();
            assert_eq!(
                (folded.key.clone(), folded.value),
                (ConsentKey::inbox("peer"), v(Denied, 10))
            );
            assert!(conn.consent_row(&orphan)?.is_none());
            assert_eq!(conversation_consent(conn, own, &dm.id)?, Denied);
            assert_eq!(conversation_consent(conn, own, &group.id)?, Unknown);
            Ok::<_, Box<dyn std::error::Error>>(())
        })?;
    }
}
