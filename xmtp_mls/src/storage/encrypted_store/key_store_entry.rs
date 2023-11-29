use diesel::prelude::*;

use super::{schema::openmls_key_store, xmtp_db_connection::XmtpDbConnection, StorageError};
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

impl Delete<StoredKeyStoreEntry> for XmtpDbConnection<'_> {
    type Key = Vec<u8>;
    fn delete(&self, key: Vec<u8>) -> Result<usize, StorageError> where {
        use super::schema::openmls_key_store::dsl::*;
        Ok(self.raw_query(|conn| {
            diesel::delete(openmls_key_store.filter(key_bytes.eq(key))).execute(conn)
        })?)
    }
}

impl XmtpDbConnection<'_> {
    pub fn insert_or_update_key_store_entry(
        &self,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> Result<(), StorageError> {
        use super::schema::openmls_key_store::dsl::*;
        let entry = StoredKeyStoreEntry {
            key_bytes: key,
            value_bytes: value,
        };

        self.raw_query(|conn| {
            diesel::replace_into(openmls_key_store)
                .values(entry)
                .execute(conn)
        })?;
        Ok(())
    }
}
