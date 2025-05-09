use crate::{StorageError, xmtp_openmls_provider::XmtpOpenMlsProvider};
use std::{fmt, marker::PhantomData};

use super::{ConnectionError, ConnectionExt, TransactionGuard};

/// A wrapper for RawDbConnection that houses all XMTP DB operations.
/// Uses a [`Mutex]` internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
// ~~~~ _NOTE_ ~~~~~
// Do not derive clone here.
// callers should be able to accomplish everything with one conn/reference.
#[doc(hidden)]
pub struct DbConnection<C = crate::DefaultConnection, E = ConnectionError> {
    conn: C,
    _phantom: PhantomData<E>,
}

impl<C> DbConnection<C> {
    pub(crate) fn new(conn: C) -> Self {
        Self {
            conn,
            _phantom: PhantomData,
        }
    }
}

impl<C, E> DbConnection<C, E>
where
    C: ConnectionExt,
{
    pub fn start_transaction(&self) -> Result<TransactionGuard<'_>, StorageError> {
        <Self as ConnectionExt>::start_transaction(self)
    }

    pub fn raw_query_read<T, F>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut C::Connection) -> Result<T, diesel::result::Error>,
        E: From<ConnectionError>,
    {
        <Self as ConnectionExt>::raw_query_read::<_, _, E>(self, fun)
    }

    pub fn raw_query_write<T, F>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut C::Connection) -> Result<T, diesel::result::Error>,
        E: From<ConnectionError>,
    {
        <Self as ConnectionExt>::raw_query_write::<_, _, E>(self, fun)
    }
}

impl<C, Err> ConnectionExt for DbConnection<C, Err>
where
    C: ConnectionExt,
{
    type Connection = C::Connection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, StorageError> {
        self.conn.start_transaction()
    }

    fn raw_query_read<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<super::ConnectionError>,
        Self: Sized,
    {
        Ok(self.conn.raw_query_read(fun)?)
    }

    fn raw_query_write<T, F, E>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E: From<super::ConnectionError>,
        Self: Sized,
    {
        self.conn.raw_query_write(fun)
    }
}

// Forces a move for conn
// This is an important distinction from deriving `Clone` on `DbConnection`.
// This way, conn will be moved into XmtpOpenMlsProvider. This forces codepaths to
// use a connection from the provider, rather than pulling a new one from the pool, resulting
// in two connections in the same scope.
impl From<DbConnection> for XmtpOpenMlsProvider {
    fn from(db: DbConnection) -> XmtpOpenMlsProvider {
        XmtpOpenMlsProvider::new(db.conn)
    }
}

impl<C> fmt::Debug for DbConnection<C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}
