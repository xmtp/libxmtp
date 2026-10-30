use super::{
    ConnectionExt,
    schema::user_preferences::{self, dsl},
};
use crate::{
    ConnectionError, DbQuery, StorageError, Store, StoredSecret,
    consent_record::{MAX_AHEAD_NS, Owed},
    sync_outbox::next_owed_seq,
};
use diesel::prelude::*;
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
    /// 0 when the root key is not owed, else the mark that owes it.
    pub hmac_key_owed_seq: i64,
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

/// Length in bytes of the HMAC root key.
pub const HMAC_ROOT_KEY_LEN: usize = 42;

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
        xmtp_cryptography::rand::rand_secret::<HMAC_ROOT_KEY_LEN>()
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
}

/// Every root-key write in the client. No `Debug`: it can hold a key.
#[derive(Clone, Copy)]
pub enum RootKeyWrite<'a> {
    /// A new random key, timed after the clock, after any key this
    /// installation holds, and at or after `not_before_ns`.
    Cycle { not_before_ns: i64 },
    /// A key from a sync message, its time capped at `bound_ns`.
    Received {
        key: &'a [u8],
        cycled_at_ns: i64,
        bound_ns: i64,
    },
    /// This installation published this key; store what its receivers stored.
    Settle {
        key: &'a [u8],
        cycled_at_ns: i64,
        bound_ns: i64,
    },
}

impl RootKeyWrite<'_> {
    /// A cycle with no lower bound: a key made because none is held.
    pub const MADE: Self = Self::Cycle {
        not_before_ns: i64::MIN,
    };

    /// A cycle for a revocation the backend timed at `revoked_at_ns`.
    /// Every key the revoked installation sent before it is capped below this.
    pub fn revocation(revoked_at_ns: i64) -> Self {
        Self::Cycle {
            not_before_ns: revoked_at_ns.saturating_add(MAX_AHEAD_NS),
        }
    }
}

/// The only function that writes the root key. `true` when the key changed.
// implements: SYNC-026, SYNC-027, SYNC-032, SYNC-034
pub fn write_root_key(
    db: &impl DbQuery,
    write: RootKeyWrite<'_>,
    owed: Owed,
) -> Result<bool, StorageError> {
    loop {
        let held = StoredUserPreferences::load(db)?;
        let held_key = held.hmac_key.as_ref().map(StoredSecret::as_slice);
        let held_at = held_key.and(held.hmac_key_cycled_at_ns);
        let (key, at_ns) = match write {
            RootKeyWrite::Cycle { not_before_ns } => {
                let after_held = held_at.map_or(i64::MIN, |t| t.saturating_add(1));
                let at_ns = now_ns().max(after_held).max(not_before_ns);
                (HmacKey::random_key(), at_ns)
            }
            // Only the exact key and time this installation sent, and only downward.
            RootKeyWrite::Settle {
                key,
                cycled_at_ns,
                bound_ns,
            } => {
                if held_key != Some(key)
                    || held_at != Some(cycled_at_ns)
                    || cycled_at_ns <= bound_ns
                {
                    return Ok(false);
                }
                (Secret::new(key.to_vec()), bound_ns)
            }
            RootKeyWrite::Received {
                key,
                cycled_at_ns,
                bound_ns,
            } => {
                if key.len() != HMAC_ROOT_KEY_LEN {
                    return Ok(false);
                }
                let at_ns = cycled_at_ns.min(bound_ns);
                let greater = match (held_key, held_at) {
                    (Some(k), Some(t)) => (at_ns, key) > (t, k),
                    _ => true,
                };
                if !greater {
                    return Ok(false);
                }
                (Secret::new(key.to_vec()), at_ns)
            }
        };
        let owed_seq = match (write, owed) {
            (RootKeyWrite::Cycle { .. }, _) | (_, Owed::Mark) => next_owed_seq(db)?,
            (_, Owed::Keep) => held.hmac_key_owed_seq,
        };
        if replace_root_key(db, &held, key.as_slice(), at_ns, owed_seq)? {
            return Ok(true);
        }
        // Another writer changed the key after the read. Read it again.
    }
}

/// Store the key only if the row still holds what `held` read.
fn replace_root_key(
    db: &impl ConnectionExt,
    held: &StoredUserPreferences,
    key: &[u8],
    at_ns: i64,
    owed_seq: i64,
) -> Result<bool, ConnectionError> {
    let held_key = held.hmac_key.as_ref().map(StoredSecret::as_slice);
    let n = db.raw_query(|c| {
        diesel::update(dsl::user_preferences)
            .filter(dsl::hmac_key.is(held_key))
            .filter(dsl::hmac_key_cycled_at_ns.is(held.hmac_key_cycled_at_ns))
            .filter(dsl::hmac_key_owed_seq.eq(held.hmac_key_owed_seq))
            .set((
                dsl::hmac_key.eq(Some(key)),
                dsl::hmac_key_cycled_at_ns.eq(Some(at_ns)),
                dsl::hmac_key_owed_seq.eq(owed_seq),
            ))
            .execute(c)
    })?;
    Ok(n == 1)
}

