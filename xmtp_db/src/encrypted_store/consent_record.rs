use crate::{StorageError, impl_store};

use super::Sqlite;
use super::{
    db_connection::DbConnection,
    schema::consent_records::{self, dsl},
};
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
use xmtp_proto::{
    ConversionError,
    xmtp::device_sync::consent_backup::{ConsentSave, ConsentStateSave, ConsentTypeSave},
};
mod convert;

/// StoredConsentRecord holds a serialized ConsentRecord
#[derive(Insertable, Queryable, Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[diesel(table_name = consent_records)]
#[diesel(primary_key(entity_type, entity))]
pub struct StoredConsentRecord {
    /// Enum, [`ConsentType`] representing the type of consent (conversation_id inbox_id, etc..)
    pub entity_type: ConsentType,
    /// Enum, [`ConsentState`] representing the state of consent (allowed, denied, etc..)
    pub state: ConsentState,
    /// The entity of what was consented (0x00 etc..)
    pub entity: String,
}

impl StoredConsentRecord {
    pub fn new(entity_type: ConsentType, state: ConsentState, entity: String) -> Self {
        Self {
            entity_type,
            state,
            entity,
        }
    }
}

impl_store!(StoredConsentRecord, consent_records);

impl DbConnection {
    /// Returns the consent_records for the given entity up
    pub fn get_consent_record(
        &self,
        entity: String,
        entity_type: ConsentType,
    ) -> Result<Option<StoredConsentRecord>, StorageError> {
        Ok(self.raw_query_read(|conn| -> diesel::QueryResult<_> {
            dsl::consent_records
                .filter(dsl::entity.eq(entity))
                .filter(dsl::entity_type.eq(entity_type))
                .first(conn)
                .optional()
        })?)
    }

    pub fn consent_records(&self) -> Result<Vec<StoredConsentRecord>, StorageError> {
        Ok(self.raw_query_read(|conn| super::schema::consent_records::table.load(conn))?)
    }

    pub fn consent_records_paged(
        &self,
        limit: i64,
        offset: i64,
    ) -> Result<Vec<StoredConsentRecord>, StorageError> {
        let query = consent_records::table
            .order_by((consent_records::entity_type, consent_records::entity))
            .limit(limit)
            .offset(offset);

        Ok(self.raw_query_read(|conn| query.load::<StoredConsentRecord>(conn))?)
    }

    /// Insert consent_records, and replace existing entries, returns records that are new or changed
    pub fn insert_or_replace_consent_records(
        &self,
        records: &[StoredConsentRecord],
    ) -> Result<Vec<StoredConsentRecord>, StorageError> {
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

        let changed = self.raw_query_write(|conn| -> diesel::QueryResult<_> {
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

    pub fn maybe_insert_consent_record_return_existing(
        &self,
        record: &StoredConsentRecord,
    ) -> Result<Option<StoredConsentRecord>, StorageError> {
        self.raw_query_write(|conn| {
            let maybe_inserted_consent_record: Option<StoredConsentRecord> =
                diesel::insert_into(dsl::consent_records)
                    .values(record)
                    .on_conflict_do_nothing()
                    .get_result(conn)
                    .optional()?;

            // if record was not inserted...
            if maybe_inserted_consent_record.is_none() {
                return Ok(dsl::consent_records
                    .find((&record.entity_type, &record.entity))
                    .first(conn)
                    .optional()?);
            }

            Ok(None)
        })
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
    use crate::test_utils::with_connection;
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
        }
    }

    #[xmtp_common::test]
    async fn insert_and_read() {
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
                .insert_or_replace_consent_records(&[consent_record.clone()])
                .expect("should store without error");
            // One record was inserted
            assert_eq!(result.len(), 1);

            // Insert it again
            let result = conn
                .insert_or_replace_consent_records(&[consent_record.clone()])
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
        .await
    }
}
