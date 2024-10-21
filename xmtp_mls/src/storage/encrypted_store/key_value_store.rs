use diesel::{prelude::*, sql_types::Binary};
use tracing::warn;

use crate::storage::StorageError;

use super::{db_connection::DbConnection, schema::key_value_store};

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = key_value_store)]
#[diesel(primary_key(key))]
pub struct KeyValueStore {
    pub key: String,
    pub value: Vec<u8>,
}

#[derive(Debug, Clone)]
pub enum StoreKey {
    MessageHistorySyncRequestId,
    ConsentSyncRequestId,
}

impl KeyValueStore {
    pub fn get<T>(conn: &DbConnection, key: &StoreKey) -> Result<Option<T>, StorageError>
    where
        T: serde::de::DeserializeOwned,
    {
        let key = format!("{key:?}");
        let store: KeyValueStore =
            conn.raw_query(|conn| key_value_store::table.find(&key).first(conn))?;

        let value = match bincode::deserialize(&store.value) {
            Ok(value) => value,
            Err(err) => {
                tracing::error!("Unable to deserialize keystore: {key}");
                return Err(StorageError::Deserialization(format!("{err:?}")));
            }
        };

        Ok(value)
    }
}
