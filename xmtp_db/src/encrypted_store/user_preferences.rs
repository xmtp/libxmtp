use super::{
    DbConnection,
    schema::user_preferences::{self, dsl},
};
use crate::{StorageError, Store};
use diesel::{insert_into, prelude::*};

#[derive(
    Identifiable, Insertable, Queryable, AsChangeset, Debug, Clone, PartialEq, Eq, Default,
)]
#[diesel(table_name = user_preferences)]
#[diesel(primary_key(id))]
pub struct StoredUserPreferences {
    pub id: i32,
    /// Randomly generated hmac key root
    pub hmac_key: Option<Vec<u8>>,

    // Sync cursor
    pub sync_cursor_group_id: Option<Vec<u8>>,
    pub sync_cursor_offset: i64,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SyncCursor {
    pub group_id: Vec<u8>,
    pub offset: i64,
}

impl SyncCursor {
    pub fn reset(group_id: &[u8], conn: &DbConnection) -> Result<(), StorageError> {
        let cursor = Self {
            group_id: group_id.to_vec(),
            offset: 0,
        };
        StoredUserPreferences::store_sync_cursor(conn, &cursor)?;
        Ok(())
    }
}

impl Store<DbConnection> for StoredUserPreferences {
    type Output = ();
    fn store(&self, conn: &DbConnection) -> Result<Self::Output, StorageError> {
        conn.raw_query_write(|conn| {
            diesel::update(dsl::user_preferences)
                .set(self)
                .execute(conn)
        })?;

        Ok(())
    }
}

#[derive(Debug)]
pub struct HmacKey {
    pub key: [u8; 42],
    // # of 30 day periods since unix epoch
    pub epoch: i64,
}

impl HmacKey {
    pub fn random_key() -> Vec<u8> {
        xmtp_common::rand_vec::<42>()
    }
}

impl StoredUserPreferences {
    pub fn load(conn: &DbConnection) -> Result<Self, StorageError> {
        let pref = conn.raw_query_read(|conn| dsl::user_preferences.first(conn).optional())?;
        Ok(pref.unwrap_or_default())
    }

    fn store(&self, conn: &DbConnection) -> Result<(), StorageError> {
        conn.raw_query_write(|conn| {
            insert_into(dsl::user_preferences)
                .values(self)
                .on_conflict(user_preferences::id)
                .do_update()
                .set(self)
                .execute(conn)
        })?;

        Ok(())
    }

    pub fn store_hmac_key(conn: &DbConnection, key: &[u8]) -> Result<(), StorageError> {
        if key.len() != 42 {
            return Err(StorageError::Generic(
                "HMAC key needs to be 42 bytes".to_string(),
            ));
        }

        let mut preferences = Self::load(conn)?;
        preferences.hmac_key = Some(key.to_vec());
        preferences.store(conn)?;

        Ok(())
    }

    // If there is no sync cursor returned, that indicates there is probably no sync group
    pub fn sync_cursor(conn: &DbConnection) -> Result<Option<SyncCursor>, StorageError> {
        let pref = Self::load(conn)?;

        let Some(group_id) = pref.sync_cursor_group_id else {
            return Ok(None);
        };

        Ok(Some(SyncCursor {
            group_id,
            offset: pref.sync_cursor_offset,
        }))
    }

    pub fn store_sync_cursor(conn: &DbConnection, cursor: &SyncCursor) -> Result<(), StorageError> {
        let mut pref = Self::load(conn)?;
        pref.sync_cursor_group_id = Some(cursor.group_id.clone());
        pref.sync_cursor_offset = cursor.offset;
        pref.store(conn)
    }
}

#[cfg(test)]
mod tests {
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);
    use super::*;

    #[xmtp_common::test]
    async fn test_insert_and_update_preferences() {
        crate::test_utils::with_connection(|conn| {
            let pref = StoredUserPreferences::load(conn).unwrap();
            // by default, there is no key
            assert!(pref.hmac_key.is_none());

            // loads and stores a default
            let pref = StoredUserPreferences::load(conn).unwrap();
            // by default, there is no key
            assert!(pref.hmac_key.is_none());

            // set an hmac key
            let hmac_key = HmacKey::random_key();
            StoredUserPreferences::store_hmac_key(conn, &hmac_key).unwrap();
            let pref = StoredUserPreferences::load(conn).unwrap();
            // Make sure it saved
            assert_eq!(hmac_key, pref.hmac_key.unwrap());

            // check that there is only one preference stored
            let query = dsl::user_preferences.order(dsl::id.desc());
            let result = conn
                .raw_query_read(|conn| query.load::<StoredUserPreferences>(conn))
                .unwrap();
            assert_eq!(result.len(), 1);
        })
        .await;
    }

    #[xmtp_common::test(unwrap_try = "true")]
    async fn test_sync_cursor() {
        crate::test_utils::with_connection(|conn| {
            // Loads fine when there's nothing in the db
            let cursor = StoredUserPreferences::sync_cursor(conn)?;
            assert!(cursor.is_none());

            let mut cursor = SyncCursor {
                group_id: vec![1, 2, 3, 4],
                offset: 10,
            };

            // Check stores on an empty row fine
            StoredUserPreferences::store_sync_cursor(conn, &cursor)?;
            let db_cursor = StoredUserPreferences::sync_cursor(conn)??;
            assert_eq!(cursor, db_cursor);

            cursor.group_id = vec![1, 2, 3, 5];
            cursor.offset = 12;

            // Check stores on an occupied row fine
            StoredUserPreferences::store_sync_cursor(conn, &cursor)?;
            let db_cursor = StoredUserPreferences::sync_cursor(conn)??;
            assert_eq!(cursor, db_cursor);
        })
        .await;
    }
}
