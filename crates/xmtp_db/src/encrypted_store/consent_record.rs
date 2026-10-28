use super::{ConnectionExt, Sqlite, group::StoredGroup};
use super::{
    db_connection::DbConnection,
    schema::{
        consent_records::{self, dsl},
        groups::dsl as groups_dsl,
    },
};
use crate::{DbQuery, StorageError, impl_store};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql, FromSqlRow},
    expression::AsExpression,
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    upsert::excluded,
};
use serde::{Deserialize, Serialize};
use xmtp_common::time::now_ns;
use xmtp_proto::{
    ConversionError,
    xmtp::device_sync::consent_backup::{ConsentSave, ConsentStateSave, ConsentTypeSave},
};
mod convert;

/// The result of merging a received consent record (sync or archive import).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConsentMerge {
    /// The row was written: the record is new or wins the time and state order.
    pub applied: bool,
    /// The app-visible state changed. A newer record with the stored state is
    /// applied (its time moves) but is not a state change.
    pub state_changed: bool,
}

/// StoredConsentRecord holds a serialized ConsentRecord
#[derive(Insertable, Queryable, Debug, Clone, Eq, Deserialize, Serialize)]
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
}

impl PartialEq for StoredConsentRecord {
    fn eq(&self, other: &Self) -> bool {
        self.entity == other.entity
            && self.entity_type == other.entity_type
            && self.state == other.state
    }
}

impl StoredConsentRecord {
    pub fn new(entity_type: ConsentType, state: ConsentState, entity: String) -> Self {
        Self {
            entity_type,
            state,
            entity,
            consented_at_ns: now_ns(),
        }
    }

    /// Give a group with a dm id the greatest consent record held for any
    /// group with that dm id, with that record's own time. The copy is a
    /// merge, so a record already stored for the group only loses to a
    /// greater one. Returns the copy when it was written.
    // implements: DMS-010
    pub fn stitch_dm_consent(
        conn: &impl DbQuery,
        group: &StoredGroup,
    ) -> Result<Option<Self>, StorageError> {
        let Some(dm_id) = &group.dm_id else {
            return Ok(None);
        };
        let Some(greatest) = conn.find_consent_by_dm_id(dm_id)?.into_iter().next() else {
            return Ok(None);
        };
        let record = Self {
            entity_type: ConsentType::ConversationId,
            state: greatest.state,
            entity: hex::encode(group.id),
            consented_at_ns: greatest.consented_at_ns,
        };
        let merge = conn.insert_newer_consent_record(record.clone())?;
        Ok(merge.applied.then_some(record))
    }
}

impl_store!(StoredConsentRecord, consent_records);

