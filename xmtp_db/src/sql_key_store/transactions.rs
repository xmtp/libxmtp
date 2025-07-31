use super::*;
use crate::{DbConnection, TransactionGuard};
use diesel::connection::LoadConnection;
use diesel::migration::MigrationConnection;
use diesel::sqlite::Sqlite;
use diesel_migrations::MigrationHarness;
use std::cell::RefCell;

/// wrapper around a mutable connection (&mut SqliteConnection)
/// Requires that all execution/transaction happens in one thread on one connection.
/// This connection _must only_ be created from starting a transaction
pub struct MutableTransactionConnection<'a, C> {
    // we cannot avoid interior mutability here
    // because raw_query methods require &self, as do MlsStorage trait methods.
    // Since we no longer have async transactions, once a transaction is started
    // we can ensure it occurs all on one thread.
    pub(crate) conn: RefCell<&'a mut C>,
}

impl<'a, C> MutableTransactionConnection<'a, C> {
    pub fn new(conn: &'a mut C) -> Self {
        Self {
            conn: RefCell::new(conn),
        }
    }
}

impl<'a, C> ConnectionExt for MutableTransactionConnection<'a, C>
where
    C: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<C as diesel::Connection>::Backend>
        + crate::TransactionalKeyStore
        + Send,
{
    type Connection = C;

    fn start_transaction(&self) -> Result<TransactionGuard, crate::ConnectionError> {
        Err(crate::ConnectionError::Database(
            diesel::result::Error::AlreadyInTransaction,
        ))
    }

    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.borrow_mut();
        fun(&mut conn).map_err(crate::ConnectionError::from)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        let mut conn = self.conn.borrow_mut();
        fun(&mut conn).map_err(crate::ConnectionError::from)
    }

    fn is_in_transaction(&self) -> bool {
        true
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

    type DbQuery<'a>
        = DbConnection<&'a C>
    where
        Self::Connection: 'a;

    type TxQuery = <C as ConnectionExt>::Connection;

    fn db<'a>(&'a self) -> Self::DbQuery<'a> {
        DbConnection::new(&self.conn)
    }

    fn transaction<T, E, F>(&self, f: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::TxQuery) -> Result<T, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        let conn = &self.conn;

        let _guard = self.conn.start_transaction(); // still needed so any reads use tx
        // one call to raw_query_write = mutex only locked once for entire transaciton
        conn.raw_query_write(|c| Ok(c.transaction(|sqlite_c| f(sqlite_c))))?
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
