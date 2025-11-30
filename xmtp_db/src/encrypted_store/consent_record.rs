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

    /// This function will perform some logic to see if a new group should be auto-consented
    /// or auto-denied based on past consent.
    pub fn stitch_dm_consent(conn: &impl DbQuery, group: &StoredGroup) -> Result<(), StorageError> {
        if let Some(dm_id) = &group.dm_id {
            let mut past_consent = conn.find_consent_by_dm_id(dm_id)?;
            let Some(last_consent) = past_consent.pop() else {
                return Ok(());
            };

            let cr = Self::new(
                ConsentType::ConversationId,
                last_consent.state,
                hex::encode(&group.id),
            );
            conn.insert_newer_consent_record(cr)?;
        }

        Ok(())
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

    // returns true if newer
    fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<bool, crate::ConnectionError>;

    /// Insert consent_records, and replace existing entries, returns records that are new or changed
    fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError>;

    fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, crate::ConnectionError>;

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
        self.raw_query_read(|conn| {
            dsl::consent_records
                .filter(dsl::entity.eq(entity))
                .filter(dsl::entity_type.eq(entity_type))
                .first(conn)
                .optional()
        })
    }

    fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, crate::ConnectionError> {
        self.raw_query_read(|conn| super::schema::consent_records::table.load(conn))
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

        self.raw_query_read(|conn| query.load::<StoredConsentRecord>(conn))
    }

    // returns true if newer
    fn insert_newer_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<bool, crate::ConnectionError> {
        self.raw_query_write(|conn| {
            let maybe_inserted_consent_record: Option<StoredConsentRecord> =
                diesel::insert_into(dsl::consent_records)
                    .values(&record)
                    .on_conflict_do_nothing()
                    .get_result(conn)
                    .optional()?;

            // if record was not inserted...
            if maybe_inserted_consent_record.is_none() {
                let old_record = dsl::consent_records
                    .find((&record.entity_type, &record.entity))
                    .first::<StoredConsentRecord>(conn)?;

                if old_record.eq(&record) {
                    return Ok(false);
                }

                let should_replace = old_record.consented_at_ns < record.consented_at_ns;
                if should_replace {
                    diesel::insert_into(dsl::consent_records)
                        .values(record)
                        .on_conflict((dsl::entity_type, dsl::entity))
                        .do_update()
                        .set(dsl::state.eq(excluded(dsl::state)))
                        .execute(conn)?;
                }
                return Ok(should_replace);
            }

            Ok(true)
        })
    }

    /// Insert consent_records, and replace existing entries, returns records that are new or changed
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

        let changed = self.raw_query_write(|conn| {
            let existing: Vec<StoredConsentRecord> = query.load(conn)?;
            let changed: Vec<_> = records
                .iter()
                .filter(|r| !existing.contains(r))
                .cloned()
                .collect();

            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                for record in records.iter() {
                    diesel::insert_into(dsl::consent_records)
                        .values(record)
                        .on_conflict((dsl::entity_type, dsl::entity))
                        .do_update()
                        .set(dsl::state.eq(excluded(dsl::state)))
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
        self.raw_query_write(|conn| {
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
        self.raw_query_read(|conn| {
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
                .order(dsl::consented_at_ns.desc())
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
    ) -> Result<bool, crate::ConnectionError> {
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
    use crate::{Store, group::tests::generate_group, test_utils::with_connection};
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

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

    #[xmtp_common::test(unwrap_try = true)]
    fn find_consent_by_dm_id() {
        with_connection(|conn| {
            let mut g = generate_group(None);
            g.dm_id = Some("dm:alpha:beta".to_string());
            g.store(conn)?;

            let cr = generate_consent_record(
                ConsentType::ConversationId,
                ConsentState::Allowed,
                hex::encode(g.id),
            );
            cr.store(conn)?;

            let mut records = conn.find_consent_by_dm_id("dm:alpha:beta")?;

            assert_eq!(records.len(), 1);
            assert_eq!(records.pop()?, cr);
        })
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
