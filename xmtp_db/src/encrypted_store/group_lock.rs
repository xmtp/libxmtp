use super::schema::group_locks;
use crate::{ConnectionExt, StorageError};
use diesel::prelude::*;
use std::time::Duration;
use xmtp_common::{NS_IN_MIN, time::now_ns};

#[derive(Queryable, Identifiable, Debug, Clone)]
#[diesel(table_name = group_locks)]
#[diesel(primary_key(group_id))]
pub struct GroupLock {
    pub group_id: Vec<u8>,
    pub locked_at_ns: i64,
    pub expires_at_ns: i64,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = group_locks)]
#[diesel(primary_key(group_id))]
pub struct GroupLockRef<'a> {
    group_id: &'a [u8],
    locked_at_ns: i64,
    expires_at_ns: i64,
}

pub trait QueryGroupLock {
    fn get_group_locks(self) -> Result<Vec<GroupLock>, StorageError>;
    fn get_group_lock(self, group_id: &[u8]) -> Result<Option<GroupLock>, StorageError>;
    fn create_group_lock(
        &mut self,
        group_id: &[u8],
        expires_in: Duration,
    ) -> Result<GroupLock, StorageError>;
    fn delete_group_lock(
        &mut self,
        group_id: &[u8],
        locked_at_ns: i64,
    ) -> Result<usize, StorageError>;
}

impl<C: ConnectionExt> QueryGroupLock for C {
    fn get_group_locks(self) -> Result<Vec<GroupLock>, StorageError> {
        self.raw_query_read(|conn| group_locks::table.load(conn))
            .map_err(Into::into)
    }
    fn get_group_lock(self, group_id: &[u8]) -> Result<Option<GroupLock>, StorageError> {
        self.raw_query_read(|conn| {
            group_locks::table
                .filter(group_locks::group_id.eq(group_id))
                .first(conn)
                .optional()
        })
        .map_err(Into::into)
    }
    fn create_group_lock(
        &mut self,
        group_id: &[u8],
        expires_in: Duration,
    ) -> Result<GroupLock, StorageError> {
        self.raw_query_write(|conn| {
            let locked_at_ns = now_ns();
            let expires_at_ns = locked_at_ns
                .saturating_add(expires_in.as_nanos().min(NS_IN_MIN as u128 * 5) as i64);
            let group_lock = GroupLockRef {
                group_id,
                expires_at_ns,
                locked_at_ns,
            };
            diesel::insert_into(group_locks::table)
                .values(&group_lock)
                .get_result::<GroupLock>(conn)
        })
        .map_err(Into::into)
    }
    fn delete_group_lock(
        &mut self,
        group_id: &[u8],
        locked_at_ns: i64,
    ) -> Result<usize, StorageError> {
        self.raw_query_write(|conn| {
            diesel::delete(
                group_locks::table.filter(
                    group_locks::group_id
                        .eq(group_id)
                        .and(group_locks::locked_at_ns.eq(locked_at_ns)),
                ),
            )
            .execute(conn)
        })
        .map_err(Into::into)
    }
}

impl QueryGroupLock for &mut diesel::SqliteConnection {
    fn get_group_locks(self) -> Result<Vec<GroupLock>, StorageError> {
        group_locks::table.load(self).map_err(Into::into)
    }
    fn get_group_lock(self, group_id: &[u8]) -> Result<Option<GroupLock>, StorageError> {
        group_locks::table
            .filter(group_locks::group_id.eq(group_id))
            .first(self)
            .optional()
            .map_err(Into::into)
    }
    fn create_group_lock(
        &mut self,
        group_id: &[u8],
        expires_in: Duration,
    ) -> Result<GroupLock, StorageError> {
        let locked_at_ns = now_ns();
        let expires_at_ns =
            locked_at_ns.saturating_add(expires_in.as_nanos().min(NS_IN_MIN as u128 * 5) as i64);
        let group_lock = GroupLockRef {
            group_id,
            expires_at_ns,
            locked_at_ns,
        };
        diesel::insert_into(group_locks::table)
            .values(&group_lock)
            .get_result::<GroupLock>(*self)
            .map_err(Into::into)
    }
    fn delete_group_lock(
        &mut self,
        group_id: &[u8],
        locked_at_ns: i64,
    ) -> Result<usize, StorageError> {
        diesel::delete(
            group_locks::table.filter(
                group_locks::group_id
                    .eq(group_id)
                    .and(group_locks::locked_at_ns.eq(locked_at_ns)),
            ),
        )
        .execute(*self)
        .map_err(Into::into)
    }
}

/// This is a group that is used to prevent multiple changes to a group at the same time.
/// It can only exist while the group is locked in the database, meaning that holding one of these
/// grants exclusive access to the group for the duration of the guard.
pub struct GroupGuard<Q: QueryGroupLock> {
    lock: Option<GroupLock>,
    conn: Q,
}

impl<Q: QueryGroupLock> std::fmt::Debug for GroupGuard<Q> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "GroupGuard {{ lock: {:?}, conn: {} }}",
            self.lock,
            std::any::type_name::<Q>()
        )
    }
}

impl<Q: QueryGroupLock> GroupGuard<Q> {
    pub fn acquire(
        group_id: &[u8],
        expires_in: Duration,
        mut conn: Q,
    ) -> Result<Self, StorageError> {
        let group_lock = conn.create_group_lock(group_id, expires_in)?;
        Ok(Self {
            lock: Some(group_lock),
            conn,
        })
    }
    pub fn conn<'a>(&'a self) -> &'a Q
    where
        Q: 'a,
    {
        &self.conn
    }
    pub fn release(mut self) -> Result<(), StorageError> {
        self.release_inner()
    }
    fn release_inner(&mut self) -> Result<(), StorageError> {
        let Some(lock) = &self.lock else {
            unreachable!("Lock is never taken outside of this method");
        };
        self.conn
            .delete_group_lock(&lock.group_id, lock.locked_at_ns)?;
        self.lock = None;
        Ok(())
    }
}

