use super::DbConnection;
use super::{schema::openmls_key_store, StorageError};
use crate::{Delete, Fetch, Store, impl_fetch_and_store};
use diesel::prelude::*;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = openmls_key_store)]
#[diesel(primary_key(key_bytes))]
pub struct StoredKeyStoreEntry {
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
}

impl_fetch_and_store!(StoredKeyStoreEntry, openmls_key_store, Vec<u8>);

impl Delete<StoredKeyStoreEntry> for DbConnection {
    type Key = Vec<u8>;
    fn delete(&mut self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(self)?)
    }
}
