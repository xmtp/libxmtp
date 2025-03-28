use std::fmt::Display;

use super::{
    schema::user_preferences::{self, dsl},
    DbConnection,
};
use crate::{
    groups::device_sync::preference_sync::UserPreferenceUpdate, storage::StorageError,
    subscriptions::LocalEvents, Store,
};
use diesel::{insert_into, prelude::*};
use tokio::sync::broadcast::Sender;

#[derive(
    Identifiable, Insertable, Queryable, AsChangeset, Debug, Clone, PartialEq, Eq, Default,
)]
#[diesel(table_name = user_preferences)]
#[diesel(primary_key(id))]
pub struct StoredUserPreferences {
    pub id: i32,
    /// Randomly generated hmac key root
    pub hmac_key: Option<Vec<u8>>,
    // Sync cursor: sync_group_id:last_message_ns
    pub sync_cursor: Option<String>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct SyncCursor {
    group_id: Vec<u8>,
    last_message_ns: u64,
}

impl SyncCursor {
    fn load(cursor: &str) -> Option<Self> {
        let mut split = cursor.split(":");
        let group_id = split.next()?;
        let last_message_ns = split.next()?.parse().ok()?;

        Some(Self {
            group_id: hex::decode(group_id).ok()?,
            last_message_ns,
        })
    }
}

impl Display for SyncCursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
            hex::encode(&self.group_id),
            self.last_message_ns
        )
    }
}

impl Store<DbConnection> for StoredUserPreferences {
    fn store(&self, conn: &DbConnection) -> Result<(), StorageError> {
        conn.raw_query_write(|conn| {
            diesel::update(dsl::user_preferences)
                .set(self)
                .execute(conn)
        })?;

        Ok(())
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

    pub fn store_new_hmac_key(
        conn: &DbConnection,
        local_events: &Sender<LocalEvents>,
    ) -> Result<Vec<u8>, StorageError> {
        let hmac_key = xmtp_common::rand_vec::<32>();

        let mut preferences = Self::load(conn)?;
        preferences.hmac_key = Some(hmac_key.clone());
        preferences.store(conn)?;

        // Sync the new key to other devices
        let _ = local_events.send(LocalEvents::OutgoingPreferenceUpdates(vec![
            UserPreferenceUpdate::HmacKeyUpdate {
                key: hmac_key.clone(),
            },
        ]));

        Ok(hmac_key)
    }

    /// Returns nothing if the cursor's group_id does not match the provided group_id
    pub fn sync_cursor(conn: &DbConnection, group_id: &[u8]) -> Result<SyncCursor, StorageError> {
        let default = || SyncCursor {
            group_id: group_id.to_vec(),
            last_message_ns: 0,
        };

        let Some(sync_cursor) = Self::load(conn)?.sync_cursor else {
            return Ok(default());
        };

        let cursor = SyncCursor::load(&sync_cursor);
        let cursor = cursor
            .and_then(|c| (c.group_id == group_id).then(|| c))
            .unwrap_or_else(default);
        Ok(cursor)
    }

    pub fn store_sync_cursor(conn: &DbConnection, cursor: &SyncCursor) -> Result<(), StorageError> {
        let mut pref = Self::load(conn)?;
        pref.sync_cursor = Some(format!("{cursor}"));
        Ok(pref.store(conn)?)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::{builder::ClientBuilder, storage::tests::with_connection};
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[xmtp_common::test]
    async fn test_insert_and_update_preferences() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let conn = client.store().conn().unwrap();

        // loads and stores a default
        let pref = StoredUserPreferences::load(&conn).unwrap();
        // by default, there is no key
        assert!(pref.hmac_key.is_none());

        // set an hmac key
        let hmac_key =
            StoredUserPreferences::store_new_hmac_key(&conn, &client.local_events).unwrap();
        let pref = StoredUserPreferences::load(&conn).unwrap();
        // Make sure it saved
        assert_eq!(hmac_key, pref.hmac_key.unwrap());

        // check that there are two preferences stored
        let query = dsl::user_preferences.order(dsl::id.desc());
        let result = conn
            .raw_query_read(|conn| query.load::<StoredUserPreferences>(conn))
            .unwrap();
        assert_eq!(result.len(), 1);
    }

    #[xmtp_common::test]
    async fn test_sync_cursor() {
        with_connection(|conn| {
            // Loads fine when there's nothing in the db
            let cursor = StoredUserPreferences::sync_cursor(conn, &[1, 2, 3, 4]).unwrap();
            assert_eq!(cursor.group_id, &[1, 2, 3, 4]);
            assert_eq!(cursor.last_message_ns, 0);

            let mut cursor = SyncCursor {
                group_id: vec![1, 2, 3, 4],
                last_message_ns: 1234,
            };

            // Check stores on an empty row fine
            StoredUserPreferences::store_sync_cursor(conn, &cursor).unwrap();
            let db_cursor = StoredUserPreferences::sync_cursor(conn, &cursor.group_id).unwrap();
            assert_eq!(cursor, db_cursor);

            cursor.group_id = vec![1, 2, 3, 5];
            cursor.last_message_ns = 1235;

            // Check stores on an occupied row fine
            StoredUserPreferences::store_sync_cursor(conn, &cursor).unwrap();
            let db_cursor = StoredUserPreferences::sync_cursor(conn, &cursor.group_id).unwrap();
            assert_eq!(cursor, db_cursor);
        })
        .await;
    }
}
