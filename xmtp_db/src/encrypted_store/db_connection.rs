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
pub struct DbConnection<C = crate::DefaultConnection, E = StorageError> {
    conn: C,
    _marker: PhantomData<E>,
}

impl<C, E> DbConnection<C, E> {
    pub(crate) fn new(conn: C) -> DbConnection<C, E> {
        Self {
            conn,
            _marker: PhantomData,
        }
    }
}

impl<C: Clone, E> DbConnection<C, E> {
    /// Transmute the error type from one to another
    pub fn transmute<E2>(&self) -> DbConnection<C, E2> {
        DbConnection {
            conn: self.conn.clone(),
            _marker: PhantomData,
        }
    }
}

impl<C, E> DbConnection<C, E>
where
    C: ConnectionExt,
    E: From<ConnectionError>,
{
    pub fn start_transaction(&self) -> Result<TransactionGuard<'_>, StorageError> {
        <Self as ConnectionExt>::start_transaction(self)
    }

    pub fn raw_query_read<T, F>(&self, fun: F) -> Result<T, E>
    where
        F: FnOnce(&mut C::Connection) -> Result<T, diesel::result::Error>,
    {
        <Self as ConnectionExt>::raw_query_read::<_, _, E>(self, fun)
    }

    pub fn raw_query_write<T, E2, F>(&self, fun: F) -> Result<T, E2>
    where
        F: FnOnce(&mut C::Connection) -> Result<T, diesel::result::Error>,
        E2: From<ConnectionError>,
    {
        <Self as ConnectionExt>::raw_query_write::<_, _, E2>(self, fun)
    }
}

impl<C, E> ConnectionExt for DbConnection<C, E>
where
    C: ConnectionExt,
    E: From<ConnectionError>,
{
    type Connection = C::Connection;

    fn start_transaction(&self) -> Result<TransactionGuard<'_>, StorageError> {
        self.conn.start_transaction()
    }

    fn raw_query_read<T, F, E2>(&self, fun: F) -> Result<T, E2>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E2: From<super::ConnectionError>,
        Self: Sized,
    {
        self.conn.raw_query_read(fun)
    }

    fn raw_query_write<T, F, E2>(&self, fun: F) -> Result<T, E2>
    where
        F: FnOnce(&mut Self::Connection) -> Result<T, diesel::result::Error>,
        E2: From<super::ConnectionError>,
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
