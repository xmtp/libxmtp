use crate::{impl_store, storage::StorageError};

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
use xmtp_id::associations::DeserializationError;
use xmtp_proto::xmtp::mls::message_contents::{
    ConsentEntityType, ConsentState as ConsentStateProto, ConsentUpdate as ConsentUpdateProto,
};

/// StoredConsentRecord holds a serialized ConsentRecord
#[derive(Insertable, Queryable, Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[diesel(table_name = hmac_key_records)]
#[diesel(primary_key(group_id, hmac_key))]
pub struct StoredHmacKeyRecord {
    /// The group id associate with these hmac keys
    pub group_id:  Vec<u8>,
    /// The dm id for stitching
    pub dm_id: Option<String>,
    /// The hmac key
    pub hmac_key:  Vec<u8>,
    /// The number of 30 day periods since epoch
    pub thirty_day_periods_since_epoch: i32,
}

impl StoredHmacKeyRecord {
    pub fn new(group_id: Vec<u8>, dm_id: Option<String>, hmac_key: Vec<u8>, thirty_day_periods_since_epoch: i32) -> Self {
        Self {
            group_id,
            dm_id,
            hmac_key,
            thirty_day_periods_since_epoch
        }
    }
}

impl_store!(StoredHmacKeyRecord, hmac_key_records);

impl DbConnection {
    /// Returns all hmac_key_records for the given group_id
    pub fn get_hmac_key_records(
        &self,
        group_id: Vec<u8>,
    ) -> Result<Vec<StoredHmacKeyRecord>, StorageError> {
        Ok(self.raw_query(|conn| -> diesel::QueryResult<_> {
            dsl::hmac_key_records
                .filter(dsl::group_id.eq(group_id))
                .load::<StoredHmacKeyRecord>(conn)
        })?)
    }

    /// Insert hmac_key_records without replacing existing ones
    pub fn insert_hmac_key_records(
        &self,
        records: &[StoredHmacKeyRecord],
    ) -> Result<(), StorageError> {
        self.raw_query(|conn| -> diesel::QueryResult<_> {
            conn.transaction::<_, diesel::result::Error, _>(|conn| {
                for record in records.iter() {
                    diesel::insert_into(dsl::hmac_key_records)
                        .values(record)
                        .on_conflict_do_nothing() 
                        .execute(conn)?;
                }
                Ok(())
            })
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::encrypted_store::tests::with_connection;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    fn generate_hmac_key_record(
        group_id: Vec<u8>,
        hmac_key: Vec<u8>,
        thirty_day_periods_since_epoch: i32,
    ) -> StoredHmacKeyRecord {
        StoredHmacKeyRecord {
            group_id,
            None,
            hmac_key,
            thirty_day_periods_since_epoch,
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn insert_and_read_hmac_key_records() {
        with_connection(|conn| {
            // Prepare test data
            let group_id = b"group_id".to_vec();
            let hmac_key = b"hmac_key".to_vec();
            let thirty_day_periods_since_epoch = 123;

            let hmac_record = generate_hmac_key_record(
                group_id.clone(),
                hmac_key.clone(),
                thirty_day_periods_since_epoch,
            );

            // Insert the record
            conn.insert_hmac_key_records(&[hmac_record.clone()])
                .expect("should insert hmac_key_record without error");

            // Read back the inserted record
            let records = conn
                .get_hmac_key_records(group_id.clone())
                .expect("query should work");

            // Ensure the records match
            assert_eq!(records.len(), 1, "There should be exactly one record");
            let retrieved_record = &records[0];

            assert_eq!(retrieved_record.group_id, hmac_record.group_id);
            assert_eq!(retrieved_record.hmac_key, hmac_record.hmac_key);
            assert_eq!(
                retrieved_record.thirty_day_periods_since_epoch,
                hmac_record.thirty_day_periods_since_epoch
            );

            // Insert a second record (same group_id and thirty_day_periods_since_epoch)
            let conflict_record = generate_hmac_key_record(
                group_id.clone(),
                b"new_hmac_key".to_vec(), // Different hmac_key
                thirty_day_periods_since_epoch,
            );

            conn.insert_hmac_key_records(&[conflict_record])
                .expect("should insert second record without error");

            let records = conn
                .get_hmac_key_records(group_id.clone())
                .expect("query should work");

            assert_eq!(records.len(), 2, "Both records should exist");
        })
        .await;
    }
}
