use super::{db_connection::DbConnection, schema::key_value_store};
use crate::storage::StorageError;
use diesel::prelude::*;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = key_value_store)]
#[diesel(primary_key(key))]
pub struct KeyValueStore {
    key: String,
    value: Vec<u8>,
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

    pub fn set<T>(conn: &DbConnection, key: &StoreKey, value: T) -> Result<(), StorageError>
    where
        T: serde::ser::Serialize,
    {
        let entry = KeyValueStore {
            key: format!("{key:?}"),
            value: bincode::serialize(&value)
                .map_err(|err| StorageError::Serialization(format!("{err:?}")))?,
        };

        conn.raw_query(|conn| {
            diesel::replace_into(key_value_store::table)
                .values(entry)
                .execute(conn)
        })?;

        Ok(())
    }
}
