use super::{
    ConnectionExt,
    schema::user_preferences::{self, dsl},
};
use crate::{StorageError, Store, StoredSecret};
use diesel::{insert_into, prelude::*, upsert::excluded};
use std::fmt;
use xmtp_common::time::now_ns;
use xmtp_cryptography::secret::{Secret, Zeroize, ZeroizeOnDrop};

#[derive(
    Identifiable,
    Insertable,
    Queryable,
    Selectable,
    AsChangeset,
    Debug,
    Clone,
    PartialEq,
    Eq,
    Default,
)]
#[diesel(table_name = user_preferences)]
#[diesel(primary_key(id))]
pub struct StoredUserPreferences {
    pub id: i32,
    /// HMAC key root
    pub hmac_key: Option<StoredSecret>,
    pub hmac_key_cycled_at_ns: Option<i64>,
    /// Whether DM group updates have been migrated.
    pub dm_group_updates_migrated: bool,
}

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

/// A group's HMAC key for one epoch. The key is wiped on drop, and `Debug`
/// shows only the epoch.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct HmacKey {
    pub key: [u8; 42],
    // # of 30 day periods since unix epoch
    #[zeroize(skip)]
    pub epoch: i64,
}

impl HmacKey {
    /// A new HMAC root key.
    pub fn random_key() -> Secret {
        xmtp_cryptography::rand::rand_secret::<42>()
    }
}

impl fmt::Debug for HmacKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HmacKey")
            .field("key", &"<redacted>")
            .field("epoch", &self.epoch)
            .finish()
    }
}

impl StoredUserPreferences {
    pub fn load(conn: impl ConnectionExt) -> Result<Self, StorageError> {
        let pref = conn.raw_query(|conn| {
            dsl::user_preferences
                .select(Self::as_select())
                .first(conn)
                .optional()
        })?;
        Ok(pref.unwrap_or_default())
    }

    // implements: SYNC-022
    pub fn store_hmac_key(
        conn: &impl crate::DbQuery,
        key: &[u8],
        cycled_at: Option<i64>,
    ) -> Result<(), StorageError> {
        if key.len() != 42 {
            return Err(StorageError::InvalidHmacLength);
        }

        use diesel::query_dsl::methods::FilterDsl;

        let local_cycle = cycled_at.is_none();
        let cycled_at = cycled_at.unwrap_or_else(now_ns);
        let preferences = Self {
            hmac_key: Some(StoredSecret::from(key.to_vec())),
            hmac_key_cycled_at_ns: Some(cycled_at),
            ..Self::default()
        };
        conn.raw_query(|conn| {
            insert_into(dsl::user_preferences)
                .values(&preferences)
                .on_conflict(dsl::id)
                .do_update()
                .set((
                    dsl::hmac_key.eq(excluded(dsl::hmac_key)),
                    dsl::hmac_key_cycled_at_ns.eq(excluded(dsl::hmac_key_cycled_at_ns)),
                ))
                .filter(
                    local_cycle
                        .into_sql::<diesel::sql_types::Bool>()
                        .or(dsl::hmac_key.is_null())
                        .or(dsl::hmac_key_cycled_at_ns.is_null())
                        .or(dsl::hmac_key_cycled_at_ns.lt(cycled_at))
                        .or(dsl::hmac_key_cycled_at_ns
                            .eq(cycled_at)
                            .and(dsl::hmac_key.lt(key))),
                )
                .execute(conn)
        })?;

        Ok(())
    }
}