impl<Q: QueryGroupLock> Drop for GroupGuard<Q> {
    fn drop(&mut self) {
        if let Some(lock) = &self.lock {
            let group_id = hex::encode(&lock.group_id);
            self.release_inner()
                .inspect_err(|e| tracing::error!(group_id, "release group lock error: {e:?}"))
                .ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        DbConnection,
        test_utils::{TestDb, XmtpTestDb},
    };
    use xmtp_common::NS_IN_SEC;

    // TODO: test this with more database connection variations
    #[xmtp_common::test]
    async fn test_group_lock_acquisition_without_transaction() {
        let db = TestDb::create_ephemeral_store().await;
        let conn1 = DbConnection::new(db.conn());
        let conn2 = DbConnection::new(db.conn());

        let group_id = vec![1, 2, 3];
        let now = now_ns();
        let guard = GroupGuard::acquire(&group_id, Duration::from_secs(1), &conn1).unwrap();
        assert!(guard.lock.is_some());
        let lock = guard.lock.as_ref().unwrap();
        assert!(lock.group_id == group_id);
        // apparently these two times can be identical in wasm
        assert!(lock.locked_at_ns >= now);
        assert!(lock.locked_at_ns + NS_IN_SEC == lock.expires_at_ns);
        let _ = GroupGuard::acquire(&group_id, Duration::from_secs(1), &conn2).unwrap_err();
        drop(guard);
        GroupGuard::acquire(&group_id, Duration::from_secs(1), &conn2).unwrap();
    }

    // TODO: test this with more database connection variations
    xmtp_common::if_native! {
        #[xmtp_common::test]
        async fn test_group_lock_acquisition_with_transaction() {
            let temp_path = xmtp_common::tmp_path();
            let db1 = TestDb::create_persistent_store(Some(temp_path.clone())).await;
            let conn1 = DbConnection::new(db1.conn());
            let db2 = TestDb::create_persistent_store(Some(temp_path)).await;
            let conn2 = DbConnection::new(db2.conn());
            let group_id = vec![1, 2, 3];

            // Use channels to coordinate between the transaction and the second attempt
            let (tx_start, rx_start) = std::sync::mpsc::channel();
            let (tx_acquired, rx_acquired) = std::sync::mpsc::channel();
            let (tx_commit, rx_commit) = std::sync::mpsc::channel();

            // Spawn a thread that will try to acquire the lock from conn2
            let group_id_clone = group_id.clone();
            let handle = std::thread::spawn(move || {
                let span = tracing::span!(tracing::Level::INFO, "thread2");
                let _enter = span.enter();
                tracing::info!("thread2 start");
                conn2
                    .conn
                    .raw_query_write(|c| {
                        tracing::info!("rx_start.recv");
                        // Wait for signal that transaction has started
                        rx_start.recv().unwrap();
                        tracing::info!("Transaction started");
                        // block to ensure that conn2 is not borrowed
                        {
                            // Try to acquire the lock - this should fail because conn1's transaction hasn't committed
                            tracing::info!("acquire 1 start");

                            let result =
                                GroupGuard::acquire(&group_id_clone, Duration::from_secs(1), &mut *c);
                            tracing::info!(?result, "Result of acquisition");
                            // Signal that we've attempted the acquisition
                            tx_acquired.send(result.is_err()).unwrap();
                        }
                        tracing::info!("rx_commit recv");
                        // Wait for transaction to commit
                        rx_commit.recv().unwrap();
                        tracing::info!("acquire 2");
                        // Now try again - this should succeed after the transaction commits
                        let _lock =
                            GroupGuard::acquire(&group_id_clone, Duration::from_secs(1), c).unwrap();
                        Ok(())
                    })
                    .unwrap();
                GroupGuard::acquire(&group_id_clone, Duration::from_secs(1), conn2).unwrap()
            });

            let span = tracing::span!(tracing::Level::INFO, "thread1");
            let _enter = span.enter();
            // Start transaction in conn1 and acquire lock
            conn1
                .conn
                .raw_query_write(|c| {
                    c.exclusive_transaction(|c| {
                        tracing::info!("create group lock");
                        let guard = GroupGuard::acquire(&group_id, Duration::from_secs(1), &mut *c)
                            .map_err(|e| diesel::result::Error::DeserializationError(e.into()))?;
                        tracing::info!("inserted lock, tx_start.send");
                        // Signal that transaction has started
                        tx_start.send(()).unwrap();

                        tracing::info!("rx_acquired.recv");
                        // Verify that conn2's attempt failed
                        let failed = rx_acquired.recv().unwrap();
                        tracing::info!(failed = failed, "failed");
                        assert!(
                            failed,
                            "Second lock acquisition should fail while transaction is open"
                        );
                        guard.release().unwrap();
                        Ok::<_, diesel::result::Error>(())
                    })?;
                    tracing::info!("tx_commit.send");
                    // Signal that transaction is committed
                    tx_commit.send(()).unwrap();
                    tracing::info!("tx_commit.send done");
                    Ok(())
                })
                .unwrap();

            // Wait for the thread to complete
            let guard = handle.join().unwrap();
            tracing::info!(?guard, "Guard acquired");
        }
    }
}
