use super::{db_connection::DbConnection, schema::key_value_store};
use crate::storage::StorageError;
use diesel::prelude::*;

#[derive(Insertable, Queryable, Debug, Clone)]
#[diesel(table_name = key_value_store)]
#[diesel(primary_key(key))]
pub(crate) struct KeyValueStore {
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
                return Err(StorageError::Deserialization(err.to_string()));
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
                .map_err(|err| StorageError::Serialization(err.to_string()))?,
        };

        conn.raw_query(|conn| {
            diesel::replace_into(key_value_store::table)
                .values(entry)
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn delete(conn: &DbConnection, key: &StoreKey) -> Result<(), StorageError> {
        let key = format!("{key:?}");
        conn.raw_query(|conn| {
            diesel::delete(key_value_store::table.filter(key_value_store::key.eq(key)))
                .execute(conn)
        })?;

        Ok(())
    }
}
