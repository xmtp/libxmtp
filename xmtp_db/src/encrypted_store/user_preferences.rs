use super::{
    ConnectionExt,
    schema::user_preferences::{self, dsl},
};
use crate::{StorageError, Store};
use diesel::{insert_into, prelude::*};
use xmtp_common::time::now_ns;

#[derive(
    Identifiable, Insertable, Queryable, AsChangeset, Debug, Clone, PartialEq, Eq, Default,
)]
#[diesel(table_name = user_preferences)]
#[diesel(primary_key(id))]
pub struct StoredUserPreferences {
    pub id: i32,
    /// HMAC key root
    pub hmac_key: Option<Vec<u8>>,
    pub hmac_key_cycled_at_ns: Option<i64>,
}

impl<C> Store<C> for StoredUserPreferences
where
    C: ConnectionExt,
{
    type Output = ();
    fn store(&self, conn: &C) -> Result<Self::Output, StorageError> {
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
    // TODO: Use xmtp_cryptography::Secret for Zeroize support
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
    pub fn load(conn: impl ConnectionExt) -> Result<Self, StorageError> {
        let pref = conn.raw_query_read(|conn| dsl::user_preferences.first(conn).optional())?;
        Ok(pref.unwrap_or_default())
    }

    fn store(&self, conn: &impl crate::DbQuery) -> Result<(), StorageError> {
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

    pub fn store_hmac_key(
        conn: &impl crate::DbQuery,
        key: &[u8],
        cycled_at: Option<i64>,
    ) -> Result<(), StorageError> {
        if key.len() != 42 {
            return Err(StorageError::InvalidHmacLength);
        }

        let mut preferences = Self::load(conn)?;

        if let (Some(old), Some(new)) = (preferences.hmac_key_cycled_at_ns, cycled_at)
            && old > new
        {
            return Ok(());
        }

        preferences.hmac_key = Some(key.to_vec());
        preferences.hmac_key_cycled_at_ns = Some(cycled_at.unwrap_or_else(now_ns));
        preferences.store(conn)?;

        Ok(())
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
            StoredUserPreferences::store_hmac_key(conn, &hmac_key, None).unwrap();
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
}
