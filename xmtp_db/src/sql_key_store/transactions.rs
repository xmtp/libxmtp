use super::*;
use crate::{DbConnection, TransactionGuard};
use diesel::connection::LoadConnection;
use diesel::migration::MigrationConnection;
use diesel::sqlite::Sqlite;
use diesel_migrations::MigrationHarness;

/// wrapper around a mutable connection (&mut SqliteConnection)
/// Requires that all execution/transaction happens in one thread on one connection.
/// This connection _must only_ be created from starting a transaction
pub struct MutableTransactionConnection<'a, C> {
    // we cannot avoid interior mutability here
    // because raw_query methods require &self, as do MlsStorage trait methods.
    // Since we no longer have async transactions, once a transaction is started
    // we can ensure it occurs all on one thread.
    conn: RefCell<&'a mut C>,
}

impl<'a, C> ConnectionExt for MutableTransactionConnection<'a, C>
where
    C: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<C as diesel::Connection>::Backend>
        + Send,
{
    type Connection = C;

    fn start_transaction(&self) -> Result<TransactionGuard<'a>, crate::ConnectionError> {
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

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        // generally does not make sense
        Ok(())
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        // generally does not make sense
        Ok(())
    }
}

impl<'store, C> SqlKeyStoreRef<'store, C>
where
    C: ConnectionExt + 'store,
{
    fn inner_transaction<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        for<'a> F: FnOnce(
            &'a SqlKeyStoreRef<'a, MutableTransactionConnection<'a, <C as ConnectionExt>::Connection>>,
        ) -> Result<T, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        let _guard = self.conn.start_transaction()?;
        let conn = &self.conn;

        // one call to raw_query_write = mutex only locked once for entire transaciton
        let r = conn.raw_query_write(|c| {
            Ok(c.transaction(|sqlite_c| {
                let s = SqlKeyStoreRef {
                    conn: BorrowedOrOwned::Owned(MutableTransactionConnection {
                        conn: RefCell::new(sqlite_c),
                    }),
                };
                fun(&s)
            }))
        })?;
        Ok(r?)
    }
}

impl<'a, C> XmtpMlsTransactionProvider
    for SqlKeyStoreRef<'a, MutableTransactionConnection<'a, C>>
where
    C: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<C as diesel::Connection>::Backend>
        + Send,
{
    type Storage = SqlKeyStoreRef<'a, MutableTransactionConnection<'a, C>>;

    fn storage(&self) -> &Self::Storage {
        &self
    }
}

pub trait XmtpMlsTransactionProvider {
    type Storage: XmtpMlsStorageProvider;

    fn storage(&self) -> &Self::Storage;
}

impl<'store, C: ConnectionExt> XmtpMlsStorageProvider for SqlKeyStoreRef<'store, C> {
    type Connection = C;

    type DbQuery<'a>
        = DbConnection<&'a C>
    where
        Self::Connection: 'a;

    type Transaction<'a>
        = SqlKeyStoreRef<'a, MutableTransactionConnection<'a, <C as ConnectionExt>::Connection>>
    where
        <C as ConnectionExt>::Connection: 'a;

    fn conn(&self) -> &Self::Connection {
        &self.conn
    }

    fn db<'a>(&'a self) -> Self::DbQuery<'a>
    where
        C: 'a,
    {
        DbConnection::new(&self.conn)
    }

    fn transaction<T, E, F>(&self, f: F) -> Result<T, E>
    where
        for<'a> F: FnOnce(&'a Self::Transaction<'a>) -> Result<T, E>,
        for<'a> C: 'a,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
    {
        self.inner_transaction(f)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{
        EphemeralDbConnection, NativeDbConnection, PersistentOrMem, TestDb, XmtpTestDb,
        group_intent::{IntentKind, IntentState, NewGroupIntent},
        prelude::QueryGroupIntent,
    };

    use super::*;

    struct Foo<'a, C> {
        key_store: SqlKeyStoreRef<'a, C>,
    }

    impl<'a, C> Foo<'a, C>
    where
        C: ConnectionExt,
    {
        async fn long_async_call(&self) {
            xmtp_common::time::sleep(std::time::Duration::from_millis(10)).await;
        }

        async fn db_op(&self) {
            // self.long_async_call().await;

            self.key_store
                .transaction(|storage| {
                    storage.storage().db().insert_group_intent(NewGroupIntent {
                        kind: IntentKind::SendMessage,
                        group_id: vec![],
                        data: vec![],
                        should_push: false,
                        state: IntentState::ToPublish,
                    })
                })
                .unwrap();
            // self.long_async_call().await;
        }
    }

    #[xmtp_common::test]
    async fn test_tx() {
        let store = TestDb::create_persistent_store(None).await;
        let conn = store.conn();
        let key_store = SqlKeyStoreRef::new(conn);

        // long_async_call
    }
}
