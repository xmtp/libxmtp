use diesel::prelude::*;

use super::{ConnectionExt, StorageError, db_connection::DbConnection, schema::openmls_key_store};
use crate::{Delete, impl_fetch, impl_store};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = openmls_key_store)]
#[diesel(primary_key(key_bytes))]
pub struct StoredKeyStoreEntry {
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
}

impl_fetch!(StoredKeyStoreEntry, openmls_key_store, Vec<u8>);
impl_store!(StoredKeyStoreEntry, openmls_key_store);

impl<C: ConnectionExt> Delete<StoredKeyStoreEntry> for DbConnection<C> {
    type Key = Vec<u8>;
    fn delete(&self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(self.raw_query_write(|conn| {
            diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(conn)
        })?)
    }
}

pub trait QueryKeyStoreEntry {
    fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), StorageError>;
}

impl<T> QueryKeyStoreEntry for &T
where
    T: QueryKeyStoreEntry,
{
    fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), StorageError> {
        (**self).insert_or_update_key_store_entry(key, value)
    }
}

impl<C: ConnectionExt> QueryKeyStoreEntry for DbConnection<C> {
    fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), StorageError> {
        use super::schema::openmls_key_store::dsl::*;
        let entry = StoredKeyStoreEntry {
            key_bytes: key,
            value_bytes: value,
        };

        self.raw_query_write(|conn| {
            diesel::replace_into(openmls_key_store)
                .values(entry)
                .execute(conn)
        })?;
        Ok(())
    }
}
