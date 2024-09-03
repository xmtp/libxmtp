use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;

use crate::storage::RawDbConnection;
use crate::xmtp_openmls_provider::XmtpOpenMlsProvider;

/// A wrapper for RawDbConnection that houses all XMTP DB operations.
/// Uses a [`Mutex]` internally for interior mutability, so that the connection
/// and transaction state can be shared between the OpenMLS Provider and
/// native XMTP operations
// ~~~~ _NOTE_ ~~~~~
// Do not derive clone here.
// callers should be able to accomplish everything with one conn/reference.
pub struct DbConnection {
    wrapped_conn: Arc<Mutex<RawDbConnection>>,
}

/// Owned DBConnection Methods
/// Lifetime is 'static' because we are using [`RefOrValue::Value`] variant.
impl DbConnection {
    pub(super) fn new(conn: RawDbConnection) -> Self {
        Self {
            wrapped_conn: Arc::new(Mutex::new(conn)),
        }
    }

    pub(super) fn from_arc_mutex(conn: Arc<Mutex<RawDbConnection>>) -> Self {
        Self { wrapped_conn: conn }
    }

    // Note: F is a synchronous fn. If it ever becomes async, we need to use
    // tokio::sync::mutex instead of std::sync::Mutex
    pub(crate) fn raw_query<T, F>(&self, fun: F) -> Result<T, diesel::result::Error>
    where
        F: FnOnce(&mut RawDbConnection) -> Result<T, diesel::result::Error>,
    {
        let mut lock = self.wrapped_conn.lock();
        fun(&mut lock)
    }
}

// Forces a move for conn
// This is an important distinction from deriving `Clone` on `DbConnection`.
// This way, conn will be moved into XmtpOpenMlsProvider. This forces codepaths to
// use a connection from the provider, rather than pulling a new one from the pool, resulting
// in two connections in the same scope.
impl From<DbConnection> for XmtpOpenMlsProvider {
    fn from(conn: DbConnection) -> XmtpOpenMlsProvider {
        XmtpOpenMlsProvider::new(conn)
    }
}

impl fmt::Debug for DbConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DbConnection")
            .field("wrapped_conn", &"DbConnection")
            .finish()
    }
}