pub trait QueryConsentRecord {
    /// Returns the consent_records for the given entity up
    fn get_consent_record(
        &self,
        entity: String,
        entity_type: ConsentType,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError>;

    fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError>;

    fn consent_records_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError>;

    /// Merge a received record. `applied` is set when the record is inserted or
    /// wins the time and state order; `state_changed` only when the stored
    /// state differs afterwards.
    fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<ConsentMerge, crate::ConnectionError>;

    /// Store app decisions. A record that is new or changes the stored state
    /// is written with its time and returned; a record with the stored state
    /// is skipped and keeps the stored time.
    fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError>;

    fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError>;

    /// Conversation records of every group with `dm_id`, greatest first:
    /// newest time, then greatest state.
    fn find_consent_by_dm_id(
        &self,
        dm_id: &str,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError>;
}

impl<C: ConnectionExt> QueryConsentRecord for DbConnection<C> {
    /// Returns the consent_records for the given entity up
    fn get_consent_record(
        &self,
        entity: String,
        entity_type: ConsentType,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| {
            dsl::consent_records
                .filter(dsl::entity.eq(entity))
                .filter(dsl::entity_type.eq(entity_type))
                .first(conn)
                .optional()
        })
    }

    fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| super::schema::consent_records::table.load(conn))
    }

    fn consent_records_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        let query = consent_records::table
            .order_by((consent_records::entity_type, consent_records::entity))
            .limit(limit)
            .offset(offset);

        self.raw_query(|conn| query.load::<StoredConsentRecord>(conn))
    }

    /// Merge a received record. A newer record with the stored state is
    /// applied and only moves the stored time: the client's own act echoed back
    /// from another installation is not a state change.
    // implements: CONS-010
    fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<ConsentMerge, crate::ConnectionError> {
        use diesel::query_dsl::methods::FilterDsl;

        self.raw_query(|conn| {
            // Two auto-committed statements: a deferred transaction that reads
            // and then writes would fail its lock upgrade without waiting.
            let stored_state: Option<ConsentState> = dsl::consent_records
                .find((&record.entity_type, &record.entity))
                .select(dsl::state)
                .first(conn)
                .optional()?;
            let applied = diesel::insert_into(dsl::consent_records)
                .values(&record)
                .on_conflict((dsl::entity_type, dsl::entity))
                .do_update()
                .set((
                    dsl::state.eq(excluded(dsl::state)),
                    dsl::consented_at_ns.eq(excluded(dsl::consented_at_ns)),
                ))
                .filter(
                    dsl::consented_at_ns.lt(excluded(dsl::consented_at_ns)).or(
                        dsl::consented_at_ns
                            .eq(excluded(dsl::consented_at_ns))
                            .and(dsl::state.lt(excluded(dsl::state))),
                    ),
                )
                .execute(conn)?;
            let applied = applied != 0;
            Ok(ConsentMerge {
                applied,
                state_changed: applied && stored_state != Some(record.state),
            })
        })
    }

    /// Write only the records that are new or change the stored state, each
    /// with its own time. A same-state record keeps the stored time.
    fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        let mut query = consent_records::table
            .into_boxed()
            .filter(false.into_sql::<diesel::sql_types::Bool>());
        let primary_keys: Vec<_> = records
            .iter()
            .map(|r| (&r.entity, &r.entity_type))
            .collect();
        for (entity, entity_type) in primary_keys {
            query = query.or_filter(
                consent_records::entity_type
                    .eq(entity_type)
                    .and(consent_records::entity.eq(entity)),
            );
        }

        let changed = self.raw_query(|conn| {
            let existing: Vec<StoredConsentRecord> = query.load(conn)?;
            let changed: Vec<_> = records
                .iter()
                .filter(|r| !existing.contains(r))
                .cloned()
                .collect();

            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                for record in &changed {
                    diesel::insert_into(dsl::consent_records)
                        .values(record)
                        .on_conflict((dsl::entity_type, dsl::entity))
                        .do_update()
                        .set((
                            dsl::state.eq(excluded(dsl::state)),
                            dsl::consented_at_ns.eq(excluded(dsl::consented_at_ns)),
                        ))
                        .execute(conn)?;
                }
                Ok(())
            })?;

            Ok(changed)
        })?;

        Ok(changed)
    }

    fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| {
            let maybe_inserted_consent_record: Option<StoredConsentRecord> =
                diesel::insert_into(dsl::consent_records)
                    .values(record)
                    .on_conflict_do_nothing()
                    .get_result(conn)
                    .optional()?;

            // if record was not inserted...
            if maybe_inserted_consent_record.is_none() {
                return dsl::consent_records
                    .find((&record.entity_type, &record.entity))
                    .first(conn)
                    .optional();
            }

            Ok(None)
        })
    }

    fn find_consent_by_dm_id(
        &self,
        dm_id: &str,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query(|conn| {
            // First, get all group IDs for this dm_id
            let group_ids: Vec<Vec<u8>> = groups_dsl::groups
                .filter(groups_dsl::dm_id.eq(dm_id))
                .select(groups_dsl::id)
                .load::<Vec<u8>>(conn)?;

            // Convert to hex strings
            let group_id_hexes: Vec<String> = group_ids.iter().map(hex::encode).collect();

            // Query consent records
            dsl::consent_records
                .filter(dsl::entity.eq_any(group_id_hexes))
                .filter(dsl::entity_type.eq(ConsentType::ConversationId))
                // State breaks a time tie as insert_newer_consent_record does:
                // ConsentState's integer order is Unknown < Allowed < Denied.
                .order((dsl::consented_at_ns.desc(), dsl::state.desc()))
                .load::<StoredConsentRecord>(conn)
        })
    }
}

