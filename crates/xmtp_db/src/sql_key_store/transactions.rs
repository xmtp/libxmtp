use super::*;
use crate::DbConnection;
use crate::TransactionOutcome;
use crate::TransactionOutcome::{Continue, Rollback};
#[cfg(not(target_arch = "wasm32"))]
use diesel::connection::SimpleConnection;

/// wrapper around a mutable connection (&mut SqliteConnection)
/// Requires that all execution/transaction happens in one thread on one connection.
/// This connection _must only_ be created from starting a transaction
pub struct MutableTransactionConnection<'a> {
    // we cannot avoid interior mutability here
    // because raw_query methods require &self, as do MlsStorage trait methods.
    // Since we no longer have async transactions, once a transaction is started
    // we can ensure it occurs all on one thread.
    pub(crate) conn: parking_lot::Mutex<&'a mut SqliteConnection>,
}

impl<'a> MutableTransactionConnection<'a> {
    pub fn new(conn: &'a mut SqliteConnection) -> Self {
        Self {
            conn: parking_lot::Mutex::new(conn),
        }
    }
}

impl<'a> ConnectionExt for MutableTransactionConnection<'a> {
    fn raw_query<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.try_lock().expect("Lock is held somewhere else");
        fun(&mut conn).map_err(crate::ConnectionError::from)
    }

    // this should cause a transaction rollback. since reconnect/disconnect is retryable
    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        Err(crate::ConnectionError::DisconnectInTransaction)
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        Err(crate::ConnectionError::ReconnectInTransaction)
    }
}

#[cfg(not(target_arch = "wasm32"))]
const DURABLE_ON_OFF: (&str, &str) = if cfg!(target_vendor = "apple") {
    (
        "PRAGMA synchronous = FULL; PRAGMA fullfsync = ON;",
        "PRAGMA synchronous = NORMAL; PRAGMA fullfsync = OFF;",
    )
} else {
    ("PRAGMA synchronous = FULL;", "PRAGMA synchronous = NORMAL;")
};

impl<C: ConnectionExt> SqlKeyStore<C> {
    fn transact<T, E, F>(&self, f: F, durable: bool) -> Result<TransactionOutcome<T>, E>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<TransactionOutcome<T>, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        #[cfg(target_arch = "wasm32")]
        let _ = durable;
        let conn = &self.conn;

        // immediate transactions force SQLite to respect BUSY_TIMEOUT
        // there are a few ways we can get DB Locked Errors:
        // 1.) A Transaction is already writing
        //  https://www.sqlite.org/rescode.html#busy
        // 2.) Promoting a transaction to write:
        // we start a transaction with BEGIN (read), then later promote the transaction to a write.
        // another transaction is already writing, so SQLite throws Database Locked.
        // code: https://www.sqlite.org/rescode.html#busy_snapshot
        // Solution:
        // - set BUSY_TIMEOUT. this is effectively a timeout for SQLite to get a lock on the
        //      write to a table. See [BUSY_TIMEOUT](xmtp_db::configuration::BUSY_TIMEOUT)
        // - use immediate_transaction to force SQLite to respect busy_timeout as soon as the
        //      transaction starts. Otherwise, we still run into problem #2, even if BUSY_TIMEOUT is
        //      set.

        // An intentional `Rollback` is turned into diesel's `RollbackTransaction`
        // sentinel (the only way to make diesel roll back) and flagged, so below we
        // can report it as `Ok(Rollback)` without inspecting the opaque `E`; any
        // other `Err` is a real failure and propagates.
        let mut rolled_back = false;
        let inner_result: Result<TransactionOutcome<T>, E> = conn
            .raw_query(|c| {
                #[cfg(not(target_arch = "wasm32"))]
                if durable {
                    c.batch_execute(DURABLE_ON_OFF.0)?;
                }
                let result = c.immediate_transaction(|sqlite_c| match f(sqlite_c) {
                    Ok(Continue(v)) => Ok(Continue(v)),
                    Ok(Rollback) => {
                        rolled_back = true;
                        Err(E::from(diesel::result::Error::RollbackTransaction))
                    }
                    Err(e) => Err(e),
                });
                #[cfg(not(target_arch = "wasm32"))]
                if durable && let Err(error) = c.batch_execute(DURABLE_ON_OFF.1) {
                    tracing::warn!(%error, "sync level not restored");
                }
                Ok(result)
            })
            .map_err(E::from)?;

        // A failed ROLLBACK after our sentinel is still reported as Ok(Rollback);
        // that rare rollback-execution error is deliberately swallowed.
        match inner_result {
            Ok(outcome) => Ok(outcome),
            Err(_) if rolled_back => Ok(Rollback),
            Err(e) => Err(e),
        }
    }
}

impl<C: ConnectionExt> XmtpMlsStorageProvider for SqlKeyStore<C> {
    type Connection = C;

    type TxQuery = SqliteConnection;

    type DbQuery<'a>
        = DbConnection<&'a C>
    where
        Self::Connection: 'a;

