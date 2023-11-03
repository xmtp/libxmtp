use diesel::prelude::*;

use super::{schema::openmls_key_store, DbConnection, StorageError};
use crate::{impl_fetch, impl_store, Delete};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = openmls_key_store)]
#[diesel(primary_key(key_bytes))]
pub struct StoredKeyStoreEntry {
    pub key_bytes: Vec<u8>,
    pub value_bytes: Vec<u8>,
}

impl_fetch!(StoredKeyStoreEntry, openmls_key_store, Vec<u8>);
impl_store!(StoredKeyStoreEntry, openmls_key_store);

impl Delete<StoredKeyStoreEntry> for DbConnection {
    type Key = Vec<u8>;
    fn delete(&mut self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(self)?)
    }
}