impl<T: QueryConsentRecord + ?Sized> QueryConsentRecord for &T {
    fn get_consent_record(
        &self,
        entity: String,
        entity_type: ConsentType,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        (**self).get_consent_record(entity, entity_type)
    }

    fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).consent_records()
    }

    fn consent_records_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).consent_records_paged(limit, offset)
    }

    fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<ConsentMerge, crate::ConnectionError> {
        (**self).insert_newer_consent_record(record)
    }

    fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).insert_or_replace_consent_records(records)
    }

    fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError> {
        (**self).maybe_insert_consent_record_return_existing(record)
    }

    fn find_consent_by_dm_id(
        &self,
        dm_id: &str,
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        (**self).find_consent_by_dm_id(dm_id)
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
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
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// The state of the consent
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
    use crate::{Store, XmtpTestDb, group::tests::generate_group, test_utils::with_connection};

    fn record(
        entity_type: ConsentType,
        entity: &str,
        state: ConsentState,
        consented_at_ns: i64,
    ) -> StoredConsentRecord {
        StoredConsentRecord {
            entity_type,
            state,
            entity: entity.into(),
            consented_at_ns,
        }
    }

    // verifies: CONS-010
    #[xmtp_common::test(unwrap_try = true)]
    fn newer_same_state_blocks_an_older_third_record_for_both_entity_types() {
        with_connection(|conn| {
            for (entity_type, entity) in [
                (ConsentType::ConversationId, "conversation-one"),
                (ConsentType::InboxId, "inbox-one"),
            ] {
                let first = record(entity_type, entity, ConsentState::Allowed, 10);
                let newest = record(entity_type, entity, ConsentState::Allowed, 30);
                let middle = record(entity_type, entity, ConsentState::Denied, 20);

                assert!(conn.insert_newer_consent_record(first)?.state_changed);
                // The same state at a later time moves the stored time only.
                assert!(
                    !conn
                        .insert_newer_consent_record(newest.clone())?
                        .state_changed
                );
                assert!(!conn.insert_newer_consent_record(middle)?.state_changed);
                let stored = conn
                    .get_consent_record(entity.into(), entity_type)?
                    .unwrap();
                assert_eq!(stored.state, ConsentState::Allowed);
                assert_eq!(stored.consented_at_ns, 30);
                assert!(!conn.insert_newer_consent_record(newest)?.state_changed);

                assert!(
                    conn.insert_newer_consent_record(record(
                        entity_type,
                        entity,
                        ConsentState::Denied,
                        40,
                    ))?
                    .state_changed
                );
                assert!(
                    !conn
                        .insert_newer_consent_record(record(
                            entity_type,
                            entity,
                            ConsentState::Allowed,
                            35,
                        ))?
                        .state_changed
                );
                let stored = conn
                    .get_consent_record(entity.into(), entity_type)?
                    .unwrap();
                assert_eq!(stored.state, ConsentState::Denied);
                assert_eq!(stored.consented_at_ns, 40);
            }
            Ok::<(), crate::ConnectionError>(())
        })?;
    }

    /// The client's own act, echoed back from the sync group with a later
    /// time and the same state, moves the stored time but is not a change.
    // verifies: CONS-010
    #[xmtp_common::test(unwrap_try = true)]
    fn own_sync_echo_with_the_same_state_is_not_a_change() {
        with_connection(|conn| {
            for entity_type in [ConsentType::ConversationId, ConsentType::InboxId] {
                let entity = "entity";
                assert!(
                    conn.insert_newer_consent_record(record(
                        entity_type,
                        entity,
                        ConsentState::Allowed,
                        10,
                    ))?
                    .state_changed
                );
                // The local act changes the state, so it stores its time.
                conn.insert_or_replace_consent_records(&[record(
                    entity_type,
                    entity,
                    ConsentState::Denied,
                    20,
                )])?;
                let stored = conn
                    .get_consent_record(entity.into(), entity_type)?
                    .unwrap();
                assert_eq!(
                    (stored.state, stored.consented_at_ns),
                    (ConsentState::Denied, 20)
                );
                // Its echo is newer but carries the same state: applied (the
                // time moves) but not a state change.
                let merge = conn.insert_newer_consent_record(record(
                    entity_type,
                    entity,
                    ConsentState::Denied,
                    25,
                ))?;
                assert!(merge.applied);
                assert!(
                    !merge.state_changed,
                    "an echo of the stored state must not report a change"
                );
                let stored = conn
                    .get_consent_record(entity.into(), entity_type)?
                    .unwrap();
                assert_eq!(
                    (stored.state, stored.consented_at_ns),
                    (ConsentState::Denied, 25)
                );
                // A newer different state is still a change.
                assert!(
                    conn.insert_newer_consent_record(record(
                        entity_type,
                        entity,
                        ConsentState::Allowed,
                        30,
                    ))?
                    .state_changed
                );
            }
            Ok::<(), crate::ConnectionError>(())
        })?;
    }

    /// Installation A repeats a local Denied before B's newer Allowed arrives;
    /// both installations end on B's record because a local repeat does not
    /// move the stored time.
    // verifies: CONS-010
    #[xmtp_common::test(unwrap_try = true)]
    fn installations_converge_when_a_local_repeat_precedes_a_newer_remote_record() {
        with_connection(|conn| {
            let kind = ConsentType::ConversationId;
            // Installation A. The local repeat keeps the stored time: the waived
            // gap in docs/specs/waivers.toml for the local same-state act time.
            // Update this test when that waiver closes.
            conn.insert_or_replace_consent_records(&[record(kind, "a", ConsentState::Denied, 10)])?;
            conn.insert_or_replace_consent_records(&[record(kind, "a", ConsentState::Denied, 20)])?;
            assert!(
                conn.insert_newer_consent_record(record(kind, "a", ConsentState::Allowed, 15))?
                    .state_changed
            );
            // Installation B.
            assert!(
                conn.insert_newer_consent_record(record(kind, "b", ConsentState::Allowed, 15))?
                    .state_changed
            );
            assert!(
                !conn
                    .insert_newer_consent_record(record(kind, "b", ConsentState::Denied, 10))?
                    .state_changed
            );
            for entity in ["a", "b"] {
                let stored = conn.get_consent_record(entity.into(), kind)?.unwrap();
                assert_eq!(
                    (stored.state, stored.consented_at_ns),
                    (ConsentState::Allowed, 15),
                    "{entity}"
                );
            }
            Ok::<(), crate::ConnectionError>(())
        })?;
    }

    // verifies: CONS-010
    #[xmtp_common::test(unwrap_try = true)]
    fn equal_time_uses_state_order_in_both_arrival_orders() {
        with_connection(|conn| {
            for entity_type in [ConsentType::ConversationId, ConsentType::InboxId] {
                let prefix = match entity_type {
                    ConsentType::ConversationId => "conversation",
                    ConsentType::InboxId => "inbox",
                };
                let upgrade_entity = format!("{prefix}-upgrade");
                let allowed = record(entity_type, &upgrade_entity, ConsentState::Allowed, 50);
                let denied = record(entity_type, &upgrade_entity, ConsentState::Denied, 50);
                let unknown = record(entity_type, &upgrade_entity, ConsentState::Unknown, 50);
                assert!(
                    conn.insert_newer_consent_record(unknown.clone())?
                        .state_changed
                );
                assert!(
                    conn.insert_newer_consent_record(allowed.clone())?
                        .state_changed
                );
                assert!(
                    conn.insert_newer_consent_record(denied.clone())?
                        .state_changed
                );
                assert!(!conn.insert_newer_consent_record(allowed)?.state_changed);
                assert!(!conn.insert_newer_consent_record(unknown)?.state_changed);
                let stored = conn
                    .get_consent_record(upgrade_entity, entity_type)?
                    .unwrap();
                assert_eq!(stored.state, ConsentState::Denied);
                assert_eq!(stored.consented_at_ns, 50);

                let downgrade_entity = format!("{prefix}-downgrade");
                let allowed = record(entity_type, &downgrade_entity, ConsentState::Allowed, 60);
                let denied = record(entity_type, &downgrade_entity, ConsentState::Denied, 60);
                assert!(
                    conn.insert_newer_consent_record(denied.clone())?
                        .state_changed
                );
                assert!(!conn.insert_newer_consent_record(allowed)?.state_changed);
                assert!(!conn.insert_newer_consent_record(denied)?.state_changed);
                let stored = conn
                    .get_consent_record(downgrade_entity, entity_type)?
                    .unwrap();
                assert_eq!(stored.state, ConsentState::Denied);
                assert_eq!(stored.consented_at_ns, 60);
            }
            Ok::<(), crate::ConnectionError>(())
        })?;
    }

    // verifies: CONS-010
    #[xmtp_common::test(unwrap_try = true)]
    async fn received_consent_order_survives_reopen_and_repeat() {
        let path = xmtp_common::tmp_path();
        let states = [
            ConsentState::Unknown,
            ConsentState::Allowed,
            ConsentState::Denied,
        ];
        let mut expected = Vec::new();
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            let conn = store.db();
            for entity_type in [ConsentType::InboxId, ConsentType::ConversationId] {
                for (stored_index, stored_state) in states.iter().copied().enumerate() {
                    for (received_index, received_state) in states.iter().copied().enumerate() {
                        for received_time in [10, 20, 30] {
                            let entity = format!(
                                "{entity_type:?}-{stored_index}-{received_index}-{received_time}"
                            );
                            let first = record(entity_type, &entity, stored_state, 20);
                            assert!(conn.insert_newer_consent_record(first.clone())?.applied);
                            let received =
                                record(entity_type, &entity, received_state, received_time);
                            let wins = received_time > 20
                                || (received_time == 20 && received_index > stored_index);
                            let merged = conn.insert_newer_consent_record(received.clone())?;
                            assert_eq!(merged.applied, wins);
                            assert_eq!(
                                merged.state_changed,
                                wins && stored_state != received_state
                            );
                            expected.push(if wins { received } else { first });
                        }
                    }
                }
            }
        }
        let store = crate::TestDb::create_persistent_store(Some(path)).await;
        let conn = store.db();
        for winner in expected {
            let stored = conn
                .get_consent_record(winner.entity.clone(), winner.entity_type)?
                .unwrap();
            assert_eq!(stored.state, winner.state);
            assert_eq!(stored.consented_at_ns, winner.consented_at_ns);
            assert!(!conn.insert_newer_consent_record(winner.clone())?.applied);
            let older = record(
                winner.entity_type,
                &winner.entity,
                ConsentState::Denied,
                winner.consented_at_ns - 1,
            );
            assert!(!conn.insert_newer_consent_record(older)?.applied);
            let stored = conn
                .get_consent_record(winner.entity.clone(), winner.entity_type)?
                .unwrap();
            assert_eq!(stored.state, winner.state);
            assert_eq!(stored.consented_at_ns, winner.consented_at_ns);
        }
    }

    fn generate_consent_record(
        entity_type: ConsentType,
        state: ConsentState,
        entity: String,
    ) -> StoredConsentRecord {
        StoredConsentRecord {
            entity_type,
            state,
            entity,
            consented_at_ns: now_ns(),
        }
    }

    /// A new DM copies the greatest record of its pair, newest time first and
    /// then greatest state, with that record's own time. The tie at `t`
    /// resolves by state, whichever group id sorts first and whichever row
    /// came first. The second pair's records are older, so a lookup that
    /// returns the first pair's records fails.
    // verifies: DMS-010
    #[xmtp_common::test(unwrap_try = true)]
    fn stitch_dm_consent_copies_greatest_record_and_time() {
        with_connection(|conn| {
            for (dm_id, denied_first, t) in [
                ("dm:alpha:beta", true, 1_000),
                ("dm:alpha:gamma", false, 990),
            ] {
                let mut groups: Vec<_> = (0..4)
                    .map(|_| {
                        let mut group = generate_group(None);
                        group.dm_id = Some(dm_id.into());
                        group
                    })
                    .collect();
                // The tied pair holds the two lowest ids; which one is denied
                // and which one is stored first swaps between the cases.
                groups.sort_by_key(|group| group.id);
                let [low, high, older, new_dm] = groups.try_into().unwrap();
                let (denied, allowed) = if denied_first {
                    (low, high)
                } else {
                    (high, low)
                };
                let rows = [
                    (&older, ConsentState::Allowed, t - 1),
                    (&denied, ConsentState::Denied, t),
                    (&allowed, ConsentState::Allowed, t),
                ];
                let ordered: Vec<_> = if denied_first {
                    rows.into_iter().collect()
                } else {
                    rows.into_iter().rev().collect()
                };
                for (group, state, at) in ordered {
                    group.store(conn)?;
                    conn.insert_newer_consent_record(record(
                        ConsentType::ConversationId,
                        &hex::encode(group.id),
                        state,
                        at,
                    ))?;
                }
                new_dm.store(conn)?;

                let copied = StoredConsentRecord::stitch_dm_consent(conn, &new_dm)?;
                let copied = copied.map(|r| (r.state, r.consented_at_ns));
                assert_eq!(copied, Some((ConsentState::Denied, t)), "{dm_id}");
                assert!(StoredConsentRecord::stitch_dm_consent(conn, &new_dm)?.is_none());
                let inherited = conn
                    .get_consent_record(hex::encode(new_dm.id), ConsentType::ConversationId)?
                    .unwrap();
                assert_eq!(
                    (inherited.state, inherited.consented_at_ns),
                    (ConsentState::Denied, t),
                    "{dm_id}"
                );
            }
            Ok::<(), StorageError>(())
        })?;
    }

    #[xmtp_common::test]
    fn insert_and_read() {
        with_connection(|conn| {
            let inbox_id = "inbox_1";
            let consent_record = generate_consent_record(
                ConsentType::InboxId,
                ConsentState::Allowed,
                inbox_id.to_string(),
            );
            let consent_record_entity = consent_record.entity.clone();

            // Insert the record
            let result = conn
                .insert_or_replace_consent_records(std::slice::from_ref(&consent_record))
                .expect("should store without error");
            // One record was inserted
            assert_eq!(result.len(), 1);

            // Insert it again
            let result = conn
                .insert_or_replace_consent_records(std::slice::from_ref(&consent_record))
                .expect("should store without error");
            // Nothing should change
            assert_eq!(result.len(), 0);

            // Insert it again, this time with a Denied state
            let result = conn
                .insert_or_replace_consent_records(&[StoredConsentRecord {
                    state: ConsentState::Denied,
                    ..consent_record
                }])
                .expect("should store without error");
            // Should change
            assert_eq!(result.len(), 1);

            let consent_record = conn
                .get_consent_record(inbox_id.to_owned(), ConsentType::InboxId)
                .expect("query should work");

            assert_eq!(consent_record.unwrap().entity, consent_record_entity);

            let conflict = generate_consent_record(
                ConsentType::InboxId,
                ConsentState::Allowed,
                inbox_id.to_string(),
            );

            let existing = conn
                .maybe_insert_consent_record_return_existing(&conflict)
                .unwrap();
            assert!(existing.is_some());
            let existing = existing.unwrap();
            // we want the old record to be returned.
            assert_eq!(existing.state, ConsentState::Denied);

            let db_cr = conn
                .get_consent_record(existing.entity, existing.entity_type)
                .unwrap()
                .unwrap();
            // ensure the db matches the state of what was returned
            assert_eq!(db_cr.state, existing.state);
        })
    }
}
