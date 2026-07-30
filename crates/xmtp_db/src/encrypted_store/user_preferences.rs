#[cfg(feature = "sync")]
use super::{
    ConnectionExt,
    schema::user_preferences::{self, dsl},
};
use crate::StorageError;
#[cfg(feature = "sync")]
use crate::Store;
#[cfg(feature = "sync")]
use diesel::{insert_into, prelude::*};
use xmtp_common::time::now_ns;

/// The single row of `user_preferences`, which both schemas pin to `id = 0`
/// with a CHECK constraint.
#[derive(Debug, Clone, PartialEq, Eq, Default, xmtp_macro::PgModel)]
#[xmtp(table = "user_preferences")]
#[cfg_attr(
    feature = "sync",
    derive(Identifiable, Insertable, Queryable, AsChangeset)
)]
#[cfg_attr(feature = "sync", diesel(table_name = user_preferences))]
#[cfg_attr(feature = "sync", diesel(primary_key(id)))]
pub struct StoredUserPreferences {
    pub id: i32,
    /// HMAC key root
    pub hmac_key: Option<Vec<u8>>,
    pub hmac_key_cycled_at_ns: Option<i64>,
    /// Whether DM group updates have been migrated.
    pub dm_group_updates_migrated: bool,
}

#[cfg(feature = "sync")]
impl<C> Store<C> for StoredUserPreferences
where
    C: ConnectionExt,
{
    type Output = ();
    fn store(&self, conn: &C) -> Result<Self::Output, StorageError> {
        conn.raw_query(|conn| {
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

/// The length a stored HMAC root key must have, and the width of
/// [`HmacKey::key`].
pub const HMAC_KEY_LEN: usize = 42;

#[cfg(feature = "sync")]
impl StoredUserPreferences {
    pub fn load(conn: impl ConnectionExt) -> Result<Self, StorageError> {
        let pref = conn.raw_query(|conn| dsl::user_preferences.first(conn).optional())?;
        Ok(pref.unwrap_or_default())
    }

    fn store(&self, conn: &impl ConnectionExt) -> Result<(), StorageError> {
        conn.raw_query(|conn| {
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
        conn: &impl ConnectionExt,
        key: &[u8],
        cycled_at: Option<i64>,
    ) -> Result<(), StorageError> {
        if key.len() != HMAC_KEY_LEN {
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

/// `user_preferences` was the one table of the 23 reached only through
/// `raw_query`, with no `Query*` trait of its own. It needs one to exist on the
/// async track at all, since `ConnectionExt` is sync-track-only.
///
/// The sync-track callers in `xmtp_mls` still go through the inherent
/// [`StoredUserPreferences`] functions above; moving them onto this trait (and
/// retiring those functions) is a follow-up, deliberately left out of the port
/// so the sync track stays behaviorally identical.
#[maybe_async::maybe_async(AFIT)]
pub trait QueryUserPreferences {
    /// The stored preferences, or their defaults when the row does not exist yet.
    async fn load_user_preferences(&self) -> Result<StoredUserPreferences, StorageError>;

    /// Store the HMAC root key, keeping `hmac_key_cycled_at_ns` monotonic.
    ///
    /// A `cycled_at_ns` older than the stored one is ignored -- that is a stale
    /// device-sync update arriving out of order. `None` means "now", and always
    /// wins.
    async fn store_hmac_key(
        &self,
        key: &[u8],
        cycled_at_ns: Option<i64>,
    ) -> Result<(), StorageError>;

    /// Record that the one-time DM group-updates cleanup has run.
    async fn set_dm_group_updates_migrated(&self) -> Result<(), StorageError>;
}

#[maybe_async::maybe_async(AFIT)]
impl<T> QueryUserPreferences for &T
where
    T: QueryUserPreferences,
{
    async fn load_user_preferences(&self) -> Result<StoredUserPreferences, StorageError> {
        (**self).load_user_preferences().await
    }

    async fn store_hmac_key(
        &self,
        key: &[u8],
        cycled_at_ns: Option<i64>,
    ) -> Result<(), StorageError> {
        (**self).store_hmac_key(key, cycled_at_ns).await
    }

    async fn set_dm_group_updates_migrated(&self) -> Result<(), StorageError> {
        (**self).set_dm_group_updates_migrated().await
    }
}

#[cfg(feature = "sync")]
impl<C: ConnectionExt> QueryUserPreferences for crate::DbConnection<C> {
    fn load_user_preferences(&self) -> Result<StoredUserPreferences, StorageError> {
        StoredUserPreferences::load(self)
    }

    fn store_hmac_key(&self, key: &[u8], cycled_at_ns: Option<i64>) -> Result<(), StorageError> {
        StoredUserPreferences::store_hmac_key(&self, key, cycled_at_ns)
    }

    fn set_dm_group_updates_migrated(&self) -> Result<(), StorageError> {
        self.raw_query(|conn| {
            diesel::update(dsl::user_preferences)
                .set(dsl::dm_group_updates_migrated.eq(true))
                .execute(conn)
        })?;
        Ok(())
    }
}

/// sqlx backend -- Postgres only. See the note on `QueryGroupVersion`'s impl for
/// why this is gated `not(feature = "sync")`.
#[cfg(all(feature = "async", not(feature = "sync"), not(target_arch = "wasm32")))]
mod pg_impl {
    use super::*;
    use crate::pg::{PgDb, PgModel};

    /// The row's fixed primary key. Both schemas pin it with `CHECK (id = 0)`.
    const SINGLETON_ID: i32 = 0;

    impl QueryUserPreferences for PgDb {
        async fn load_user_preferences(&self) -> Result<StoredUserPreferences, StorageError> {
            let sql = format!(
                "SELECT {} FROM user_preferences LIMIT 1",
                StoredUserPreferences::select_columns()
            );
            let mut c = self.conn().await?;
            let stored = sqlx::query_as::<_, StoredUserPreferences>(&sql)
                .fetch_optional(&mut *c)
                .await
                .map_err(crate::ConnectionError::from)?;
            Ok(stored.unwrap_or_default())
        }

        /// One statement instead of the sync path's load-compare-write: the
        /// monotonicity guard becomes the `DO UPDATE ... WHERE`, so a stale
        /// writer is rejected by the engine rather than by a read that can race.
        ///
        /// `$2` is the timestamp actually written; `$3` is the caller's
        /// argument, which is what the guard keys off. The two differ when the
        /// caller passes `None`, and that case must still overwrite -- matching
        /// the sync path, whose guard fires only when *both* sides are `Some`.
        async fn store_hmac_key(
            &self,
            key: &[u8],
            cycled_at_ns: Option<i64>,
        ) -> Result<(), StorageError> {
            if key.len() != HMAC_KEY_LEN {
                return Err(StorageError::InvalidHmacLength);
            }

            let mut c = self.conn().await?;
            sqlx::query(
                "INSERT INTO user_preferences (id, hmac_key, hmac_key_cycled_at_ns) \
                 VALUES ($1, $4, $2) \
                 ON CONFLICT (id) DO UPDATE \
                    SET hmac_key = excluded.hmac_key, \
                        hmac_key_cycled_at_ns = excluded.hmac_key_cycled_at_ns \
                  WHERE $3::bigint IS NULL \
                     OR user_preferences.hmac_key_cycled_at_ns IS NULL \
                     OR user_preferences.hmac_key_cycled_at_ns <= $3",
            )
            .bind(SINGLETON_ID)
            .bind(cycled_at_ns.unwrap_or_else(now_ns))
            .bind(cycled_at_ns)
            .bind(key)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(())
        }

        /// Deliberately an upsert, where the sync path issues a bare `UPDATE`.
        ///
        /// The preferences row is created lazily by the first `store_hmac_key`,
        /// so on a database where that has not happened yet the sync path's
        /// update matches nothing and the one-time migration silently re-runs on
        /// every start. Inserting here can only make the flag stick.
        async fn set_dm_group_updates_migrated(&self) -> Result<(), StorageError> {
            let mut c = self.conn().await?;
            sqlx::query(
                "INSERT INTO user_preferences (id, dm_group_updates_migrated) VALUES ($1, TRUE) \
                 ON CONFLICT (id) DO UPDATE SET dm_group_updates_migrated = TRUE",
            )
            .bind(SINGLETON_ID)
            .execute(&mut *c)
            .await
            .map_err(crate::ConnectionError::from)?;
            Ok(())
        }
    }
}

// Two separate outer attributes rather than `#[cfg(all(test, feature = "sync"))]`:
// clippy keys `allow-unwrap-in-tests` off a *literal* `#[cfg(test)]`, and folding
// the track condition into it makes clippy stop treating this as test code, so
// every `unwrap()` below would trip `-Dwarnings`. An inner `#![cfg(...)]` is not
// an option either -- on an inline module that is `mixed_attributes_style`.
#[cfg(test)]
#[cfg(feature = "sync")]
mod tests {
    use super::*;

    #[xmtp_common::test]
    fn test_insert_and_update_preferences() {
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
                .raw_query(|conn| query.load::<StoredUserPreferences>(conn))
                .unwrap();
            assert_eq!(result.len(), 1);
        })
    }
}
