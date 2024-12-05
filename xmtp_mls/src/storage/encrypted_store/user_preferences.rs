use crate::storage::StorageError;

use super::{
    schema::user_preferences::{self, dsl},
    DbConnection,
};
use diesel::prelude::*;

#[derive(Identifiable, Insertable, Queryable, Debug, Clone, PartialEq, Eq, Default)]
#[diesel(table_name = user_preferences)]
#[diesel(primary_key(id))]
pub struct StoredUserPreferences {
    /// Primary key - latest key is the "current" preference
    pub id: Option<i32>,
    /// Randomly generated hmac key root
    pub hmac_key: Option<Vec<u8>>,
}

impl StoredUserPreferences {
    pub fn load(conn: &DbConnection) -> Result<Self, StorageError> {
        let query = dsl::user_preferences.order(dsl::id.desc()).limit(1);
        let mut result = conn.raw_query(|conn| query.load::<StoredUserPreferences>(conn))?;

        Ok(result.pop().unwrap_or_default())
    }

    pub fn set_hmac_key(conn: &DbConnection, hmac_key: Vec<u8>) -> Result<(), StorageError> {
        let mut preferences = Self::load(conn)?;
        preferences.id = None;
        preferences.hmac_key = Some(hmac_key);

        preferences.insert(conn)?;

        Ok(())
    }

    fn insert(&self, conn: &DbConnection) -> Result<(), StorageError> {
        conn.raw_query(|conn| {
            diesel::insert_into(dsl::user_preferences)
                .values(self)
                .execute(conn)?;

            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::encrypted_store::tests::with_connection;
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
            pref.insert(conn).unwrap();

            // set an hmac key
            let hmac_key = vec![1, 2, 1, 2, 1, 2, 1, 2, 1, 2];
            StoredUserPreferences::set_hmac_key(conn, hmac_key.clone()).unwrap();

            // load preferences from db
            let pref = StoredUserPreferences::load(conn).unwrap();
            assert_eq!(pref.hmac_key, Some(hmac_key));
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
