use crate::{sql_key_store::SqlKeyStore, xmtp_openmls_provider::XmtpOpenMlsProvider};
use std::fmt;

use super::{ConnectionExt, TransactionGuard};

/// A wrapper for RawDbConnection that houses all XMTP DB operations.
pub struct DbConnection<C> {
    pub(super) conn: C,
}

impl<C> DbConnection<C> {
    pub fn new(conn: C) -> Self {
        Self { conn }
    }
}

impl<C: ConnectionExt> crate::IntoConnection for DbConnection<C> {
    type Connection = C;

    fn into_connection(self) -> Self::Connection {
        self.conn
    }
}

impl<C> DbConnection<C>
where
    C: ConnectionExt,
{
    /*
    pub fn start_transaction(&self) -> Result<TransactionGuard<'_>, crate::ConnectionError> {
        <Self as ConnectionExt>::start_transaction(self)
    }
*/
    pub fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut C::Connection) -> Result<T, diesel::result::Error>,
    {
        <Self as ConnectionExt>::raw_query_read::<_, _>(self, fun)
    }

    pub fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut C::Connection) -> Result<T, diesel::result::Error>,
    {
        <Self as ConnectionExt>::raw_query_write::<_, _>(self, fun)
    }
}

impl<C> ConnectionExt for DbConnection<C>
where
    C: ConnectionExt,
{
    type Connection = C::Connection;
/*
    fn start_transaction(&self) -> Result<TransactionGuard<'_>, crate::ConnectionError> {
        self.conn.start_transaction()
    }
*/
    fn raw_query_read<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        self.conn.raw_query_read(fun)
    }

    fn raw_query_write<T, F>(&self, fun: F) -> Result<T, crate::ConnectionError>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        Self: Sized,
    {
        self.conn.raw_query_write(fun)
    }

    fn is_in_transaction(&self) -> bool {
        self.conn.is_in_transaction()
    }

    fn disconnect(&self) -> Result<(), crate::ConnectionError> {
        self.conn.disconnect()
    }

    fn reconnect(&self) -> Result<(), crate::ConnectionError> {
        self.conn.reconnect()
    }
}

// Forces a move for conn
// This is an important distinction from deriving `Clone` on `DbConnection`.
// This way, conn will be moved into XmtpOpenMlsProvider. This forces codepaths to
// use a connection from the provider, rather than pulling a new one from the pool, resulting
// in two connections in the same scope.
impl<C> From<DbConnection<C>> for XmtpOpenMlsProvider<SqlKeyStore<C>> {
    fn from(db: DbConnection<C>) -> XmtpOpenMlsProvider<SqlKeyStore<C>> {
        XmtpOpenMlsProvider::new(SqlKeyStore::new(db.conn))
    }
}

impl<C> fmt::Debug for DbConnection<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}
