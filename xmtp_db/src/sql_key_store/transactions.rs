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

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, crate::ConnectionError> {
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

impl<C> SqlKeyStore<C>
where
    C: ConnectionExt,
{
    fn inner_transaction<T, F, E, C2>(&self, fun: F) -> Result<T, E>
    where
        for<'a> F: FnOnce(SqlKeyStore<MutableTransactionConnection<'a, C2>>) -> Result<T, E>,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
        for<'a> C2: diesel::Connection<Backend = Sqlite>
            + diesel::connection::SimpleConnection
            + LoadConnection
            + MigrationConnection
            + MigrationHarness<<C2 as diesel::Connection>::Backend>
            + Send
            + 'a,
        C: ConnectionExt<Connection = C2>,
    {
        let _guard = self.conn.start_transaction()?;
        let conn = &self.conn;

        // one call to raw_query_write = mutex only locked once for entire transaciton
        let r = conn.raw_query_write(|c| {
            Ok(c.transaction(|sqlite_c| {
                let s = SqlKeyStore {
                    conn: MutableTransactionConnection {
                        conn: RefCell::new(sqlite_c),
                    },
                };
                fun(s)
            }))
        })?;
        Ok(r?)
    }
}

impl<'a, C> XmtpMlsTransactionProvider<'a> for SqlKeyStore<MutableTransactionConnection<'a, C>>
where
    C: diesel::Connection<Backend = Sqlite>
        + diesel::connection::SimpleConnection
        + LoadConnection
        + MigrationConnection
        + MigrationHarness<<C as diesel::Connection>::Backend>
        + Send,
{
    type Storage = SqlKeyStore<MutableTransactionConnection<'a, C>>;

    fn storage(&self) -> &Self::Storage {
        &self
    }
}

pub trait XmtpMlsTransactionProvider<'a> {
    type Storage: XmtpMlsStorageProvider;

    fn storage(&self) -> &Self::Storage;
}

impl<C: ConnectionExt> XmtpMlsStorageProvider for SqlKeyStore<C> {
    type Connection = C;

    type DbQuery<'a>
        = DbConnection<&'a C>
    where
        Self::Connection: 'a;

    type Transaction<'a, C2>
        = SqlKeyStore<MutableTransactionConnection<'a, C2>>
    where
        C2: diesel::Connection<Backend = Sqlite>
            + diesel::connection::SimpleConnection
            + LoadConnection
            + MigrationConnection
            + MigrationHarness<<C2 as diesel::Connection>::Backend>
            + Send
            + 'a;

    fn conn(&self) -> &Self::Connection {
        &self.conn
    }

    fn db<'a>(&'a self) -> Self::DbQuery<'a>
    where
        C: 'a,
    {
        DbConnection::new(&self.conn)
    }

    fn transaction<T, E, F, C2>(&self, f: F) -> Result<T, E>
    where
        for<'a> F: FnOnce(Self::Transaction<'a, C2>) -> Result<T, E>,
        for<'a> C2: diesel::Connection<Backend = Sqlite>
            + diesel::connection::SimpleConnection
            + LoadConnection
            + MigrationConnection
            + MigrationHarness<<C2 as diesel::Connection>::Backend>
            + Send
            + 'a,
        E: From<diesel::result::Error> + From<crate::ConnectionError> + std::error::Error,
        C: ConnectionExt<Connection = C2>,
    {
        self.inner_transaction(f)
    }
}