/// Store `key` as received with no cap, not owed. Tests only.
#[cfg(any(test, feature = "test-utils"))]
pub fn store_test_root_key(
    db: &impl DbQuery,
    key: &[u8],
    cycled_at_ns: i64,
) -> Result<bool, StorageError> {
    let write = RootKeyWrite::Received {
        key,
        cycled_at_ns,
        bound_ns: i64::MAX,
    };
    write_root_key(db, write, Owed::Keep)
}

/// The root key, made when none is held. `true` when it was
/// made; the caller then emits the root-key events.
pub fn ensure_root_key(db: &impl DbQuery) -> Result<(StoredSecret, bool), StorageError> {
    let mut made = false;
    loop {
        if let Some(key) = StoredUserPreferences::load(db)?.hmac_key {
            return Ok((key, made));
        }
        made |= write_root_key(db, RootKeyWrite::MADE, Owed::Mark)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Store a received key with no cap, as the old merge did.
    fn receive(conn: &impl DbQuery, key: &[u8], cycled_at_ns: i64) -> Result<bool, StorageError> {
        let write = RootKeyWrite::Received {
            key,
            cycled_at_ns,
            bound_ns: i64::MAX,
        };
        write_root_key(conn, write, Owed::Keep)
    }

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

            // make an hmac key
            let (hmac_key, made) = ensure_root_key(conn).unwrap();
            assert!(made);
            let pref = StoredUserPreferences::load(conn).unwrap();
            // Make sure it saved, marked owed
            assert_eq!(hmac_key.as_slice(), pref.hmac_key.unwrap().as_slice());
            assert!(pref.hmac_key_owed_seq > 0);
            assert!(!ensure_root_key(conn).unwrap().1);

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

    // verifies: SYNC-032
    #[xmtp_common::test(unwrap_try = true)]
    fn root_keys_converge_under_time_and_byte_order() {
        crate::test_utils::with_connection(|conn| {
            let mut lower = [0x80; 42];
            let mut higher = lower;
            lower[41] = 0x7f;
            higher[41] = 0x80;
            for (first, second) in [(lower, higher), (higher, lower)] {
                conn.raw_query(|conn| {
                    diesel::update(dsl::user_preferences)
                        .set((
                            dsl::hmac_key.eq(None::<Vec<u8>>),
                            dsl::hmac_key_cycled_at_ns.eq(None::<i64>),
                        ))
                        .execute(conn)
                })?;
                receive(conn, &first, 20)?;
                let mut preferences = StoredUserPreferences::load(conn)?;
                preferences.dm_group_updates_migrated = true;
                Store::store(&preferences, conn)?;
                receive(conn, &second, 20)?;
                receive(conn, &second, 20)?;
                receive(conn, &[0xff; 42], 10)?;
                let preferences = StoredUserPreferences::load(conn)?;
                assert_eq!(preferences.hmac_key, Some(higher.to_vec().into()));
                assert_eq!(preferences.hmac_key_cycled_at_ns, Some(20));
                assert!(preferences.dm_group_updates_migrated);
                receive(conn, &lower, 30)?;
                let preferences = StoredUserPreferences::load(conn)?;
                assert_eq!(preferences.hmac_key, Some(lower.to_vec().into()));
                assert_eq!(preferences.hmac_key_cycled_at_ns, Some(30));
                assert!(preferences.dm_group_updates_migrated);
            }
            Ok::<_, StorageError>(())
        })?;
    }

    /// The `hmac_key` column holds exactly the stored root key bytes, as the
    /// `Vec<u8>` column did, whether written by `write_root_key` or `Store`.
    #[xmtp_common::test(unwrap_try = true)]
    fn hmac_key_column_holds_the_input_bytes() {
        crate::test_utils::with_connection(|conn| {
            let raw_column = || -> Result<Option<Vec<u8>>, StorageError> {
                Ok(conn.raw_query(|conn| dsl::user_preferences.select(dsl::hmac_key).first(conn))?)
            };
            let key: Vec<u8> = (0..42).map(|i| 0x80 | i).collect();
            receive(conn, &key, 1)?;
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

    // verifies: SYNC-032
    #[xmtp_common::test(unwrap_try = true)]
    fn invalid_root_key_lengths_leave_preferences_unchanged() {
        crate::test_utils::with_connection(|conn| {
            for has_key in [false, true] {
                if has_key {
                    receive(conn, &[1; 42], 10)?;
                }
                let before = StoredUserPreferences::load(conn)?;
                for length in [0, 41, 43] {
                    assert!(!receive(conn, &vec![2; length], 20)?);
                    assert_eq!(StoredUserPreferences::load(conn)?, before);
                }
            }
            Ok::<_, StorageError>(())
        })?;
    }

    /// A cycle comes after the held key and, for a revocation, a day after
    /// its server time; a received key is capped at its envelope plus a day;
    /// a settle lowers only the exact key this installation sent.
    // verifies: SYNC-026, SYNC-027, SYNC-032, SYNC-034
    #[xmtp_common::test(unwrap_try = true)]
    fn root_key_rules() {
        crate::test_utils::with_connection(|conn| {
            let held = |conn: &_| StoredUserPreferences::load(conn);
            let now = now_ns();
            let ahead = now + 23 * xmtp_common::NS_IN_HOUR;
            receive(conn, &[1; 42], ahead)?;
            assert_eq!(held(conn)?.hmac_key_owed_seq, 0);
            assert!(write_root_key(conn, RootKeyWrite::MADE, Owed::Keep)?);
            let cycled = held(conn)?;
            assert_eq!(cycled.hmac_key_cycled_at_ns, Some(ahead + 1));
            assert_ne!(cycled.hmac_key, Some(vec![1; 42].into()));
            assert!(cycled.hmac_key_owed_seq > 0, "a cycle is always owed");

            let revoked_at = now + xmtp_common::NS_IN_HOUR;
            write_root_key(conn, RootKeyWrite::revocation(revoked_at), Owed::Mark)?;
            let revocation = held(conn)?.hmac_key_cycled_at_ns.unwrap();
            assert_eq!(revocation, revoked_at + MAX_AHEAD_NS);
            // A key sent before the revocation is capped below the cycle.
            let envelope = revoked_at - 1;
            let late = RootKeyWrite::Received {
                key: &[0xff; 42],
                cycled_at_ns: now + 2 * xmtp_common::NS_IN_DAY,
                bound_ns: envelope + MAX_AHEAD_NS,
            };
            assert!(!write_root_key(conn, late, Owed::Mark)?);
            assert_eq!(held(conn)?.hmac_key_cycled_at_ns, Some(revocation));

            // Received two days after its envelope: stored at envelope plus a day.
            let far = revocation + 3 * xmtp_common::NS_IN_DAY;
            let received = RootKeyWrite::Received {
                key: &[2; 42],
                cycled_at_ns: far + 2 * xmtp_common::NS_IN_DAY,
                bound_ns: far + MAX_AHEAD_NS,
            };
            assert!(write_root_key(conn, received, Owed::Keep)?);
            assert_eq!(held(conn)?.hmac_key_cycled_at_ns, Some(far + MAX_AHEAD_NS));

            // Settle: only the exact sent key and time, only downward, mark kept.
            let mark = held(conn)?.hmac_key_owed_seq;
            fn settle(key: &[u8], cycled_at_ns: i64, bound_ns: i64) -> RootKeyWrite<'_> {
                RootKeyWrite::Settle {
                    key,
                    cycled_at_ns,
                    bound_ns,
                }
            }
            let sent_at = far + MAX_AHEAD_NS;
            assert!(!write_root_key(
                conn,
                settle(&[3; 42], sent_at, far),
                Owed::Keep
            )?);
            assert!(!write_root_key(
                conn,
                settle(&[2; 42], sent_at + 1, far),
                Owed::Keep
            )?);
            assert!(!write_root_key(
                conn,
                settle(&[2; 42], sent_at, sent_at),
                Owed::Keep
            )?);
            assert!(write_root_key(
                conn,
                settle(&[2; 42], sent_at, far),
                Owed::Keep
            )?);
            let settled = held(conn)?;
            assert_eq!(settled.hmac_key, Some(vec![2; 42].into()));
            assert_eq!(settled.hmac_key_cycled_at_ns, Some(far));
            assert_eq!(settled.hmac_key_owed_seq, mark);
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

    // verifies: SYNC-032
    #[cfg(not(target_arch = "wasm32"))]
    #[xmtp_common::test(unwrap_try = true)]
    async fn concurrent_root_key_updates_retain_the_winner_after_reopen() {
        use crate::XmtpTestDb;
        let path = xmtp_common::tmp_path();
        {
            let store = crate::TestDb::create_persistent_store(Some(path.clone())).await;
            let conn = store.db();
            receive(&conn, &[1; 42], 10)?;
            for (lower_time, winner_time) in [(20, 30), (40, 40)] {
                let pause = std::sync::Arc::new(std::sync::Barrier::new(2));
                let contender = crate::DbConnection::new(PauseAfterQuery {
                    inner: store.conn(),
                    pause: pause.clone(),
                    first: std::sync::atomic::AtomicBool::new(true),
                });
                std::thread::scope(|scope| {
                    let update = scope.spawn(|| receive(&contender, &[2; 42], lower_time));
                    pause.wait();
                    let winner = receive(&conn, &[3; 42], winner_time);
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
        receive(&conn, &[2; 42], 40)?;
        let preferences = StoredUserPreferences::load(&conn)?;
        assert_eq!(preferences.hmac_key, Some(vec![3; 42].into()));
        assert_eq!(preferences.hmac_key_cycled_at_ns, Some(40));
    }
}