#[cfg(test)]
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
            StoredUserPreferences::store_hmac_key(conn, hmac_key.as_slice(), None).unwrap();
            let pref = StoredUserPreferences::load(conn).unwrap();
            // Make sure it saved
            assert_eq!(hmac_key.as_slice(), pref.hmac_key.unwrap().as_slice());

            // check that there is only one preference stored
            let query = dsl::user_preferences.order(dsl::id.desc());
            let result = conn
                .raw_query(|conn| {
                    query
                        .select(StoredUserPreferences::as_select())
                        .load::<StoredUserPreferences>(conn)
                })
                .unwrap();
            assert_eq!(result.len(), 1);
        })
    }

    // verifies: SYNC-022
    #[xmtp_common::test(unwrap_try = true)]
    fn root_keys_converge_under_time_and_byte_order() {
        crate::test_utils::with_connection(|conn| {
            let mut lower = [0x80; 42];
            let mut higher = lower;
            lower[41] = 0x7f;
            higher[41] = 0x80;
            for (first, second) in [(lower, higher), (higher, lower)] {
                conn.raw_query(|conn| diesel::delete(dsl::user_preferences).execute(conn))?;
                StoredUserPreferences::store_hmac_key(conn, &first, Some(20))?;
                let mut preferences = StoredUserPreferences::load(conn)?;
                preferences.dm_group_updates_migrated = true;
                Store::store(&preferences, conn)?;
                StoredUserPreferences::store_hmac_key(conn, &second, Some(20))?;
                StoredUserPreferences::store_hmac_key(conn, &second, Some(20))?;
                StoredUserPreferences::store_hmac_key(conn, &[0xff; 42], Some(10))?;
                let preferences = StoredUserPreferences::load(conn)?;
                assert_eq!(preferences.hmac_key, Some(higher.to_vec().into()));
                assert_eq!(preferences.hmac_key_cycled_at_ns, Some(20));
                assert!(preferences.dm_group_updates_migrated);
                StoredUserPreferences::store_hmac_key(conn, &lower, Some(30))?;
                let preferences = StoredUserPreferences::load(conn)?;
                assert_eq!(preferences.hmac_key, Some(lower.to_vec().into()));
                assert_eq!(preferences.hmac_key_cycled_at_ns, Some(30));
                assert!(preferences.dm_group_updates_migrated);
            }
            Ok::<_, StorageError>(())
        })?;
    }

    /// The `hmac_key` column holds exactly the stored root key bytes, as the
    /// `Vec<u8>` column did, whether written by `store_hmac_key` or `Store`.
    #[xmtp_common::test(unwrap_try = true)]
    fn hmac_key_column_holds_the_input_bytes() {
        crate::test_utils::with_connection(|conn| {
            let raw_column = || -> Result<Option<Vec<u8>>, StorageError> {
                Ok(conn.raw_query(|conn| dsl::user_preferences.select(dsl::hmac_key).first(conn))?)
            };
            let key: Vec<u8> = (0..42).map(|i| 0x80 | i).collect();
            StoredUserPreferences::store_hmac_key(conn, &key, None)?;
            assert_eq!(raw_column()?, Some(key.clone()));

            let mut preferences = StoredUserPreferences::load(conn)?;
            assert_eq!(
                preferences.hmac_key.as_ref().map(StoredSecret::as_slice),
                Some(key.as_slice())
            );
            let replaced: Vec<u8> = key.iter().map(|byte| byte ^ 0x5a).collect();
            preferences.hmac_key = Some(replaced.clone().into());
            Store::store(&preferences, conn)?;
            assert_eq!(raw_column()?, Some(replaced));
            Ok::<_, StorageError>(())
        })?;
    }

    /// `Debug` of the preferences row and of a derived key shows no key bytes.
    #[xmtp_common::test(unwrap_try = true)]
    fn hmac_key_holders_redact_debug() {
        let key: [u8; 42] = std::array::from_fn(|i| 0x80 | i as u8);
        let preferences = StoredUserPreferences {
            hmac_key: Some(key.to_vec().into()),
            ..Default::default()
        };
        let derived = HmacKey { key, epoch: 7 };
        let forms = [
            hex::encode(key),
            hex::encode_upper(key),
            // How `Debug` prints a byte slice or array, anywhere in the list.
            format!("{:?}", &key[..4])
                .trim_matches(['[', ']'])
                .to_owned(),
        ];
        let derived_debug = format!("{derived:?}");
        assert!(derived_debug.contains("epoch: 7"), "{derived_debug}");
        for (name, debug) in [
            ("StoredUserPreferences", format!("{preferences:?}")),
            ("HmacKey", derived_debug),
        ] {
            for form in &forms {
                assert!(
                    !debug.contains(form.as_str()),
                    "{name} Debug shows the key as {form}: {debug}"
                );
            }
        }
    }

    // verifies: SYNC-022
    #[xmtp_common::test(unwrap_try = true)]
    fn invalid_root_key_lengths_leave_preferences_unchanged() {
        crate::test_utils::with_connection(|conn| {
            for has_key in [false, true] {
                if has_key {
                    StoredUserPreferences::store_hmac_key(conn, &[1; 42], Some(10))?;
                }
                let before = StoredUserPreferences::load(conn)?;
                for length in [0, 41, 43] {
                    assert!(matches!(
                        StoredUserPreferences::store_hmac_key(conn, &vec![2; length], Some(20)),
                        Err(StorageError::InvalidHmacLength)
                    ));
                    assert_eq!(StoredUserPreferences::load(conn)?, before);
                }
            }
            Ok::<_, StorageError>(())
        })?;
    }

    #[cfg(not(target_arch = "wasm32"))]
    struct PauseAfterQuery<C> {
        inner: C,
        pause: std::sync::Arc<std::sync::Barrier>,
        first: std::sync::atomic::AtomicBool,
    }

    #[cfg(not(target_arch = "wasm32"))]
    impl<C: ConnectionExt> ConnectionExt for PauseAfterQuery<C> {
        fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
        where
            F: FnOnce(&mut diesel::SqliteConnection) -> Result<T, diesel::result::Error>,
        {
            let result = self.inner.raw_query(fun);
            if self.first.swap(false, std::sync::atomic::Ordering::SeqCst) {
                // Let the other update commit between this query and the next.
                self.pause.wait();
                self.pause.wait();
            }
            result
        }

        fn disconnect(&self) -> Result<(), crate::ConnectionError> {
            self.inner.disconnect()
        }

        fn reconnect(&self) -> Result<(), crate::ConnectionError> {
            self.inner.reconnect()
        }
    }

    // verifies: SYNC-022
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn concurrent_root_key_updates_retain_the_winner_after_reopen() {
        use crate::XmtpTestDb;
        let path = xmtp_common::tmp_path();
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            let conn = store.db();
            StoredUserPreferences::store_hmac_key(&conn, &[1; 42], Some(10))?;
            for (lower_time, winner_time) in [(20, 30), (40, 40)] {
                let pause = std::sync::Arc::new(std::sync::Barrier::new(2));
                let contender = crate::DbConnection::new(PauseAfterQuery {
                    inner: store.conn(),
                    pause: pause.clone(),
                    first: std::sync::atomic::AtomicBool::new(true),
                });
                std::thread::scope(|scope| {
                    let update = scope.spawn(|| {
                        StoredUserPreferences::store_hmac_key(
                            &contender,
                            &[2; 42],
                            Some(lower_time),
                        )
                    });
                    pause.wait();
                    let winner =
                        StoredUserPreferences::store_hmac_key(&conn, &[3; 42], Some(winner_time));
                    pause.wait();
                    winner?;
                    update.join().unwrap()?;
                    Ok::<_, StorageError>(())
                })?;
                let preferences = StoredUserPreferences::load(&conn)?;
                assert_eq!(preferences.hmac_key, Some(vec![3; 42].into()));
                assert_eq!(preferences.hmac_key_cycled_at_ns, Some(winner_time));
            }
        }
        let store = crate::TestDb::create_persistent_store(Some(path)).await;
        let conn = store.db();
        StoredUserPreferences::store_hmac_key(&conn, &[2; 42], Some(40))?;
        let preferences = StoredUserPreferences::load(&conn)?;
        assert_eq!(preferences.hmac_key, Some(vec![3; 42].into()));
        assert_eq!(preferences.hmac_key_cycled_at_ns, Some(40));
    }
}
