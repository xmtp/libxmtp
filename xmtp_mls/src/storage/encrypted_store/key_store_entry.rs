use diesel::prelude::*;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{db_connection::DbConnection, schema::openmls_key_store, StorageError};
use crate::{impl_fetch, impl_store, Delete};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = openmls_key_store)]
#[diesel(primary_key(key_bytes))]
pub struct StoredKeyStoreEntry {
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
    pub expire_at_s: Option<i64>,
}

impl_fetch!(StoredKeyStoreEntry, openmls_key_store, Vec<u8>);
impl_store!(StoredKeyStoreEntry, openmls_key_store);

impl Delete<StoredKeyStoreEntry> for DbConnection<'_> {
    type Key = Vec<u8>;
    fn delete(&self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(self.raw_query(|conn| {
            diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(conn)
        })?)
    }
}

impl DbConnection<'_> {
    pub fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
        exp: Option<u64>,
    ) -> Result<(), StorageError> {
        use super::schema::openmls_key_store::dsl::*;
        let entry = StoredKeyStoreEntry {
            key_bytes: key,
            value_bytes: value,
            expire_at_s: if let Some(e) = exp {
                e.try_into().ok()
            } else {
                None
            },
        };

        let current_time = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        self.raw_query(|conn| {
            diesel::replace_into(openmls_key_store)
                .values(entry)
                .execute(conn)?;
            // Delete expired entries.
            diesel::delete(openmls_key_store.filter(expire_at_s.lt(current_time))).execute(conn)
        })?;

        Ok(())
    }
}