    fn db<'a>(&'a self) -> Self::DbQuery<'a> {
        DbConnection::new(&self.conn)
    }

    #[xmtp_common::db_span(redact_error)]
    fn transaction<T, E, F>(&self, f: F) -> Result<TransactionOutcome<T>, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        self.transact(f, false)
    }

    #[xmtp_common::db_span(redact_error)]
    fn durable_transaction<T, E, F>(&self, f: F) -> Result<TransactionOutcome<T>, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        self.transact(f, true)
    }

    // Same Rollback-sentinel handling as `transact`; see there for the rationale.
    fn savepoint<T, E, F>(&self, f: F) -> Result<TransactionOutcome<T>, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<TransactionOutcome<T>, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        let mut rolled_back = false;
        let inner_result: Result<TransactionOutcome<T>, E> = self
            .conn
            .raw_query(|c| {
                Ok(c.transaction(|sqlite_c| match f(sqlite_c) {
                    Ok(Continue(v)) => Ok(Continue(v)),
                    Ok(Rollback) => {
                        rolled_back = true;
                        Err(E::from(diesel::result::Error::RollbackTransaction))
                    }
                    Err(e) => Err(e),
                }))
            })
            .map_err(E::from)?;

        match inner_result {
            Ok(outcome) => Ok(outcome),
            Err(_) if rolled_back => Ok(Rollback),
            Err(e) => Err(e),
        }
    }

    fn read<V: Entity<CURRENT_VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Option<V>, SqlKeyStoreError> {
        self.read(label, key)
    }

    fn read_list<V: Entity<CURRENT_VERSION>>(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<Vec<V>, <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        self.read_list(label, key)
    }

    fn delete(
        &self,
        label: &[u8],
        key: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        self.delete::<CURRENT_VERSION>(label, key)
    }

    fn write(
        &self,
        label: &[u8],
        key: &[u8],
        value: &[u8],
    ) -> Result<(), <Self as StorageProvider<CURRENT_VERSION>>::Error> {
        self.write::<CURRENT_VERSION>(label, key, value)
    }

    #[cfg(feature = "test-utils")]
    fn hash_all(&self) -> Result<Vec<u8>, SqlKeyStoreError> {
        self.conn
            .raw_query(OpenMlsKeyValue::hash_all)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {

    #![allow(unused)]

    use crate::{
        TestDb, TransactionOutcome, XmtpTestDb,
        group_intent::{IntentKind, IntentState, NewGroupIntent},
        prelude::QueryGroupIntent,
    };
    use xmtp_proto::types::GroupId;

    use super::*;

    #[cfg(not(target_arch = "wasm32"))]
    #[rstest::rstest]
    #[case::commit(0)]
    #[case::rollback(1)]
    #[case::error(2)]
    #[xmtp_common::test(unwrap_try = true)]
    fn durable_transaction_syncs_and_restores(#[case] exit: u8) -> Result<(), crate::StorageError> {
        use crate::{NativeDb, StorageError, XmtpDb};
        #[derive(diesel::QueryableByName)]
        struct Pragma {
            #[diesel(sql_type = diesel::sql_types::Integer)]
            synchronous: i32,
        }
        let db = NativeDb::builder()
            .persistent(xmtp_common::tmp_path())
            .max_pool_size(1)
            .min_pool_size(1)
            .build_unencrypted()?;
        let storage = SqlKeyStore::new(db.conn());
        let read = |conn: &mut SqliteConnection| {
            diesel::sql_query("PRAGMA synchronous")
                .get_result::<Pragma>(conn)
                .unwrap()
                .synchronous
        };
        let result: Result<TransactionOutcome<()>, StorageError> =
            storage.durable_transaction(|conn| {
                assert_eq!(read(conn), 2, "durable transaction must use FULL");
                #[cfg(target_vendor = "apple")]
                assert_eq!(
                    diesel::sql_query("SELECT fullfsync AS synchronous FROM pragma_fullfsync")
                        .get_result::<Pragma>(conn)?
                        .synchronous,
                    1
                );
                match exit {
                    0 => Ok(Continue(())),
                    1 => Ok(Rollback),
                    _ => Err(diesel::result::Error::RollbackTransaction.into()),
                }
            });
        assert!(matches!(
            (&result, exit),
            (Ok(Continue(())), 0) | (Ok(Rollback), 1) | (Err(_), 2)
        ));
        assert_eq!(storage.db().raw_query(|conn| Ok(read(conn)))?, 1);
        Ok(())
    }

    // Test to ensure that we can use the transaction() callback without requiring a 'static
    // lifetimes
    // This ensures we do not propagate 'static throughout all of our code.
    // have not figured out a good, ergonomic way to pass SqlKeyStore directly into the
    // transaction callback
    struct Foo<C> {
        key_store: SqlKeyStore<C>,
    }

    impl<C> Foo<C>
    where
        C: ConnectionExt,
    {
        async fn long_async_call(&self) {
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        async fn db_op(&self) {
            self.long_async_call().await;

            self.key_store
                .transaction(|conn| {
                    let storage = conn.key_store();
                    storage
                        .db()
                        .insert_group_intent(NewGroupIntent {
                            kind: IntentKind::SendMessage,
                            group_id: GroupId::default(),
                            data: vec![].into(),
                            should_push: false,
                            state: IntentState::ToPublish,
                        })
                        .map(Continue)
                })
                .unwrap();
            self.long_async_call().await;
        }
    }
}
