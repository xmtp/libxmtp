use super::DbConnection;
use super::{schema::openmls_key_store, StorageError};
use crate::{Delete, Fetch, Store};
use diesel::prelude::*;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = openmls_key_store)]
#[diesel(primary_key(key_bytes))]
pub struct StoredKeyStoreEntry {
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
}

impl Store<DbConnection> for StoredKeyStoreEntry {
    fn store(&self, into: &mut DbConnection) -> Result<(), StorageError> {
        diesel::insert_into(openmls_key_store::table)
            .values(self)
            .execute(into)?;

        Ok(())
    }
}

impl Fetch<StoredKeyStoreEntry> for DbConnection {
    type Key = Vec<u8>;
    fn fetch(&mut self, key: Vec<u8>) -> Result<Option<StoredKeyStoreEntry>, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(openmls_key_store.find(key).first(self).optional()?)
    }
}

impl Delete<StoredKeyStoreEntry> for DbConnection {
    type Key = Vec<u8>;
    fn delete(&mut self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(self)?)
    }
}
