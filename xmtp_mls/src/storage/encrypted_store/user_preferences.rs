use crate::{
    groups::device_sync::preference_sync::UserPreferenceUpdate, impl_store, storage::StorageError,
    subscriptions::LocalEvents, Store,
};

use super::{
    schema::user_preferences::{self, dsl},
    DbConnection,
};
use diesel::prelude::*;
use rand::{rngs::OsRng, RngCore};
use tokio::sync::broadcast::Sender;

#[derive(Identifiable, Insertable, Queryable, Debug, Clone, PartialEq, Eq, Default)]
#[diesel(table_name = user_preferences)]
#[diesel(primary_key(id))]
pub struct StoredUserPreferences {
    /// Primary key - latest key is the "current" preference
    pub id: Option<i32>,
    /// Randomly generated hmac key root
    pub hmac_key: Option<Vec<u8>>,
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
                result.store(conn)?;
                result
            }
        };

        Ok(result)
    }

    pub fn new_hmac_key<C>(
        conn: &DbConnection,
        local_events: &Sender<LocalEvents<C>>,
    ) -> Result<Vec<u8>, StorageError> {
        let mut preferences = Self::load(conn)?;
        // Have the id increment
        preferences.id = None;

        let mut hmac_key = vec![0; 32];
        OsRng.fill_bytes(&mut hmac_key);
        preferences.hmac_key = Some(hmac_key.clone());

        // Sync the new key to other devices
        let _ = local_events.send(LocalEvents::OutgoingPreferenceUpdates(vec![
            UserPreferenceUpdate::HmacKeyUpdate {
                key: hmac_key.clone(),
            },
        ]));

        preferences.store(conn)?;

        Ok(hmac_key)
    }
}

#[cfg(test)]
mod tests {
    use xmtp_cryptography::utils::generate_local_wallet;

    use crate::builder::ClientBuilder;
    #[cfg(target_arch = "wasm32")]
    wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_dedicated_worker);

    use super::*;

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), tokio::test)]
    async fn test_insert_and_update_preferences() {
        let wallet = generate_local_wallet();
        let client = ClientBuilder::new_test_client(&wallet).await;

        let conn = client.store().conn().unwrap();

        // loads and stores a default
        let pref = StoredUserPreferences::load(&conn).unwrap();
        // by default, there is no key
        assert!(pref.hmac_key.is_none());

        // set an hmac key
        let hmac_key = StoredUserPreferences::new_hmac_key(&conn, &client.local_events).unwrap();
        let pref = StoredUserPreferences::load(&conn).unwrap();
        // Make sure it saved
        assert_eq!(hmac_key, pref.hmac_key.unwrap());

        // check that there are two preferences stored
        let query = dsl::user_preferences.order(dsl::id.desc());
        let result = conn
            .raw_query(|conn| query.load::<StoredUserPreferences>(conn))
            .unwrap();
        assert_eq!(result.len(), 2);
    }
}
