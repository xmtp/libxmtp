use crate::{impl_store, storage::StorageError, Store};

use super::{
    schema::user_preferences::{self, dsl},
    DbConnection,
};
use diesel::prelude::*;
use rand::{rngs::OsRng, RngCore};

#[derive(Identifiable, Insertable, Queryable, Debug, Clone, PartialEq, Eq)]
#[diesel(table_name = user_preferences)]
#[diesel(primary_key(id))]
pub struct StoredUserPreferences {
    /// Primary key - latest key is the "current" preference
    pub id: Option<i32>,
    /// Randomly generated hmac key root
    pub hmac_key: Vec<u8>,
}

impl Default for StoredUserPreferences {
    fn default() -> Self {
        let mut hmac_key = vec![0; 32];
        OsRng.fill_bytes(&mut hmac_key);

        Self { id: None, hmac_key }
    }
}

impl_store!(StoredUserPreferences, user_preferences);

impl StoredUserPreferences {
    pub fn load(conn: &DbConnection) -> Result<Self, StorageError> {
        let query = dsl::user_preferences.order(dsl::id.desc()).limit(1);
        let mut result = conn.raw_query(|conn| query.load::<StoredUserPreferences>(conn))?;

        let result = match result.pop() {
            Some(result) => result,
            None => {
                // Create a default and store it.
                let result = Self::default();
                // TODO: emit an hmac key update event here.
                result.store(conn)?;
                result
            }
        };

        Ok(result)
    }

    pub fn set_hmac_key(conn: &DbConnection, hmac_key: Vec<u8>) -> Result<(), StorageError> {
        let mut preferences = Self::load(conn)?;
        // Have the id to increment
        preferences.id = None;
        preferences.hmac_key = hmac_key;

        preferences.store(conn)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::{storage::encrypted_store::tests::with_connection, Store};
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_insert_and_upate_preferences() {
        with_connection(|conn| {
            // loads a default
            let pref = StoredUserPreferences::load(conn).unwrap();
            assert_eq!(pref, StoredUserPreferences::default());
            assert_eq!(pref.id, None);
            // save that default
            pref.store(conn).unwrap();

            // set an hmac key
            let hmac_key = vec![1, 2, 1, 2, 1, 2, 1, 2, 1, 2];
            StoredUserPreferences::set_hmac_key(conn, hmac_key.clone()).unwrap();

            // load preferences from db
            let pref = StoredUserPreferences::load(conn).unwrap();
            assert_eq!(pref.hmac_key, hmac_key);
            assert_eq!(pref.id, Some(2));

            // check that there are two preferences stored
            let query = dsl::user_preferences.order(dsl::id.desc());
            let result = conn
                .raw_query(|conn| query.load::<StoredUserPreferences>(conn))
                .unwrap();
            assert_eq!(result.len(), 2);
        })
        .await;
    }
}
