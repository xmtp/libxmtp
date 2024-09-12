use crate::{impl_store, storage::StorageError};

use super::{
    db_connection::DbConnection,
    schema::consent_records::{self, dsl},
};
use diesel::{
    backend::Backend,
    deserialize::{self, FromSql},
    prelude::*,
    serialize::{self, IsNull, Output, ToSql},
    sql_types::Integer,
    sqlite::Sqlite,
    upsert::excluded,
};
use serde::{Deserialize, Serialize};

/// StoredConsentRecord holds a serialized ConsentRecord
#[derive(Insertable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = consent_records)]
#[diesel(primary_key(entity_type, entity))]
pub struct StoredConsentRecord {
    /// Enum, [`ConsentType`] representing the type of consent (group_id inbox_id, etc..)
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
        Ok(self.raw_query(|conn| {
            dsl::consent_records
                .filter(dsl::entity.eq(entity))
                .filter(dsl::entity_type.eq(entity_type))
                .first(conn)
                .optional()
        })?)
    }

    /// Insert consent_record, and replace existing entries
    pub fn insert_or_replace_consent_record(
        &self,
        record: StoredConsentRecord,
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::insert_into(dsl::consent_records)
                .values(&record)
                .on_conflict((dsl::entity_type, dsl::entity))
                .do_update()
                .set(dsl::state.eq(excluded(dsl::state)))
                .execute(conn)?;
            Ok(())
        })?;

        Ok(())
    }
}

#[repr(i32)]
#[derive(Debug, Copy, Clone, Serialize, Deserialize, Eq, PartialEq, AsExpression, FromSqlRow)]
#[diesel(sql_type = Integer)]
/// Type of consent record stored
pub enum ConsentType {
    /// Consent is for a group
    GroupId = 1,
    /// Consent is for an inbox
    InboxId = 2,
    /// Consent is for an address
    Address = 3,
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
            1 => Ok(ConsentType::GroupId),
            2 => Ok(ConsentType::InboxId),
            3 => Ok(ConsentType::Address),
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
            1 => Ok(ConsentState::Allowed),
            2 => Ok(ConsentState::Denied),
            x => Err(format!("Unrecognized variant {}", x).into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::encrypted_store::tests::with_connection;

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

    #[test]
    fn insert_and_read() {
        with_connection(|conn| {
            let inbox_id = "inbox_1";
            let consent_record = generate_consent_record(
                ConsentType::InboxId,
                ConsentState::Denied,
                inbox_id.to_string(),
            );
            let consent_record_entity = consent_record.entity.clone();

            conn.insert_or_replace_consent_record(consent_record)
                .expect("should store without error");

            let consent_record = conn
                .get_consent_record(inbox_id.to_owned(), ConsentType::InboxId)
                .expect("query should work");

            assert_eq!(consent_record.unwrap().entity, consent_record_entity);
        });
    }
}
