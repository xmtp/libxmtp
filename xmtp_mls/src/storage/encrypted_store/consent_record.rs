use crate::{impl_store, storage::StorageError};

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
    sqlite::Sqlite,
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
    pub fn new(
        entity_type: ConsentType,
        state: ConsentState,
        entity: String,
    ) -> Self {
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

    /// Batch insert consent_records, ignoring duplicates.
    pub fn insert_or_ignore_consent_records(
        &self,
        updates: &[StoredConsentRecord],
    ) -> Result<(), StorageError> {
        Ok(self.raw_query(|conn| {
            diesel::insert_or_ignore_into(dsl::consent_records)
                .values(updates)
                .execute(conn)?;

            Ok(())
        })?)
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
    /// Consent is allowed
    Allowed = 1,
    /// Consent is denied
    Denied = 2
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
    use crate::{
        storage::encrypted_store::tests::with_connection,
        utils::test::{rand_time, rand_vec},
        Store,
    };

    use super::*;

    // fn build_update(inbox_id: &str, sequence_id: i64) -> StoredConsentRecord {
    //     StoredConsentRecord::new(inbox_id.to_string(), sequence_id, rand_time(), rand_vec())
    // }

    // #[test]
    // fn insert_and_read() {
    //     with_connection(|conn| {
    //         let inbox_id = "inbox_1";
    //         let update_1 = build_update(inbox_id, 1);
    //         let update_1_payload = update_1.payload.clone();
    //         let update_2 = build_update(inbox_id, 2);
    //         let update_2_payload = update_2.payload.clone();

    //         update_1.store(&conn).expect("should store without error");
    //         update_2.store(&conn).expect("should store without error");

    //         let all_updates = conn
    //             .get_identity_updates(inbox_id, None)
    //             .expect("query should work");

    //         assert_eq!(all_updates.len(), 2);
    //         let first_update = all_updates.first().unwrap();
    //         assert_eq!(first_update.payload, update_1_payload);
    //         let second_update = all_updates.last().unwrap();
    //         assert_eq!(second_update.payload, update_2_payload);
    //     });
    // }

    // #[test]
    // fn test_filter() {
    //     with_connection(|conn| {
    //         let inbox_id = "inbox_1";
    //         let update_1 = build_update(inbox_id, 1);
    //         let update_2 = build_update(inbox_id, 2);
    //         let update_3 = build_update(inbox_id, 3);

    //         conn.insert_or_ignore_identity_updates(&[update_1, update_2, update_3])
    //             .expect("insert should succeed");

    //         let update_1_and_2 = conn
    //             .get_identity_updates(inbox_id, Some(2))
    //             .expect("query should work");

    //         assert_eq!(update_1_and_2.len(), 2);

    //         let all_updates = conn
    //             .get_identity_updates(inbox_id, None)
    //             .expect("query should work");

    //         assert_eq!(all_updates.len(), 3);
    //     })
    // }
}
