use super::*;
use crate::DbConnection;

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
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut SqliteConnection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.try_lock().expect("Lock is held somewhere else");
        fun(&mut conn).map_err(crate::ConnectionError::from)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
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

    fn transaction<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<T, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        let conn = &self.conn;

        // immediate transactions force SQLite to respect BUSY_TIMEOUT
        // there are a few ways we can get DB Locked Errors:
        // 1.) A Transaction is already writing
        //  https://www.sqlite.org/rescode.html#busy
        // 2.) Promoting a transaction to write:
        // we start a transaction with BEGIN (read), then later promote the transaction to a write.
        // another tranaction is already writing, so SQLite throws Database Locked.
        // code: https://www.sqlite.org/rescode.html#busy_snapshot
        // Solution:
        // - set BUSY_TIMEOUT. this is effectively a timeout for SQLite to get a lock on the
        //      write to a table. See [BUSY_TIMOUT](xmtp_db::configuration::BUSY_TIMEOUT)
        // - use immediate_transaction to force SQLite to respect busy_timeout as soon as the
        //      tranaction starts. Otherwise, we still run into problem #2, even if BUSY_TIMEOUT is
        //      set.

        conn.raw_query_write(|c| Ok(c.immediate_transaction(|sqlite_c| f(sqlite_c))))?
    }

    fn savepoint<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<T, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        self.conn
            .raw_query_write(|c| Ok(c.transaction(|sqlite_c| f(sqlite_c))))?
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
            .raw_query_read(OpenMlsKeyValue::hash_all)
            .map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {

    #![allow(unused)]

    use crate::{
        TestDb, XmtpTestDb,
        group_intent::{IntentKind, IntentState, NewGroupIntent},
        prelude::QueryGroupIntent,
    };

    use super::*;

    // Test to ensure that we can use the transaction() callback without requiring a 'static
    // lifetimes
    // This ensures we do not propogate 'static throughout all of our code.
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
                    storage.db().insert_group_intent(NewGroupIntent {
                        kind: IntentKind::SendMessage,
                        group_id: vec![],
                        data: vec![],
                        should_push: false,
                        state: IntentState::ToPublish,
                    })
                })
                .unwrap();
            self.long_async_call().await;
        }
    }
}
